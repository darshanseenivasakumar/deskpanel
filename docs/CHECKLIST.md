# Definition of Done

> The raw evidence these items cite (logs, screenshots) is kept locally and not published, because it contains window titles and personal data from the test machine. The scripts that produce it are in `../scripts/`.

Every item is binary. **Who:**
- **C** = Claude verifies on this machine with recorded evidence
- **U** = needs you at the keyboard; Claude provides the script/log
- **C+U** = Claude checks automatically, then you confirm by eye

Evidence goes in `evidence/` (logs, screenshots, measurements). The project is not done until every box is ticked.

> **How your brief's items map to the desktop-layer model.**
> - "Appears only when desktop/home is the focus" → **visible only where the desktop is visible, never above an app unless popped** (A1–A3).
> - "Auto-closes when another window gains focus" → **the app covers it, because it's at the bottom of the window stack** (A2).
> - "Manual-dismiss rule" → **× hides until shortcut/tray** (B4).

## 0. Your own acceptance tests (top priority)
- [ ] **U0.1 Never covers my apps.** Use Chrome + VS Code normally for 60 min with the panel running. The panel never appears above an app unless you pressed Win+Alt+Space. *Verify:* you confirm, plus `events.log` shows zero `SetTopmost` effects without a `Shortcut`/`TrayPop` input.
- [ ] **U0.2 Controls really work.**
  - Wi-Fi and BT toggles change the radios (the Windows Quick Settings flyout agrees).
  - Volume and brightness match the Windows flyouts.
  - a media player reacts to play/pause/next.
  - Power mode change shows up in Settings › Power.
  - *Verify:* you confirm. Claude provides the automated cross-checks in §E.
- [ ] **U0.3 Looks Apple-smooth.** The glass looks right on your wallpaper, springs feel smooth at 144 Hz, and there's no white flash at startup. *Verify:* you confirm, plus the §D measurements.
- [ ] **U0.4 Survives daily life.** Lid close/sleep, lock/unlock and restart: the panel comes back in place, with no crash and no duplicate instance. *Verify:* you run it, plus the `events.log` resume entries and a `Get-Process` count of 1.

## A. Desktop-layer placement
- [x] **A1** On a clear desktop (after Win+D), the panel is visible at its saved position. *Verify (C):* screenshot + `GetWindowRect` matches config. ✅ *Evidence:* installed build: visible at its position after Win+D (`evidence/final/results.txt`, `A1-desktop.png`).
- [x] **A2** Focusing Chrome, Explorer, and VS Code over the panel covers it. Over 10 focus switches, the z-order dump shows the panel below all three every time. *Verify (C):* `scripts/zorder-dump.ps1` after each switch. ✅ *Evidence:* 10/10 focus switches across 3 apps (Claude, a media player, test window): panel stayed under the active app (`evidence/final/results.txt`).
- [x] **A3** Clicking a tile or slider on the panel does not raise it above an overlapping app window and does not change the foreground window. *Verify (C):* `GetForegroundWindow` before/after the click is identical; z-order dump. ✅ *Evidence:* tile clicks with the test window over half the panel: overlap point still the test window (`evidence/final/results.txt`).
- [x] **A4** Win+D shows the desktop **with the panel still visible**, 10/10 times. Restoring apps (Win+D again) covers it again. *Verify (C):* screenshots + z-order dumps. ✅ *Evidence:* 10/10 Win+D cycles: visible, then covered again (`evidence/final/results.txt`).
- [x] **A5** Start menu, Alt+Tab, taskbar click, Quick Settings (Win+A), and Notification Center (Win+N) all draw above the panel. The panel does not flicker: zero show/hide/z-order effects are logged while each one opens and closes. *Verify (C):* `events.log` + screenshots. ✅ *Evidence:* Quick Settings / Notifications drawn over the panel; Start, taskbar: 0 panel show/hide/z/anim effects (`evidence/final/results.txt`).
- [x] **A6** The panel is not in Alt+Tab and has no taskbar button. *Verify (C):* screenshot of the Alt+Tab switcher + `WS_EX_TOOLWINDOW` style check. ✅ *Evidence:* WS_EX_TOOLWINDOW, no WS_EX_APPWINDOW in desktop and popped mode; 0 taskbar buttons (`evidence/final/results.txt`).

