<#
.SYNOPSIS
  Measures RAM (private working set, the Task Manager "Memory" column) and CPU for the app
  plus every msedgewebview2.exe child that belongs to it (matched by user-data-dir marker).
.EXAMPLE
  ./scripts/measure.ps1 -ExeName deskpanel-spike -Marker dev.darshan.deskpanel.spike -Seconds 60
#>
param(
  [Parameter(Mandatory)][string]$ExeName,
  [Parameter(Mandatory)][string]$Marker,
  [int]$Seconds = 60
)

function Get-AppPids {
  $procs = Get-CimInstance Win32_Process
  $app = $procs | Where-Object { $_.Name -eq "$ExeName.exe" }
  $wv = $procs | Where-Object { $_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -like "*$Marker*" }
  [pscustomobject]@{ App = @($app.ProcessId); WebView = @($wv.ProcessId) }
}

function Get-PrivateWS([int[]]$ids) {
  $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $ids -contains [int]$_.IDProcess }
  ($perf | Measure-Object -Property WorkingSetPrivate -Sum).Sum
}

$p = Get-AppPids
if (-not $p.App) { Write-Error "$ExeName not running"; exit 1 }
$all = @($p.App) + @($p.WebView)
$appWS = Get-PrivateWS $p.App
$wvWS = if ($p.WebView) { Get-PrivateWS $p.WebView } else { 0 }

function Get-CpuMs([int[]]$ids) {
  (Get-Process -Id $ids -ErrorAction SilentlyContinue | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
}
$cpu0 = Get-CpuMs $all
$t0 = Get-Date
Start-Sleep -Seconds $Seconds
$cpu1 = Get-CpuMs $all
$elapsed = ((Get-Date) - $t0).TotalMilliseconds
$cores = [Environment]::ProcessorCount
$cpuPct = [math]::Round((($cpu1 - $cpu0) / ($elapsed * $cores)) * 100, 3)

[pscustomobject]@{
  Timestamp       = (Get-Date).ToString('s')
  AppPids         = ($p.App -join ',')
  WebViewPids     = ($p.WebView -join ',')
  AppPrivateMB    = [math]::Round($appWS / 1MB, 1)
  WebViewPrivateMB= [math]::Round($wvWS / 1MB, 1)
  TotalPrivateMB  = [math]::Round(($appWS + $wvWS) / 1MB, 1)
  CpuPercentAvg   = $cpuPct
  SampleSeconds   = $Seconds
  Cores           = $cores
}
