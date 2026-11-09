# Stage 7: visible CPU (H3), tray menu (J1), autostart toggle (J3 registry part), second instance (J4),
# Explorer restart recovery (I3), Quit with no orphans (J2). ~3 min. Restarts Explorer once.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$out = 'evidence\stage7'; New-Item -ItemType Directory -Force $out | Out-Null
$log = "$env:APPDATA\DeskPanel\logs\events.log"; $cfgPath = "$env:APPDATA\DeskPanel\config.json"
$exe = (Resolve-Path 'target\release\deskpanel.exe').Path
$UIA = [System.Windows.Automation.AutomationElement]
$res = [ordered]@{}
function Check($n, [bool]$ok, $d) { $res[$n] = "$(if ($ok) { 'PASS' } else { 'FAIL' })  $d"; "{0,-44} {1}" -f $n, $res[$n] }
function Find($name, $type) {
  $c = New-Object System.Windows.Automation.AndCondition(
    (New-Object System.Windows.Automation.PropertyCondition($UIA::ControlTypeProperty, $type)),
    (New-Object System.Windows.Automation.PropertyCondition($UIA::IsOffscreenProperty, $false)))
  foreach ($e in $UIA::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, $c)) { if ($e.Current.Name -like $name) { return $e } }
  $null
}
function Center($e) { $r = $e.Current.BoundingRectangle; @([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2)) }
# Open the tray overflow and return the DeskPanel icon.
function TrayIcon {
  dp keys '5B,42'; Start-Sleep -Milliseconds 800                       # Win+B: focus the notification area
  $ch = Find 'Show Hidden Icons' ([System.Windows.Automation.ControlType]::Button)
  if ($ch) { $p = Center $ch; dp click $p[0] $p[1]; Start-Sleep -Milliseconds 900 }
  $i = Find 'DeskPanel*' ([System.Windows.Automation.ControlType]::Button)
  if (-not $i) { throw 'DeskPanel tray icon not found' }
  $i
}
# Tauri's tray menu is owner-drawn, so UI Automation sees no item names: navigate it with the
# keyboard (Down x position, Enter). Order is fixed in src-tauri/src/tray.rs.
$MenuIndex = @{ 'Show / hide panel' = 1; 'Pop over apps' = 2; 'Live pill' = 3; 'Start with Windows' = 4; 'Open config file' = 5; 'Quit DeskPanel' = 6 }
function TrayMenu($item) {
  $p = Center (TrayIcon); dp rclick $p[0] $p[1]; Start-Sleep -Milliseconds 700
  if ((dp fg) -notmatch 'tray_icon_app') { dp keys '1B'; throw "tray menu did not open for '$item'" }
  for ($i = 0; $i -lt $MenuIndex[$item]; $i++) { dp keys '28'; Start-Sleep -Milliseconds 60 }
  dp keys '0D'; Start-Sleep -Milliseconds 900
}
function TrayLeftClick { $p = Center (TrayIcon); dp click $p[0] $p[1]; Start-Sleep -Milliseconds 1000 }
function Procs { @(Get-Process deskpanel -ErrorAction SilentlyContinue) }
function Orphans { @(Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -like '*dev.darshan.deskpanel*' }) }
function RunValue { (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name DeskPanel -ErrorAction SilentlyContinue).DeskPanel }

if ((Procs).Count -eq 0) { Start-Process $exe; Start-Sleep -Seconds 4 }
$mark = (Get-Content $log).Count

if (-not $env:SKIP_H3) {
"== H3 visible on the desktop, stats ticking (60 s)"
[void](Show-Desktop); if ((dp hit) -match 'visible=false') { dp keys '5B,12,20' }; Start-Sleep -Seconds 8
$h = dp hit; "   $h"
$mv = (Get-Content $log).Count
$v = ./scripts/measure.ps1 -ExeName deskpanel -Marker dev.darshan.deskpanel -Seconds 60
# valid only if nothing covered the panel during the minute (occlusion log) and it is still visible
$stillVisible = ((dp hit) -match 'left=PANEL right=PANEL') -and -not (Get-Content $log | Select-Object -Skip $mv | Select-String 'covered=true')
Check 'H3 visible CPU < 1 %' (($v.CpuPercentAvg -lt 1) -and $stillVisible) ("CPU {0} %  RAM {1} MB  (desktop stayed in front: {2})" -f $v.CpuPercentAvg, $v.TotalPrivateMB, $stillVisible)
Check 'H1 RAM <= 120 MB (visible)' ($v.TotalPrivateMB -le 120) "$($v.TotalPrivateMB) MB"
}

