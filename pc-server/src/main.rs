#![cfg(windows)] // this binary is Windows-only
#![windows_subsystem = "windows"]
mod gui;
mod server;
mod audio;
mod discovery;
mod media;
mod system;

fn main() {
    gui::run_gui();
}
