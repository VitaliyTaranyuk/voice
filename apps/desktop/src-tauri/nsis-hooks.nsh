; NSIS installer hooks for Voice.
; Wired in through tauri.conf.json -> bundle.windows.nsis.installerHooks.
;
; ASCII only and no BOM, deliberately: this file is !include-d into a script
; compiled with `Unicode true`, and a BOM or a stray non-ASCII byte makes
; makensis fail with a parse error that names a line number and nothing else.
; The rest of the repository writes comments in Russian; this file cannot.

; Stop the local API sidecar so the installer can replace its files.
;
; Voice ships resources\voice-api\voice-api.exe, a frozen Python server. While it
; runs it holds resources\voice-api\_internal\*.pyd open, and NSIS stops on
;
;   Can't write to file "...\_internal\_asyncio.pyd"   Abort / Retry / Ignore
;
; which is exactly how the 0.1.2 -> 0.1.3 update failed on a real machine:
; voice-desktop.exe had already exited, its sidecar had not, and 1001 of 1003
; sidecar files stayed at the old version while the main binary was replaced.
;
; Tauri's own CheckIfAppIsRunning only knows ${MAINBINARYNAME}, so the sidecar is
; the installer's problem. Solving it here rather than only in the app is what
; makes the fix reach people already running 0.1.2 and 0.1.3: they download and
; launch THIS installer, and no change shipped inside the new app could help them.
; It also covers the two cases the app cannot see at all — a sidecar orphaned by
; an earlier session (parent gone, still listening on 8787), and someone running
; Setup.exe by hand while Voice is up.
;
; Killing the sidecar before CheckIfAppIsRunning stops voice-desktop.exe cannot
; race with a respawn: the app starts the sidecar once at launch
; (API_SPAWN_ATTEMPTED in api_boot.rs) and otherwise only on an API key change.
!macro KillVoiceSidecar
  nsis_tauri_utils::FindProcess "voice-api.exe"
  Pop $R0
  ${If} $R0 = 0
    DetailPrint "Stopping the Voice local API (voice-api.exe)"
    nsis_tauri_utils::KillProcess "voice-api.exe"
    Pop $R0
    ; The kernel releases the file handles a moment after the process dies, and
    ; the next thing this installer does is write to those very files. Tauri's own
    ; macro waits 500 ms for a single executable; there are ~1000 files behind
    ; this one.
    Sleep 1000
  ${EndIf}
!macroend

; Runs before CheckIfAppIsRunning and before the first File command.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro KillVoiceSidecar
!macroend

; Uninstall deletes the same files; a live sidecar leaves them on disk.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro KillVoiceSidecar
!macroend
