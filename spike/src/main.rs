// THROWAWAY Stage 1 spike: a Tauri panel window placed with one of several z-order strategies,
// logging every foreground/minimize event the shell reports. Usage: spike.exe <bottom|owner|child>
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_F12};
use std::time::Instant;

use tauri::Manager;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
static T0: OnceLock<Instant> = OnceLock::new();
static MODE: OnceLock<String> = OnceLock::new();
static PANEL: AtomicUsize = AtomicUsize::new(0);
static PROGMAN: AtomicUsize = AtomicUsize::new(0);
static PREV_FG: AtomicUsize = AtomicUsize::new(0);
static POPPED: AtomicBool = AtomicBool::new(false);
const HOTKEY_ID: i32 = 1;

fn hwnd_of(a: &AtomicUsize) -> HWND {
    HWND(a.load(Ordering::SeqCst) as *mut core::ffi::c_void)
}

/// child-of-Progman -> top-level topmost window that takes keyboard focus.
unsafe fn pop() {
    unsafe {
        let h = hwnd_of(&PANEL);
        let fg = GetForegroundWindow();
        PREV_FG.store(fg.0 as usize, Ordering::SeqCst);
        let st = GetWindowLongPtrW(h, GWL_STYLE);
        SetWindowLongPtrW(h, GWL_STYLE, (st & !(WS_CHILD.0 as isize)) | WS_POPUP.0 as isize);
        let _ = SetParent(h, None);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) & !(WS_EX_NOACTIVATE.0 as isize);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_FRAMECHANGED);
        let ok = SetForegroundWindow(h).as_bool();
        POPPED.store(true, Ordering::SeqCst);
        log(format!("POP prevFg={} setFg={ok} nowFg={}", describe(fg), describe(GetForegroundWindow())));
    }
}

/// top-level -> back to child of Progman, focus returned to whoever had it before the pop.
unsafe fn drop_back(reason: &str) {
    unsafe {
        let h = hwnd_of(&PANEL);
        let _ = SetWindowPos(h, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) | WS_EX_NOACTIVATE.0 as isize;
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex);
        let st = GetWindowLongPtrW(h, GWL_STYLE);
        SetWindowLongPtrW(h, GWL_STYLE, (st & !(WS_POPUP.0 as isize)) | WS_CHILD.0 as isize);
        let _ = SetParent(h, Some(hwnd_of(&PROGMAN)));
        let _ = SetWindowPos(h, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        let prev = hwnd_of(&PREV_FG);
        let was_fg = GetForegroundWindow() == h;
        if was_fg && !prev.0.is_null() {
            let _ = SetForegroundWindow(prev);
        }
        POPPED.store(false, Ordering::SeqCst);
        log(format!("DROP reason={reason} restoredFgTo={} nowFg={}", describe(prev), describe(GetForegroundWindow())));
    }
}

#[tauri::command]
fn key(k: String) {
    let fg = unsafe { GetForegroundWindow() };
    log(format!("JS keydown {k:?} popped={} fg={}", POPPED.load(Ordering::SeqCst), describe(fg)));
    if k == "Escape" && POPPED.load(Ordering::SeqCst) {
        unsafe { drop_back("esc") };
    }
}

#[tauri::command]
fn clicked(n: u32) {
    log(format!("JS click #{n} fg={}", describe(unsafe { GetForegroundWindow() })));
}

fn log(s: impl AsRef<str>) {
    let t = T0.get().map(|t| t.elapsed().as_millis()).unwrap_or(0);
    let line = format!("{t:>7} {}", s.as_ref());
    println!("{line}");
    if let Some(f) = LOG.get() {
        let _ = writeln!(f.lock().unwrap(), "{line}");
    }
}

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
fn exe_of(h: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid as *mut u32));
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return format!("pid{pid}");
        };
        let mut b = [0u16; 512];
        let mut n = b.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        if ok {
            let s = String::from_utf16_lossy(&b[..n as usize]);
            s.rsplit('\\').next().unwrap_or("").to_string()
        } else {
            format!("pid{pid}")
        }
    }
}
fn describe(h: HWND) -> String {
    format!("hwnd={:#x} class={:?} exe={} title={:?}", h.0 as usize, class_of(h), exe_of(h), title_of(h))
}

unsafe extern "system" fn on_event(_h: HWINEVENTHOOK, ev: u32, hwnd: HWND, obj: i32, _c: i32, _t: u32, _ms: u32) {
    if obj != OBJID_WINDOW.0 {
        return;
    }
    let name = match ev {
        EVENT_SYSTEM_FOREGROUND => "FOREGROUND",
        EVENT_SYSTEM_MINIMIZESTART => "MINSTART",
        EVENT_SYSTEM_MINIMIZEEND => "MINEND",
        _ => "OTHER",
    };
    log(format!("EV {name} {}", describe(hwnd)));
}

