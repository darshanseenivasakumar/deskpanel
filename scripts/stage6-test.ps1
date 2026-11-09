# Stage 6: drag (G1), clamp (G2), persist (G3), off-screen recovery (G4), pop snapshot (B1, D6).
# Drives the desktop for ~90 s. Leaves the panel hidden at its default position, as before.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
$out = 'evidence\stage6'; New-Item -ItemType Directory -Force $out | Out-Null
$log = "$env:APPDATA\DeskPanel\logs\events.log"
$cfgPath = "$env:APPDATA\DeskPanel\config.json"
$s = 1.25
function Rect { (dp rect).Split(' ') | ForEach-Object { [int]$_ } }
function SetCfg($pos, $hidden) { $c = Get-Content $cfgPath -Raw | ConvertFrom-Json; $c.position = $pos; $c.hidden = $hidden; $c | ConvertTo-Json | Set-Content $cfgPath }
function Restart($envName, $envVal) {
  Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 500
  if ($envName) { Set-Item "Env:$envName" $envVal }
  Start-Process target\release\deskpanel.exe
  if ($envName) { Set-Item "Env:$envName" '' }
  Start-Sleep -Seconds 3
}
function Cursor($x, $y) { python -c "import ctypes; ctypes.windll.user32.SetProcessDpiAwarenessContext(-4); ctypes.windll.user32.SetCursorPos($x,$y)" }
$res = [ordered]@{}
function Check($n, [bool]$ok, $d) { $res[$n] = "$(if ($ok) { 'PASS' } else { 'FAIL' })  $d"; "{0,-40} {1}" -f $n, $res[$n] }

"== reference: bare desktop (no panel, cursor parked)"
Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 400
Cursor 2 1078; if (-not (Show-Desktop)) { throw 'could not show the desktop' }; Start-Sleep -Milliseconds 300; dp shot "$out\ref-desktop.png" | Out-Null

"== G1 drag on the desktop"
SetCfg $null $false; Restart; [void](Show-Desktop); Assert-PanelUncovered
$r0 = Rect; "start rect $r0"
$rects = ((Get-Content $log | Select-String 'JS RECTS (.*)$' | Select-Object -Last 1).Matches[0].Groups[1].Value) | ConvertFrom-Json
$mp = $rects.'m-play'
$gx = [int]($r0[0] + 92 * $s); $gy = [int]($r0[1] + ($mp[1] + $mp[3] / 2) * $s)   # album-art square: not interactive
$mark = (Get-Content $log).Count
dp drag $gx $gy ($gx - 500) ($gy + 120) 30 "$out\middrag"
Start-Sleep -Milliseconds 600
$r1 = Rect; "after drag $r1"
Check 'G1 drag moved the panel' (([math]::Abs(($r1[0] - $r0[0]) + 500) -le 3) -and ([math]::Abs(($r1[1] - $r0[1]) - 120) -le 3)) "dx=$($r1[0]-$r0[0]) dy=$($r1[1]-$r0[1]) (expected -500, +120)"
dp shot "$out\after-drag.png" | Out-Null
"   glass/ring at rest after drag:"; python scripts\stage6_check.py ring "$out\after-drag.png" "$out\ref-desktop.png" $r1[0] $r1[1] $r1[2] $r1[3] $s
$mid = 15 / 30; $mx = [int]($r0[0] - 500 * $mid); $my = [int]($r0[1] + 120 * $mid); $w = $r0[2] - $r0[0]; $h = $r0[3] - $r0[1]
"   mid-drag, 0 ms after a move step:"; python scripts\stage6_check.py ring "$out\middrag-0ms.png" "$out\ref-desktop.png" $mx $my ($mx + $w) ($my + $h) $s
"   mid-drag, held still 300 ms:"; python scripts\stage6_check.py ring "$out\middrag-300ms.png" "$out\ref-desktop.png" $mx $my ($mx + $w) ($my + $h) $s
$saved = (Get-Content $cfgPath -Raw | ConvertFrom-Json).position
Check 'G3a position saved on drag end' (($saved[0] -eq $r1[0]) -and ($saved[1] -eq $r1[1])) "config.position=$($saved -join ',') rect=$($r1[0]),$($r1[1])"

