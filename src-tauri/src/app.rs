//! Glue: OS events / webview commands -> `Input` -> state machine -> `Effect`s executed here.
//! Everything that touches the machine or window styles runs on the main (UI) thread.
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use deskpanel_core::state::{Anim, Effect, Input, Machine, Panel, Pill, WindowId};
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::HWND;

use crate::config::Config;
use crate::desktop::{self, hwnd};
use crate::wallpaper::{self, Glass};

pub struct Ctx {
    pub app: AppHandle,
    /// HWND of the panel window. Atomic because the window is re-created if Explorer restarts
    /// (it is a child of Progman and Windows destroys it with its parent).
    panel_h: AtomicUsize,
    pub pill: usize,
    machine: Mutex<Machine>,
    pub cfg: Mutex<Config>,
    ready: AtomicBool,
    pending_show: AtomicBool,
    pending_anim: Mutex<Option<Anim>>,
    glass: Mutex<Option<Glass>>,
    anim_seq: AtomicU64,
    /// Set by Effect::CaptureSnapshot; the capture runs after the whole effect batch, when the
    /// window is top-level (needed for capture exclusion) and the animation is already running.
    snapshot_pending: AtomicBool,
}

/// If the webview never reports an animation finished (hung, throttled, reloaded), complete it
/// from Rust so the state machine can never get stuck mid-animation with input queued.
const ANIM_WATCHDOG_MS: u64 = 2000;

fn emit_anim(a: Anim) {
    let c = ctx();
    let seq = c.anim_seq.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = c.app.emit_to(panel_label().as_str(), "anim", anim_name(a));
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ANIM_WATCHDOG_MS));
        let _ = ctx().app.run_on_main_thread(move || {
            let c = ctx();
            let stuck = c.anim_seq.load(Ordering::SeqCst) == seq && c.machine.lock().unwrap().animating() == Some(a);
            if stuck {
                crate::log!("WATCHDOG: no AnimDone({a:?}) after {ANIM_WATCHDOG_MS} ms, completing it");
                dispatch(Input::AnimDone(a));
            }
        });
    });
}

static CTX: OnceLock<Ctx> = OnceLock::new();

impl Ctx {
    pub fn panel(&self) -> usize {
        self.panel_h.load(Ordering::SeqCst)
    }
}

/// Tauri label of the current panel webview: "main", or "main-N" after an Explorer restart
/// forced a re-create (Tauri never releases the label of a window Windows destroyed).
static PANEL_LABEL: Mutex<String> = Mutex::new(String::new());
static RECREATES: AtomicUsize = AtomicUsize::new(0);

pub fn panel_label() -> String {
    let l = PANEL_LABEL.lock().unwrap();
    if l.is_empty() { "main".into() } else { l.clone() }
}

/// The app context. Tauri creates the config windows before `setup()` runs, so on a busy machine
/// the page can call a command first (seen: "ctx not initialised" panic 0.7 s after launch).
/// Commands run on worker threads, so they simply wait for setup. Main-thread hooks that can fire
/// during setup use `CTX.get()` and bail out instead.
pub fn ctx() -> &'static Ctx {
    if let Some(c) = CTX.get() {
        return c;
    }
    for _ in 0..2000 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        if let Some(c) = CTX.get() {
            return c;
        }
    }
    panic!("ctx not initialised after 10 s")
}

fn anim_name(a: Anim) -> &'static str {
    match a {
        Anim::Appear => "appear",
        Anim::Disappear => "disappear",
        Anim::Pop => "pop",
        Anim::Drop => "drop",
    }
}

fn parse_anim(s: &str) -> Option<Anim> {
    Some(match s {
        "appear" => Anim::Appear,
        "disappear" => Anim::Disappear,
        "pop" => Anim::Pop,
        "drop" => Anim::Drop,
        _ => return None,
    })
}

/// Feed one input to the machine and execute the resulting effects. Main thread only.
/// The machine lock is released before effects run, because effects can re-enter (WinEvents).
pub fn dispatch(input: Input) {
    let c = ctx();
    let (fx, panel) = {
        let mut m = c.machine.lock().unwrap();
        let fx = m.step(input);
        (fx, m.panel())
    };
    if !fx.is_empty() || !matches!(input, Input::Focus(_)) {
        crate::log!("IN {input:?} -> {fx:?} => {panel:?}");
    }
    for e in fx {
        execute(e);
    }
    if c.snapshot_pending.swap(false, Ordering::SeqCst) {
        take_snapshot();
    }
    if matches!(input, Input::AnimDone(_) | Input::ShellRestarted) {
        crate::broker::schedule_occlusion_check();
    }
}

