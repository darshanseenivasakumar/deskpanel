# Final review run against the INSTALLED app: A1-A6, B2, B3, B4, B6, D4, E8. ~6 min.
# Uses its own throwaway "cover" window for focus/Esc tests (keys never go to your apps).
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class FT {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern void SwitchToThisWindow(IntPtr h, bool alt);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, UIntPtr w, string l, uint f, uint t, out UIntPtr r);
  public static void Theme(){ UIntPtr r; SendMessageTimeout((IntPtr)0xffff, 0x1A, UIntPtr.Zero, "ImmersiveColorSet", 2, 2000, out r); }
}
"@
$out = 'evidence\final'; New-Item -ItemType Directory -Force $out | Out-Null
$log = "$env:APPDATA\DeskPanel\logs\events.log"; $s = 1.25
$exe = "$env:LOCALAPPDATA\DeskPanel\deskpanel.exe"
$keylog = (Join-Path (Resolve-Path $out) 'cover-keys.log'); if (Test-Path $keylog) { [IO.File]::WriteAllText($keylog, '') }
$res = [ordered]@{}
function Check($n, [bool]$ok, $d) { $res[$n] = "$(if ($ok) { 'PASS' } else { 'FAIL' })  $d"; "{0,-44} {1}" -f $n, $res[$n] }
function Rects { ((Get-Content $log | Select-String 'JS RECTS (.*)$' | Select-Object -Last 1).Matches[0].Groups[1].Value) | ConvertFrom-Json }
function PRect { (dp rect).Split(' ') | ForEach-Object { [int]$_ } }
function At($id, $fx = 0.5, $fy = 0.5) { $r = PRect; $q = (Rects).$id; @([int]($r[0] + ($q[0] + $q[2] * $fx) * $s), [int]($r[1] + ($q[1] + $q[3] * $fy) * $s)) }
function OnPanel($x, $y) { (dp onpanel $x $y) -eq 'yes' }
function ClickAt($id) { $p = At $id; if (-not (OnPanel $p[0] $p[1])) { throw "refusing to click '$id' at $($p -join ','): the panel is not under that point" }; dp click $p[0] $p[1] }
function CoverHwnd { dp find 'DeskPanel test window' }
function ClickCover { if ((CoverHwnd) -eq '0x0') { throw 'test window missing: refusing to click' }; dp click ($cx + 150) ($cy + 15) }
function Since($n) { Get-Content $log | Select-Object -Skip $n }
function FgIsCover { (dp fg) -match '"DeskPanel test window"' }

if (-not (Get-Process deskpanel -ErrorAction SilentlyContinue)) { Start-Process $exe; Start-Sleep -Seconds 4 }
$mark = (Get-Content $log).Count
if (-not (Show-Desktop)) { throw 'cannot show desktop' }
Start-Sleep -Milliseconds 600   # let Win+D settle, so the shortcut sees the desktop in front
if ((dp hit) -match 'visible=false') { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1300 }
if ((dp hit) -match 'parent=#32769') { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1100 }   # came up popped -> drop to desktop
[void](Show-Desktop); Assert-PanelUncovered
if ((dp hit) -notmatch 'parent=Progman') { throw "panel is not in desktop mode: $(dp hit)" }

"== A1 visible on a clear desktop at its position"
$h = dp hit; $r = PRect
$cfg = Get-Content "$env:APPDATA\DeskPanel\config.json" -Raw | ConvertFrom-Json
$expect = if ($cfg.position) { "saved $($cfg.position -join ',')" } else { 'default: right edge, 24 px margin' }
$okPos = if ($cfg.position) { ($r[0] -eq $cfg.position[0]) -and ($r[1] -eq $cfg.position[1]) } else { [math]::Abs((1920 - ($r[2] - 50)) - 30) -le 2 }
Check 'A1 visible at its position after Win+D' (($h -match 'left=PANEL right=PANEL') -and $okPos) "$expect; rect $($r -join ','); $h"
dp shot "$out\A1-desktop.png" | Out-Null

