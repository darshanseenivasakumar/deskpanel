//! The "controls" worker: one thread (COM MTA) owns every OS control object. Webview commands
//! and OS change callbacks post `Msg`s to it; it applies them and publishes a `Controls` snapshot
//! to the panel only when something changed.
//!
//! Idle cost: when the panel is not visible and no timer runs, the worker blocks on its queue
//! with no timeout at all (0 % CPU). While visible it ticks at 1 Hz for stats.
pub mod audio;
pub mod brightness;
pub mod media;
pub mod power;
pub mod radios;
pub mod stats;
pub mod theme;
pub mod timers;

use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use self::media::MediaInfo;
use self::radios::Which;
use self::stats::StatsInfo;
use self::timers::{StopwatchInfo, TimerInfo, Timers};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Cmd {
    Volume { v: f32 },
    Mute { on: bool },
    Brightness { v: u8 },
    Wifi { on: bool },
    Bluetooth { on: bool },
    Dark { on: bool },
    Power { mode: String },
    Media { action: String },
    TimerStart { ms: u64 },
    TimerPause,
    TimerResume,
    TimerReset,
    SwStart,
    SwStop,
    SwLap,
    SwReset,
}

pub enum Msg {
    Cmd(Cmd),
    /// Something changed in the OS (volume, radio, media...): re-read and publish.
    OsChanged,
    MediaSessionChanged,
    /// Panel visible -> tick stats at 1 Hz; hidden/covered -> stop ticking.
    Visible(bool),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Controls {
    pub volume: Option<f32>,
    pub muted: bool,
    pub brightness: Option<u8>,
    pub wifi: Option<bool>,
    pub wifi_name: Option<String>,
    pub bluetooth: Option<bool>,
    pub dark: bool,
    pub power: String,
    pub media: Option<MediaInfo>,
    pub timer: TimerInfo,
    pub stopwatch: StopwatchInfo,
    /// Last error from a control action, shown briefly in the UI.
    pub error: Option<String>,
}

static TX: OnceLock<Mutex<Sender<Msg>>> = OnceLock::new();
static LATEST: Mutex<Option<Controls>> = Mutex::new(None);
static LATEST_STATS: Mutex<Option<StatsInfo>> = Mutex::new(None);
static ART: Mutex<Option<String>> = Mutex::new(None);

pub fn send(m: Msg) {
    if let Some(tx) = TX.get() {
        let _ = tx.lock().unwrap().send(m);
    }
}

pub fn start(app: AppHandle, visible: bool) {
    let (tx, rx) = channel();
    let _ = TX.set(Mutex::new(tx.clone()));
    let _ = std::thread::Builder::new().name("controls".into()).spawn(move || worker(app, tx, rx, visible));
}

struct Worker {
    app: AppHandle,
    audio: Option<audio::Audio>,
    bright: Option<brightness::Brightness>,
    brightness: Option<u8>,
    radios: Option<radios::Radios>,
    media: Option<media::Media>,
    stats: stats::Stats,
    timers: Timers,
    visible: bool,
    last: Option<Controls>,
    last_activity: Option<bool>,
    error: Option<String>,
    ticks: u32,
}

fn warn<T, E: std::fmt::Display>(what: &str, r: Result<T, E>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            crate::log!("controls: {what} failed: {e}");
            None
        }
    }
}

