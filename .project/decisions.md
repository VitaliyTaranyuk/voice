# Архитектурные решения (ADR)

Формат записи:

```md
## ADR-XXX: Краткий заголовок

- Дата: YYYY-MM-DD
- Статус: proposed | accepted | superseded
- Контекст: ...
- Решение: ...
- Последствия: ...
```

## ADR-001: Инициализация проектной документации

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Пустой репозиторий Voice без зафиксированных правил разработки.
- Решение: Хранить обязательные правила в `.project/` (`instructions.md`, `workflows.md`, `conventions.md`, `architecture.md`, `decisions.md`) и вспомогательные материалы в `docs/`.
- Последствия: ИИ-ассистенты и разработчики используют локальную документацию как источник с высшим приоритетом.

## ADR-002: Desktop stack — Tauri v2 + React + TypeScript

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Нужен лёгкий desktop-клиент с нативной интеграцией ОС и низким потреблением RAM.
- Решение: Tauri v2 (Rust core) + React + TypeScript + Vite; UI — TailwindCSS, shadcn/ui, Motion, Lucide; state — Zustand + TanStack Query + RHF + Zod.
- Последствия: Меньший бинарник и RAM vs Electron; OS-логика в Rust; UI только презентация через IPC.

## ADR-003: Backend — FastAPI за language-agnostic contracts

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Нужны auth, billing, AI orchestration, sync; клиент не должен зависеть от языка сервера.
- Решение: Python FastAPI + Pydantic + SQLAlchemy + Alembic + AsyncIO; PostgreSQL + Redis; публичный контракт только `/v1` в `packages/contracts`.
- Последствия: Desktop говорит с API через SDK; backend можно заменить другим языком без смены клиента. Desktop-specific логика на сервер не попадает.

## ADR-004: Monorepo — pnpm + Turborepo

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Нужны desktop, backend, AI gateway, shared contracts в одном репозитории.
- Решение: Структура `apps/`, `services/`, `packages/`, `crates/`, `docs/`, `.project/`, `infrastructure/`; pnpm workspaces + Turborepo; Python services через `uv`.
- Последствия: Единые contracts и CI; строгий запрет циклов desktop ↔ api source.

## ADR-005: Provider Architecture для ASR / LLM / Signal

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Провайдеры речи и LLM будут меняться; нельзя хардкодить одного вендора в pipeline.
- Решение: Ports (`AsrProvider`, `LlmProvider`, `SignalEnhancer`) + Registry + Router (Strategy, DI). Новый провайдер = adapter + registration.
- Последствия: Pipeline стабилен; DeepSig и local ASR подключаются без переписывания ядра.

## ADR-006: Local-first данные — SQLite

- Дата: 2026-07-17
- Статус: accepted
- Контекст: История и словари должны работать offline; cloud sync — опционально.
- Решение: SQLite + Drizzle на desktop; encrypted sensitive fields; cloud sync позже как encrypted blobs.
- Последствия: Privacy by default; sync не блокер MVP.

## ADR-007: Три режима privacy

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Разные пользователи требуют local-only vs cloud quality.
- Решение: Режимы `local` | `hybrid` | `cloud` как системный switch на уровне pipeline и router.
- Последствия: Полностью локальная работа архитектурно возможна (реализация local ASR/LLM — V1+).

## ADR-008: Text injection — UIA-first with focused clipboard fallback (Windows)

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Нужна вставка текста в то поле, где был курсор на старте диктовки, без per-app плагинов. Clipboard+Ctrl+V в текущий foreground ломается при смене окна во время записи.
- Решение: На `start()` захватывать `InputTarget` (HWND + UIA RuntimeId + can_insert). Порт inject: (1) restore focus + UIA ValuePattern для пустых полей, (2) focus restore + clipboard Ctrl+V (основной путь для caret mid-string), (3) SendInput unicode. macOS — отдельный adapter позже (ADR-012).
- Последствия: Текст уходит в сохранённый target; при отсутствии текстового поля — предупреждение и сохранение в history; риск конфликтов с буфером на fallback — документировать UX.

## ADR-009: AI refine guardrails

