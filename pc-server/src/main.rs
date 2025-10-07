#[cfg(not(windows))]
compile_error!("This example currently only builds on Windows (uses Core Audio).");

use std::convert::Infallible;

use futures::{SinkExt, StreamExt};
use log::{error, info};
use serde::Deserialize;
use serde_json::json;
use warp::ws::{Message, WebSocket};
use warp::Filter;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting PC Remote WS server...");

    // Health check (optional)
    let health = warp::path!("health").map(|| "ok");

    // WebSocket route at /ws
    let ws_route = warp::path!("ws")
        .and(warp::ws())
        .and_then(|ws: warp::ws::Ws| async move {
            Ok::<_, Infallible>(ws.on_upgrade(handle_ws))
        });

    let routes = health
        .or(ws_route)
        .with(warp::cors().allow_any_origin())
        .with(warp::log("pc_remote_ws"));

    let addr = ([0, 0, 0, 0], 3030);
    let ip = std::net::Ipv4Addr::new(0,0,0,0);
    let port = 3030u16;
    info!("Listening on ws://{}:{}/ws", ip, port);
    warp::serve(routes).run(addr).await;
}

async fn handle_ws(ws: WebSocket) {
    let (mut tx, mut rx) = ws.split();

    // Send a hello/status when connected
    if let Err(e) = tx
        .send(Message::text(
            json!({
                "type": "hello",
                "server": "pc-remote-ws",
                "version": "0.1.0",
                "hint": "send JSON like {\"cmd\":\"get_status\"} or {\"cmd\":\"volume_up\",\"delta\":0.05}"
            })
                .to_string(),
        ))
        .await
    {
        error!("Failed to send welcome: {e}");
        return;
    }

    while let Some(Ok(msg)) = rx.next().await {
        if !msg.is_text() {
            continue;
        }
        let text = msg.to_str().unwrap_or_default();

        let reply = match serde_json::from_str::<WsCommand>(text) {
            Ok(cmd) => handle_command(cmd).unwrap_or_else(|e| {
                json!({"type":"error","message":e.to_string()})
            }),
            Err(e) => json!({"type":"error","message":format!("invalid json: {e}")}),
        };

        if let Err(e) = tx.send(Message::text(reply.to_string())).await {
            error!("Failed to send response: {e}");
            break;
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum WsCommand {
    // {"cmd":"get_status"}
    GetStatus,

    // {"cmd":"set_volume","level":0.42}
    SetVolume { level: f32 },

    // {"cmd":"volume_up","delta":0.05}
    VolumeUp { delta: Option<f32> },

    // {"cmd":"volume_down","delta":0.05}
    VolumeDown { delta: Option<f32> },

    // {"cmd":"toggle_mute"}
    ToggleMute,

    // {"cmd":"mute"}
    Mute,

    // {"cmd":"unmute"}
    Unmute,
}

fn handle_command(cmd: WsCommand) -> anyhow::Result<serde_json::Value> {
    match cmd {
        WsCommand::GetStatus => {
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"status","volume":vol,"muted":muted}))
        }
        WsCommand::SetVolume { level } => {
            let level = level.clamp(0.0, 1.0);
            windows_audio::set_volume(level)?;
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"set_volume","volume":vol,"muted":muted}))
        }
        WsCommand::VolumeUp { delta } => {
            let delta = delta.unwrap_or(0.05).clamp(0.0, 1.0);
            let (mut vol, _) = windows_audio::get_volume_and_mute()?;
            vol = (vol + delta).clamp(0.0, 1.0);
            windows_audio::set_volume(vol)?;
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"volume_up","volume":vol,"muted":muted}))
        }
        WsCommand::VolumeDown { delta } => {
            let delta = delta.unwrap_or(0.05).clamp(0.0, 1.0);
            let (mut vol, _) = windows_audio::get_volume_and_mute()?;
            vol = (vol - delta).clamp(0.0, 1.0);
            windows_audio::set_volume(vol)?;
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"volume_down","volume":vol,"muted":muted}))
        }
        WsCommand::ToggleMute => {
            let (_, muted) = windows_audio::get_volume_and_mute()?;
            windows_audio::set_mute(!muted)?;
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"toggle_mute","volume":vol,"muted":muted}))
        }
        WsCommand::Mute => {
            windows_audio::set_mute(true)?;
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"mute","volume":vol,"muted":muted}))
        }
        WsCommand::Unmute => {
            windows_audio::set_mute(false)?;
            let (vol, muted) = windows_audio::get_volume_and_mute()?;
            Ok(json!({"type":"ok","action":"unmute","volume":vol,"muted":muted}))
        }
    }
}

#[cfg(windows)]
mod windows_audio {
    use anyhow::Result;
    use std::ptr;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
    };
    use windows::Win32::Foundation::BOOL;

    fn ensure_com_initialized() {
        unsafe {
            // Best-effort: try MTA, then STA. Ignore "already initialized" errors.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
    }

    fn default_render_endpoint() -> Result<IMMDevice> {
        ensure_com_initialized();
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            Ok(device)
        }
    }

    fn endpoint_volume() -> Result<IAudioEndpointVolume> {
        ensure_com_initialized();
        unsafe {
            let device = default_render_endpoint()?;
            // New generic Activate signature in windows 0.58
            let ep: IAudioEndpointVolume = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)?;
            Ok(ep)
        }
    }

    pub fn get_volume_and_mute() -> Result<(f32, bool)> {
        ensure_com_initialized();
        unsafe {
            let ep = endpoint_volume()?;
            let level = ep.GetMasterVolumeLevelScalar()?; // returns f32
            let muted = ep.GetMute()?.as_bool();          // returns BOOL
            Ok((level, muted))
        }
    }

    pub fn set_volume(level: f32) -> Result<()> {
        ensure_com_initialized();
        unsafe {
            let ep = endpoint_volume()?;
            let level = level.clamp(0.0, 1.0);
            // Pass null event-context GUID
            ep.SetMasterVolumeLevelScalar(level, ptr::null())?;
            Ok(())
        }
    }

    pub fn set_mute(mute: bool) -> Result<()> {
        ensure_com_initialized();
        unsafe {
            let ep = endpoint_volume()?;
            ep.SetMute(BOOL(mute as i32), ptr::null())?;
            Ok(())
        }
    }
}
