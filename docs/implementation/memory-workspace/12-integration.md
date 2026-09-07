# 12. Интеграция с существующей документацией

**Редакция интеграции:** 0.1.1, 2026-09-07.  
**Исходный код:** `6f6102536e9a535b7086db14573bf45fe750ad71`.  
**Объём:** документация и её проверочные артефакты; не реализация продукта.

## 12.1. Единственное место спецификации

Полный пакет находится в `docs/implementation/memory-workspace/`. [Общий указатель](../../README.md), [индекс расширений](../README.md), [архитектурный вход](../../architecture.md), [нормативный индекс](../../specs/README.md) и [индекс планирования](../../plans/README.md) ведут сюда. Новая копия предметных требований в `docs/specs/` или `docs/superpowers/specs/` не создаётся.

Монолитный Markdown и ZIP — производные поставки. При расхождении исправляются по файлам пакета, а не наоборот. `source-manifest.json` остаётся записью источников исходного code-review на указанном commit; интеграция оглавлений не переписывает этот baseline.

## 12.2. Четыре независимых статуса

| Измерение | Статус этой поставки | Что требуется дальше |
| --- | --- | --- |
| Размещение документации | Интеграция запрошена пользователем; подготовлены связанные документы | Обычный review/merge документационного изменения |
| Решения MW-D01–MW-D12 | Proposed; состав и нумерация сохранены | Принятие конкретных решений и уточнение обозначенных compatibility gates |
| Исполнение MW-пакетов | Не выполнялось | MW-00, точный scope, принятые интерфейсы предшественников |
| Квалификация продукта | Не заявлена | Свежие runtime/PostgreSQL/browser/fault evidence на реализации |

Утверждение размещения документов не является blanket-разрешением кода, миграций, зависимостей или изменений защищённых P04 paths. Слова MUST в предметных главах описывают предлагаемый контракт после принятия, не меняют нынешнюю реализацию.

## 12.3. Иерархия и карта соответствия

Существующий Architecture Contract, более новые Accepted ADR и специализированные normative specs сохраняют свою иерархию. MW-пакет уточняет способ реализации выбранного пользовательского цикла; при конфликте меняется MW-документ либо принимается отдельный ADR с явно названным изменением. Совпадение имени сущности не разрешает создавать второго владельца состояния.

| Существующая граница | Применение в MW | Где описана реализация |
| --- | --- | --- |
| Memory-first, canonical vs derived (Architecture Contract §3, Block 1) | UI/CLI работают с канонической памятью; индекс и preview не определяют истину | [Design](01-design.md), [data](02-data-and-transactions.md) |
| Immutable revisions, occurrence/recording/validity (Blocks 1–2) | Исправление создаёт новую ревизию; файл не диктует время факта или доверие | [Data](02-data-and-transactions.md), [sync](05-import-sync.md) |
| Explicit mutations, conflicts, reconciliation (Block 3) | Source version и manual revision сохраняются; B/I/M расхождение требует решения | [Sync contract](contracts/sync-decision-contract.md), [MW-05](plans/05-sync.md) |
| ContextPack provenance, destination policy and hard token bound (Block 4) | Выдача содержимого проходит общий read gate; byte-only ответ не выдаётся за нормативный hard-token ContextPack | [API](03-api.md), [MW-01](plans/01-memory-read-context.md) |
| Capability and workspace authority (Blocks 6–7) | Bearer-resolved identity, current authorization, no authority imported from JSON | [Security](07-security-operations.md), [portability](06-portability.md) |
| Material lifecycle, erasure and disclosure | Source staging/exports используют существующие материалы; ни новый vault, ни plaintext обход | [Data](02-data-and-transactions.md), [MW-04](plans/04-import.md), [MW-06](plans/06-portability.md) |
| Atomic mutation + audit/idempotency/outbox | Один commit фиксирует изменение и receipt; точный повтор не создаёт новое событие | [MW-02](plans/02-atomic-corrections.md) |
| Single execution/recovery boundary | Существующий worker/outbox; специализированные item receipts не образуют generic Task/Attempt runtime | [Program](08-program.md), [import](05-import-sync.md) |
| Scoped qualification/evidence | MW-07 проверяет feature; не присваивает TRUSTED и не закрывает P12 | [Acceptance](09-acceptance.md), [MW-07](plans/07-qualification.md) |

Точные исходные ссылки и прочитанные ranges сохранены в [sources](sources.md) и [source manifest](source-manifest.json). Таблица связывает требования, но не доказывает, что перечисленные механизмы уже работают целиком.

## 12.4. Сосуществование с P01–P12 и историческими планами

Текущий [индекс планов](../../plans/README.md) различает 36-PR transition и P01–P12 gate program. MW-00–MW-07 не перенумеровывают эти программы, не становятся P13–P20 и не меняют frozen package count.

По умолчанию release placement MW **не назначен**. Это явная граница, а не обещание успеть в v1.0. Включение в v1.0 требует отдельного разрешённого program amendment с пересмотром зависимостей и evidence; независимый milestone должен иметь своё имя и пределы приёмки.

Принятый P04/14D останавливается на ResultPrepared. Пакет не объявляет 14E или последующую worker/generation closure выполненной. MW-01 read/detail может рассматриваться отдельно, но выдача context и индексирование не обходят реальную generation/readiness зависимость. Наличие одних лишь исходников или старого PASS недостаточно.

Номера миграций 0196–0199 в плане являются кандидатами. 0195 относится к предложению P04/14E; перед SQL-реализацией MW-00 проверяет актуальный ряд и согласованно обновляет все ссылки. Регистрация документации не резервирует номера.

## 12.5. Пересечения, которые нельзя потерять

1. **Legacy memory write → новый writer.** Одновременные HTTP, Console и import изменения должны сходиться на одном атомарном contract; нельзя оставить старый путь как обход CAS, audit или replay.
2. **Source snapshot → human correction.** Повторная загрузка не стирает редакторскую ревизию, а UI не изменяет сохранённые bytes источника.
3. **Content read → projection.** Detail, history, context, export и diagnostics проходят сопоставимые текущие access checks; cache не становится новой выдающей authority.
4. **Material → sync receipt.** Сбой между vault и SQL обрабатывается существующим material protocol; недоступный материал не подменяется успешным импортом.
5. **Import progress → retrieval readiness.** Applied и indexed не синонимы; интерфейс показывает эти измерения отдельно.
6. **Portable history → local trust.** Импорт сохраняет переданную историю как внешние сведения, но не переносит actor identity, grants, квалификацию или право на classification.

## 12.6. Совместимость, требующая явного решения в MW-00

Сохраняются существующие gates из [handoff](11-handoff.md) и [preflight](plans/00-preflight.md): ordinary-material owner/read, current content policy, idempotency namespace, общий lock order, подходящий tokenizer и занятые P04 пути. Их нельзя закрыть фразой «документация интегрирована».

Byte-only context допускается только как явно названный ограниченный API-ответ после принятия соответствующего уточнения. Клиент, которому нужен нормативный hard-token bound, получает qualified tokenizer path либо отказ, а не незаметный fallback.

## 12.7. Проверка самой интеграции

Проверять пакет командой из [verification](verification/report.md), затем полный документационный diff. Сверять: все исходные требования MW сохранены, ссылки ведут на одну копию, proposed schemas не подменяют runtime schema, защищённые спецификации и исторические evidence не изменены, наружу не попали секреты или временные download URLs.

Фактические результаты этой проверки записываются в `verification/integration-result.json` и `verification/integration-report.md`. Они доказывают только согласованность и состав документации. Проверки продукта перечислены отдельно как невыполненные.
