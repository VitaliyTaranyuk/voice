use std::io::Cursor;

use hound::{SampleFormat, WavSpec, WavWriter};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WavError {
    #[error("wav encode failed: {0}")]
    Encode(String),
}

/// Encode mono f32 samples as 16-bit PCM WAV for Whisper/Deepgram.
pub fn encode_wav_pcm16(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, WavError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    {
        let mut writer =
            WavWriter::new(&mut cursor, spec).map_err(|e| WavError::Encode(e.to_string()))?;
        for &sample in samples {
            let clipped = sample.clamp(-1.0, 1.0);
            let i = (clipped * i16::MAX as f32) as i16;
            writer
                .write_sample(i)
                .map_err(|e| WavError::Encode(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| WavError::Encode(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}
