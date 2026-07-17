from __future__ import annotations

import asyncio
import io
import os
import sys
import threading
import time
import wave
from pathlib import Path
from typing import ClassVar

import httpx
import numpy as np

from app.core.config import settings
from app.schemas.asr import AsrResponse

# Cloud ASR HTTP bound — local Whisper has its own runtime cost.
_ASR_HTTP_TIMEOUT = 25.0


def _ensure_cuda_dll_path() -> None:
    """Add pip-shipped NVIDIA CUDA 12 DLLs to the process search path (Windows)."""
    if sys.platform != "win32":
        return

    bin_dirs: list[Path] = []
    for module_name in (
        "nvidia.cublas",
        "nvidia.cuda_runtime",
        "nvidia.cudnn",
        "nvidia.cuda_nvrtc",
    ):
        try:
            module = __import__(module_name, fromlist=["*"])
        except ImportError:
            continue
        for entry in getattr(module, "__path__", []):
            candidate = Path(entry) / "bin"
            if candidate.is_dir():
                bin_dirs.append(candidate)

    for directory in bin_dirs:
        os.add_dll_directory(str(directory))
        path = os.environ.get("PATH", "")
        prefix = str(directory)
        if prefix.lower() not in path.lower():
            os.environ["PATH"] = prefix + os.pathsep + path


class AsrError(Exception):
    pass


