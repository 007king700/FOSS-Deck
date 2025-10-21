#![cfg(windows)] // this binary is Windows-only

mod gui;
mod server;
mod audio;
mod discovery;

fn main() {
    gui::run_gui();
}
