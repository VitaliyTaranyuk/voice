use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CloudError {
    #[error("http error: {0}")]
    Http(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("empty transcript")]
    EmptyTranscript,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AsrResult {
    pub text: String,
    pub provider: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RefineResult {
    pub text: String,
    pub provider: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub applied_rules: Vec<String>,
}

#[derive(Clone)]
pub struct VoiceApi {
    base_url: String,
    client: reqwest::Client,
}

impl VoiceApi {
    pub fn from_env() -> Self {
        let base_url = std::env::var("VOICE_API_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".into())
            .trim_end_matches('/')
            .to_string();
        // Bound waits so Transcribing overlay cannot hang forever on a stuck API/ASR.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { base_url, client }
    }

    pub async fn health(&self) -> Result<(), CloudError> {
        let response = self
            .client
            .get(format!("{}/v1/health", self.base_url))
            .send()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;
        if !response.status().is_success() {
            return Err(CloudError::Api(format!("health {}", response.status())));
        }
        Ok(())
    }

    pub async fn transcribe(&self, wav: Vec<u8>, locale: &str) -> Result<AsrResult, CloudError> {
        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("dictation.wav")
            .mime_str("audio/wav")
            .map_err(|e| CloudError::Http(e.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("locale", locale.to_string());

        // #region agent log
        let t_send = std::time::Instant::now();
        let url = format!("{}/v1/ai/asr", self.base_url);
        crate::agent_debug_log(
            "C",
            "cloud.rs:transcribe:before_send",
            "POST /v1/ai/asr starting",
            serde_json::json!({ "url": url, "locale": locale }),
        );
        // #endregion
        let response = self
            .client
            .post(format!("{}/v1/ai/asr", self.base_url))
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                // #region agent log
                crate::agent_debug_log(
                    "C",
                    "cloud.rs:transcribe:send_error",
                    "POST /v1/ai/asr failed",
                    serde_json::json!({
                        "error": e.to_string(),
                        "elapsedMs": t_send.elapsed().as_millis() as u64,
                    }),
                );
                // #endregion
                CloudError::Http(e.to_string())
            })?;

        // #region agent log
        crate::agent_debug_log(
            "C",
            "cloud.rs:transcribe:after_send",
            "POST /v1/ai/asr got response headers",
            serde_json::json!({
                "status": response.status().as_u16(),
                "elapsedMs": t_send.elapsed().as_millis() as u64,
            }),
        );
        // #endregion

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CloudError::Api(format!("ASR {status}: {body}")));
        }

        let result: AsrResult = response
            .json()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;
        // #region agent log
        crate::agent_debug_log(
            "A",
            "cloud.rs:transcribe:parsed",
            "ASR JSON parsed",
            serde_json::json!({
                "provider": result.provider,
                "textLen": result.text.len(),
                "totalMs": t_send.elapsed().as_millis() as u64,
            }),
        );
        // #endregion
        if result.text.trim().is_empty() && result.provider == "passthrough" {
            return Err(CloudError::Api(
                result
                    .warnings
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "ASR passthrough: configure GROQ_API_KEY (or OPENAI/DEEPGRAM)".into()),
            ));
        }
        if result.text.trim().is_empty() {
            return Err(CloudError::EmptyTranscript);
        }
        Ok(result)
    }

    pub async fn refine(
        &self,
        raw_transcript: &str,
        locale: &str,
        app_category: &str,
        process_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Result<RefineResult, CloudError> {
        let body = serde_json::json!({
            "rawTranscript": raw_transcript,
            "locale": locale,
            "privacyMode": "cloud",
            "appContext": {
                "appId": process_name.unwrap_or("unknown"),
                "appCategory": app_category,
                "windowTitle": window_title,
                "processName": process_name,
            },
            "instructions": [],
            "dictionaryHints": [],
        });

        // #region agent log
        let t_refine = std::time::Instant::now();
        crate::agent_debug_log(
            "B",
            "cloud.rs:refine:before_send",
            "POST /v1/ai/refine starting",
            serde_json::json!({
                "url": format!("{}/v1/ai/refine", self.base_url),
                "rawLen": raw_transcript.len(),
            }),
        );
        // #endregion
        let response = self
            .client
            .post(format!("{}/v1/ai/refine", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                // #region agent log
                crate::agent_debug_log(
                    "B",
                    "cloud.rs:refine:send_error",
                    "POST /v1/ai/refine failed",
                    serde_json::json!({
                        "error": e.to_string(),
                        "elapsedMs": t_refine.elapsed().as_millis() as u64,
                    }),
                );
                // #endregion
                CloudError::Http(e.to_string())
            })?;

        // #region agent log
        crate::agent_debug_log(
            "B",
            "cloud.rs:refine:after_send",
            "POST /v1/ai/refine got response headers",
            serde_json::json!({
                "status": response.status().as_u16(),
                "elapsedMs": t_refine.elapsed().as_millis() as u64,
            }),
        );
        // #endregion

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CloudError::Api(format!("Refine {status}: {body}")));
        }

        let result = response
            .json()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;
        // #region agent log
        crate::agent_debug_log(
            "B",
            "cloud.rs:refine:parsed",
            "Refine JSON parsed",
            serde_json::json!({ "elapsedMs": t_refine.elapsed().as_millis() as u64 }),
        );
        // #endregion
        Ok(result)
    }
}
