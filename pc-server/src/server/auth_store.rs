// src/server/auth_store.rs
#![cfg(windows)]

use anyhow::Result;
use directories_next::ProjectDirs;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct AuthorizedStore {
    pub(crate) devices: HashMap<String, AuthorizedDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthorizedDevice {
    pub name: Option<String>,
    pub token_hash: String,
    pub added_at: i64,
    pub last_seen: i64,
}

pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(crate) fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}

pub(crate) fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub(crate) fn auth_store_path() -> PathBuf {
    // %APPDATA%/FOSS-Deck/authorized.json (or similar)
    if let Some(proj_dirs) = ProjectDirs::from("org", "FOSS-Deck", "FOSS-Deck") {
        let dir = proj_dirs.data_dir();
        let _ = fs::create_dir_all(dir);
        dir.join("authorized.json")
    } else {
        PathBuf::from("authorized.json")
    }
}

pub(crate) fn load_store(path: &Path) -> AuthorizedStore {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AuthorizedStore::default(),
    }
}

pub(crate) fn save_store(path: &Path, store: &AuthorizedStore) -> Result<()> {
    let s = serde_json::to_string_pretty(store)?;
    fs::write(path, s)?;
    Ok(())
}