/// Run on the main thread from any thread (commands run on a worker pool).
pub fn dispatch_main(input: Input) {
    let _ = ctx().app.run_on_main_thread(move || dispatch(input));
}

fn execute(e: Effect) {
    let c = ctx();
    let panel = hwnd(c.panel());
    match e {
        Effect::ReparentToDesktop => {
            desktop::to_desktop(panel);
            let _ = c.app.emit_to(panel_label().as_str(), "surface", true);
        }
        Effect::ReparentToTopmost => {
            desktop::to_topmost(panel);
            let _ = c.app.emit_to(panel_label().as_str(), "surface", false);

        }
        // The capture itself happens right after ReparentToTopmost: only a top-level window can be
        // excluded from capture, which lets us read what is behind it without hiding it.
        Effect::CaptureSnapshot => c.snapshot_pending.store(true, Ordering::SeqCst),
        Effect::Show => {
            if c.ready.load(Ordering::SeqCst) {
                desktop::show_noactivate(panel);
            } else {
                c.pending_show.store(true, Ordering::SeqCst);
            }
        }
        Effect::Hide => desktop::hide(panel),
        Effect::Activate => {
            let ok = desktop::activate(panel);
            crate::log!("activate panel ok={ok} fg={}", desktop::describe(desktop::foreground()));
        }
        Effect::RestoreForeground(w) => {
            let ok = desktop::activate(hwnd(w));
            crate::log!("restore fg -> {} ok={ok}", desktop::describe(hwnd(w)));
        }
        Effect::PlayAnim(a) => {
            if c.ready.load(Ordering::SeqCst) {
                emit_anim(a);
            } else {
                *c.pending_anim.lock().unwrap() = Some(a);
            }
        }
        Effect::PersistHidden(h) => {
            let mut cfg = c.cfg.lock().unwrap();
            cfg.hidden = h;
            cfg.save();
        }
        Effect::SetPill(p) => {
            let pill = hwnd(c.pill);
            match p {
                Pill::Shown => desktop::show_noactivate(pill),
                Pill::Off => desktop::hide(pill),
            }
            // The pill page runs its meter/countdown only while it is actually on screen.
            let _ = c.app.emit_to("pill", "pill-shown", p == Pill::Shown);
        }
        Effect::SuspendPanel(s) => {
            crate::log!("panel visible={}", !s);
            crate::controls::send(crate::controls::Msg::Visible(!s)); // stats tick only while visible
            // The page can't tell a hidden/covered panel from a visible one (the HWND stays
            // "visible" to Chromium), so tell it: it stops its live loops while nobody can see it.
            let _ = c.app.emit_to(panel_label().as_str(), "panel-visible", !s);
            set_memory_target(s);
        }
    }
}

/// While the panel is hidden or covered, ask WebView2 to keep as little memory as it can
/// (ICoreWebView2_19::MemoryUsageTargetLevel); back to normal when it is shown (CHECKLIST H1).
fn set_memory_target(low: bool) {
    let Some(w) = ctx().app.get_webview_window(&panel_label()) else { return };
    let _ = w.with_webview(move |pw| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        use windows::core::Interface;
        if let Ok(core) = pw.controller().CoreWebView2() {
            if let Ok(c19) = core.cast::<ICoreWebView2_19>() {
                let lvl = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
                let r = c19.SetMemoryUsageTargetLevel(lvl);
                crate::log!("webview memory target {} -> {:?}", if low { "low" } else { "normal" }, r.is_ok());
            }
        }
    });
}

/// Is the pill on screen right now? (The pill page asks at startup; the event may predate it.)
#[tauri::command]
pub fn is_pill_shown() -> bool {
    ctx().machine.lock().unwrap().pill() == Pill::Shown
}

