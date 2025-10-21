// src/server.rs
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use log::{error, info};
use serde::Deserialize;
use serde_json::json;
use tokio::{select, sync::oneshot};
use tokio_util::sync::CancellationToken;
use warp::ws::{Message, WebSocket};
use warp::{Filter, http::StatusCode};

use crate::audio;

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
}

pub async fn run_ws_server(port: u16, shutdown_rx: oneshot::Receiver<()>) -> Result<()> {
    let cancel = CancellationToken::new();
    let cancel_filter = warp::any().map({
        let cancel = cancel.clone();
        move || cancel.clone()
    });

    // Health endpoint
    let health = warp::path!("health")
        .map(|| warp::reply::with_status("ok", StatusCode::OK))
        .boxed();

    // WebSocket endpoint
    let ws_route = warp::path!("ws")
        .and(warp::ws())
        .and(cancel_filter)
        .map(|ws: warp::ws::Ws, cancel: CancellationToken| {
            ws.on_upgrade(move |socket| async move {
                handle_ws(socket, cancel).await;
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

async fn handle_ws(ws: WebSocket, cancel: CancellationToken) {
    let (mut tx, mut rx) = ws.split();

    // Send welcome message
    if let Err(e) = tx
        .send(Message::text(
            json!({
                "type": "hello",
                "server": "foss-deck",
                "version": "0.1.0",
                "hint": "send JSON like {\"cmd\":\"get_status\"} / {\"cmd\":\"set_volume\",\"level\":0.5}"
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
                            Ok(cmd) => handle_command(cmd).unwrap_or_else(|e| json!({"type":"error","message":e.to_string()})),
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
                    None => break, // client disconnected
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
    }
}
