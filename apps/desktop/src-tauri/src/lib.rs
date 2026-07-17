mod audio;
mod commands;
mod pipeline;

use commands::{
    cancel_dictation, get_runtime_info, get_session_status, ping, start_dictation, stop_dictation,
};
use pipeline::PipelineState;
use tauri::{Emitter, Manager}; // Manager: try_state for PTT handler

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(PipelineState::default())
        .invoke_handler(tauri::generate_handler![
            ping,
            get_runtime_info,
            get_session_status,
            start_dictation,
            stop_dictation,
            cancel_dictation
        ])
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                // PTT default: Ctrl+Shift+Space (Aqua-like hold-to-talk, low conflict on Windows).
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
                                    if let Ok(snap) = state.stop() {
                                        let _ = app.emit("dictation://status", snap);
                                    }
                                }
                            }
                        })
                        .build(),
                )?;

                app.global_shortcut().register(ptt)?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Voice");
}
