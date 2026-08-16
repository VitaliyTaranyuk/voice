# Voice

AI Voice Input Assistant для Windows. Источник истины: [VitaliyTaranyuk/voice](https://github.com/VitaliyTaranyuk/voice).

**MVP:** Windows desktop (Tauri v2) · ASR: **локальный Whisper (CUDA)** · LLM refine: **DeepSeek** · без Claude/Anthropic.

## Что умеет сейчас

1. Глобальный PTT: **Left Ctrl+Space** (Toggle: **+Shift**)
2. Захват микрофона (WASAPI)
3. Распознавание речи через API (`/v1/ai/asr`)
4. Правка текста через DeepSeek (`/v1/ai/refine`) с guardrails
5. Вставка в поле, зафиксированное на старте записи (UIA / focus+paste)
6. Локальная история (SQLite)
7. Определение активного приложения (IDE / chat / email / …)
8. Компактный overlay статуса во время записи

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
- API keys / ASR:
  - ASR по умолчанию **локальный** (`ASR_PROVIDER=local`, модель `small` на GPU, latency-first) — VPN не нужен
  - `DEEPSEEK_API_KEY` (refine по умолчанию; отключить: `VOICE_SKIP_REFINE=1`)
  - Опционально cloud ASR: `GROQ_API_KEY` / `OPENAI_API_KEY` / `DEEPGRAM_API_KEY` (`ASR_PROVIDER=auto`)

## Запуск (полный цикл)

Быстрый старт: ярлык **Voice** на рабочем столе или `scripts/start-voice.cmd`. Обновить ярлык после переноса папки: `powershell -File scripts/install-desktop-shortcut.ps1`.

```powershell
# 1) зависимости
pnpm install
pnpm --filter @voice/contracts --filter @voice/domain-types --filter @voice/sdk build

# 2) API
cd services/api
copy .env.example .env
# заполните DEEPSEEK_API_KEY (ASR локальный по умолчанию)
uv sync
uv run uvicorn app.main:app --reload --host 127.0.0.1 --port 8787

# 3) Desktop (другой терминал, из корня)
pnpm --filter @voice/desktop exec tauri dev
```

Опционально: `VOICE_API_BASE_URL=http://127.0.0.1:8787` (это default).

## Секреты

Ключи живут в двух местах, и оба вне репозитория и вне установщика:

| Где | Когда |
|---|---|
| `services/api/.env` | локальная разработка; файл в `.gitignore`, в репозиторий идут только имена переменных (`services/api/.env.example`) |
| Credential Manager Windows | у пользователя приложения; вводится в окне **Settings** |

Приложение читает ключ из Credential Manager и передаёт его процессу API переменной
окружения при запуске. Рядом с `voice-api.exe` он не пишется: в `runtime.env` остаются
только не-секретные настройки, а переменные окружения в pydantic-settings приоритетнее
`env_file`.

Смена ключа перезапускает локальный API: `settings = Settings()` читает окружение один
раз при импорте, поэтому на живом процессе смена не отразилась бы.

Реальное значение ключа не должно появляться нигде в репозитории, включая README
и примеры команд: 17.07.2026 `DEEPSEEK_API_KEY` уехал в публичный README именно
так и провисел месяц.

Один раз в каждом клоне включите pre-commit-проверку:

```powershell
git config core.hooksPath .githooks
```

Хук блокирует коммит, если в staged-изменениях появился ключ или файл `.env`.
Работает без дополнительных установок; если в системе есть `gitleaks`, использует
его. На стороне GitHub то же самое ловит push protection — он включён и сработает,
даже если хук в клоне не настроен.

ASR по умолчанию: локальный `faster-whisper` `small` на CUDA (`ASR_PROVIDER=local` в `services/api/.env`) — быстрее `large-v3`. Качество: `medium` / `large-v3`. DeepSeek refine **включён** на hot path (пунктуация, fillers); отключить: `VOICE_SKIP_REFINE=1`. Тишина / только «аммм» не вставляются.

### Как пользоваться

1. Запустите API и desktop
2. Кликните в текстовое поле (Notepad, Cursor, Slack, …)
3. Удерживайте **Left Ctrl+Space**, говорите, отпустите (или **Left Ctrl+Space+Shift** для Toggle)
4. Текст появится в том же поле после ASR + refine + inject (тишина не вставляется)

## Структура

```
apps/desktop          Tauri + React (Windows-first)
services/api          FastAPI /v1 health, asr, refine
packages/contracts    Zod schemas
packages/domain-types Domain types
packages/sdk          HTTP client
```

## Раздача друзьям (NSIS Setup)

Один установщик: скачал → Установить → выбрать папку → готово.

```powershell
pwsh scripts/build-release.ps1
```

Артефакт: `dist/voice-release/Voice_*-setup.exe` — отправьте друзьям.  
Внутри установщика: `Voice.exe` + `voice-api/` + `runtime.env` **без секретов**.  
История только на их ПК. Первый запуск может скачать Whisper в `%LOCALAPPDATA%\Voice`.

Только sidecar: `pwsh scripts/build-api-sidecar.ps1`.

**Ключи в установщик не попадают.** Диктовка работает сразу: распознавание локальное
и ключа не требует. Чтобы включить причёсывание текста через DeepSeek, получатель
открывает **Settings** и вводит свой ключ — тот ложится в Credential Manager Windows
на его машине. Ключ, зашитый в сборку, читался бы открытым текстом каждым, кто получил
файл, и отзыв не спасал бы: следующая сборка уносила бы новый.

## Статус

- **M0** Done — monorepo foundation
- **M1** Done — mic + PTT
- **M2** Done — ASR upload pipeline
- **M3** Done — DeepSeek refine + inject + history