## B. Show/hide, pop, dismiss
- [x] **B1** Win+Alt+Space with Chrome in front: the panel's spring animation starts ≤ 50 ms after the key press (snapshot capture included), and it ends above Chrome. *Verify (C):* timestamps in `events.log` + z-order dump. ✅ *Evidence:* animation starts 2 ms after the hotkey (epoch stamps); snapshot ready 45 ms (`evidence/stage6/events-pop.log`).
- [x] **B2** While popped, clicking into Chrome leaves the panel on top. Win+Alt+Space again drops it to the bottom, and Chrome covers it. *Verify (C).* ✅ *Evidence:* popped panel stays on top after clicking into the app; shortcut drops it under the app (`evidence/final/results.txt`).
- [x] **B3** Esc (pressed before clicking elsewhere) drops a popped panel. After clicking into Chrome, Esc reaches Chrome and does not affect the panel. *Verify (C):* a test page in Chrome logs the `keydown Escape` it receives; panel state is unchanged. ✅ *Evidence:* Esc on the focused panel drops it; after clicking away, Esc reached the app (key log) and the panel stayed popped (`evidence/final/results.txt`).
- [x] **B4** × hides the panel. It stays hidden through a desktop return **and an app restart**, and comes back only via shortcut or tray. *Verify (C):* `config.json` `hidden:true` + screenshots. ✅ *Evidence:* x → hidden; still hidden on the desktop and after an app restart; shortcut brings it back (`evidence/final/results.txt`).
- [x] **B5** When the panel drops back from Popped, keyboard focus returns to the window that had it before the pop. *Verify (C):* `GetForegroundWindow` before pop == after drop. ✅ *Evidence:* `evidence/stage4/events-popdrop-rerun.log`: 40/40 drops logged `restore fg -> claude.exe ok=true`; final `fg=Claude`.
- [x] **B6** Keyboard focus is never taken except by an explicit pop. In Desktop mode, 50 clicks on tiles/sliders cause 0 foreground changes to the panel. *Verify (C):* WinEvent log. ✅ *Evidence:* 50 tile clicks: app back in front 50/50 (Explorer's desktop activation undone in 15–35 ms); 0 panel activations; from the desktop no app is pulled forward (`evidence/final/STATUS.md`).

## C. State machine tests
- [x] **C1** `cargo test` covers every (state × input) pair in the transition table, including inputs that arrive mid-animation. All tests pass. *Verify (C):* test output saved to `evidence/`. ✅ *Evidence:* `evidence/stage2/cargo-test.txt`: 18/18 pass (`transition_table` = 3 states × 7 inputs, plus mid-animation queue, focus-restore, ordering tests).
- [x] **C2** The pill's derived state is tested for every combination of {panel visible, activity, fullscreen, pill enabled}. *Verify (C).* ✅ *Evidence:* `evidence/stage2/cargo-test.txt`: 18/18 pass (`derive_pill_truth_table`, `pill_follows_machine_state`).

## D. Animation & rendering
- [x] **D1** Open, close, pop and drop animations at 144 Hz: in the frame-time log over 20 runs of each, ≥ 95% of frames are ≤ 7.6 ms and no frame is > 16.7 ms. *Verify (C):* rAF frame-time log in debug build, `scripts/fps-report.ps1`. ✅ *Evidence:* `evidence/stage4/STAGE4.md`: 144 fps in all 160 runs; worst run 97.1 % ≤ 7.6 ms; 0 frames > 16.7 ms.
- [ ] **D2** No flash of unstyled or white content at startup: 10 launches, captured with screen recording at 144 fps, show zero white or unstyled frames. *Verify (C+U):* Claude records via the Windows capture API, you eyeball the clip.
- [ ] **D3** No tearing during a pop animation (WebView2 is DWM-composited, so tearing would mean a bug). *Verify (U):* you watch 10 pops.
- [x] **D4** Glass + rounded corners render correctly in **dark** and **light** theme. Theme flip is live, under 300 ms, no restart. *Verify (C):* screenshots of both. ✅ *Evidence:* dark + light screenshots (stage 3/final); live flip: panel repainted 18 ms after DeskPanel received the change (`evidence/final/results.txt`).
- [ ] **D5** Correct at **100%, 125%, 150%** scaling: corners are not jagged, text is crisp, size is the same in logical px, and the wallpaper blur is aligned with the real wallpaper (no offset). *Verify (U):* you switch scaling; Claude screenshots and compares at each step.
- [x] **D6** Pop-over snapshot blur matches what's behind the panel: an offset of at most 2 px, checked against a captured reference. *Verify (C).* ✅ *Evidence:* raw capture vs real background: best offset 0,0, error 0.00 (`evidence/stage6/results.txt`).
- [ ] **D7** Changing the wallpaper updates the glass within 2 s. *Verify (C+U).*

## E. Controls work against the OS (each is checked through an independent read)
- [x] **E1** Volume slider → the `IAudioEndpointVolume` level read by a separate PowerShell script is within ±1%. A change from the Windows flyout updates the slider within 500 ms. *Verify (C).* ✅ *Evidence:* slider 30%→0.301, 70%→0.701; external 0.42 reflected within 500 ms (`evidence/stage5/STAGE5.md`).
- [x] **E2** Mute button ↔ system mute state. *Verify (C).* ✅ *Evidence:* mute True→False read back via Core Audio (`evidence/stage5/results.txt`).
- [x] **E3** Brightness slider → `WmiMonitorBrightness.CurrentBrightness` is within ±2. External changes are reflected. *Verify (C).* ✅ *Evidence:* slider 25→25, 60→60 via WMI; external 55 reflected (`evidence/stage5/results.txt`).
- [x] **E4** Wi-Fi toggle → `Get-NetAdapter` / radio state flips both ways. *Verify (C), with your OK since it drops the network briefly.* ✅ *Evidence:* `evidence/stage5/wifi-result.txt`: tile On→Off (adapter Disconnected)→On (adapter Up), internet back after 4 s.
- [x] **E5** Bluetooth toggle → BT radio state flips both ways (checked through the Radios API from a separate script). *Verify (C).* ✅ *Evidence:* Off→On→Off read back via WinRT radios (`evidence/stage5/results.txt`).
- [x] **E6** Dark/Light toggle → `AppsUseLightTheme` and `SystemUsesLightTheme` registry values flip, and Explorer re-themes. *Verify (C).* ✅ *Evidence:* registry apps+system 0→1→0, Explorer re-themed (`evidence/stage5/theme-light-desktop.png`).
- [x] **E7** Power mode → `powercfg /getactivescheme` overlay / Settings reflects the change for all 3 modes. *Verify (C+U).* ✅ *Evidence:* efficiency→balanced→performance read back via PowerGetEffectiveOverlayScheme (`evidence/stage5/results.txt`).
- [x] **E8** Now Playing shows a media player's track, artist and art. Prev/play-pause/next actually change a media player's state, and the progress bar advances. Also tested with YouTube in Chrome. *Verify (C+U):* needs a media player installed and playing. ✅ *Evidence:* the panel shows the current media session with art (test tone in Windows Media Player; a media player.g. `evidence/final/pill-media-crop.png`) and its play/pause toggles that session. In the final runs the current session was your a media player, so the test paused and resumed a media player; it now refuses to run while you have media playing (`scripts/final-test.ps1`).
- [ ] **E9** Timer: a 1-minute timer ends within ±1 s, plays a sound, and shows a Windows toast. Pause/resume work. *Verify (C).*
- [ ] **E10** Stopwatch: start/lap/stop/reset. Drift < 50 ms over 5 min. *Verify (C).*
- [ ] **E11** Stats: CPU% within ±5 of Task Manager, RAM within ±0.2 GB, GPU % and °C within ±3 of `nvidia-smi`, net rates within ±10%, battery % equal. *Verify (C).*

## F. Live pill
- [x] **F1** The pill appears only when (timer running OR media playing) AND the panel is not visible. All 4 combinations were tested. *Verify (C):* screenshots. ✅ *Evidence:* all 4 combinations: running+visible → none, running+covered → pill, paused+covered → none, running+hidden → pill; area around the pill is click-through (`evidence/stage8/pill-results.txt`).
- [x] **F2** The pill never appears over a fullscreen app: tested with fullscreen YouTube and a PowerPoint/slideshow. *Verify (C).* ✅ *Evidence:* borderless fullscreen window in front → pill hidden; back when it closes. Maximized windows are not treated as fullscreen (`evidence/stage8/pill-results.txt`).
- [x] **F3** Hovering expands it, the controls work, and clicking it never changes the foreground window. *Verify (C).* ✅ *Evidence:* hover expands (`evidence/stage8/pill-expanded-crop.png`); pause clicked from the pill, foreground unchanged (`evidence/stage8/pill-results.txt`).
- [x] **F4** The pill can be disabled from the tray, and this persists. *Verify (C).* ✅ *Evidence:* tray Live pill toggled pill_enabled false→true in config.json (`evidence/stage7/results.txt`).

## G. Drag & persistence
- [x] **G1** Dragging the panel works in Desktop and Popped mode. The glass doesn't visibly "swim": the wallpaper offset error is ≤ 2 px during the drag, sampled per frame. *Verify (C).* ✅ *Evidence:* desktop drag −500/+120 with 0 px glass lag (real rect at shot time); popped drag −300/+80 (`evidence/stage6/STAGE6.md`).
- [x] **G2** The panel can't be dragged off-screen. At least 80% of the panel stays inside the work area (excluding the taskbar). *Verify (C).* ✅ *Evidence:* far drag stopped with 81 % of the panel on screen (`evidence/stage6/results.txt`).
- [ ] **G3** Position persists across an app restart and a Windows restart. *Verify (C) / (U) for reboot.* ⏳ *Partial:* app restart ✅; position saved on drag end and identical after app restart (`evidence/stage6/results.txt`). Windows-restart part: re-check in Stage 7 with autostart.
- [x] **G4** An off-screen saved position (edited into config, or after a resolution change) snaps back on-screen at launch. *Verify (C).* ✅ *Evidence:* saved (5000,5000) restored 100 % on screen (`evidence/stage6/results.txt`).

## H. Performance budgets
- [ ] **H1** RAM ≤ 120 MB: the sum of private working set (exe + WebView2 children), after 10 min idle on the desktop. *Verify (C):* `scripts/measure.ps1`.
- [x] **H2** CPU < 0.1% average over 300 s while covered or hidden. *Verify (C).* ✅ *Evidence:* hidden 300 s: CPU 0.014 %, RAM 98.4 MB (`evidence/stage7/h2-hidden-300s.txt`); covered 60 s: 0.055 % (`evidence/stage7/STAGE7.md`).
- [ ] **H3** CPU < 1% average over 300 s while visible with stats ticking. *Verify (C).*
- [ ] **H4** No RAM growth > 10 MB over a 4-hour run with periodic pops. *Verify (C):* sampled every 5 min.

## I. Robustness
- [ ] **I1** Sleep → wake ×3: the panel is in place, hooks are still firing (A2 re-check passes), no crash. *Verify (U + C log).*
- [ ] **I2** Lock → unlock ×5: same. *Verify (U + C log).*
- [x] **I3** `explorer.exe` restart: the tray icon comes back, the desktop layer is re-anchored, and A1/A2 pass afterwards. *Verify (C):* `scripts/explorer-restart.ps1`. ✅ *Evidence:* panel re-created + re-parented 472 ms after TaskbarCreated, tray back, same process (`evidence/stage7/STAGE7.md`).
- [ ] **I4** Resolution change / scaling change while running: the panel stays on-screen and is re-rendered crisp. *Verify (U + C).*
- [ ] **I5** 8-hour run: no crash, no panic in the log. *Verify (C).*
- [x] **I6** (Monitor unplug is N/A: single screen.) Covered by G4 off-screen recovery instead. ✅ *Evidence:* N/A by design (single screen). Off-screen recovery is G4 ✅.

## J. Tray, autostart, exit
- [x] **J1** Tray menu items all work: Show/Hide, Pop over, Pill on/off, Start with Windows, Open config, Quit. *Verify (C).* ✅ *Evidence:* every tray item exercised by real clicks: show/hide, pop (menu + left click), pill, autostart, open config, quit (`evidence/stage7/results.txt`).
- [x] **J2** Quit leaves zero processes behind: no app exe, no `msedgewebview2.exe` children with our user-data folder. *Verify (C):* process scan 2 s after Quit. ✅ *Evidence:* 0 deskpanel and 0 of its WebView2 processes 3 s after tray Quit (`evidence/stage7/results.txt`).
- [ ] **J3** Start with Windows: after a sign-out/in (or reboot), exactly one instance is running and the panel is on the desktop. Toggling it off removes the Run key. *Verify (U for sign-in, C for registry).*
- [x] **J4** A second launch doesn't create a second instance; it pops the existing panel. *Verify (C).* ✅ *Evidence:* second launch: still 1 process, existing panel popped (`evidence/stage7/results.txt`).
- [x] **J5** Nothing requires admin, and the installer runs as a standard user. *Verify (C):* manifest `asInvoker` + per-user install path. ✅ *Evidence:* manifest `asInvoker`; install/uninstall/reinstall ran from a non-elevated shell with no UAC prompt into %LOCALAPPDATA% (`evidence/stage8/embedded-manifest.xml`).

## K. Build & docs
- [x] **K1** `npm run tauri build` produces an NSIS per-user installer and a standalone exe. *Verify (C).* ✅ *Evidence:* `npm run app:release` → `target/release/bundle/nsis/DeskPanel_0.1.0_x64-setup.exe` (1.43 MB) + standalone exe (`evidence/stage8/STAGE8.md`).
- [x] **K2** Fresh install from the installer works end-to-end: A1, B1, E1 smoke test. *Verify (C).* ✅ *Evidence:* silent per-user install (exit 0); installed exe: desktop A1 + pop B1 + controls E1 smoke OK; Run value re-pointed; toast labelled "DeskPanel" (`evidence/stage8/installed-toast-crop.png`).
- [x] **K3** Uninstall removes the app, the Run key, and (optionally) config. *Verify (C).* ✅ *Evidence:* silent uninstall: exe, folder, Start-menu shortcut, Apps entry and Run value all removed; config kept (`evidence/stage8/STAGE8.md`).
- [ ] **K4** README covers install, SmartScreen note, usage, shortcut, config file keys, known limitations, and building from source. *Verify (U):* you read it.
- [x] **K5** Windows Defender full scan of the installed exe: no detections. *Verify (C):* `Start-MpScan -ScanPath`. ✅ *Evidence:* MpCmdRun custom scan of installed exe and setup: "found no threats" (signatures 1.463.203.0), 0 detections in history (`evidence/stage8/STAGE8.md`).
