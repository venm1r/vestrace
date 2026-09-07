# CLI: существующие группы команд

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

Источник синтаксиса — Clap в `crates/vestrace-cli/src/main.rs`. Команды ниже найдены в source; они не запускались здесь. Перед mutating operation читайте `--help` именно вашего бинарного файла и требования к окружению.

| Команда | Назначение и граница |
| --- | --- |
| `server` | HTTP процесс; требует DB/schema/vault/policy prerequisites |
| `worker` | Постоянный обработчик доступной работы |
| `worker --once` | Один цикл; 0 — работа, 3 — idle, 1 — ошибка; не full-Run result |
| `mcp` | MCP process mode |
| `migrate` | Миграции после предусмотренного role provisioning |
| `doctor` | Диагностика текущей реализации |
| `plan --finding-id UUID` | План для названных findings; не generic roadmap editor |
| `repair --plan-id UUID --current-state-ref REF` | Применение по конкретному плану и состоянию |
| `rebuild TARGET` | Target задаётся enum исходников и help, не придумывается документацией |
| `schema FORMAT` | Выдача поддерживаемого schema format |
| `conformance …` | Qualification artifacts/checks и evidence-bound release |

Глобальные `--config` и `--http-bind` не заменяют provisioning. Database URL не помещается в TOML или командную строку: используйте секретную environment выбранного процесса.

В группе conformance присутствуют list/check, bundle/manifest, verify/sign/verify-signature, publish-baseline/release и явно destructive qualification modes. Нельзя называть любую `conformance check` полной production acceptance. Поля target digests, profiles, artifact identity и trust sources читаются из команды и принятого qualification contract.

Не используйте придуманные `vestrace qualify` или `vestrace doctor --plan` из старых концептуальных примеров. Folder import/export команды MW являются проектом и будут включены только после соответствующей реализации.

---
**Основание:** [R03: crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