/// Can the user see the panel right now? (The page asks at startup, before any event.)
#[tauri::command]
pub fn is_panel_visible() -> bool {
    panel_now() != Panel::Hidden && !COVERED.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------- OS events (main thread)

/// Last foreground window that is NOT one of ours: where focus belongs when we must give it back.
static LAST_EXTERNAL_FG: AtomicUsize = AtomicUsize::new(0);
/// Last foreground *app* window (not ours, not desktop/taskbar): the "previous app" for tray pops,
/// because clicking the tray first makes the taskbar the foreground window.
static LAST_APP_FG: AtomicUsize = AtomicUsize::new(0);

pub fn on_hotkey() {
    let c = ctx();
    let mut fg = desktop::foreground();
    let is_panel = fg.0 as usize == c.panel();
    if !is_panel && desktop::is_ours(fg) {
        // One of our hidden windows (pill/broker) holds focus: the real "previous" window is the last external one.
        fg = hwnd(LAST_EXTERNAL_FG.load(Ordering::SeqCst));
    }
    let fg_opt = if is_panel || fg.0.is_null() { None } else { Some(fg.0 as usize) };
    crate::log!("hotkey fg={} epoch={}", desktop::describe(fg), crate::controls::timers::now_ms());
    dispatch(Input::Shortcut { fg: fg_opt, fg_is_desktop: !is_panel && desktop::is_desktopish(fg) });
}

// ---- desktop-click focus return (CHECKLIST B6) ----
// When Explorer renders the desktop through DirectComposition (seen after a reboot on build 26300),
// a click on any child of Progman activates the DESKTOP, which steals focus from the app you were
// in. Explorer decides that, not us. So: if the desktop takes the foreground within 400 ms of a
// press on the panel, hand focus straight back to the window that had it a moment before.
static CURRENT_FG: AtomicUsize = AtomicUsize::new(0);
static BEFORE_DESKTOP: AtomicUsize = AtomicUsize::new(0);
static DESKTOP_FG_AT: AtomicU64 = AtomicU64::new(0);
static PANEL_PRESS_AT: AtomicU64 = AtomicU64::new(0);
const PRESS_WINDOW_MS: u64 = 400;

static LAST_GIVEBACK_AT: AtomicU64 = AtomicU64::new(0);

/// An app window we may hand focus back to (not the desktop, not the taskbar, not ours).
fn is_app_window(h: HWND) -> bool {
    !h.0.is_null() && desktop::is_window(h) && !desktop::is_ours(h) && !desktop::is_desktopish(h)
}

fn give_focus_back(why: &str) {
    let prev = hwnd(BEFORE_DESKTOP.load(Ordering::SeqCst));
    if !is_app_window(prev) {
        return;
    }
    LAST_GIVEBACK_AT.store(crate::controls::timers::now_ms(), Ordering::SeqCst);
    #[link(name = "user32")]
    extern "system" {
        fn SwitchToThisWindow(h: HWND, alt_tab: i32);
    }
    unsafe { SwitchToThisWindow(prev, 1) };
    crate::log!("focus returned to {} after a panel click ({why})", desktop::describe(prev));
}

/// The page saw a pointer press on the panel.
#[tauri::command]
pub fn panel_pressed() {
    let now = crate::controls::timers::now_ms();
    PANEL_PRESS_AT.store(now, Ordering::SeqCst);
    if now.saturating_sub(DESKTOP_FG_AT.load(Ordering::SeqCst)) < PRESS_WINDOW_MS {
        let _ = ctx().app.run_on_main_thread(|| give_focus_back("press after activation"));
    }
}

/// Focus guard (CHECKLIST B6): our windows may only hold keyboard focus while the panel is
/// deliberately popped. Any other activation of our windows (e.g. Windows activating a hidden
/// window at process start) is undone immediately.
pub fn on_foreground(h: HWND) {
    let c = ctx();
    let before = CURRENT_FG.swap(h.0 as usize, Ordering::SeqCst);

    if desktop::is_child(hwnd(c.panel())) && matches!(desktop::class_of(h).as_str(), "Progman" | "WorkerW") {
        let now = crate::controls::timers::now_ms();
        if is_app_window(hwnd(before)) {
            // An app was in front right before the desktop took over: that's where focus belongs.
            BEFORE_DESKTOP.store(before, Ordering::SeqCst);
        } else if now.saturating_sub(LAST_GIVEBACK_AT.load(Ordering::SeqCst)) >= 1000 {
            // The desktop was already in front (you were on the desktop): nothing to give back.
            // (Within 1 s of a hand-back we are mid-burst and keep the app we just returned to.)
            BEFORE_DESKTOP.store(0, Ordering::SeqCst);
        }
        DESKTOP_FG_AT.store(now, Ordering::SeqCst);
        if now.saturating_sub(PANEL_PRESS_AT.load(Ordering::SeqCst)) < PRESS_WINDOW_MS {
            give_focus_back("activation after press");
        }
    }
    if !desktop::is_ours(h) {
        if !h.0.is_null() {
            LAST_EXTERNAL_FG.store(h.0 as usize, Ordering::SeqCst);
            if !desktop::is_desktopish(h) {
                LAST_APP_FG.store(h.0 as usize, Ordering::SeqCst);
            }
        }
    } else {
        // Only the panel and the pill are policed. Other windows of ours are legitimate
        // foreground owners, e.g. the tray's message window, which must take focus for its
        // context menu to stay open.
        let is_panel = h.0 as usize == c.panel();
        let is_pill = h.0 as usize == c.pill;
        let allowed = (!is_panel && !is_pill) || (is_panel && c.machine.lock().unwrap().panel() == Panel::Popped);
        if !allowed {
            let back = hwnd(LAST_EXTERNAL_FG.load(Ordering::SeqCst));
            let ok = !back.0.is_null() && desktop::is_window(back) && desktop::activate(back);
            crate::log!("FOCUS GUARD: {} took focus unexpectedly -> gave it back to {} ok={ok}", desktop::describe(h), desktop::describe(back));
        }
    }
    dispatch(Input::Focus(h.0 as usize == c.panel()));
}

/// Seed the guard with whatever was in front before our windows existed.
pub fn seed_foreground(h: HWND) {
    if !desktop::is_ours(h) && !h.0.is_null() {
        LAST_EXTERNAL_FG.store(h.0 as usize, Ordering::SeqCst);
    }
}

pub fn on_wallpaper_changed() {
    crate::log!("wallpaper changed");
    rebuild_glass();
}

pub fn on_theme_changed() {
    // Push it to the panel at once: Chromium's own prefers-color-scheme update lags ~0.4 s (D4).
    let dark = crate::controls::theme::is_dark();
    crate::log!("theme changed (ImmersiveColorSet) dark={dark} epoch={}", crate::controls::timers::now_ms());
    let _ = ctx().app.emit_to(panel_label().as_str(), "theme", dark);
}

pub fn on_display_changed() {
    revalidate("display change");
}

#[derive(Clone, serde::Serialize)]
struct Snapshot {
    url: String,
    /// Size of the captured area (= the window) in physical px.
    w: i32,
    h: i32,
}

/// Pop-over glass: capture what is behind the (top-level) panel, blur it off the UI thread.
fn take_snapshot() {
    let c = ctx();
    let panel = hwnd(c.panel());
    let t0 = std::time::Instant::now();
    let Some((bgra, w, h)) = desktop::capture_behind(panel) else { return };
    let capture_ms = t0.elapsed().as_millis();
    if let Some(path) = std::env::var_os("DESKPANEL_DUMP_SNAPSHOT") {
        // Test hook (CHECKLIST D6): save the raw capture to compare with what is really behind.
        let rgb: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
        if let Some(img) = image::RgbImage::from_raw(w as u32, h as u32, rgb) {
            let _ = img.save(std::path::PathBuf::from(path));
        }
    }
    let scale = desktop::scale(panel);
    let (blur, sat) = {
        let cfg = c.cfg.lock().unwrap();
        (cfg.blur_px, cfg.saturate)
    };
    std::thread::spawn(move || {
        if let Some(url) = wallpaper::snapshot_glass(&bgra, w, h, scale, blur, sat) {
            crate::log!("snapshot {w}x{h}: capture {capture_ms} ms, total {} ms", t0.elapsed().as_millis());
            let _ = ctx().app.emit_to(panel_label().as_str(), "snapshot", Snapshot { url, w, h });
        }
    });
}

/// WM_MOVE (during a drag or any reposition) and WM_EXITSIZEMOVE (drag finished).
pub fn on_panel_moved(finished: bool) {
    // Setup moves the window before the context exists; those moves need no handling.
    let Some(c) = CTX.get() else { return };
    let _ = c.app.emit_to(panel_label().as_str(), "geom-changed", ());
    if finished {
        let panel = hwnd(c.panel());
        let r = desktop::rect(panel);
        {
            let mut cfg = c.cfg.lock().unwrap();
            cfg.position = Some((r.left, r.top));
            cfg.save();
        }
        crate::log!("drag end: saved position {},{}", r.left, r.top);
        let _ = c.app.emit_to(panel_label().as_str(), "backdrop-changed", ());
        if !desktop::is_child(panel) {
            take_snapshot(); // popped and moved: what is behind it changed
        }
    }
}

// ---------------------------------------------------------------- occlusion

/// The panel counts as covered when >= 97 % of it is hidden: below that the user can still see
/// part of it, so stats keep updating and no pill is shown (SPEC 4.3, revised from 60 %).
const COVERED_AT: f64 = 0.97;
static COVERED: AtomicBool = AtomicBool::new(false);

static FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn on_occlusion_check() {
    let Some(c) = CTX.get() else { return };
    // Fullscreen app in front -> no pill (L3). Checked on the same debounced window events.
    let fs = desktop::fullscreen_app_in_front();
    if FULLSCREEN.swap(fs, Ordering::SeqCst) != fs {
        crate::log!("fullscreen app in front: {fs}");
        dispatch(Input::Fullscreen(fs));
    }
    let panel = hwnd(c.panel());
    let covered = desktop::is_child(panel) && panel_now() == Panel::Desktop && {
        let cov = crate::occlusion::coverage(panel);
        cov >= COVERED_AT
    };
    if COVERED.swap(covered, Ordering::SeqCst) != covered {
        crate::log!("occlusion: covered={covered}");
        dispatch(Input::Covered(covered));
    }
}

// ---------------------------------------------------------------- tray / single instance

fn last_app() -> Option<WindowId> {
    let h = hwnd(LAST_APP_FG.load(Ordering::SeqCst));
    (!h.0.is_null() && desktop::is_window(h)).then_some(h.0 as usize)
}

pub fn panel_now() -> Panel {
    ctx().machine.lock().unwrap().panel()
}

/// Tray "Show / hide panel".
pub fn tray_toggle() {
    let input = if panel_now() == Panel::Hidden { Input::TrayShow } else { Input::TrayHide };
    dispatch(input);
}

/// Tray icon left click / "Pop over apps": bring the panel above apps (or drop it if popped).
pub fn tray_pop() {
    if panel_now() == Panel::Popped {
        dispatch(Input::Shortcut { fg: None, fg_is_desktop: false });
    } else {
        dispatch(Input::TrayPop { fg: last_app() });
    }
}

pub fn set_pill_enabled(on: bool) {
    {
        let mut cfg = ctx().cfg.lock().unwrap();
        cfg.pill_enabled = on;
        cfg.save();
    }
    dispatch(Input::PillEnabled(on));
}

pub fn set_autostart(on: bool) {
    {
        let mut cfg = ctx().cfg.lock().unwrap();
        cfg.autostart = on;
        cfg.save();
    }
    crate::autostart::apply(on);
}

/// A second copy was launched (SPEC S4): don't start another, just pop this one.
pub fn on_second_instance() {
    crate::log!("second instance launched -> pop");
    let _ = ctx().app.run_on_main_thread(|| {
        if panel_now() != Panel::Popped {
            dispatch(Input::TrayPop { fg: last_app() });
        }
    });
}

// ---------------------------------------------------------------- session / power / display

/// Lock/unlock, sleep/resume: re-check that the panel is still where it should be.
pub fn on_session_event(what: &str) {
    crate::log!("session: {what}");
    if matches!(what, "unlock" | "resume") {
        revalidate(what);
    }
}

/// Re-anchor to the (current) desktop window, keep the position on screen, refresh the glass.
pub fn revalidate(why: &str) {
    let c = ctx();
    let panel = hwnd(c.panel());
    if !desktop::is_window(panel) {
        crate::log!("revalidate({why}): panel window is gone!");
        return;
    }
    let state = panel_now();
    if state == Panel::Desktop {
        let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(panel).unwrap_or_default() };
        if parent != desktop::progman() {
            crate::log!("revalidate({why}): parent changed -> re-anchoring");
            desktop::to_desktop(panel);
        }
    }
    let r = desktop::rect(panel);
    let (x, y) = desktop::restore_position(panel, r.left, r.top);
    if (x, y) != (r.left, r.top) {
        desktop::move_to(panel, x, y);
    }
    if desktop::is_child(panel) {
        desktop::set_region(panel, true);
    }
    rebuild_glass();
    let _ = c.app.emit_to(panel_label().as_str(), "geom-changed", ());
    crate::log!("revalidate({why}): ok state={state:?} parent={} pos={x},{y}", desktop::describe(unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetParent(panel).unwrap_or_default()
    }));
}

