# Start Voice desktop (Tauri).
# Usage: pwsh scripts/dev-desktop.ps1
# Refine on by default (DeepSeek polish). Set VOICE_SKIP_REFINE=1 to skip.

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\.."

pnpm install
pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build
pnpm --filter @voice/desktop exec tauri dev
