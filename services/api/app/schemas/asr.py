from typing import Literal

from pydantic import BaseModel, Field


class AsrResponse(BaseModel):
    text: str
    language: str | None = None
    provider: Literal["openai_whisper", "deepgram", "passthrough"]
    confidence: float | None = None
    warnings: list[str] = Field(default_factory=list)
