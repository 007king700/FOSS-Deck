// src/server.rs
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use log::{error, info};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::{select, sync::oneshot};
use tokio_util::sync::CancellationToken;
use warp::http::StatusCode;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::audio;

const PAIRING_TTL: Duration = Duration::from_secs(300);
const PAIRING_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------
// Rate limiting (in-memory)
// ---------------------------
// Applies to:
//  - invalid pairing code attempts
//  - invalid auth token attempts
//
// Policy:
//  - max N attempts within WINDOW
//  - if exceeded -> lockout for LOCKOUT duration

const RL_MAX_ATTEMPTS: u32 = 5;
const RL_WINDOW: Duration = Duration::from_secs(30);
const RL_LOCKOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
struct RateLimitEntry {
    window_start: Instant,
    attempts: u32,
    lockout_until: Option<Instant>,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            attempts: 0,
            lockout_until: None,
        }
    }

    fn is_locked(&self) -> bool {
        self.lockout_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    fn remaining_lockout_secs(&self) -> u64 {
        if let Some(t) = self.lockout_until {
            if Instant::now() >= t {
                0
            } else {
                (t - Instant::now()).as_secs()
            }
        } else {
            0
        }
    }

    fn register_failure(&mut self) {
        // reset window if expired
        if self.window_start.elapsed() > RL_WINDOW {
            self.window_start = Instant::now();
            self.attempts = 0;
            self.lockout_until = None;
        }

        self.attempts += 1;
        if self.attempts >= RL_MAX_ATTEMPTS {
            self.lockout_until = Some(Instant::now() + RL_LOCKOUT);
        }
    }

    fn register_success(&mut self) {
        // On success, reset failures for that IP
        self.window_start = Instant::now();
        self.attempts = 0;
        self.lockout_until = None;
    }
}

// ---------------------------
// Persistent authorization store
// ---------------------------

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct AuthorizedStore {
    devices: HashMap<String, AuthorizedDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthorizedDevice {
    pub name: Option<String>,
    pub token_hash: String,
    pub added_at: i64,
    pub last_seen: i64,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

fn auth_store_path() -> PathBuf {
    if let Some(proj_dirs) = directories_next::ProjectDirs::from("com", "fossdeck", "FOSS-Deck") {
        let dir = proj_dirs.data_dir();
        let _ = fs::create_dir_all(dir);
        dir.join("authorized_devices.json")
    } else {
        PathBuf::from("authorized_devices.json")
    }
}

fn load_store(path: &Path) -> AuthorizedStore {
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AuthorizedStore::default(),
    }
}

fn save_store(path: &Path, store: &AuthorizedStore) -> anyhow::Result<()> {
    let s = serde_json::to_string_pretty(store)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, s)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

// ---------------------------
// Pairing runtime state
// ---------------------------

#[derive(Debug, Clone)]
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

    pub fn list_authorized(&self) -> Vec<(String, AuthorizedDevice)> {
        let mut v: Vec<_> = self
            .store
            .devices
            .iter()
            .map(|(id, dev)| (id.clone(), dev.clone()))
            .collect();
        // sort by last_seen desc
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
        let token_hash = sha256_hex(token);
        self.store
            .devices
            .get(device_id)
            .map(|d| d.token_hash == token_hash)
            .unwrap_or(false)
    }

    pub fn upsert_authorized(&mut self, device_id: &str, device_name: Option<String>, token: &str) {
        let token_hash = sha256_hex(token);
        let ts = now_unix();

        self.store.devices.insert(
            device_id.to_string(),
            AuthorizedDevice {
                name: device_name,
                token_hash,
                added_at: ts,
                last_seen: ts,
            },
        );

        let _ = save_store(&self.store_path, &self.store);
    }

    // ----- rate limiting helpers -----

    pub fn rl_is_locked(&mut self, ip: IpAddr) -> Option<u64> {
        let ent = self.rate_limit.entry(ip).or_insert_with(RateLimitEntry::new);
        if ent.is_locked() {
            Some(ent.remaining_lockout_secs())
        } else {
            None
        }
    }

    pub fn rl_register_success(&mut self, ip: IpAddr) {
        let ent = self.rate_limit.entry(ip).or_insert_with(RateLimitEntry::new);
        ent.register_success();
    }

    pub fn rl_register_failure(&mut self, ip: IpAddr) {
        let ent = self.rate_limit.entry(ip).or_insert_with(RateLimitEntry::new);
        ent.register_failure();
    }
}

pub fn generate_pairing_code() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let mut v = nanos as u64 ^ 0xa5a5_5a5a_1234_5678;
    let mut digits = [0u8; 6];
    for d in &mut digits {
        *d = (v % 10) as u8;
        v /= 10;
    }
    digits
        .into_iter()
        .rev()
        .map(|d| char::from(b'0' + d))
        .collect()
}

