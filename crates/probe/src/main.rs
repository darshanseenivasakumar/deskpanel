//! dpprobe: test helper for DeskPanel evidence.
//!   dpprobe hit                     -> panel visibility via WindowFromPoint at its left/right quarter
//!   dpprobe shot <file.png> [panel] -> full-screen (or panel-rect) screenshot
//!   dpprobe keys 5B,44              -> press VK codes together (hex), release in reverse
//!   dpprobe click <x> <y>           -> left click at physical px
//!   dpprobe fg                      -> describe the foreground window
//!   dpprobe rect                    -> panel window rect
use std::io::BufWriter;
use std::thread::sleep;
use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn class_of(h: HWND) -> String {
    let mut b = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
fn title_of(h: HWND) -> String {
    let mut b = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}

fn panel() -> HWND {
    unsafe {
        let pm = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default();
        let h = FindWindowExW(Some(pm), None, PCWSTR::null(), w!("DeskPanel")).unwrap_or_default();
        if !h.0.is_null() {
            return h;
        }
        FindWindowW(PCWSTR::null(), w!("DeskPanel")).unwrap_or_default()
    }
}

fn rect(h: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut r);
    }
    r
}

fn under(mut h: HWND, t: HWND) -> bool {
    for _ in 0..32 {
        if h.0.is_null() {
            return false;
        }
        if h == t {
            return true;
        }
        h = unsafe { GetAncestor(h, GA_PARENT) };
    }
    false
}

fn hit() {
    let p = panel();
    if p.0.is_null() {
        println!("panel=NOT_FOUND");
        return;
    }
    let r = rect(p);
    let cy = (r.top + r.bottom) / 2;
    let fg = unsafe { GetForegroundWindow() };
    let mut s = format!(
        "panel={:#x} visible={} parent={} fg={}:{:?} fgIsPanel={}",
        p.0 as usize,
        unsafe { IsWindowVisible(p).as_bool() },
        class_of(unsafe { GetAncestor(p, GA_PARENT) }),
        class_of(fg),
        title_of(fg),
        fg == p
    );
    for (name, x) in [("left", r.left + (r.right - r.left) / 4), ("right", r.left + 3 * (r.right - r.left) / 4)] {
        let h = unsafe { WindowFromPoint(POINT { x, y: cy }) };
        let root = unsafe { GetAncestor(h, GA_ROOT) };
        s += &format!(" {name}={}", if under(h, p) { "PANEL".to_string() } else { format!("other:{}", class_of(root)) });
    }
    println!("{s}");
}

fn shot(path: &str, crop: Option<RECT>) {
    unsafe {
        let (x, y, w, h) = match crop {
            Some(r) => (r.left, r.top, r.right - r.left, r.bottom - r.top),
            None => (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)),
        };
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let _ = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for px in buf.chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        let f = BufWriter::new(std::fs::File::create(path).expect("create png"));
        let mut enc = png::Encoder::new(f, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        println!("saved {path} {w}x{h}");
    }
}

/// Capture the panel window's own pixels via PrintWindow, even if other windows cover it.
/// Transparent areas come out black (no desktop behind), so this checks rendering, not composition.
fn pshot(path: &str) {
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    unsafe {
        let p = panel();
        let r = rect(p);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let ok = PrintWindow(p, mem, PRINT_WINDOW_FLAGS(2 /* PW_RENDERFULLCONTENT */)).as_bool();
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let rgb: Vec<u8> = buf.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
        let f = BufWriter::new(std::fs::File::create(path).expect("create png"));
        let mut enc = png::Encoder::new(f, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        println!("printwindow ok={ok} saved {path} {w}x{h}");
    }
}

fn grab(x: i32, y: i32, w: i32, h: i32) -> Vec<u8> {
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let _ = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        buf
    }
}

