//! The panel's show/hide state machine.
//!
//! Pure logic: `Machine::step(Input) -> Vec<Effect>`. The Win32/Tauri shell layer turns real events
//! (hotkey, clicks, WinEvent hooks, animation callbacks) into `Input`s and executes the returned
//! `Effect`s in order. Nothing in here touches the OS, which is what makes every transition testable.
//!
//! Timing rule for animations:
//! - transitions that make the panel *more* visible (Appear, Pop) change `panel` when they START;
//! - transitions that make it *less* visible (Drop, Disappear) change `panel` when they FINISH,
//!   so the panel counts as visible for the whole time it is on screen.
//!
//! While an animation runs, "active" inputs (hotkey, Esc, tray, close) are queued (FIFO, max
//! `MAX_QUEUE`) and replayed after `AnimDone`. "Passive" inputs (occlusion, focus, activity,
//! fullscreen, pill setting, shell restart) are applied immediately.

use std::collections::VecDeque;

/// Opaque id of a top-level window (an HWND as usize in the shell layer).
pub type WindowId = usize;

pub const MAX_QUEUE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Closed with × or the tray. Not on screen. Persisted across restarts.
    Hidden,
    /// Child of the desktop window: under every app, visible wherever the desktop is.
    Desktop,
    /// Top-level, topmost, above apps (summoned by the shortcut/tray).
    Popped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anim {
    Appear,
    Disappear,
    Pop,
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pill {
    Off,
    Shown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    // ---- active (queued while animating) ----
    /// Win+Alt+Space. `fg` = foreground window at press time (never the panel itself).
    /// `fg_is_desktop` = the desktop, the taskbar or nothing is in front.
    Shortcut { fg: Option<WindowId>, fg_is_desktop: bool },
    /// Only delivered while the popped panel has keyboard focus (the webview's keydown).
    Esc,
    CloseClicked,
    TrayShow,
    TrayHide,
    TrayPop { fg: Option<WindowId> },
    // ---- passive (applied immediately) ----
    AnimDone(Anim),
    /// An app covers the panel (only meaningful in `Desktop`).
    Covered(bool),
    /// The panel window currently has keyboard focus.
    Focus(bool),
    /// A fullscreen app is in front.
    Fullscreen(bool),
    /// A timer/stopwatch is running or media is playing.
    Activity(bool),
    PillEnabled(bool),
    /// explorer.exe restarted (TaskbarCreated): the desktop window was re-created.
    ShellRestarted,
}

impl Input {
    fn is_active(&self) -> bool {
        matches!(
            self,
            Input::Shortcut { .. } | Input::Esc | Input::CloseClicked | Input::TrayShow | Input::TrayHide | Input::TrayPop { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Make the panel a no-activate child of the desktop window.
    ReparentToDesktop,
    /// Grab what is behind the panel for the pop-over glass. Always emitted BEFORE `ReparentToTopmost`.
    CaptureSnapshot,
    /// Make the panel a top-level topmost window.
    ReparentToTopmost,
    Show,
    Hide,
    /// Give the panel keyboard focus (allowed right after a hotkey/tray click).
    Activate,
    /// Hand focus back to the window that had it before the pop.
    RestoreForeground(WindowId),
    PlayAnim(Anim),
    PersistHidden(bool),
    SetPill(Pill),
    /// Suspend the webview while nobody can see it (idle-CPU budget).
    SuspendPanel(bool),
}

#[derive(Debug, Clone)]
pub struct Machine {
    panel: Panel,
    animating: Option<Anim>,
    queue: VecDeque<Input>,
    prev_fg: Option<WindowId>,
    covered: bool,
    focused: bool,
    fullscreen: bool,
    activity: bool,
    pill_enabled: bool,
    pill: Pill,
    suspended: bool,
}

/// Whether the panel can be seen by the user right now.
pub fn panel_visible(panel: Panel, covered: bool) -> bool {
    match panel {
        Panel::Hidden => false,
        Panel::Desktop => !covered,
        Panel::Popped => true,
    }
}

/// Should the Dynamic-Island-style pill be on screen?
///
/// Spec (SPEC.md §2.2, L2/L3/L5): show it only when something is live (timer running or media
/// playing) AND the user can't currently see the panel, never over a fullscreen app, and never
/// when the user switched the pill off.
pub fn derive_pill(panel_visible: bool, activity: bool, fullscreen: bool, enabled: bool) -> Pill {
    if !enabled || fullscreen || !activity || panel_visible {
        return Pill::Off;
    }
    Pill::Shown
}

impl Machine {
    /// Build the machine from persisted state. Returns the effects needed to reach it on screen.
    pub fn new(start_hidden: bool, pill_enabled: bool) -> (Self, Vec<Effect>) {
        let mut m = Machine {
            panel: Panel::Hidden,
            animating: None,
            queue: VecDeque::new(),
            prev_fg: None,
            covered: false,
            focused: false,
            fullscreen: false,
            activity: false,
            pill_enabled,
            pill: Pill::Off,
            suspended: false,
        };
        let mut fx = Vec::new();
        if start_hidden {
            m.suspended = true;
            fx.push(Effect::SuspendPanel(true));
        } else {
            m.appear(&mut fx);
        }
        m.sync_derived(&mut fx);
        (m, fx)
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }
    pub fn pill(&self) -> Pill {
        self.pill
    }
    pub fn animating(&self) -> Option<Anim> {
        self.animating
    }
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        let mut fx = Vec::new();
        match input {
            Input::Covered(c) => self.covered = c,
            Input::Focus(f) => self.focused = f,
            Input::Fullscreen(f) => self.fullscreen = f,
            Input::Activity(a) => self.activity = a,
            Input::PillEnabled(e) => self.pill_enabled = e,
            Input::ShellRestarted => {
                // The old desktop window is gone; anything parented to it must be re-anchored.
                if self.panel == Panel::Desktop {
                    fx.push(Effect::ReparentToDesktop);
                }
            }
            Input::AnimDone(a) => {
                // Ignore stale/duplicate completions.
                if self.animating == Some(a) {
                    self.animating = None;
                    self.finish(a, &mut fx);
                    self.drain(&mut fx);
                }
            }
            active => {
                debug_assert!(active.is_active());
                if self.animating.is_some() {
                    if self.queue.len() < MAX_QUEUE {
                        self.queue.push_back(active);
                    }
                } else {
                    self.apply(active, &mut fx);
                }
            }
        }
        self.sync_derived(&mut fx);
        fx
    }

    fn drain(&mut self, fx: &mut Vec<Effect>) {
        while self.animating.is_none() {
            let Some(next) = self.queue.pop_front() else { break };
            self.apply(next, fx);
        }
    }

    fn apply(&mut self, input: Input, fx: &mut Vec<Effect>) {
        use Input::*;
        use Panel::*;
        match (self.panel, input) {
            (Hidden, Shortcut { fg_is_desktop: true, .. }) | (Hidden, TrayShow) => self.appear(fx),
            (Hidden, Shortcut { fg, fg_is_desktop: false }) | (Hidden, TrayPop { fg }) => self.pop(fg, fx),

            (Desktop, Shortcut { fg, .. }) | (Desktop, TrayPop { fg }) => self.pop(fg, fx),
            (Desktop, CloseClicked) | (Desktop, TrayHide) => self.start(Anim::Disappear, fx),

            (Popped, Shortcut { .. }) | (Popped, Esc) => self.start(Anim::Drop, fx),
            (Popped, CloseClicked) | (Popped, TrayHide) => self.start(Anim::Disappear, fx),

            // Everything else is a no-op: TrayShow while shown, Esc when not popped,
            // TrayPop while popped, Close while hidden, ...
            _ => {}
        }
    }

    fn start(&mut self, a: Anim, fx: &mut Vec<Effect>) {
        self.animating = Some(a);
        fx.push(Effect::PlayAnim(a));
    }

    fn appear(&mut self, fx: &mut Vec<Effect>) {
        self.panel = Panel::Desktop;
        fx.extend([Effect::ReparentToDesktop, Effect::Show, Effect::PersistHidden(false)]);
        self.start(Anim::Appear, fx);
    }

    fn pop(&mut self, fg: Option<WindowId>, fx: &mut Vec<Effect>) {
        let from_hidden = self.panel == Panel::Hidden;
        self.prev_fg = fg;
        self.panel = Panel::Popped;
        // Animation first: the webview starts animating while the shell re-parents the window
        // (~25 ms cross-process), which keeps hotkey -> motion under 50 ms (CHECKLIST B1).
        // CaptureSnapshot only arms the capture; the shell takes it once the window is top-level.
        self.start(Anim::Pop, fx);
        fx.extend([Effect::CaptureSnapshot, Effect::ReparentToTopmost, Effect::Show, Effect::Activate]);
        if from_hidden {
            fx.push(Effect::PersistHidden(false));
        }
        self.focused = true;
    }

    /// Hand focus back only if the panel still owns it. If the user already clicked into another
    /// app, stealing focus back to the old window would be wrong.
    fn restore_focus(&mut self, fx: &mut Vec<Effect>) {
        if self.focused {
            if let Some(w) = self.prev_fg {
                fx.push(Effect::RestoreForeground(w));
            }
            self.focused = false;
        }
        self.prev_fg = None;
    }

    fn finish(&mut self, a: Anim, fx: &mut Vec<Effect>) {
        match a {
            Anim::Appear | Anim::Pop => {}
            Anim::Drop => {
                fx.push(Effect::ReparentToDesktop);
                self.restore_focus(fx);
                self.panel = Panel::Desktop;
            }
            Anim::Disappear => {
                let was_popped = self.panel == Panel::Popped;
                fx.push(Effect::Hide);
                if was_popped {
                    fx.push(Effect::ReparentToDesktop);
                    self.restore_focus(fx);
                }
                fx.push(Effect::PersistHidden(true));
                self.panel = Panel::Hidden;
            }
        }
    }

    fn sync_derived(&mut self, fx: &mut Vec<Effect>) {
        let visible = panel_visible(self.panel, self.covered);
        let pill = derive_pill(visible, self.activity, self.fullscreen, self.pill_enabled);
        if pill != self.pill {
            self.pill = pill;
            fx.push(Effect::SetPill(pill));
        }
        let suspend = !visible;
        if suspend != self.suspended {
            self.suspended = suspend;
            fx.push(Effect::SuspendPanel(suspend));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Effect::*;

    const CHROME: WindowId = 0xC0;
    const VSCODE: WindowId = 0xC1;

    fn shortcut_over_app() -> Input {
        Input::Shortcut { fg: Some(CHROME), fg_is_desktop: false }
    }
    fn shortcut_on_desktop() -> Input {
        Input::Shortcut { fg: None, fg_is_desktop: true }
    }

    /// Feed AnimDone for whatever is animating until idle; return all effects.
    fn settle(m: &mut Machine) -> Vec<Effect> {
        let mut all = vec![];
        for _ in 0..16 {
            match m.animating() {
                Some(a) => all.extend(m.step(Input::AnimDone(a))),
                None => break,
            }
        }
        assert!(m.animating().is_none(), "did not settle");
        all
    }

    fn machine_in(p: Panel) -> Machine {
        let (mut m, _) = Machine::new(p == Panel::Hidden, true);
        settle(&mut m);
        if p == Panel::Popped {
            m.step(shortcut_over_app());
            settle(&mut m);
        }
        assert_eq!(m.panel(), p);
        m
    }

    // ---------- startup ----------

    #[test]
    fn starts_on_desktop_with_appear() {
        let (m, fx) = Machine::new(false, true);
        assert_eq!(m.panel(), Panel::Desktop);
        assert_eq!(&fx[..4], &[ReparentToDesktop, Show, PersistHidden(false), PlayAnim(Anim::Appear)]);
    }

    #[test]
    fn starts_hidden_and_suspended_when_persisted_hidden() {
        let (m, fx) = Machine::new(true, true);
        assert_eq!(m.panel(), Panel::Hidden);
        assert_eq!(fx, vec![SuspendPanel(true)]);
    }

    // ---------- full transition table: every state x every active input ----------

    #[test]
    fn transition_table() {
        use Input::*;
        use Panel::*;
        let inputs: Vec<(&str, Input)> = vec![
            ("shortcut/app", shortcut_over_app()),
            ("shortcut/desktop", shortcut_on_desktop()),
            ("esc", Esc),
            ("close", CloseClicked),
            ("trayShow", TrayShow),
            ("trayHide", TrayHide),
            ("trayPop", TrayPop { fg: Some(CHROME) }),
        ];
        // expected end state after settling, per (start, input)
        let expect = |s: Panel, name: &str| -> Panel {
            match (s, name) {
                (Hidden, "shortcut/app") | (Hidden, "trayPop") => Popped,
                (Hidden, "shortcut/desktop") | (Hidden, "trayShow") => Desktop,
                (Hidden, _) => Hidden,
                (Desktop, "shortcut/app") | (Desktop, "shortcut/desktop") | (Desktop, "trayPop") => Popped,
                (Desktop, "close") | (Desktop, "trayHide") => Hidden,
                (Desktop, _) => Desktop,
                (Popped, "shortcut/app") | (Popped, "shortcut/desktop") | (Popped, "esc") => Desktop,
                (Popped, "close") | (Popped, "trayHide") => Hidden,
                (Popped, _) => Popped,
            }
        };
        for start in [Hidden, Desktop, Popped] {
            for (name, input) in &inputs {
                let mut m = machine_in(start);
                m.step(*input);
                settle(&mut m);
                assert_eq!(m.panel(), expect(start, name), "{start:?} + {name}");
            }
        }
    }

    // ---------- effect ordering / details ----------

    #[test]
    fn pop_captures_snapshot_before_going_topmost() {
        let mut m = machine_in(Panel::Desktop);
        let fx = m.step(shortcut_over_app());
        let snap = fx.iter().position(|e| *e == CaptureSnapshot).unwrap();
        let top = fx.iter().position(|e| *e == ReparentToTopmost).unwrap();
        assert!(snap < top, "{fx:?}");
        assert!(fx.contains(&Activate));
    }

    #[test]
    fn pop_starts_the_animation_before_reparenting() {
        let mut m = machine_in(Panel::Desktop);
        let fx = m.step(shortcut_over_app());
        assert_eq!(fx[0], PlayAnim(Anim::Pop), "{fx:?}");
    }

    #[test]
    fn panel_counts_as_visible_until_drop_animation_finishes() {
        let mut m = machine_in(Panel::Popped);
        m.step(Input::Covered(true)); // an app would cover it once it is back on the desktop
        let fx = m.step(Input::Esc);
        assert_eq!(fx, vec![PlayAnim(Anim::Drop)]);
        assert_eq!(m.panel(), Panel::Popped, "still on screen while animating");
        let fx = m.step(Input::AnimDone(Anim::Drop));
        assert!(fx.contains(&ReparentToDesktop));
        assert_eq!(m.panel(), Panel::Desktop);
        assert!(fx.contains(&SuspendPanel(true)), "covered on desktop -> suspend: {fx:?}");
    }

    #[test]
    fn drop_restores_focus_when_panel_still_focused() {
        let mut m = machine_in(Panel::Popped);
        m.step(Input::Esc);
        let fx = settle(&mut m);
        assert!(fx.contains(&RestoreForeground(CHROME)), "{fx:?}");
    }

    #[test]
    fn drop_does_not_steal_focus_after_user_clicked_elsewhere() {
        let mut m = machine_in(Panel::Popped);
        m.step(Input::Focus(false)); // user clicked into VS Code while the panel stayed on top
        m.step(Input::Shortcut { fg: Some(VSCODE), fg_is_desktop: false });
        let fx = settle(&mut m);
        assert!(!fx.iter().any(|e| matches!(e, RestoreForeground(_))), "{fx:?}");
        assert_eq!(m.panel(), Panel::Desktop);
    }

    #[test]
    fn close_from_popped_hides_reanchors_and_restores_focus() {
        let mut m = machine_in(Panel::Popped);
        m.step(Input::CloseClicked);
        let fx = settle(&mut m);
        assert_eq!(fx[..4], [Hide, ReparentToDesktop, RestoreForeground(CHROME), PersistHidden(true)], "{fx:?}");
        assert_eq!(m.panel(), Panel::Hidden);
    }

    #[test]
    fn close_persists_hidden_and_show_unpersists() {
        let mut m = machine_in(Panel::Desktop);
        m.step(Input::CloseClicked);
        assert!(settle(&mut m).contains(&PersistHidden(true)));
        let fx = m.step(Input::TrayShow);
        assert!(fx.contains(&PersistHidden(false)));
    }

    #[test]
    fn esc_is_ignored_unless_popped() {
        for p in [Panel::Hidden, Panel::Desktop] {
            let mut m = machine_in(p);
            assert!(m.step(Input::Esc).is_empty(), "{p:?}");
        }
    }

    // ---------- inputs during animation ----------

    #[test]
    fn inputs_during_animation_are_queued_then_replayed() {
        let mut m = machine_in(Panel::Desktop);
        m.step(shortcut_over_app()); // Pop starts
        let fx = m.step(shortcut_over_app()); // pressed again mid-animation
        assert!(fx.is_empty(), "queued, no effects yet: {fx:?}");
        assert_eq!(m.queued(), 1);
        let fx = m.step(Input::AnimDone(Anim::Pop));
        assert!(fx.contains(&PlayAnim(Anim::Drop)), "queued shortcut replayed: {fx:?}");
        settle(&mut m);
        assert_eq!(m.panel(), Panel::Desktop);
    }

    #[test]
    fn queue_is_bounded() {
        let mut m = machine_in(Panel::Desktop);
        m.step(shortcut_over_app());
        for _ in 0..50 {
            m.step(shortcut_over_app());
        }
        assert_eq!(m.queued(), MAX_QUEUE);
        settle(&mut m); // must terminate
    }

    #[test]
    fn stale_anim_done_is_ignored() {
        let mut m = machine_in(Panel::Desktop);
        assert!(m.step(Input::AnimDone(Anim::Pop)).is_empty());
        assert_eq!(m.panel(), Panel::Desktop);
    }

    #[test]
    fn passive_inputs_apply_immediately_during_animation() {
        let mut m = machine_in(Panel::Desktop);
        m.step(Input::CloseClicked); // Disappear running
        m.step(Input::PillEnabled(false));
        assert_eq!(m.queued(), 0);
    }

    // ---------- shell restart ----------

    #[test]
    fn shell_restart_reanchors_only_when_on_desktop() {
        assert_eq!(machine_in(Panel::Desktop).step(Input::ShellRestarted), vec![ReparentToDesktop]);
        assert!(machine_in(Panel::Popped).step(Input::ShellRestarted).is_empty());
        assert!(machine_in(Panel::Hidden).step(Input::ShellRestarted).is_empty());
    }

    // ---------- suspend (idle CPU) ----------

    #[test]
    fn webview_suspended_exactly_when_not_visible() {
        let mut m = machine_in(Panel::Desktop);
        assert_eq!(m.step(Input::Covered(true)), vec![SuspendPanel(true)]);
        assert_eq!(m.step(Input::Covered(false)), vec![SuspendPanel(false)]);
        m.step(Input::Covered(true));
        let fx = m.step(shortcut_over_app()); // popping makes it visible again immediately
        assert!(fx.contains(&SuspendPanel(false)), "{fx:?}");
    }

    // ---------- pill (C2): every combination of the four inputs ----------

    #[test]
    fn derive_pill_truth_table() {
        for visible in [false, true] {
            for activity in [false, true] {
                for fullscreen in [false, true] {
                    for enabled in [false, true] {
                        let want = if !visible && activity && !fullscreen && enabled { Pill::Shown } else { Pill::Off };
                        assert_eq!(
                            derive_pill(visible, activity, fullscreen, enabled),
                            want,
                            "visible={visible} activity={activity} fullscreen={fullscreen} enabled={enabled}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn pill_follows_machine_state() {
        let mut m = machine_in(Panel::Desktop);
        assert!(m.step(Input::Activity(true)).is_empty(), "panel visible: no pill");
        assert!(m.step(Input::Covered(true)).contains(&SetPill(Pill::Shown)));
        assert_eq!(m.step(Input::Fullscreen(true)), vec![SetPill(Pill::Off)]);
        m.step(Input::Fullscreen(false));
        assert_eq!(m.pill(), Pill::Shown);
        // popping over the app makes the panel visible -> pill goes away
        assert!(m.step(shortcut_over_app()).contains(&SetPill(Pill::Off)));
    }
}
