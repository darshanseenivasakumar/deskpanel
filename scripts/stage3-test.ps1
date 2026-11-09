# Stage 3 on-desktop verification: A1 (visible after Win+D), D4 (dark + light), D7 (wallpaper change -> glass rebuild).
# Temporarily flips the app theme to light and re-applies the CURRENT wallpaper; both are restored.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class S3 {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, UIntPtr w, string l, uint f, uint t, out UIntPtr r);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SystemParametersInfo(uint a, uint p, string v, uint f);
  public static void Broadcast(string what){ UIntPtr r; SendMessageTimeout((IntPtr)0xffff, 0x1A, UIntPtr.Zero, what, 2, 2000, out r); }
}
"@
$out = 'evidence\stage3'
$log = "$env:APPDATA\DeskPanel\logs\events.log"
$mark = (Get-Content $log).Count
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
$origApps = (Get-ItemProperty $key).AppsUseLightTheme

"1) Win+D -> panel visible (A1), dark theme (D4)"
dp keys '5B,44'; Start-Sleep -Milliseconds 1500
dp hit
dp shot "$out\desktop-dark.png"
dp shot "$out\panel-dark-crop.png" panel

"2) light theme (D4)"
Set-ItemProperty $key AppsUseLightTheme 1; [S3]::Broadcast('ImmersiveColorSet'); Start-Sleep -Milliseconds 1500
dp hit
dp shot "$out\desktop-light.png"
dp shot "$out\panel-light-crop.png" panel
Set-ItemProperty $key AppsUseLightTheme $origApps; [S3]::Broadcast('ImmersiveColorSet'); Start-Sleep -Milliseconds 800
"   theme restored to AppsUseLightTheme=$origApps"

"3) re-apply current wallpaper (D7 mechanism)"
$wp = (Get-ItemProperty 'HKCU:\Control Panel\Desktop').WallPaper
$t0 = Get-Date
[void][S3]::SystemParametersInfo(0x14, 0, $wp, 3)   # SPI_SETDESKWALLPAPER, SPIF_UPDATEINIFILE|SPIF_SENDCHANGE
Start-Sleep -Milliseconds 2500

"4) Win+D again -> your windows come back"
dp keys '5B,44'; Start-Sleep -Milliseconds 1200
dp hit

"--- log since test start"
Get-Content $log | Select-Object -Skip $mark
