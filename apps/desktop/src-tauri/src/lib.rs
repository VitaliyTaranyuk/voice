mod audio;
mod cloud;
mod commands;
mod context;
mod history;
mod inject;
mod pipeline;
mod wav;

use commands::{
    cancel_dictation, get_runtime_info, get_session_status, list_history, ping, start_dictation,
    stop_dictation,
};
use history::HistoryStore;
use pipeline::PipelineState;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let history = HistoryStore::open_default().map_err(|e| e.to_string())?;
            app.manage(PipelineState::new(history));

            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                let ptt = Shortcut::new(
                    Some(Modifiers::CONTROL | Modifiers::SHIFT),
                    Code::Space,
                );

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |app, shortcut, event| {
                            if shortcut != &ptt {
                                return;
                            }
                            let Some(state) = app.try_state::<PipelineState>() else {
                                return;
                            };
                            match event.state() {
                                ShortcutState::Pressed => {
                                    if let Ok(snap) = state.start() {
                                        let _ = app.emit("dictation://status", snap);
                                    }
                                }
                                ShortcutState::Released => {
                                    let _ = state.stop_and_process(app.clone());
                                }
                            }
                        })
                        .build(),
                )?;

                app.global_shortcut().register(ptt)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            get_runtime_info,
            get_session_status,
            start_dictation,
            stop_dictation,
            cancel_dictation,
            list_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running Voice");
}
