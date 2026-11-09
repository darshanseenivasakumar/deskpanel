# Desktop Control Panel — SPEC

Status: **Approved and implemented**, see the README for what shipped · Date: 2026-11-08 · Owner: Darshan

## 1. Intent

An Apple-style frosted-glass control panel that lives **on the Windows desktop** and **never covers an app unless you ask it to**. The original problem: a normal always-on-top floating window blocks part of Chrome once you leave the desktop. The fix chosen in the interview is to make the panel **part of the desktop layer**. It sits underneath every app window, so apps cover it naturally, and a shortcut lifts it on top when you want it.

## 2. Confirmed requirements (from the interview)

Tags: **[you]** = you said it · **[my call]** = I decided it and you can override

### 2.1 Placement model
| # | Requirement | Source |
|---|---|---|
| P1 | The panel is a **desktop-layer window**. It sits at the bottom of the z-order, under every normal app window, so it is visible exactly where the desktop is visible. | [you] picked "Desktop layer" |
| P2 | Any app window that gets focus or overlaps the panel covers it. The panel never moves above an app on its own. | [you] main concern |
| P3 | **Win+D / Show Desktop** leaves the panel visible on the desktop (it must not get minimized along with your apps). | [my call], implied by P1 |
| P4 | **Win+Alt+Space** "pops" the panel above all apps (topmost, spring-lift animation). It stays on top **even if you click into another app**, until you press Win+Alt+Space again, then it drops back to the desktop layer. | [you] |
| P5 | **Esc** drops a popped panel back, but only while the panel still has keyboard focus. Once you click into another app, Esc belongs to that app again and only the shortcut drops the panel. | [you] "Esc until you click away" |
| P6 | Start menu, Alt+Tab, taskbar, Quick Settings and notifications always draw above the panel. Nothing special happens. | consequence of P1 |
| P7 | The **close (×)** button hides the panel completely. It comes back only via the shortcut or the tray. The hidden state persists across restarts. | [you] |
| P8 | Single display only (laptop panel: 1920×1080, 125%, 144 Hz). Multi-monitor handling is limited to: *if the saved position is off-screen, snap back onto the primary screen.* | [you] "Never" external |

### 2.2 Live pill (Dynamic-Island-style)
| # | Requirement | Source |
|---|---|---|
| L1 | A small solid-black pill (~200×36 logical px) at the top center of the screen, **above apps**. | [you] |
| L2 | Shows only when **(a timer/stopwatch is running OR media is playing) AND the panel is not visible**, meaning an app covers it or the panel is hidden. | [you] |
| L3 | Never shown while a fullscreen app is in front (game, fullscreen video, presentation). | [you] |
| L4 | Hovering expands it (spring) to show controls: timer time/pause, or track + play/pause/next. It never takes keyboard focus. | [my call] |
| L5 | Can be turned off in config/tray. | [my call] |

### 2.3 Panel contents (Control Center grid, ~380 logical px wide). Every control works against the real OS.
| Tile | What it really does | API (no admin) | Risk |
|---|---|---|---|
| Wi-Fi toggle | Turns the Wi-Fi radio on or off | WinRT `Windows.Devices.Radios` | Some drivers refuse unpackaged apps; proven in a spike |
| Bluetooth toggle | Turns the BT radio on or off | WinRT `Windows.Devices.Radios` | Same as above |
| Dark/Light toggle | Flips Windows app + system theme | HKCU `Personalize` registry keys + `WM_SETTINGCHANGE` broadcast | Low |
| Power mode | Best efficiency / Balanced / Best performance | `PowerSetActiveOverlayScheme` (powrprof, undocumented but stable) | Lenovo Vantage may override it |
| Display brightness slider | Sets the internal panel's brightness | WMI `WmiMonitorBrightnessMethods` (verified readable: 40%) | Low |
| Sound slider + mute | Sets the default output device's volume | Core Audio `IAudioEndpointVolume` (+ change notifications) | Low |
| Now Playing card | Album art, title/artist, progress bar, prev / play-pause / next | WinRT `GlobalSystemMediaTransportControlsSessionManager` | Some apps don't report a timeline |
| Timer | Ring progress, presets 5/10/25 min, custom, pause/resume; sound + Windows toast at zero | in-app + `Windows.UI.Notifications` toast | Low |
| Stopwatch | start/stop/lap/reset | in-app | Low |
| Stats strip | CPU %, RAM used, GPU % + GPU °C (RTX 3050), net up/down, battery % | `GetSystemTimes`, `GlobalMemoryStatusEx`, NVML (`nvml.dll` from the driver), `GetIfTable2`, `GetSystemPowerStatus` | NVML missing → GPU fields show "—" |

