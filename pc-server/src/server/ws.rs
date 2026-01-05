// src/server/ws.rs
#![cfg(windows)]

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use log::{error, info};
use serde_json::json;
use std::net::{SocketAddr};
use std::sync::{Arc, Mutex};
use tokio::{select, sync::oneshot};
use tokio_util::sync::CancellationToken;
use warp::http::StatusCode;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::server::auth_store::{generate_token, sha256_hex};
use crate::server::commands::{handle_command, WsCommand};
use crate::server::pairing::{generate_pairing_code, PairingState};

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

    let health = warp::path!("health").map(|| warp::reply::with_status("ok", StatusCode::OK));

    let ws_route = warp::path!("ws")
        .and(warp::ws())
        .and(warp::addr::remote())
        .and(cancel_filter.clone())
        .and(pairing_filter.clone())
        .map(|ws: warp::ws::Ws, remote: Option<SocketAddr>, cancel: CancellationToken, pairing| {
            ws.on_upgrade(move |socket| async move {
                handle_ws(socket, cancel, remote, pairing).await;
            })
        });

    let routes = health.or(ws_route);

    let addr = ([0, 0, 0, 0], port);

    // idle watchdog: if no heartbeat, clear active session
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

    let cancel_for_shutdown = cancel.clone();
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

    let hello = json!({
        "type":"hello",
        "paired": is_active_paired,
        "active_device_id": active_id,
        "authorized_count": authorized_count,
        "pairing_code": code,
        "pairing_code_expired": code_expired,
    });

    if tx.send(Message::text(hello.to_string())).await.is_err() {
        return;
    }

    loop {
        select! {
            _ = cancel.cancelled() => {
                let _ = tx.send(Message::close()).await;
                break;
            }

            msg = rx.next() => {
                let Some(Ok(msg)) = msg else { break; };

                if !msg.is_text() {
                    continue;
                }

                let text = match msg.to_str() {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                let reply = match serde_json::from_str::<WsCommand>(text) {

                    // ---------------------------
                    // AUTH
                    // ---------------------------
                    Ok(WsCommand::Auth { device_id, token }) => {
                        if remote_ip.is_none() {
                            json!({"type":"auth_error","reason":"no_remote_ip"})
                        } else {
                            let ip = remote_ip.unwrap();
                            let mut st = pairing.lock().unwrap();

                            if let Some(rem) = st.rl_is_locked(ip) {
                                json!({"type":"rate_limited","reason":"auth","retry_after_secs": rem})
                            } else {
                                if st.is_authorized(&device_id, &token) {
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
                        }
                    }

                    // ---------------------------
                    // PAIR
                    // ---------------------------
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
                                    st.created_at = std::time::Instant::now();
                                }

                                if st.code != code {
                                    st.rl_register_failure(ip);
                                    json!({"type":"pairing_error","reason":"invalid_code"})
                                } else {
                                    st.rl_register_success(ip);

                                    // generate + store token
                                    let token = generate_token();
                                    let token_hash = sha256_hex(&token);
                                    st.upsert_authorized(device_id.clone(), token_hash, device_name);

                                    authenticated = true;
                                    authed_device_id = Some(device_id.clone());

                                    st.active_device_id = Some(device_id);
                                    st.active_client_ip = Some(ip);
                                    st.mark_seen();

                                    json!({"type":"pairing_ok","token": token})
                                }
                            }
                        }
                    }

                    // ---------------------------
                    // DEVICE CONTROL COMMANDS
                    // ---------------------------
                    Ok(cmd) => {
                        if !authenticated {
                            json!({"type":"error","reason":"not_authenticated"})
                        } else {
                            // heartbeat / keepalive
                            {
                                let mut st = pairing.lock().unwrap();
                                st.mark_seen();
                            }

                            match handle_command(cmd) {
                                Ok(v) => v,
                                Err(e) => {
                                    error!("Command error: {e:?}");
                                    json!({"type":"error","reason":"command_failed"})
                                }
                            }
                        }
                    }

                    Err(e) => {
                        error!("Bad JSON from client: {e:?}");
                        json!({"type":"error","reason":"bad_request"})
                    }
                };

                // if authenticated, ensure active session still matches this device
                if authenticated {
                    let st = pairing.lock().unwrap();
                    if let (Some(active), Some(me)) = (&st.active_device_id, &authed_device_id) {
                        if active != me {
                            authenticated = false;
                            authed_device_id = None;
                        }
                    }
                }

                if tx.send(Message::text(reply.to_string())).await.is_err() {
                    break;
                }
            }
        }
    }

    // client disconnected; watchdog clears active session if no heartbeat
}
