use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

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
    let snap = state.stop().map_err(|e| e.to_string())?;
    let _ = app.emit("dictation://status", &snap);
    Ok(snap)
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
