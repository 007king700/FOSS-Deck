// src/server.rs
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use log::{error, info};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::{select, sync::oneshot};
use tokio_util::sync::CancellationToken;
use warp::ws::{Message, WebSocket};
use warp::{Filter, http::StatusCode};

use crate::audio;

const PAIRING_TTL: Duration = Duration::from_secs(300);
const PAIRING_IDLE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct PairingState {
    pub code: String,
    pub created_at: Instant,
    pub client: Option<SocketAddr>,
    pub last_seen: Option<Instant>,
}

impl PairingState {
    pub fn new(code: String) -> Self {
        Self {
            code,
            created_at: Instant::now(),
            client: None,
            last_seen: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > PAIRING_TTL
    }

    pub fn mark_seen(&mut self) {
        self.last_seen = Some(Instant::now());
    }

    pub fn is_idle_too_long(&self) -> bool {
        match (self.client, self.last_seen) {
            (Some(_), Some(last)) => last.elapsed() > PAIRING_IDLE_TIMEOUT,
            _ => false,
        }
    }
}

pub fn generate_pairing_code() -> String {
    // Simple 6-digit code using system time as a seed.
    // Not cryptographically strong but sufficient as a short-lived local pairing code.
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
    Pair { code: String },
}

pub async fn run_ws_server(port: u16, shutdown_rx: oneshot::Receiver<()>, pairing_state: Arc<Mutex<PairingState>>) -> Result<()> {
    let cancel = CancellationToken::new();
    let cancel_filter = warp::any().map({
        let cancel = cancel.clone();
        move || cancel.clone()
    });

    let pairing_filter = {
        let shared = pairing_state.clone();
        warp::any().map(move || shared.clone())
    };

    // Health endpoint
    let health = warp::path!("health")
        .map(|| warp::reply::with_status("ok", StatusCode::OK))
        .boxed();

    // WebSocket endpoint
    let ws_route = warp::path!("ws")
        .and(warp::ws())
        .and(warp::addr::remote())
        .and(cancel_filter)
        .and(pairing_filter)
        .map(|ws: warp::ws::Ws, remote: Option<SocketAddr>, cancel: CancellationToken, pairing: Arc<Mutex<PairingState>>| {
            ws.on_upgrade(move |socket| async move {
                handle_ws(socket, cancel, remote, pairing).await;
            })
        })
        .boxed();

    // Combine routes
    let routes = health
        .or(ws_route)
        .with(warp::cors().allow_any_origin())
        .with(warp::log("fossdeck_ws"))
        .boxed();

    let addr = ([0, 0, 0, 0], port);
    info!("Listening on ws://0.0.0.0:{port}/ws");

    let cancel_for_shutdown = cancel.clone();

    // Heartbeat watchdog: every few seconds clear stale pairing info.
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
                    st.client = None;
                    st.last_seen = None;
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

async fn handle_ws(ws: WebSocket, cancel: CancellationToken, remote: Option<SocketAddr>, pairing: Arc<Mutex<PairingState>>) {
    let (mut tx, mut rx) = ws.split();

    let is_paired = {
        let st = pairing.lock().unwrap();
        st.client.is_some()
    };

    // Send welcome message
    if let Err(e) = tx
        .send(Message::text(
            json!({
                "type": "hello",
                "server": "foss-deck",
                "version": env!("CARGO_PKG_VERSION"),
                "pairing_required": true,
                "paired": is_paired,
            })
                .to_string(),
        ))
        .await
    {
        error!("Failed to send hello: {e}");
        return;
    }

    let mut authenticated = false;

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
                            Ok(WsCommand::Pair { code }) => {
                                let mut st = pairing.lock().unwrap();
                                if st.is_expired() && st.client.is_none() {
                                    st.code = generate_pairing_code();
                                    st.created_at = Instant::now();
                                }

                                if st.is_expired() {
                                    json!({"type":"pairing_error","reason":"expired"})
                                } else if code == st.code {
                                    if let Some(addr) = remote {
                                        st.client = Some(addr);
                                    }
                                    st.mark_seen();
                                    authenticated = true;
                                    json!({"type":"pairing_ok"})
                                } else {
                                    json!({"type":"pairing_error","reason":"invalid"})
                                }
                            }
                            Ok(cmd) => {
                                if !authenticated {
                                    json!({"type":"error","message":"unauthorized"})
                                } else {
                                    // update last_seen on any successful authenticated command
                                    {
                                        let mut st = pairing.lock().unwrap();
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
                        // client disconnected; mark as idle so watchdog will clear pairing soon
                        let mut st = pairing.lock().unwrap();
                        st.last_seen = Some(Instant::now() - PAIRING_IDLE_TIMEOUT * 2);
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
        WsCommand::Pair { .. } => {
            // Pair is handled earlier in handle_ws; reaching here is a logic error.
            Ok(json!({"type":"error","message":"pair_not_allowed_here"}))
        }
    }
}