"== D4 theme flip latency (live, no restart)"
$px = @(($r[0] + [int](60 * $s)), ($r[1] + [int](300 * $s)))   # a point on the panel's tint (between tiles)
$m4 = (Get-Content $log).Count
$d4 = python scripts\theme_latency.py $px[0] $px[1]; "   $d4"
$pe = [int64]([regex]::Match($d4, 'pixel_changed_epoch=(\d+)').Groups[1].Value)
$re = [int64]((Since $m4 | Select-String 'theme changed .* epoch=(\d+)' | Select-Object -First 1).Matches[0].Groups[1].Value)
$own = $pe - $re
Check 'D4 theme flips live < 300 ms' (($pe -gt 0) -and ($re -gt 0) -and ($own -lt 300)) "panel repainted $own ms after DeskPanel received the change ($d4)"

"== A4 Win+D x10: visible on the desktop, covered again after"
Restore-Windows; $okA4 = 0
$appWin = (Get-Process | Where-Object { $_.MainWindowTitle -and $_.ProcessName -match '^(claude|opera|chrome|msedge|Code)$' } | Select-Object -First 1).MainWindowHandle
for ($i = 1; $i -le 10; $i++) {
  [void](dp activate ('{0:x}' -f [int64]$appWin)); Start-Sleep -Milliseconds 500   # an app in front, covering the panel
  dp keys '5B,44'; Start-Sleep -Milliseconds 1000; $a = dp hit
  dp keys '5B,44'; Start-Sleep -Milliseconds 1000; $b = dp hit
  if ($i -eq 1) { "   cycle 1: after Win+D: $a"; "   cycle 1: after 2nd Win+D: $b" }
  if (($a -match 'left=PANEL right=PANEL') -and ($b -notmatch 'left=PANEL') -and ($b -notmatch 'right=PANEL')) { $okA4++ }
}
Check 'A4 Win+D shows panel / restore covers (10x)' ($okA4 -eq 10) "$okA4/10 cycles correct"

