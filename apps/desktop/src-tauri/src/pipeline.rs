use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::audio::{CaptureStats, MicCapture};
use crate::cloud::VoiceApi;
use crate::context::{detect_foreground, AppContext};
use crate::history::HistoryStore;
use crate::inject::inject_text;
use crate::wav::encode_wav_pcm16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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
    pub raw_text: Option<String>,
    pub final_text: Option<String>,
    pub app_context: Option<AppContext>,
}

struct Inner {
    session_id: Option<String>,
    status: DictationStatus,
    last_audio: Option<CaptureStats>,
    raw_text: Option<String>,
    final_text: Option<String>,
    app_context: Option<AppContext>,
    message_override: Option<String>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            session_id: None,
            status: DictationStatus::Idle,
            last_audio: None,
            raw_text: None,
            final_text: None,
            app_context: None,
            message_override: None,
        }
    }
}

pub const DEFAULT_HOTKEY: &str = "Ctrl+Shift+Space";

pub struct PipelineState {
    inner: Arc<Mutex<Inner>>,
    mic: MicCapture,
    history: Arc<HistoryStore>,
    api: VoiceApi,
}

impl PipelineState {
    pub fn new(history: HistoryStore) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            mic: MicCapture::default(),
            history: Arc::new(history),
            api: VoiceApi::from_env(),
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let guard = self.inner.lock().expect("pipeline mutex poisoned");
        let audio = if guard.status == DictationStatus::Recording {
            self.mic.stats()
        } else {
            guard.last_audio.clone()
        };
        snapshot_from(&guard, audio)
    }

    pub fn start(&self) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        match guard.status {
            DictationStatus::Idle
            | DictationStatus::Completed
            | DictationStatus::Failed
            | DictationStatus::Cancelled => {
                let ctx = detect_foreground().ok();
                let stats = self.mic.start().map_err(PipelineError::Audio)?;
                guard.session_id = Some(Uuid::new_v4().to_string());
                guard.status = DictationStatus::Recording;
                guard.last_audio = Some(stats.clone());
                guard.raw_text = None;
                guard.final_text = None;
                guard.app_context = ctx;
                guard.message_override = None;
                Ok(snapshot_from(&guard, Some(stats)))
            }
            _ => Err(PipelineError::InvalidTransition),
        }
    }

    /// Stop capture and run ASR → DeepSeek refine → clipboard inject.
    pub fn stop_and_process(&self, app: AppHandle) -> Result<SessionSnapshot, PipelineError> {
        let (session_id, stats, samples, sample_rate, app_context) = {
            let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
            if guard.status != DictationStatus::Recording {
                return Err(PipelineError::InvalidTransition);
            }
            let (stats, samples, sample_rate) = self.mic.stop().map_err(PipelineError::Audio)?;
            let session_id = guard
                .session_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let app_context = guard.app_context.clone();
            guard.status = DictationStatus::Transcribing;
            guard.last_audio = Some(stats.clone());
            guard.message_override = None;
            (session_id, stats, samples, sample_rate, app_context)
        };

        let snap = self.snapshot();
        let _ = app.emit("dictation://status", &snap);

        let history = Arc::clone(&self.history);
        let api = self.api.clone();
        let inner = Arc::clone(&self.inner);

        tauri::async_runtime::spawn(async move {
            let emit_status = |status: DictationStatus, message: &str| {
                if let Ok(mut guard) = inner.lock() {
                    guard.status = status;
                    guard.message_override = Some(message.to_string());
                    let snap = snapshot_from(&guard, guard.last_audio.clone());
                    let _ = app.emit("dictation://status", snap);
                }
            };

            emit_status(DictationStatus::Transcribing, "Transcribing…");

            let outcome = process_dictation(
                api,
                history,
                session_id,
                samples,
                sample_rate,
                app_context,
                |phase| match phase {
                    ProcessPhase::Refining => {
                        emit_status(DictationStatus::Refining, "Refining with DeepSeek…")
                    }
                    ProcessPhase::Injecting => {
                        emit_status(DictationStatus::Injecting, "Inserting text…")
                    }
                },
            )
            .await;

            let mut guard = inner.lock().expect("pipeline mutex poisoned");
            match outcome {
                Ok(result) => {
                    guard.status = DictationStatus::Completed;
                    guard.raw_text = Some(result.raw_text);
                    guard.final_text = Some(result.final_text.clone());
                    guard.message_override = Some(result.message);
                    guard.last_audio = Some(stats);
                }
                Err(err) => {
                    guard.status = DictationStatus::Failed;
                    guard.message_override = Some(err);
                    guard.last_audio = Some(stats);
                }
            }
            let snap = snapshot_from(&guard, guard.last_audio.clone());
            drop(guard);
            let _ = app.emit("dictation://status", snap);
        });

        Ok(snap)
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
        guard.message_override = None;
        Ok(snapshot_from(&guard, guard.last_audio.clone()))
    }

    pub fn list_history(&self, limit: i64) -> Result<Vec<crate::history::HistoryItem>, String> {
        self.history.list_recent(limit).map_err(|e| e.to_string())
    }

    pub async fn check_api_health(&self) -> Result<bool, String> {
        self.api.health().await.map(|_| true).map_err(|e| e.to_string())
    }
}

