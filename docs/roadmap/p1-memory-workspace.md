# P1 — управляемая память проекта

**Редакция:** 2026-09-07 · **Baseline репозитория:** `07e2977a`.

**Статус:** Предлагаемые функциональные приоритеты; приёмочные проверки не выполнялись.

## Решение о группе

Главный выбранный пользовательский цикл: получить → исправить → импортировать → синхронизировать → перенести.

Готовность каждого пункта разделена на source observation и реальную проверку. Здесь ни один пункт не получает новый runtime PASS. Dependencies ниже задают полный сценарий приёмки, а не запрет заранее писать spec или read-only тест.

<a id="f101"></a>

## F101. Полезный Memory read API, библиотека и история

**Зачем:** Можно увидеть содержимое и точную ревизию до редактирования или использования.

**В коде/документах:** Существующий ограниченный внешний интерфейс.

**Переиспользовать:** MemoryUseCases/find_revision, PgRevisionHydrator, текущие MemoryResponse и MCP get_memory.

**Изменения:**

1. Реализовать единый authorized query port для detail/history/browse.
2. Сохранить совместимость legacy DTO либо явно версионировать изменение; не выдавать новые маршруты за существующие.
3. Страницы и cursors связывать с filters/scope/snapshot; историческая ревизия не заменяется текущей.

**Условия приёмки:**

- **F101-AC01:** Один вызов возвращает разрешённое содержимое, номер ревизии и безопасные provenance refs.
- **F101-AC02:** History открывает точные revisions и не обходится через ID чужого memory.
- **F101-AC03:** Browse не делает N+1 из браузера и не раскрывает скрытые записи через total/cursor.

