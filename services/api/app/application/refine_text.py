from app.adapters.deepseek import DeepSeekLlmAdapter
from app.domain.guardrails import build_refine_system_prompt
from app.schemas.refine import RefineRequest, RefineResponse


class RefineTextUseCase:
    """Application use-case: refine transcript via DeepSeek with preservation guardrails."""

    def __init__(self, llm: DeepSeekLlmAdapter | None = None) -> None:
        self._llm = llm or DeepSeekLlmAdapter()

    async def execute(self, request: RefineRequest) -> RefineResponse:
        system_prompt = build_refine_system_prompt(
            instructions=request.instructions,
            dictionary_hints=request.dictionaryHints,
            app_category=request.appContext.appCategory if request.appContext else None,
        )
        result = await self._llm.refine(
            system_prompt=system_prompt,
            raw_transcript=request.rawTranscript,
            locale=request.locale,
        )
        return RefineResponse(
            text=result.text,
            confidence=result.confidence,
            applied_rules=result.applied_rules,
            warnings=result.warnings,
            provider="deepseek",
        )