pub fn on_shell_restarted() {
    crate::log!("explorer restarted (TaskbarCreated)");
    if desktop::is_window(hwnd(ctx().panel())) {
        dispatch(Input::ShellRestarted); // still alive (e.g. it was popped): just re-anchor
    } else {
        recreate_panel();
    }
}

/// Explorer restarted and took our desktop-child window down with Progman. Everything that
/// matters (state machine, config, glass, controls) lives in Rust, so just build a fresh
/// webview window from the same config and adopt it.
fn recreate_panel() {
    let c = ctx();
    crate::log!("panel window was destroyed with Progman -> re-creating it");
    c.ready.store(false, Ordering::SeqCst);
    let app = c.app.clone();
    // Built off the UI thread: Tauri marshals window creation to the event loop itself.
    std::thread::spawn(move || {
        let Some(wc) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() else { return };
        // Tauri keeps the dead window's label registered, so the new window gets a fresh one.
        if let Some(old) = app.get_webview_window(&panel_label()) {
            let _ = old.destroy();
        }
        let n = RECREATES.fetch_add(1, Ordering::SeqCst) + 1;
        let mut wc = wc;
        wc.label = format!("main-{n}");
        let label = wc.label.clone();
        match tauri::WebviewWindowBuilder::from_config(&app, &wc).and_then(|b| b.build()) {
            Ok(win) => match win.hwnd() {
                Ok(h) => {
                    let h = h.0 as usize;
                    *PANEL_LABEL.lock().unwrap() = label.clone();
                    let _ = app.run_on_main_thread(move || adopt_panel(hwnd(h)));
                }
                Err(e) => crate::log!("re-create: no hwnd: {e}"),
            },
            Err(e) => crate::log!("re-create failed: {e}"),
        }
    });
}

