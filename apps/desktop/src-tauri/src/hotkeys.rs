//! Global Left-Ctrl+Space dictation hotkeys via WH_KEYBOARD_LL.
//!
//! The low-level hook callback must return immediately. Any blocking work
//! (mic, ASR, inject) freezes keyboard input for the whole OS.

use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_ESCAPE, VK_LCONTROL, VK_LSHIFT, VK_RSHIFT, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::overlay;
use crate::pipeline::{DictationStatus, PipelineState, RecordingMode};

static APP: OnceLock<AppHandle> = OnceLock::new();
static ACTIONS: OnceLock<Sender<HotkeyAction>> = OnceLock::new();

#[derive(Debug)]
enum HotkeyAction {
    Start(RecordingMode),
    Stop,
    Cancel,
    UpgradeToToggle,
}

#[derive(Default)]
struct Keys {
    left_ctrl: bool,
    space: bool,
    shift: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChordMode {
    Idle,
    PushToTalk,
    Toggle,
}

struct ChordState {
    keys: Keys,
    mode: ChordMode,
    /// Space went down while Shift was already held → toggle chord.
    space_with_shift: bool,
    /// Rising-edge latch so Toggle stop fires once per chord press.
    toggle_chord_latched: bool,
}

static STATE: Mutex<ChordState> = Mutex::new(ChordState {
    keys: Keys {
        left_ctrl: false,
        space: false,
        shift: false,
    },
    mode: ChordMode::Idle,
    space_with_shift: false,
    toggle_chord_latched: false,
});

fn enqueue(action: HotkeyAction) {
    if let Some(tx) = ACTIONS.get() {
        let _ = tx.send(action);
    }
}

/// Install LL keyboard hook; heavy work runs on a worker thread.
pub fn start(app: AppHandle) {
    let _ = APP.set(app);

    let (tx, rx) = mpsc::channel::<HotkeyAction>();
    let _ = ACTIONS.set(tx);

    thread::spawn(move || {
        while let Ok(action) = rx.recv() {
            match action {
                HotkeyAction::Start(mode) => start_recording(mode),
                HotkeyAction::Stop => stop_recording(),
                HotkeyAction::Cancel => cancel_recording(),
                HotkeyAction::UpgradeToToggle => upgrade_to_toggle(),
            }
        }
    });

    thread::spawn(|| unsafe {
        let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
            Ok(h) => h,
            Err(_) => return,
        };
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    });
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let vk = info.vkCode as u16;
    let msg = wparam.0 as u32;
    let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

