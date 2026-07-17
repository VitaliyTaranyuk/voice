# DEV launcher only: API + tauri dev. Do NOT use as a product shortcut under load.
# Prefer: one API terminal + one desktop terminal from Cursor.
# Usage: pwsh scripts/start-voice.ps1

$ErrorActionPreference = "Stop"

function Get-VoicePsHost {
  $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
  if ($null -ne $pwsh) {
    return $pwsh.Source
  }
  $powershell = Get-Command powershell -ErrorAction SilentlyContinue
  if ($null -ne $powershell) {
    return $powershell.Source
  }
  throw "PowerShell not found (pwsh / powershell)"
}

function Focus-ExistingVoiceWindow {
  try {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class VoiceWinActivate {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
}
"@ -ErrorAction SilentlyContinue
    $hwnd = [VoiceWinActivate]::FindWindow($null, "Voice")
    if ($hwnd -ne [IntPtr]::Zero) {
      [void][VoiceWinActivate]::ShowWindow($hwnd, 9)
      [void][VoiceWinActivate]::SetForegroundWindow($hwnd)
    }
  } catch {
    # best-effort focus
  }
}

# Prevent double-click / parallel launches (cargo + multiple keyboard hooks = system hang).
$launcherMutex = New-Object System.Threading.Mutex($false, "Local\com.voice.app.dev-launcher")
if (-not $launcherMutex.WaitOne(0)) {
  Write-Host "Voice launcher already running — not starting a second copy (protects the OS)."
  Focus-ExistingVoiceWindow
  exit 0
}

try {
  $root = Resolve-Path (Join-Path $PSScriptRoot "..")
  $psHost = Get-VoicePsHost

  Write-Host ""
  Write-Host "=== Voice DEV launcher ===" -ForegroundColor Yellow
  Write-Host "This starts compile + API. Double-clicking again is blocked."
  Write-Host "If the PC feels stuck: Task Manager → end voice-desktop / cargo / rustc."
  Write-Host ""

  # Refresh PATH (cargo / uv / pnpm after installs)
  $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
    [System.Environment]::GetEnvironmentVariable("Path", "User")
  $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
  if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
  }

  function Test-ApiHealthy {
    try {
      $res = Invoke-WebRequest -Uri "http://127.0.0.1:8787/v1/health" -UseBasicParsing -TimeoutSec 2
      return $res.StatusCode -eq 200
    } catch {
      return $false
    }
  }

  function Enter-VoiceVsDevShell {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) {
      return
    }
    $installPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $installPath) {
      return
    }
    $dll = Join-Path $installPath "Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
    if (-not (Test-Path $dll)) {
      return
    }
    Import-Module $dll
    Enter-VsDevShell -VsInstallPath $installPath -SkipAutomaticLocation -DevCmdArguments "-arch=x64" | Out-Null
    if (Test-Path $cargoBin) {
      $env:Path = "$cargoBin;$env:Path"
    }
  }

  function Test-VoiceBuildBusy {
    # Only cargo/rustc — rust-analyzer runs in Cursor and must not block launch.
    foreach ($name in @("cargo", "rustc")) {
      $procs = @(Get-Process -Name $name -ErrorAction SilentlyContinue)
      if ($procs.Count -gt 0) {
        return $true
      }
    }
    return $false
  }

  # --- API ---
  if (-not (Test-ApiHealthy)) {
    Write-Host "Starting API on http://127.0.0.1:8787 ..."
    $apiDir = Join-Path $root "services\api"
    if (-not (Test-Path (Join-Path $apiDir ".env"))) {
      Copy-Item (Join-Path $apiDir ".env.example") (Join-Path $apiDir ".env")
      Write-Host "Created services/api/.env - fill GROQ_API_KEY and DEEPSEEK_API_KEY"
    }

    $apiCmd = @"
Set-Location '$apiDir'
uv sync
uv run uvicorn app.main:app --reload --host 127.0.0.1 --port 8787
"@
    Start-Process -FilePath $psHost -ArgumentList @("-NoExit", "-NoProfile", "-Command", $apiCmd) -WindowStyle Minimized

    $deadline = (Get-Date).AddSeconds(60)
    while (-not (Test-ApiHealthy)) {
      if ((Get-Date) -gt $deadline) {
        Write-Warning "API did not become healthy within 60s - desktop will start anyway"
        break
      }
      Start-Sleep -Milliseconds 500
    }
  } else {
    Write-Host "API already running"
  }

  # --- Desktop ---
  Write-Host "Starting Voice desktop..."
  Enter-VoiceVsDevShell
  Set-Location $root

  if (-not (Test-Path (Join-Path $root "node_modules"))) {
    pnpm install
    pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build
  }

  $existingDesktop = @(Get-Process -Name "voice-desktop" -ErrorAction SilentlyContinue)
  $tauriParents = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -and ($_.CommandLine -match 'tauri.*dev' -or $_.CommandLine -match '@voice/desktop') })

  if ($existingDesktop.Count -gt 0) {
    Write-Host "Voice already running - focusing existing window"
    Focus-ExistingVoiceWindow
    exit 0
  }

  if ($tauriParents.Count -gt 0) {
    Write-Host "Voice is already starting (tauri dev) - skipping duplicate launch"
    exit 0
  }

  if (Test-VoiceBuildBusy) {
    Write-Host "Rust/cargo already compiling - not starting another Voice build (protects the OS)." -ForegroundColor Yellow
    Focus-ExistingVoiceWindow
    exit 0
  }

  pnpm --filter @voice/desktop exec tauri dev
} catch {
  Write-Host ""
  Write-Host "Launch failed: $($_.Exception.Message)" -ForegroundColor Red
  if ($_.ScriptStackTrace) {
    Write-Host $_.ScriptStackTrace
  }
  Read-Host "Press Enter to close"
  exit 1
} finally {
  if ($null -ne $launcherMutex) {
    try { [void]$launcherMutex.ReleaseMutex() } catch {}
    $launcherMutex.Dispose()
  }
}
