# PostgreSQL: канонические данные, проекции и миграции

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Что является источником схемы

Точные таблицы, constraints, guarded SQL functions, ownership и grants определяются ordered `migrations/` и PostgreSQL adapters выбранной сборки. Список имён здесь — карта ответственности, не альтернативная DDL. Документация не резервирует номер следующей миграции.

Canonical memory identities/revisions, исходные events, Run history и authoritative governance facts отличаются от search documents, vectors и caches. Таблица в БД не становится канонической только из-за места хранения. Производные представления пересоздаются из более авторитетного состояния и не переписывают его ради локальной согласованности.

## Существующая память

`memories` указывает на активную immutable revision. `memory_revisions` хранит содержимое, temporal metadata и classification. `memory_sources` связывает с источником. В текущем repository эти части и поисковая проекция движутся одной транзакцией, включая CAS; outbox/idempotency на service boundary пока отдельны. F003 усиливает общий commit, не отрицает существующую атомарность нижнего уровня.

## Изоляция

Scoped transaction устанавливает workspace/principal из аутентифицированного RequestContext. Application authorization, explicit workspace predicates, RLS и narrow runtime grants дополняют друг друга. Администратор БД не является корректным substitute для проверки запрета runtime identity.

Для новой таблицы проверяются composite ownership keys, foreign references в том же workspace, immutable columns и допустимые state transitions. Полная readonly projection не должна случайно принимать writes через generic adapter.

## Миграции

Изменения только forward. Нельзя исправить уже применённую миграцию так, чтобы checksum-история новой среды расходилась со старой. Provisioning/upgrade владельцев и functions проверяются и на чистой, и на обновляемой disposable БД.

Миграции MW 0196–0199 в прежнем плане — кандидаты, а 0195 назван в P04/14E. Перед реализацией сверить актуальный ряд и исправить план целиком при коллизии. Не выполнять proposed DDL автоматически из документационного пакета.

## Рост данных

Для каждой новой сущности определить границы: canonical history, rebuildable projection, временный staging или observation. Индекс, TTL и очистка не должны удалять evidence, которое ещё требуется retention/recovery. Объём и задержки сначала измеряются на синтетических и реальных разрешённых workload; произвольная таблица benchmarks не заменяет измерения.

Точная будущая модель source/import/receipt находится в [MW data contract](implementation/memory-workspace/02-data-and-transactions.md).

---
**Основание:** [S06: crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [S07: crates/vestrace-application/src/governed_mutation.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).

[Карта документации](README.md) · [Состояние и ограничения](status.md) · [Реестр источников](maintenance/sources.md)
