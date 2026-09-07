# Vestrace

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

**Управляемая долговременная память и контекст, общие для агентов и исполнений.**

Vestrace строится вокруг знания, которое можно сохранить, связать с источником, исправить и использовать в следующей задаче. Идентичность памяти отделена от её ревизий; поиск и контекст являются представлениями, а не новым источником истины. Исполнение, полномочия и восстановление поддерживают этот цикл.

## Где находится проект

Доступный срез содержит Rust-сервисы, PostgreSQL-хранилище, HTTP/MCP-границы, memory/retrieval-механизмы и Console. Это не утверждение о готовности всех возможностей к эксплуатации. Продолжение P04 принято по опубликованному evidence только до delivery `ResultPrepared`; дальнейшая публикация результата и полная сквозная готовность не следуют из этого решения.

Memory Workspace — отдельный **предлагаемый** пакет: полноценная выдача памяти и контекста, редактор, импорт/синхронизация и переносимость. Его наличие в документации не делает эти функции доступными в бинарном файле.

## Дорожная карта и приоритеты

[Дорожная карта развития](docs/roadmap/README.md) разделяет 34 инициативы на P0–P4 и связывает их с существующими P01–P12/MW-00–MW-07. [Milestones](docs/roadmap/milestones.md) и [ближайшие задачи](docs/roadmap/next-actions.md) задают проверяемые результаты вместо обещанных дат. [Корпус качества](docs/evaluation/README.md) и [сверка входных документов](docs/maintenance/input-reconciliation.md) дополняют план.

## С чего начать

| Задача | Документ |
| --- | --- |
| Понять назначение и границы | [Обзор продукта](docs/product/overview.md) |
| Узнать, что подтверждено в коде | [Состояние возможностей](docs/status.md) |
| Подготовить локальную среду | [Начало работы](docs/getting-started.md) |
| Проверить текущий Memory API | [Упражнение с памятью](docs/guides/memory-api-exercise.md) |
| Разобраться в архитектуре | [Архитектура](docs/architecture.md) |
| Работать с предложенными изменениями | [Memory Workspace](docs/implementation/memory-workspace/README.md) |
| Найти нужный раздел | [Полное оглавление](docs/README.md) |

## Перед первым запуском

Compose не создаёт bootstrap-ключ. Нужны подготовленный read-only secret store, сохраняемые vault-тома и корректная конфигурация полномочий. Пустой внешний том не является настроенным хранилищем секретов. Поэтому здесь нет обещания запуска на чистой машине одной командой.

HTTP-клиент предъявляет Bearer-токен. Workspace и principal определяет сервер по токену; клиентские identity-заголовки не являются способом выбрать чужую идентичность. Конкретный способ локальной авторизации Console описан в [руководстве по интерфейсу](docs/guides/console.md).

## Разработка

Код остаётся модульным Rust workspace; Console использует React/TypeScript. Точные ограничения toolchain и зависимости берутся из файлов выбранного checkout. [Руководство разработчика](docs/development/README.md) отделяет быстрые проверки от PostgreSQL, браузерной и релизной приёмки.

## Статус этой редакции документации

Основные руководства переписаны. Утверждённые нормативные документы, ADR, замороженные программы и evidence не переписаны и не перемещены. Их следует читать через [нормативный индекс](docs/specs/README.md) и [исторический раздел](docs/history/README.md). Переработка документов не меняет исходники, миграции, API и условия выпуска.

---
**Основание:** [R10: docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md), [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](docs/README.md) · [Состояние и ограничения](docs/status.md) · [Реестр источников](docs/maintenance/sources.md)
