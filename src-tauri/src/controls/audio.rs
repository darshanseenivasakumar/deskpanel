//! Default output device volume/mute via Core Audio, with change notifications (so the slider
//! follows the Windows volume flyout and media keys without polling).
use std::sync::mpsc::Sender;

use windows::core::{implement, Result, PWSTR};
use windows::Win32::Media::Audio::Endpoints::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

use super::Msg;

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback(Sender<Msg>);

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        let _ = self.0.send(Msg::OsChanged);
        Ok(())
    }
}

pub struct Audio {
    enumerator: IMMDeviceEnumerator,
    device_id: String,
    ep: IAudioEndpointVolume,
    cb: IAudioEndpointVolumeCallback,
    tx: Sender<Msg>,
}

fn default_device(en: &IMMDeviceEnumerator) -> Result<(IMMDevice, String)> {
    unsafe {
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let id: PWSTR = dev.GetId()?;
        let s = id.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id.0 as *const _));
        Ok((dev, s))
    }
}

impl Audio {
    pub fn new(tx: Sender<Msg>) -> Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let (dev, device_id) = default_device(&enumerator)?;
            let ep: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
            let cb: IAudioEndpointVolumeCallback = VolumeCallback(tx.clone()).into();
            ep.RegisterControlChangeNotify(&cb)?;
            Ok(Audio { enumerator, device_id, ep, cb, tx })
        }
    }

    /// Headphones plugged in / default device switched: re-bind to the new default device.
    pub fn follow_default_device(&mut self) {
        if let Ok((_, id)) = default_device(&self.enumerator) {
            if id != self.device_id {
                if let Ok(n) = Audio::new(self.tx.clone()) {
                    crate::log!("audio: default device changed, re-bound");
                    *self = n;
                }
            }
        }
    }

    pub fn volume(&self) -> Result<f32> {
        unsafe { self.ep.GetMasterVolumeLevelScalar() }
    }
    pub fn muted(&self) -> Result<bool> {
        unsafe { Ok(self.ep.GetMute()?.as_bool()) }
    }
    pub fn set_volume(&self, v: f32) -> Result<()> {
        unsafe { self.ep.SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), std::ptr::null()) }
    }
    pub fn set_mute(&self, m: bool) -> Result<()> {
        unsafe { self.ep.SetMute(m, std::ptr::null()) }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        unsafe {
            let _ = self.ep.UnregisterControlChangeNotify(&self.cb);
        }
    }
}
