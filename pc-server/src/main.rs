#![cfg(windows)] // this binary is Windows-only

mod gui;
mod server;
mod audio;
mod discovery;
mod media;
mod system;

fn main() {
    gui::run_gui();
}