"== start the test window over the left half of the panel"
[void](Show-Desktop); $r = PRect
$cx = $r[0] - 500; $cy = $r[1] + 20; $cw = 500 + [int](($r[2] - $r[0]) / 2); $ch = ($r[3] - $r[1]) - 40
$coverProc = Start-Process target\debug\dpprobe.exe -ArgumentList @('cover', $cx, $cy, $cw, $ch, 240, "`"$keylog`"") -PassThru -WindowStyle Hidden
Start-Sleep -Milliseconds 1500; ClickCover; Start-Sleep -Milliseconds 600
"   fg: $(dp fg)"

"== A2 focus switches: every app covers the panel (10x)"
$targets = @()
$cw0 = CoverHwnd; if ($cw0 -ne '0x0') { $targets += [int64]$cw0 }
Get-Process | Where-Object { $_.MainWindowTitle -and $_.ProcessName -match '^(claude|opera|chrome|msedge|Code|explorer)$' } | Select-Object -First 3 | ForEach-Object { $targets += $_.MainWindowHandle }
"   A2 targets: $(@(Get-Process | Where-Object { $_.MainWindowTitle -and $_.ProcessName -match '^(claude|opera|chrome|msedge|Code|explorer)$' } | Select-Object -First 3 | ForEach-Object { $_.ProcessName }) -join ', ') + test window"
$okA2 = 0; $n = 0
for ($i = 0; $i -lt 10; $i++) {
  $t = $targets[$i % $targets.Count]; $act = dp activate ('{0:x}' -f [int64]$t); Start-Sleep -Milliseconds 400; if ($act -notmatch 'ok=True') { "   switch to {0:x} did not take: $act" -f [int64]$t }
  $hh = dp hit; $n++
  # the panel must never be the top window at a point the active window covers
  if ($hh -notmatch 'fgIsPanel=true' -and $hh -match 'parent=Progman' -and $hh -notmatch 'left=PANEL') { $okA2++ }
}
Check 'A2 focus switches never raise the panel' ($okA2 -eq $n) "$okA2/$n switches across $($targets.Count) apps: panel stayed under the active app"

"== A3 + B6 click the panel (uncovered half) while the test window is active"
$shell = New-Object -ComObject Shell.Application; $shell.MinimizeAll(); Start-Sleep -Milliseconds 1200
[void](dp activate ((CoverHwnd) -replace '^0x','')); Start-Sleep -Milliseconds 600; ClickCover; Start-Sleep -Milliseconds 600
if (-not (FgIsCover)) { throw "test window is not in front: $(dp fg)" }
[void](dp activate ((CoverHwnd) -replace '^0x','')); Start-Sleep -Milliseconds 600
$m6 = (Get-Content $log).Count; $fgBad = 0
for ($i = 1; $i -le 50; $i++) { ClickAt 'sw-toggle'; Start-Sleep -Milliseconds 120; if (-not (FgIsCover)) { $fgBad++ } }
$h = dp hit
ClickAt 'sw-second'; Start-Sleep -Milliseconds 300   # stopwatch is stopped after 50 clicks -> Reset
$guard = @(Since $m6 | Select-String 'FOCUS GUARD|activate panel').Count
Check 'A3 clicking the panel never raises it' (($h -match 'left=other') -and ($h -match 'right=PANEL')) "after clicks: $h"
$returned = @(Since $m6 | Select-String 'focus returned').Count
Check 'B6 50 clicks: focus never left the app' (($fgBad -eq 0) -and ($guard -eq 0)) "app not in front 120 ms after a click: $fgBad/50; panel focus-guard/activate lines=$guard; Explorer desktop activations undone=$returned"

"== B6b clicking the panel while on the desktop must not pull an app forward"
[void](Show-Desktop); Start-Sleep -Milliseconds 500; $m7 = (Get-Content $log).Count
ClickAt 'sw-toggle'; Start-Sleep -Milliseconds 400; ClickAt 'sw-toggle'; Start-Sleep -Milliseconds 400
$fgD = dp fg; $ret = @(Since $m7 | Select-String 'focus returned').Count
Check 'B6b panel click from the desktop pulls no app' (($fgD -match 'Progman|WorkerW') -and ($ret -eq 0)) "fg=$fgD; hand-backs=$ret"
[void](dp activate ((CoverHwnd) -replace '^0x','')); Start-Sleep -Milliseconds 600; ClickCover; Start-Sleep -Milliseconds 500

"== B2 popped: click into the app keeps it on top; shortcut drops it"
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1100; $a = dp hit
ClickCover; Start-Sleep -Milliseconds 600; $b = dp hit
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1000; $c = dp hit
Check 'B2 stays on top after clicking the app; drop' (($a -match 'parent=#32769') -and ($b -match 'left=PANEL') -and (FgIsCover) -and ($c -match 'left=other') -and ($c -match 'parent=Progman')) "popped: $($a -match 'parent=#32769'); after app click on top: $($b -match 'left=PANEL'); after shortcut back under app: $($c -match 'left=other')"

"== B3 Esc: drops while the panel has focus; goes to the app after clicking away"
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1100
if ((dp fg) -match 'DeskPanel') { dp keys '1B'; Start-Sleep -Milliseconds 900 }; $e1 = dp hit
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1100
ClickCover; Start-Sleep -Milliseconds 500
$keysBefore = @(Get-Content $keylog -ErrorAction SilentlyContinue | Select-String 'vk=0x1b').Count
if (FgIsCover) { dp keys '1B'; Start-Sleep -Milliseconds 600 }
$keysAfter = @(Get-Content $keylog -ErrorAction SilentlyContinue | Select-String 'vk=0x1b').Count; $e2 = dp hit
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1000
Check 'B3 Esc semantics' (($e1 -match 'parent=Progman') -and ($keysAfter -eq $keysBefore + 1) -and ($e2 -match 'parent=#32769')) "Esc on panel dropped it: $($e1 -match 'parent=Progman'); Esc after clicking away reached the app: $($keysAfter -eq $keysBefore + 1), panel stayed popped: $($e2 -match 'parent=#32769')"
Stop-Process -Id $coverProc.Id -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 500
$shell.UndoMinimizeALL(); Start-Sleep -Milliseconds 1200   # your windows back

"== A5 shell surfaces draw above the panel, no flicker"
[void](Show-Desktop); Assert-PanelUncovered; $m5 = (Get-Content $log).Count; $r = PRect
$probe = @(($r[2] - 120), ($r[3] - 120))   # bottom-right of the panel: Quick Settings / Notifications overlap here

$surf = [ordered]@{}
dp keys '5B,41'; Start-Sleep -Milliseconds 1200; $surf['Quick Settings'] = dp hit; if ((dp fg) -match 'ControlCenter|Quick') { dp keys '1B' }; Start-Sleep -Milliseconds 700
dp keys '5B,4E'; Start-Sleep -Milliseconds 1200; $surf['Notifications'] = dp hit; if ((dp fg) -match 'CoreWindow|Notification') { dp keys '1B' }; Start-Sleep -Milliseconds 700

dp keys '5B'; Start-Sleep -Milliseconds 1200; $surf['Start'] = dp hit; if ((dp fg) -match 'CoreWindow|Search|Start') { dp keys '1B' }; Start-Sleep -Milliseconds 700
dp keys '5B,54'; Start-Sleep -Milliseconds 900; $surf['Taskbar'] = dp hit; if ((dp fg) -match 'Shell_TrayWnd') { dp keys '1B' }; Start-Sleep -Milliseconds 700
$fx = @(Since $m5 | Select-String 'IN .*(ReparentTo|Show|Hide|PlayAnim)').Count
foreach ($kv in $surf.GetEnumerator()) { "   {0,-15} {1}" -f $kv.Key, $kv.Value }
$overlapping = ($surf['Quick Settings'] -match 'right=other') -or ($surf['Notifications'] -match 'right=other')
Check 'A5 shell UI above panel, 0 panel effects' (($fx -eq 0) -and $overlapping) "show/hide/z/anim effects while shell UI opened+closed: $fx; overlapping surface drawn above the panel: $overlapping"

"== A6 not in Alt+Tab, no taskbar button"
# Alt+Tab and the taskbar exclude WS_EX_TOOLWINDOW windows (unless WS_EX_APPWINDOW is set): check both modes.
function ExStyle { python -c "import ctypes; u=ctypes.windll.user32; h=u.FindWindowW(u.FindWindowW('Progman',None) and None, 'DeskPanel'); pm=u.FindWindowW('Progman',None); h=h or u.FindWindowExW(pm,None,None,'DeskPanel'); u.GetWindowLongPtrW.restype=ctypes.c_ssize_t; print(hex(u.GetWindowLongPtrW(h,-20)))" }
$exDesk = ExStyle; dp keys '5B,12,20'; Start-Sleep -Milliseconds 1100; $exPop = ExStyle; dp keys '5B,12,20'; Start-Sleep -Milliseconds 900
$tool = { param($x) (([Convert]::ToInt64($x, 16) -band 0x80) -ne 0) -and (([Convert]::ToInt64($x, 16) -band 0x40000) -eq 0) }
"   exstyle desktop=$exDesk popped=$exPop"
$UIA = [System.Windows.Automation.AutomationElement]
dp keys '5B,54'; Start-Sleep -Milliseconds 900
$names = @(); try { $names = $UIA::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, (New-Object System.Windows.Automation.PropertyCondition($UIA::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button))) | ForEach-Object { try { $_.Current.Name } catch { } } } catch { "   (UIA search hiccup: $($_.Exception.Message))" }
$tb = @($names | Where-Object { $_ -match 'DeskPanel' -and $_ -notmatch '^DeskPanel$' })   # the tray icon itself is named exactly "DeskPanel"
if ((dp fg) -match 'Shell_TrayWnd') { dp keys '1B' }
Check 'A6 not in Alt+Tab / taskbar' (($tb.Count -eq 0) -and (& $tool $exDesk) -and (& $tool $exPop)) "TOOLWINDOW & no APPWINDOW: desktop=$(& $tool $exDesk) popped=$(& $tool $exPop); taskbar buttons mentioning DeskPanel (excl. tray icon): $($tb.Count)"

"== E8 Now Playing (test tone; your volume is muted)"
# Never touch YOUR media: Windows has one "current" session, and play/pause would go to it.
$cur = (Get-Content $log | Select-String 'CTL .*media=Some\("([^"]*)' | Select-Object -Last 1)
if ($cur -and $cur.Line -notmatch 'deskpanel-test-tone') { Check 'E8 now playing (skipped)' $true "skipped: your own media session is current ($($cur.Matches[0].Groups[1].Value)); the panel would control it"; $skipE8 = $true }
$tone = "$env:TEMP\deskpanel-test-tone.wav"; $m8 = (Get-Content $log).Count
$edgeWasRunning = [bool](Get-Process msedge -ErrorAction SilentlyContinue)
if (-not $skipE8) { Start-Process $tone; Start-Sleep -Seconds 5 }
$media = Since $m8 | Select-String 'CTL .*media=Some' | Select-Object -Last 1
$player = 'Windows Media Player (legacy)'
if (-not $media) {
  Get-Process wmplayer -ErrorAction SilentlyContinue | Stop-Process -Force
  $player = 'Microsoft Edge'
  Start-Process msedge -ArgumentList '--new-window', ('file:///' + ($tone -replace '\\', '/')); Start-Sleep -Seconds 6
  $media = Since $m8 | Select-String 'CTL .*media=Some' | Select-Object -Last 1
}
"   player: $player; $($media.Line.Substring($media.Line.IndexOf('media=')))"
[void](Show-Desktop); Start-Sleep -Milliseconds 800
$m8b = (Get-Content $log).Count
function WaitLog($from, $pat) { $t0 = Get-Date; while (((Get-Date) - $t0).TotalSeconds -lt 4) { $hit = Since $from | Select-String $pat | Select-Object -Last 1; if ($hit) { return $hit }; Start-Sleep -Milliseconds 200 }; $null }
ClickAt 'm-play'; $p1 = WaitLog $m8b 'playing=false'   # legacy WMP reports its new state slowly
$m8c = (Get-Content $log).Count
ClickAt 'm-play'; $p2 = WaitLog $m8c 'playing=true'
dp shot "$out\E8-now-playing.png" panel | Out-Null
Check 'E8 now playing shown + play/pause works' ([bool]$media -and [bool]$p1 -and [bool]$p2) "player=$player; detected=$([bool]$media); paused via panel=$([bool]$p1); resumed via panel=$([bool]$p2)"
Get-Process wmplayer -ErrorAction SilentlyContinue | Stop-Process -Force
if (-not $edgeWasRunning) { Get-Process msedge -ErrorAction SilentlyContinue | Stop-Process -Force }

"== B4 x hides; stays hidden on the desktop and across a restart; shortcut brings it back"
[void](Show-Desktop); Start-Sleep -Milliseconds 800; ClickAt 'close'; Start-Sleep -Milliseconds 800; $b1 = dp hit
Get-Process deskpanel | Stop-Process -Force; Start-Sleep -Milliseconds 600; Start-Process $exe; Start-Sleep -Seconds 4; [void](Show-Desktop); $b2 = dp hit
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1300; $b3 = dp hit
Check 'B4 hidden survives desktop + restart' (($b1 -match 'visible=false') -and ($b2 -match 'visible=false') -and ($b3 -match 'visible=true')) "after x: $(([regex]::Match($b1,'visible=\w+')).Value); after restart: $(([regex]::Match($b2,'visible=\w+')).Value); after shortcut: $(([regex]::Match($b3,'visible=\w+')).Value)"

"== leave it as you had it: hidden"
ClickAt 'close'; Start-Sleep -Milliseconds 700; Restore-Windows
"== SUMMARY"; $res.GetEnumerator() | ForEach-Object { "{0,-44} {1}" -f $_.Key, $_.Value } | Tee-Object "$out\results.txt"
Since $mark | Set-Content "$out\events-run.log"
