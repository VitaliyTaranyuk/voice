# Create/update Desktop shortcut Voice.lnk → scripts/start-voice.cmd
# Usage: pwsh scripts/install-desktop-shortcut.ps1

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$target = Join-Path $PSScriptRoot "start-voice.cmd"
if (-not (Test-Path $target)) {
  throw "Missing launcher: $target"
}

$desktop = [Environment]::GetFolderPath("Desktop")
$lnkPath = Join-Path $desktop "Voice.lnk"
$icon = Join-Path $root "apps\desktop\src-tauri\icons\icon.ico"

$w = New-Object -ComObject WScript.Shell
$s = $w.CreateShortcut($lnkPath)
$s.TargetPath = $target
$s.WorkingDirectory = "$root"
$s.Description = "Voice DEV launcher (tauri dev) — do not open multiple copies"
$s.WindowStyle = 1
if (Test-Path $icon) {
  $s.IconLocation = "$icon,0"
}
$s.Save()

Write-Host "Updated: $lnkPath"
Write-Host "Target:  $target"
