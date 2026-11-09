# Stage 8: live pill. F1 (all visibility combinations), F2 (never over fullscreen), F3 (hover expand,
# controls work, no focus change). ~70 s. Leaves the panel hidden and the timer reset.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
$out = 'evidence\stage8'; New-Item -ItemType Directory -Force $out | Out-Null
$log = "$env:APPDATA\DeskPanel\logs\events.log"; $s = 1.25
$res = [ordered]@{}
function Check($n, [bool]$ok, $d) { $res[$n] = "$(if ($ok) { 'PASS' } else { 'FAIL' })  $d"; "{0,-46} {1}" -f $n, $res[$n] }
function Cursor($x, $y) { dp move $x $y }   # real mouse-move input, so hover fires
function PillVisible { (dp pill) -match 'visible=true' }
function Rects { ((Get-Content $log | Select-String 'JS RECTS (.*)$' | Select-Object -Last 1).Matches[0].Groups[1].Value) | ConvertFrom-Json }
function ClickPanel($id) { $r = (dp rect).Split(' ') | % { [int]$_ }; $q = (Rects).$id; dp click ([int]($r[0] + ($q[0] + $q[2] / 2) * $s)) ([int]($r[1] + ($q[1] + $q[3] / 2) * $s)) }

Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 500
$mark = (Get-Content $log).Count
Start-Process target\release\deskpanel.exe; Start-Sleep -Seconds 4
[void](Show-Desktop); if ((dp hit) -match 'visible=false') { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1300 }
Assert-PanelUncovered
"== start a timer from the panel"
ClickPanel 'timer-toggle'; Start-Sleep -Milliseconds 1500
Check 'F1a running + panel visible -> no pill' (-not (PillVisible)) (dp pill)

"== cover the panel with your apps"
Restore-Windows; Start-Sleep -Milliseconds 1500
$p = dp pill
Check 'F1b running + panel covered -> pill' ($p -match 'visible=true') $p
Check 'F1 pill does not block clicks around it' ($p -match 'click-through ok') $p
dp shot "$out\pill-collapsed.png" | Out-Null

"== F2 fullscreen app in front"
$job = Start-Job -ScriptBlock { param($d) & "$d\target\debug\dpprobe.exe" fullscreen 6 } -ArgumentList (Get-Location).Path
Start-Sleep -Seconds 3
$pf = dp pill; "   during fullscreen: $pf  fg=$(dp fg)"
Check 'F2 no pill over a fullscreen app' ($pf -match 'visible=false') $pf
Wait-Job $job | Out-Null; Remove-Job $job; Start-Sleep -Milliseconds 1500
$pa = dp pill
Check 'F2 pill returns after fullscreen closes' ($pa -match 'visible=true') $pa

"== F3 hover expands; buttons work without stealing focus"
$pr = ([regex]::Match($pa, 'rect=(-?\d+),(-?\d+),(-?\d+),(-?\d+)').Groups | Select-Object -Skip 1 | % { [int]$_.Value })
$cx = [int](($pr[0] + $pr[2]) / 2); $cy = $pr[1] + [int]((8 + 18) * $s)
$fg0 = dp fg
Cursor $cx $cy; Start-Sleep -Milliseconds 900
dp shot "$out\pill-expanded.png" | Out-Null
$m3 = (Get-Content $log).Count
dp click $cx ($pr[1] + [int](80 * $s)); Start-Sleep -Milliseconds 900       # middle button = pause
$paused = [bool](Get-Content $log | Select-Object -Skip $m3 | Select-String 'controls: TimerPause')
$fg1 = dp fg
Check 'F3 hover expands + pause works, focus unchanged' ($paused -and ($fg0 -eq $fg1)) "pause sent=$paused; fg before='$fg0' after='$fg1'"
Cursor 960 600; Start-Sleep -Milliseconds 900
Check 'F1c paused (no activity) + covered -> no pill' (-not (PillVisible)) (dp pill)

"== F1d timer running + panel hidden -> pill"
dp keys '5B,12,20'; Start-Sleep -Milliseconds 1200                            # pop the panel over the app
ClickPanel 'timer-toggle'; Start-Sleep -Milliseconds 600                      # resume
ClickPanel 'close'; Start-Sleep -Milliseconds 1500                            # x: hide it
$ph = dp pill
Check 'F1d running + panel hidden -> pill' ($ph -match 'visible=true') $ph

"== cleanup: reset the timer from the pill (expanded, left button)"
$pr = ([regex]::Match($ph, 'rect=(-?\d+),(-?\d+),(-?\d+),(-?\d+)').Groups | Select-Object -Skip 1 | % { [int]$_.Value })
$cx = [int](($pr[0] + $pr[2]) / 2)
Cursor $cx ($pr[1] + [int]((8 + 18) * $s)); Start-Sleep -Milliseconds 900
dp click ($cx - [int](50 * $s)) ($pr[1] + [int](80 * $s)); Start-Sleep -Milliseconds 900
Cursor 960 600; Start-Sleep -Milliseconds 800
"   after reset: $(dp pill)"
"== SUMMARY"; $res.GetEnumerator() | ForEach-Object { "{0,-46} {1}" -f $_.Key, $_.Value } | Tee-Object "$out\pill-results.txt"
Get-Content $log | Select-Object -Skip $mark | Set-Content "$out\events-pill.log"
