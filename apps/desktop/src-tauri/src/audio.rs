use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use thiserror::Error;

const TARGET_SAMPLE_RATE: u32 = 16_000;
/// Decay applied to rolling level on each stats() poll so the waveform falls when quiet.
const LEVEL_DECAY: f32 = 0.62;

/// cpal marks Stream !Send for cross-platform; WASAPI stream is Send in practice.
/// Kept alive so the WASAPI callback runs; dropped on stop.
#[allow(dead_code)]
struct SendStream(Stream);
unsafe impl Send for SendStream {}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default input device")]
    NoInputDevice,
    #[error("unsupported sample format: {0:?}")]
    UnsupportedFormat(SampleFormat),
    #[error("audio device error: {0}")]
    Device(String),
    #[error("already capturing")]
    AlreadyCapturing,
    #[error("not capturing")]
    NotCapturing,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStats {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: usize,
    pub duration_ms: u64,
    /// Session-max amplitude (0..1).
    pub peak_amplitude: f32,
    /// Short-window level for live meters / waveform (0..1).
    pub level: f32,
}

struct CaptureInner {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    started_at: Instant,
    peak: f32,
    /// Rolling peak for UI; raised in the audio callback, decayed in stats().
    rolling: f32,
}

/// Microphone capture for Windows (cpal WASAPI).
/// Hot path keeps f32 mono frames in memory for later ASR streaming (M2).
pub struct MicCapture {
    active: AtomicBool,
    stream: Mutex<Option<SendStream>>,
    inner: Arc<Mutex<Option<CaptureInner>>>,
}

impl Default for MicCapture {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            stream: Mutex::new(None),
            inner: Arc::new(Mutex::new(None)),
        }
    }
}

impl MicCapture {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn start(&self) -> Result<CaptureStats, AudioError> {
        if self.is_active() {
            return Err(AudioError::AlreadyCapturing);
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        let supported = device
            .default_input_config()
            .map_err(|e| AudioError::Device(e.to_string()))?;

        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.clone().into();
        let sample_rate = config.sample_rate.0;
        let channels = config.channels;

        let inner = Arc::clone(&self.inner);
        *inner.lock().expect("audio mutex") = Some(CaptureInner {
            samples: Vec::with_capacity((sample_rate as usize) * 2),
            sample_rate,
            channels,
            started_at: Instant::now(),
            peak: 0.0,
            rolling: 0.0,
        });

        let err_fn = |err| eprintln!("Voice audio stream error: {err}");

        let stream = match sample_format {
            SampleFormat::F32 => device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| write_frames(inner.as_ref(), data, channels),
                    err_fn,
                    None,
                )
                .map_err(|e| AudioError::Device(e.to_string()))?,
            SampleFormat::I16 => device
                .build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let converted: Vec<f32> =
                            data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                        write_frames(inner.as_ref(), &converted, channels);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AudioError::Device(e.to_string()))?,
            SampleFormat::U16 => device
                .build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let converted: Vec<f32> = data
                            .iter()
                            .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                            .collect();
                        write_frames(inner.as_ref(), &converted, channels);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AudioError::Device(e.to_string()))?,
            other => return Err(AudioError::UnsupportedFormat(other)),
        };

        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;
        *self.stream.lock().expect("stream mutex") = Some(SendStream(stream));
        self.active.store(true, Ordering::SeqCst);

        Ok(CaptureStats {
            sample_rate,
            channels,
            frames: 0,
            duration_ms: 0,
            peak_amplitude: 0.0,
            level: 0.0,
        })
    }

    pub fn stop(&self) -> Result<(CaptureStats, Vec<f32>, u32), AudioError> {
        if !self.is_active() {
            return Err(AudioError::NotCapturing);
        }
        {
            let mut stream = self.stream.lock().expect("stream mutex");
            *stream = None; // drop stops WASAPI stream
        }
        self.active.store(false, Ordering::SeqCst);

        let mut guard = self.inner.lock().expect("audio mutex");
        let captured = guard.take().ok_or(AudioError::NotCapturing)?;
        // Whisper expects 16 kHz — downsample here to cut WAV size and ASR work.
        let samples = resample_mono(&captured.samples, captured.sample_rate, TARGET_SAMPLE_RATE);
        let stats = CaptureStats {
            sample_rate: TARGET_SAMPLE_RATE,
            channels: 1,
            frames: samples.len(),
            duration_ms: captured.started_at.elapsed().as_millis() as u64,
            peak_amplitude: captured.peak,
            level: captured.rolling,
        };
        Ok((stats, samples, TARGET_SAMPLE_RATE))
    }

    pub fn stats(&self) -> Option<CaptureStats> {
        let mut guard = self.inner.lock().expect("audio mutex");
        let captured = guard.as_mut()?;
        let level = captured.rolling;
        // Decay so quiet moments drop the waveform between polls.
        captured.rolling *= LEVEL_DECAY;
        Some(CaptureStats {
            sample_rate: captured.sample_rate,
            channels: captured.channels,
            frames: captured.samples.len(),
            duration_ms: captured.started_at.elapsed().as_millis() as u64,
            peak_amplitude: captured.peak,
            level,
        })
    }
}

/// Linear resample mono f32 to `to_rate` (Whisper target 16 kHz).
fn resample_mono(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || from_rate == to_rate {
        return samples.to_vec();
    }
    let ratio = f64::from(to_rate) / f64::from(from_rate);
    let out_len = ((samples.len() as f64) * ratio).round().max(1.0) as usize;
    let last = samples.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let i0 = (src.floor() as usize).min(last);
        let i1 = (i0 + 1).min(last);
        let t = (src - i0 as f64) as f32;
        out.push(samples[i0].mul_add(1.0 - t, samples[i1] * t));
    }
    out
}

fn write_frames(inner: &Mutex<Option<CaptureInner>>, data: &[f32], channels: u16) {
    let Ok(mut guard) = inner.lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };

    if channels <= 1 {
        for &sample in data {
            let a = sample.abs();
            state.peak = state.peak.max(a);
            state.rolling = state.rolling.max(a);
            state.samples.push(sample);
        }
        return;
    }

    // Downmix to mono for ASR path.
    for frame in data.chunks(channels as usize) {
        let sum: f32 = frame.iter().copied().sum();
        let mono = sum / channels as f32;
        let a = mono.abs();
        state.peak = state.peak.max(a);
        state.rolling = state.rolling.max(a);
        state.samples.push(mono);
    }
}