class LocalWhisperAsrAdapter:
    """On-device ASR via faster-whisper (CUDA when available). No VPN required."""

    _model: ClassVar[object | None] = None
    _lock: ClassVar[threading.Lock] = threading.Lock()
    _runtime: ClassVar[str] = ""
    _cuda_path_ready: ClassVar[bool] = False

    @staticmethod
    def _wav_bytes_to_float32(audio: bytes) -> np.ndarray:
        """Decode mono PCM16 WAV to float32 in-memory (no tempfile)."""
        with wave.open(io.BytesIO(audio), "rb") as wf:
            channels = wf.getnchannels()
            width = wf.getsampwidth()
            frames = wf.readframes(wf.getnframes())
        if width != 2:
            raise AsrError(f"Unsupported WAV sample width: {width}")
        samples = np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0
        if channels > 1:
            samples = samples.reshape(-1, channels).mean(axis=1)
        return samples

    @staticmethod
    def _warmup(model: object) -> None:
        """Force a real decode so missing cuBLAS fails at load-time, not mid-request."""
        silence = np.zeros(1600, dtype=np.float32)  # 100 ms @ 16 kHz
        segments, _info = model.transcribe(  # type: ignore[attr-defined]
            silence,
            language="ru",
            beam_size=1,
            vad_filter=False,
            without_timestamps=True,
        )
        # Consume generator — this is where CUDA/cuBLAS errors surface.
        for _ in segments:
            pass

    def _load_model(self, *, device: str, compute: str, model_size: str) -> object:
        from faster_whisper import WhisperModel

        if device != "cpu" and not LocalWhisperAsrAdapter._cuda_path_ready:
            _ensure_cuda_dll_path()
            LocalWhisperAsrAdapter._cuda_path_ready = True

        model = WhisperModel(model_size, device=device, compute_type=compute)
        self._warmup(model)
        return model

    def _ensure_model(self) -> object:
        if LocalWhisperAsrAdapter._model is not None:
            return LocalWhisperAsrAdapter._model

        with LocalWhisperAsrAdapter._lock:
            if LocalWhisperAsrAdapter._model is not None:
                return LocalWhisperAsrAdapter._model

            model_size = settings.local_whisper_model
            device = settings.local_whisper_device.strip().lower()
            compute = settings.local_whisper_compute_type.strip().lower()
            if device == "auto":
                device = "cuda"

            if device != "cpu":
                try:
                    LocalWhisperAsrAdapter._model = self._load_model(
                        device=device, compute=compute, model_size=model_size
                    )
                    LocalWhisperAsrAdapter._runtime = f"{device}/{compute}/{model_size}"
                    return LocalWhisperAsrAdapter._model
                except Exception as cuda_exc:
                    # Driver without CUDA toolkit (missing cublas64_12.dll) → CPU.
                    print(f"Voice API: CUDA Whisper unavailable ({cuda_exc}); falling back to CPU")

            # CPU: keep multilingual sizes only (distil-* is English-only).
            cpu_ok = {"tiny", "base", "small", "medium"}
            cpu_model = model_size if model_size in cpu_ok else "medium"
            LocalWhisperAsrAdapter._model = self._load_model(
                device="cpu", compute="int8", model_size=cpu_model
            )
            LocalWhisperAsrAdapter._runtime = f"cpu/int8/{cpu_model}"
            return LocalWhisperAsrAdapter._model

    def _transcribe_sync(
        self, *, audio: bytes, language: str
    ) -> tuple[str, str | None, list[str]]:
        model = self._ensure_model()
        samples = self._wav_bytes_to_float32(audio)

        duration_s = len(samples) / 16000.0
        # Quiet mics (peak~0.1) make Whisper/VAD drop or garble speech — boost first.
        peak = float(np.max(np.abs(samples))) if samples.size else 0.0
        if peak > 1e-4:
            samples = np.clip(samples * (0.9 / peak), -1.0, 1.0)
        # Sensitive VAD after normalize: split on speech, don't skip quiet talk.
        use_vad = duration_s >= 1.5
        t0 = time.perf_counter()
        text = self._transcribe_one(
            model, samples, language=language or "ru", use_vad=use_vad
        )
        elapsed_ms = int((time.perf_counter() - t0) * 1000)
        print(
            f"Voice API: ASR {LocalWhisperAsrAdapter._runtime} "
            f"{elapsed_ms}ms · {duration_s:.1f}s audio · vad={use_vad}"
        )
        lang = language or "ru"
        warnings: list[str] = []
        if LocalWhisperAsrAdapter._runtime.startswith("cpu/"):
            warnings.append(f"local_whisper runtime: {LocalWhisperAsrAdapter._runtime}")
        return text, lang, warnings

    def _transcribe_one(
        self,
        model: object,
        samples: np.ndarray,
        *,
        language: str,
        use_vad: bool,
    ) -> str:
        kwargs: dict = {
            "language": language,
            "beam_size": 5,
            "best_of": 5,
            "temperature": 0.0,
            "vad_filter": use_vad,
            "no_speech_threshold": 0.75,
            "compression_ratio_threshold": 2.4,
            "log_prob_threshold": -1.2,
            "condition_on_previous_text": False,
            "without_timestamps": False,
        }
        if use_vad:
            kwargs["vad_parameters"] = {
                "threshold": 0.3,
                "min_speech_duration_ms": 250,
                "min_silence_duration_ms": 400,
                "speech_pad_ms": 400,
            }
        segments, _info = model.transcribe(samples, **kwargs)  # type: ignore[attr-defined]
        parts: list[str] = []
        for segment in segments:
            piece = str(getattr(segment, "text", "") or "").strip()
            if piece:
                parts.append(piece)
        return " ".join(parts).strip()

    async def transcribe(
        self,
        *,
        audio: bytes,
        filename: str,
        content_type: str,
        language: str = "ru",
    ) -> AsrResponse:
        _ = filename, content_type
        try:
            text, lang, warnings = await asyncio.wait_for(
                asyncio.to_thread(
                    self._transcribe_sync,
                    audio=audio,
                    language=language,
                ),
                timeout=90.0,
            )
        except TimeoutError as exc:
            raise AsrError("Local Whisper timed out after 90s") from exc
        except Exception as exc:
            # If a stale CUDA model was cached before fallback existed, reset once.
            err = str(exc).lower()
            if "cublas" in err or "cuda" in err:
                with LocalWhisperAsrAdapter._lock:
                    LocalWhisperAsrAdapter._model = None
                    LocalWhisperAsrAdapter._runtime = ""
                try:
                    text, lang, warnings = await asyncio.wait_for(
                        asyncio.to_thread(
                            self._transcribe_sync,
                            audio=audio,
                            language=language,
                        ),
                        timeout=90.0,
                    )
                except Exception as retry_exc:
                    raise AsrError(f"Local Whisper failed: {retry_exc}") from retry_exc
            else:
                raise AsrError(f"Local Whisper failed: {exc}") from exc

        if not text:
            raise AsrError("Local Whisper returned empty transcript")

        return AsrResponse(
            text=text,
            provider="local_whisper",
            language=lang,
            warnings=warnings,
        )


