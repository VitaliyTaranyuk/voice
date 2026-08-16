import os
from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict


def _env_files() -> tuple[str, ...]:
    """runtime.env (release sidecar) → VOICE_ENV_FILE → .env (dev)."""
    found: list[str] = []
    override = os.environ.get("VOICE_ENV_FILE", "").strip()
    if override and Path(override).is_file():
        found.append(override)
    for name in ("runtime.env", ".env"):
        path = Path(name)
        if path.is_file() and str(path.resolve()) not in {str(Path(f).resolve()) for f in found}:
            found.append(str(path))
    return tuple(found) if found else (".env",)


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=_env_files(),
        env_file_encoding="utf-8",
        extra="ignore",
    )

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

    # ASR: local Whisper (default, no VPN) → optional cloud fallbacks
    asr_provider: str = "local"  # local | auto | groq | openai | deepgram
    # Speed/quality balance on CUDA; faster: base · quality: medium | large-v3
    local_whisper_model: str = "small"
    local_whisper_device: str = "cuda"  # cuda | cpu | auto
    local_whisper_compute_type: str = "float16"  # float16 | int8 | int8_float16
    groq_api_key: str | None = None
    groq_base_url: str = "https://api.groq.com/openai"
    groq_whisper_model: str = "whisper-large-v3-turbo"
    openai_api_key: str | None = None
    openai_base_url: str = "https://api.openai.com"
    openai_whisper_model: str = "whisper-1"
    deepgram_api_key: str | None = None
    deepgram_base_url: str = "https://api.deepgram.com"


settings = Settings()

