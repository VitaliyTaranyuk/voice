use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static API_SPAWN_ATTEMPTED: AtomicBool = AtomicBool::new(false);
static API_CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// If local API is down, spawn sidecar (release/NSIS) or monorepo uvicorn (dev) once.
pub fn ensure_local_api_in_background(resource_dir: Option<PathBuf>) {
    if API_SPAWN_ATTEMPTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let api = crate::cloud::VoiceApi::from_env();
        if api.health().await.is_ok() {
            return;
        }
        if let Err(e) = spawn_local_api(resource_dir.as_deref()) {
            eprintln!("Voice: failed to start local API: {e}");
        }
    });
}

/// Pass stored keys to the API child process through its environment.
///
/// Nothing is written to disk: `runtime.env` next to `voice-api.exe` holds only
/// non-secret settings, and pydantic-settings ranks environment variables above
/// `env_file`, so these win.
fn apply_secret_env(cmd: &mut Command) {
    for (var, value) in crate::secrets::env_pairs() {
        cmd.env(var, value);
    }
}

/// Restart the local API so a newly saved key reaches its environment.
///
/// A process restart is what it takes: `settings = Settings()` in
/// `app/core/config.py` reads the environment once at import, so changing a key
/// has no effect on a running process.
///
/// Errors when the API is not ours (in dev, `uvicorn` is often started by hand) —
/// pretending the key took effect would be a silent lie.
pub fn restart_local_api(resource_dir: Option<PathBuf>) -> Result<(), String> {
    let owned = API_CHILD
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);
    if !owned {
        return Err("local API was not started by the app — restart it yourself to apply the key"
            .into());
    }

    tauri::async_runtime::spawn(async move {
        shutdown_local_api();

        // Wait for the port to free up: a new process cannot bind 8787 while the
        // old one still holds it, and the API would simply fail to come back.
        let api = crate::cloud::VoiceApi::from_env();
        for _ in 0..20 {
            if api.health().await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        API_SPAWN_ATTEMPTED.store(false, Ordering::SeqCst);
        if let Err(e) = spawn_local_api(resource_dir.as_deref()) {
            eprintln!("Voice: failed to restart local API: {e}");
        }
    });
    Ok(())
}

pub fn shutdown_local_api() {
    let Ok(mut guard) = API_CHILD.lock() else {
        return;
    };
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn spawn_local_api(resource_dir: Option<&Path>) -> Result<(), String> {
    if let Some((exe, work_dir)) = find_sidecar(resource_dir) {
        return spawn_sidecar(&exe, &work_dir);
    }
    spawn_monorepo_api()
}

fn spawn_sidecar(exe: &Path, work_dir: &Path) -> Result<(), String> {
    let hf_home = voice_data_dir().join("hf-cache");
    let _ = std::fs::create_dir_all(&hf_home);

    let mut cmd = Command::new(exe);
    cmd.current_dir(work_dir)
        .env("HF_HOME", &hf_home)
        .env("VOICE_DATA_DIR", voice_data_dir());

    let runtime_env = work_dir.join("runtime.env");
    if runtime_env.is_file() {
        cmd.env("VOICE_ENV_FILE", &runtime_env);
    }

    apply_secret_env(&mut cmd);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let child = cmd
        .spawn()
        .map_err(|e| format!("spawn sidecar {}: {e}", exe.display()))?;

    if let Ok(mut guard) = API_CHILD.lock() {
        *guard = Some(child);
    }
    Ok(())
}

fn spawn_monorepo_api() -> Result<(), String> {
    let root =
        find_repo_root().ok_or_else(|| "repo root not found (services/api missing)".to_string())?;
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

    let mut cmd = Command::new(&ps);
    cmd.args([
        "-NoProfile",
        "-WindowStyle",
        "Minimized",
        "-Command",
        &script,
    ]);

    apply_secret_env(&mut cmd);

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let child = cmd
        .spawn()
        .map_err(|e| format!("spawn {ps}: {e}"))?;

    if let Ok(mut guard) = API_CHILD.lock() {
        *guard = Some(child);
    }
    Ok(())
}

fn find_sidecar(resource_dir: Option<&Path>) -> Option<(PathBuf, PathBuf)> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(res) = resource_dir {
        // NSIS / Tauri resource layouts
        dirs.push(res.join("voice-api"));
        dirs.push(res.join("resources").join("voice-api"));
        dirs.push(res.to_path_buf());
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("voice-api"));
            dirs.push(parent.join("resources").join("voice-api"));
            dirs.push(parent.to_path_buf());
            if let Some(grand) = parent.parent() {
                dirs.push(grand.join("voice-api"));
                dirs.push(grand.join("resources").join("voice-api"));
            }
        }
    }

    // Dev: after scripts/build-api-sidecar.ps1
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dirs.push(manifest.join("resources").join("voice-api"));
    if let Some(repo) = find_repo_root() {
        dirs.push(repo.join("dist").join("voice-sidecar").join("voice-api"));
    }

    for dir in dirs {
        let exe = dir.join("voice-api.exe");
        if exe.is_file() {
            return Some((exe, dir));
        }
    }
    None
}

fn voice_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Voice")
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
