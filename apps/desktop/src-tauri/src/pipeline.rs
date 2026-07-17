use std::sync::Mutex;

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Transcribing/Refining/Injecting/Failed wired in M1–M3
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
}

#[derive(Debug)]
struct Inner {
    session_id: Option<String>,
    status: DictationStatus,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            session_id: None,
            status: DictationStatus::Idle,
        }
    }
}

/// In-process dictation orchestrator stub (M0).
/// Real audio/ASR/inject land in M1–M3 behind the same state machine.
pub struct PipelineState {
    inner: Mutex<Inner>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }
}

impl PipelineState {
    pub fn snapshot(&self) -> SessionSnapshot {
        let guard = self.inner.lock().expect("pipeline mutex poisoned");
        SessionSnapshot {
            session_id: guard.session_id.clone(),
            status: guard.status,
            message: message_for(guard.status),
        }
    }

    pub fn start(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        match guard.status {
            DictationStatus::Idle
            | DictationStatus::Completed
            | DictationStatus::Failed
            | DictationStatus::Cancelled => {
                guard.session_id = Some(Uuid::new_v4().to_string());
                guard.status = DictationStatus::Recording;
                Ok(snapshot_from(&guard))
            }
            _ => Err(PipelineError::InvalidTransition),
        }
    }

    pub fn stop(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        if guard.status != DictationStatus::Recording {
            return Err(PipelineError::InvalidTransition);
        }
        // M0: skip real ASR/LLM; mark completed so UI can wire the happy path.
        guard.status = DictationStatus::Completed;
        Ok(snapshot_from(&guard))
    }

    pub fn cancel(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        if matches!(
            guard.status,
            DictationStatus::Idle | DictationStatus::Completed | DictationStatus::Cancelled
        ) {
            return Err(PipelineError::InvalidTransition);
        }
        guard.status = DictationStatus::Cancelled;
        Ok(snapshot_from(&guard))
    }
}

fn snapshot_from(inner: &Inner) -> SessionSnapshot {
    SessionSnapshot {
        session_id: inner.session_id.clone(),
        status: inner.status,
        message: message_for(inner.status),
    }
}

fn message_for(status: DictationStatus) -> String {
    match status {
        DictationStatus::Idle => "Ready".into(),
        DictationStatus::Recording => "Listening…".into(),
        DictationStatus::Transcribing => "Transcribing…".into(),
        DictationStatus::Refining => "Refining with DeepSeek…".into(),
        DictationStatus::Injecting => "Inserting text…".into(),
        DictationStatus::Completed => "Done (stub — audio/ASR in M1–M3)".into(),
        DictationStatus::Failed => "Failed".into(),
        DictationStatus::Cancelled => "Cancelled".into(),
    }
}

#[derive(Debug)]
pub enum PipelineError {
    InvalidTransition,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTransition => write!(f, "invalid dictation state transition"),
        }
    }
}

impl std::error::Error for PipelineError {}
