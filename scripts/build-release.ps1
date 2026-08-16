# Build Voice NSIS installer for friends: Setup.exe with the local API inside.
#
# No API keys are baked into the installer. Each recipient adds their own key in
# Settings; it lands in their Windows Credential Manager and is handed to the API
# process at startup. A key written into the bundle would be readable in plain text
# by everyone who receives the file.
#
# Prerequisites: Node/pnpm, Rust, uv.
#
# Usage:
#   pwsh scripts/build-release.ps1
#
# Output: dist/voice-release/Voice_*-setup.exe  (also copies from target/release/bundle/nsis)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$releaseDir = Join-Path $root "dist\voice-release"
$sidecarScript = Join-Path $PSScriptRoot "build-api-sidecar.ps1"
$tauriDir = Join-Path $root "apps\desktop\src-tauri"
$resourceApi = Join-Path $tauriDir "resources\voice-api"
$nsisDir = Join-Path $tauriDir "target\release\bundle\nsis"

function Write-RuntimeEnv([string]$Dir) {
  if (-not (Test-Path $Dir)) {
    throw "Sidecar dir missing: $Dir"
  }
  # Non-secret defaults only. API keys arrive as environment variables from the
  # desktop app, which reads them from the Credential Manager at spawn time, and
  # pydantic-settings ranks those above this file.
  $lines = @(
    "ASR_PROVIDER=local",
    "LOCAL_WHISPER_MODEL=small",
    "LOCAL_WHISPER_DEVICE=auto",
    "LOCAL_WHISPER_COMPUTE_TYPE=float16"
  )
  $path = Join-Path $Dir "runtime.env"
  Set-Content -Path $path -Value $lines -Encoding UTF8
  Write-Host "Wrote $path (no secrets)"
}

Write-Host "==> 1/3 sidecar API → src-tauri/resources/voice-api"
& $sidecarScript
if (-not (Test-Path (Join-Path $resourceApi "voice-api.exe"))) {
  throw "Missing $(Join-Path $resourceApi 'voice-api.exe') after sidecar build"
}
Write-RuntimeEnv $resourceApi

Write-Host "==> 2/3 tauri NSIS (bundles resources/voice-api, no keys)"
Set-Location $root
pnpm install
pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build
pnpm --filter @voice/desktop exec tauri build --bundles nsis

$setupFiles = @(Get-ChildItem -Path $nsisDir -Filter "*-setup.exe" -ErrorAction SilentlyContinue)
if ($setupFiles.Count -eq 0) {
  throw "No *-setup.exe in $nsisDir — tauri NSIS build failed"
}

Write-Host "==> 3/3 copy installer to dist/voice-release"
if (Test-Path $releaseDir) {
  Remove-Item -Recurse -Force $releaseDir
}
New-Item -ItemType Directory -Path $releaseDir | Out-Null
foreach ($f in $setupFiles) {
  Copy-Item $f.FullName (Join-Path $releaseDir $f.Name)
}

Write-Host ""
Write-Host "OK installer(s):"
Get-ChildItem $releaseDir -Filter "*-setup.exe" | ForEach-Object { Write-Host "  $($_.FullName)" }
Write-Host ""
Write-Host "Send the *-setup.exe to friends. They: Run → choose folder → Install."
Write-Host "Dictation works right away; refinement needs their own DeepSeek key in Settings."
Write-Host "Do not commit runtime.env or the setup exe to git."
