# Проект развития временного знания и конфликтов

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая функциональная спецификация F201/F202.

## Область

Это детализация F201/F202 для review, не новая принятая schema. Сначала сверяются existing TimePerspective, Claim/Conflict и authoritative temporal specs. Current, valid-as-of и recorded-as-known не объединяются автоматически в существующий `as_of` API.

## Предлагаемый контракт

Запрос задаёт, какой вопрос о времени решается, временную границу и разрешённую область. Ответ связывается с exact source/memory revisions, их validity и записанным временем. Если для requested-as-known нет надёжной истории, возвращается явная неподдерживаемая/неполная семантика, а не реконструкция по latest timestamps.

Смысловой конфликт представляет совместимые по типу, но несовместимые по содержанию утверждения и их источники. Он отличен от технического sync conflict B/I/M. Общий UI может показывать оба, но resolution не использует чужую authority.

## Разрешение

Допустимые варианты зависят от предметной политики: признать источник исправлением, сохранить конкурирующие утверждения, ограничить validity либо зафиксировать авторизованное human decision. Любое resolution сохраняет основание, actor и ожидаемое состояние. Автоматический latest wins разрешён только как явно принятая предметная policy, не универсальная эвристика.

Если один источник закрыт, доступному пользователю нельзя показать его содержимое или детали conflict, из которых оно выводится. Для безопасного ответа допустим ограниченный результат с понятной границей, без утечки hidden counts/names. Reclassification не является обычным content edit.

## Инвалидация

Изменение source/revision/policy вызывает пересмотр зависимых context/summary projections. Оно не переписывает сохранённые факты прошлых запусков. Историческое чтение проходит текущие права и retention; уничтоженный контент не восстанавливается догадкой из приблизительной сводки.

## Порядок реализации после решения владельца

1. Зафиксировать точную семантику запросов и минимальный corpus.
2. Сопоставить существующие temporal columns/indexes и missing historical facts.
3. Добавить только необходимый query/mutation contract с backward compatibility.
4. Проверить exact revision reads, late-arrival, конкуренцию resolution и revoke.
5. Подключить UI explanation и оценить полезность на том же corpus.

Не начинать с автоматического semantic merger: сначала пользователь должен видеть проблему и основания.

---
**Основание:** [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [R13: crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
