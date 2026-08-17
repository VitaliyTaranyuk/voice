# Build Voice NSIS installer for friends: Setup.exe with the local API inside.
#
# No API keys are baked into the installer. Each recipient adds their own key in
# Settings; it lands in their Windows Credential Manager and is handed to the API
# process at startup. A key written into the bundle would be readable in plain text
# by everyone who receives the file.
#
# Prerequisites: Node/pnpm, Rust, uv, and the updater signing key.
#
# The signing key is required, not optional: with createUpdaterArtifacts enabled
# an unsigned build produces artifacts no installed app will accept, and the
# failure would only surface later, on a user's machine. So it is checked first.
#
# Usage:
#   $env:TAURI_SIGNING_PRIVATE_KEY = "$HOME\.tauri\voice-updater.key"
#   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
#   pwsh scripts/build-release.ps1
#
# Output: dist/voice-release/ — Voice_*-setup.exe, Voice_*-setup.nsis.zip,
#         its .sig, and latest.json (the update manifest).

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$releaseDir = Join-Path $root "dist\voice-release"
$sidecarScript = Join-Path $PSScriptRoot "build-api-sidecar.ps1"
$tauriDir = Join-Path $root "apps\desktop\src-tauri"
$resourceApi = Join-Path $tauriDir "resources\voice-api"
$nsisDir = Join-Path $tauriDir "target\release\bundle\nsis"
$repo = "VitaliyTaranyuk/voice"

# Fail before spending ten minutes on a build whose artifacts would be useless.
if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
  throw @"
TAURI_SIGNING_PRIVATE_KEY is not set — the build would produce unsigned updater
artifacts that installed apps reject.

  `$env:TAURI_SIGNING_PRIVATE_KEY = "`$HOME\.tauri\voice-updater.key"
  `$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""

The key must be the same one whose public half sits in tauri.conf.json. Lose it
and existing installs can never be updated again — back it up.
"@
}
if ($null -eq $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
  # Empty is a valid password; unset is not — the CLI would prompt and hang.
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
}

function Write-RuntimeEnv([string]$Dir) {
  if (-not (Test-Path $Dir)) {
    throw "Sidecar dir missing: $Dir"
  }
  # Non-secret defaults only. API keys arrive as environment variables from the
  # desktop app, which reads them from the Credential Manager at spawn time, and
  # pydantic-settings ranks those above this file.
  # device=cpu, а не auto: CUDA в сайдкар намеренно не входит (см. voice_api.spec),
  # и с auto каждый старт начинался бы с заведомо неудачной попытки поднять модель
  # на GPU. Результат тот же, но без лишней задержки и пугающей строки в логе.
  $lines = @(
    "ASR_PROVIDER=local",
    "LOCAL_WHISPER_MODEL=small",
    "LOCAL_WHISPER_DEVICE=cpu",
    "LOCAL_WHISPER_COMPUTE_TYPE=int8"
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

Write-Host "==> 3/3 collect artifacts + update manifest → dist/voice-release"
if (Test-Path $releaseDir) {
  Remove-Item -Recurse -Force $releaseDir
}
New-Item -ItemType Directory -Path $releaseDir | Out-Null
foreach ($f in $setupFiles) {
  Copy-Item $f.FullName (Join-Path $releaseDir $f.Name)
}

# Updater artifacts: the app downloads the .nsis.zip, not the setup.exe, and
# verifies it against the .sig. Both must reach the release.
$zip = @(Get-ChildItem -Path $nsisDir -Filter "*-setup.nsis.zip" -ErrorAction SilentlyContinue)[0]
if (-not $zip) {
  throw "No *-setup.nsis.zip in $nsisDir — createUpdaterArtifacts is off, or the build did not sign"
}
$sig = Join-Path $nsisDir "$($zip.Name).sig"
if (-not (Test-Path $sig)) {
  throw "Missing signature $sig — TAURI_SIGNING_PRIVATE_KEY was not applied"
}
Copy-Item $zip.FullName (Join-Path $releaseDir $zip.Name)
Copy-Item $sig (Join-Path $releaseDir "$($zip.Name).sig")

# latest.json is not produced by `tauri build` — it has to be assembled here.
# Version comes from tauri.conf.json, NOT from a git tag: the updater compares
# plain semver against the running app, and a `v` prefix would never match.
$conf = Get-Content (Join-Path $tauriDir "tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version
$manifest = [ordered]@{
  version   = $version
  notes     = "См. https://github.com/$repo/releases/tag/v$version"
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = (Get-Content $sig -Raw).Trim()
      url       = "https://github.com/$repo/releases/download/v$version/$($zip.Name)"
    }
  }
}
$manifestPath = Join-Path $releaseDir "latest.json"
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path $manifestPath -Encoding UTF8
Write-Host "Wrote $manifestPath (version $version)"

Write-Host ""
Write-Host "OK artifacts:"
Get-ChildItem $releaseDir | ForEach-Object { Write-Host "  $($_.Name)" }
Write-Host ""
Write-Host "Publish ALL of them to the release, or updates break:"
Write-Host "  gh release create v$version --repo $repo --notes-file <notes> (Get-ChildItem '$releaseDir' | % FullName)"
Write-Host ""
Write-Host "  *-setup.exe  — first install, this is what the landing page links to"
Write-Host "  *.nsis.zip   — what installed apps download to update"
Write-Host "  *.sig        — signature checked against the pubkey in tauri.conf.json"
Write-Host "  latest.json  — the manifest the app polls via releases/latest/download"
Write-Host ""
Write-Host "Dictation works right away; refinement needs their own DeepSeek key in Settings."
Write-Host "Do not commit runtime.env or any build artifact to git."
