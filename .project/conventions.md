# Соглашения по коду

Стек зафиксирован (см. ADR-002, ADR-003, ADR-011…013 в `decisions.md`).

## Общие

- Изменения минимальны и ограничены задачей
- Стиль кода следует уже существующему в репозитории
- Без лишних комментариев и мёртвого кода
- Без секретов в репозитории (`.env`, ключи, credentials)
- Архитектурные решения документировать в `.project/architecture.md` и ADR одновременно с внедрением

## TypeScript

- Строгая типизация, без `any`
- Для неизвестных данных — `unknown`
- Интерфейсы/типы описывать сразу или выносить в `packages/domain-types` / рядом с модулем
- Shared API-схемы — в `packages/contracts` (Zod на клиенте, зеркало Pydantic на сервере)

## Desktop (React + Tauri)

- React + Vite + TypeScript
- UI: TailwindCSS, shadcn/ui, Motion, Lucide
- State: Zustand (setup-style stores), TanStack Query для серверного состояния
- Формы: React Hook Form + Zod
- Логику UI выносить в hooks/composables-эквиваленты; OS/audio/pipeline — в Rust через IPC
- Не блокировать UI-поток тяжёлой работой

## Rust (Tauri core)

- Audio, hotkeys, injection, pipeline orchestration — в Rust
- Windows-first реализации за портами (`TextInjector`, `HotkeyManager`, …)
- Горячий путь аудио без лишних аллокаций

## Backend (FastAPI)

- FastAPI + Pydantic + SQLAlchemy + Alembic + AsyncIO
- Ошибки через явный error model contracts; входные данные валидировать Pydantic
- Слои: domain → application → adapters (Clean / Hexagonal)
- LLM: только DeepSeek API (ADR-013), через порт `LlmProvider`
- Документы/ORM → POJO-эквиваленты (схемы ответа), не сырые ORM-объекты наружу

## HTTP / SDK

- Единый клиент в `packages/sdk` (axios или fetch-обёртка) с интерцепторами
- Desktop ходит в cloud только через SDK + `/v1`

## Local Data

- SQLite + Drizzle на desktop
- Секреты — Windows Credential Manager (macOS Keychain позже)
- Sensitive fields — encryption at rest

## Тесты

- Только по явному запросу
- Unit: Vitest для TS; для React — `@testing-library/react` при необходимости
- Логика — чистые функции; компоненты — события и пропсы, без проверки вёрстки
- Моки через `vi.mock()`