unsafe extern "system" fn subclass(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => {
            log("MSG WM_MOUSEACTIVATE -> MA_NOACTIVATE");
            return LRESULT(MA_NOACTIVATE as isize);
        }
        WM_ACTIVATE => log(format!("MSG WM_ACTIVATE wparam={}", wp.0 & 0xffff)),
        WM_HOTKEY if wp.0 as i32 == HOTKEY_ID => {
            log("MSG WM_HOTKEY ctrl+alt+space");
            unsafe {
                if POPPED.load(Ordering::SeqCst) { drop_back("hotkey") } else { pop() }
            }
            return LRESULT(0);
        }
        WM_WINDOWPOSCHANGING => {
            let pos = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
            if !pos.flags.contains(SWP_NOZORDER) {
                log(format!(
                    "MSG WPCHANGING z-change insertAfter={:#x} flags={:#x}",
                    pos.hwndInsertAfter.0 as usize, pos.flags.0
                ));
                if MODE.get().map(|m| m == "bottom").unwrap_or(false) {
                    pos.hwndInsertAfter = HWND_BOTTOM;
                }
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(h, msg, wp, lp) }
}

unsafe fn apply_mode(hwnd: HWND, mode: &str) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex = (ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let progman = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default();
        log(format!("progman {}", describe(progman)));
        match mode {
            "owner" => {
                SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, progman.0 as isize);
            }
            "child" => {
                let st = GetWindowLongPtrW(hwnd, GWL_STYLE);
                let st = (st & !(WS_POPUP.0 as isize)) | WS_CHILD.0 as isize;
                SetWindowLongPtrW(hwnd, GWL_STYLE, st);
                let r = SetParent(hwnd, Some(progman));
                log(format!("SetParent -> {:?}", r.map(|h| h.0 as usize)));
            }
            _ => {}
        }
    }
}

unsafe fn place_and_show(hwnd: HWND, mode: &str) {
    unsafe {
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let x = work.right - w - 30;
        let y = work.top + (work.bottom - work.top - h) / 2;
        let insert = match mode {
            "bottom" => HWND_BOTTOM,
            "child" => HWND_TOP,
            _ => HWND_BOTTOM,
        };
        let _ = SetWindowPos(hwnd, Some(insert), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(insert), 0, 0, 0, 0, SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE);
        log(format!("placed at {x},{y} size {w}x{h} work={:?}", (work.left, work.top, work.right, work.bottom)));
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "owner".into());
    T0.set(Instant::now()).ok();
    std::fs::create_dir_all("logs").ok();
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(format!("logs/app-{mode}.log"))
        .unwrap();
    LOG.set(Mutex::new(f)).ok();
    MODE.set(mode.clone()).ok();
    log(format!("start mode={mode} pid={}", std::process::id()));

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![key, clicked])
        .setup(move |app| {
            let main = app.get_webview_window("main").unwrap();
            let hwnd = HWND(main.hwnd()?.0 as *mut core::ffi::c_void);
            log(format!("panel {}", describe(hwnd)));
            PANEL.store(hwnd.0 as usize, Ordering::SeqCst);
            unsafe {
                let pm = FindWindowW(w!("Progman"), PCWSTR::null()).unwrap_or_default();
                PROGMAN.store(pm.0 as usize, Ordering::SeqCst);
                let hk = RegisterHotKey(Some(hwnd), HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_F12.0 as u32);
                log(format!("RegisterHotKey ctrl+alt+space -> {hk:?}"));
                let _ = SetWindowSubclass(hwnd, Some(subclass), 1, 0);
                apply_mode(hwnd, &mode);
                place_and_show(hwnd, &mode);
                for ev in [EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND] {
                    SetWinEventHook(ev, ev, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT);
                }
            }
            // Pill: topmost, never activated. Only here for the 2-webview RAM probe.
            let pill = app.get_webview_window("pill").unwrap();
            let ph = HWND(pill.hwnd()?.0 as *mut core::ffi::c_void);
            unsafe {
                let ex = GetWindowLongPtrW(ph, GWL_EXSTYLE) | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
                SetWindowLongPtrW(ph, GWL_EXSTYLE, ex);
                let mut r = RECT::default();
                let _ = GetWindowRect(ph, &mut r);
                let sw = GetSystemMetrics(SM_CXSCREEN);
                let x = (sw - (r.right - r.left)) / 2;
                let _ = SetWindowPos(ph, Some(HWND_TOPMOST), x, 10, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running spike");
}