fn adopt_panel(h: HWND) {
    let c = ctx();
    c.panel_h.store(h.0 as usize, Ordering::SeqCst);
    crate::panelwnd::install(h);
    let (x, y) = match c.cfg.lock().unwrap().position {
        Some((x, y)) => desktop::restore_position(h, x, y),
        None => desktop::default_position(h),
    };
    desktop::move_to(h, x, y);
    desktop::to_desktop(h);
    if panel_now() == Panel::Desktop {
        // Shown (with the appear animation) once the new page reports ready, like at startup.
        c.pending_show.store(true, Ordering::SeqCst);
        *c.pending_anim.lock().unwrap() = Some(Anim::Appear);
    }
    crate::log!("re-created panel {} state={:?}", desktop::describe(h), panel_now());
}

// ---------------------------------------------------------------- glass

fn rebuild_glass() {
    let c = ctx();
    let panel = hwnd(c.panel());
    let (mon, _) = desktop::monitor_of(panel);
    let scale = desktop::scale(panel);
    let (blur, sat) = {
        let cfg = c.cfg.lock().unwrap();
        (cfg.blur_px, cfg.saturate)
    };
    std::thread::spawn(move || match wallpaper::build(mon, scale, blur, sat) {
        Ok(g) => {
            crate::log!("glass rebuilt in {} ms ({})", g.build_ms, g.mode);
            *ctx().glass.lock().unwrap() = Some(g);
            let _ = ctx().app.emit_to(panel_label().as_str(), "glass-changed", ());
        }
        Err(e) => crate::log!("glass build failed: {e}"),
    });
}

