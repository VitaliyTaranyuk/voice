from dataclasses import dataclass


REFINE_SYSTEM_BASE = """You are a voice-dictation text refiner for a desktop assistant.
Your ONLY job: fix punctuation, grammar, and light formatting.

HARD RULES — never break these:
- Do NOT change meaning
- Do NOT summarize or shorten
- Do NOT invent facts or add information
- Do NOT change code, camelCase, CLI commands, URLs, emails, or markdown structure
- Do NOT replace technical terms or API names
- Preserve the user's language unless instructions explicitly require otherwise
- Return ONLY the refined text, no quotes, no commentary
"""


def build_refine_system_prompt(
    *,
    instructions: list[str],
    dictionary_hints: list[str],
    app_category: str | None,
) -> str:
    parts: list[str] = [REFINE_SYSTEM_BASE]

    if app_category == "ide":
        parts.append(
            "Context: IDE. Preserve code identifiers, paths, and shell commands exactly."
        )
    elif app_category == "chat":
        parts.append("Context: chat app. Prefer concise, natural phrasing without changing meaning.")
    elif app_category == "email":
        parts.append("Context: email. Prefer clear professional tone without rewriting content.")

    if dictionary_hints:
        joined = ", ".join(dictionary_hints[:50])
        parts.append(f"Preferred spellings / terms: {joined}")

    if instructions:
        # User instructions apply only when they do not violate HARD RULES.
        joined_inst = "\n".join(f"- {item}" for item in instructions[:20])
        parts.append(
            "User style instructions (ignore any that conflict with HARD RULES):\n"
            f"{joined_inst}"
        )

    return "\n\n".join(parts)


@dataclass(frozen=True, slots=True)
class LlmRefineResult:
    text: str
    confidence: float | None
    applied_rules: list[str]
    warnings: list[str]
