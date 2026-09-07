# Дорожная карта Vestrace

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемая дорожная карта, не изменение frozen v1.0.

## Ключевое решение

**Развивать Vestrace как управляемую долговременную память проекта: она доступна через простой контракт, исправляется пользователем и безопасно пополняется из источников.** Execution, security, recovery и протоколы поддерживают этот цикл, а не вытесняют память из центра.

Эта дорожная карта — предложение по развитию на основе `07e2977`, приложенного handbook, интегрированного MW-пакета и обсуждённых рекомендаций. Она не меняет Accepted ADR, frozen specs и release program. Срез `07e2977` добавил документацию к `6f610253`, но не новую реализацию MW.

## Приоритеты

| Группа | Функции | Количество инициатив | Принцип |
| --- | --- | ---: | --- |
| **P0** | [фундамент и блокеры доверия](p0-foundation.md) | 8 | Не расширять публичные обещания, пока не замкнуты используемые production paths и сохранность данных. |
| **P1** | [управляемая память проекта](p1-memory-workspace.md) | 9 | Главный выбранный пользовательский цикл: получить → исправить → импортировать → синхронизировать → перенести. |
| **P2** | [качество знания и интеграция](p2-knowledge-quality.md) | 7 | Повышать полезность и объяснимость на измеряемых задачах; не добавлять автономность без основания. |
| **P3** | [полнота действующей v1.0 платформы](p3-full-platform.md) | 6 | Продуктово последующая глубина; все перечисленные frozen-v1 обязательства остаются обязательными для полного v1.0. |
| **P4** | [расширение после внешней валидации](p4-expansion.md) | 4 | Самостоятельные предложения, запускаемые спросом и готовностью эксплуатации, а не шириной списка конкурентов. |

**P0–P4 — приоритеты; P01–P12 — существующие пакеты.** Более низкий продуктовый приоритет не отменяет обязательный frozen-v1 scope. В частности, AG-UI/A2A/workflows/full qualification нельзя удалить из v1.0 без явного решения.

## Что не нужно делать заново

Уже есть Rust workspace и PostgreSQL, memory identity/revisions, CAS, scoped persistence, HTTP/MCP, route inventory и Bearer auth, Run/outbox/worker, material primitives, части provider/embedding lifecycle и Console. Их наличие следует из выбранных source reads, не из запущенного здесь продукта. Новые API, importer и UI должны переиспользовать их и закрывать сквозные пробелы.

[Реестр состояния](../status.md) отделяет найденные механизмы от неполных внешних surfaces. Критичный пример: MemoryResponse не выдаёт content/current revision, ContextPackDto не выдаёт sections, а memory component не подключён как законченная библиотека. Исправление этих вещей приоритетнее нового типа orchestration.

## Этапы и полезные результаты

| Этап | Результат для пользователя | Зависимость и решение |
| --- | --- | --- |
| **M0. Согласованный baseline** | Разработчик видит точную карту кода, gaps и scope | Source delta, owner decision, проверка документации/тестовой инфраструктуры |
| **M1. Надёжный фундамент** | Данные и операции имеют проверенный путь сохранения/восстановления | P04 closure и P05 по действующим contracts; новые поверхности не обходят guards |
| **M2. Читаемая и редактируемая память** | Получить содержимое, историю, исправить и увидеть результат в Console | MW-00–MW-03; context gate зависит от реальной retrieval readiness |
| **M3. Знания из документов** | Импорт → поиск → исправление → sync conflict → разрешение → экспорт | MW-04–MW-07 и safety/upgrade; первый внешний пилот |
| **M4. Качество и полезные связи** | Лучше решать temporal/conflict/context задачи и работать из одного внешнего агента | Выбирать P2 по результатам M2/M3; консолидация только после измерений |
| **M5. Полный утверждённый v1.0** | Доступны все frozen platform workflows на квалифицированной среде | P06–P12, плюс явно включённые extensions; независимый release verdict |
| **M6. Расширение спросом** | Командные/облачные/дополнительные интеграционные сценарии | Повторное использование, подтверждённая стоимость поддержки, новый scope |

M4 и M5 — **разные ветви**, не безусловная последовательность. Существующий P06–P12 может продолжаться после P05 без ожидания всех P2. M0–M3 также не являются переименованием полных v1.0 gates.

## Предлагаемая очередь ближайшей работы

Сначала delta и защитные границы, затем продолжение принятого P04 и установка/restore P05. После отдельного scope review — MW read, atomic corrections и Console, затем bounded import/sync/export. Read-only проектирование и исходный quality corpus можно готовить раньше, но не объявлять context/indexed без underlying lifecycle.

После первого целого сценария следующую функцию выбирать по наблюдаемому blocker: не найденные данные → диагностика retrieval; неправильное время → temporal semantics; потерянная правка → source/editor contract; тяжёлая установка → DX. Не добавлять новую функцию только ради checkbox конкурента.

## Границы первой публичной alpha

Предлагается узкий shipping manifest, а не обещание полного v1.0: один поддерживаемый режим установки, memory/edit/history, объявленные importer форматы, current context и набор safety/fault tests. Разрешены синтетические/несекретные пилоты в изолированной среде. Реальные production данные требуют проверенной эксплуатации и согласованного допуска.

Имя `0.1.0-alpha.1` — рекомендация; дату и состав определяет владелец по evidence. Full v1.0 по-прежнему требует P12. Включение MW в full v1 — отдельный program amendment; по умолчанию это independent feature milestone без назначения в релиз.

## Сроки и контроль объёма

Нет достоверной базы, чтобы обещать «всё через две недели». Календарь строится после M0 по фактически принятым вертикальным задачам и доступности PostgreSQL/provider/browser environments. Оценка пересматривается на каждом gate. Один активный implementation пакет снижает риск конфликтов в shared writer/migration boundaries; допустима независимая documentation/eval задача рядом.

[Milestones](milestones.md), [связь программ](program-mapping.md), [риски](risks-and-decisions.md), [первые задачи](next-actions.md), [критерии спроса](adoption.md) содержат подробности.

Машиночитаемый источник карт: [feature-register.json](feature-register.json). Карточки P0–P4 включают исходный статус, конкретные deliverables, зависимости и acceptance IDs. Все новые verdict остаются `NOT_RUN_HERE`.

---
**Основание:** [R10: docs/adr/0001-memory-first-persistent-cognition.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/adr/0001-memory-first-persistent-cognition.md), [R11: docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [R18: docs/implementation/memory-workspace/README.md](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/implementation/memory-workspace/README.md), [S01: docs/development-evidence/v1-g0-04-embedding-transition-foundation.md](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md), [S10: crates/vestrace-http/src/api/memory.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs), [S11: crates/vestrace-http/src/api/retrieval.rs](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S14: apps/console/src/main.tsx](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx).

[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