// ---------------------------------------------------------------- webview commands

#[derive(serde::Serialize)]
pub struct Geom {
    /// Window top-left relative to its monitor, physical px.
    x: i32,
    y: i32,
    scale: f64,
}

#[tauri::command]
pub fn get_geom() -> Geom {
    let panel = hwnd(ctx().panel());
    let r = desktop::rect(panel);
    let (mon, _) = desktop::monitor_of(panel);
    Geom { x: r.left - mon.left, y: r.top - mon.top, scale: desktop::scale(panel) }
}

#[tauri::command]
pub async fn get_glass() -> Result<Glass, String> {
    if let Some(g) = ctx().glass.lock().unwrap().clone() {
        return Ok(g);
    }
    let c = ctx();
    let panel = hwnd(c.panel());
    let (mon, _) = desktop::monitor_of(panel);
    let (blur, sat) = {
        let cfg = c.cfg.lock().unwrap();
        (cfg.blur_px, cfg.saturate)
    };
    let g = wallpaper::build(mon, desktop::scale(panel), blur, sat)?;
    crate::log!("glass built in {} ms ({})", g.build_ms, g.mode);
    *c.glass.lock().unwrap() = Some(g.clone());
    Ok(g)
}

#[derive(serde::Serialize)]
pub struct Backdrop {
    url: String,
    /// Window size in physical px (the crop covers exactly the window).
    w: i32,
    h: i32,
}