class GroqAsrAdapter:
    """Whisper ASR via Groq OpenAI-compatible API."""

    async def transcribe(self, *, audio: bytes, filename: str, content_type: str) -> AsrResponse:
        if not settings.groq_api_key:
            raise AsrError("GROQ_API_KEY not configured")

        headers = {"Authorization": f"Bearer {settings.groq_api_key}"}
        files = {"file": (filename, audio, content_type)}
        data = {"model": settings.groq_whisper_model, "response_format": "json"}
        timeout = httpx.Timeout(connect=5.0, read=_ASR_HTTP_TIMEOUT, write=15.0, pool=5.0)

        try:
            async with httpx.AsyncClient(
                base_url=settings.groq_base_url, timeout=timeout
            ) as client:
                response = await client.post(
                    "/v1/audio/transcriptions",
                    headers=headers,
                    files=files,
                    data=data,
                )
                status = response.status_code
                body_preview = response.text[:200]
                payload = response.json() if status < 400 else None
        except httpx.TimeoutException as exc:
            raise AsrError(f"Groq ASR timeout after {_ASR_HTTP_TIMEOUT:.0f}s") from exc
        except httpx.HTTPError as exc:
            raise AsrError(f"Groq ASR network error: {exc}") from exc

        if status >= 400:
            if status == 403:
                raise AsrError(
                    "Groq ASR Forbidden (403) — GROQ_API_KEY invalid/expired or VPN required"
                )
            raise AsrError(f"Groq ASR error: HTTP {status} {body_preview}")
        if not isinstance(payload, dict):
            raise AsrError("Groq returned invalid JSON")
        text = str(payload.get("text", "")).strip()
        if not text:
            raise AsrError("Groq returned empty transcript")
        return AsrResponse(text=text, provider="groq_whisper", language=payload.get("language"))


class WhisperAsrAdapter:
    async def transcribe(self, *, audio: bytes, filename: str, content_type: str) -> AsrResponse:
        if not settings.openai_api_key:
            raise AsrError("OPENAI_API_KEY not configured")

        headers = {"Authorization": f"Bearer {settings.openai_api_key}"}
        files = {"file": (filename, audio, content_type)}
        data = {"model": settings.openai_whisper_model, "response_format": "json"}

        try:
            async with httpx.AsyncClient(
                base_url=settings.openai_base_url, timeout=_ASR_HTTP_TIMEOUT
            ) as client:
                response = await client.post(
                    "/v1/audio/transcriptions",
                    headers=headers,
                    files=files,
                    data=data,
                )
        except httpx.TimeoutException as exc:
            raise AsrError(f"OpenAI ASR timeout after {_ASR_HTTP_TIMEOUT:.0f}s") from exc
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

        try:
            async with httpx.AsyncClient(
                base_url=settings.deepgram_base_url, timeout=_ASR_HTTP_TIMEOUT
            ) as client:
                response = await client.post(
                    "/v1/listen",
                    headers=headers,
                    params=params,
                    content=audio,
                )
        except httpx.TimeoutException as exc:
            raise AsrError(f"Deepgram ASR timeout after {_ASR_HTTP_TIMEOUT:.0f}s") from exc
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
    """Select ASR provider by config (Strategy). Default: local Whisper."""

    def __init__(self) -> None:
        self._local = LocalWhisperAsrAdapter()
        self._groq = GroqAsrAdapter()
        self._whisper = WhisperAsrAdapter()
        self._deepgram = DeepgramAsrAdapter()

    async def transcribe(
        self,
        *,
        audio: bytes,
        filename: str,
        content_type: str,
        language: str = "ru",
    ) -> AsrResponse:
        if not audio:
            raise AsrError("Empty audio payload")

        provider = settings.asr_provider.lower().strip()
        errors: list[str] = []

        if provider == "local":
            order = ["local"]
        elif provider == "groq":
            order = ["groq"]
        elif provider == "openai":
            order = ["openai"]
        elif provider == "deepgram":
            order = ["deepgram"]
        else:
            # auto: local first (no VPN), then cloud if keys exist
            order = ["local", "groq", "openai", "deepgram"]

        for name in order:
            try:
                if name == "local":
                    return await self._local.transcribe(
                        audio=audio,
                        filename=filename,
                        content_type=content_type,
                        language=language,
                    )
                if name == "groq" and settings.groq_api_key:
                    return await self._groq.transcribe(
                        audio=audio, filename=filename, content_type=content_type
                    )
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

        if provider == "local" or not (
            settings.groq_api_key or settings.openai_api_key or settings.deepgram_api_key
        ):
            raise AsrError(
                "; ".join(errors)
                if errors
                else "Local Whisper unavailable — install faster-whisper / check CUDA"
            )

        raise AsrError("; ".join(errors) if errors else "No ASR provider available")
