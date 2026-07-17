use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

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
    let _ = app.emit("dictation://status", &snap);
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
    Ok(snap)
}

#[tauri::command]
pub fn list_history(state: State<'_, PipelineState>) -> Result<Vec<HistoryItem>, String> {
    state.list_history(50).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_api_health(state: State<'_, PipelineState>) -> Result<bool, String> {
    state.check_api_health().await.map_err(|e| e.to_string())
}
