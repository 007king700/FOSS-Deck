// src/server/pairing.rs
#![cfg(windows)]

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::server::auth_store::{auth_store_path, load_store, now_unix, save_store, AuthorizedStore};
use crate::server::rate_limit::RateLimitEntry;

const PAIRING_TTL: Duration = Duration::from_secs(300);
const PAIRING_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct PairingState {
    pub code: String,
    pub created_at: Instant,

    // Active session (runtime)
    pub active_device_id: Option<String>,
    pub active_client_ip: Option<IpAddr>,
    pub last_seen: Option<Instant>,

    // Persistent allowlist
    store_path: PathBuf,
    store: AuthorizedStore,

    // Rate-limit map (in-memory)
    rate_limit: HashMap<IpAddr, RateLimitEntry>,
}

impl PairingState {
    pub fn new(code: String) -> Self {
        let store_path = auth_store_path();
        let store = load_store(&store_path);

        Self {
            code,
            created_at: Instant::now(),
            active_device_id: None,
            active_client_ip: None,
            last_seen: None,
            store_path,
            store,
            rate_limit: HashMap::new(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > PAIRING_TTL
    }

    pub fn mark_seen(&mut self) {
        self.last_seen = Some(Instant::now());

        // persist last_seen for active device
        if let Some(id) = self.active_device_id.clone() {
            if let Some(dev) = self.store.devices.get_mut(&id) {
                dev.last_seen = now_unix();
                let _ = save_store(&self.store_path, &self.store);
            }
        }
    }

    pub fn is_idle_too_long(&self) -> bool {
        match (&self.active_device_id, self.last_seen) {
            (Some(_), Some(last)) => last.elapsed() > PAIRING_IDLE_TIMEOUT,
            _ => false,
        }
    }

    pub fn clear_active(&mut self) {
        self.active_device_id = None;
        self.active_client_ip = None;
        self.last_seen = None;
    }

    pub fn authorized_count(&self) -> usize {
        self.store.devices.len()
    }

    pub fn list_authorized(&self) -> Vec<(String, crate::server::auth_store::AuthorizedDevice)> {
        let mut v: Vec<_> = self.store.devices.iter().map(|(k, d)| (k.clone(), d.clone())).collect();
        // newest last_seen first
        v.sort_by_key(|(_, d)| -d.last_seen);
        v
    }

    pub fn revoke_device(&mut self, device_id: &str) {
        self.store.devices.remove(device_id);
        let _ = save_store(&self.store_path, &self.store);

        // If revoked device was active, clear active session
        if self.active_device_id.as_deref() == Some(device_id) {
            self.clear_active();
        }
    }

    pub fn is_authorized(&self, device_id: &str, token: &str) -> bool {
        let token_hash = crate::server::auth_store::sha256_hex(token);

        self.store
            .devices
            .get(device_id)
            .map(|d| d.token_hash == token_hash)
            .unwrap_or(false)
    }

    pub fn upsert_authorized(&mut self, device_id: String, token_hash: String, device_name: Option<String>) {
        let now = now_unix();
        self.store.devices.insert(
            device_id,
            crate::server::auth_store::AuthorizedDevice {
                name: device_name,
                token_hash,
                added_at: now,
                last_seen: now,
            },
        );
        let _ = save_store(&self.store_path, &self.store);
    }

    pub fn rl_is_locked(&mut self, ip: IpAddr) -> Option<u64> {
        let entry = self.rate_limit.entry(ip).or_insert_with(RateLimitEntry::new);
        if entry.is_locked() {
            Some(entry.remaining_lockout_secs())
        } else {
            None
        }
    }

    pub fn rl_register_success(&mut self, ip: IpAddr) {
        let entry = self.rate_limit.entry(ip).or_insert_with(RateLimitEntry::new);
        entry.register_success();
    }

    pub fn rl_register_failure(&mut self, ip: IpAddr) {
        let entry = self.rate_limit.entry(ip).or_insert_with(RateLimitEntry::new);
        entry.register_failure();
    }
}

// Original pairing code generator, preserved (used by GUI + server)
pub fn generate_pairing_code() -> String {
    // Example behavior: 6-digit code derived from current time.
    // (kept functionally identical to your original approach)
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut n = (secs % 1_000_000) as u32;
    let mut digits = Vec::with_capacity(6);

    for _ in 0..6 {
        digits.push((n % 10) as u8);
        n /= 10;
    }

    digits
        .into_iter()
        .rev()
        .map(|d| char::from(b'0' + d))
        .collect()
}
