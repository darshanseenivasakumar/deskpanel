//! Wi-Fi / Bluetooth radio on-off through WinRT Windows.Devices.Radios (proven in the Stage 1
//! spike: access Allowed for an unpackaged exe, BT toggled Off->On->Off).
use std::sync::mpsc::Sender;

use windows::core::Result;
use windows::Devices::Radios::*;
use windows::Foundation::TypedEventHandler;

use super::Msg;

pub struct Radios {
    wifi: Option<Radio>,
    bt: Option<Radio>,
}

#[derive(Clone, Copy, Debug)]
pub enum Which {
    Wifi,
    Bluetooth,
}

impl Radios {
    pub fn new(tx: Sender<Msg>) -> Result<Self> {
        let access = Radio::RequestAccessAsync()?.join()?;
        crate::log!("radios: access {access:?}");
        let mut out = Radios { wifi: None, bt: None };
        for r in Radio::GetRadiosAsync()?.join()? {
            let kind = r.Kind()?;
            let slot = if kind == RadioKind::WiFi {
                &mut out.wifi
            } else if kind == RadioKind::Bluetooth {
                &mut out.bt
            } else {
                continue;
            };
            if slot.is_some() {
                continue;
            }
            let t = tx.clone();
            r.StateChanged(&TypedEventHandler::new(move |_, _| {
                let _ = t.send(Msg::OsChanged);
                Ok(())
            }))?;
            *slot = Some(r);
        }
        Ok(out)
    }

    fn radio(&self, w: Which) -> Option<&Radio> {
        match w {
            Which::Wifi => self.wifi.as_ref(),
            Which::Bluetooth => self.bt.as_ref(),
        }
    }

    pub fn state(&self, w: Which) -> Option<bool> {
        self.radio(w).and_then(|r| r.State().ok()).map(|s| s == RadioState::On)
    }

    pub fn set(&self, w: Which, on: bool) -> std::result::Result<(), String> {
        let r = self.radio(w).ok_or("no such radio")?;
        let target = if on { RadioState::On } else { RadioState::Off };
        let res = r.SetStateAsync(target).map_err(|e| e.to_string())?.join().map_err(|e| e.to_string())?;
        if res == RadioAccessStatus::Allowed {
            Ok(())
        } else {
            Err(format!("radio access {res:?}"))
        }
    }
}

/// Name of the Wi-Fi network we are connected to, if the OS reveals it without location access.
pub fn wifi_name() -> Option<String> {
    use windows::Networking::Connectivity::NetworkInformation;
    let p = NetworkInformation::GetInternetConnectionProfile().ok()?;
    if !p.IsWlanConnectionProfile().ok()? {
        return None;
    }
    p.ProfileName().ok().map(|s| s.to_string()).filter(|s| !s.is_empty())
}