fn worker(app: AppHandle, tx: Sender<Msg>, rx: Receiver<Msg>, visible: bool) {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
    }
    let t0 = Instant::now();
    let bright = warn("brightness init", brightness::Brightness::new());
    let mut w = Worker {
        app,
        audio: warn("audio init", audio::Audio::new(tx.clone())),
        brightness: bright.as_ref().and_then(|b| warn("brightness read", b.get())),
        bright,
        radios: warn("radios init", radios::Radios::new(tx.clone())),
        media: warn("media init", media::Media::new(tx.clone())),
        stats: stats::Stats::new(),
        timers: Timers::default(),
        visible,
        last: None,
        last_activity: None,
        error: None,
        ticks: 0,
    };
    crate::log!("controls: worker up in {} ms (visible={visible})", t0.elapsed().as_millis());
    w.publish();
    if w.visible {
        w.tick();
    }

    loop {
        let msg = match w.next_wakeup() {
            Some(d) => rx.recv_timeout(d),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match msg {
            Ok(first) => {
                let mut batch = vec![first];
                while let Ok(m) = rx.try_recv() {
                    batch.push(m);
                }
                w.handle(batch);
            }
            Err(RecvTimeoutError::Timeout) => w.tick(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
        w.check_timer();
        w.publish();
    }
}

impl Worker {
    fn next_wakeup(&self) -> Option<Duration> {
        let stats = self.visible.then(|| Duration::from_millis(1000));
        let timer = self.timers.ms_until_timer_end().map(|ms| Duration::from_millis(ms.max(5)));
        match (stats, timer) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn handle(&mut self, batch: Vec<Msg>) {
        // Slider drags arrive as bursts: apply only the newest volume / brightness value.
        let last_vol = batch.iter().rposition(|m| matches!(m, Msg::Cmd(Cmd::Volume { .. })));
        let last_bri = batch.iter().rposition(|m| matches!(m, Msg::Cmd(Cmd::Brightness { .. })));
        for (i, m) in batch.into_iter().enumerate() {
            match m {
                Msg::Cmd(Cmd::Volume { .. }) if Some(i) != last_vol => {}
                Msg::Cmd(Cmd::Brightness { .. }) if Some(i) != last_bri => {}
                Msg::Cmd(c) => self.apply(c),
                Msg::OsChanged => {}
                Msg::MediaSessionChanged => {
                    if let Some(m) = self.media.as_mut() {
                        m.rehook();
                    }
                }
                Msg::Visible(v) => {
                    if v != self.visible {
                        self.visible = v;
                        crate::log!("controls: stats {}", if v { "on" } else { "off" });
                        if v {
                            self.tick();
                        }
                    }
                }
            }
        }
    }

    fn apply(&mut self, c: Cmd) {
        crate::log!("controls: {c:?}");
        let r: Result<(), String> = match &c {
            Cmd::Volume { v } => self.audio.as_ref().ok_or("no audio".to_string()).and_then(|a| a.set_volume(*v).map_err(|e| e.to_string())),
            Cmd::Mute { on } => self.audio.as_ref().ok_or("no audio".to_string()).and_then(|a| a.set_mute(*on).map_err(|e| e.to_string())),
            Cmd::Brightness { v } => match self.bright.as_ref() {
                Some(b) => b.set(*v).map(|_| self.brightness = Some(*v)).map_err(|e| e.to_string()),
                None => Err("brightness unavailable".into()),
            },
            Cmd::Wifi { on } => self.radios.as_ref().ok_or("no radios".to_string()).and_then(|r| r.set(Which::Wifi, *on)),
            Cmd::Bluetooth { on } => self.radios.as_ref().ok_or("no radios".to_string()).and_then(|r| r.set(Which::Bluetooth, *on)),
            Cmd::Dark { on } => {
                if theme::set_dark(*on) {
                    Ok(())
                } else {
                    Err("could not write theme".into())
                }
            }
            Cmd::Power { mode } => power::set(mode),
            Cmd::Media { action } => self.media.as_ref().ok_or("no media".to_string()).and_then(|m| m.command(action)),
            Cmd::TimerStart { ms } => Ok(self.timers.timer_start(*ms)),
            Cmd::TimerPause => Ok(self.timers.timer_pause()),
            Cmd::TimerResume => Ok(self.timers.timer_resume()),
            Cmd::TimerReset => Ok(self.timers.timer_reset()),
            Cmd::SwStart => Ok(self.timers.sw_start()),
            Cmd::SwStop => Ok(self.timers.sw_stop()),
            Cmd::SwLap => Ok(self.timers.sw_lap()),
            Cmd::SwReset => Ok(self.timers.sw_reset()),
        };
        self.error = r.err().map(|e| {
            crate::log!("controls: {c:?} -> ERROR {e}");
            e
        });
    }

    /// 1 Hz while visible: stats, external brightness changes, audio device switches.
    fn tick(&mut self) {
        let s = self.stats.sample();
        if std::env::var_os("DESKPANEL_LOG_STATS").is_some() {
            crate::log!("STATS {s:?}");
        }
        *LATEST_STATS.lock().unwrap() = Some(s.clone());
        let _ = self.app.emit_to(crate::app::panel_label().as_str(), "stats", s);
        // WMI is the costliest call in a tick: poll brightness every 3 s (external changes only).
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % 3 == 0 {
            if let Some(b) = self.bright.as_ref() {
                if let Ok(v) = b.get() {
                    self.brightness = Some(v);
                }
            }
        }
        if let Some(a) = self.audio.as_mut() {
            a.follow_default_device();
        }
    }

    fn check_timer(&mut self) {
        if self.timers.timer_check_done() {
            crate::log!("controls: timer finished at {}", timers::now_ms());
            notify_timer_done(&self.app);
            let _ = self.app.emit_to(crate::app::panel_label().as_str(), "timer-done", ());
        }
    }

    /// Screenshot/demo mode (`DESKPANEL_DEMO=1`): real controls, but sample text where the real
    /// value would be personal (network name, what you are listening to).
    fn demo(c: &mut Controls) {
        c.wifi_name = c.wifi.filter(|on| *on).map(|_| "Home Wi-Fi".to_string());
        c.volume = Some(0.65);
        c.muted = false;
        c.brightness = Some(70);
        c.media = Some(media::MediaInfo {
            title: "Midnight Drive".into(),
            artist: "The Demo Orchestra".into(),
            app: "Spotify.exe".into(),
            playing: true,
            position_ms: 78_000,
            duration_ms: 214_000,
            updated_epoch_ms: timers::now_ms() as i64,
            art_id: "demo".into(),
        });
    }

    fn snapshot(&mut self) -> Controls {
        let (volume, muted) = match self.audio.as_ref() {
            Some(a) => (a.volume().ok(), a.muted().unwrap_or(false)),
            None => (None, false),
        };
        let wifi = self.radios.as_ref().and_then(|r| r.state(Which::Wifi));
        let mut snap = Controls {
            volume,
            muted,
            brightness: self.brightness,
            wifi,
            wifi_name: if wifi == Some(true) { radios::wifi_name() } else { None },
            bluetooth: self.radios.as_ref().and_then(|r| r.state(Which::Bluetooth)),
            dark: theme::is_dark(),
            power: power::get(),
            media: self.media.as_mut().and_then(|m| m.info()),
            timer: self.timers.timer.clone(),
            stopwatch: self.timers.sw.clone(),
            error: self.error.clone(),
        };
        if std::env::var_os("DESKPANEL_DEMO").is_some() {
            Self::demo(&mut snap);
        }
        snap
    }

    fn publish(&mut self) {
        let snap = self.snapshot();
        if let Some(m) = self.media.as_ref() {
            // demo mode: never show the real artwork of whatever is playing
            *ART.lock().unwrap() = if std::env::var_os("DESKPANEL_DEMO").is_some() { None } else { m.art() };
        }
        if self.last.as_ref() != Some(&snap) {
            crate::log!(
                "CTL vol={:?} mute={} bri={:?} wifi={:?} bt={:?} dark={} power={} media={:?} timer={:?} sw_running={} laps={}",
                snap.volume.map(|v| (v * 1000.0).round() / 1000.0),
                snap.muted,
                snap.brightness,
                snap.wifi,
                snap.bluetooth,
                snap.dark,
                snap.power,
                snap.media.as_ref().map(|m| format!("{} - {} playing={}", m.title, m.artist, m.playing)),
                snap.timer,
                snap.stopwatch.running,
                snap.stopwatch.laps.len()
            );
            *LATEST.lock().unwrap() = Some(snap.clone());
            let _ = self.app.emit_to(crate::app::panel_label().as_str(), "controls", snap.clone());
            let _ = self.app.emit_to("pill", "controls", snap.clone());
            self.last = Some(snap.clone());
        }
        // Pill: something "live" = timer/stopwatch running or media playing.
        let active = self.timers.active() || snap.media.as_ref().map(|m| m.playing).unwrap_or(false);
        if self.last_activity != Some(active) {
            self.last_activity = Some(active);
            crate::app::dispatch_main(deskpanel_core::state::Input::Activity(active));
        }
    }
}

fn notify_timer_done(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title("Timer done").body("Your DeskPanel timer has finished.").show() {
        crate::log!("controls: toast failed: {e}");
    }
    unsafe {
        use windows::core::w;
        use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
        let _ = PlaySoundW(w!("SystemNotification"), None, SND_ALIAS | SND_ASYNC);
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn ctl(cmd: Cmd) {
    send(Msg::Cmd(cmd));
}

#[tauri::command]
pub fn get_controls() -> Option<Controls> {
    LATEST.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_stats() -> Option<StatsInfo> {
    LATEST_STATS.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_media_art() -> Option<String> {
    ART.lock().unwrap().clone()
}
