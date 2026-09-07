# Планы: приоритеты, зависимости и действующие обязательства

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Три разные карты

| Документ | Роль | Что не следует из него |
| --- | --- | --- |
| Исторический 36-PR transition | Переход от исходного v0.2 baseline, сохранённый для истории требований | Что его номера автоматически описывают нынешнюю очередь |
| Frozen P01–P12 | Действующий утверждённый full-v1 delivery program | Что все отмеченные в его тексте этапы уже приняты на текущем коде |
| MW-00–MW-07 | Предлагаемое расширение Memory/Context API, UI, import/sync/export | Что оно включено в v1 или меняет protected P04 scope |

Новая [дорожная карта](../roadmap/README.md) задаёт продуктовую приоритизацию **P0–P4**. Это не номера implementation packages **P01–P12**. Пакет P08 может быть product priority P3 и при этом оставаться обязательным для полного v1.0.

## Операционный порядок

Продолжение P04 принимается по его собственному контракту. P05 зависит от P02/P04. P06 следует за P05, P07 за P06, P08/P09 за P07, P10 за P09, P11 за P08/P10, P12 за P11. Новая таблица не меняет этот граф.

MW начинает с отдельного MW-00 и точного review пересечений. Read-only часть может разрабатываться отдельно до полного P04 closure, но context/import readiness не обходят действующие guards. Одновременная работа в общих writer/migration files требует явного порядка, а не optimistic merge.

## Утверждение нового решения

Владелец выбирает отдельный milestone или изменяет frozen release program явно. До этого дорожная карта и подробные P2/P4 specs являются proposals. При изменении условий повторяются delta review, scope и acceptance matrix. Сроки определяются скоростью принятых вертикальных задач, а не количеством оставшихся галочек.

Ни исторические планы, ни эта карта не дают права commit/push/deploy сами по себе.

---
**Основание:** [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R18: docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md), [R09: docs/specs/vestrace-architecture-contract-v0.2.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
