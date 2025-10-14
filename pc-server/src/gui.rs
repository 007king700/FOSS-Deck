use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use log::info;
use tokio::{runtime::Runtime, sync::oneshot};

use crate::audio;
use crate::server::run_ws_server;
use crate::discovery::run_discovery_server;

pub fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    eframe::run_native("FOSS-Deck PC", options, Box::new(|_cc| Box::new(App::new())))
}

// ----------------------------- GUI APP -----------------------------
struct App {
    // server
    rt: Runtime,
    port: u16,
    server_running: bool,
    shutdown_tx: Vec<oneshot::Sender<()>>,
    last_status: String,

    // audio
    volume: f32,
    muted: bool,
    last_refresh: Instant,
}

impl App {
    fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (vol, muted) = audio::get_volume_and_mute().unwrap_or((0.5, false));
        Self {
            rt,
            port: 3030,
            server_running: false,
            shutdown_tx: Vec::new(),
            last_status: String::from("stopped"),
            volume: vol,
            muted,
            last_refresh: Instant::now(),
        }
    }

    fn start_server(&mut self) {
        if self.server_running {
            return;
        }
        let port = self.port;
        let (tx1, rx1) = oneshot::channel::<()>();
        let (tx2, rx2) = oneshot::channel::<()>();
        self.shutdown_tx = vec![tx1, tx2];

        self.rt.spawn(async move { let _ = run_ws_server(port, rx1).await; });
        self.rt.spawn(async move { let _ = run_discovery_server(port, rx2).await; });

        self.server_running = true;
        self.last_status = format!("listening on ws://0.0.0.0:{}/ws", port);
        info!("{}", self.last_status);
    }

    fn stop_server(&mut self) {
        for tx in self.shutdown_tx.drain(..) {
            let _ = tx.send(());
        }
        self.server_running = false;
        self.last_status = "stopped".into();
        info!("server stopped");
    }

    fn refresh_audio(&mut self) {
        if let Ok((v, m)) = audio::get_volume_and_mute() {
            self.volume = v;
            self.muted = m;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // auto refresh audio status every 1s
        if self.last_refresh.elapsed() > Duration::from_secs(1) {
            self.refresh_audio();
            self.last_refresh = Instant::now();
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("FOSS-Deck PC");
            ui.label("Minimal control panel");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Port:");
                let mut port_str = self.port.to_string();
                if ui.text_edit_singleline(&mut port_str).lost_focus() {
                    if let Ok(p) = port_str.parse::<u16>() {
                        self.port = p;
                    }
                }

                if !self.server_running {
                    if ui.button("Start server").clicked() {
                        self.start_server();
                    }
                } else if ui.button("Stop server").clicked() {
                    self.stop_server();
                }

                if ui.button("Copy WS URL").clicked() {
                    let url = format!("ws://{}:{}/ws", Ipv4Addr::UNSPECIFIED, self.port);
                    ui.output_mut(|o| o.copied_text = url);
                }
            });

            ui.separator();
            ui.label(format!("Server status: {}", self.last_status));

            ui.separator();
            ui.heading("Audio");

            let mut vol = self.volume;
            if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).text("Master volume")).changed() {
                self.volume = vol.clamp(0.0, 1.0);
                let _ = audio::set_volume(self.volume);
            }

            let mut m = self.muted;
            if ui.checkbox(&mut m, "Muted").changed() {
                self.muted = m;
                let _ = audio::set_mute(self.muted);
            }

            if ui.button("Refresh").clicked() {
                self.refresh_audio();
            }

            ui.add_space(8.0);
            ui.small("Tip: Android client can connect to ws://<your-pc-ip>:<port>/ws");
        });
    }
}