**Зависимости:** [F002](p0-foundation.md#f002), [F006](p0-foundation.md#f006).

**Связь с программами:** MW-01; release relationship: `proposed-extension`.

**Не включать:** Смена lifecycle, расширение доступа или новый memory store.

**Основание:** [S03](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs), [S10](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs), [R13](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs), [R17](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-mcp/src/server.rs). Наблюдение source не является тестом реализации.

<a id="f102"></a>

## F102. Context API с текстом и честным бюджетом

**Зачем:** Клиент получает пригодный для модели контекст, а не счётчики секций.

**В коде/документах:** Существующий ограниченный внешний интерфейс.

**Переиспользовать:** RetrievalService, ContextPackBuilder, существующие temporal/intent поля и generation guards.

**Изменения:**

1. Выдать реально построенные sections/rendered content, точные revisions, warnings и disclosure destination.
2. Различать byte-only ограничение и hard-token bound с квалифицированным tokenizer.
3. Не подменять пустое содержимое explanation-строкой; ограничить общий rendered payload.

**Условия приёмки:**

- **F102-AC01:** Возвращённый текст совпадает с разрешёнными revisions; источник не повышается до instruction.
- **F102-AC02:** Измерение UTF-8 и tokenizer соответствует реально сформированному контексту, включая separators.
- **F102-AC03:** Stale generation или missing policy не скрываются пустым успешным ответом.

**Зависимости:** [F101](p1-memory-workspace.md#f101), [F001](p0-foundation.md#f001).

**Связь с программами:** MW-01; release relationship: `proposed-extension`.

**Не включать:** Новый retrieval engine и неподтверждённый универсальный token estimator.

**Основание:** [S11](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/retrieval.rs), [S12](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/retrieval/context_builder.rs). Наблюдение source не является тестом реализации.

<a id="f103"></a>

## F103. Исправление и восстановление старого текста

**Зачем:** Пользователь исправляет память, не уничтожая историю и чужие изменения.

**В коде/документах:** Есть в исходниках; полного нового замыкания здесь не проверяли.

**Переиспользовать:** revise_memory, immutable revisions, existing CAS, MW atomic writer.

**Изменения:**

1. Ввести явный correction reason и precondition актуального состояния.
2. Restore прежнего текста создаёт новую revision с источником операции.
3. Label transition не маскируется под обычную правку; текущий запрет смены classification сохраняется.

**Условия приёмки:**

- **F103-AC01:** Параллельная правка даёт conflict без silent overwrite.
- **F103-AC02:** Повтор correction/restore возвращает ту же receipt.
- **F103-AC03:** Старая ревизия неизменна; новый source fact описывает действие автора, а не поддельное подтверждение.

**Зависимости:** [F101](p1-memory-workspace.md#f101), [F003](p0-foundation.md#f003).

**Связь с программами:** MW-02; release relationship: `proposed-extension`.

**Не включать:** Необратимый purge под видом простого delete, semantic auto-merge и автоматическое повышение confidence.

**Основание:** [S05](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/services.rs), [S06](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [S10](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-http/src/api/memory.rs). Наблюдение source не является тестом реализации.

<a id="f104"></a>

## F104. Memory Console: поиск, карточка, редактор

**Зачем:** Управление памятью доступно без SQL, ручных JSON-запросов и знания внутреннего графа.

**В коде/документах:** Есть компонент представления, не законченный UI-сценарий.

**Переиспользовать:** MemoryConsole.tsx, существующая дизайн-система, typed HTTP client и ErrorBoundary.

**Изменения:**

1. Подключить реальный маршрут библиотеки/detail/history и действия correction/restore.
2. Показывать source, timestamps, revision conflict и сохранённое состояние после перезапуска.
3. Empty/loading/denied/unavailable/degraded разделить; новые кнопки не имитируют непоявившуюся функцию.

**Условия приёмки:**

- **F104-AC01:** Browser → HTTP → PostgreSQL edit виден после reload и из второго клиента.
- **F104-AC02:** Два редактора показывают корректный conflict и сохраняют draft до решения.
- **F104-AC03:** Keyboard navigation, focus/error announcements и различимые состояния проходят браузерный checklist.

**Зависимости:** [F101](p1-memory-workspace.md#f101), [F103](p1-memory-workspace.md#f103).

**Связь с программами:** MW-03; release relationship: `proposed-extension`.

**Не включать:** Полный редизайн сайта, 3D graph explorer и отдельная Console.

**Основание:** [S13](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/memory/MemoryConsole.tsx), [S14](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/main.tsx), [S15](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts), [S16](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json). Наблюдение source не является тестом реализации.

<a id="f105"></a>

## F105. Ограниченный импорт источников с предпросмотром

**Зачем:** Память можно наполнить существующей папкой документации без обязательного LLM.

**В коде/документах:** В просмотренных поверхностях — проект MW, без заявленной реализации.

**Переиспользовать:** MW source/collection design, существующие memory/event/material/worker boundaries.

**Изменения:**

1. Markdown/TXT/описанный JSON; local scanner читает только выбранный каталог, сервер не читает произвольный path.
2. Preview фиксирует точные bytes, policy и parser version; apply действует на этот snapshot.
3. Новая source identity отделена от хеша и file path; отказ отдельного элемента прозрачен.

**Условия приёмки:**

- **F105-AC01:** Изменившийся после preview файл не применяется под старым согласием.
- **F105-AC02:** Повтор same input не создаёт новых canonical revisions; другой источник с теми же bytes остаётся другим.
- **F105-AC03:** Path traversal, symlink escape, BOM/invalid UTF-8/oversize/duplicate IDs отклоняются до изменения.

**Зависимости:** [F104](p1-memory-workspace.md#f104), [F008](p0-foundation.md#f008), [F001](p0-foundation.md#f001).

**Связь с программами:** MW-04; release relationship: `proposed-extension`.

**Не включать:** LLM extraction по умолчанию, PDF/OCR, remote crawler и общая файловая система сервера.

**Основание:** [S03](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/memory/mod.rs), [S07](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs), [S08](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs), [S17](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs). Наблюдение source не является тестом реализации.

<a id="f106"></a>

## F106. Синхронизация без потери редакторских изменений

**Зачем:** Обновление документов не уничтожает принятую пользователем коррекцию.

**В коде/документах:** В просмотренных поверхностях — проект MW, без заявленной реализации.

**Переиспользовать:** MW B/I/M decision contract, источники/ревизии и общий atomic writer.

**Изменения:**

1. Фиксировать предыдущий импорт B, новую входную версию I и текущую память M.
2. При расходящихся изменениях создавать предметный sync conflict; manual resolution имеет preconditions.
3. Missing не означает delete; rename определяется устойчивой identity или явным mapping.

**Условия приёмки:**

- **F106-AC01:** Keep-local, accept-source и manual-merge дают новую документированную revision/receipt.
- **F106-AC02:** Cancel racing apply не обещает откат уже committed items; результат показывает applied/pending.
- **F106-AC03:** Сбой после item commit до ack не удваивает source version или revision.

**Зависимости:** [F105](p1-memory-workspace.md#f105).

**Связь с программами:** MW-05; release relationship: `proposed-extension`.

**Не включать:** Свободное LLM-решение конфликтов и last modified wins.

**Основание:** [S07](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/governed_mutation.rs), [S08](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/outbox.rs). Наблюдение source не является тестом реализации.

<a id="f107"></a>

## F107. Экспорт и импорт переносимого знания

**Зачем:** Пользователь может забрать разрешённые данные и повторить эксперимент в другой установке.

**В коде/документах:** В просмотренных поверхностях — проект MW, без заявленной реализации.

**Переиспользовать:** MW portable schema, material/download policy, source and memory identities.

**Изменения:**

1. JSON-пакет с версией формата, разрешённой историей и refs; Markdown как reading view.
2. Новая установка назначает local IDs и трактует remote history как внешний источник.
3. Разделить экспорт знания и backup runtime; declared omissions не раскрывают denied данные.

**Условия приёмки:**

- **F107-AC01:** Проверяется round-trip содержимого/разрешённого provenance без переносимых grants, secrets и qualification.
- **F107-AC02:** Dangling refs, future format, imported authority и неверный active revision отклоняются.
- **F107-AC03:** Отзыв права до нового download блокирует выдачу; уже выданная копия не считается отозванной автоматически.

**Зависимости:** [F106](p1-memory-workspace.md#f106), [F008](p0-foundation.md#f008), [F005](p0-foundation.md#f005).

**Связь с программами:** MW-06, MW-07; release relationship: `proposed-extension`.

**Не включать:** Полное восстановление установки из knowledge JSON и гарантированный отзыв внешних копий.

**Основание:** [R09](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/specs/vestrace-architecture-contract-v0.2.md), [S17](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/crates/vestrace-application/src/material/commands.rs). Наблюдение source не является тестом реализации.

<a id="f108"></a>

## F108. Один typed клиент и стабильный внешний контракт

**Зачем:** Интегратор работает через небольшой API и не повторяет служебную механику Vestrace.

**В коде/документах:** Есть внутренний клиент; предлагается расширить контракт.

**Переиспользовать:** apps/console/src/sdk/client.ts; один shared read/write application path для HTTP/MCP.

**Изменения:**

1. Console становится первым полноценным клиентом; библиотеки используют новые DTO только после появления endpoints.
2. Передавать idempotency/preconditions явно; read errors, retryability и UNKNOWN не сглаживаются в success.
3. Отдельный внешний SDK выбрать по первому реальному интегратору, не выпускать три сразу.

**Условия приёмки:**

- **F108-AC01:** Contract fixtures реально проходят HTTP; parsing отвергает несовместимый ответ.
- **F108-AC02:** Клиент не выставляет доверенный actor/workspace и не делает blind retry необратимых mutations.
- **F108-AC03:** Проверка fresh session с тем же backend демонстрирует продолжение работы.

**Зависимости:** [F101](p1-memory-workspace.md#f101), [F102](p1-memory-workspace.md#f102).

**Связь с программами:** MW-01, MW-03; release relationship: `proposed-extension`.

**Не включать:** Опубликованные package names и методы SDK до их создания.

**Основание:** [S15](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/src/sdk/client.ts), [S16](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/apps/console/package.json), [R01](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/crates/vestrace-http/src/auth.rs). Наблюдение source не является тестом реализации.

<a id="f109"></a>

## F109. Эталонный сценарий и документация первого результата

**Зачем:** Незнакомый с внутренностями инженер выполняет задачу и понимает ограничения.

**В коде/документах:** Есть документация с обнаруженными расхождениями.

**Переиспользовать:** Новая docs structure, MW fixture corpus и существующий local runtime.

**Изменения:**

1. Одна задача: загрузить документы → найти решение → исправить → синхронизировать → экспортировать.
2. Показать нормальное выполнение рядом с fault case.
3. Каждая команда маркирована текущей/предлагаемой и указывает требуемую среду.

**Условия приёмки:**

- **F109-AC01:** Два независимых тестировщика выполняют опубликованный сценарий; необходимая помощь записывается.
- **F109-AC02:** Ожидаемые результаты сверены с реальной версией, sensitive output не включён в публичный кейс.
- **F109-AC03:** Неисполненные шаги не заменены демонстрационным UI или ручным SQL.

**Зависимости:** [F104](p1-memory-workspace.md#f104), [F105](p1-memory-workspace.md#f105), [F108](p1-memory-workspace.md#f108).

**Связь с программами:** MW-07; release relationship: `proposed-extension`.

**Не включать:** Маркетинговое обещание пяти минут без измерения.

**Основание:** [R05](https://github.com/venm1r/vestrace/blob/07e2977a20b05c5b16953a206a6d68bdbff3a052/docs/getting-started.md), [S01](https://github.com/venm1r/vestrace/blob/6f6102536e9a535b7086db14573bf45fe750ad71/docs/development-evidence/v1-g0-04-embedding-transition-foundation.md). Наблюдение source не является тестом реализации.

---
[Карта документации](../README.md) · [Состояние и ограничения](../status.md) · [Реестр источников](../maintenance/sources.md)
