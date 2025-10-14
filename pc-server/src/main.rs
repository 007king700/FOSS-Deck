#![cfg(windows)] // this binary is Windows-only

mod gui;
mod server;
mod audio;
mod discovery;

fn main() {
    env_logger::init();
    if let Err(e) = gui::run() {
        eprintln!("GUI failed to start: {e}");
    }
}
