//! Timer + stopwatch live in Rust, not in the webview: the webview is suspended while the panel
//! is covered/hidden, and a timer must still fire then. Times are Unix-epoch ms so the UI can
//! render locally without asking Rust every frame.
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TimerInfo {
    Idle { total_ms: u64 },
    Running { total_ms: u64, end_epoch_ms: u64 },
    Paused { total_ms: u64, remaining_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StopwatchInfo {
    pub running: bool,
    /// While running: elapsed = now - start_epoch_ms + banked_ms.
    pub start_epoch_ms: Option<u64>,
    pub banked_ms: u64,
    pub laps: Vec<u64>,
}

pub struct Timers {
    pub timer: TimerInfo,
    pub sw: StopwatchInfo,
}

impl Default for Timers {
    fn default() -> Self {
        Timers {
            timer: TimerInfo::Idle { total_ms: 25 * 60_000 },
            sw: StopwatchInfo { running: false, start_epoch_ms: None, banked_ms: 0, laps: vec![] },
        }
    }
}

impl Timers {
    pub fn timer_start(&mut self, ms: u64) {
        let ms = ms.clamp(1_000, 24 * 3_600_000);
        self.timer = TimerInfo::Running { total_ms: ms, end_epoch_ms: now_ms() + ms };
    }
    pub fn timer_pause(&mut self) {
        if let TimerInfo::Running { total_ms, end_epoch_ms } = self.timer {
            self.timer = TimerInfo::Paused { total_ms, remaining_ms: end_epoch_ms.saturating_sub(now_ms()) };
        }
    }
    pub fn timer_resume(&mut self) {
        if let TimerInfo::Paused { total_ms, remaining_ms } = self.timer {
            self.timer = TimerInfo::Running { total_ms, end_epoch_ms: now_ms() + remaining_ms };
        }
    }
    pub fn timer_reset(&mut self) {
        let total_ms = match self.timer {
            TimerInfo::Idle { total_ms } | TimerInfo::Running { total_ms, .. } | TimerInfo::Paused { total_ms, .. } => total_ms,
        };
        self.timer = TimerInfo::Idle { total_ms };
    }
    /// Returns true exactly once, when a running timer reaches zero.
    pub fn timer_check_done(&mut self) -> bool {
        if let TimerInfo::Running { total_ms, end_epoch_ms } = self.timer {
            if now_ms() >= end_epoch_ms {
                self.timer = TimerInfo::Idle { total_ms };
                return true;
            }
        }
        false
    }
    pub fn ms_until_timer_end(&self) -> Option<u64> {
        match self.timer {
            TimerInfo::Running { end_epoch_ms, .. } => Some(end_epoch_ms.saturating_sub(now_ms())),
            _ => None,
        }
    }

    fn sw_elapsed(&self) -> u64 {
        self.sw.banked_ms + self.sw.start_epoch_ms.map(|s| now_ms().saturating_sub(s)).unwrap_or(0)
    }
    pub fn sw_start(&mut self) {
        if !self.sw.running {
            self.sw.running = true;
            self.sw.start_epoch_ms = Some(now_ms());
        }
    }
    pub fn sw_stop(&mut self) {
        if self.sw.running {
            self.sw.banked_ms = self.sw_elapsed();
            self.sw.running = false;
            self.sw.start_epoch_ms = None;
        }
    }
    pub fn sw_lap(&mut self) {
        if self.sw.running {
            let e = self.sw_elapsed();
            self.sw.laps.push(e);
        }
    }
    pub fn sw_reset(&mut self) {
        self.sw = StopwatchInfo { running: false, start_epoch_ms: None, banked_ms: 0, laps: vec![] };
    }

    pub fn active(&self) -> bool {
        matches!(self.timer, TimerInfo::Running { .. }) || self.sw.running
    }
}
