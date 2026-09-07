# Проект фоновой консолидации памяти

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая функциональная спецификация F206.

## Первый ограниченный объект

Обновляемая справка «решения, ограничения и незавершённые вопросы проекта». Не универсальный агент, который сам решает, во что системе верить. Пользователь задаёт цель справки и разрешённую область источников.

## Входы и выходы

Вход — точный набор разрешённых revisions и версия метода/модели/политики. Выход — derived projection со ссылками на эти inputs, watermark обработки и статусом актуальности. Requested, building, ready/stale/failed — состояния этой проекции или проекции существующего Run, а не отдельный generic task runtime.

Canonical facts не меняются от выпуска справки. Публикация новой canonical memory из её вывода — отдельное предложение/мутация с полномочиями и provenance. Частота использования или высокая модельная confidence не повышают доверие автоматически.

## Исполнение

Работа ставится в существующий execution/outbox путь. Before-disclosure проверяются источники и destination. При retry используется immutable input identity. Изменение источника во время работы либо приводит к корректному snapshot-result с последующей stale меткой, либо к отмене по принятому contract; молчаливое смешение поколений запрещено.

## Права и retention

Projection не получает больше возможностей раскрытия, чем исходники. Loss of access invalidates current serving decisions. Пока projection нужна для законной истории, её хранение согласуется с исходными material/erasure blockers. Audit не становится бессрочной копией summary plaintext.

## Acceptance

Справка помогает решить контрольную задачу с меньшим контекстом; исходные решения и отрицания не теряются. Поздняя коррекция инвалидирует актуальность. Повтор после process failure не создаёт два независимых принятых результата. Без новой информации recompute не увеличивает trusted knowledge.

## Зависимости и исключения

Начинать после F201/F202/F204/F008 и явного scope. LLM call optional только там, где заявлен; стоимость и latency измеряются. Не строить скрытые постоянные «sleep agents», новый policy engine или auto-promotion beliefs.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S08: crates/vestrace-application/src/outbox.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [S17: crates/vestrace-application/src/material/commands.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
