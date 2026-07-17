from typing import Literal

from pydantic import BaseModel, Field


class AppContext(BaseModel):
    appId: str
    appCategory: Literal["ide", "chat", "email", "browser", "docs", "other"]
    windowTitle: str | None = None
    processName: str | None = None


class RefineRequest(BaseModel):
    rawTranscript: str = Field(min_length=1)
    locale: str = "ru-RU"
    privacyMode: Literal["local", "hybrid", "cloud"]
    appContext: AppContext | None = None
    instructions: list[str] = Field(default_factory=list)
    dictionaryHints: list[str] = Field(default_factory=list)


class RefineResponse(BaseModel):
    text: str
    confidence: float | None = None
    applied_rules: list[str] = Field(default_factory=list)
    warnings: list[str] = Field(default_factory=list)
    provider: Literal["deepseek"] = "deepseek"
