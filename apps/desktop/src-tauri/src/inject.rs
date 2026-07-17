//! Multi-strategy text injection into a captured InputTarget.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use thiserror::Error;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_CONTROL, VK_V,
};

use crate::input_target::{
    prepare_target_for_inject, try_uia_insert, InputTarget, InputTargetError,
};

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("input simulation error: {0}")]
    Input(String),
    #[error("target error: {0}")]
    Target(#[from] InputTargetError),
    #[error("no active text field to insert into")]
    NoTextField,
}

/// Insert `text` into the InputTarget captured at dictation start.
///
/// Order: UIA ValuePattern (empty fields) → focus restore + clipboard paste → SendInput unicode.
pub fn inject_text(text: &str, target: &InputTarget) -> Result<(), InjectError> {
    if text.is_empty() {
        return Ok(());
    }

    if !target.can_insert {
        return Err(InjectError::NoTextField);
    }

    let element = prepare_target_for_inject(target)?;
    thread::sleep(Duration::from_millis(25));

    if let Some(ref el) = element {
        if try_uia_insert(el, text).is_ok() {
            return Ok(());
        }
    }

    // Re-focus right before paste — ASR can take seconds.
    let _ = prepare_target_for_inject(target)?;
    thread::sleep(Duration::from_millis(15));

    if clipboard_paste(text).is_ok() {
        return Ok(());
    }

    // Last resort: type unicode directly into the focused control.
    let _ = prepare_target_for_inject(target)?;
    thread::sleep(Duration::from_millis(15));
    send_input_unicode(text).map_err(InjectError::Input)?;
    Ok(())
}

fn clipboard_paste(text: &str) -> Result<(), InjectError> {
    let mut clipboard = Clipboard::new().map_err(|e| InjectError::Clipboard(e.to_string()))?;
    let previous = clipboard.get_text().ok();

    clipboard
        .set_text(text)
        .map_err(|e| InjectError::Clipboard(e.to_string()))?;

    thread::sleep(Duration::from_millis(20));

    send_ctrl_v().map_err(InjectError::Input)?;

    // Give the target app time to consume clipboard before we restore it.
    thread::sleep(Duration::from_millis(50));

    if let Some(prev) = previous {
        let _ = clipboard.set_text(prev);
    }

    Ok(())
}

fn send_ctrl_v() -> Result<(), String> {
    let inputs = [
        key_vk(VK_CONTROL, false),
        key_vk(VK_V, false),
        key_vk(VK_V, true),
        key_vk(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(format!("Ctrl+V SendInput sent {sent}/{}", inputs.len()));
    }
    Ok(())
}

fn send_input_unicode(text: &str) -> Result<(), String> {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(text.chars().count() * 2);
    for ch in text.encode_utf16() {
        inputs.push(unicode_key(ch, false));
        inputs.push(unicode_key(ch, true));
    }
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(format!("SendInput sent {sent}/{}", inputs.len()));
    }
    Ok(())
}

fn key_vk(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode_key(scan: u16, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_UNICODE
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
