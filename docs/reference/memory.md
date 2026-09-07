# Memory API и ревизии: текущий контракт

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Руководство по срезу исходников; не свидетельство испытания.

## Доступные в прочитанном HTTP пути операции

| Метод и путь | Назначение | Существенная граница |
| --- | --- | --- |
| POST `/v1/events` | Записать исходное событие | Не делает любой payload подтверждённым знанием |
| POST `/v1/memories` | Создать память с основанием | kind/content/confidence/importance/source_event_id/evidence_role и optional classification |
| GET `/v1/memories/{id}` | Получить метаданные | id/kind/status/classification/created_at/updated_at; нет content/history |
| POST `/v1/memories/{id}/revisions` | Создать новую content revision | Ожидаемая revision передаётся числовым If-Match |
| DELETE `/v1/memories/{id}` | Hard purge через отдельную authority | Не кнопка «скрыть»; reason/approval, critical risk и MemoryPurge |

Точную структуру optional полей и ошибок сверять с `api/memory.rs`. Для автоматического клиента не выводить DTO из этой сокращённой таблицы.

## Содержимое и provenance

Внутренний `Memory` хранит идентичность и указатель на revision. `MemoryRevision` несёт содержимое и temporal metadata. Их чтение не должно перемешивать текущую identity с произвольным revision другого memory. Ссылка на источник — происхождение утверждения, не логическое доказательство его истинности.

Текущий `memory_sources` относится к памяти; новая exact revision↔source связь предусмотрена MW. Историю старых связей нельзя искусственно уточнять до номера ревизии, если необходимых фактов не было сохранено.

## Запись и concurrency

Существующий repository CAS проверяет state/content revisions. Revision update не является безусловным overwrite. Но outbox/idempotency остаются следующими service writes; усиление этой общей границы относится к F003/MW-02.

Повторная активация прежнего содержания оформляется новой revision. `valid_from/valid_until`, `created_at` и номер revision — разные измерения. Отсутствующее occurred time не подменяется mtime локального файла.

## Classification

Creation принимает vocabulary установки. Обычная revision сейчас наследует label либо допускает идентичное значение; изменение/clear отклоняется, поскольку отдельный переход не реализован в данном пути. Поэтому редактор label read-only до появления законного transition workflow. Не добавлять severity ordering к произвольным labels по алфавиту или имени.

## Следующая версия поверхности

Detail, history, browse, correction/restore, source import и portable export спроектированы в [MW API](../implementation/memory-workspace/03-api.md). Это extension с отдельной приёмкой, не уже работающий контракт выше.

---
**Основание:** [S03: crates/vestrace-application/src/memory/mod.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs), [S05: crates/vestrace-application/src/memory/services.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [S06: crates/vestrace-infrastructure/src/postgres/memory_repository.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [S10: crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
