//! Now Playing via Windows' Global System Media Transport Controls: the same source as the
//! media flyout, so it covers the Spotify app, YouTube in Chrome/Edge, etc. No Spotify login.
use std::sync::mpsc::Sender;

use base64::Engine;
use windows::core::Result;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::*;
use windows::Storage::Streams::DataReader;

use super::Msg;

type Manager = GlobalSystemMediaTransportControlsSessionManager;
type Session = GlobalSystemMediaTransportControlsSession;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub app: String,
    pub playing: bool,
    /// Position at `updated_epoch_ms`; the UI extrapolates while playing.
    pub position_ms: i64,
    pub duration_ms: i64,
    pub updated_epoch_ms: i64,
    /// Changes when the artwork changes; fetch it with `get_media_art`.
    pub art_id: String,
}

pub struct Media {
    mgr: Manager,
    tx: Sender<Msg>,
    hooked: Option<(Session, [i64; 3])>,
    art_id: String,
    art: Option<String>,
}

/// 100 ns ticks since 1601 -> Unix ms.
fn filetime_to_unix_ms(t: i64) -> i64 {
    (t - 116_444_736_000_000_000) / 10_000
}

impl Media {
    pub fn new(tx: Sender<Msg>) -> Result<Self> {
        let mgr = Manager::RequestAsync()?.join()?;
        let t = tx.clone();
        mgr.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::MediaSessionChanged);
            Ok(())
        }))?;
        let mut m = Media { mgr, tx, hooked: None, art_id: String::new(), art: None };
        m.rehook();
        Ok(m)
    }

    /// Subscribe to the current session's change events (and drop the previous session's).
    pub fn rehook(&mut self) {
        if let Some((s, [a, b, c])) = self.hooked.take() {
            let _ = s.RemoveMediaPropertiesChanged(a);
            let _ = s.RemovePlaybackInfoChanged(b);
            let _ = s.RemoveTimelinePropertiesChanged(c);
        }
        let Ok(s) = self.mgr.GetCurrentSession() else { return };
        // One handler per event: each event has its own argument type.
        let (t1, t2, t3) = (self.tx.clone(), self.tx.clone(), self.tx.clone());
        let a = s.MediaPropertiesChanged(&TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(move |_, _| {
            let _ = t1.send(Msg::OsChanged);
            Ok(())
        }));
        let b = s.PlaybackInfoChanged(&TypedEventHandler::<Session, PlaybackInfoChangedEventArgs>::new(move |_, _| {
            let _ = t2.send(Msg::OsChanged);
            Ok(())
        }));
        let c = s.TimelinePropertiesChanged(&TypedEventHandler::<Session, TimelinePropertiesChangedEventArgs>::new(move |_, _| {
            let _ = t3.send(Msg::OsChanged);
            Ok(())
        }));
        if let (Ok(a), Ok(b), Ok(c)) = (a, b, c) {
            self.hooked = Some((s, [a, b, c]));
        }
    }

    fn session(&self) -> Option<Session> {
        self.mgr.GetCurrentSession().ok()
    }

    pub fn info(&mut self) -> Option<MediaInfo> {
        let s = self.session()?;
        let props = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
        let title = props.Title().ok()?.to_string();
        let artist = props.Artist().map(|a| a.to_string()).unwrap_or_default();
        let app = s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default();
        let playing = s
            .GetPlaybackInfo()
            .and_then(|p| p.PlaybackStatus())
            .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
            .unwrap_or(false);
        let (mut position_ms, mut duration_ms, mut updated_epoch_ms) = (0, 0, 0);
        if let Ok(tl) = s.GetTimelineProperties() {
            position_ms = tl.Position().map(|p| p.Duration / 10_000).unwrap_or(0);
            let start = tl.StartTime().map(|p| p.Duration / 10_000).unwrap_or(0);
            duration_ms = tl.EndTime().map(|p| p.Duration / 10_000).unwrap_or(0) - start;
            updated_epoch_ms = tl.LastUpdatedTime().map(|d| filetime_to_unix_ms(d.UniversalTime)).unwrap_or(0);
        }
        let art_id = format!("{app}|{title}|{artist}");
        if art_id != self.art_id {
            self.art_id = art_id.clone();
            self.art = load_art(&props);
        }
        Some(MediaInfo { title, artist, app, playing, position_ms, duration_ms, updated_epoch_ms, art_id })
    }

    pub fn art(&self) -> Option<String> {
        self.art.clone()
    }

    pub fn command(&self, cmd: &str) -> std::result::Result<(), String> {
        let s = self.session().ok_or("no media session")?;
        let op = match cmd {
            "play_pause" => s.TryTogglePlayPauseAsync(),
            "next" => s.TrySkipNextAsync(),
            "prev" => s.TrySkipPreviousAsync(),
            _ => return Err(format!("unknown media command {cmd}")),
        };
        let ok = op.map_err(|e| e.to_string())?.join().map_err(|e| e.to_string())?;
        if ok { Ok(()) } else { Err(format!("{cmd} refused by the player")) }
    }
}

fn load_art(props: &GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<String> {
    let r = props.Thumbnail().ok()?;
    let stream = r.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 4_000_000 {
        return None;
    }
    let mime = stream.ContentType().map(|s| s.to_string()).unwrap_or_else(|_| "image/jpeg".into());
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    reader.LoadAsync(size as u32).ok()?.join().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    let mime = if mime.is_empty() { "image/jpeg".into() } else { mime };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf)))
}
