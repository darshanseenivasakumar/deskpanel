# CHECKLIST E4: click the panel's Wi-Fi tile off and on, read the radio back independently.
# Your network drops for ~5-15 s. The script waits until the internet is back before finishing.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
. .\scripts\osread.ps1
$log = "$env:APPDATA\DeskPanel\logs\events.log"
if (-not (Get-Process deskpanel -ErrorAction SilentlyContinue)) { Start-Process target\release\deskpanel.exe; Start-Sleep 3 }
dp keys '5B,44'; Start-Sleep -Milliseconds 1300
$wasHidden = (dp hit) -match 'visible=false'
if ($wasHidden) { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1300 }
$rects = ((Get-Content $log | Select-String 'JS RECTS (.*)$' | Select-Object -Last 1).Matches[0].Groups[1].Value) | ConvertFrom-Json
$wr = (dp rect).Split(' ') | ForEach-Object { [int]$_ }
function ClickAt($id) { $r = $rects.$id; dp click ([int]($wr[0] + ($r[0] + $r[2] / 2) * 1.25)) ([int]($wr[1] + ($r[1] + $r[3] / 2) * 1.25)) }

$before = Get-Radio 'Wi-Fi'
ClickAt wifi; Start-Sleep -Seconds 4
$off = Get-Radio 'Wi-Fi'; $adapterOff = (Get-NetAdapter -Physical | Where-Object { $_.Name -match 'Wi-?Fi|WLAN' } | Select-Object -First 1).Status
ClickAt wifi
$t0 = Get-Date; do { Start-Sleep -Seconds 1; $net = Test-Connection 1.1.1.1 -Count 1 -Quiet -TimeoutSeconds 1 } until ($net -or ((Get-Date) - $t0).TotalSeconds -gt 45)
$on = Get-Radio 'Wi-Fi'; $adapterOn = (Get-NetAdapter -Physical | Where-Object { $_.Name -match 'Wi-?Fi|WLAN' } | Select-Object -First 1).Status
$ok = ($before -eq 'On' -and $off -eq 'Off' -and $on -eq 'On')
$res = "E4 Wi-Fi: $(if ($ok) { 'PASS' } else { 'FAIL' })  before=$before  after-1st-click=$off (adapter $adapterOff)  after-2nd-click=$on (adapter $adapterOn)  internet back after $([int]((Get-Date) - $t0).TotalSeconds) s"
$res; New-Item -ItemType Directory -Force evidence\stage5 | Out-Null; $res | Set-Content evidence\stage5\wifi-result.txt
if ($wasHidden) { ClickAt close }
dp keys '5B,44'
