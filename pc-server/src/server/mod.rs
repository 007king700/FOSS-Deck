// src/server/mod.rs
#![cfg(windows)]

pub mod auth_store;
pub mod commands;
pub mod pairing;
pub mod rate_limit;
pub mod ws;

pub use pairing::{generate_pairing_code, PairingState};
pub use ws::run_ws_server;
