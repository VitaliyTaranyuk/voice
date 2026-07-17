use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

static API_SPAWN_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// If local API is down, spawn uvicorn once (dev monorepo layout).
pub fn ensure_local_api_in_background() {
    if API_SPAWN_ATTEMPTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async {
        let api = crate::cloud::VoiceApi::from_env();
        if api.health().await.is_ok() {
            return;
        }
        if let Err(e) = spawn_local_api() {
            eprintln!("Voice: failed to start local API: {e}");
        }
    });
}

fn spawn_local_api() -> Result<(), String> {
    let root = find_repo_root().ok_or_else(|| "repo root not found (services/api missing)".to_string())?;
    let api_dir = root.join("services").join("api");
    if !api_dir.join("pyproject.toml").is_file() {
        return Err(format!("missing pyproject.toml in {}", api_dir.display()));
    }

    let env_path = api_dir.join(".env");
    if !env_path.is_file() {
        let example = api_dir.join(".env.example");
        if example.is_file() {
            std::fs::copy(&example, &env_path).map_err(|e| format!("copy .env: {e}"))?;
        }
    }

    let ps = find_powershell().ok_or_else(|| "powershell not found".to_string())?;
    let api_dir_str = api_dir.to_string_lossy().replace('\'', "''");
    let sync_step = if api_dir.join(".venv").is_dir() {
        String::new()
    } else {
        "uv sync; ".to_string()
    };
    let script = format!(
        "$env:Path = [System.Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [System.Environment]::GetEnvironmentVariable('Path','User'); Set-Location '{api_dir_str}'; {sync_step}uv run uvicorn app.main:app --reload --host 127.0.0.1 --port 8787"
    );

    Command::new(&ps)
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Minimized",
            "-Command",
            &script,
        ])
        .spawn()
        .map_err(|e| format!("spawn {ps}: {e}"))?;

    Ok(())
}

fn find_powershell() -> Option<String> {
    for name in ["pwsh", "powershell"] {
        if Command::new(name)
            .args(["-NoProfile", "-Command", "exit 0"])
            .output()
            .is_ok()
        {
            return Some(name.to_string());
        }
    }
    None
}

fn find_repo_root() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            candidates.push(ancestor.to_path_buf());
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Some(parent) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        candidates.push(parent.to_path_buf());
        if let Some(grand) = parent.parent() {
            candidates.push(grand.to_path_buf());
            if let Some(root) = grand.parent() {
                candidates.push(root.to_path_buf());
            }
        }
    }

    for dir in candidates {
        for ancestor in dir.ancestors() {
            let api = ancestor.join("services").join("api").join("pyproject.toml");
            if api.is_file() {
                return Some(ancestor.to_path_buf());
            }
        }
    }
    None
}
