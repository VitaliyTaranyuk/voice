//! Floating dictation status overlay window (non-activating).

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::pipeline::{DictationStatus, SessionSnapshot};

const ORB_W: f64 = 160.0;
const ORB_H: f64 = 160.0;

#[derive(Clone, Copy)]
pub enum CueKind {
    Start,
    Success,
    Fail,
}

pub fn play_cue(kind: CueKind) {
    #[cfg(windows)]
    {
        // MessageBeep is not exported by windows-rs 0.61 Win32_UI_WindowsAndMessaging.
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBeep(utype: u32) -> i32;
        }
        const MB_OK: u32 = 0x0000_0000;
        const MB_ICONHAND: u32 = 0x0000_0010;
        const MB_ICONASTERISK: u32 = 0x0000_0040;

        let flag = match kind {
            CueKind::Start => MB_OK,
            CueKind::Success => MB_ICONASTERISK,
            CueKind::Fail => MB_ICONHAND,
        };
        unsafe {
            let _ = MessageBeep(flag);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = kind;
    }
}

pub fn sync_overlay(app: &AppHandle, snap: &SessionSnapshot) {
    // #region agent log
    crate::agent_debug_log(
        "F",
        "overlay.rs:sync_overlay:dispatch",
        "scheduling sync_overlay on main thread",
        serde_json::json!({
            "status": format!("{:?}", snap.status),
            "thread": format!("{:?}", std::thread::current().id()),
        }),
    );
    // #endregion
    let app2 = app.clone();
    let snap = snap.clone();
    // Window show/hide/size must run on the UI thread — calling from tokio deadlocks on Windows.
    let _ = app.run_on_main_thread(move || {
        sync_overlay_on_main(&app2, &snap);
    });
}

fn sync_overlay_on_main(app: &AppHandle, snap: &SessionSnapshot) {
    // #region agent log
    let t0 = std::time::Instant::now();
    crate::agent_debug_log(
        "A",
        "overlay.rs:sync_overlay:enter",
        "sync_overlay enter",
        serde_json::json!({
            "status": format!("{:?}", snap.status),
            "thread": format!("{:?}", std::thread::current().id()),
        }),
    );
    // #endregion
    let Some(window) = app.get_webview_window("overlay") else {
        // #region agent log
        crate::agent_debug_log(
            "A",
            "overlay.rs:sync_overlay:no_window",
            "overlay window missing",
            serde_json::json!({ "elapsedMs": t0.elapsed().as_millis() as u64 }),
        );
        // #endregion
        return;
    };

    match snap.status {
        DictationStatus::Recording
        | DictationStatus::Transcribing
        | DictationStatus::Refining
        | DictationStatus::Failed
        | DictationStatus::Cancelled
        | DictationStatus::Completed
        | DictationStatus::Idle => {
            if matches!(snap.status, DictationStatus::Completed) {
                play_cue(CueKind::Success);
            }
            if matches!(snap.status, DictationStatus::Failed) {
                play_cue(CueKind::Fail);
            }
            // #region agent log
            crate::agent_debug_log(
                "A",
                "overlay.rs:sync_overlay:before_show",
                "about to position+show",
                serde_json::json!({
                    "elapsedMs": t0.elapsed().as_millis() as u64,
                    "thread": format!("{:?}", std::thread::current().id()),
                }),
            );
            // #endregion
            position_bottom_center(&window);
            show_without_activate(&window);
            // #region agent log
            crate::agent_debug_log(
                "A",
                "overlay.rs:sync_overlay:after_show",
                "position+show done",
                serde_json::json!({ "elapsedMs": t0.elapsed().as_millis() as u64 }),
            );
            // #endregion
        }
        // Hide before paste so overlay never competes for focus/caret.
        DictationStatus::Injecting => {
            let _ = window.hide();
        }
    }
    // #region agent log
    crate::agent_debug_log(
        "A",
        "overlay.rs:sync_overlay:exit",
        "sync_overlay exit",
        serde_json::json!({
            "status": format!("{:?}", snap.status),
            "elapsedMs": t0.elapsed().as_millis() as u64,
        }),
    );
    // #endregion
}

fn position_bottom_center(window: &WebviewWindow) {
    let _ = window.set_size(tauri::LogicalSize::new(ORB_W, ORB_H));
    if let Ok(Some(monitor)) = window.current_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let x = (f64::from(size.width) / scale - ORB_W) / 2.0;
        let y = f64::from(size.height) / scale - ORB_H - 48.0;
        let _ = window.set_position(tauri::PhysicalPosition::new(
            (x * scale) as i32,
            (y * scale) as i32,
        ));
    }
}

fn show_without_activate(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        };

        if let Ok(hwnd) = window.hwnd() {
            let hwnd = HWND(hwnd.0 as *mut _);
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
                );
            }
            return;
        }
    }

    let _ = window.show();
}