"== J1 tray: Show / hide panel"
TrayMenu 'Show / hide panel'; Start-Sleep -Milliseconds 500; $a = dp hit
TrayMenu 'Show / hide panel'; Start-Sleep -Milliseconds 800; $b = dp hit
Check 'J1 show/hide' (($a -match 'visible=false') -and ($b -match 'visible=true')) "after 1st: $(([regex]::Match($a,'visible=\w+')).Value), after 2nd: $(([regex]::Match($b,'visible=\w+')).Value)"

"== J1 tray: Pop over apps + left click"
Restore-Windows
TrayMenu 'Pop over apps'; $a = dp hit; dp keys '5B,12,20'; Start-Sleep -Milliseconds 900
TrayLeftClick; $b = dp hit; dp keys '5B,12,20'; Start-Sleep -Milliseconds 900
Check 'J1 pop over (menu) + left click' (($a -match 'parent=#32769') -and ($b -match 'parent=#32769')) "menu: $(([regex]::Match($a,'parent=\S+')).Value)  left-click: $(([regex]::Match($b,'parent=\S+')).Value)"

"== J1 tray: Live pill toggle"
TrayMenu 'Live pill'; $p1 = (Get-Content $cfgPath -Raw | ConvertFrom-Json).pill_enabled
TrayMenu 'Live pill'; $p2 = (Get-Content $cfgPath -Raw | ConvertFrom-Json).pill_enabled
Check 'J1 live pill toggles + persists (F4)' ((-not $p1) -and $p2) "config pill_enabled: $p1 -> $p2"

"== J1/J3 tray: Start with Windows toggle"
TrayMenu 'Start with Windows'; $r1 = RunValue
TrayMenu 'Start with Windows'; $r2 = RunValue
Check 'J3 autostart toggle (registry)' ((-not $r1) -and ($r2 -like "*deskpanel.exe*")) "off: '$r1'  on: '$r2'"

"== J1 tray: Open config file"
$before = @(Get-Process notepad -ErrorAction SilentlyContinue).Id
TrayMenu 'Open config file'; Start-Sleep -Seconds 2
$np = Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id -and $_.MainWindowTitle -match 'config' }
Check 'J1 open config file' ([bool]$np) "notepad window: '$($np.MainWindowTitle)'"
$np | Stop-Process -Force -ErrorAction SilentlyContinue

"== J4 second launch pops the existing panel"
Restore-Windows; $n0 = (Procs).Count
Start-Process $exe; Start-Sleep -Seconds 3
$n1 = (Procs).Count; $a = dp hit
Check 'J4 single instance' (($n0 -eq 1) -and ($n1 -eq 1) -and ($a -match 'parent=#32769')) "processes before=$n0 after=$n1; panel $(([regex]::Match($a,'parent=\S+')).Value)"
dp keys '5B,12,20'; Start-Sleep -Milliseconds 900

"== I3 Explorer restart"
$pid0 = (Procs)[0].Id; $m3 = (Get-Content $log).Count
Stop-Process -Name explorer -Force; Start-Sleep -Seconds 4
if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
Start-Sleep -Seconds 8
$alive = [bool](Get-Process -Id $pid0 -ErrorAction SilentlyContinue)
[void](Show-Desktop); Start-Sleep -Milliseconds 800; $a = dp hit
$trayBack = $false; try { [void](TrayIcon); $trayBack = $true; dp keys '1B' } catch { }
Get-Content $log | Select-Object -Skip $m3 | Select-String 'explorer|revalidate|to_desktop|PANIC|gone' | ForEach-Object { "   " + $_.Line.Trim() }
Check 'I3 survives Explorer restart' ($alive -and ($a -match 'parent=Progman') -and ($a -match 'left=PANEL') -and $trayBack) "process alive=$alive; $a; tray icon back=$trayBack"
dp shot "$out\after-explorer-restart.png" | Out-Null
Restore-Windows

"== J2 Quit from the tray leaves nothing behind"
TrayMenu 'Quit DeskPanel'; Start-Sleep -Seconds 3
$p = (Procs).Count; $o = (Orphans).Count
Check 'J2 quit, no orphan processes' (($p -eq 0) -and ($o -eq 0)) "deskpanel processes=$p, its WebView2 processes=$o"

"== restart DeskPanel so it keeps running for you"
Start-Process $exe; Start-Sleep -Seconds 3
"== SUMMARY"; $res.GetEnumerator() | ForEach-Object { "{0,-44} {1}" -f $_.Key, $_.Value } | Tee-Object "$out\results.txt"
Get-Content $log | Select-Object -Skip $mark | Set-Content "$out\events-run.log"
