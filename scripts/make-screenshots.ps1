# Generates the README screenshots in docs/images using DESKPANEL_DEMO=1 (sample Wi-Fi name and
# track, so no personal data is shown). Takes ~1 min and drives the desktop. Restores the theme.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
$img = 'docs\images'; New-Item -ItemType Directory -Force $img | Out-Null
$tmp = Join-Path $env:TEMP 'deskpanel-shots'; New-Item -ItemType Directory -Force $tmp | Out-Null
$exe = "$env:LOCALAPPDATA\DeskPanel\deskpanel.exe"
$k = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
$origTheme = (Get-ItemProperty $k).AppsUseLightTheme

function Broadcast { python -c "import ctypes; r=ctypes.c_size_t(); ctypes.windll.user32.SendMessageTimeoutW(0xFFFF,0x1A,0,ctypes.c_wchar_p('ImmersiveColorSet'),2,2000,ctypes.byref(r))" }
function Crop($src, $dst, $l, $t, $r, $b) { python -c "from PIL import Image; Image.open(r'$src').crop(($l,$t,$r,$b)).save(r'$dst')" }
function PRect { (dp rect).Split(' ') | ForEach-Object { [int]$_ } }

try {
  Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 800
  $env:DESKPANEL_DEMO = '1'; Start-Process $exe; $env:DESKPANEL_DEMO = $null; Start-Sleep -Seconds 5

  if (-not (Show-Desktop)) { throw 'cannot show desktop' }
  Start-Sleep -Milliseconds 600
  if ((dp hit) -match 'visible=false') { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1500 }
  if ((dp hit) -match 'parent=#32769') { dp keys '5B,12,20'; Start-Sleep -Milliseconds 1200 }
  Assert-PanelUncovered
  dp move 700 300; Start-Sleep -Milliseconds 1500
  $r = PRect; $box = @(($r[0] + 44), ($r[1] + 44), ($r[2] - 44), ($r[3] - 44)); "panel box: $box"

  "== dark, on the desktop"
  dp shot "$tmp\dark.png" | Out-Null; Crop "$tmp\dark.png" "$img\panel-dark.png" @box
  dp shot "$tmp\desktop.png" | Out-Null

  "== light"
  Set-ItemProperty $k AppsUseLightTheme 1; Broadcast; Start-Sleep -Milliseconds 1200
  dp shot "$tmp\light.png" | Out-Null; Crop "$tmp\light.png" "$img\panel-light.png" @box
  Set-ItemProperty $k AppsUseLightTheme $origTheme; Broadcast; Start-Sleep -Milliseconds 800

  "== covered by an app (plain backdrop window, so nothing private shows behind the glass): pill"
  Restore-Windows
  $bd = Start-Process target\debug\dpprobe.exe -ArgumentList @('cover', 0, 0, 1920, 1020, 90, "`"$tmp\bd-keys.log`"", 58, 66, 118) -PassThru -WindowStyle Hidden
  Start-Sleep -Milliseconds 1500
  [void](dp activate ((dp find 'DeskPanel test window') -replace '^0x', '')); Start-Sleep -Milliseconds 2200
  "   $(dp pill)"
  dp shot "$tmp\pill.png" | Out-Null; Crop "$tmp\pill.png" "$img\pill.png" 760 0 1160 66
  dp move 960 30; Start-Sleep -Milliseconds 1000
  dp shot "$tmp\pill-open.png" | Out-Null; Crop "$tmp\pill-open.png" "$img\pill-expanded.png" 720 0 1200 130
  dp move 700 600; Start-Sleep -Milliseconds 800

  "== popped over the app"
  dp keys '5B,12,20'; Start-Sleep -Milliseconds 1600
  if ((dp hit) -notmatch 'parent=#32769') { throw "not popped: $(dp hit)" }
  dp move 700 300; Start-Sleep -Milliseconds 600
  dp shot "$tmp\popped.png" | Out-Null; Crop "$tmp\popped.png" "$img\panel-popped.png" @box
  dp keys '5B,12,20'; Start-Sleep -Milliseconds 1000
  Stop-Process -Id $bd.Id -Force -ErrorAction SilentlyContinue
  "done"
}
finally {
  if ($bd) { Stop-Process -Id $bd.Id -Force -ErrorAction SilentlyContinue }
  Set-ItemProperty $k AppsUseLightTheme $origTheme; Broadcast
  Restore-Windows
  Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 800
  Start-Process $exe   # back to normal mode (real data, hidden as configured)
}
