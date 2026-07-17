# Voice

AI Voice Input Assistant для Windows. Источник истины: [VitaliyTaranyuk/voice](https://github.com/VitaliyTaranyuk/voice).

**MVP:** Windows desktop (Tauri v2) · LLM refine: **DeepSeek API** · без Claude/Anthropic.

## Документация

| Файл | Содержание |
| --- | --- |
| [`.project/instructions.md`](.project/instructions.md) | Правила разработки и Git |
| [`.project/architecture.md`](.project/architecture.md) | Архитектура продукта |
| [`.project/decisions.md`](.project/decisions.md) | ADR |
| [`.project/conventions.md`](.project/conventions.md) | Стек и соглашения |
| [`docs/`](docs/) | Доп. материалы |

## Структура monorepo

```
apps/desktop          Tauri v2 + React (Windows-first)
services/api          FastAPI (/v1 health + refine → DeepSeek)
packages/contracts    Zod API/domain schemas
packages/domain-types Shared TS domain types
packages/sdk          HTTP client для /v1
```

## Требования

- Node.js ≥ 20, pnpm 9
- Rust stable (для desktop)
- Python ≥ 3.12 + [uv](https://github.com/astral-sh/uv)
- Windows: WebView2 (обычно уже установлен)
- Windows: [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) с workload **Desktop development with C++** (`link.exe`)

## Быстрый старт

```bash
pnpm install
pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build

# API (passthrough refine без ключа; с ключом — DeepSeek)
cd services/api
cp .env.example .env
uv sync
uv run uvicorn app.main:app --reload --port 8787

# Desktop (из корня)
pnpm --filter @voice/desktop exec tauri dev
```

Опционально в `services/api/.env`:

```
DEEPSEEK_API_KEY=sk-...
```

## Статус M0

Готово: monorepo, contracts/sdk, FastAPI health+refine, Tauri shell, state-machine stub диктовки.

Далее (M1–M3): audio capture, hotkeys, ASR stream, injection, DeepSeek E2E.