**Not included** (no public API or needs admin): CPU temperature, airplane mode, Night light, Do Not Disturb.

### 2.4 Look & feel
| # | Requirement | Source |
|---|---|---|
| V1 | Theme follows Windows' app theme live (currently Dark). | [you] |
| V2 | Motion is an Apple-style spring: response ≈0.4 s, damping ≈0.8, slight overshoot. Used for open/close, pop/drop, tile press, and pill expand. | [you] |
| V3 | Glass on the desktop = **self-drawn blur of your wallpaper**: blur ≈40 px, saturate 180%, theme tint. Strength set in config. | [my call] (wallpaper is the only thing behind a bottom-layer window) |
| V4 | Glass while popped over apps = **snapshot blur**. The screen region behind the panel is captured at pop time and blurred. It stays static while popped. | [you] |
| V5 | Rounded corners (≈22 px radius), hairline border, soft shadow. All drawn in CSS inside a transparent window. | [my call] |
| V6 | Per-monitor DPI-aware v2. Crisp at 100/125/150%. Sizes in logical px. | [my call] |
| V7 | No white flash or unstyled first frame. The window is shown only after the first paint. | brief |
| V8 | Default position: right edge of the screen, vertically centered, 24 px margin. | [my call] |

### 2.5 System integration
| # | Requirement | Source |
|---|---|---|
| S1 | Tray icon menu: Show/Hide panel · Pop over · Live pill on/off · Start with Windows ✓ · Open config · Quit | [you] |
| S2 | No taskbar button and not in Alt+Tab (`WS_EX_TOOLWINDOW`). | [you] |
| S3 | Starts with Windows (per-user `HKCU\…\Run`, no admin). On by default, toggled from the tray. | [you] |
| S4 | Single instance. A second launch just pops the existing panel. | [my call] |
| S5 | Config + state in `%APPDATA%\DeskPanel\config.json` (position, hidden flag, shortcut, blur, pill on/off). | [my call] |
| S6 | Survives sleep/wake, lock/unlock, display changes, and **explorer.exe restarts** (tray re-added, desktop layer re-anchored). | brief + [my call] |

### 2.6 Budgets / constraints
| # | Budget | How measured |
|---|---|---|
| B1 | **RAM ≤ 120 MB** total while idle on the desktop | Sum of private working set: app exe + all its `msedgewebview2.exe` children (`scripts/measure.ps1`) |
| B2 | **CPU ≈ 0%** when the panel is covered or hidden (avg < 0.1% over 5 min) | Process CPU-time delta over 300 s, all processes in B1 |
| B3 | CPU < 1% avg when visible on the desktop with stats ticking at 1 Hz | same |
| B4 | No admin rights at runtime. Per-user installer. | Install as standard user |
| B5 | Windows Defender: no low-level keyboard/mouse hooks, no injected DLLs, no PyInstaller-style packers. Unsigned exe → SmartScreen "unknown publisher" prompt once (accepted limitation). | — |
| B6 | Dev tools: Rust (rustup, per-user) + VS Build Tools C++ workload. **One UAC prompt, approved by you.** | — |

## 3. Stack and why

