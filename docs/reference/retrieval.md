# Поиск и ContextPack: текущие входы и ограничения

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Запрос

`POST /v1/retrieval/search` принимает `query`, optional `intent`, `token_budget`, `limit`, `time_perspective`, `as_of`.

Intent из прочитанного parser: `current_state`, `decision_recall`, `timeline`, `task_resume`, `procedure_lookup`, `user_preferences`, `semantic_recall`. При отсутствии intent используется semantic_recall. Неизвестное значение отклоняется.

Time perspective: `current`, `as_of`, `timeline`, `all_history`; `as_of` требует timestamp. Само наличие такого enum не доказывает полную поддержку historical-as-known по recorded time. Это отдельно выделено в F201.

## Ответ

Кандидат содержит memory/revision IDs, статус, номер ревизии, validity, revision timestamp, source generation, score/channel/rank, explanation и optional classification. Содержимое candidate в текущем DTO не передаётся. Ответ содержит withheld, policy version, temporal perspective, degraded channels и warnings.

При `token_budget` сервер дополнительно строит ContextPack, но HTTP DTO возвращает его сводку. Model-ready sections/rendered context — предлагаемая новая поверхность. Нельзя считать `section_count > 0` текстом, который клиент уже получил.

## Точный исторический материал

Hydrator запрашивает заданные memory/revision пары и не должен подменять их latest. Несогласованные пары фильтруются; missing reference не исправляется созданием предполагаемого source. Историческое чтение всё равно подчинено действующей policy и retention.

## Ограничения бюджета и содержания

Текущий helper считает ceil(UTF-8 bytes/4). Это эвристическая оценка, не доказательство hard-token bound для произвольного tokenizer. Новый API MW различает byte-only preview и нормативный ContextPack с квалифицированным counter. Нельзя называть byte-only путь полной реализацией hard-token требования.

Текущий builder использует explanation при пустом content. Это отмечено как gap: в новом контексте техническая строка не должна выдаваться за исходное знание. Автоматическое сокращение также не должно удалять смысловое отрицание и затем рекламироваться как достоверная сводка.

## Диагностика

Не сравнивать несопоставимые channel scores произвольной суммой. Указывать, какие каналы сработали и какие ограничены; degradation допускается только в разрешённом безопасном path. Нельзя обходить generation fence, просто исключив vector channel, без проверки текущего полного contract.

---
**Основание:** [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S12: crates/vestrace-application/src/retrieval/context_builder.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs), [R13: crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
