// src/server/commands.rs
#![cfg(windows)]

use serde::Deserialize;
use serde_json::json;

use crate::{audio, media, system};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum WsCommand {
    GetStatus,
    SetVolume { level: f32 },
    VolumeUp { delta: Option<f32> },
    VolumeDown { delta: Option<f32> },
    ToggleMute,
    NextTrack,
    PreviousTrack,
    TogglePlayPause,
    ToggleMicMute,
    TakeScreenshot,
    OpenCalculator,
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

// NOTE: Pair/Auth are handled in ws.rs. This function is for "device control" commands.
pub fn handle_command(cmd: WsCommand) -> anyhow::Result<serde_json::Value> {
    match cmd {
        WsCommand::GetStatus => {
            let (vol, muted) = audio::get_volume_and_mute()?;
            let mic_muted = audio::get_mic_mute()?;
            Ok(json!({"type":"status","volume":vol,"muted":muted,"mic_muted":mic_muted}))
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
            Ok(json!({
                "type": "ok",
                "action": "toggle_mute",
                "volume": vol,
                "muted": muted
            }))
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
        WsCommand::NextTrack => {
            media::next_track()?;
            Ok(json!({"type":"ok","action":"next_track"}))
        }
        WsCommand::PreviousTrack => {
            media::previous_track()?;
            Ok(json!({"type":"ok","action":"previous_track"}))
        }
        WsCommand::TogglePlayPause => {
            media::toggle_play_pause()?;
            Ok(json!({"type":"ok","action":"toggle_play_pause"}))
        }
        WsCommand::ToggleMicMute => {
            let mic_muted = audio::get_mic_mute()?;
            audio::set_mic_mute(!mic_muted)?;
            let (vol, muted) = audio::get_volume_and_mute()?;
            let mic_muted = audio::get_mic_mute()?;
            Ok(json!({"type":"ok","action":"toggle_mic_mute","volume":vol,"muted":muted,"mic_muted":mic_muted}))
        }
        WsCommand::TakeScreenshot => {
            system::take_screenshot()?;
            Ok(json!({"type":"ok","action":"take_screenshot"}))
        }
        WsCommand::OpenCalculator => {
            system::open_calculator()?;
            Ok(json!({"type":"ok","action":"open_calculator"}))
        }

        // These should never hit handle_command (handled in ws.rs)
        WsCommand::Pair { .. } | WsCommand::Auth { .. } => {
            Ok(json!({"type":"error","reason":"invalid_command_context"}))
        }
    }
}
