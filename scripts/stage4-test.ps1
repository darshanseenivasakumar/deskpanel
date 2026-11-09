# Stage 4: D2 (no white flash on launch, 10 launches) and D1 (frame times, 20x each animation).
# Drives the desktop for ~90 s. Uses the RELEASE build.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
. .\scripts\probe.ps1
$exe = 'target\release\deskpanel.exe'
$out = 'evidence\stage4'; New-Item -ItemType Directory -Force $out | Out-Null
$log = "$env:APPDATA\DeskPanel\logs\events.log"
Get-Process deskpanel -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

"== show desktop"
dp keys '5B,44'; Start-Sleep -Milliseconds 1200

"== D2: 10 launches, watch for white frames"
$flash = @()
for ($i = 1; $i -le 10; $i++) {
  $p = Start-Process $exe -PassThru
  $r = dp flashwatch 2500 "$out\flash-worst-$i.png"
  $flash += "launch $i : $r"
  Stop-Process -Id $p.Id -Force; Start-Sleep -Milliseconds 700
}
$flash

"== D1: start app for animation runs"
$mark = (Get-Content $log).Count
$p = Start-Process $exe -PassThru
Start-Sleep -Seconds 3
$r = (dp rect).Split(' ') | ForEach-Object { [int]$_ }
$closeX = $r[0] + [int]((460 - 60) * 1.25); $closeY = $r[1] + [int](60 * 1.25)
"close button at $closeX,$closeY"

"== appear/disappear x20 (on desktop)"
for ($i = 1; $i -le 20; $i++) {
  dp click $closeX $closeY; Start-Sleep -Milliseconds 600      # x -> Disappear
  dp keys '5B,12,20'; Start-Sleep -Milliseconds 900           # Win+Alt+Space on desktop -> Appear
}

"== restore your windows (app in front)"
dp keys '5B,44'; Start-Sleep -Milliseconds 1200
dp hit

"== pop/drop x20 (over the app in front)"
for ($i = 1; $i -le 20; $i++) {
  dp keys '5B,12,20'; Start-Sleep -Milliseconds 800           # Pop
  dp keys '5B,12,20'; Start-Sleep -Milliseconds 700           # Drop
}
dp hit

"== results"
$lines = Get-Content $log | Select-Object -Skip $mark
$lines | Select-String 'FRAMES' | ForEach-Object { $_.Line } | Set-Content "$out\frames.log"
$lines | Set-Content "$out\events-run.log"
foreach ($a in 'appear', 'disappear', 'pop', 'drop') {
  $f = $lines | Select-String "FRAMES anim=$a "
  $within = $f | ForEach-Object { [double]([regex]::Match($_.Line, 'within7.6=([\d.]+)%').Groups[1].Value) }
  $over = $f | ForEach-Object { [int]([regex]::Match($_.Line, 'over16.7=(\d+)').Groups[1].Value) }
  $fps = $f | ForEach-Object { [double]([regex]::Match($_.Line, 'fps~(\d+)').Groups[1].Value) }
  "{0,-9} runs={1,2} fps(avg)={2,5:N0} within7.6ms(min run)={3,5:N1}% (avg {4,5:N1}%)  runs with a >16.7ms frame={5}" -f $a, $f.Count, ($fps | Measure-Object -Average).Average, ($within | Measure-Object -Minimum).Minimum, ($within | Measure-Object -Average).Average, @($over | Where-Object { $_ -gt 0 }).Count
}
