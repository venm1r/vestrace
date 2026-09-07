# MCP: текущие инструменты и границы интеграции

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

MCP — адаптер к существующим application authorities, не независимый store. В прочитанном `server.rs` tool call сначала проходит authorization boundary. Неизвестный инструмент отклоняется.

`search_memories` создаёт обычный RetrievalRequest; `get_memory` возвращает id/kind/status, без содержимого. Поэтому наличие обоих инструментов не является завершённым пользовательским сценарием воспоминания. F101/F102 должны сначала обеспечить безопасное чтение данных; подключение внешнего клиента без этого не исправляет пробел.

CLI содержит режим `mcp`. Конкретный transport, initialization и credential binding нужно сверять с выбранным executable/source. Этот документ не выдаёт универсальный конфиг DSH/Cursor как испытанную интеграцию.

## Предлагаемый порядок расширения

Сначала read-only tool на одном qualified client; затем явная запись пользователем, затем отдельно opt-in capture. Recall выдает data с источниками, а не privileged instructions. Capture модели — наблюдение с provenance; автоматическое повышение до trusted knowledge запрещено.

Identity привязывает trusted transport/client setup, а не аргумент workspace, свободно выбранный LLM. Memory-интеграция не контролирует прочие tool calls чужого runtime. Удаление локальной памяти не удаляет уже переданный контекст из журнала этого runtime; политика такого раскрытия должна быть явной.

---
**Основание:** [R17: crates/vestrace-mcp/src/server.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-mcp/src/server.rs), [R03: crates/vestrace-cli/src/main.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-cli/src/main.rs), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
