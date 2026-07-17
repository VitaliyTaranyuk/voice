@echo off
title Voice DEV launcher
echo.
echo Voice DEV launcher — compiles the app. Do not open several copies.
echo If the PC freezes: Task Manager - end voice-desktop / cargo / rustc
echo.
cd /d "%~dp0.."

where pwsh >nul 2>&1
if %errorlevel%==0 (
  set "VOICE_PS=pwsh"
) else (
  where powershell >nul 2>&1
  if %errorlevel%==0 (
    set "VOICE_PS=powershell"
  ) else (
    echo PowerShell not found. Install PowerShell 7 or use Windows PowerShell.
    pause
    exit /b 1
  )
)

"%VOICE_PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-voice.ps1"
if errorlevel 1 (
  echo.
  echo Launch failed. Press any key to close.
  pause >nul
)
