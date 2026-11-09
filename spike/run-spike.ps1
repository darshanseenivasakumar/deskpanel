# THROWAWAY: runs the spike app in each z-order mode and drives the desktop with the harness.
param([string[]]$Modes = @('bottom', 'owner', 'child'))
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$bin = Join-Path $PSScriptRoot 'target\debug'
foreach ($m in $Modes) {
  Get-Process spike -ErrorAction SilentlyContinue | Stop-Process -Force
  $app = Start-Process -FilePath "$bin\spike.exe" -ArgumentList $m -WorkingDirectory $PSScriptRoot -PassThru -WindowStyle Hidden
  Start-Sleep -Seconds 4
  & "$bin\harness.exe" $m
  Start-Sleep -Milliseconds 500
  Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 1
}
