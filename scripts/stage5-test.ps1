# Stage 5: every control is clicked on the real panel and read back through an independent path.
# Changes volume, mute, brightness, Bluetooth, theme and power mode, then RESTORES the originals.
# Wi-Fi is NOT touched (it would cut the network); see scripts/wifi-test.ps1. ~2.5 min.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
. .\scripts\osread.ps1
$out = 'evidence\stage5'; New-Item -ItemType Directory -Force $out | Out-Null
$log = "$env:APPDATA\DeskPanel\logs\events.log"
$orig = Get-OsState; $origTheme = Get-Theme; $origPower = $orig.power
"originals: $($orig | ConvertTo-Json -Compress)"

Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 500
$mark = (Get-Content $log).Count
$env:DESKPANEL_LOG_STATS = '1'; Start-Process target\release\deskpanel.exe; $env:DESKPANEL_LOG_STATS = $null
Start-Sleep -Seconds 3
dp keys '5B,44'; Start-Sleep -Milliseconds 1300
if ((dp hit) -match 'visible=false') { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1300 }
dp hit

function NewLog { Get-Content $log | Select-Object -Skip $script:mark }
$rects = ((NewLog | Select-String 'JS RECTS (.*)$' | Select-Object -Last 1).Matches[0].Groups[1].Value) | ConvertFrom-Json
$wr = (dp rect).Split(' ') | ForEach-Object { [int]$_ }; $s = 1.25
function At($id, $fx = 0.5, $fy = 0.5) { $r = $rects.$id; @([int]($wr[0] + ($r[0] + $r[2] * $fx) * $s), [int]($wr[1] + ($r[1] + $r[3] * $fy) * $s)) }
function ClickAt($id, $fx = 0.5, $fy = 0.5) { $p = At $id $fx $fy; dp click $p[0] $p[1] }
$results = [ordered]@{}
function Check($name, [bool]$ok, $detail) { $results[$name] = "$(if ($ok) { 'PASS' } else { 'FAIL' })  $detail"; "{0,-34} {1}" -f $name, $results[$name] }
function CtlSince($n, $pattern) { @(Get-Content $log | Select-Object -Skip $n | Select-String $pattern).Count -gt 0 }

"== E1 volume slider -> OS"
ClickAt volume 0.30; Start-Sleep -Milliseconds 700; $v = [OsRead]::Volume(); Check 'E1 slider 30% -> OS' ([math]::Abs($v - 0.30) -le 0.01) "OS volume=$([math]::Round($v,3))"
ClickAt volume 0.70; Start-Sleep -Milliseconds 700; $v = [OsRead]::Volume(); Check 'E1 slider 70% -> OS' ([math]::Abs($v - 0.70) -le 0.01) "OS volume=$([math]::Round($v,3))"
$n = (Get-Content $log).Count; [OsRead]::SetVolume(0.42); Start-Sleep -Milliseconds 500
Check 'E1 external 42% -> panel <=500ms' (CtlSince $n 'CTL vol=Some\(0\.42') 'CTL snapshot with vol=0.42 logged within 500 ms'
dp shot "$out\after-volume.png" panel | Out-Null

"== E2 mute"
$r = $rects.volume; $mx = [int]($wr[0] + ($r[0] + 14) * $s); $my = [int]($wr[1] + ($r[1] + $r[3] / 2) * $s)
dp click $mx $my; Start-Sleep -Milliseconds 600; $m1 = [OsRead]::Muted()
dp click $mx $my; Start-Sleep -Milliseconds 600; $m2 = [OsRead]::Muted()
Check 'E2 mute on/off' ($m1 -and -not $m2) "after 1st click muted=$m1, after 2nd muted=$m2"

