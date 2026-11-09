# Thin wrapper around the Rust probe (crates/probe). Usage:  . ./scripts/probe.ps1; dp hit; dp shot evidence/x.png panel
$script:DpProbe = Join-Path $PSScriptRoot '..\target\debug\dpprobe.exe'
function dp { & $script:DpProbe @args }
# Common VK codes (hex) for `dp keys` (QUOTE the list: dp keys '5B,44'): LWIN=5B, D=44, ALT=12, CTRL=11, SPACE=20, ESC=1B, TAB=09

# Win+D is a toggle, so never press it blindly: these check the result.
function Show-Desktop { for ($i = 0; $i -lt 3; $i++) { if ((dp fg) -match 'Progman|WorkerW') { return $true }; dp keys '5B,44'; Start-Sleep -Milliseconds 1200 }; return ((dp fg) -match 'Progman|WorkerW') }
function Restore-Windows { if ((dp fg) -match 'Progman|WorkerW') { dp keys '5B,44'; Start-Sleep -Milliseconds 1200 } }
# Only interact when the panel is really what is under both probe points.
function Assert-PanelUncovered { $h = dp hit; if ($h -notmatch 'left=PANEL right=PANEL') { throw "panel not uncovered, refusing to click/drag: $h" } }
