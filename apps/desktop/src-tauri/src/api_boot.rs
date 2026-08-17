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
        shutdown_local_api(resource_dir.as_deref());

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

/// Stop the local API: the child this process owns, plus any sidecar left behind
/// by an earlier run.
pub fn shutdown_local_api(resource_dir: Option<&Path>) {
    if let Ok(mut guard) = API_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    kill_stale_sidecar(resource_dir);
}

/// Terminate a `voice-api.exe` this process never spawned.
///
/// Not an exotic case — it is the usual one right after an update. The installer
/// relaunches Voice, `ensure_local_api_in_background` finds port 8787 already
/// answering and deliberately does not start a second sidecar, so `API_CHILD`
/// stays `None` while the previous run's `voice-api.exe` goes on holding
/// `resources\voice-api\_internal\*.pyd` open. Killing only the owned child would
/// therefore have left the 0.1.3 update failing exactly as it did: an orphan
/// (measured: pid 17620, parent long dead, still listening on 8787) reproduces
/// itself across every restart and blocks both the installer and local builds.
///
/// Scoped to the executable we would have spawned ourselves, compared by full
/// path. A `voice-api.exe` from another installation is not ours to kill, and
/// matching on the file name alone would cross the same line `restart_local_api`
/// refuses to cross when the API is not ours.
#[cfg(windows)]
fn kill_stale_sidecar(resource_dir: Option<&Path>) {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    let Some((exe, _)) = find_sidecar(resource_dir) else {
        return;
    };
    let target = normalize_path(&exe.to_string_lossy());
    let own_pid = std::process::id();

    unsafe {
        let mut pids = [0_u32; 4096];
        let mut needed = 0_u32;
        if EnumProcesses(
            pids.as_mut_ptr(),
            std::mem::size_of_val(&pids) as u32,
            &mut needed,
        )
        .is_err()
        {
            return;
        }
        let found = (needed as usize / std::mem::size_of::<u32>()).min(pids.len());

        for &pid in &pids[..found] {
            if pid == 0 || pid == own_pid {
                continue;
            }
            // Both rights at once: a process we may look at but not terminate is
            // one we would skip anyway.
            let Ok(handle) = OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                false,
                pid,
            ) else {
                continue;
            };

            let mut buf = [0_u16; 1024];
            let mut len = buf.len() as u32;
            let path = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok()
            .then(|| String::from_utf16_lossy(&buf[..len as usize]));

            if path.as_deref().is_some_and(|p| normalize_path(p) == target) {
                let _ = TerminateProcess(handle, 0);
                // Whoever called us is about to overwrite the files this process
                // holds open, so waiting for it to actually be gone is the point.
                let _ = WaitForSingleObject(handle, 5_000);
            }

            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
fn kill_stale_sidecar(_resource_dir: Option<&Path>) {}

/// Compare Windows paths the way `tray_promote` does: separators unified, case
/// folded. Same reason — two OS calls can hand back the same file with different
/// spelling.
#[cfg(windows)]
fn normalize_path(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
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