"== E3 brightness"
ClickAt brightness 0.25; Start-Sleep -Milliseconds 1200; $b = Get-Brightness; Check 'E3 slider 25% -> OS' ([math]::Abs($b - 25) -le 2) "WMI brightness=$b"
ClickAt brightness 0.60; Start-Sleep -Milliseconds 1200; $b = Get-Brightness; Check 'E3 slider 60% -> OS' ([math]::Abs($b - 60) -le 2) "WMI brightness=$b"
$n = (Get-Content $log).Count; Set-Brightness 55; Start-Sleep -Milliseconds 1800
Check 'E3 external 55 -> panel' (CtlSince $n 'CTL .*bri=Some\(55\)') 'CTL snapshot with bri=55 logged'

"== E5 Bluetooth"
ClickAt bt; Start-Sleep -Milliseconds 3000; $b1 = Get-Radio 'Bluetooth'
ClickAt bt; Start-Sleep -Milliseconds 3000; $b2 = Get-Radio 'Bluetooth'
Check 'E5 bluetooth toggles both ways' ($b1 -ne $b2) "after 1st=$b1 after 2nd=$b2 (orig $($orig.bt))"

"== E6 theme"
ClickAt 'theme-tile'; Start-Sleep -Milliseconds 1800; $t1 = Get-Theme; dp shot "$out\theme-light-desktop.png" | Out-Null; dp shot "$out\theme-light-panel.png" panel | Out-Null
ClickAt 'theme-tile'; Start-Sleep -Milliseconds 1800; $t2 = Get-Theme
Check 'E6 theme flips both ways' (($t1.Apps -eq 1 -and $t1.System -eq 1) -and ($t2.Apps -eq 0 -and $t2.System -eq 0)) "1st: apps=$($t1.Apps) sys=$($t1.System); 2nd: apps=$($t2.Apps) sys=$($t2.System)"

"== E7 power mode (cycles efficiency -> balanced -> performance)"
$seen = @()
for ($i = 0; $i -lt 3; $i++) { ClickAt 'power-tile'; Start-Sleep -Milliseconds 900; $seen += Get-Power }
Check 'E7 power mode, all 3 modes' (($seen -join ',') -eq 'efficiency,balanced,performance') "OS read-back: $($seen -join ' -> ')"