/// D2: watch the panel's screen region while the app starts. Counts pixels that became
/// near-white (all channels > 235) relative to the first captured frame. Prints the worst frame.
fn flashwatch(ms: u64, save: Option<&str>) {
    let (x, y, w, h) = (1300, 120, 620, 840); // right column where the panel appears (1920x1080)
    let base = grab(x, y, w, h);
    let white = |p: &[u8]| p[0] > 235 && p[1] > 235 && p[2] > 235;
    let t0 = std::time::Instant::now();
    let (mut frames, mut worst, mut worst_buf) = (0u32, 0usize, Vec::new());
    while t0.elapsed().as_millis() < ms as u128 {
        let cur = grab(x, y, w, h);
        frames += 1;
        let n = cur.chunks_exact(4).zip(base.chunks_exact(4)).filter(|(c, b)| white(c) && !white(b)).count();
        if n > worst {
            worst = n;
            worst_buf = cur;
        }
    }
    let total = (w * h) as f64;
    println!("flashwatch frames={frames} fps~{:.0} worstNewWhitePx={worst} ({:.3}%)", frames as f64 / (ms as f64 / 1000.0), 100.0 * worst as f64 / total);
    if let (Some(path), false) = (save, worst_buf.is_empty()) {
        let rgb: Vec<u8> = worst_buf.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
        let f = BufWriter::new(std::fs::File::create(path).unwrap());
        let mut enc = png::Encoder::new(f, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    }
}

unsafe extern "system" fn fs_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

static KEYLOG: std::sync::OnceLock<String> = std::sync::OnceLock::new();
unsafe extern "system" fn cover_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_KEYDOWN {
        if let Some(p) = KEYLOG.get() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "WM_KEYDOWN vk={:#x}", wp.0);
            }
        }
    }
    if m == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

fn cover_window(x: i32, y: i32, w: i32, h: i32, secs: u64, keylog: String, color: Option<(u32, u32, u32)>) {
    let _ = KEYLOG.set(keylog);
    unsafe {
        let inst = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(cover_proc),
            hInstance: inst.into(),
            lpszClassName: w!("DpCover"),
            hbrBackground: match color {
                Some((r, g, b)) => CreateSolidBrush(COLORREF(r | (g << 8) | (b << 16))),
                None => HBRUSH(GetStockObject(WHITE_BRUSH).0),
            },
            ..Default::default()
        };
        RegisterClassW(&wc);
        // With a colour: a borderless backdrop (README screenshots). Otherwise a normal test window.
        let style = if color.is_some() { WS_POPUP | WS_VISIBLE } else { WS_OVERLAPPEDWINDOW | WS_VISIBLE };
        let hw = CreateWindowExW(WINDOW_EX_STYLE(0), w!("DpCover"), w!("DeskPanel test window"), style, x, y, w, h, None, None, Some(inst.into()), None).unwrap();
        // A process started hidden applies SW_HIDE to its first shown window: show it explicitly.
        let _ = ShowWindow(hw, SW_SHOW);
        println!("cover {:#x}", hw.0 as usize);
        let t0 = std::time::Instant::now();
        let mut msg = MSG::default();
        while t0.elapsed().as_secs() < secs {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            sleep(Duration::from_millis(10));
        }
        let _ = DestroyWindow(hw);
    }
}

