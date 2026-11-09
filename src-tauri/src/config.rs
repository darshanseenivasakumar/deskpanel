//! Persisted settings + state: %APPDATA%\DeskPanel\config.json
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Panel top-left in physical pixels (screen coords). None = default spot (right edge, centred).
    pub position: Option<(i32, i32)>,
    /// Closed with ×: stays hidden across restarts until shortcut/tray.
    pub hidden: bool,
    pub pill_enabled: bool,
    /// Start with Windows (HKCU Run). On by default (SPEC S3).
    pub autostart: bool,
    /// Glass blur radius in CSS px (like CSS `blur()`).
    pub blur_px: f32,
    /// Glass saturation boost (1.0 = none).
    pub saturate: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config { position: None, hidden: false, pill_enabled: true, autostart: true, blur_px: 40.0, saturate: 1.8 }
    }
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("DeskPanel")
}

fn path() -> PathBuf {
    data_dir().join("config.json")
}

impl Config {
    pub fn load() -> Config {
        match std::fs::read_to_string(path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                crate::log!("config: parse error ({e}), using defaults");
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    /// Atomic-ish save: write a temp file then rename, so a crash never leaves half a file.
    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("config.json.tmp");
        if let Ok(s) = serde_json::to_string_pretty(self) {
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, path());
            }
        }
    }
}