/// The unblurred wallpaper behind the window (only the 4 px ring around the panel shows it).
#[tauri::command]
pub async fn get_backdrop() -> Option<Backdrop> {
    let c = ctx();
    let jpeg = c.glass.lock().unwrap().as_ref().map(|g| g.full_jpeg.clone())?;
    let panel = hwnd(c.panel());
    let r = desktop::rect(panel);
    let (mon, _) = desktop::monitor_of(panel);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let url = wallpaper::crop_backdrop(&jpeg, r.left - mon.left, r.top - mon.top, w, h)?;
    Some(Backdrop { url, w, h })
}

/// The page has painted its first real frame (glass decoded). Only now may the window appear,
/// which is what prevents a white / unstyled first frame (CHECKLIST D2).
#[tauri::command]
pub fn ready() {
    let _ = ctx().app.run_on_main_thread(|| {
        let c = ctx();
        c.ready.store(true, Ordering::SeqCst);
        crate::log!("webview ready");
        crate::panelwnd::hook_children(hwnd(c.panel())); // webview host windows exist by now
        if !is_panel_visible() {
            set_memory_target(true); // started hidden: the startup SuspendPanel ran before the webview existed
        }
        let pending = c.pending_anim.lock().unwrap().take();
        if let Some(a) = pending {
            emit_anim(a);
        }
        if c.pending_show.swap(false, Ordering::SeqCst) {
            desktop::show_noactivate(hwnd(c.panel()));
        }
    });
}

