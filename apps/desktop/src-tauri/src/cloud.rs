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
    #[error("unauthorized — set VOICE_API_KEY (or start local voice-api on :8787)")]
    Unauthorized,
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

#[derive(Debug, Deserialize)]
struct HealthBody {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    service: Option<String>,
}

#[derive(Clone)]
pub struct VoiceApi {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl VoiceApi {
    pub fn from_env() -> Self {
        let base_url = std::env::var("VOICE_API_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".into())
            .trim_end_matches('/')
            .to_string();
        let api_key = std::env::var("VOICE_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        // Bound waits so Transcribing overlay cannot hang forever on a stuck API/ASR.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url,
            api_key,
            client,
        }
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) if !key.is_empty() => req.bearer_auth(key),
            _ => req,
        }
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

        let body: HealthBody = response
            .json()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;
        let service = body.service.unwrap_or_default();
        let has_key = self.has_api_key();

        // Local voice-api is open. Cloud gateway on the same port needs a Bearer key.
        if service == "voice-cloud-gateway" && !has_key {
            return Err(CloudError::Api(
                "port 8787 is voice-cloud-gateway (needs VOICE_API_KEY) — stop it and start local voice-api, or set the key".into(),
            ));
        }
        if !service.is_empty() && service != "voice-api" && !has_key {
            return Err(CloudError::Api(format!(
                "unexpected API service '{service}' on {} — expected voice-api",
                self.base_url
            )));
        }
        let _ = body.status;
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

        let request = self
            .client
            .post(format!("{}/v1/ai/asr", self.base_url))
            .multipart(form);
        let response = self
            .apply_auth(request)
            .send()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;

        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(CloudError::Unauthorized);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CloudError::Api(format!("ASR {status}: {body}")));
        }

        let result: AsrResult = response
            .json()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;
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

        let request = self
            .client
            .post(format!("{}/v1/ai/refine", self.base_url))
            .json(&body);
        let response = self
            .apply_auth(request)
            .send()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;

        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(CloudError::Unauthorized);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CloudError::Api(format!("Refine {status}: {body}")));
        }

        let result = response
            .json()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?;
        Ok(result)
    }
}
