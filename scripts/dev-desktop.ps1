# Start Voice desktop (Tauri).
# Usage: pwsh scripts/dev-desktop.ps1

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\.."

pnpm install
pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build
pnpm --filter @voice/desktop exec tauri dev
