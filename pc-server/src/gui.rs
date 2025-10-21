// src/gui.rs
#![cfg(windows)]

use env_logger;
use log::info;
use tokio::{runtime::Runtime, sync::oneshot};

use crate::discovery::run_discovery_server;
use crate::server::run_ws_server;

const PORT: u16 = 3030; // hardcoded port

pub fn run_gui() {
    env_logger::init();
    let options = eframe::NativeOptions::default();

    if let Err(e) = eframe::run_native(
        "FOSS-Deck PC",
        options,
        Box::new(|_cc| Box::new(App::new())),
    ) {
        eprintln!("GUI failed to start: {e}");
    }
}

struct App {
    rt: Runtime,

    // toggles
    server_on: bool,
    discovery_on: bool,

    // channels
    server_tx: Option<oneshot::Sender<()>>,
    discovery_tx: Option<oneshot::Sender<()>>,

    last_status: String,
}

impl App {
    fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self {
            rt,
            server_on: false,
            discovery_on: false,
            server_tx: None,
            discovery_tx: None,
            last_status: "Idle".into(),
        }
    }

    fn start_server(&mut self) {
        if self.server_on {
            return;
        }

        let (tx, rx) = oneshot::channel::<()>();
        self.server_tx = Some(tx);

        self.rt.spawn(async move {
            let _ = run_ws_server(PORT, rx).await;
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("FOSS-Deck PC");
            ui.label("Service Control Panel");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // --- Service toggle ---
            let mut srv = self.server_on;
            if ui.checkbox(&mut srv, "Enable WebSocket Server").changed() {
                if srv {
                    self.start_server();
                } else {
                    self.stop_server();
                }
            }

            // --- Discovery toggle (independent from server) ---
            let mut disc = self.discovery_on;
            if ui.checkbox(&mut disc, "Enable Discoverability").changed() && self.server_on {
                if disc {
                    self.start_discovery();
                } else {
                    self.stop_discovery();
                }
            }

            ui.separator();
            ui.label(format!("Status: {}", self.last_status));
        });
    }
}