"== E9/E10/E11 timer (1 min) + stopwatch + stats, ~65 s"
$td = At 'timer-digits'; dp wheel $td[0] $td[1] -24; Start-Sleep -Milliseconds 400
$nT = (Get-Content $log).Count
ClickAt 'timer-toggle'                       # start 1-minute timer
ClickAt 'sw-toggle'; Start-Sleep -Seconds 2  # stopwatch start
ClickAt 'sw-second'; Start-Sleep -Seconds 1  # lap
ClickAt 'sw-second'                          # lap
# E11 reference samples, taken while the app logs STATS every second
$nS = (Get-Content $log).Count
$cpuRef = (Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 1 -MaxSamples 10).CounterSamples.CookedValue
$nE = (Get-Content $log).Count
$os = Get-CimInstance Win32_OperatingSystem; $ramRef = ($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1MB
$batRef = (Get-CimInstance Win32_Battery).EstimatedChargeRemaining
$appStats = Get-Content $log | Select-Object -Skip $nS -First ($nE - $nS) | Select-String 'STATS StatsInfo \{ cpu: ([\d.]+), ram_used_gb: ([\d.]+).*battery: (Some\((\d+)\)|None)'
$appCpu = ($appStats | ForEach-Object { [double]$_.Matches[0].Groups[1].Value } | Measure-Object -Average).Average
$appRam = [double]($appStats | Select-Object -Last 1).Matches[0].Groups[2].Value
$appBat = ($appStats | Select-Object -Last 1).Matches[0].Groups[4].Value
Check 'E11 CPU (10 s avg) vs perf counter' ([math]::Abs($appCpu - ($cpuRef | Measure-Object -Average).Average) -le 5) ("app {0:N1}% vs counter {1:N1}% ({2} app samples)" -f $appCpu, ($cpuRef | Measure-Object -Average).Average, $appStats.Count)
Check 'E11 RAM vs CIM' ([math]::Abs($appRam - $ramRef) -le 0.2) ("app {0:N2} GB vs CIM {1:N2} GB" -f $appRam, $ramRef)
Check 'E11 battery vs Win32_Battery' ("$appBat" -eq "$batRef") "app $appBat% vs WMI $batRef%"
$gpuLine = (Get-Content $log | Select-Object -Skip $nS | Select-String 'STATS' | Select-Object -Last 1).Line
"   last STATS: $($gpuLine.Substring($gpuLine.IndexOf('STATS')))"

# wait for the timer
$deadline = (Get-Date).AddSeconds(75)
while ((Get-Date) -lt $deadline -and -not (CtlSince $nT 'timer finished')) { Start-Sleep -Milliseconds 250 }
Start-Sleep -Milliseconds 800
dp shot "$out\timer-done-toast.png" | Out-Null
$tl = Get-Content $log | Select-Object -Skip $nT
$tStart = ($tl | Select-String '^\s*(\d+) controls: TimerStart' | Select-Object -First 1).Matches[0].Groups[1].Value
$tEnd = ($tl | Select-String '^\s*(\d+) controls: timer finished' | Select-Object -First 1)
if ($tEnd) { $dur = [int]$tEnd.Matches[0].Groups[1].Value - [int]$tStart; Check 'E9 1-min timer within +-1 s' ([math]::Abs($dur - 60000) -le 1000) "fired after $dur ms; toast/sound attempted (see timer-done-toast.png)" }
else { Check 'E9 1-min timer within +-1 s' $false 'timer never finished' }
$toastErr = $tl | Select-String 'toast failed'; if ($toastErr) { "   TOAST ERROR: $($toastErr.Line)" }

ClickAt 'sw-toggle'; Start-Sleep -Milliseconds 500        # stop
$swCtl = (Get-Content $log | Select-Object -Skip $nT | Select-String 'CTL .*sw_running=false laps=2' | Select-Object -Last 1)
ClickAt 'sw-second'; Start-Sleep -Milliseconds 500        # reset
$swReset = (Get-Content $log | Select-Object -Skip $nT | Select-String 'CTL .*sw_running=false laps=0' | Select-Object -Last 1)
Check 'E10 stopwatch start/lap/stop/reset' ([bool]$swCtl -and [bool]$swReset) 'CTL: stopped with 2 laps, then reset to 0'

"== E8 media"
$med = (Get-Content $log | Select-Object -Skip $mark | Select-String 'CTL .*media=(Some\("[^"]*"\)|None)' | Select-Object -Last 1)
"   last media state: $($med.Matches[0].Groups[1].Value)"

"== restore originals"
[OsRead]::SetVolume([float]$orig.volume); [OsRead]::SetMuted([bool]$orig.muted)
Set-Brightness $orig.brightness
if ((Get-Radio 'Bluetooth') -ne $orig.bt) { ClickAt bt; Start-Sleep -Seconds 3 }
if ((Get-Theme).Apps -ne $origTheme.Apps) { ClickAt 'theme-tile'; Start-Sleep -Seconds 2 }
Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' SystemUsesLightTheme $origTheme.System
switch ($origPower) { 'performance' { Set-Power 'ded574b5-45a0-4f42-8737-46345c09c238' } 'efficiency' { Set-Power '961cc777-2547-4f9d-8174-7d86181b8a7a' } 'balanced' { Set-Power '00000000-0000-0000-0000-000000000000' } }
Start-Sleep -Milliseconds 800
"restored: $(Get-OsState | ConvertTo-Json -Compress)"
ClickAt close; Start-Sleep -Milliseconds 600   # leave the panel hidden, as you had it
dp keys '5B,44'

"== SUMMARY"; $results.GetEnumerator() | ForEach-Object { "{0,-34} {1}" -f $_.Key, $_.Value } | Tee-Object "$out\results.txt"
Get-Content $log | Select-Object -Skip $mark | Set-Content "$out\events-run.log"
