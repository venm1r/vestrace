# ADR: Vestrace durable event schema compatibility

**Статус:** предложено для письменного утверждения  
**Дата:** 2026-07-31  
**Репозиторий:** `venm1r/vestrace`  
**Связанный boundary:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`  
**Основание:** `docs/superpowers/specs/2026-07-31-vestrace-state-engine-reconciliation-design.md`  
**Владельцы контрактов:** H1 durable Run journal, H7 interaction/public events, H10 audit/evaluation records, H11 public schema publication

> Этот ADR является documentation-only решением. Он не разрешает менять Rust-код, миграции, существующие journal rows, public event stream, generated schemas или runtime serialization.

## 1. Контекст

Vestrace хранит несколько классов долговечных событий и фактов:

- H1 `RunEvent` для авторитетных logical Run mutations;
- H7 interaction, continuation и public-event records;
- H2/H6/H8 обязательные domain и security facts;
- H10 security audit, evaluation, verification и replay records;
- H11 публичные event DTO и versioned schemas.

Эти записи переживают рестарты, обновления бинарника, экспорт, replay, аудит и длительное хранение. Поэтому совместимость нельзя связывать только с текущим Rust enum или текущим SQL row type.

Без отдельного контракта возможны опасные сценарии:

1. существующий event kind получает новое значение полей без явной версии;
2. переименование Rust variant незаметно меняет публичный wire format;
3. новая версия приложения перестаёт читать старый Run journal;
4. миграция переписывает historical payload и ломает audit/hash evidence;
5. public stream публикует внутренний payload без стабильного schema contract;
6. extension или model регистрирует произвольный event kind и превращает schema registry в runtime authority;
7. неизвестная будущая версия тихо интерпретируется как старая;
8. replay или import выполняется на частично понятом событии.

## 2. Решение

Vestrace использует стабильный event kind и независимую per-kind schema version.

Нормативная пара:

```text
(stable_event_kind, schema_version)
```

- `stable_event_kind` определяет неизменяемую бизнес-семантику факта;
- `schema_version` определяет точный сериализованный контракт envelope/payload для этого kind;
- версия начинается с `1` отдельно для каждого stable kind;
- любое изменение serialized contract создаёт новую schema version;
- изменение смысла создаёт новый stable event kind, даже если поля похожи;
- historical rows не переписываются;
- чтение старых форматов выполняется pure read-time upcasters;
- H11 публикует generated schemas и compatibility metadata;
- неизвестная или неподдерживаемая версия обрабатывается fail-closed для interpretation, replay и mutation.

## 3. Не единая универсальная event-модель

ADR не создаёт один новый глобальный event store и не переносит authority между Horizons.

Сохраняется распределённое владение:

| Область | Владелец | Что остаётся авторитетным |
|---|---|---|
| Run ordering и logical mutations | H1 | `RunEvent`, Run sequence/version и atomic commit |
| Interaction и continuation semantics | H7 | interaction/public-event records и durable cursor |
| Authorization, Artifact, credential facts | H2/H6/H8 | owning domain/audit contracts |
| Security audit, evaluation и replay | H10 | собственные durable records и integrity chain |
| External DTO, schema publication и compatibility surface | H11 | `/v1` contracts, SDK types и published schemas |

Общий compatibility contract применяется к сериализации, но не создаёт общий mutable aggregate или общий lifecycle.

## 4. Event envelope

Каждый сериализованный durable event использует концептуальный envelope:

```text
EventEnvelope
├── event_id
├── stable_event_kind
├── schema_version
├── workspace_id
├── authority_scope
├── aggregate_or_reference_ids
├── sequence_or_cursor
├── correlation_id?
├── causation_id?
├── occurred_at?
├── recorded_at
├── payload
└── payload_hash
```

Точное наличие полей зависит от owning Horizon, но следующие правила обязательны.

### 4.1 `event_id`

- глобально уникальный UUIDv7;
- идентифицирует один зафиксированный факт;
- не используется как единственное доказательство порядка;
- не меняется при upcast или projection.

### 4.2 `stable_event_kind`

- строковый или generated typed identifier;
- имеет стабильное wire name;
- описывает уже произошедший факт;
- не зависит от имени Rust module/variant;
- не переиспользуется для другого смысла.

Примеры допустимого naming:

```text
run.created
run.step_started
run.outcome_unknown
interaction.user_message_recorded
approval.granted
artifact.revision_registered
```

Naming style может различаться между internal и public surfaces только через явный mapping. Неявное использование `Debug`/variant name как wire kind запрещено.

### 4.3 `schema_version`

- положительное целое число;
- начинается с `1` для каждого stable kind;
- относится к exact serialized envelope/payload contract;
- не является номером версии приложения, API major, migration или Run;
- не сбрасывается при переименовании Rust type;
- не определяется моделью, extension или внешним producer.

### 4.4 Authority и references

Envelope содержит только identifiers/references, необходимые owning contract.

Reference не предоставляет permission. Наличие `workspace_id`, `run_id`, `artifact_id` или другого ID не разрешает чтение ресурса и не заменяет H2/H6 policy.

### 4.5 Sequence или cursor

Порядок определяется owning Horizon:

- H1 использует authoritative Run sequence/version;
- H7 использует durable workspace/public-event cursor;
- H10 audit использует audit-stream sequence и previous-hash linkage;
- transport offset не становится domain order.

Один общий global sequence для всех event families этим ADR не вводится.

### 4.6 `occurred_at` и `recorded_at`

- `recorded_at` обязателен и задаётся Vestrace authority при durable commit;
- `occurred_at` опционален для внешнего или фактического времени источника;
- external timestamp считается untrusted data и проходит bounds/normalization;
- отсутствие `occurred_at` не блокирует запись;
- upcaster не заменяет historical timestamps текущим временем.

### 4.7 `payload_hash`

Payload hash:

- вычисляется по нормативной canonical serialization соответствующего schema version;
- не включает transport framing, compression и connection-specific metadata;
- не пересчитывается поверх upcast representation как замена historical hash;
- позволяет доказать exact persisted bytes/semantic document, определённый owning contract;
- не является разрешением на доступ к payload.

Если owning Horizon уже имеет более сильную integrity chain, ADR не заменяет её одним `payload_hash`.

## 5. Stable event kind semantics

### 5.1 Смысл kind неизменяем

После публикации stable kind его смысл не меняется.

Например, если `run.completed` первоначально означает подтверждённое завершение после verification gate, его нельзя позднее использовать для optimistic или unverified completion.

Для новой семантики создаётся новый kind либо другой owning record.

### 5.2 Прошедшее время и факт

Event kind обозначает факт, а не команду или намерение.

Правильно:

```text
approval.granted
run.cancel_requested
tool.outcome_became_unknown
```

Неправильно:

```text
grant_approval
cancel_run
retry_tool
```

Commands имеют собственные request/receipt contracts и могут породить ноль или несколько durable events.

### 5.3 Kind не равен UI-сообщению

Публичная presentation может группировать, локализовать или скрывать детали, но не меняет canonical event kind.

UI label не является schema identifier.

## 6. Правила изменения схемы

### 6.1 Любое wire-format изменение создаёт новую версию

Новая schema version требуется при любом изменении сериализованного контракта, включая:

- добавление optional или required поля;
- удаление поля;
- переименование поля;
- изменение типа;
- изменение enum set;
- изменение nullability;
- изменение units, scale или timestamp format;
- изменение normalization/canonicalization;
- изменение вложенной структуры;
- изменение допустимых bounds;
- изменение required relationship между полями.

Это намеренно строгий контракт. Существующий schema hash для `(kind, version)` никогда не меняется.

### 6.2 Документационные правки без изменения контракта

Исправление описания или примера может не увеличивать schema version только если:

- generated schema bytes и schema hash не изменились;
- semantics не изменились;
- validation behavior не изменилось;
- compatibility tests подтверждают идентичность.

### 6.3 Breaking semantic change

Если изменяется смысл, создаётся новый stable kind, а не только новая schema version.

Примеры:

- `run.completed` больше не требует verification;
- `approval.granted` начинает означать workspace-wide grant вместо exact operation binding;
- `artifact.deleted` начинает означать physical purge вместо logical deletion.

### 6.4 Split и merge

Разделение одного kind на несколько или объединение нескольких kinds создаёт новые kinds.

Upcaster может предоставить compatibility view, но не переписывает historical identity и не утверждает, что старое событие физически стало несколькими новыми событиями.

## 7. Schema registry

Vestrace использует repository-owned schema registry.

Registry состоит из:

- typed Rust definitions владельца;
- generated JSON Schema documents;
- stable kind metadata;
- schema version;
- schema hash;
- compatibility classification;
- upcaster path metadata;
- owning Horizon;
- exposure level: internal, audit, public или export-only;
- retention/classification notes;
- test vectors.

Registry не является model-editable runtime table.

### 7.1 Кто может добавлять schema

Новая schema добавляется только через reviewed repository change владельца Horizon.

Запрещено:

- модели регистрировать event kinds;
- Agent Package создавать canonical Run events;
- extension публиковать произвольный Vestrace-owned event kind;
- webhook payload определять event kind или schema version;
- admin SQL менять active schema semantics;
- runtime download автоматически активировать schema.

Extensions могут иметь собственные namespaced payload schemas внутри H9 extension protocol, но Vestrace сохраняет их как untrusted external payload/reference и не превращает автоматически в canonical event kind.

### 7.2 Путь публикации

Для H11 public API major `v1` рекомендуемый repository layout:

```text
schemas/events/v1/<stable-kind>.v<schema-version>.json
schemas/events/v1/index.json
schemas/events/v1/compatibility.json
```

Здесь первый `v1` — major публичного event surface, а suffix `.vN` — per-kind schema version.

Internal-only schemas могут жить в отдельном generated namespace, но public и internal contracts не должны ссылаться на один mutable file с разной семантикой.

### 7.3 Schema hash

Каждый generated document имеет deterministic schema hash.

Release manifest и compatibility baseline содержат exact hashes. Изменение файла с тем же `(surface, kind, version)` считается contract violation.

## 8. Upcasters

### 8.1 Назначение

Upcaster преобразует старую сериализованную версию в следующую поддерживаемую in-memory representation для чтения.

```text
(kind, v1 bytes)
→ validate v1
→ upcast v1→v2
→ validate v2
→ ...
→ current read model
```

### 8.2 Обязательные свойства

Upcaster должен быть:

- pure;
- deterministic;
- side-effect free;
- total для всех payload, валидных по исходной schema;
- bounded по CPU и памяти;
- независим от сети;
- независим от текущего времени;
- независим от current policy;
- независим от mutable database state;
- не использующим модель;
- не читающим secrets;
- не выполняющим Tool/connector calls.

### 8.3 Что запрещено upcaster

Upcaster не может:

- обновлять journal row;
- создавать новое canonical event;
- менять `event_id`;
- менять historical sequence/cursor;
- подменять `recorded_at`;
- выполнять authorization decision;
- восстанавливать отсутствующие данные догадкой;
- повышать confidence или verification outcome;
- разрешать `Unknown` outcome;
- активировать memory или Artifact;
- скрывать несовместимость default-значением, которое меняет смысл.

### 8.4 Missing information

Если новая read model требует данных, которых нет в старой версии, допустимы только явные варианты:

- bounded `None`/`Unknown`/`NotRecorded` field;
- compatibility wrapper с source version;
- отказ от конкретной операции как `insufficient_event_data`;
- сохранение старого read path.

Нельзя создавать фиктивное значение, выглядящее как historical fact.

### 8.5 Upcaster chain

Поддерживается последовательная цепочка:

```text
v1 → v2 → v3
```

Прямой `v1 → v3` оптимизатор допустим только если property/fixture tests доказывают эквивалентность нормативной цепочке.

## 9. Persistence и migrations

### 9.1 Historical rows immutable

Schema migration не переписывает historical event payload для нормального version upgrade.

Запрещены массовые операции вида:

```text
UPDATE run_events SET payload = upcast(payload)
DELETE old events after projection rebuild
replace old kind/version in place
```

### 9.2 Допустимые migration changes

Миграция может добавлять:

- envelope/version column для новых записей;
- immutable schema hash metadata;
- indexes;
- compatibility lookup tables с repository-controlled data;
- projection cursor/state;
- constraints для будущих writes;
- content-free migration audit facts.

Она не меняет смысл уже записанного факта.

### 9.3 Legacy rows

Если существующая версия Vestrace имеет записи без явного `schema_version`, migration/design обязан:

1. доказать однозначное соответствие legacy layout конкретной версии;
2. назначить compatibility interpretation без изменения payload bytes;
3. сохранить provenance о legacy origin;
4. fail closed для неоднозначных rows;
5. не маркировать corrupted/unknown rows как валидные v1.

## 10. Internal Rust compatibility

Rust type evolution и wire compatibility тестируются отдельно.

Запрещено полагаться на:

- default Serde enum tagging без зафиксированного contract;
- Rust variant order;
- `Debug` output;
- module path;
- auto-derived field rename;
- architecture-dependent integer width;
- unordered map iteration;
- floating-point representation для authoritative values.

Typed domain model может быть реорганизована без wire change, если adapter сохраняет exact published schema.

## 11. Public event compatibility

### 11.1 H11 API major и event schema version

Public API major и event schema version независимы:

```text
/v1 event stream
+ stable_event_kind
+ schema_version
```

Новый payload version не требует автоматически `/v2`, если H11 `v1` contract допускает versioned event variants и клиент может безопасно определить поддержку.

Изменение transport semantics, authentication, cursor semantics или общего public envelope может потребовать новый API major.

### 11.2 Public projection

H11 не обязан публиковать внутренний H1/H7/H10 payload напрямую.

Public event может быть policy-filtered projection, но обязан иметь:

- собственный stable public kind;
- собственную schema version;
- reference на source event/fact без раскрытия forbidden data;
- monotonic durable cursor из H7 contract;
- bounded payload;
- explicit omission/redaction metadata, когда это безопасно.

### 11.3 Client behavior

Клиент обязан проверять `(kind, schema_version)`.

Неизвестный event:

- может быть сохранён как opaque envelope для diagnostics, если policy разрешает;
- может быть пропущен presentation layer с explicit warning;
- не может интерпретироваться как известный более старый payload;
- не может вызывать state mutation;
- не может автоматически подтверждать success/approval/completion.

SDK предоставляет typed known variants и `UnknownEventEnvelope`, сохраняя identifiers, kind, version и bounded raw/reference representation согласно data policy.

## 12. Replay, recovery и projections

### 12.1 H1 logical replay

H1 replay читает только поддерживаемые validated schemas/upcasts.

Если событие, необходимое для Run state, неизвестно или corrupted:

- replay останавливается;
- Run получает recovery/compatibility failure через существующий H1 mechanism;
- side effects не выполняются;
- текущая projection не считается доказательством правильного состояния.

### 12.2 H10 safe replay

H10 safe replay может работать только при достаточных schemas и captured data.

Unsupported version приводит к `NotReplayable`, `Inconclusive` или explicit compatibility failure. Оно не разрешает model-based reconstruction.

### 12.3 Projection rebuild

Projector:

1. читает immutable source event;
2. валидирует exact source schema;
3. применяет approved upcaster chain;
4. строит projection;
5. сохраняет source cursor/version и projector revision.

Projection rows могут перестраиваться. Source events — нет.

### 12.4 Lagging old consumer

Старый consumer не должен получать новый schema version без возможности определить её.

Для внутренних workers rollout policy обязана обеспечить либо:

- consumer поддерживает новую version до начала producer emission;
- producer emission включается feature/revision gate после обновления consumers;
- compatibility public projection продолжает старую version на ограниченный срок;
- operation fail closed.

Silent downgrade запрещён.

## 13. Import и signed Run export

Будущий Run export обязан включать:

- exact source event bytes или canonical serialized representation;
- stable kind;
- schema version;
- schema hash/reference;
- required schemas;
- upcaster compatibility metadata или minimum reader version;
- checksums/signature scope.

Import сначала проверяет manifest, hashes, signature и schema support, а затем parse/upcast.

Unsupported schema не активирует Run, не создаёт work и не вызывает replay. Допускается только inert inspection/opaque preservation, если policy разрешает.

## 14. Compatibility policy

### 14.1 Reader support window

Каждая released Vestrace version публикует:

- schemas, которые она может производить;
- schemas, которые она может читать;
- upcaster chains;
- minimum compatible release/export version;
- deprecated versions;
- planned removal gate.

### 14.2 Deprecation

Schema version может быть deprecated, но persisted historical support нельзя удалить, пока:

- retained authoritative events этой версии существуют;
- supported exports могут содержать её;
- upgrade path требует её чтения;
- legal/audit retention требует verification.

Удаление reader/upcaster требует отдельного retention and migration review, а не обычного refactor.

### 14.3 Producer retirement

Vestrace может прекратить создавать старую version после controlled rollout, сохраняя read support.

## 15. Error semantics

Compatibility errors используют bounded categories:

```text
unknown_event_kind
unsupported_schema_version
schema_hash_mismatch
payload_validation_failed
upcast_failed
upcast_chain_missing
insufficient_event_data
corrupted_event_envelope
incompatible_public_client
```

Errors не раскрывают raw sensitive payload, SQL, stack trace, secret, backend path или unrestricted parser output.

### 15.1 Fail-closed operations

Следующие операции fail closed при неизвестной необходимой schema:

- authoritative Run replay;
- Run resume;
- approval interpretation;
- budget/accounting reconstruction;
- external-action reconciliation;
- verification completion;
- memory activation;
- Artifact export decision;
- import activation;
- mutation command, основанная на event-derived expected state.

### 15.2 Partial query

Administrative query может вернуть известную часть и explicit compatibility warning, если это не создаёт ложное утверждение о полном состоянии.

## 16. Security implications

Event payload считается untrusted при чтении, даже если находится в собственной БД, поскольку он мог быть создан старой/скомпрометированной версией или импортирован.

Обязательны:

- schema validation до typed deserialization/use;
- size/depth/string bounds;
- deny unknown fields там, где они могут менять security semantics;
- controlled handling unknown fields в явно forward-compatible presentation DTO;
- no code execution/custom class loading;
- no external references fetched during parsing/upcast;
- no secret material in errors;
- policy check при доступе к payload;
- integrity verification до import/replay;
- fuzz/property tests для parsers/upcasters.

## 17. Testing contract

Минимальный будущий test suite:

### 17.1 Schema immutability

- published schema hash для `(kind, version)` не меняется;
- duplicate registration с другим hash отклоняется;
- generated schema deterministic.

### 17.2 Round-trip

- current event serializes and validates;
- deserialize/serialize сохраняет normative canonical form;
- identifiers, timestamps и fixed-point values не меняются.

### 17.3 Upcaster fixtures

- каждый historical fixture проходит exact chain;
- old valid payload не вызывает panic;
- missing data остаётся explicit;
- chain deterministic;
- optimized path эквивалентен sequential path.

### 17.4 Failure tests

- unknown kind;
- unsupported future version;
- schema hash mismatch;
- malformed payload;
- oversized/deep payload;
- corrupted sequence/cursor;
- missing upcaster;
- upcaster error;
- ambiguous legacy row.

### 17.5 Replay safety

- unknown event блокирует authoritative resume;
- no Tool/model/remote call occurs;
- projection не используется как fallback truth;
- public client does not mutate state from unknown event.

### 17.6 Public contract

- SDK recognizes all published `(kind, version)` pairs;
- unknown envelope preserved safely;
- SSE reconnect/cursor behavior independent from payload version;
- no internal-only fields leak into public schema;
- compatibility baseline detects changed existing schema.

## 18. Rollout sequence

После утверждения ADR implementation plan должен идти в порядке:

1. inventory всех durable event families и existing wire layouts;
2. назначение owning Horizon и stable kinds;
3. фиксация legacy version `1` только там, где mapping однозначен;
4. generated schemas и hashes;
5. validation layer;
6. read-time upcasters;
7. compatibility tests/fixtures;
8. producer rollout gates;
9. H11 schema index и SDK support;
10. export/import integration;
11. cross-plan normalization.

Нельзя сначала начать emission новой version, а затем добавлять readers.

## 19. Отклонённые альтернативы

### 19.1 Version only by application release

Отклонено: одна release version не сообщает, какой именно event contract изменился, и связывает независимые Horizons.

### 19.2 Одна глобальная event schema version

Отклонено: изменение одного kind заставляет искусственно версионировать все остальные.

### 19.3 «Добавление optional fields всегда backward-compatible»

Отклонено: старые signatures/hashes, strict clients и semantic assumptions могут измениться. Любое wire change получает новую per-kind version.

### 19.4 Переписывать journal при migration

Отклонено: ломает append-only history, integrity evidence и forensic provenance.

### 19.5 Хранить только latest normalized payload

Отклонено: теряется точный historical contract и возможность доказать, что было записано.

### 19.6 Model-assisted upcasting

Отклонено: недетерминированно, дорого, может выдумывать факты и создаёт side effects/data exposure.

### 19.7 Runtime mutable schema registry

Отклонено: позволяет обойти review и превратить external/model input в authority.

### 19.8 Treat unknown future version as latest known

Отклонено: опасное silent misinterpretation.

## 20. Consequences

Положительные последствия:

- длительно живущие Runs остаются читаемыми после обновлений;
- публичные клиенты могут точно определять поддержку;
- audit/export сохраняют exact historical contract;
- projections перестраиваются без mutation source history;
- schema drift обнаруживается CI/release gates;
- owners H1/H7/H10/H11 сохраняют authority boundaries.

Стоимость:

- каждая serialized change требует новой schema version;
- нужны fixtures и upcasters;
- release/rollout становится более строгим;
- reader support может жить дольше producer support;
- generated schema registry требует дисциплины.

Эта стоимость принимается, потому что durable state, replay и audit важнее удобства неявного изменения payload.

## 21. Documentation-only boundary

Этот ADR не разрешает:

- менять H1/H7/H10/H11 runtime contracts;
- создавать migration;
- добавлять schema tables;
- генерировать или публиковать schemas;
- менять Serde attributes;
- переписывать historical rows;
- включать новый public event kind;
- создавать implementation branch;
- выполнять replay/import.

После письменного утверждения ссылки и краткие normative clauses добавляются в H1, H7 и H11 документы на cross-plan normalization pass либо отдельным documentation-only amendment.

## 22. Итоговый инвариант

> **У каждого долговечного сериализованного события Vestrace есть стабильный смысл и точная per-kind schema version. Новые readers адаптируют старые immutable события pure upcasters; старые события никогда не переписываются под текущую модель, а неизвестная версия никогда не интерпретируется оптимистично.**