"== G2 clamp: drag far off-screen"
Assert-PanelUncovered; $r = Rect; $gx = [int]($r[0] + 92 * $s); $gy = [int]($r[1] + ($mp[1] + $mp[3] / 2) * $s)
dp drag $gx $gy ($gx + 1600) ($gy + 900) 30; Start-Sleep -Milliseconds 600
$r2 = Rect; $p2 = @(($r2[0] + 50), ($r2[1] + 50), ($r2[2] - 50), ($r2[3] - 50)); $vis = ([math]::Min($p2[2], 1920) - [math]::Max($p2[0], 0)) * ([math]::Min($p2[3], 1080) - [math]::Max($p2[1], 0)) / (($p2[2] - $p2[0]) * ($p2[3] - $p2[1]))
Check 'G2 stays >= 80% on screen' ($vis -ge 0.8) ("rect $($r2 -join ',') -> {0:P0} of the panel on screen" -f $vis)

"== G3 position survives an app restart"
Restart; $r3 = Rect
Check 'G3b same position after restart' (($r3[0] -eq $r2[0]) -and ($r3[1] -eq $r2[1])) "before $($r2[0]),$($r2[1]) after $($r3[0]),$($r3[1])"

"== G4 off-screen saved position is recovered"
Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 400
SetCfg @(5000, 5000) $false; Restart; $r4 = Rect
$p4 = @(($r4[0] + 50), ($r4[1] + 50), ($r4[2] - 50), ($r4[3] - 50)); $vis4 = ([math]::Min($p4[2], 1920) - [math]::Max($p4[0], 0)) * ([math]::Min($p4[3], 1080) - [math]::Max($p4[1], 0)) / (($p4[2] - $p4[0]) * ($p4[3] - $p4[1]))
Check 'G4 (5000,5000) snapped back on screen' ($vis4 -ge 0.999) ("rect $($r4 -join ',') -> {0:P0} on screen" -f $vis4)

"== B1 + D6: pop over an app, snapshot glass"
Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 400
SetCfg $null $false
$cap = (Resolve-Path $out).Path + '\snapshot-raw.png'
Restart 'DESKPANEL_DUMP_SNAPSHOT' $cap
Restore-Windows                                         # bring your windows back: an app is in front
$mark = (Get-Content $log).Count
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1200; dp hit
dp shot "$out\popped.png" | Out-Null
dp keys '5B,12,20'; Start-Sleep -Milliseconds 900; dp hit  # drop
$rp = Rect; Cursor 2 1078; Start-Sleep -Milliseconds 300
dp shot "$out\behind-after-drop.png" | Out-Null
$L = Get-Content $log | Select-Object -Skip $mark
# Both stamps are system-clock epoch ms: the hotkey as Rust received it, and the moment the
# animation actually started inside the webview (not when Rust later logged that message).
$tHot = [int64](($L | Select-String 'hotkey .* epoch=(\d+)' | Select-Object -First 1).Matches[0].Groups[1].Value)
$tAnim = [int64](($L | Select-String 'JS anim pop start .* epoch=(\d+)' | Select-Object -First 1).Matches[0].Groups[1].Value)
$snap = ($L | Select-String 'snapshot \d+x\d+: capture (\d+) ms, total (\d+) ms' | Select-Object -First 1)
Check 'B1 pop animation starts <= 50 ms after key' (($tAnim - $tHot) -le 50) "hotkey->anim start $($tAnim - $tHot) ms; $($snap.Line.Trim())"
"   capture vs what is really behind:"; $off = python scripts\stage6_check.py offset $cap "$out\behind-after-drop.png" $rp[0] $rp[1]; $off
$m = [regex]::Match($off, 'dx=(-?\d+) dy=(-?\d+)')
Check 'D6 snapshot aligned (<= 2 px)' (([math]::Abs([int]$m.Groups[1].Value) -le 2) -and ([math]::Abs([int]$m.Groups[2].Value) -le 2)) $off

"== leave it as you had it: hidden, default position"
$rects = ((Get-Content $log | Select-String 'JS RECTS (.*)$' | Select-Object -Last 1).Matches[0].Groups[1].Value) | ConvertFrom-Json
[void](Show-Desktop); Assert-PanelUncovered
$r = Rect; $c = $rects.close; dp click ([int]($r[0] + ($c[0] + $c[2] / 2) * $s)) ([int]($r[1] + ($c[1] + $c[3] / 2) * $s)); Start-Sleep -Milliseconds 700
Restore-Windows
"== SUMMARY"; $res.GetEnumerator() | ForEach-Object { "{0,-40} {1}" -f $_.Key, $_.Value } | Tee-Object "$out\results.txt"
Get-Content $log | Select-Object -Skip $mark | Set-Content "$out\events-pop.log"
