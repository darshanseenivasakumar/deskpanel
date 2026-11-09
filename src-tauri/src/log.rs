//! Tiny append-only event log: %APPDATA%\DeskPanel\logs\events.log (evidence for CHECKLIST items).
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static FILE: OnceLock<Mutex<File>> = OnceLock::new();
static T0: OnceLock<Instant> = OnceLock::new();

pub fn init() {
    T0.get_or_init(Instant::now);
    let dir = crate::config::data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("events.log");
    // Keep the log bounded: start fresh when it grows past 5 MB.
    let truncate = std::fs::metadata(&path).map(|m| m.len() > 5_000_000).unwrap_or(false);
    if let Ok(f) = OpenOptions::new().create(true).append(!truncate).write(true).truncate(truncate).open(&path) {
        let _ = FILE.set(Mutex::new(f));
    }
}

pub fn write(s: &str) {
    let t = T0.get().map(|t| t.elapsed().as_millis()).unwrap_or(0);
    let line = format!("{t:>9} {s}");
    #[cfg(debug_assertions)]
    eprintln!("{line}");
    if let Some(f) = FILE.get() {
        if let Ok(mut f) = f.lock() {
            let _ = writeln!(f, "{line}");
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::log::write(&format!($($t)*)) };
}
