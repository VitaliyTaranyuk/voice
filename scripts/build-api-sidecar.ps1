# Build frozen Voice API sidecar (PyInstaller onedir).
# Output: dist/voice-sidecar/voice-api/voice-api.exe
# Usage: pwsh scripts/build-api-sidecar.ps1

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$apiDir = Join-Path $root "services\api"
$outRoot = Join-Path $root "dist\voice-sidecar"
$resourceDir = Join-Path $root "apps\desktop\src-tauri\resources\voice-api"

Set-Location $apiDir

Write-Host "==> uv sync (incl. pyinstaller)"
uv sync --group dev

Write-Host "==> PyInstaller onedir"
if (Test-Path (Join-Path $apiDir "dist\voice-api")) {
  Remove-Item -Recurse -Force (Join-Path $apiDir "dist\voice-api")
}
if (Test-Path (Join-Path $apiDir "build")) {
  Remove-Item -Recurse -Force (Join-Path $apiDir "build")
}

uv run pyinstaller --noconfirm --clean voice_api.spec

$built = Join-Path $apiDir "dist\voice-api"
if (-not (Test-Path (Join-Path $built "voice-api.exe"))) {
  throw "PyInstaller did not produce voice-api.exe in $built"
}

if (Test-Path $outRoot) {
  Remove-Item -Recurse -Force $outRoot
}
New-Item -ItemType Directory -Path $outRoot | Out-Null
Copy-Item -Recurse $built (Join-Path $outRoot "voice-api")

# Also stage next to Tauri resources for local release assembly.
if (Test-Path $resourceDir) {
  Remove-Item -Recurse -Force $resourceDir
}
New-Item -ItemType Directory -Path (Split-Path $resourceDir -Parent) -Force | Out-Null
Copy-Item -Recurse (Join-Path $outRoot "voice-api") $resourceDir

Write-Host "OK: $(Join-Path $outRoot 'voice-api\voice-api.exe')"
Write-Host "OK: $resourceDir\voice-api.exe"
