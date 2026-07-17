from __future__ import annotations

import httpx

from app.core.config import settings
from app.schemas.asr import AsrResponse


class AsrError(Exception):
    pass


class WhisperAsrAdapter:
    async def transcribe(self, *, audio: bytes, filename: str, content_type: str) -> AsrResponse:
        if not settings.openai_api_key:
            raise AsrError("OPENAI_API_KEY not configured")

        headers = {"Authorization": f"Bearer {settings.openai_api_key}"}
        files = {"file": (filename, audio, content_type)}
        data = {"model": settings.openai_whisper_model, "response_format": "json"}

        async with httpx.AsyncClient(base_url=settings.openai_base_url, timeout=120.0) as client:
            response = await client.post(
                "/v1/audio/transcriptions",
                headers=headers,
                files=files,
                data=data,
            )
            if response.status_code >= 400:
                raise AsrError(f"Whisper error: HTTP {response.status_code} {response.text[:200]}")
            payload = response.json()

        text = str(payload.get("text", "")).strip()
        if not text:
            raise AsrError("Whisper returned empty transcript")
        return AsrResponse(text=text, provider="openai_whisper", language=payload.get("language"))


class DeepgramAsrAdapter:
    async def transcribe(self, *, audio: bytes, filename: str, content_type: str) -> AsrResponse:
        if not settings.deepgram_api_key:
            raise AsrError("DEEPGRAM_API_KEY not configured")

        headers = {
            "Authorization": f"Token {settings.deepgram_api_key}",
            "Content-Type": content_type or "audio/wav",
        }
        params = {"model": "nova-2", "smart_format": "true", "punctuate": "true"}

        async with httpx.AsyncClient(base_url=settings.deepgram_base_url, timeout=120.0) as client:
            response = await client.post(
                "/v1/listen",
                headers=headers,
                params=params,
                content=audio,
            )
            if response.status_code >= 400:
                raise AsrError(f"Deepgram error: HTTP {response.status_code} {response.text[:200]}")
            payload = response.json()

        try:
            alt = payload["results"]["channels"][0]["alternatives"][0]
            text = str(alt.get("transcript", "")).strip()
            confidence = alt.get("confidence")
        except (KeyError, IndexError, TypeError) as exc:
            raise AsrError("Unexpected Deepgram response shape") from exc

        if not text:
            raise AsrError("Deepgram returned empty transcript")
        return AsrResponse(
            text=text,
            provider="deepgram",
            confidence=float(confidence) if confidence is not None else None,
        )


class AsrRouter:
    """Select ASR provider by config and available keys (Strategy)."""

    def __init__(self) -> None:
        self._whisper = WhisperAsrAdapter()
        self._deepgram = DeepgramAsrAdapter()

    async def transcribe(self, *, audio: bytes, filename: str, content_type: str) -> AsrResponse:
        if not audio:
            raise AsrError("Empty audio payload")

        provider = settings.asr_provider.lower().strip()
        errors: list[str] = []

        order: list[str]
        if provider == "openai":
            order = ["openai"]
        elif provider == "deepgram":
            order = ["deepgram"]
        else:
            order = ["openai", "deepgram"]

        for name in order:
            try:
                if name == "openai" and settings.openai_api_key:
                    return await self._whisper.transcribe(
                        audio=audio, filename=filename, content_type=content_type
                    )
                if name == "deepgram" and settings.deepgram_api_key:
                    return await self._deepgram.transcribe(
                        audio=audio, filename=filename, content_type=content_type
                    )
            except AsrError as exc:
                errors.append(str(exc))

        if not settings.openai_api_key and not settings.deepgram_api_key:
            # Dev fallback so desktop E2E can be exercised without cloud keys.
            return AsrResponse(
                text="",
                provider="passthrough",
                warnings=[
                    "No ASR API key configured (OPENAI_API_KEY or DEEPGRAM_API_KEY). "
                    "Set a key for real transcription."
                ],
            )

        raise AsrError("; ".join(errors) if errors else "No ASR provider available")
