# Start Voice API then print ready message.
# Usage: pwsh scripts/dev-api.ps1

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\..\services\api"

if (-not (Test-Path ".env")) {
  Copy-Item ".env.example" ".env"
  Write-Host "Created .env — add DEEPSEEK_API_KEY (ASR is local by default)"
}

uv sync
Write-Host "API → http://127.0.0.1:8787/docs"
uv run uvicorn app.main:app --reload --host 127.0.0.1 --port 8787
