//! Window-procedure hooks on the panel window: keep a drag on-screen while it happens (WM_MOVING),
//! report moves so the glass / wallpaper ring follow the window, and make sure clicking the panel on
//! the desktop never activates anything.
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::BOOL;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::desktop;

static MA_LOGGED: AtomicU32 = AtomicU32::new(0);

/// Answer for every window in the panel's chain that is asked WM_MOUSEACTIVATE.
/// On the desktop: never activate. Left to DefWindowProc, the question bubbles up to Progman,
/// which activates the DESKTOP and steals focus from the app you were in (CHECKLIST B6).
/// When popped, taking focus is intended, so it falls through.
fn mouse_activate(panel_root: HWND) -> Option<LRESULT> {
    if desktop::is_child(panel_root) {
        if MA_LOGGED.fetch_add(1, Ordering::Relaxed) < 3 {
            crate::log!("WM_MOUSEACTIVATE on desktop panel -> MA_NOACTIVATE");
        }
        Some(LRESULT(MA_NOACTIVATE as isize))
    } else {
        None
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_MOVING => {
            // Clamp the proposed rect so >= 81% of the panel stays inside the work area (G2).
            let r = unsafe { &mut *(lp.0 as *mut RECT) };
            let (x, y) = desktop::clamp_on_screen(h, r.left, r.top);
            let (dx, dy) = (x - r.left, y - r.top);
            r.left += dx;
            r.right += dx;
            r.top += dy;
            r.bottom += dy;
            return LRESULT(1);
        }
        WM_MOUSEACTIVATE => {
            if let Some(r) = mouse_activate(h) {
                return r;
            }
        }
        WM_MOVE => crate::app::on_panel_moved(false),
        WM_EXITSIZEMOVE => crate::app::on_panel_moved(true),
        _ => {}
    }
    unsafe { DefSubclassProc(h, msg, wp, lp) }
}

/// Same answer on the panel's in-process child windows (wry's WRY_WEBVIEW host), which can
/// reply to WM_MOUSEACTIVATE before the question ever reaches the panel window itself.
unsafe extern "system" fn child_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, root: usize) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        if let Some(r) = mouse_activate(HWND(root as *mut _)) {
            return r;
        }
    }
    unsafe { DefSubclassProc(h, msg, wp, lp) }
}

unsafe extern "system" fn hook_child(child: HWND, root: LPARAM) -> BOOL {
    if desktop::is_ours(child) {
        let _ = unsafe { SetWindowSubclass(child, Some(child_proc), 0x0de6, root.0 as usize) };
    }
    BOOL(1)
}

pub fn install(h: HWND) {
    unsafe {
        let _ = SetWindowSubclass(h, Some(proc), 0x0de5, 0);
    }
    hook_children(h);
}

/// Hook the panel's in-process descendants. The webview host windows are created a little after the
/// panel window, so this is called again once the page reports ready.
pub fn hook_children(h: HWND) {
    unsafe {
        let _ = EnumChildWindows(Some(h), Some(hook_child), LPARAM(h.0 as isize));
    }
}

/// Start the system move loop for a drag that began in the webview (mouse button still down).
pub fn begin_drag(h: HWND) {
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        // SC_MOVE | HTCAPTION: the classic "drag by client area" move loop.
        let _ = PostMessageW(Some(h), WM_SYSCOMMAND, WPARAM(0xF012), LPARAM(0));
    }
}
