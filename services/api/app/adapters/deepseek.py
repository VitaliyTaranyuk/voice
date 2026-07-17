from __future__ import annotations

import httpx

from app.core.config import settings
from app.domain.guardrails import LlmRefineResult


class DeepSeekLlmAdapter:
    """LLM port adapter — DeepSeek Chat Completions API only (ADR-013)."""

    async def refine(
        self,
        *,
        system_prompt: str,
        raw_transcript: str,
        locale: str,
    ) -> LlmRefineResult:
        if not settings.deepseek_api_key:
            # M0/M3 stub: pass-through so desktop can develop without cloud keys.
            return LlmRefineResult(
                text=raw_transcript.strip(),
                confidence=None,
                applied_rules=["passthrough_no_api_key"],
                warnings=["DEEPSEEK_API_KEY not configured; returned raw transcript"],
            )

        headers = {
            "Authorization": f"Bearer {settings.deepseek_api_key}",
            "Content-Type": "application/json",
        }
        payload = {
            "model": settings.deepseek_model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {
                    "role": "user",
                    "content": f"Locale: {locale}\n\nTranscript:\n{raw_transcript}",
                },
            ],
            "temperature": 0.2,
        }

        async with httpx.AsyncClient(base_url=settings.deepseek_base_url, timeout=60.0) as client:
            response = await client.post("/v1/chat/completions", json=payload, headers=headers)
            if response.status_code >= 400:
                raise ValueError(f"DeepSeek error: HTTP {response.status_code}")
            data = response.json()

        try:
            text = data["choices"][0]["message"]["content"].strip()
        except (KeyError, IndexError, TypeError, AttributeError) as exc:
            raise ValueError("Unexpected DeepSeek response shape") from exc

        return LlmRefineResult(
            text=text,
            confidence=0.9,
            applied_rules=["deepseek_refine"],
            warnings=[],
        )
