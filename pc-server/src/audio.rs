use anyhow::Result;
use windows::core::GUID;
use windows::Win32::Foundation::BOOL;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};

fn ensure_com_initialized() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

fn default_render_endpoint() -> Result<IMMDevice> {
    ensure_com_initialized();
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        Ok(enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?)
    }
}

fn endpoint_volume() -> Result<IAudioEndpointVolume> {
    ensure_com_initialized();
    unsafe {
        let device = default_render_endpoint()?;
        // windows 0.58+ generic Activate API (works with your current setup):
        let ep: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        Ok(ep)
    }
}

pub fn get_volume_and_mute() -> Result<(f32, bool)> {
    ensure_com_initialized();
    unsafe {
        let ep = endpoint_volume()?;
        let level = ep.GetMasterVolumeLevelScalar()?;
        let mute = ep.GetMute()?.as_bool();
        Ok((level, mute))
    }
}

pub fn set_volume(level: f32) -> Result<()> {
    ensure_com_initialized();
    unsafe {
        let ep = endpoint_volume()?;
        ep.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), &GUID::zeroed())?;
        Ok(())
    }
}

pub fn set_mute(mute: bool) -> Result<()> {
    ensure_com_initialized();
    unsafe {
        let ep = endpoint_volume()?;
        ep.SetMute(BOOL(mute as i32), &GUID::zeroed())?;
        Ok(())
    }
}