**Tauri v2**: Rust backend (`windows` crate) plus a TypeScript/HTML/CSS frontend in WebView2.
- **Native control over the window stack.** Keeping a window at the bottom means catching `WM_WINDOWPOSCHANGING` and *changing* it before it applies. Rust can subclass the window to do that. Electron can only observe the message after the fact, which risks a one-frame flash above other windows.
- **One language for every OS API.** Core Audio (COM), the Radios and media APIs (WinRT), WMI, NVML, the power API and hooks are all called directly from Rust. No helper exe, no second runtime.
- **Smooth animation.** Springs are compiled into CSS `linear()` easing curves, which run on Chromium's compositor thread at 144 Hz with no JavaScript per frame.
- **Memory.** About 50–90 MB expected vs 180–250 MB for Electron. **Risk:** WebView2 with *two* windows (panel + pill) could approach the 120 MB limit, so Stage 1 measures it. Fallback: draw the pill natively (Direct2D) instead of in a second webview.
- Frontend: **vanilla TS + Vite**, no UI framework [my call]. It's small, uses less RAM, and is plain TS you can read.
- Rejected: Electron (RAM, after-the-fact window-stack fixes, helper needed for Windows APIs), WPF (software-rendered transparency at 144 Hz, C#), PySide6 (Defender false positives).

## 4. How detection works

The desktop-layer model removes most of the original "is the desktop focused?" problem. The panel no longer *decides* when to show itself; the window stack does that. Detection is still needed for four jobs:

### 4.1 Staying at the bottom (z-order guard)
- Window styles: `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` while in Desktop mode, so clicking a slider never takes focus away from whatever has it.
- Subclass the window procedure. On `WM_WINDOWPOSCHANGING` in Desktop mode, force `hwndInsertAfter = HWND_BOTTOM`. On `WM_MOUSEACTIVATE`, return `MA_NOACTIVATE`.
- **DECIDED in Stage 1** (evidence: `evidence/stage1/STAGE1.md`): in Desktop mode the panel is a **child window of `Progman`** (`WS_CHILD`, `SetParent(progman)`, `HWND_TOP` among Progman's children, above `SHELLDLL_DefView`).
  - Survives Win+D.
  - Is covered by every app.
  - Receives clicks without taking focus.
  - Renders WebView2 transparency correctly.
- Rejected:
  - `HWND_BOTTOM`: goes behind the desktop after Win+D.
  - Owner = Progman: steals focus on click and stays above apps after Win+D.
- **Pop:** `SetParent(NULL)` + `WS_POPUP` + `HWND_TOPMOST` + `SetForegroundWindow` (allowed because a hotkey just fired).
- **Drop:** reverse of pop, then `SetForegroundWindow(previous)`. Verified on this machine.
- **Risk:** an explorer.exe restart destroys Progman, and Windows destroys child windows along with their parent, so the panel's window will likely be destroyed too.
  - Mitigation: Rust-side state survives. On `TaskbarCreated` (or on the window's own `WM_DESTROY` when we didn't request it), re-create the panel webview and re-parent it.
  - Untested yet: it needs an Explorer restart, which is scheduled for Stage 7 (CHECKLIST I3).

### 4.2 Focus / foreground events
- `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, …, WINEVENT_OUTOFCONTEXT)`, plus `EVENT_SYSTEM_MINIMIZESTART/END`. These are event-driven with **no polling**, which keeps idle CPU at 0.
- Uses:
  - Clear the "Esc armed" flag when the foreground moves off the popped panel (P5).
  - Recompute occlusion (4.3).
  - Recompute fullscreen state (4.4).
- Stage 1 logs the real window classes this build reports for: desktop click, Win+D, minimizing the last window, Start, Alt+Tab, taskbar, Quick Settings. The table goes into this doc.

### 4.3 "Is the panel visible?" (drives L2 and stats pausing)
- On each relevant event, debounced to 100 ms: walk the windows above the panel in z-order. Skip invisible, minimized and cloaked windows (`DwmGetWindowAttribute(DWMWA_CLOAKED)` filters UWP ghosts and other virtual desktops). If any window's rect covers more than X% of the panel rect → **covered**.
- **Revised in Stage 7:** X = **97%** [my call]. Below that the user can still see part of the panel, so stats keep ticking and no pill shows. (60% would have paused a panel that was still 40% visible.)
- `EVENT_OBJECT_LOCATIONCHANGE` is very noisy, so it is only subscribed while an app window overlaps the panel, and only for that window.

### 4.4 Fullscreen
`SHQueryUserNotificationState` (`QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`, `QUNS_PRESENTATION_MODE`), OR the foreground window's rect equals the monitor rect and it isn't the desktop. Re-checked on foreground change.

### 4.5 Session / system events
- `WTSRegisterSessionNotification` (lock/unlock).
- `WM_POWERBROADCAST` (`PBT_APMRESUMEAUTOMATIC`).
- `WM_DISPLAYCHANGE`, `WM_DPICHANGED`.
- `WM_SETTINGCHANGE` (theme, wallpaper).
- `RegisterWindowMessage("TaskbarCreated")` (explorer restart).
- Each event triggers: re-anchor the z-order, re-validate the position, re-read the wallpaper/theme, and re-add the tray icon if needed.

## 5. State machine (pure Rust, unit-tested, no Win32 inside)

| From → To | Trigger |
|---|---|
| Desktop → Popped | Shortcut, tray "Pop over" |
| Popped → Desktop | Shortcut; Esc while `esc_armed` |
| Desktop / Popped → Hidden | × button, tray "Hide" |
| Hidden → Desktop | Tray "Show"; shortcut when the desktop is in front |
| Hidden → Popped | Shortcut when an app is in front |
| Popped (`esc_armed` → false) | Foreground moves to any other window |

- **Panel states:** `Hidden`, `Desktop{covered: bool}`, `Popped{esc_armed: bool}`.
- **Transient flags:** `animating_in` / `animating_out`. Inputs are queued during an animation, never dropped.
- **Pill states:** `Off`, `Collapsed`, `Expanded`. Derived from (panel visible?, activity?, fullscreen?, pill enabled?).
- From `Hidden`, the shortcut goes straight to `Popped` if an app is in front, so you see it immediately; otherwise it goes to `Desktop` [my call].
- Inputs: `Shortcut`, `Esc`, `CloseClicked`, `TrayShow`, `TrayHide`, `TrayPop`, `ForegroundChanged{is_self}`, `OcclusionChanged`, `FullscreenChanged`, `ActivityChanged`, `SessionResumed`, `DisplayChanged`.
- Outputs are *effects* (`SetZBottom`, `SetTopmost`, `Activate`, `RestoreForeground(hwnd)`, `CaptureSnapshot`, `PlayAnim(kind)`, `ShowPill`, …), executed by a thin Win32 layer. This is what makes the machine testable.

## 6. File structure

```
windows control panel/
├─ SPEC.md  CHECKLIST.md  README.md
├─ spike/                      # Stage 1 throwaway: Rust console app, logs focus events + z-order tests
├─ package.json  vite.config.ts  tsconfig.json
├─ index.html  pill.html
├─ src/                        # frontend (TypeScript)
│  ├─ panel/ main.ts  tiles/*.ts  nowplaying.ts  timer.ts  stats.ts
│  ├─ pill/  main.ts
│  ├─ lib/   spring.ts (spring → CSS linear())  ipc.ts  glass.ts (wallpaper/snapshot offsets)
│  └─ styles/ tokens.css  glass.css  tiles.css
├─ src-tauri/
│  ├─ Cargo.toml  tauri.conf.json  build.rs  capabilities/  icons/
│  └─ src/
│     ├─ main.rs  lib.rs  config.rs  tray.rs
│     ├─ state.rs              # pure state machine + #[cfg(test)] tests
│     ├─ win/  zorder.rs  events.rs  occlusion.rs  fullscreen.rs  capture.rs  wallpaper.rs  session.rs  dpi.rs
│     └─ controls/ audio.rs  brightness.rs  radios.rs  media.rs  power.rs  theme.rs  stats.rs
└─ scripts/ measure.ps1 (RAM/CPU)  explorer-restart.ps1  fps-report.ps1
```

## 7. Build stages (Phase 3)
1. **Focus + z-order spike.** Log real window classes on build 26300. Prove the bottom-layer guard + Win+D survival (choose A or B). Prove the Radios API can toggle from an unpackaged exe. Measure WebView2 RAM with two windows.
2. **State machine** + unit tests.
3. **Window shell:** transparent Tauri windows, wallpaper blur, rounded corners, theme, DPI.
4. **Animation:** spring curves, no first-frame flash, frame-time logging at 144 Hz.
5. **Controls:** one tile at a time, each verified against the OS.
6. **Drag + persistence:** off-screen recovery, snapshot blur on pop.
7. **Tray + autostart + single instance + session events.**
8. **Polish:** pill, README, NSIS per-user installer.

## 8. Non-goals
- Multi-monitor layouts beyond off-screen recovery.
- Browser-tab awareness.
- CPU temperature, airplane mode, Night light, Do Not Disturb.
- Text input inside the panel (no search box).
- Code signing.
- Live (moving) blur while popped over apps.
- Showing over exclusive-fullscreen DirectX games (Windows doesn't allow it).
- macOS/Linux support.

## 9. Known risks (blunt)
1. **Insider build 26300.** Desktop window classes and Win+D behavior can change between flights. Stage 1 documents what this build does, and a future flight may break it.
2. **Wallpaper blur ≠ real backdrop.** Desktop icons under the panel won't show in the glass. Slideshow/Spotlight wallpapers need change events, and a missed event means a stale glass until the next wallpaper change.
3. **Blur while dragging.** The background offset must track the window position every frame, or the glass "swims". Stage 6 measures this.
4. **RAM 120 MB with two WebView2 windows** may be tight. The fallback is a native pill.
5. **Lenovo Vantage** may fight the power-mode tile.
6. **DPI, sleep/wake and panel-on-top-of-desktop-icons UX** need you at the keyboard. I can't change scaling or sleep the machine without breaking my own session or policy.
