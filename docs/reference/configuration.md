# Конфигурация и поддерживаемая среда

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Источник значений

Читать `Cargo.toml`, `rust-toolchain.toml`, `AppConfig`, `docker-compose.yml`, entrypoints server/worker и frontend config выбранного checkout. Этот документ не хранит второй полный список defaults.

Конфигурация проекта разделяет non-secret параметры и секретные environment/mounted stores. В описанном runtime порядке применяются defaults, optional TOML, `VESTRACE_` environment с `__` для вложенности и typed CLI overrides. Неизвестные TOML-поля отклоняются; секретный database URL не должен мигрировать в публичный конфиг.

## Основные группы

| Группа | Значение для эксплуатации |
| --- | --- |
| Database | URL из secret environment; pool и scope; ограниченная runtime identity |
| HTTP | Bind и proxy boundary; loopback dev не равно public deployment |
| Workspaces | Какую область worker обслуживает; пустая область не означает успешную полезную работу |
| Provider execution | Material vault root и отдельный read-only bootstrap root; identity ключа, не plaintext |
| Policy | Capability engine и явные disclosure/destination constraints |
| Observability | Безопасные structured logs без credentials/content |
| Qualification/recovery | Включение процедур с точными prerequisites, не возможность административно поставить trusted |

Root volumes и permitted external destinations должны совпадать у участвующих processes. Shared governed provider graph уменьшает расхождения, но настройка deployment всё равно проверяется отдельно.

## Матрица подтверждения среды

Baseline называет Rust 1.85, Node 22 и PostgreSQL 17/pgvector, а также Compose services. Ни Windows, ни Linux, ни конкретная файловая система не объявлены этой документацией квалифицированными новым запуском. Для release запишите OS, kernel/filesystem, architecture, image digests, toolchain, role layout и model/provider settings.

Изменение filesystem semantics для key vault, Node/TypeScript major или PostgreSQL/extensions требует отдельной проверки. Обновление lockfiles — кодовая/зависимостная задача, не часть переработки документов.

---
**Основание:** [R03: crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs), [R04: docker-compose.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docker-compose.yml), [R05: docs/getting-started.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R12: .github/workflows/ci.yml](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/.github/workflows/ci.yml).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