- Дата: 2026-07-17
- Статус: accepted
- Контекст: LLM склонен «улучшать» текст ценой смысла и кода.
- Решение: Immutable system policy: не менять смысл/код/URL/email/markdown/CLI/термины; только грамматика, пунктуация, формат и допустимые user instructions. Fallback = сырой ASR.
- Последствия: Предсказуемость для разработчиков; валидаторы на выходе refine.

## ADR-010: ai-gateway отдельно от identity/billing API

- Дата: 2026-07-17
- Статус: accepted
- Контекст: AI-трафик (стримы, retries, cost) отличается от CRUD auth/billing.
- Решение: `services/api` (identity, billing, sync) и `services/ai-gateway` (ASR/LLM) как раздельные deployables с общими contracts.
- Последствия: Независимое масштабирование и SLA для AI path.

## ADR-011: Conventions — React / Zustand / FastAPI (не Vue / Express)

- Дата: 2026-07-17
- Статус: accepted
- Контекст: В `conventions.md` были заготовки под Vue/Pinia/Express+Mongoose.
- Решение: Зафиксировать стек из ADR-002/003; обновить conventions; Vue/Express placeholders считать устаревшими.
- Последствия: Единый стиль для ассистентов и разработчиков.

## ADR-012: Windows-first MVP

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Нужен быстрый путь к рабочему продукту; dual-platform увеличивает риск на injection/hotkeys.
- Решение: MVP только Windows. macOS — отдельная фаза после стабильного Windows pipeline. Порты OS-интеграции проектируются сразу, реализация macOS — позже.
- Последствия: Фокус spike M0 на Win32 hotkeys + injection; меньше поверхность тестирования MVP.

## ADR-013: LLM provider — только DeepSeek API

- Дата: 2026-07-17
- Статус: accepted
- Контекст: Не использовать Anthropic/Claude. Разработка ведётся в Cursor; для refine нужен один облачный LLM.
- Решение: Единственный LLM-адаптер MVP/V1 — DeepSeek API через `LlmProvider`. Claude/Anthropic не подключать. Добавление другого LLM — только новым ADR.
- Последствия: Упрощение gateway и биллинга; риск single-vendor — mitigations в architecture (retry, degrade to raw ASR).

## ADR-014: Сайдкар гасят двое — приложение и установщик

- Дата: 2026-08-17
- Статус: accepted
- Контекст: Обновление 0.1.2 → 0.1.3 легло наполовину: `voice-desktop.exe` заменён, 1001 файл сайдкара из 1003 остался старым, реестр остался на 0.1.2. Живой `voice-api.exe` держит открытыми `resources\voice-api\_internal\*.pyd`, а NSIS не перезаписывает файл, который используется. Windows-ветка `tauri-plugin-updater` заканчивается на `std::process::exit(0)` сразу после запуска установщика, поэтому `RunEvent::Exit` не приходит и `shutdown_local_api()` не вызывается; хук `on_before_exit` плагин занял сам и наружу не отдаёт.
- Решение: Ответственность делится и дублируется намеренно. (1) Приложение не оставляет своего ребёнка сиротой: `stop_local_api` вызывается фронтендом между `download()` и `install()`, а `shutdown_local_api()` убивает и `voice-api.exe`, чей полный путь совпадает с сайдкаром, который мы бы сами запустили — по имени не убивает. (2) Установщик гасит `voice-api.exe` сам, в `NSIS_HOOK_PREINSTALL`/`PREUNINSTALL` (`apps/desktop/src-tauri/nsis-hooks.nsh`).
- Последствия: Пункт (2) — единственное, что лечит уже установленные версии: обновление скачивает и запускает новый установщик, и никакая правка внутри нового приложения до них не доедет. Он же закрывает сценарии, которых приложение не видит: сирота от прошлой сессии (родитель мёртв, порт 8787 занят) и ручной запуск `Setup.exe` при работающем Voice. Пункт (1) не избыточен: без него приложение штатно оставляет процесс, который блокирует и установщик, и локальную сборку. Удаление любого из двух возвращает дефект — частично.
