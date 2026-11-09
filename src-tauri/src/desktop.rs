//! Window placement primitives, proven in the Stage 1 spike (evidence/stage1/STAGE1.md):
//! - Desktop mode = WS_CHILD of Progman (survives Win+D, covered by every app, never takes focus).
//! - Popped mode  = top-level HWND_TOPMOST that may take focus (only right after the hotkey/tray).
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn hwnd(v: usize) -> HWND {
    HWND(v as *mut core::ffi::c_void)
}

pub fn progman() -> HWND {
    unsafe { FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default() }
}

pub fn class_of(h: HWND) -> String {
    let mut b = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}

pub fn exe_of(h: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return format!("pid{pid}") };
        let mut b = [0u16; 512];
        let mut n = b.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        if ok {
            String::from_utf16_lossy(&b[..n as usize]).rsplit('\\').next().unwrap_or("").to_string()
        } else {
            format!("pid{pid}")
        }
    }
}

pub fn describe(h: HWND) -> String {
    if h.0.is_null() {
        return "null".into();
    }
    format!("{:#x}/{}/{}", h.0 as usize, class_of(h), exe_of(h))
}

/// "Desktop-ish" foreground: the desktop itself, the taskbar, or nothing.
pub fn is_desktopish(h: HWND) -> bool {
    if h.0.is_null() {
        return true;
    }
    matches!(class_of(h).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

fn set_styles(h: HWND, child: bool) {
    unsafe {
        let st = GetWindowLongPtrW(h, GWL_STYLE);
        let st = if child {
            (st & !(WS_POPUP.0 as isize)) | WS_CHILD.0 as isize
        } else {
            (st & !(WS_CHILD.0 as isize)) | WS_POPUP.0 as isize
        };
        SetWindowLongPtrW(h, GWL_STYLE, st);
        let mut ex = GetWindowLongPtrW(h, GWL_EXSTYLE) | WS_EX_TOOLWINDOW.0 as isize;
        ex &= !(WS_EX_APPWINDOW.0 as isize);
        if child {
            ex |= WS_EX_NOACTIVATE.0 as isize;
        } else {
            ex &= !(WS_EX_NOACTIVATE.0 as isize);
        }
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
    }
}

/// CSS layout constants (src/styles/panel.css): the panel sits PANEL_INSET css px inside the
/// window and has PANEL_RADIUS css px corners. The desktop-mode region keeps a RING css px band
/// around the panel (painted with the real wallpaper) so anti-aliased corners blend correctly.
const PANEL_INSET: f64 = 40.0;
const PANEL_RADIUS: f64 = 24.0;
const RING: f64 = 4.0;

/// Desktop mode: clip the window to the panel shape (+ring). Needed because when Progman renders
/// through DirectComposition (WS_EX_NOREDIRECTIONBITMAP, seen after a reboot on build 26300),
/// a child window's transparent pixels come out black instead of showing the wallpaper.
/// Popped mode (top-level) has real per-pixel transparency, so the region is removed.
pub fn set_region(h: HWND, desktop_mode: bool) {
    unsafe {
        if !desktop_mode {
            SetWindowRgn(h, None, true);
            return;
        }
        let s = scale(h);
        let r = rect(h);
        let (w, hh) = (r.right - r.left, r.bottom - r.top);
        let inset = ((PANEL_INSET - RING) * s).floor() as i32;
        let d = ((PANEL_RADIUS + RING) * 2.0 * s).round() as i32;
        let rgn = CreateRoundRectRgn(inset, inset, w - inset + 1, hh - inset + 1, d, d);
        // On success the system owns the region; do not delete it.
        SetWindowRgn(h, Some(rgn), true);
    }
}

pub fn to_desktop(h: HWND) {
    unsafe {
        let pm = progman();
        let _ = SetWindowPos(h, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        set_styles(h, true);
        set_region(h, true);
        let r = SetParent(h, Some(pm));
        let _ = SetWindowPos(h, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        crate::log!("to_desktop parent={} ok={}", describe(pm), r.is_ok());
    }
}

pub fn to_topmost(h: HWND) {
    unsafe {
        set_styles(h, false);
        set_region(h, false);
        let _ = SetParent(h, None);
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        crate::log!("to_topmost");
    }
}

/// Topmost overlay that can never take focus (the pill). Unlike `to_topmost`, which is for a
/// deliberately popped panel, this keeps WS_EX_NOACTIVATE and never re-parents.
pub fn make_overlay(h: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex & !(WS_EX_APPWINDOW.0 as isize));
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Capture what is on screen BEHIND a top-level window, without hiding it: the window is briefly
/// excluded from capture (WDA_EXCLUDEFROMCAPTURE, Win10 2004+), one compositor frame is awaited,
/// and the screen is read. Returns BGRA pixels of the window's rect.
pub fn capture_behind(h: HWND) -> Option<(Vec<u8>, i32, i32)> {
    unsafe {
        let r = rect(h);
        let (w, hh) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || hh <= 0 {
            return None;
        }
        let excluded = SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE).is_ok();
        let _ = windows::Win32::Graphics::Dwm::DwmFlush();
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, hh);
        let old = SelectObject(mem, bmp.into());
        let _ = BitBlt(mem, 0, 0, w, hh, Some(screen), r.left, r.top, SRCCOPY | CAPTUREBLT);
        if excluded {
            let _ = SetWindowDisplayAffinity(h, WDA_NONE);
        }
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -hh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * hh * 4) as usize];
        GetDIBits(mem, bmp, 0, hh as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !excluded {
            crate::log!("capture: display affinity not supported; snapshot may include the panel");
        }
        Some((buf, w, hh))
    }
}

/// Clip a window to a rounded rect given in CSS px (window-relative). Used for the pill, so its
/// transparent surroundings never swallow clicks meant for the app underneath.
pub fn set_round_region(h: HWND, x: f64, y: f64, w: f64, hh: f64, r: f64) {
    unsafe {
        let s = scale(h);
        let (l, t) = ((x * s).floor() as i32, (y * s).floor() as i32);
        let (rr, b) = (((x + w) * s).ceil() as i32 + 1, ((y + hh) * s).ceil() as i32 + 1);
        let d = (2.0 * r * s).round() as i32;
        let rgn = CreateRoundRectRgn(l, t, rr, b, d, d);
        SetWindowRgn(h, Some(rgn), true);
    }
}

/// A fullscreen app is in front: Windows' own notion (fullscreen / D3D exclusive / presentation
/// mode), or the foreground app window covers its whole monitor (borderless fullscreen video).
pub fn fullscreen_app_in_front() -> bool {
    use windows::Win32::UI::Shell::*;
    if let Ok(st) = unsafe { SHQueryUserNotificationState() } {
        if st == QUNS_BUSY || st == QUNS_RUNNING_D3D_FULL_SCREEN || st == QUNS_PRESENTATION_MODE {
            return true;
        }
    }
    let fg = foreground();
    if fg.0.is_null() || is_ours(fg) || is_desktopish(fg) {
        return false;
    }
    // A *maximized* window also covers the whole monitor when the taskbar auto-hides, but it is
    // not fullscreen: borderless fullscreen (video, games, F11) is never in the maximized state.
    if unsafe { IsZoomed(fg).as_bool() } {
        return false;
    }
    let (mon, _) = monitor_of(fg);
    let r = rect(fg);
    r.left <= mon.left && r.top <= mon.top && r.right >= mon.right && r.bottom >= mon.bottom
}

/// True while the panel is in desktop mode (a WS_CHILD of Progman).
pub fn is_child(h: HWND) -> bool {
    unsafe { (GetWindowLongPtrW(h, GWL_STYLE) & WS_CHILD.0 as isize) != 0 }
}

pub fn show_noactivate(h: HWND) {
    unsafe {
        let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
    }
}

pub fn hide(h: HWND) {
    unsafe {
        let _ = ShowWindow(h, SW_HIDE);
    }
}

pub fn activate(h: HWND) -> bool {
    unsafe { SetForegroundWindow(h).as_bool() }
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn is_window(h: HWND) -> bool {
    unsafe { IsWindow(Some(h)).as_bool() }
}

pub fn pid_of(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid as *mut u32)) };
    pid
}

