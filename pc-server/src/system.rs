// src/system.rs
use anyhow::Result;

pub fn open_calculator() -> Result<()> {
    // spawn and detach
    std::process::Command::new("calc.exe").spawn()?;
    Ok(())
}

pub fn take_screenshot() -> Result<()> {
    // Win + PrintScreen -> saves into Pictures\Screenshots
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        VK_LWIN, VK_SNAPSHOT, VIRTUAL_KEY,
    };

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    unsafe {
        let inputs = [
            key(VK_LWIN, false),     // Win down
            key(VK_SNAPSHOT, false), // PrtScn down
            key(VK_SNAPSHOT, true),  // PrtScn up
            key(VK_LWIN, true),      // Win up
        ];

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            anyhow::bail!("SendInput failed (sent {sent}/{})", inputs.len());
        }
    }

    Ok(())
}
