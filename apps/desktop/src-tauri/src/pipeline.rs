use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::audio::{CaptureStats, MicCapture};
use crate::cloud::VoiceApi;
use crate::context::{detect_foreground, AppContext};
use crate::history::HistoryStore;
use crate::inject::inject_text;
use crate::input_target::{capture_focused, InputTarget};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    PushToTalk,
    Toggle,
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
    pub recording_mode: Option<RecordingMode>,
    pub can_insert: Option<bool>,
}

struct Inner {
    session_id: Option<String>,
    status: DictationStatus,
    last_audio: Option<CaptureStats>,
    raw_text: Option<String>,
    final_text: Option<String>,
    app_context: Option<AppContext>,
    input_target: Option<InputTarget>,
    recording_mode: Option<RecordingMode>,
    message_override: Option<String>,
    /// Bumped on each new take / cancel so abandoned ASR tasks cannot overwrite state.
    process_gen: u64,
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
            input_target: None,
            recording_mode: None,
            message_override: None,
            process_gen: 0,
        }
    }
}

pub const DEFAULT_HOTKEY: &str = "Left Ctrl+Space";

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

    pub fn set_recording_mode(&self, mode: RecordingMode) {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        guard.recording_mode = Some(mode);
    }

    pub fn start(&self) -> Result<SessionSnapshot, PipelineError> {
        self.start_with_mode(RecordingMode::PushToTalk)
    }

    pub fn start_with_mode(&self, mode: RecordingMode) -> Result<SessionSnapshot, PipelineError> {
        let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
        match guard.status {
            DictationStatus::Recording => Err(PipelineError::InvalidTransition),
            // Idle-like OR busy processing: start a new take (bumps gen → abandons in-flight ASR).
            DictationStatus::Idle
            | DictationStatus::Completed
            | DictationStatus::Failed
            | DictationStatus::Cancelled
            | DictationStatus::Transcribing
            | DictationStatus::Refining
            | DictationStatus::Injecting => {
                if self.mic.is_active() {
                    let _ = self.mic.stop();
                }
                let ctx = detect_foreground().ok();
                let target = capture_focused().ok();
                let can_insert = target.as_ref().map(|t| t.can_insert);
                let stats = self.mic.start().map_err(PipelineError::Audio)?;
                guard.process_gen = guard.process_gen.wrapping_add(1);
                guard.session_id = Some(Uuid::new_v4().to_string());
                guard.status = DictationStatus::Recording;
                guard.last_audio = Some(stats.clone());
                guard.raw_text = None;
                guard.final_text = None;
                guard.app_context = ctx;
                guard.input_target = target;
                guard.recording_mode = Some(mode);
                guard.message_override = if can_insert == Some(false) {
                    Some("No active text field — text will be kept if insert fails".into())
                } else {
                    None
                };
                Ok(snapshot_from(&guard, Some(stats)))
            }
        }
    }

    /// Stop capture and run ASR → DeepSeek refine → inject into saved InputTarget.
    pub fn stop_and_process(&self, app: AppHandle) -> Result<SessionSnapshot, PipelineError> {
        let (session_id, stats, samples, sample_rate, app_context, input_target, process_gen) = {
            let mut guard = self.inner.lock().expect("pipeline mutex poisoned");
            if guard.status != DictationStatus::Recording {
                return Err(PipelineError::InvalidTransition);
            }
            let (stats, samples, sample_rate) = self.mic.stop().map_err(PipelineError::Audio)?;

            // Accidental chord / tap: skip ASR entirely.
            if stats.duration_ms < 300 {
                guard.status = DictationStatus::Cancelled;
                guard.last_audio = Some(stats.clone());
                guard.recording_mode = None;
                guard.input_target = None;
                guard.message_override = Some("Too short — hold and speak".into());
                let snap = snapshot_from(&guard, Some(stats));
                drop(guard);
                let _ = app.emit("dictation://status", &snap);
                crate::overlay::sync_overlay(&app, &snap);
                schedule_idle_reset(
                    app.clone(),
                    Arc::clone(&self.inner),
                    DictationStatus::Cancelled,
                    120,
                );
                return Ok(snap);
            }

            // Mic open with no real speech — don't call ASR (avoids Whisper hallucinations).
            if is_mostly_silence(&samples, stats.peak_amplitude) {
                guard.status = DictationStatus::Cancelled;
                guard.last_audio = Some(stats.clone());
                guard.recording_mode = None;
                guard.input_target = None;
                guard.message_override = Some("No speech — nothing inserted".into());
                let snap = snapshot_from(&guard, Some(stats));
                drop(guard);
                let _ = app.emit("dictation://status", &snap);
                crate::overlay::sync_overlay(&app, &snap);
                schedule_idle_reset(
                    app.clone(),
                    Arc::clone(&self.inner),
                    DictationStatus::Cancelled,
                    120,
                );
                return Ok(snap);
            }

            let session_id = guard
                .session_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let app_context = guard.app_context.clone();
            let input_target = guard.input_target.clone();
            guard.status = DictationStatus::Transcribing;
            guard.last_audio = Some(stats.clone());
            guard.message_override = None;
            let process_gen = guard.process_gen;
            (
                session_id,
                stats,
                samples,
                sample_rate,
                app_context,
                input_target,
                process_gen,
            )
        };

        let snap = self.snapshot();
        let _ = app.emit("dictation://status", &snap);
        crate::overlay::sync_overlay(&app, &snap);

        let history = Arc::clone(&self.history);
        let api = self.api.clone();
        let inner = Arc::clone(&self.inner);

        tauri::async_runtime::spawn(async move {
            let emit_status = |status: DictationStatus, message: &str| {
                let snap = {
                    let Ok(mut guard) = inner.lock() else {
                        return;
                    };
                    if guard.process_gen != process_gen {
                        return;
                    }
                    guard.status = status;
                    guard.message_override = Some(message.to_string());
                    snapshot_from(&guard, guard.last_audio.clone())
                };
                // Never call Win32/Tauri window APIs while holding the pipeline mutex.
                let _ = app.emit("dictation://status", &snap);
                crate::overlay::sync_overlay(&app, &snap);
            };

            emit_status(DictationStatus::Transcribing, "Transcribing…");

            let outcome = process_dictation(
                api,
                history,
                Arc::clone(&inner),
                process_gen,
                session_id,
                samples,
                sample_rate,
                app_context,
                input_target,
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
            if guard.process_gen != process_gen {
                return;
            }
            if matches!(outcome.as_ref().err().map(String::as_str), Some(ABANDONED_ERR)) {
                return;
            }
            let terminal = match outcome {
                Ok(result) => {
                    guard.status = DictationStatus::Completed;
                    guard.raw_text = Some(result.raw_text);
                    guard.final_text = Some(result.final_text.clone());
                    guard.message_override = Some(result.message);
                    guard.last_audio = Some(stats);
                    guard.recording_mode = None;
                    guard.input_target = None;
                    DictationStatus::Completed
                }
                Err(err) if err == NO_SPEECH_ERR => {
                    // Filler-only / empty after sanitize — silent cancel, no inject.
                    guard.status = DictationStatus::Cancelled;
                    guard.message_override = Some("No speech — nothing inserted".into());
                    guard.last_audio = Some(stats);
                    guard.recording_mode = None;
                    guard.input_target = None;
                    DictationStatus::Cancelled
                }
                Err(err) => {
                    guard.status = DictationStatus::Failed;
                    guard.message_override = Some(err);
                    guard.last_audio = Some(stats);
                    guard.recording_mode = None;
                    DictationStatus::Failed
                }
            };
            let snap = snapshot_from(&guard, guard.last_audio.clone());
            drop(guard);
            let _ = app.emit("dictation://status", &snap);
            crate::overlay::sync_overlay(&app, &snap);
            let idle_delay_ms = match terminal {
                DictationStatus::Failed => 350,
                DictationStatus::Cancelled => 120,
                _ => 450,
            };
            schedule_idle_reset(app, Arc::clone(&inner), terminal, idle_delay_ms);
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
        guard.process_gen = guard.process_gen.wrapping_add(1);
        guard.status = DictationStatus::Cancelled;
        guard.recording_mode = None;
        guard.input_target = None;
        guard.message_override = None;
        Ok(snapshot_from(&guard, guard.last_audio.clone()))
    }

    /// Return to Idle after a terminal status if it hasn't changed.
    pub fn schedule_idle_after(&self, app: AppHandle, expected: DictationStatus, delay_ms: u64) {
        schedule_idle_reset(app, Arc::clone(&self.inner), expected, delay_ms);
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

/// Marker: take was superseded by a newer recording — drop without UI Failed.
const ABANDONED_ERR: &str = "__abandoned__";

async fn process_dictation<F>(
    api: VoiceApi,
    history: Arc<HistoryStore>,
    inner: Arc<Mutex<Inner>>,
    process_gen: u64,
    session_id: String,
    samples: Vec<f32>,
    sample_rate: u32,
    app_context: Option<AppContext>,
    input_target: Option<InputTarget>,
    mut on_phase: F,
) -> Result<ProcessOutcome, String>
where
    F: FnMut(ProcessPhase),
{
    let still_current = || {
        inner
            .lock()
            .map(|g| g.process_gen == process_gen)
            .unwrap_or(false)
    };
    if samples.len() < (sample_rate as usize / 10) {
        return Err("Recording too short — hold the hotkey and speak".into());
    }

    let wav = encode_wav_pcm16(&samples, sample_rate).map_err(|e| e.to_string())?;
    let locale = "ru";

    let asr = api.transcribe(wav, locale).await.map_err(|e| match e {
        crate::cloud::CloudError::EmptyTranscript => NO_SPEECH_ERR.to_string(),
        other => other.to_string(),
    })?;
    let Some(raw) = sanitize_transcript(&asr.text) else {
        return Err(NO_SPEECH_ERR.into());
    };
    if !still_current() {
        return Err(ABANDONED_ERR.into());
    }

    let final_text = if skip_refine_enabled() {
        raw.clone()
    } else {
        on_phase(ProcessPhase::Refining);

        let category = app_context
            .as_ref()
            .map(|c| c.app_category.as_str())
            .unwrap_or("other");
        let process = app_context.as_ref().and_then(|c| c.process_name.as_deref());
        let title = app_context.as_ref().and_then(|c| c.window_title.as_deref());

        // ADR-009: refine failure → raw ASR (never block inject on polish errors).
        let refined_text = match api.refine(&raw, locale, category, process, title).await {
            Ok(refined) => {
                let t = refined.text.trim().to_string();
                match sanitize_transcript(&t) {
                    Some(cleaned) => cleaned,
                    // Empty refine → raw (ADR-009). Filler-only refine → no insert.
                    None if t.is_empty() => raw.clone(),
                    None => return Err(NO_SPEECH_ERR.into()),
                }
            }
            Err(err) => {
                eprintln!("Voice: refine failed, using raw ASR: {err}");
                raw.clone()
            }
        };
        refined_text
    };

    if !still_current() {
        return Err(ABANDONED_ERR.into());
    }
    on_phase(ProcessPhase::Injecting);

    let save_history = |final_text: &str, raw: &str| {
        let app_id = app_context.as_ref().map(|c| c.app_id.as_str());
        if let Err(err) = history.insert(&session_id, final_text, Some(raw), app_id) {
            // EmptyText is impossible here after ASR guard; other DB errors should not block inject.
            eprintln!("Voice: history insert failed: {err}");
        }
    };

    match &input_target {
        Some(target) => {
            let can_insert = target.can_insert;
            let text = final_text.clone();
            let target = target.clone();
            // UIA / SendInput are blocking Win32 — keep them off the async runtime.
            let inject_result = tokio::task::spawn_blocking(move || inject_text(&text, &target))
                .await
                .map_err(|e| format!("inject join error: {e}"))?;
            if let Err(err) = inject_result {
                save_history(&final_text, &raw);
                if !can_insert {
                    return Err(format!(
                        "No active text field — saved to history: {}",
                        truncate(&final_text, 60)
                    ));
                }
                return Err(format!(
                    "Insert failed ({err}) — saved to history: {}",
                    truncate(&final_text, 60)
                ));
            }
        }
        None => {
            save_history(&final_text, &raw);
            return Err(format!(
                "No capture target — saved to history: {}",
                truncate(&final_text, 60)
            ));
        }
    }

    // History after inject — SQLite must not delay paste.
    save_history(&final_text, &raw);

    Ok(ProcessOutcome {
        message: format!("Inserted · {}", truncate(&final_text, 80)),
        raw_text: raw,
        final_text,
    })
}

/// After a terminal status, return to Idle so overlay/main UI don't stick.
fn schedule_idle_reset(
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    expected: DictationStatus,
    delay_ms: u64,
) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let Ok(mut guard) = inner.lock() else {
            return;
        };
        if guard.status != expected {
            return;
        }
        guard.status = DictationStatus::Idle;
        guard.message_override = None;
        let snap = snapshot_from(&guard, guard.last_audio.clone());
        drop(guard);
        let _ = app.emit("dictation://status", &snap);
        crate::overlay::sync_overlay(&app, &snap);
    });
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
        recording_mode: inner.recording_mode,
        can_insert: inner.input_target.as_ref().map(|t| t.can_insert),
    }
}

