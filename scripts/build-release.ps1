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
#   pwsh scripts/build-release.ps1
#
# The password is read from <key>.pass when the env var is unset, because Windows
# cannot hold an empty environment variable at all: both `$env:X = ''` and
# SetEnvironmentVariable(..., '') leave the variable non-existent (measured). An
# empty-password key would therefore always drop the build into an interactive
# prompt, and in CI or a background shell that is an indefinite hang, not an
# error. So the key carries a real password and the password lives in a file
# next to it.
#
# Be honest about what that buys: a password stored beside the key it unlocks
# adds no security over an unencrypted key — whoever can read one can read both.
# The protection here is filesystem access, nothing more. Its purpose is making
# builds non-interactive, not making the key safer.
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
if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
  $passFile = "$($env:TAURI_SIGNING_PRIVATE_KEY).pass"
  if (-not (Test-Path $passFile)) {
    throw @"
No signing password: TAURI_SIGNING_PRIVATE_KEY_PASSWORD is unset and $passFile
does not exist. Without it the CLI drops into an interactive prompt and the
build hangs instead of failing.
"@
  }
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content $passFile -Raw).Trim()
  if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
    throw "$passFile is empty — an empty password cannot be passed through the environment on Windows"
  }
  Write-Host "Signing password read from $passFile"
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

# Build-machine paths should not travel inside a binary handed to other people.
# Rust bakes source paths into panic metadata and debug info: the 0.1.2 build
# carried 780 occurrences of the developer's cargo registry path, so the account
# name was readable in the exe. Not a secret, but no reason to ship it.
#
# Computed here rather than pinned in .cargo/config.toml, because a config file
# would have to spell out one machine's absolute paths — hardcoding the very
# username this removes, and silently doing nothing on any other machine.
# Diagnostics survive: file names and line numbers stay, only the prefix changes.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
$env:RUSTFLAGS = @(
  "--remap-path-prefix=$(Join-Path $cargoHome 'registry\src')=/cargo/registry"
  "--remap-path-prefix=${root}=/voice"
) -join " "
Write-Host "Remapping build paths out of the binary"

pnpm --filter @voice/desktop exec tauri build --bundles nsis

# Version drives artifact selection: the bundle directory accumulates installers
# from previous builds, and picking "the first *-setup.exe" would happily publish
# a stale one under the new version's manifest.
$conf = Get-Content (Join-Path $tauriDir "tauri.conf.json") -Raw | ConvertFrom-Json
$version = $conf.version

$setup = @(Get-ChildItem -Path $nsisDir -Filter "*_${version}_*-setup.exe" -ErrorAction SilentlyContinue)[0]
if (-not $setup) {
  throw "No *_${version}_*-setup.exe in $nsisDir — tauri NSIS build failed"
}

# Tauri v2 signs the installer itself; there is no separate .nsis.zip (the docs
# still describe the older shape). So one artifact serves both the first install
# and the update, and the updater checks it against this .sig.
$sig = "$($setup.FullName).sig"
if (-not (Test-Path $sig)) {
  throw "Missing signature $sig — createUpdaterArtifacts is off, or the key was not applied"
}

Write-Host "==> 3/3 collect artifacts + update manifest → dist/voice-release"
if (Test-Path $releaseDir) {
  Remove-Item -Recurse -Force $releaseDir
}
New-Item -ItemType Directory -Path $releaseDir | Out-Null
Copy-Item $setup.FullName (Join-Path $releaseDir $setup.Name)
Copy-Item $sig (Join-Path $releaseDir "$($setup.Name).sig")

# latest.json is not produced by `tauri build` — it has to be assembled here.
# Version comes from tauri.conf.json, NOT from a git tag: the updater compares
# plain semver against the running app, and a `v` prefix would never match.
$manifest = [ordered]@{
  version   = $version
  notes     = "См. https://github.com/$repo/releases/tag/v$version"
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = (Get-Content $sig -Raw).Trim()
      url       = "https://github.com/$repo/releases/download/v$version/$($setup.Name)"
    }
  }
}
$manifestPath = Join-Path $releaseDir "latest.json"
# UTF-8 WITHOUT a BOM, deliberately: `Set-Content -Encoding UTF8` on Windows
# PowerShell 5.1 prepends one, a BOM is not valid JSON per RFC 8259, and the
# updater's parser rejects it — the manifest would be unreadable while looking
# perfectly fine in an editor.
[System.IO.File]::WriteAllText(
  $manifestPath,
  ($manifest | ConvertTo-Json -Depth 5),
  (New-Object System.Text.UTF8Encoding($false))
)
$firstBytes = [System.IO.File]::ReadAllBytes($manifestPath)[0..2]
if ($firstBytes[0] -eq 0xEF -and $firstBytes[1] -eq 0xBB -and $firstBytes[2] -eq 0xBF) {
  throw "latest.json was written with a BOM — the updater cannot parse it"
}
Write-Host "Wrote $manifestPath (version $version, no BOM)"

Write-Host ""
Write-Host "OK artifacts:"
Get-ChildItem $releaseDir | ForEach-Object { Write-Host "  $($_.Name)" }
Write-Host ""
Write-Host "Publish ALL of them to the release, or updates break:"
Write-Host "  gh release create v$version --repo $repo --notes-file <notes> (Get-ChildItem '$releaseDir' | % FullName)"
Write-Host ""
Write-Host "  *-setup.exe      — first install AND what installed apps download to update"
Write-Host "  *-setup.exe.sig  — signature checked against the pubkey in tauri.conf.json"
Write-Host "  latest.json      — the manifest the app polls via releases/latest/download"
Write-Host ""
Write-Host "Dictation works right away; refinement needs their own DeepSeek key in Settings."
Write-Host "Do not commit runtime.env or any build artifact to git."
