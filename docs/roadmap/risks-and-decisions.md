# Риски, trade-offs и проектные решения

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая политика scope и риска.

| Риск | Ранний признак | Контроль |
| --- | --- | --- |
| Scope бесконечно растёт | Новый capability добавляется без пользовательского сценария | Один milestone, owner decision и explicit non-goals |
| Проект красив в specs, но не работает целиком | Fixtures создают недостижимые production states | Composition E2E с реальным binary/roles |
| Неполная атомарность | Memory committed, receipt/outbox отсутствует | Один UoW и fault/replay tests |
| Обход доступа новым UI/export | Content защищён в GET, но доступен из trace | Единый disclosure gate и negative matrix |
| Импорт затирает автора | mtime wins или полный overwrite | B/I/M conflict и preconditions |
| Источник перепутан с истиной | LLM extract получает Active/Trusted без принятия | Provenance и отдельная mutation authority |
| Консолидация скрывает ошибки | Ранняя сводка не инвалидируется | Source revision set, stale watermark, обратные зависимости |
| Ресурсная стоимость разработки | CI падает на disk, developer cycle растёт | Измерение profiles/targets без отключения классов тестов |
| Конкурент удобнее | Пользователь не доходит до полезного результата | Один SDK, эталонный сценарий, наблюдение внешней установки |
| История docs стала второй правдой | Три копии endpoints с разными полями | Canonical sources, generated views, status labels |

## Принятые в этой редакции именно редакционные решения

Документальный baseline — 07e2977; runtime ancestor — 6f610253. Старый MW source-manifest и frozen specs остаются без переписывания. Новая структура помещает roadmap отдельно от current API. Архив — overlay, не полная копия репозитория.

## Предлагаемые продуктовые решения

1. Memory-first остаётся центром; основной P1 — выбранные пользователем API/Console/import+portability.
2. MW располагается отдельным feature milestone до явного решения о release placement.
3. Первый importer bounded и не требует LLM; источник и редакторская revision не смешиваются.
4. Первый внешний client один; дополнительные SDK требуют задачи и поддержки.
5. P2 summaries являются derived state, а не самоизменением канонической правды.
6. P3 frozen-v1 scope не отменяется приоритетом. Hosted/team/Harness как продукт — P4.

Эти предложения не помечаются Accepted ADR автоматически. Для каждого необходимого изменения утверждённых laws создаётся точный amendment с последствиями, тестами и migration impact.

## Что сознательно не выбирается

Собственная модель, собственная vector database, новый IDE/coding agent, no-code конструктор, десятки adapters ради числа и отдельный generic importer runtime. Они могут быть отдельными будущими решениями, но не скрытыми задачами выбранного milestone.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R10: docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