fn message_for(
    status: DictationStatus,
    audio: Option<&CaptureStats>,
    final_text: Option<&str>,
) -> String {
    match status {
        DictationStatus::Idle => "Ready — hold Left Ctrl+Space, speak, release".into(),
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

/// Marker: silence / filler-only — cancel without Failed UI.
const NO_SPEECH_ERR: &str = "__no_speech__";

/// Below this peak (0..1) treat capture as silence.
const SILENCE_PEAK: f32 = 0.02;
/// Mean abs amplitude below this (with low peak) → silence.
const SILENCE_MEAN: f32 = 0.004;

fn is_mostly_silence(samples: &[f32], peak: f32) -> bool {
    if samples.is_empty() || peak < SILENCE_PEAK {
        return true;
    }
    let mean = samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len() as f32;
    mean < SILENCE_MEAN && peak < 0.05
}

/// Drop filler-only ASR hallucinations ("аммм…", "um", …). Keep real words.
fn sanitize_transcript(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut kept: Vec<String> = Vec::new();
    for token in trimmed.split_whitespace() {
        if is_filler_token(token) {
            continue;
        }
        kept.push(token.to_string());
    }

    if kept.is_empty() {
        return None;
    }

    let joined = kept.join(" ");
    // Entire string is a prolonged filler without spaces (аммммммм).
    if is_filler_token(&joined) {
        return None;
    }
    Some(joined)
}

fn is_filler_token(token: &str) -> bool {
    // Keep numbers / mixed tokens ("5-10", "v2", "3.14") — only drop pure punctuation.
    if token.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let letters: String = token
        .chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(|c| c.to_lowercase())
        .collect();
    if letters.is_empty() {
        // Punctuation-only token — drop when scanning fillers.
        return true;
    }

    matches!(
        letters.as_str(),
        "а" | "ам" | "ум" | "эм" | "мм" | "ээ" | "аа" | "м" | "угу" | "ага" | "хм" | "хмм"
            | "hmm" | "hm" | "um" | "uh" | "ah" | "erm" | "uhh" | "umm" | "ahh"
    ) || is_prolonged_filler(&letters)
}

fn is_prolonged_filler(s: &str) -> bool {
    if s.chars().count() < 2 {
        return false;
    }
    let only_vocal = s.chars().all(|c| {
        matches!(
            c,
            'а' | 'м' | 'у' | 'э' | 'е' | 'ы' | 'a' | 'h' | 'u' | 'm' | 'e'
        )
    });
    if !only_vocal {
        return false;
    }
    // "амммм", "ээээ", "аааа", "ummm" — not real words like "музыка".
    let has_m = s.contains('м') || s.contains('m');
    let pure_vowel = s.chars().all(|c| matches!(c, 'а' | 'э' | 'е' | 'ы' | 'a' | 'e' | 'u'));
    (has_m || pure_vowel) && !is_real_short_word(s)
}

fn is_real_short_word(s: &str) -> bool {
    // Avoid stripping short real words that share filler letter sets.
    matches!(s, "мама" | "ума" | "мы" | "emu" | "me" | "am")
}

/// Skip DeepSeek refine when `VOICE_SKIP_REFINE=1`. Default: refine on (quality path).
fn skip_refine_enabled() -> bool {
    match std::env::var("VOICE_SKIP_REFINE") {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
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
