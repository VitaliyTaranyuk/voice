use serde::Serialize;
use tauri::State;

use crate::pipeline::{DictationStatus, PipelineState, SessionSnapshot};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub app_name: String,
    pub version: String,
    pub platform: String,
    pub mvp_target: String,
    pub llm_provider: String,
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
    }
}

#[tauri::command]
pub fn get_session_status(state: State<'_, PipelineState>) -> SessionSnapshot {
    state.snapshot()
}

#[tauri::command]
pub fn start_dictation(state: State<'_, PipelineState>) -> Result<SessionSnapshot, String> {
    state.start().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_dictation(state: State<'_, PipelineState>) -> Result<SessionSnapshot, String> {
    state.stop().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_dictation(state: State<'_, PipelineState>) -> Result<SessionSnapshot, String> {
    state.cancel().map_err(|e| e.to_string())
}

#[allow(dead_code)]
pub fn status_label(status: DictationStatus) -> &'static str {
    match status {
        DictationStatus::Idle => "idle",
        DictationStatus::Recording => "recording",
        DictationStatus::Transcribing => "transcribing",
        DictationStatus::Refining => "refining",
        DictationStatus::Injecting => "injecting",
        DictationStatus::Completed => "completed",
        DictationStatus::Failed => "failed",
        DictationStatus::Cancelled => "cancelled",
    }
}
