//! Start with Windows via HKCU\...\Run (per-user, no admin). CHECKLIST J3 / SPEC S3.
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::*;

const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const NAME: PCWSTR = w!("DeskPanel");

fn command() -> Option<Vec<u16>> {
    let exe = std::env::current_exe().ok()?;
    let s = format!("\"{}\" --autostart", exe.display());
    Some(s.encode_utf16().chain(std::iter::once(0)).collect())
}

/// Make the registry match the setting. Also refreshes the path if the exe moved (e.g. installed).
pub fn apply(on: bool) {
    unsafe {
        let r = if on {
            match command() {
                Some(v) => RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(v.as_ptr() as *const _), (v.len() * 2) as u32),
                None => return,
            }
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME)
        };
        crate::log!("autostart {} -> {:?}", if on { "on" } else { "off" }, r);
    }
}