/// The page reports its content height (CSS px, panel only). Resize the window to fit it
/// (+40 px shadow margin each side) and, if the user never placed it, keep it vertically centred.
#[tauri::command]
pub fn fit_height(css_px: f64) {
    let _ = ctx().app.run_on_main_thread(move || {
        let c = ctx();
        let panel = hwnd(c.panel());
        let s = desktop::scale(panel);
        let r = desktop::rect(panel);
        let w = r.right - r.left;
        let h = ((css_px + 80.0) * s).round() as i32;
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;
            let _ = SetWindowPos(panel, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        let placed = c.cfg.lock().unwrap().position;
        let (x, y) = match placed {
            Some((x, y)) => desktop::restore_position(panel, x, y),
            None => desktop::default_position(panel),
        };
        desktop::move_to(panel, x, y);
        if desktop::is_child(panel) {
            desktop::set_region(panel, true); // region depends on the window size
        }
        crate::log!("fit_height css={css_px} -> {w}x{h} at {x},{y}");
    });
}

#[tauri::command]
pub fn anim_done(anim: String) {
    // Test hook (CHECKLIST evidence for the watchdog): DESKPANEL_TEST_LOSE_ANIMDONE=drop
    // makes the app ignore the first AnimDone of that kind, simulating a hung webview.
    static LOST: AtomicBool = AtomicBool::new(false);
    if std::env::var("DESKPANEL_TEST_LOSE_ANIMDONE").map(|v| v == anim).unwrap_or(false) && !LOST.swap(true, Ordering::SeqCst) {
        crate::log!("TEST: dropping AnimDone({anim}) on purpose");
        return;
    }
    if let Some(a) = parse_anim(&anim) {
        dispatch_main(Input::AnimDone(a));
    }
}

/// Frame-time stats for one animation (CHECKLIST D1). `deltas` = ms between rAF callbacks.
#[tauri::command]
pub fn frame_stats(anim: String, deltas: Vec<f64>) {
    let mut d: Vec<f64> = deltas.into_iter().filter(|x| x.is_finite() && *x > 0.0).collect();
    if d.is_empty() {
        return;
    }
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = d.len();
    let p = |q: f64| d[((n as f64 * q).ceil() as usize).saturating_sub(1).min(n - 1)];
    let mean = d.iter().sum::<f64>() / n as f64;
    let within = d.iter().filter(|x| **x <= 7.6).count();
    let over_60 = d.iter().filter(|x| **x > 16.7).count();
    crate::log!(
        "FRAMES anim={anim} n={n} mean={mean:.2}ms fps~{:.0} p50={:.2} p95={:.2} max={:.2} within7.6={:.1}% over16.7={over_60}",
        1000.0 / mean,
        p(0.5),
        p(0.95),
        d[n - 1],
        100.0 * within as f64 / n as f64
    );
}

#[tauri::command]
pub fn js_log(msg: String) {
    crate::log!("JS {msg}");
}

/// Pointer went down on a non-interactive part of the panel: start the system move loop.
#[tauri::command]
pub fn drag_start() {
    let _ = ctx().app.run_on_main_thread(|| crate::panelwnd::begin_drag(hwnd(ctx().panel())));
}

/// The pill page reports its current shape (CSS px in the pill window): clip the window to it.
#[tauri::command]
pub fn pill_region(x: f64, y: f64, w: f64, h: f64, r: f64) {
    let _ = ctx().app.run_on_main_thread(move || desktop::set_round_region(hwnd(ctx().pill), x, y, w, h, r));
}

#[tauri::command]
pub fn esc() {
    dispatch_main(Input::Esc);
}

#[tauri::command]
pub fn close_panel() {
    dispatch_main(Input::CloseClicked);
}

#[tauri::command]
pub fn panel_state() -> String {
    format!("{:?}", ctx().machine.lock().unwrap().panel())
}

// ---------------------------------------------------------------- startup

pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load();
    let main = app.get_webview_window("main").ok_or("no main window")?;
    let pill = app.get_webview_window("pill").ok_or("no pill window")?;
    let panel_h = HWND(main.hwnd()?.0 as *mut core::ffi::c_void);
    let pill_h = HWND(pill.hwnd()?.0 as *mut core::ffi::c_void);
    crate::log!("start v{} panel={} pill={} cfg={cfg:?}", env!("CARGO_PKG_VERSION"), desktop::describe(panel_h), desktop::describe(pill_h));

    crate::panelwnd::install(panel_h);
    // Position before anything is visible.
    let (x, y) = match cfg.position {
        Some((x, y)) => desktop::restore_position(panel_h, x, y),
        None => desktop::default_position(panel_h),
    };
    desktop::move_to(panel_h, x, y);

    // Pill: topmost tool window that never activates.
    desktop::make_overlay(pill_h);
    let pr = desktop::rect(pill_h);
    let (mon, _) = desktop::monitor_of(pill_h);
    desktop::move_to(pill_h, mon.left + (mon.right - mon.left - (pr.right - pr.left)) / 2, mon.top); // pill has its own 8px top margin


    let (machine, fx) = Machine::new(cfg.hidden, cfg.pill_enabled);
    let started_hidden = machine.panel() == Panel::Hidden;
    let ctx = Ctx {
        app: app.handle().clone(),
        panel_h: AtomicUsize::new(panel_h.0 as usize),
        pill: pill_h.0 as usize,
        machine: Mutex::new(machine),
        cfg: Mutex::new(cfg),
        ready: AtomicBool::new(false),
        pending_show: AtomicBool::new(false),
        pending_anim: Mutex::new(None),
        glass: Mutex::new(None),
        anim_seq: AtomicU64::new(0),
        snapshot_pending: AtomicBool::new(false),
    };
    CTX.set(ctx).map_err(|_| "ctx already set")?;
    if started_hidden {
        // Still parent it to the desktop so a later "show" only has to make it visible.
        desktop::to_desktop(panel_h);
    }
    crate::log!("initial effects {fx:?}");
    for e in fx {
        execute(e);
    }
    {
        let auto = self::ctx().cfg.lock().unwrap().autostart;
        crate::autostart::apply(auto); // keep the Run entry in sync (and pointing at this exe)
    }
    crate::controls::start(app.handle().clone(), !started_hidden);
    crate::broker::start()?;
    crate::tray::build(app)?;
    // If Windows already handed focus to one of our (hidden) windows during startup, give it back.
    let fg = desktop::foreground();
    if desktop::is_ours(fg) {
        on_foreground(fg);
    }
    Ok(())
}
