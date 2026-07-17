mod audio;
mod cloud;
mod commands;
mod context;
mod history;
mod inject;
mod pipeline;
mod wav;

use commands::{
    cancel_dictation, check_api_health, get_runtime_info, get_session_status, list_history, ping,
    start_dictation, stop_dictation,
};
use history::HistoryStore;
use pipeline::PipelineState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let history = HistoryStore::open_default().map_err(|e| e.to_string())?;
            app.manage(PipelineState::new(history));

            let show_i = MenuItem::with_id(app, "show", "Show Voice", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Voice — Ctrl+Shift+Space to dictate")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Close to tray (Aqua/Raycast-like background presence).
            if let Some(window) = app.get_webview_window("main") {
                let window_h = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_h.hide();
                    }
                });
            }

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
            list_history,
            check_api_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running Voice");
}
