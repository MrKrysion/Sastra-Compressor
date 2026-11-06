; The app registers its name for notifications when it starts (src-tauri/src/windows.rs),
; so uninstalling removes that entry too.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\com.krysion.sastra.compressor"
!macroend
