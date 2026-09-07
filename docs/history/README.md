# История, baseline и сохранённые доказательства

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Исторический навигатор; оригиналы не переписаны.

## Что сохранено

Нормативные specs, Accepted ADR, frozen plans, historical gap-deltas и development evidence остаются по прежним путям репозитория. Эта редакция не перемещает их и не меняет hashes. Такой подход сохраняет совместимость scope/protocol manifests и исторических ссылок.

Старые вводные руководства могут содержать смешанные текущие/целевые описания. Их прежние bytes доступны по baseline `07e2977a20b05c5b16953a206a6d68bdbff3a052` через Git history; новые вступительные документы заменяют навигацию, а не прошлое наблюдение.

## Точки отсчёта

| Срез | Значение |
| --- | --- |
| `729d456f…` | Старый inspected implementation snapshot, на который ссылается часть v0.2 документов |
| `58e7dac3…` | Исходный handbook и предыдущее обсуждение P04 |
| `6f610253…` | Кодовый baseline MW с продолжением embedding lifecycle до принятого 14D |
| `07e2977a` | Документационная интеграция MW; сравнение не показало изменения runtime-кода |

Не следует переписывать source-manifest MW с новым SHA ради внешней видимости актуальности: он описывает выполненный тогда review. Для нового понимания добавляется source delta.

## Ссылки на оригиналы

- [Исторический implementation snapshot](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/current-implementation.md).
- [v0.2 documentation status](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/documentation-status-v0.2.md).
- [Frozen P01–P12](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md).
- [P04 evidence](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md).
- [Планы перехода 36 PR](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/plans/v0.2-to-v1.0-pr-specification-index.md).

Опубликованное approval 14D относится к delivery ResultPrepared, не ко всему embedding system. Ранее записанный PASS не переносится на более новую комбинацию source/config/environment автоматически.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
