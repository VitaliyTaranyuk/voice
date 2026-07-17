from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.api.router import api_router
from app.core.config import settings


@asynccontextmanager
async def lifespan(_app: FastAPI):
    # Warm local Whisper so the first dictation doesn't pay model-load latency.
    if settings.asr_provider.lower().strip() in {"local", "auto"}:
        try:
            from app.adapters.asr import LocalWhisperAsrAdapter

            LocalWhisperAsrAdapter()._ensure_model()
            print(
                f"Voice API: local Whisper ready "
                f"({settings.local_whisper_model} / {LocalWhisperAsrAdapter._runtime})"
            )
        except Exception as exc:
            print(f"Voice API: local Whisper warmup failed: {exc}")
    yield


app = FastAPI(
    title="Voice API",
    version=settings.app_version,
    docs_url="/docs",
    redoc_url="/redoc",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.cors_origins,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(api_router, prefix="/v1")
