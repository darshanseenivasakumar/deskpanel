//! "Can the user see the panel?" for a desktop-layer panel. Every visible top-level window sits
//! above Progman (our parent), so we walk the z-order from the top down to Progman and measure how
//! much of the panel those windows hide (sampled on a grid, so overlapping windows are a union).
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::desktop;

/// Share of the panel (0..1) hidden by other windows.
pub fn coverage(panel: HWND) -> f64 {
    let pr = desktop::rect(panel);
    let m = desktop::shadow_inset(panel);
    let (l, t, r, b) = (pr.left + m, pr.top + m, pr.right - m, pr.bottom - m);
    if r <= l || b <= t {
        return 0.0;
    }
    let progman = desktop::progman();
    let mut rects: Vec<RECT> = Vec::new();
    unsafe {
        let mut w = GetTopWindow(None).unwrap_or_default();
        let mut guard = 0;
        while !w.0.is_null() && w != progman && guard < 4000 {
            guard += 1;
            if occludes(w) {
                let mut fr = RECT::default();
                let ok = DwmGetWindowAttribute(w, DWMWA_EXTENDED_FRAME_BOUNDS, &mut fr as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32).is_ok();
                if !ok {
                    let _ = GetWindowRect(w, &mut fr);
                }
                if fr.right > l && fr.left < r && fr.bottom > t && fr.top < b {
                    rects.push(fr);
                }
            }
            w = GetWindow(w, GW_HWNDNEXT).unwrap_or_default();
        }
    }
    if rects.is_empty() {
        return 0.0;
    }
    // 24 x 32 sample grid over the panel.
    let (nx, ny) = (24, 32);
    let mut hit = 0;
    for j in 0..ny {
        let y = t + (b - t) * (2 * j + 1) / (2 * ny);
        for i in 0..nx {
            let x = l + (r - l) * (2 * i + 1) / (2 * nx);
            if rects.iter().any(|q| x >= q.left && x < q.right && y >= q.top && y < q.bottom) {
                hit += 1;
            }
        }
    }
    hit as f64 / (nx * ny) as f64
}

/// Does this top-level window actually hide what is beneath it?
fn occludes(w: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(w).as_bool() || IsIconic(w).as_bool() || desktop::is_ours(w) {
            return false;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(w, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
        if cloaked != 0 {
            return false;
        }
        let ex = GetWindowLongPtrW(w, GWL_EXSTYLE);
        // Click-through layered overlays (game bars, screen dimmers...) don't hide anything.
        if (ex & WS_EX_TRANSPARENT.0 as isize) != 0 && (ex & WS_EX_LAYERED.0 as isize) != 0 {
            return false;
        }
        !matches!(desktop::class_of(w).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW")
    }
}
