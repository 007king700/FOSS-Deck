// src/gui.rs
#![cfg(windows)]

use std::sync::{Arc, Mutex};

use env_logger;
use log::info;
use tokio::{runtime::Runtime, sync::oneshot};

use crate::discovery::run_discovery_server;
use crate::server::{generate_pairing_code, run_ws_server, PairingState};

const PORT: u16 = 3030;

pub fn run_gui() {
    env_logger::try_init().ok();
    let options = eframe::NativeOptions::default();

    if let Err(e) = eframe::run_native(
        "FOSS-Deck PC",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    ) {
        eprintln!("GUI failed to start: {e}");
    }
}

struct App {
    rt: Runtime,

    server_on: bool,
    discovery_on: bool,

    server_tx: Option<oneshot::Sender<()>>,
    discovery_tx: Option<oneshot::Sender<()>>,

    last_status: String,

    pairing: Arc<Mutex<PairingState>>,
}

impl App {
    fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let initial_code = generate_pairing_code();
        let pairing = Arc::new(Mutex::new(PairingState::new(initial_code)));

        Self {
            rt,
            server_on: false,
            discovery_on: false,
            server_tx: None,
            discovery_tx: None,
            last_status: "Idle".into(),
            pairing,
        }
    }

    fn start_server(&mut self) {
        if self.server_on {
            return;
        }

        let (tx, rx) = oneshot::channel::<()>();
        self.server_tx = Some(tx);

        let pairing = self.pairing.clone();
        self.rt.spawn(async move {
            let _ = run_ws_server(PORT, rx, pairing).await;
        });

        self.server_on = true;
        self.last_status = format!("Server running on ws://0.0.0.0:{}/ws", PORT);
        info!("{}", self.last_status);
    }

    fn stop_server(&mut self) {
        if let Some(tx) = self.server_tx.take() {
            let _ = tx.send(());
        }
        self.server_on = false;
        self.last_status = "Server stopped".into();
        self.stop_discovery();
        info!("{}", self.last_status);
    }

    fn start_discovery(&mut self) {
        if self.discovery_on {
            return;
        }

        let (tx, rx) = oneshot::channel::<()>();
        self.discovery_tx = Some(tx);

        self.rt.spawn(async move {
            let _ = run_discovery_server(PORT, rx).await;
        });

        self.discovery_on = true;
        info!("Discovery enabled");
    }

    fn stop_discovery(&mut self) {
        if let Some(tx) = self.discovery_tx.take() {
            let _ = tx.send(());
        }
        self.discovery_on = false;
        info!("Discovery disabled");
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        eframe::egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("FOSS-Deck PC");
            ui.label("Service Control Panel");
        });

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            // --- Service toggle ---
            let mut srv = self.server_on;
            if ui.checkbox(&mut srv, "Enable WebSocket Server").changed() {
                if srv {
                    self.start_server();
                } else {
                    self.stop_server();
                }
            }

            // --- Discovery toggle ---
            let mut disc = self.discovery_on;
            let resp = ui
                .add_enabled(
                    self.server_on,
                    eframe::egui::Checkbox::new(&mut disc, "Enable Discoverability"),
                )
                .on_disabled_hover_text("Start the server to change discoverability.");
            if resp.changed() {
                if disc {
                    self.start_discovery();
                } else {
                    self.stop_discovery();
                }
            }

            ui.separator();
            ui.label(format!("Status: {}", self.last_status));

            ui.separator();
            ui.heading("Pairing / Authorization");

            // Snapshot for display + list
            let (code, active_id, active_ip, authorized_list_len, authorized_list) = {
                let st = self.pairing.lock().unwrap();
                (
                    st.code.clone(),
                    st.active_device_id.clone(),
                    st.active_client_ip.map(|ip| ip.to_string()),
                    st.authorized_count(),
                    st.list_authorized(),
                )
            };

            ui.label(format!("Pairing code: {}", code));
            ui.label(format!("Authorized devices stored: {}", authorized_list_len));

            if let Some(id) = &active_id {
                ui.label(format!("Active paired device: {}", id));
            } else {
                ui.label("Active paired device: (none)");
            }
            if let Some(ip) = &active_ip {
                ui.label(format!("Active client IP: {}", ip));
            }

            ui.separator();
            ui.heading("Authorized devices");

            if authorized_list.is_empty() {
                ui.label("(no authorized devices yet)");
                return;
            }

            eframe::egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for (device_id, dev) in authorized_list {
                        ui.group(|ui| {
                            let name = dev.name.clone().unwrap_or_else(|| "Unnamed device".into());
                            ui.label(format!("Name: {}", name));
                            ui.label(format!("Device ID: {}", device_id));
                            ui.label(format!("Last seen (unix): {}", dev.last_seen));

                            let is_active = active_id.as_deref() == Some(device_id.as_str());
                            if is_active {
                                ui.label("Status: ACTIVE");
                            }

                            ui.horizontal(|ui| {
                                let revoke = ui.button("Revoke");
                                if revoke.clicked() {
                                    let mut st = self.pairing.lock().unwrap();
                                    st.revoke_device(&device_id);
                                }
                            });
                        });
                        ui.add_space(6.0);
                    }
                });
        });
    }
}
