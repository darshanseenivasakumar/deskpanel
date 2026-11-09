; DeskPanel NSIS hooks (bundle.windows.nsis.installerHooks in tauri.conf.json).

!macro NSIS_HOOK_PREUNINSTALL
  ; "Start with Windows" is written by the app at runtime (HKCU Run), so the generated uninstaller
  ; does not know about it: remove it explicitly so no dead autostart entry is left behind.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DeskPanel"
!macroend