    if !down && !up {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if handle_key(vk, down) {
        return LRESULT(1);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn handle_key(vk: u16, down: bool) -> bool {
    // Never block the LL hook: a contended/poisoned lock must not stall OS input.
    let mut guard = match STATE.try_lock() {
        Ok(g) => g,
        Err(_) => return false,
    };

    // Escape cancels only while actively recording (PTT / Toggle).
    if vk == VK_ESCAPE.0 {
        if down && matches!(guard.mode, ChordMode::PushToTalk | ChordMode::Toggle) {
            guard.mode = ChordMode::Idle;
            guard.keys.space = false;
            guard.space_with_shift = false;
            guard.toggle_chord_latched = false;
            drop(guard);
            enqueue(HotkeyAction::Cancel);
            return true;
        }
        return false;
    }

    let is_shift = vk == VK_SHIFT.0 || vk == VK_LSHIFT.0 || vk == VK_RSHIFT.0;
    let is_lctrl = vk == VK_LCONTROL.0;
    let is_space = vk == VK_SPACE.0;

    if !is_lctrl && !is_space && !is_shift {
        return false;
    }

    if is_lctrl {
        guard.keys.left_ctrl = down;
    } else if is_space {
        if down {
            guard.space_with_shift = guard.keys.shift;
            guard.keys.space = true;
        } else {
            guard.keys.space = false;
        }
    } else if is_shift {
        guard.keys.shift = down;
    }

    let left_ctrl = guard.keys.left_ctrl;
    let space = guard.keys.space;
    let shift = guard.keys.shift;
    let space_with_shift = guard.space_with_shift;
    let mode = guard.mode;

    // Variant C: Shift during PTT → Toggle (release Ctrl/Space will not stop).
    if mode == ChordMode::PushToTalk && is_shift && down {
        guard.mode = ChordMode::Toggle;
        drop(guard);
        enqueue(HotkeyAction::UpgradeToToggle);
        return is_space;
    }

    // Toggle chord: Left Ctrl + Space + Shift
    if left_ctrl && space && (shift || space_with_shift) {
        if down && (is_space || is_shift || is_lctrl) && !guard.toggle_chord_latched {
            guard.toggle_chord_latched = true;
            match mode {
                ChordMode::Idle => {
                    guard.mode = ChordMode::Toggle;
                    drop(guard);
                    enqueue(HotkeyAction::Start(RecordingMode::Toggle));
                    return true;
                }
                ChordMode::Toggle => {
                    guard.mode = ChordMode::Idle;
                    drop(guard);
                    enqueue(HotkeyAction::Stop);
                    return true;
                }
                ChordMode::PushToTalk => {}
            }
        }
    } else if !left_ctrl || !space {
        guard.toggle_chord_latched = false;
    }

    // PTT start: Left Ctrl + Space (no Shift)
    if mode == ChordMode::Idle && left_ctrl && space && !shift && !space_with_shift && down && is_space
    {
        guard.mode = ChordMode::PushToTalk;
        drop(guard);
        enqueue(HotkeyAction::Start(RecordingMode::PushToTalk));
        return true;
    }

    // PTT release: either Ctrl or Space up
    if mode == ChordMode::PushToTalk && (is_lctrl || is_space) && !down {
        if guard.mode == ChordMode::PushToTalk {
            guard.mode = ChordMode::Idle;
            drop(guard);
            enqueue(HotkeyAction::Stop);
            return true;
        }
    }

    // Prevent Space leaking into the focused app while Left Ctrl is held.
    if is_space && left_ctrl {
        return true;
    }

    false
}

fn reset_chord_mode() {
    if let Ok(mut guard) = STATE.lock() {
        guard.mode = ChordMode::Idle;
        guard.toggle_chord_latched = false;
        guard.space_with_shift = false;
    }
}

fn start_recording(mode: RecordingMode) {
    let Some(app) = APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<PipelineState>() else {
        return;
    };
    match state.start_with_mode(mode) {
        Ok(snap) => {
            overlay::play_cue(overlay::CueKind::Start);
            let _ = app.emit("dictation://status", &snap);
            overlay::sync_overlay(app, &snap);
        }
        Err(_) => {
            // Chord was latched before start — reset so the next press can Start again.
            reset_chord_mode();
            overlay::play_cue(overlay::CueKind::Fail);
        }
    }
}

fn stop_recording() {
    let Some(app) = APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<PipelineState>() else {
        return;
    };
    let result = state.stop_and_process(app.clone());
    if result.is_err() {
        // Desync: chord thought we were recording. Unlock chord for the next Start.
        reset_chord_mode();
        overlay::play_cue(overlay::CueKind::Fail);
    }
}

fn cancel_recording() {
    let Some(app) = APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<PipelineState>() else {
        return;
    };
    if let Ok(snap) = state.cancel() {
        let _ = app.emit("dictation://status", &snap);
        overlay::sync_overlay(app, &snap);
        state.schedule_idle_after(app.clone(), DictationStatus::Cancelled, 120);
    }
}

fn upgrade_to_toggle() {
    let Some(app) = APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<PipelineState>() else {
        return;
    };
    state.set_recording_mode(RecordingMode::Toggle);
    let snap = state.snapshot();
    let _ = app.emit("dictation://status", &snap);
    overlay::sync_overlay(app, &snap);
}
