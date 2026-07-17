from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")

    app_name: str = "voice-api"
    app_version: str = "0.1.0"
    cors_origins: list[str] = [
        "http://localhost:1420",
        "http://127.0.0.1:1420",
        "tauri://localhost",
        "http://tauri.localhost",
    ]

    # LLM (ADR-013): DeepSeek only
    deepseek_api_key: str | None = None
    deepseek_base_url: str = "https://api.deepseek.com"
    deepseek_model: str = "deepseek-chat"

    # ASR providers (Whisper and/or Deepgram)
    asr_provider: str = "auto"  # auto | openai | deepgram
    openai_api_key: str | None = None
    openai_base_url: str = "https://api.openai.com"
    openai_whisper_model: str = "whisper-1"
    deepgram_api_key: str | None = None
    deepgram_base_url: str = "https://api.deepgram.com"


settings = Settings()
