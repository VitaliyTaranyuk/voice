mod api_boot;
mod audio;
mod cloud;
mod commands;
mod context;
mod history;
mod hotkeys;
mod inject;
mod input_target;
mod overlay;
mod pipeline;
mod tray_promote;
mod wav;

use commands::{
    cancel_dictation, check_api_health, copy_last_history, copy_text, get_runtime_info,
    get_session_status, list_history, ping, start_dictation, stop_dictation,
};
use history::HistoryStore;
use pipeline::PipelineState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    if visible && !minimized {
        let _ = window.hide();
        tray_promote::promote_voice_tray_icon_async();
        return;
    }
    show_main_window(app);
}

// #region agent log
pub(crate) fn agent_debug_log(
    hypothesis_id: &str,
    location: &str,
    message: &str,
    data: serde_json::Value,
) {
    use std::io::Write;
    let payload = serde_json::json!({
        "sessionId": "73f0f8",
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    });
    let line = payload.to_string();
    // Absolute workspace path first — Tauri CWD is often src-tauri/.
    let paths = [
        std::path::PathBuf::from(r"c:\Users\CyberPC\Desktop\Vibe\voice\debug-73f0f8.log"),
        std::path::PathBuf::from("debug-73f0f8.log"),
        std::path::PathBuf::from("../debug-73f0f8.log"),
        std::path::PathBuf::from("../../debug-73f0f8.log"),
    ];
    for path in &paths {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
            break;
        }
    }
}
// #endregion

/// Prevent multiple Voice windows: named mutex + focus existing main window.
/// HANDLE is Copy and does not auto-close; we keep the value so we never CloseHandle
/// until process exit (OS then releases the mutex).
#[cfg(windows)]
fn try_acquire_single_instance() -> bool {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
    };

    static HELD_MUTEX: AtomicIsize = AtomicIsize::new(0);

    unsafe {
        let handle = match CreateMutexW(None, true, w!("Local\\com.voice.app.single-instance")) {
            Ok(h) => h,
            Err(_) => return true,
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            if let Ok(hwnd) = FindWindowW(None, w!("Voice")) {
                if !hwnd.0.is_null() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            return false;
        }
        HELD_MUTEX.store(handle.0 as isize, Ordering::SeqCst);
        true
    }
}

#[cfg(not(windows))]
fn try_acquire_single_instance() -> bool {
    true
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if !try_acquire_single_instance() {
        return;
    }

    // Manage PipelineState on the Builder so it exists before webviews invoke commands.
    // Managing only inside setup races with early get_session_status from main/overlay.
    let history = match HistoryStore::open_default() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Voice: failed to open history db: {e}");
            return;
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(PipelineState::new(history))
        .setup(|app| {
            let show_i = MenuItem::with_id(app, "show", "Show Voice", true, None::<&str>)?;
            let history_i = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
            let copy_last_i =
                MenuItem::with_id(app, "copy_last", "Copy last", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &history_i, &copy_last_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Voice — Left Ctrl+Space to dictate")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => toggle_main_window(app),
                    "history" => {
                        if let Some(window) = app.get_webview_window("history") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "copy_last" => {
                        let state = app.state::<PipelineState>();
                        let _ = copy_last_history(state);
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
                        // Left click also opens the tray menu; only restore here (no hide).
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Prefer visible tray slot (not under ^ overflow). Order is still OS-owned.
            tray_promote::promote_voice_tray_icon_async();

            // Close to tray (Aqua/Raycast-like background presence).
            if let Some(window) = app.get_webview_window("main") {
                let window_h = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_h.hide();
                        tray_promote::promote_voice_tray_icon_async();
                    }
                });
            }

            if let Some(window) = app.get_webview_window("history") {
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
                hotkeys::start(app.handle().clone());
                api_boot::ensure_local_api_in_background();
            }

            // Always-on dormant orb so presence is visible before first hotkey.
            {
                let state = app.state::<PipelineState>();
                let snap = state.snapshot();
                overlay::sync_overlay(app.handle(), &snap);
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
            copy_text,
            copy_last_history,
            check_api_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running Voice");
}
