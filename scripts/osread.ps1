# Independent read-back of OS state for CHECKLIST E1-E7 (different code path from the app).
# Usage: . ./scripts/osread.ps1; Get-OsState
Add-Type @"
using System; using System.Runtime.InteropServices;
[Guid("5CDF2C82-841E-4546-9722-0CF74078229A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IAudioEndpointVolumeX {
  int RegisterControlChangeNotify(IntPtr p); int UnregisterControlChangeNotify(IntPtr p); int GetChannelCount(out uint c);
  int SetMasterVolumeLevel(float db, ref Guid ctx); int SetMasterVolumeLevelScalar(float v, ref Guid ctx);
  int GetMasterVolumeLevel(out float db); int GetMasterVolumeLevelScalar(out float v);
  int SetChannelVolumeLevel(uint ch, float db, ref Guid ctx); int SetChannelVolumeLevelScalar(uint ch, float v, ref Guid ctx);
  int GetChannelVolumeLevel(uint ch, out float db); int GetChannelVolumeLevelScalar(uint ch, out float v);
  int SetMute([MarshalAs(UnmanagedType.Bool)] bool m, ref Guid ctx); int GetMute([MarshalAs(UnmanagedType.Bool)] out bool m);
}
[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDeviceX { int Activate(ref Guid iid, int ctx, IntPtr p, [MarshalAs(UnmanagedType.IUnknown)] out object o); }
[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IMMDeviceEnumeratorX { int EnumAudioEndpoints(int f, int s, out IntPtr c); int GetDefaultAudioEndpoint(int flow, int role, out IMMDeviceX d); }
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] public class MMDeviceEnumeratorX {}
public static class OsRead {
  static IAudioEndpointVolumeX Ep() {
    var en = (IMMDeviceEnumeratorX)(new MMDeviceEnumeratorX()); IMMDeviceX d; en.GetDefaultAudioEndpoint(0, 0, out d);
    Guid iid = typeof(IAudioEndpointVolumeX).GUID; object o; d.Activate(ref iid, 23, IntPtr.Zero, out o); return (IAudioEndpointVolumeX)o; }
  public static float Volume() { float v; Ep().GetMasterVolumeLevelScalar(out v); return v; }
  public static void SetVolume(float v) { Guid g = Guid.Empty; Ep().SetMasterVolumeLevelScalar(v, ref g); }
  public static bool Muted() { bool m; Ep().GetMute(out m); return m; }
  public static void SetMuted(bool m) { Guid g = Guid.Empty; Ep().SetMute(m, ref g); }
  [DllImport("powrprof.dll")] public static extern uint PowerGetEffectiveOverlayScheme(out Guid g);
  [DllImport("powrprof.dll")] public static extern uint PowerSetActiveOverlayScheme(ref Guid g);
}
"@

$script:Radios = Join-Path $PSScriptRoot '..\spike\target\release\radios.exe'
function Get-Power { $g = [Guid]::Empty; [void][OsRead]::PowerGetEffectiveOverlayScheme([ref]$g); switch ("$g") { '961cc777-2547-4f9d-8174-7d86181b8a7a' { 'efficiency' } '00000000-0000-0000-0000-000000000000' { 'balanced' } 'ded574b5-45a0-4f42-8737-46345c09c238' { 'performance' } default { "custom:$g" } } }
function Set-Power([string]$guid) { $g = [Guid]$guid; [void][OsRead]::PowerSetActiveOverlayScheme([ref]$g) }
function Get-Brightness { (Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightness).CurrentBrightness }
function Set-Brightness([int]$v) { Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightnessMethods | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{ Timeout = 0; Brightness = [byte]$v } | Out-Null }
function Get-Theme { $k = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'; [pscustomobject]@{ Apps = $k.AppsUseLightTheme; System = $k.SystemUsesLightTheme } }
function Get-Radio([string]$name) { $l = & $script:Radios | Select-String "name=`"$name`""; if ($l -match 'state=RadioState\((\d)\)') { if ($Matches[1] -eq '1') { 'On' } else { 'Off' } } else { 'n/a' } }
function Get-OsState {
  [pscustomobject]@{ volume = [math]::Round([OsRead]::Volume(), 3); muted = [OsRead]::Muted(); brightness = Get-Brightness; bt = Get-Radio 'Bluetooth'; wifi = Get-Radio 'Wi-Fi'; theme = (Get-Theme).Apps; power = Get-Power }
}
