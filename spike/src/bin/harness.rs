// THROWAWAY: drives the real desktop with synthetic input and checks, via WindowFromPoint,
// whether the spike panel is visible / covered at each step. Usage: harness <mode>
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::mpsc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

struct H {
    out: File,
    t0: Instant,
    mode: String,
    panel: HWND,
}

impl H {
    fn log(&mut self, s: impl AsRef<str>) {
        let line = format!("{:>7} {}", self.t0.elapsed().as_millis(), s.as_ref());
        println!("{line}");
        let _ = writeln!(self.out, "{line}");
    }
}

fn class_of(h: HWND) -> String {
    let mut b = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
fn title_of(h: HWND) -> String {
    let mut b = [0u16; 128];
    let n = unsafe { GetWindowTextW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
fn d(h: HWND) -> String {
    format!("{:#x}/{}/{:?}", h.0 as usize, class_of(h), title_of(h))
}

fn is_panel_or_descendant(mut h: HWND, panel: HWND) -> bool {
    for _ in 0..32 {
        if h.0.is_null() {
            return false;
        }
        if h == panel {
            return true;
        }
        h = unsafe { GetAncestor(h, GA_PARENT) };
    }
    false
}

fn key_combo(keys: &[VIRTUAL_KEY]) {
    let mut inputs = vec![];
    for k in keys {
        inputs.push(kbd(*k, false));
    }
    for k in keys.iter().rev() {
        inputs.push(kbd(*k, true));
    }
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}
fn kbd(k: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: k, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    }
}
fn key_down(k: VIRTUAL_KEY) {
    unsafe { SendInput(&[kbd(k, false)], std::mem::size_of::<INPUT>() as i32) };
}
fn key_up(k: VIRTUAL_KEY) {
    unsafe { SendInput(&[kbd(k, true)], std::mem::size_of::<INPUT>() as i32) };
}
fn click(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
        sleep(Duration::from_millis(60));
        let m = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
        SendInput(&[m(MOUSEEVENTF_LEFTDOWN), m(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
    }
}

fn screenshot_png(path: &str) {
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp.into());
        let _ = BitBlt(mem, 0, 0, w, h, Some(screen), 0, 0, SRCCOPY | CAPTUREBLT);
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
        // half-res RGB PNG
        let (hw, hh) = ((w / 2) as usize, (h / 2) as usize);
        let mut rgb = Vec::with_capacity(hw * hh * 3);
        for y in 0..hh {
            for x in 0..hw {
                let i = ((y * 2) * w as usize + x * 2) * 4;
                rgb.extend_from_slice(&[buf[i + 2], buf[i + 1], buf[i]]);
            }
        }
        let f = BufWriter::new(File::create(path).unwrap());
        let mut enc = png::Encoder::new(f, hw as u32, hh as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    }
}

fn panel_rect(panel: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(panel, &mut r);
    }
    r
}

fn probe(h: &mut H, label: &str) {
    let r = panel_rect(h.panel);
    let cy = (r.top + r.bottom) / 2;
    let pts = [("overlapPt", r.left + (r.right - r.left) / 4), ("freePt", r.left + 3 * (r.right - r.left) / 4)];
    let fg = unsafe { GetForegroundWindow() };
    let vis = unsafe { IsWindowVisible(h.panel).as_bool() };
    let icon = unsafe { IsIconic(h.panel).as_bool() };
    let mut parts = vec![];
    for (name, x) in pts {
        let hit = unsafe { WindowFromPoint(POINT { x, y: cy }) };
        let mine = is_panel_or_descendant(hit, h.panel);
        let root = unsafe { GetAncestor(hit, GA_ROOT) };
        parts.push(format!("{name}={} (hit {} root {})", if mine { "PANEL" } else { "other" }, class_of(hit), d(root)));
    }
    let fg_is_panel = fg == h.panel;
    h.log(format!(
        "PROBE {label}: visible={vis} iconic={icon} fgIsPanel={fg_is_panel} fg={} | {}",
        d(fg),
        parts.join(" | ")
    ));
    zdump(h);
    let path = format!("../evidence/stage1/{}-{}.png", h.mode, label);
    screenshot_png(&path);
}

fn zdump(h: &mut H) {
    let mut out = vec![];
    unsafe {
        let mut w = GetTopWindow(None).unwrap_or_default();
        let mut i = 0;
        while !w.0.is_null() && out.len() < 14 && i < 2000 {
            i += 1;
            let mut r = RECT::default();
            let _ = GetWindowRect(w, &mut r);
            let cloaked = {
                let mut c = 0u32;
                let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
                    w,
                    windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
                    &mut c as *mut _ as *mut _,
                    4,
                );
                c != 0
            };
            if IsWindowVisible(w).as_bool() && !cloaked && r.right > r.left && r.bottom > r.top {
                let mark = if w == h.panel { "**PANEL**" } else { "" };
                let top = if (GetWindowLongPtrW(w, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize) != 0 { "T" } else { "-" };
                out.push(format!("{top}{}{mark}", class_of(w)));
            }
            w = GetWindow(w, GW_HWNDNEXT).unwrap_or_default();
        }
    }
    h.log(format!("  Z(top->down, visible, uncloaked): {}", out.join(" > ")));
}

unsafe extern "system" fn cover_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}

fn spawn_cover(x: i32, y: i32, w: i32, h: i32) -> (HWND, u32) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(cover_proc),
            hInstance: inst.into(),
            lpszClassName: w!("HarnessCover"),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("HarnessCover"),
            w!("Harness Cover (Chrome stand-in)"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(inst.into()),
            None,
        )
        .unwrap();
        tx.send((hwnd.0 as usize, windows::Win32::System::Threading::GetCurrentThreadId())).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    let (h, tid) = rx.recv().unwrap();
    (HWND(h as *mut _), tid)
}