// ---------------------------
// WebSocket protocol
// ---------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum WsCommand {
    GetStatus,
    SetVolume { level: f32 },
    VolumeUp { delta: Option<f32> },
    VolumeDown { delta: Option<f32> },
    ToggleMute,
    Mute,
    Unmute,

    Pair {
        code: String,
        device_id: String,
        device_name: Option<String>,
    },
    Auth {
        device_id: String,
        token: String,
    },
}

// ---------------------------
// Server
// ---------------------------

pub async fn run_ws_server(
    port: u16,
    shutdown_rx: oneshot::Receiver<()>,
    pairing_state: Arc<Mutex<PairingState>>,
) -> Result<()> {
    let cancel = CancellationToken::new();
    let cancel_filter = warp::any().map({
        let cancel = cancel.clone();
        move || cancel.clone()
    });

    let pairing_filter = {
        let shared = pairing_state.clone();
        warp::any().map(move || shared.clone())
    };

    let health = warp::path!("health")
        .map(|| warp::reply::with_status("ok", StatusCode::OK))
        .boxed();

    let ws_route = warp::path!("ws")
        .and(warp::ws())
        .and(warp::addr::remote())
        .and(cancel_filter)
        .and(pairing_filter)
        .map(
            |ws: warp::ws::Ws,
             remote: Option<SocketAddr>,
             cancel: CancellationToken,
             pairing: Arc<Mutex<PairingState>>| {
                ws.on_upgrade(move |socket| async move {
                    handle_ws(socket, cancel, remote, pairing).await;
                })
            },
        )
        .boxed();

    let routes = health
        .or(ws_route)
        .with(warp::cors().allow_any_origin())
        .with(warp::log("fossdeck_ws"))
        .boxed();

    let addr = ([0, 0, 0, 0], port);
    info!("Listening on ws://0.0.0.0:{port}/ws");

    let cancel_for_shutdown = cancel.clone();

    // Watchdog: clear active session if no heartbeat for IDLE_TIMEOUT
    let pairing_for_watchdog = pairing_state.clone();
    let cancel_for_watchdog = cancel.clone();
    tokio::spawn(async move {
        use tokio::time::{sleep, Duration as TokioDuration};
        loop {
            if cancel_for_watchdog.is_cancelled() {
                break;
            }
            {
                let mut st = pairing_for_watchdog.lock().unwrap();
                if st.is_idle_too_long() {
                    st.clear_active();
                }
            }
            sleep(TokioDuration::from_secs(5)).await;
        }
    });

    warp::serve(routes)
        .bind_with_graceful_shutdown(addr, async move {
            let _ = shutdown_rx.await;
            info!("Graceful shutdown signal received — closing all clients...");
            cancel_for_shutdown.cancel();
        })
        .1
        .await;

    Ok(())
}

