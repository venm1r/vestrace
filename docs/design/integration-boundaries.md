# Проект подключения внешнего агента

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая функциональная спецификация F203.

## Режимы

**Memory-client:** внешний runtime управляет своей сессией и вызывает разрешённые memory read/write tools. Vestrace отвечает только за свои данные и операции. Наличие MCP подключения не делает остальные tools runtime governed Vestrace.

**Governed action:** конкретное внешнее действие целиком проходит через Run/effect authority Vestrace. Требуется отдельный contract; два runtime не могут независимо retry одну операцию. Этот режим не входит автоматически в первый memory plugin.

## Первый совместимый клиент

Выбрать DSH либо другой фактический клиент по доступной версии и потребителю. Сначала read-only, затем explicit user writes; automatic capture только opt-in и с источником. Не использовать внешний memory plugin catalogue как свидетельство нашей совместимости.

Проверить transport, auth, schema, error taxonomy, reconnect, cancellation и privacy хранения клиентом. Config examples публикуются только после воспроизводимого interop, с безопасными placeholders, не реальными credentials.

## Context injection

Контекст источников — данные, не privileged instruction. Выбранный client pipeline должен сохранять этот уровень authority. Required-context и optional-memory режим задаётся приложением: при отсутствии обязательных сведений задача отказывается, при optional запросе продолжается с явной границей. Плагин не выбирает этот смысл по наличию сетевой ошибки.

## Запись

Любое capture получает client/source identity, content type и immutable event/provenance. Повтор завершённого turn не создаёт вторую запись. Модельный текст не маркируется пользовательским подтверждением. Private/project/team scopes не назначаются свободным аргументом LLM.

## Acceptance

Две сессии на одной разрешённой identity видят принятое исправление. Другая identity не видит эту память. Restart клиента не повторяет mutation. При недоступности сервиса поведение соответствует приложенной policy. Публичная документация объясняет, что уже переданные bytes могут сохраняться в чужих logs и не отзываются локальным delete.

---
**Основание:** [R17: crates/vestrace-mcp/src/server.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-mcp/src/server.rs), [R01: crates/vestrace-http/src/auth.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
