mod commands;
mod pipeline;

use commands::{
    cancel_dictation, get_runtime_info, get_session_status, ping, start_dictation, stop_dictation,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(pipeline::PipelineState::default())
        .invoke_handler(tauri::generate_handler![
            ping,
            get_runtime_info,
            get_session_status,
            start_dictation,
            stop_dictation,
            cancel_dictation
        ])
        .run(tauri::generate_context!())
        .expect("error while running Voice");
}