async fn handle_ws(
    ws: WebSocket,
    cancel: CancellationToken,
    remote: Option<SocketAddr>,
    pairing: Arc<Mutex<PairingState>>,
) {
    let (mut tx, mut rx) = ws.split();
    let remote_ip = remote.map(|a| a.ip());

    let mut authenticated = false;
    let mut authed_device_id: Option<String> = None;

    // hello
    let (is_active_paired, active_id, authorized_count, code, code_expired) = {
        let st = pairing.lock().unwrap();
        (
            st.active_device_id.is_some(),
            st.active_device_id.clone(),
            st.authorized_count(),
            st.code.clone(),
            st.is_expired(),
        )
    };

    if let Err(e) = tx
        .send(Message::text(
            json!({
                "type": "hello",
                "server": "foss-deck",
                "version": env!("CARGO_PKG_VERSION"),
                "pairing_required": true,
                "paired_active": is_active_paired,
                "active_device_id": active_id,
                "authorized_devices": authorized_count,
                "code_expired": code_expired,
                "pairing_code": code,
            })
                .to_string(),
        ))
        .await
    {
        error!("Failed to send hello: {e}");
        return;
    }

    loop {
        select! {
            _ = cancel.cancelled() => {
                let _ = tx.send(Message::text(json!({"type":"shutdown","message":"server stopped"}).to_string())).await;
                let _ = tx.close().await;
                break;
            }
            maybe_msg = rx.next() => {
                match maybe_msg {
                    Some(Ok(msg)) if msg.is_text() => {
                        let text = msg.to_str().unwrap_or_default();

                        let reply = match serde_json::from_str::<WsCommand>(text) {
                            Ok(WsCommand::Auth { device_id, token }) => {
                                let ip = if let Some(ip) = remote_ip {
                                    ip
                                } else {
                                    return;
                                };

                                let mut st = pairing.lock().unwrap();

                                if let Some(rem) = st.rl_is_locked(ip) {
                                    json!({"type":"rate_limited","reason":"auth","retry_after_secs": rem})
                                } else if st.is_authorized(&device_id, &token) {
                                    st.rl_register_success(ip);
                                    authenticated = true;
                                    authed_device_id = Some(device_id.clone());

                                    st.active_device_id = Some(device_id);
                                    st.active_client_ip = Some(ip);
                                    st.mark_seen();

                                    json!({"type":"auth_ok"})
                                } else {
                                    st.rl_register_failure(ip);
                                    authenticated = false;
                                    authed_device_id = None;
                                    json!({"type":"auth_error","reason":"invalid_token"})
                                }
                            }



                            Ok(WsCommand::Pair { code, device_id, device_name }) => {
                                if remote_ip.is_none() {
                                    json!({"type":"pairing_error","reason":"no_remote_ip"})
                                } else {
                                    let ip = remote_ip.unwrap();
                                    let mut st = pairing.lock().unwrap();

                                    if let Some(rem) = st.rl_is_locked(ip) {
                                        json!({"type":"rate_limited","reason":"pair","retry_after_secs": rem})
                                    } else {
                                        if st.is_expired() && st.active_device_id.is_none() {
                                            st.code = generate_pairing_code();
                                            st.created_at = Instant::now();
                                        }

                                        if st.is_expired() {
                                            json!({"type":"pairing_error","reason":"expired"})
                                        } else if code == st.code {
                                            st.rl_register_success(ip);

                                            let token = generate_token();
                                            st.upsert_authorized(&device_id, device_name, &token);

                                            authenticated = true;
                                            authed_device_id = Some(device_id.clone());

                                            st.active_device_id = Some(device_id);
                                            st.active_client_ip = Some(ip);
                                            st.mark_seen();

                                            json!({"type":"pairing_ok","token":token})
                                        } else {
                                            st.rl_register_failure(ip);
                                            json!({"type":"pairing_error","reason":"invalid"})
                                        }
                                    }
                                }
                            }


                            Ok(cmd) => {
                                if !authenticated {
                                    json!({"type":"error","message":"unauthorized"})
                                } else {
                                    {
                                        let mut st = pairing.lock().unwrap();
                                        if st.active_device_id.is_none() {
                                            st.active_device_id = authed_device_id.clone();
                                            st.active_client_ip = remote_ip;
                                        }
                                        st.mark_seen();
                                    }
                                    handle_command(cmd).unwrap_or_else(|e| json!({"type":"error","message":e.to_string()}))
                                }
                            }

                            Err(e) => json!({"type":"error","message":format!("invalid json: {e}")}),
                        };

                        if let Err(e) = tx.send(Message::text(reply.to_string())).await {
                            error!("Failed to send response: {e}");
                            break;
                        }
                    }
                    Some(Ok(_)) => { /* ignore binary */ }
                    Some(Err(e)) => {
                        error!("WebSocket error: {e}");
                        break;
                    }
                    None => {
                        // client disconnected; watchdog clears active session if no heartbeat
                        break;
                    }
                }
            }
        }
    }
}

fn handle_command(cmd: WsCommand) -> anyhow::Result<serde_json::Value> {
    match cmd {
        WsCommand::GetStatus => {
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"status","volume":vol,"muted":muted}))
        }
        WsCommand::SetVolume { level } => {
            let level = level.clamp(0.0, 1.0);
            audio::set_volume(level)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"set_volume","volume":vol,"muted":muted}))
        }
        WsCommand::VolumeUp { delta } => {
            let delta = delta.unwrap_or(0.05).clamp(0.0, 1.0);
            let (mut vol, _) = audio::get_volume_and_mute()?;
            vol = (vol + delta).clamp(0.0, 1.0);
            audio::set_volume(vol)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"volume_up","volume":vol,"muted":muted}))
        }
        WsCommand::VolumeDown { delta } => {
            let delta = delta.unwrap_or(0.05).clamp(0.0, 1.0);
            let (mut vol, _) = audio::get_volume_and_mute()?;
            vol = (vol - delta).clamp(0.0, 1.0);
            audio::set_volume(vol)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"volume_down","volume":vol,"muted":muted}))
        }
        WsCommand::ToggleMute => {
            let (_, muted) = audio::get_volume_and_mute()?;
            audio::set_mute(!muted)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"toggle_mute","volume":vol,"muted":muted}))
        }
        WsCommand::Mute => {
            audio::set_mute(true)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"mute","volume":vol,"muted":muted}))
        }
        WsCommand::Unmute => {
            audio::set_mute(false)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"unmute","volume":vol,"muted":muted}))
        }

        WsCommand::Pair { .. } | WsCommand::Auth { .. } => {
            Ok(json!({"type":"error","message":"pair/auth_not_allowed_here"}))
        }
    }
}