enum ProcessPhase {
    Refining,
    Injecting,
}

struct ProcessOutcome {
    raw_text: String,
    final_text: String,
    message: String,
}

async fn process_dictation<F>(
    api: VoiceApi,
    history: Arc<HistoryStore>,
    session_id: String,
    samples: Vec<f32>,
    sample_rate: u32,
    app_context: Option<AppContext>,
    mut on_phase: F,
) -> Result<ProcessOutcome, String>
where
    F: FnMut(ProcessPhase),
{
    if samples.len() < (sample_rate as usize / 10) {
        return Err("Recording too short — hold the hotkey and speak".into());
    }

    let wav = encode_wav_pcm16(&samples, sample_rate).map_err(|e| e.to_string())?;
    let locale = "ru";

    let asr = api.transcribe(wav, locale).await.map_err(|e| e.to_string())?;
    let raw = asr.text.trim().to_string();
    if raw.is_empty() {
        return Err("Empty transcript from ASR".into());
    }

    on_phase(ProcessPhase::Refining);

    let category = app_context
        .as_ref()
        .map(|c| c.app_category.as_str())
        .unwrap_or("other");
    let process = app_context.as_ref().and_then(|c| c.process_name.as_deref());
    let title = app_context.as_ref().and_then(|c| c.window_title.as_deref());

    let refined = api
        .refine(&raw, locale, category, process, title)
        .await
        .map_err(|e| e.to_string())?;
    let final_text = {
        let t = refined.text.trim().to_string();
        if t.is_empty() {
            raw.clone()
        } else {
            t
        }
    };

    on_phase(ProcessPhase::Injecting);
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    inject_text(&final_text).map_err(|e| e.to_string())?;

    let app_id = app_context.as_ref().map(|c| c.app_id.as_str());
    let _ = history.insert(&session_id, &final_text, Some(&raw), app_id);

    Ok(ProcessOutcome {
        message: format!("Inserted · {}", truncate(&final_text, 80)),
        raw_text: raw,
        final_text,
    })
}

fn snapshot_from(inner: &Inner, audio: Option<CaptureStats>) -> SessionSnapshot {
    SessionSnapshot {
        session_id: inner.session_id.clone(),
        status: inner.status,
        message: inner.message_override.clone().unwrap_or_else(|| {
            message_for(inner.status, audio.as_ref(), inner.final_text.as_deref())
        }),
        audio,
        hotkey: DEFAULT_HOTKEY.into(),
        raw_text: inner.raw_text.clone(),
        final_text: inner.final_text.clone(),
        app_context: inner.app_context.clone(),
    }
}

fn message_for(
    status: DictationStatus,
    audio: Option<&CaptureStats>,
    final_text: Option<&str>,
) -> String {
    match status {
        DictationStatus::Idle => "Ready — hold Ctrl+Shift+Space, speak, release".into(),
        DictationStatus::Recording => {
            if let Some(a) = audio {
                format!(
                    "Listening… {:.1}s · peak {:.0}%",
                    a.duration_ms as f32 / 1000.0,
                    a.peak_amplitude * 100.0
                )
            } else {
                "Listening…".into()
            }
        }
        DictationStatus::Transcribing => "Transcribing…".into(),
        DictationStatus::Refining => "Refining with DeepSeek…".into(),
        DictationStatus::Injecting => "Inserting text…".into(),
        DictationStatus::Completed => final_text
            .map(|t| format!("Inserted · {}", truncate(t, 80)))
            .unwrap_or_else(|| "Done".into()),
        DictationStatus::Failed => "Failed".into(),
        DictationStatus::Cancelled => "Cancelled".into(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
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
