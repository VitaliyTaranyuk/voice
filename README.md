# Voice

AI Voice Input Assistant для Windows. Источник истины: [VitaliyTaranyuk/voice](https://github.com/VitaliyTaranyuk/voice).

**MVP:** Windows desktop (Tauri v2) · ASR: Whisper / Deepgram · LLM refine: **DeepSeek** · без Claude/Anthropic.

## Что умеет сейчас

1. Глобальный PTT: **Ctrl+Shift+Space**
2. Захват микрофона (WASAPI)
3. Распознавание речи через API (`/v1/ai/asr`)
4. Правка текста через DeepSeek (`/v1/ai/refine`) с guardrails
5. Вставка в активное приложение (clipboard + Ctrl+V)
6. Локальная история (SQLite)
7. Определение активного приложения (IDE / chat / email / …)

## Документация

| Файл | Содержание |
| --- | --- |
| [`.project/architecture.md`](.project/architecture.md) | Архитектура |
| [`.project/decisions.md`](.project/decisions.md) | ADR |
| [`.project/conventions.md`](.project/conventions.md) | Стек |

## Требования

- Node.js ≥ 20, pnpm 9
- Rust stable + **VS 2022 Build Tools** (C++)
- Python ≥ 3.12 + [uv](https://github.com/astral-sh/uv)
- WebView2
- API keys (для реального распознавания/правки):
  - `OPENAI_API_KEY` и/или `DEEPGRAM_API_KEY` (ASR)
  - `DEEPSEEK_API_KEY` (refine)

## Запуск (полный цикл)

```powershell
# 1) зависимости
pnpm install
pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build

# 2) API
cd services/api
copy .env.example .env
# заполните OPENAI_API_KEY (или DEEPGRAM_API_KEY) и DEEPSEEK_API_KEY
uv sync
uv run uvicorn app.main:app --reload --host 127.0.0.1 --port 8787

# 3) Desktop (другой терминал, из корня)
pnpm --filter @voice/desktop exec tauri dev
```

Опционально: `VOICE_API_BASE_URL=http://127.0.0.1:8787` (это default).

### Как пользоваться

1. Запустите API и desktop
2. Кликните в текстовое поле (Notepad, Cursor, Slack, …)
3. Удерживайте **Ctrl+Shift+Space**, говорите, отпустите
4. Текст появится в поле после ASR + DeepSeek + paste

## Структура

```
apps/desktop          Tauri + React (Windows-first)
services/api          FastAPI /v1 health, asr, refine
packages/contracts    Zod schemas
packages/domain-types Domain types
packages/sdk          HTTP client
```

## Статус

- **M0** Done — monorepo foundation
- **M1** Done — mic + PTT
- **M2** Done — ASR upload pipeline
- **M3** Done — DeepSeek refine + inject + history
