use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("input simulation error: {0}")]
    Input(String),
}

/// Clipboard-first text injection (ADR-008), then Ctrl+V via SendInput.
pub fn inject_text(text: &str) -> Result<(), InjectError> {
    let mut clipboard = Clipboard::new().map_err(|e| InjectError::Clipboard(e.to_string()))?;
    let previous = clipboard.get_text().ok();

    clipboard
        .set_text(text)
        .map_err(|e| InjectError::Clipboard(e.to_string()))?;

    // Brief delay so target app sees updated clipboard.
    thread::sleep(Duration::from_millis(40));

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| InjectError::Input(e.to_string()))?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| InjectError::Input(e.to_string()))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| InjectError::Input(e.to_string()))?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| InjectError::Input(e.to_string()))?;

    // Restore previous clipboard asynchronously-ish (best effort).
    if let Some(prev) = previous {
        thread::sleep(Duration::from_millis(80));
        let _ = clipboard.set_text(prev);
    }

    Ok(())
}