pub fn is_ours(h: HWND) -> bool {
    !h.0.is_null() && pid_of(h) == std::process::id()
}

/// Move (physical px, screen coords; Progman's client origin == screen origin on this setup).
pub fn move_to(h: HWND, x: i32, y: i32) {
    unsafe {
        let _ = SetWindowPos(h, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn rect(h: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut r);
    }
    r
}

pub fn scale(h: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(h) };
    if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 }
}

/// Monitor rect + work area (excludes taskbar) for the monitor the window is on.
pub fn monitor_of(h: HWND) -> (RECT, RECT) {
    unsafe {
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        (mi.rcMonitor, mi.rcWork)
    }
}

/// Default spot: right edge of the work area, vertically centred, 24 logical px margin.
pub fn default_position(h: HWND) -> (i32, i32) {
    let (_, work) = monitor_of(h);
    let r = rect(h);
    let (w, hh) = (r.right - r.left, r.bottom - r.top);
    let margin = (24.0 * scale(h)) as i32;
    (work.right - w - margin + shadow_inset(h), work.top + (work.bottom - work.top - hh) / 2)
}

/// The window is larger than the visible panel by a transparent margin that holds the CSS shadow.
pub fn shadow_inset(h: HWND) -> i32 {
    (40.0 * scale(h)) as i32
}

/// Clamp a window position so the PANEL (window minus its transparent margin) keeps at least
/// (1 - slack)^2 of its area inside the work area. Dragging uses slack 0.1 (>= 81 % visible,
/// CHECKLIST G2); restoring a saved position uses 0.0 (fully on screen, G4).
pub fn clamp_panel(h: HWND, x: i32, y: i32, slack: f64) -> (i32, i32) {
    let (_, work) = monitor_of(h);
    let r = rect(h);
    let m = shadow_inset(h);
    let (pw, ph) = ((r.right - r.left) - 2 * m, (r.bottom - r.top) - 2 * m);
    let (ox, oy) = ((pw as f64 * slack) as i32, (ph as f64 * slack) as i32);
    let (xmin, xmax) = (work.left - ox - m, work.right + ox - m - pw);
    let (ymin, ymax) = (work.top - oy - m, work.bottom + oy - m - ph);
    (x.clamp(xmin, xmax.max(xmin)), y.clamp(ymin, ymax.max(ymin)))
}

/// A saved position is restored exactly if a drag could have put it there; anything beyond that
/// (resolution change, edited config) is pulled fully on screen (CHECKLIST G3/G4).
pub fn restore_position(h: HWND, x: i32, y: i32) -> (i32, i32) {
    if clamp_panel(h, x, y, 0.1) == (x, y) { (x, y) } else { clamp_panel(h, x, y, 0.0) }
}

/// Drag-time clamp (see `clamp_panel`).
pub fn clamp_on_screen(h: HWND, x: i32, y: i32) -> (i32, i32) {
    clamp_panel(h, x, y, 0.1)
}
