//! Hidden top-level "broker" window + WinEvent hook: the single place OS events enter the app.
//! A child window (the panel, in Desktop mode) never receives broadcasts like WM_SETTINGCHANGE,
//! so a dedicated hidden top-level window owns the hotkey and all system notifications.
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;


use crate::app;

pub const HOTKEY_ID: i32 = 1;
// Fixed Win32 values (WM_WTSSESSION_CHANGE / WM_POWERBROADCAST wParam).
const WTS_SESSION_LOCK: u32 = 0x7;
const WTS_SESSION_UNLOCK: u32 = 0x8;
const PBT_APMSUSPEND: u32 = 0x4;
const PBT_APMRESUMEAUTOMATIC: u32 = 0x12;
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static BROKER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Occlusion is re-checked 150 ms after the last window event (moves arrive in bursts).
const OCCLUSION_TIMER: usize = 7;

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_HOTKEY if wp.0 as i32 == HOTKEY_ID => app::on_hotkey(),
        WM_SETTINGCHANGE => {
            let what = if lp.0 != 0 { unsafe { PCWSTR(lp.0 as *const u16).to_string().unwrap_or_default() } } else { String::new() };
            if wp.0 == SPI_SETDESKWALLPAPER.0 as usize {
                app::on_wallpaper_changed();
            } else if what == "ImmersiveColorSet" {
                app::on_theme_changed();
            }
        }
        WM_DISPLAYCHANGE => app::on_display_changed(),
        WM_TIMER if wp.0 == OCCLUSION_TIMER => {
            unsafe {
                let _ = KillTimer(Some(h), OCCLUSION_TIMER);
            }
            app::on_occlusion_check();
        }
        WM_WTSSESSION_CHANGE => match wp.0 as u32 {
            WTS_SESSION_LOCK => app::on_session_event("lock"),
            WTS_SESSION_UNLOCK => app::on_session_event("unlock"),
            _ => {}
        },
        WM_POWERBROADCAST => match wp.0 as u32 {
            PBT_APMSUSPEND => app::on_session_event("suspend"),
            PBT_APMRESUMEAUTOMATIC => app::on_session_event("resume"),
            _ => {}
        },
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => app::on_shell_restarted(),
        _ => {}
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

unsafe extern "system" fn on_win_event(_h: HWINEVENTHOOK, ev: u32, hwnd: HWND, obj: i32, _c: i32, _t: u32, _ms: u32) {
    if obj != OBJID_WINDOW.0 {
        return;
    }
    if ev == EVENT_SYSTEM_FOREGROUND {
        app::on_foreground(hwnd);
    }
    // Any top-level window appearing, moving, minimizing, cloaking...: re-check occlusion soon.
    if _c == 0 && !hwnd.0.is_null() && unsafe { GetAncestor(hwnd, GA_ROOT) } == hwnd {
        schedule_occlusion_check();
    }
}

pub fn schedule_occlusion_check() {
    let b = BROKER.load(Ordering::Relaxed);
    if b != 0 {
        unsafe {
            let _ = SetTimer(Some(HWND(b as *mut _)), OCCLUSION_TIMER, 150, None);
        }
    }
}

/// Create the broker, register the global hotkey, install the foreground hook. Main thread only.
pub fn start() -> Result<HWND, String> {
    unsafe {
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        let inst = GetModuleHandleW(PCWSTR::null()).map_err(|e| e.to_string())?;
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: w!("DeskPanelBroker"), ..Default::default() };
        RegisterClassW(&wc);
        let h = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("DeskPanelBroker"),
            w!("DeskPanel Broker"),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, Some(inst.into()), None,
        )
        .map_err(|e| e.to_string())?;

        match RegisterHotKey(Some(h), HOTKEY_ID, MOD_WIN | MOD_ALT | MOD_NOREPEAT, VK_SPACE.0 as u32) {
            Ok(()) => crate::log!("hotkey Win+Alt+Space registered"),
            Err(e) => crate::log!("hotkey Win+Alt+Space FAILED: {e}"),
        }
        BROKER.store(h.0 as usize, Ordering::Relaxed);
        let hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_win_event), 0, 0, WINEVENT_OUTOFCONTEXT);
        // Occlusion inputs: minimize start/end, show/hide/destroy/location change, cloak/uncloak.
        for (lo, hi) in [(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND), (EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE), (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED)] {
            SetWinEventHook(lo, hi, None, Some(on_win_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        }
        // Lock/unlock notifications (sleep/resume arrive as WM_POWERBROADCAST on their own).
        let wts = windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(h, windows::Win32::System::RemoteDesktop::NOTIFY_FOR_THIS_SESSION);
        crate::log!("session notifications registered ok={}", wts.is_ok());
        crate::log!("broker {:#x}, foreground hook ok={}", h.0 as usize, !hook.0.is_null());
        Ok(h)
    }
}
