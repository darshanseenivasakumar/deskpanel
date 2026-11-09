// THROWAWAY: child-mode interaction test. Click-through without focus theft, pop/drop via the real
// Ctrl+Alt+Space hotkey, keyboard reaching the popped webview, Esc semantics. Needs `spike child` running.
use std::fs::File;
use std::io::Write;
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

fn class_of(h: HWND) -> String {
    let mut b = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
fn under(mut h: HWND, target: HWND) -> bool {
    for _ in 0..32 {
        if h.0.is_null() {
            return false;
        }
        if h == target {
            return true;
        }
        h = unsafe { GetAncestor(h, GA_PARENT) };
    }
    false
}
fn kbd(k: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: k, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() },
        },
    }
}
fn combo(keys: &[VIRTUAL_KEY]) {
    let mut v: Vec<INPUT> = keys.iter().map(|k| kbd(*k, false)).collect();
    v.extend(keys.iter().rev().map(|k| kbd(*k, true)));
    unsafe { SendInput(&v, std::mem::size_of::<INPUT>() as i32) };
}
fn click(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
        sleep(Duration::from_millis(60));
        let m = |f| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: f, ..Default::default() } } };
        SendInput(&[m(MOUSEEVENTF_LEFTDOWN), m(MOUSEEVENTF_LEFTUP)], std::mem::size_of::<INPUT>() as i32);
    }
}
fn ms(n: u64) {
    sleep(Duration::from_millis(n));
}

unsafe extern "system" fn cover_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if m == WM_KEYDOWN {
        println!("      COVER received WM_KEYDOWN vk={:#x}", wp.0);
        let _ = std::fs::OpenOptions::new().append(true).create(true).open("logs/cover-keys.log")
            .and_then(|mut f| writeln!(f, "COVER WM_KEYDOWN vk={:#x}", wp.0));
    }
    if m == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}
fn spawn_cover(x: i32, y: i32, w: i32, h: i32) -> HWND {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(cover_proc),
            hInstance: inst.into(),
            lpszClassName: w!("HarnessCover2"),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), w!("HarnessCover2"), w!("Harness Cover 2"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE, x, y, w, h, None, None, Some(inst.into()), None).unwrap();
        tx.send(hwnd.0 as usize).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    HWND(rx.recv().unwrap() as *mut _)
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let _ = std::fs::remove_file("logs/cover-keys.log");
    let mut out = File::create("logs/harness2.log").unwrap();
    let t0 = Instant::now();
    let mut log = |s: String| {
        let l = format!("{:>7} {s}", t0.elapsed().as_millis());
        println!("{l}");
        let _ = writeln!(out, "{l}");
    };
    let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default() };
    let mut panel = unsafe { FindWindowExW(Some(progman), None, PCWSTR::null(), w!("deskpanel-spike-main")).unwrap_or_default() };
    if panel.0.is_null() {
        panel = unsafe { FindWindowW(PCWSTR::null(), w!("deskpanel-spike-main")).unwrap_or_default() };
    }
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(panel, &mut r);
    }
    let pw = r.right - r.left;
    let cy = (r.top + r.bottom) / 2;
    let overlap = (r.left + pw / 4, cy);
    let free = (r.left + 3 * pw / 4, cy);
    let button = ((r.left + r.right) / 2, cy + 50); // "Click me" button (from screenshot: ~46px below centre at 125%)
    let (cx, cyy, cw, ch) = (r.left - 600, r.top - 60, 600 + pw / 2, (r.bottom - r.top) + 120);
    let caption = (cx + 120, cyy + 16);

    let state = |label: &str, panel: HWND| -> String {
        let fg = unsafe { GetForegroundWindow() };
        let o = unsafe { WindowFromPoint(POINT { x: overlap.0, y: overlap.1 }) };
        let f = unsafe { WindowFromPoint(POINT { x: free.0, y: free.1 }) };
        format!("CHECK {label}: fg={} fgIsPanel={} overlapPt={} freePt={}",
            class_of(fg), fg == panel,
            if under(o, panel) { "PANEL" } else { "other" },
            if under(f, panel) { "PANEL" } else { "other" })
    };

    log(format!("panel rect {:?}", (r.left, r.top, r.right, r.bottom)));

    // T1: cover app active over left half; click the panel's button on the free half.
    let cover = spawn_cover(cx, cyy, cw, ch);
    ms(800);
    click(caption.0, caption.1);
    ms(600);
    log(state("T1a cover-active", panel));
    click(button.0, button.1);
    ms(700);
    log(state("T1b after-click-button (expect fg=cover, overlap=other)", panel));
    click(button.0, button.1);
    ms(700);
    log(state("T1c after-2nd-click", panel));

    // T2: pop with the real hotkey.
    combo(&[VK_CONTROL, VK_MENU, VK_F12]);
    ms(900);
    log(state("T2 popped (expect fgIsPanel=true overlap=PANEL)", panel));
    combo(&[VK_X]);
    ms(500);
    log("T2b typed 'x' (see app log for JS keydown)".into());

    // T3: click into the cover app; panel must stay on top; Esc must go to the cover.
    click(caption.0, caption.1);
    ms(600);
    log(state("T3 clicked cover (expect fg=cover, overlap=PANEL)", panel));
    if unsafe { GetForegroundWindow() } == cover {
        combo(&[VK_ESCAPE]);
        ms(500);
        log(state("T3b Esc in cover (expect still overlap=PANEL)", panel));
    } else {
        log("T3b SKIPPED Esc: cover not foreground".into());
    }

    // T4: hotkey drops it back.
    combo(&[VK_CONTROL, VK_MENU, VK_F12]);
    ms(900);
    log(state("T4 dropped via hotkey (expect overlap=other)", panel));

    // T5: pop again, Esc while panel focused drops it and returns focus to cover.
    combo(&[VK_CONTROL, VK_MENU, VK_F12]);
    ms(900);
    log(state("T5a popped", panel));
    if unsafe { GetForegroundWindow() } == panel {
        combo(&[VK_ESCAPE]);
        ms(800);
        log(state("T5b Esc in panel (expect dropped, fg=cover)", panel));
    } else {
        log("T5b SKIPPED Esc: panel not foreground".into());
    }

    // T6: Win+D still works after a pop/drop cycle (re-parented correctly).
    combo(&[VK_LWIN, VK_D]);
    ms(1200);
    log(state("T6 win+D (expect overlap=PANEL free=PANEL)", panel));
    combo(&[VK_LWIN, VK_D]);
    ms(1200);
    log(state("T6b win+D restore (expect overlap=other)", panel));

    unsafe {
        let _ = PostMessageW(Some(cover), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    ms(300);
    log("done".into());
}
