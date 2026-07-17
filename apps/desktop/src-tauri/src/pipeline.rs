use std::sync::Mutex;

use serde::Serialize;
use uuid::Uuid;

use crate::audio::{CaptureStats, MicCapture};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Transcribing/Refining/Injecting/Failed wired in M2–M3
pub enum DictationStatus {
    Idle,
    Recording,
    Transcribing,
    Refining,
    Injecting,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub session_id: Option<String>,
    pub status: DictationStatus,
    pub message: String,
    pub audio: Option<CaptureStats>,
    pub hotkey: String,
}

#[derive(Debug)]
struct Inner {
    session_id: Option<String>,
    status: DictationStatus,
    last_audio: Option<CaptureStats>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            session_id: None,
            status: DictationStatus::Idle,
            last_audio: None,
        }
    }
}

pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+Space";

pub struct PipelineState {
    inner: Mutex<Inner>,
    mic: MicCapture,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            mic: MicCapture::default(),
        }
    }
}

impl PipelineState {
    pub fn snapshot(&self) -> SessionSnapshot {
        let guard = self.inner.lock().expect("pipeline mutex poisoned");
        let audio = if guard.status == DictationStatus::Recording {
            self.mic.stats()
        } else {
            guard.last_audio.clone()
        };
        SessionSnapshot {
            session_id: guard.session_id.clone(),
            status: guard.status,
            message: message_for(guard.status, audio.as_ref()),
            audio,
            hotkey: DEFAULT_HOTKEY.into(),
        }
    }

    pub fn start(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        match guard.status {
            DictationStatus::Idle
            | DictationStatus::Completed
            | DictationStatus::Failed
            | DictationStatus::Cancelled => {
                let stats = self.mic.start().map_err(PipelineError::Audio)?;
                guard.session_id = Some(Uuid::new_v4().to_string());
                guard.status = DictationStatus::Recording;
                guard.last_audio = Some(stats.clone());
                Ok(SessionSnapshot {
                    session_id: guard.session_id.clone(),
                    status: guard.status,
                    message: message_for(guard.status, Some(&stats)),
                    audio: Some(stats),
                    hotkey: DEFAULT_HOTKEY.into(),
                })
            }
            _ => Err(PipelineError::InvalidTransition),
        }
    }

    pub fn stop(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        if guard.status != DictationStatus::Recording {
            return Err(PipelineError::InvalidTransition);
        }
        let stats = self.mic.stop().map_err(PipelineError::Audio)?;
        guard.status = DictationStatus::Completed;
        guard.last_audio = Some(stats.clone());
        Ok(SessionSnapshot {
            session_id: guard.session_id.clone(),
            status: guard.status,
            message: message_for(guard.status, Some(&stats)),
            audio: Some(stats),
            hotkey: DEFAULT_HOTKEY.into(),
        })
    }

    pub fn cancel(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        if matches!(
            guard.status,
            DictationStatus::Idle | DictationStatus::Completed | DictationStatus::Cancelled
        ) {
            return Err(PipelineError::InvalidTransition);
        }
        if self.mic.is_active() {
            let _ = self.mic.stop();
        }
        guard.status = DictationStatus::Cancelled;
        Ok(SessionSnapshot {
            session_id: guard.session_id.clone(),
            status: guard.status,
            message: message_for(guard.status, None),
            audio: guard.last_audio.clone(),
            hotkey: DEFAULT_HOTKEY.into(),
        })
    }
}

fn message_for(status: DictationStatus, audio: Option<&CaptureStats>) -> String {
    match status {
        DictationStatus::Idle => "Ready — hold Ctrl+Shift+Space".into(),
        DictationStatus::Recording => {
            if let Some(a) = audio {
                format!("Listening… {:.1}s · peak {:.0}%", a.duration_ms as f32 / 1000.0, a.peak_amplitude * 100.0)
            } else {
                "Listening…".into()
            }
        }
        DictationStatus::Transcribing => "Transcribing…".into(),
        DictationStatus::Refining => "Refining with DeepSeek…".into(),
        DictationStatus::Injecting => "Inserting text…".into(),
        DictationStatus::Completed => {
            if let Some(a) = audio {
                format!(
                    "Captured {:.1}s ({} frames @ {} Hz) — ASR in M2",
                    a.duration_ms as f32 / 1000.0,
                    a.frames,
                    a.sample_rate
                )
            } else {
                "Done".into()
            }
        }
        DictationStatus::Failed => "Failed".into(),
        DictationStatus::Cancelled => "Cancelled".into(),
    }
}

#[derive(Debug)]
pub enum PipelineError {
    InvalidTransition,
    Audio(crate::audio::AudioError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTransition => write!(f, "invalid dictation state transition"),
            Self::Audio(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PipelineError {}