fn fullscreen_window(secs: u64) {
    unsafe {
        let inst = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(fs_proc),
            hInstance: inst.into(),
            lpszClassName: w!("DpFullscreen"),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let h = CreateWindowExW(WINDOW_EX_STYLE(0), w!("DpFullscreen"), w!("Fullscreen test"), WS_POPUP | WS_VISIBLE, 0, 0, sw, sh, None, None, Some(inst.into()), None).unwrap();
        // Activate it the way a user would: a click on it.
        let _ = SetCursorPos(sw / 2, sh / 2);
        let m = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
        SendInput(&[m(MOUSEEVENTF_LEFTDOWN), m(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
        println!("fullscreen window up for {secs}s");
        let t0 = std::time::Instant::now();
        let mut msg = MSG::default();
        while t0.elapsed().as_secs() < secs {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            sleep(Duration::from_millis(20));
        }
        let _ = DestroyWindow(h);
    }
}

fn kbd(k: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(k), dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(|s| s.as_str()) {
        Some("hit") => hit(),
        Some("rect") => {
            let r = rect(panel());
            println!("{} {} {} {}", r.left, r.top, r.right, r.bottom);
        }
        Some("fg") => {
            let f = unsafe { GetForegroundWindow() };
            println!("{:#x} {} {:?}", f.0 as usize, class_of(f), title_of(f));
        }
        Some("shot") => {
            let crop = if a.get(2).map(|s| s == "panel").unwrap_or(false) { Some(rect(panel())) } else { None };
            shot(&a[1], crop);
        }
        Some("pshot") => pshot(&a[1]),
        Some("flashwatch") => flashwatch(a[1].parse().unwrap(), a.get(2).map(|s| s.as_str())),
        Some("keys") => {
            let ks: Vec<u16> = a[1].split(',').map(|s| u16::from_str_radix(s.trim(), 16).unwrap()).collect();
            // One event at a time with small gaps: a single batched SendInput sometimes lost the
            // second key of Win+D (Start opened instead).
            let mut v: Vec<INPUT> = ks.iter().map(|k| kbd(*k, false)).collect();
            v.extend(ks.iter().rev().map(|k| kbd(*k, true)));
            for i in v {
                unsafe { SendInput(&[i], std::mem::size_of::<INPUT>() as i32) };
                sleep(Duration::from_millis(35));
            }
        }
        Some("wheel") => unsafe {
            // dpprobe wheel <x> <y> <notches>  (positive = up)
            let (x, y, n): (i32, i32, i32) = (a[1].parse().unwrap(), a[2].parse().unwrap(), a[3].parse().unwrap());
            let _ = SetCursorPos(x, y);
            sleep(Duration::from_millis(60));
            for _ in 0..n.abs() {
                let i = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: MOUSEEVENTF_WHEEL, mouseData: (if n > 0 { 120i32 } else { -120 }) as u32, ..Default::default() } },
                };
                SendInput(&[i], std::mem::size_of::<INPUT>() as i32);
                sleep(Duration::from_millis(25));
            }
        },
        Some("drag") => unsafe {
            // dpprobe drag <x1> <y1> <x2> <y2> <steps> [shot_prefix]: hold left button, move in steps.
            // With shot_prefix: at the midpoint saves <prefix>-0ms.png right after a move step and
            // <prefix>-300ms.png after holding still, to measure how far the glass lags the window.
            let v: Vec<i32> = a[1..6].iter().map(|s| s.parse().unwrap()).collect();
            let (x1, y1, x2, y2, steps) = (v[0], v[1], v[2], v[3], v[4].max(1));
            let m = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
            let _ = SetCursorPos(x1, y1);
            sleep(Duration::from_millis(80));
            SendInput(&[m(MOUSEEVENTF_LEFTDOWN)], std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(120));
            for i in 1..=steps {
                let _ = SetCursorPos(x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps);
                if i == steps / 2 {
                    if let Some(pref) = a.get(6) {
                        let r0 = rect(panel());
                        shot(&format!("{pref}-0ms.png"), None);
                        println!("rect-0ms {} {} {} {}", r0.left, r0.top, r0.right, r0.bottom);
                        sleep(Duration::from_millis(300));
                        let r1 = rect(panel());
                        shot(&format!("{pref}-300ms.png"), None);
                        println!("rect-300ms {} {} {} {}", r1.left, r1.top, r1.right, r1.bottom);
                    }
                }
                sleep(Duration::from_millis(16));
            }
            sleep(Duration::from_millis(100));
            SendInput(&[m(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
        },
        Some("pill") => unsafe {
            let h = FindWindowW(PCWSTR::null(), w!("DeskPanel Pill")).unwrap_or_default();
            let r = rect(h);
            // Also check that the area just under the pill lets clicks through (region clip).
            let below = WindowFromPoint(POINT { x: (r.left + r.right) / 2, y: r.top + 70 });
            let pt = WindowFromPoint(POINT { x: (r.left + r.right) / 2, y: r.top + 25 });
            println!(
                "pill visible={} rect={},{},{},{} hitOnPill={} hitBelowPill={}",
                IsWindowVisible(h).as_bool(), r.left, r.top, r.right, r.bottom,
                if under(pt, h) { "PILL" } else { "other" },
                if under(below, h) { "PILL(blocks clicks!)" } else { "other(click-through ok)" }
            );
        },
        Some("watchfg") => unsafe {
            // dpprobe watchfg <ms>: print every foreground-window change (and keyboard-focus owner class)
            let ms: u128 = a[1].parse().unwrap();
            let t0 = std::time::Instant::now();
            let mut last = HWND::default();
            while t0.elapsed().as_millis() < ms {
                let f = GetForegroundWindow();
                if f != last {
                    let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
                    let _ = GetGUIThreadInfo(0, &mut gi);
                    println!("{:>5} ms fg={:#x} {} {:?} focus={}", t0.elapsed().as_millis(), f.0 as usize, class_of(f), title_of(f), class_of(gi.hwndFocus));
                    last = f;
                }
                sleep(Duration::from_millis(2));
            }
        },
        Some("find") => unsafe {
            // dpprobe find <exact title>: hwnd (hex) or 0
            let t: Vec<u16> = a[1].encode_utf16().chain(std::iter::once(0)).collect();
            let h = FindWindowW(PCWSTR::null(), PCWSTR(t.as_ptr())).unwrap_or_default();
            println!("{:#x}", h.0 as usize);
        },
        Some("activate") => unsafe {
            // dpprobe activate <hwnd hex>: bring an app window to the front like a user switch would
            let h = HWND(usize::from_str_radix(a[1].trim_start_matches("0x"), 16).unwrap() as *mut _);
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            #[link(name = "user32")]
            extern "system" {
                fn SwitchToThisWindow(h: HWND, alt_tab: i32);
            }
            SwitchToThisWindow(h, 1);
            sleep(Duration::from_millis(250));
            let f = GetForegroundWindow();
            println!("fg={:#x} ok={}", f.0 as usize, f == h);
        },
        Some("onpanel") => unsafe {
            // dpprobe onpanel <x> <y>: is the panel (or its webview) the window under this point?
            let (x, y): (i32, i32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
            let h = WindowFromPoint(POINT { x, y });
            println!("{}", if under(h, panel()) { "yes" } else { "no" });
        },
        Some("cover") => {
            // dpprobe cover <x> <y> <w> <h> <seconds> <keylog>: a normal app window that logs keys.
            let v: Vec<i32> = a[1..6].iter().map(|s| s.parse().unwrap()).collect();
            // optional: r g b -> borderless solid-colour window (README screenshot backdrop)
            let color = if a.len() >= 10 { Some((a[7].parse::<u32>().unwrap(), a[8].parse::<u32>().unwrap(), a[9].parse::<u32>().unwrap())) } else { None };
            cover_window(v[0], v[1], v[2], v[3], v[4] as u64, a[6].clone(), color);
        }
        Some("fullscreen") => {
            // dpprobe fullscreen <seconds>: a borderless window covering the monitor, activated.
            let secs: u64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);
            fullscreen_window(secs);
        }
        Some("move") => unsafe {
            // dpprobe move <x> <y>: real mouse-move input in a few steps (fires hover/pointerenter).
            let (x, y): (i32, i32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
            let mut cur = POINT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cur);
            let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            for i in 1..=12 {
                let px = cur.x + (x - cur.x) * i / 12;
                let py = cur.y + (y - cur.y) * i / 12;
                let mi = MOUSEINPUT {
                    dx: px * 65535 / (sw - 1),
                    dy: py * 65535 / (sh - 1),
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    ..Default::default()
                };
                SendInput(&[INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } }], std::mem::size_of::<INPUT>() as i32);
                sleep(Duration::from_millis(15));
            }
        },
        Some("rclick") => unsafe {
            let (x, y): (i32, i32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
            let _ = SetCursorPos(x, y);
            sleep(Duration::from_millis(60));
            let m = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
            SendInput(&[m(MOUSEEVENTF_RIGHTDOWN), m(MOUSEEVENTF_RIGHTUP)], std::mem::size_of::<INPUT>() as i32);
        },
        Some("click") => unsafe {
            let (x, y): (i32, i32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
            let _ = SetCursorPos(x, y);
            sleep(Duration::from_millis(60));
            let m = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
            SendInput(&[m(MOUSEEVENTF_LEFTDOWN), m(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
        },
        _ => eprintln!("usage: dpprobe hit|rect|fg|shot <png> [panel]|keys <hex,..>|click x y"),
    }
}
