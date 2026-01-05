// src/media.rs
use anyhow::Result;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VIRTUAL_KEY,
};

fn send_media_key(vk: VIRTUAL_KEY) -> Result<()> {
    unsafe {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        let inputs = [down, up];

        // SendInput returns number of events successfully inserted
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            anyhow::bail!("SendInput failed (sent {sent}/{})", inputs.len());
        }
    }

    Ok(())
}

pub fn next_track() -> Result<()> {
    send_media_key(VK_MEDIA_NEXT_TRACK)
}

pub fn previous_track() -> Result<()> {
    send_media_key(VK_MEDIA_PREV_TRACK)
}

pub fn toggle_play_pause() -> Result<()> {
    send_media_key(VK_MEDIA_PLAY_PAUSE)
}
