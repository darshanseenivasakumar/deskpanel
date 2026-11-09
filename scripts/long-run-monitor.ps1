# Long-run monitor for CHECKLIST H4 (RAM growth) and I5 (no crash, 8 h). Runs detached; samples every 5 min.
param([int]$Hours = 8)
$root = Split-Path $PSScriptRoot -Parent
$csv = Join-Path $root 'evidence\final\long-run.csv'
$log = "$env:APPDATA\DeskPanel\logs\events.log"
'time,alive,pid,total_private_mb,app_private_mb,webview_private_mb,cpu_ms_total,panics_in_log' | Set-Content $csv
$end = (Get-Date).AddHours($Hours)
while ((Get-Date) -lt $end) {
  $app = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'deskpanel.exe' }
  $wv = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -like '*dev.darshan.deskpanel*' }
  $ids = @($app.ProcessId) + @($wv.ProcessId)
  $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $ids -contains [int]$_.IDProcess }
  $appMb = [math]::Round((($perf | Where-Object { @($app.ProcessId) -contains [int]$_.IDProcess } | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
  $wvMb = [math]::Round((($perf | Where-Object { @($wv.ProcessId) -contains [int]$_.IDProcess } | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
  $cpu = [int]((Get-Process -Id $ids -ErrorAction SilentlyContinue | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum)
  $panics = @(Get-Content $log -ErrorAction SilentlyContinue | Select-String 'PANIC').Count
  "{0},{1},{2},{3},{4},{5},{6},{7}" -f (Get-Date).ToString('s'), [bool]$app, ($app.ProcessId -join ' '), ($appMb + $wvMb), $appMb, $wvMb, $cpu, $panics | Add-Content $csv
  Start-Sleep -Seconds 300
}
