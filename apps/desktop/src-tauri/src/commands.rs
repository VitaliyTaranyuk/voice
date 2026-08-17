use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::history::HistoryItem;
use crate::pipeline::{PipelineState, SessionSnapshot};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub app_name: String,
    pub version: String,
    pub platform: String,
    pub mvp_target: String,
    pub llm_provider: String,
    pub hotkey: String,
    pub api_base_url: String,
}

#[tauri::command]
pub fn ping() -> String {
    "pong".into()
}

#[tauri::command]
pub fn get_runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_name: "Voice".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        platform: "windows".into(),
        mvp_target: "windows".into(),
        llm_provider: "deepseek".into(),
        hotkey: crate::pipeline::DEFAULT_HOTKEY.into(),
        api_base_url: std::env::var("VOICE_API_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".into()),
    }
}

#[tauri::command]
pub fn get_session_status(state: State<'_, PipelineState>) -> SessionSnapshot {
    state.snapshot()
}

#[tauri::command]
pub fn start_dictation(
    app: AppHandle,
    state: State<'_, PipelineState>,
) -> Result<SessionSnapshot, String> {
    let snap = state.start().map_err(|e| e.to_string())?;
    crate::overlay::play_cue(crate::overlay::CueKind::Start);
    let _ = app.emit("dictation://status", &snap);
    crate::overlay::sync_overlay(&app, &snap);
    Ok(snap)
}

#[tauri::command]
pub fn stop_dictation(
    app: AppHandle,
    state: State<'_, PipelineState>,
) -> Result<SessionSnapshot, String> {
    state.stop_and_process(app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_dictation(
    app: AppHandle,
    state: State<'_, PipelineState>,
) -> Result<SessionSnapshot, String> {
    let snap = state.cancel().map_err(|e| e.to_string())?;
    let _ = app.emit("dictation://status", &snap);
    crate::overlay::sync_overlay(&app, &snap);
    Ok(snap)
}

#[tauri::command]
pub fn list_history(state: State<'_, PipelineState>) -> Result<Vec<HistoryItem>, String> {
    state.list_history(50).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("clipboard write failed: {e}"))
}

#[tauri::command]
pub fn copy_last_history(state: State<'_, PipelineState>) -> Result<String, String> {
    let items = state.list_history(1).map_err(|e| e.to_string())?;
    let Some(item) = items.into_iter().next() else {
        return Err("No dictations yet".into());
    };
    copy_text(item.text.clone())?;
    Ok(item.text)
}

#[tauri::command]
pub async fn check_api_health(state: State<'_, PipelineState>) -> Result<bool, String> {
    state.check_api_health().await.map_err(|e| e.to_string())
}

/// Stop the local API before the updater hands control to the NSIS installer.
///
/// The Windows updater ends in `std::process::exit(0)` immediately after
/// launching the installer (`tauri-plugin-updater`, `updater.rs::install_inner`),
/// so the `RunEvent::Exit` handler that normally stops the sidecar never runs.
/// Without this the sidecar outlives the app, keeps
/// `resources\voice-api\_internal\*.pyd` open, and the installer stops on
/// "Can't write to file".
///
/// The installer kills the sidecar too (`nsis-hooks.nsh`), and that is what
/// rescues apps already installed without this command. It is a backstop for
/// installs the app knows nothing about, not a licence to leak our own child.
#[tauri::command]
pub fn stop_local_api(app: AppHandle) {
    crate::api_boot::shutdown_local_api(app.path().resource_dir().ok().as_deref());
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub provider: String,
    pub configured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeySaveResult {
    pub keys: Vec<ApiKeyStatus>,
    /// What actually happened after storing the key. Always set: the restart is
    /// asynchronous, so an empty notice would let the window imply "already live"
    /// when the API is still coming back up.
    pub notice: Option<String>,
}

fn api_key_statuses() -> Vec<ApiKeyStatus> {
    crate::secrets::PROVIDERS
        .iter()
        .map(|provider| ApiKeyStatus {
            provider: (*provider).to_string(),
            configured: crate::secrets::is_configured(provider),
        })
        .collect()
}

/// Restart the API and say plainly what state the key is in.
fn applied(app: &AppHandle) -> ApiKeySaveResult {
    let notice = match crate::api_boot::restart_local_api(app.path().resource_dir().ok()) {
        Ok(()) => Some("Saved — restarting the local API to apply it.".to_string()),
        Err(e) => Some(format!("Saved, but not applied yet: {e}")),
    };
    ApiKeySaveResult {
        keys: api_key_statuses(),
        notice,
    }
}

/// Configured / not configured only. Key values never cross this boundary.
#[tauri::command]
pub fn api_key_status() -> Vec<ApiKeyStatus> {
    api_key_statuses()
}

#[tauri::command]
pub fn set_api_key(
    app: AppHandle,
    provider: String,
    key: String,
) -> Result<ApiKeySaveResult, String> {
    crate::secrets::set(&provider, &key)?;
    Ok(applied(&app))
}

#[tauri::command]
pub fn clear_api_key(app: AppHandle, provider: String) -> Result<ApiKeySaveResult, String> {
    crate::secrets::clear(&provider)?;
    Ok(applied(&app))
}