fn ms(n: u64) {
    sleep(Duration::from_millis(n));
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let mode = std::env::args().nth(1).unwrap_or_else(|| "owner".into());
    std::fs::create_dir_all("../evidence/stage1").ok();
    std::fs::create_dir_all("logs").ok();
    let out = File::create(format!("logs/harness-{mode}.log")).unwrap();
    let mut panel = unsafe { FindWindowW(PCWSTR::null(), w!("deskpanel-spike-main")).unwrap_or_default() };
    if panel.0.is_null() {
        let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default() };
        panel = unsafe { FindWindowExW(Some(progman), None, PCWSTR::null(), w!("deskpanel-spike-main")).unwrap_or_default() };
    }
    let mut h = H { out, t0: Instant::now(), mode: mode.clone(), panel };
    h.log(format!("harness mode={mode} panel={}", d(panel)));
    if panel.0.is_null() {
        h.log("PANEL NOT FOUND");
        return;
    }

    probe(&mut h, "00-initial");
    key_combo(&[VK_LWIN, VK_D]);
    ms(1200);
    probe(&mut h, "01-after-winD");

    // Cover window over the LEFT half of the panel, then activate it by clicking its caption.
    let r = panel_rect(panel);
    let pw = r.right - r.left;
    let (cx, cy, cw, ch) = (r.left - 600, r.top - 60, 600 + pw / 2, (r.bottom - r.top) + 120);
    let (cover, cover_tid) = spawn_cover(cx, cy, cw, ch);
    ms(700);
    click(cx + 120, cy + 16);
    ms(600);
    probe(&mut h, "02-cover-active");

    // Click the uncovered half of the panel: must NOT raise it or take focus.
    click(r.left + 3 * pw / 4, (r.top + r.bottom) / 2);
    ms(700);
    probe(&mut h, "03-after-click-panel");

    key_combo(&[VK_LWIN]);
    ms(1200);
    probe(&mut h, "04-start-open");
    click(cx + 120, cy + 16);
    ms(700);

    key_down(VK_MENU);
    ms(80);
    key_combo(&[VK_TAB]);
    ms(1000);
    probe(&mut h, "05-alttab-open");
    key_combo(&[VK_ESCAPE]); // consumed by the switcher while Alt is held: cancels the switch
    ms(150);
    key_up(VK_MENU);
    ms(800);
    probe(&mut h, "05b-after-alttab");

    key_combo(&[VK_LWIN, VK_A]);
    ms(1300);
    probe(&mut h, "06-quicksettings");
    click(cx + 120, cy + 16);
    ms(700);

    key_combo(&[VK_LWIN, VK_N]);
    ms(1300);
    probe(&mut h, "07-notifcenter");
    click(cx + 120, cy + 16);
    ms(700);

    key_combo(&[VK_LWIN, VK_T]);
    ms(900);
    probe(&mut h, "08-taskbar-focus");
    click(cx + 120, cy + 16);
    ms(700);

    // Activate cover, then minimize it: where does foreground land?
    click(cx + 120, cy + 16);
    ms(500);
    unsafe {
        let _ = ShowWindow(cover, SW_MINIMIZE);
    }
    ms(1000);
    probe(&mut h, "09-cover-minimized");
    unsafe {
        let _ = ShowWindow(cover, SW_RESTORE);
    }
    ms(500);
    click(cx + 120, cy + 16);
    ms(500);
    probe(&mut h, "10-cover-restored");

    key_combo(&[VK_LWIN, VK_D]);
    ms(1200);
    probe(&mut h, "11-winD-with-cover");
    key_combo(&[VK_LWIN, VK_D]);
    ms(1200);
    probe(&mut h, "12-winD-again-restore");

    unsafe {
        let _ = PostMessageW(Some(cover), WM_CLOSE, WPARAM(0), LPARAM(0));
        let _ = windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(cover_tid, WM_QUIT, WPARAM(0), LPARAM(0));
    }
    ms(300);
    h.log("done");
}
