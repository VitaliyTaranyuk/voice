# Smoke gate for desktop changes — run before treating a UI/pipeline fix as done.
# Usage: powershell -File scripts/smoke-desktop.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "== typecheck @voice/desktop ==" -ForegroundColor Cyan
pnpm --filter @voice/desktop typecheck
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "== unit: overlay radar geometry ==" -ForegroundColor Cyan
node --test (Join-Path $root "scripts\test-overlay-radar.mjs")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "== cargo test (history + unit) ==" -ForegroundColor Cyan
Push-Location (Join-Path $root "apps/desktop/src-tauri")
try {
  cargo test --quiet
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
  Pop-Location
}

Write-Host ""
Write-Host "Automated checks passed." -ForegroundColor Green
Write-Host "Manual smoke / regression (required for inject/history UX):" -ForegroundColor Yellow
Write-Host "  1. Frameless beige card (#F4F1ED): equal History | Push-to-talk, mic header, no Online dot"
Write-Host "  2. Idle Ready; recording Listening… orange + pulse flower overlay (no diamonds/rotation)"
Write-Host "  3. History opens; Last text tight under buttons"
Write-Host "  4. Focus Notepad -> dictate -> text at caret"
Write-Host "  5. API stop -> Offline banner only; no green LED in header"
Write-Host "  6. Failed insert shows Failed + text still in History"
