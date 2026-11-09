//! Windows light/dark: HKCU Personalize (apps + system), then the same broadcast the Settings app
//! sends, so Explorer, the taskbar and our own webview (prefers-color-scheme) re-theme live.
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

fn read(name: PCWSTR) -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut size))
    };
    r.is_ok().then_some(v)
}

fn write(name: PCWSTR, v: u32) -> bool {
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, name, REG_DWORD.0, Some(&v as *const u32 as *const _), 4).is_ok() }
}

pub fn is_dark() -> bool {
    read(w!("AppsUseLightTheme")) == Some(0)
}

pub fn set_dark(dark: bool) -> bool {
    let v = if dark { 0 } else { 1 };
    let ok = write(w!("AppsUseLightTheme"), v) & write(w!("SystemUsesLightTheme"), v);
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            2000,
            None,
        );
    }
    ok
}
