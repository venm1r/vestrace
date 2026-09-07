# MW-01. Memory read и Context API — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Memory read и Context API.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-00.

## Global Constraints

- Исходный срез `6f6102536e9a535b7086db14573bf45fe750ad71`; перед первой записью актуализировать diff и exact scope.
- 64 KiB UTF-8 на текстовую запись, 100 items, 8 MiB decoded payload на import/export package; это проектные ограничения MVP, не измеренная производительность.
- No direct client SQL, no phantom PASS, no secret/raw-content logs, no parallel authority.
- Idempotency/CAS/policy/source exactness действуют на все новые mutations и worker delivery.
- Не менять protected P04/spec/PLAN.md; новые миграции forward-only, кандидаты0196–0199 не зарезервированы.
- Все changed paths вне принятого индивидуального scope требуют amendment до записи. Не считать scope ниже blanket permission.
- До запуска create необходимых targets ошибки test target not found не считаются поведенческим RED.

## Карта затрагиваемых файлов

Статус каждого пути и ответственность перечислены в [file-plan.json](../file-plan.json). Existing referenced paths перечитываются; new paths проверяются на коллизии. Из broad package map выбрать exact task diff; соседние package paths автоматически не разрешены.

- `crates/vestrace-application/src/memory/mod.rs` — Общие memory use cases, экспорт новых query/write портов [existing_inspected].
- `crates/vestrace-http/src/api/memory.rs` — Сохранить old wire; подключить новый correction writer [existing_inspected].
- `crates/vestrace-http/src/api/retrieval.rs` — Legacy retrieval остаётся неизменным; shared new context service [existing_inspected].
- `crates/vestrace-application/src/retrieval/context_builder.rs` — Единые реальные sections; точное final rendering; counter qualification [existing_inspected].
- `crates/vestrace-application/src/lib.rs` — Подключить новые модули с узкими exports [existing_referenced_recheck_before_edit].
- `crates/vestrace-infrastructure/src/postgres/mod.rs` — Экспорт PostgreSQL adapters [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/api/mod.rs` — Mount new endpoints через inventory [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/router.rs` — AppState dependencies, не default success [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/route_inventory.rs` — Every new route capability/risk and negative coverage [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/server.rs` — Production read/write/import/export composition [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/mcp.rs` — Shared read service composition [existing_referenced_recheck_before_edit].
- `crates/vestrace-mcp/src/server.rs` — Preserve tool names; lawful content hydration [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/schema.rs` — Served OpenAPI generator; do not replace whole schema [existing_referenced_recheck_before_edit].
- `crates/vestrace-application/src/memory/read.rs` — MemoryReadService and lawful view ports [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/memory/context_delivery.rs` — RenderedMemoryContext, renderer and token-counter port [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/src/postgres/memory_read_repository.rs` — Consistent detail/history/browse queries [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/memory_workspace.rs` — New detail/history/correction routes and DTO conversion [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/context_packs.rs` — ContextRequest/ContextResponse endpoint [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/memory_read_contract.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-http/tests/memory_workspace_api.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-application/tests/context_delivery.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-cli/tests/memory_read_wiring.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].

## T01.1. Единое содержательное чтение с полномочиями

**Files:**
- `crates/vestrace-application/src/memory/read.rs`
- `crates/vestrace-infrastructure/src/postgres/memory_read_repository.rs`
- `crates/vestrace-http/src/api/memory_workspace.rs`
- `crates/vestrace-cli/tests/memory_read_wiring.rs`

**Requirements:** MW-R02, MW-R07.

**Interfaces:**

Proposed MemoryReadRepository::detail(&RequestContext, MemoryId) -> Result<Option<MemoryDetail>, ApplicationError>;
MemoryReadRepository::revision(&RequestContext, MemoryId, MemoryRevisionId) -> Result<Option<Revision>, ApplicationError>.
MemoryReadService применяет existing capability и content/disclosure predicates до DTO. MemoryDetail/Revision — поля schemas, domain result отделён от HTTP serialization.
Consumes existing RequestContext/Memory/MemoryRevision/policy. Produces shared service for HTTP/MCP/Console.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A02 Detail hydrate
  Дано Memory r2 в W1, source event разрешён; у W2 известен тот же UUID
  Тогда W1 получает exact r2 text/state; W2 одинаковый 404 без text/ID источника
  И тест отличает ошибку: Выдать metadata source W1 через denial
Сценарий: A09 Disclosure
  Дано Разрешённый список IDs включает источник с denied label/destination
  Тогда Нет запрещённого text/snippet/title в context/detail/source/ошибке
  И тест отличает ошибку: Применить фильтр после отправки в reranker
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test memory_read_contract --locked
cargo test -p vestrace-cli --test memory_read_wiring --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Открыть consistent scoped read; получить memory, active revision, видимые origins.
Проверить revision workspace/identity, действующие content права и attribution.
Проецировать только разрешённые данные; unknown/denied object одинаковый404.
Подключить в server и MCP composition без default-unavailable unnoticed.
Не расширять старый GET metadata wire. Новые precise links появятся MW02, старые пока legacy_unattributed.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A02/A09: r2 content read only after policy. Runtime wiring uses actual server/MCP composition, not direct object-only test.

Все связанные cases: A02, A09. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T01.2. Browse и history без несогласованных страниц

**Files:**
- `crates/vestrace-application/src/memory/read.rs`
- `crates/vestrace-infrastructure/src/postgres/memory_read_repository.rs`
- `crates/vestrace-http/tests/memory_workspace_api.rs`
- `migrations/0196_memory_workspace_receipts.sql`
- `docker/postgres/init-runtime-role.sh`

**Requirements:** MW-R03.

**Interfaces:**

Proposed MemoryReadRepository::list(&RequestContext, BrowseRequest) -> Result<MemoryPage, ApplicationError>;
MemoryReadRepository::history(&RequestContext, MemoryId, HistoryRequest) -> Result<RevisionPage, ApplicationError>.
BrowseRequest: kind/status/collection filter, limit, opaque cursor. HistoryRequest: limit,cursor. Cursor binding follows02.6; collection filter cannot be enabled before MW04 supplies lawful relationship.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A03 Browse pagination
  Дано 3 записи с одинаковым created_at, page size=2; между page1/page2 correction
  Тогда Первые страницы используют id tie-breaker; после mutation CURSOR_EXPIRED
  И тест отличает ошибку: Пропустить/повторить запись молча
Сценарий: A04 История
  Дано r1 закрыта, r2 разрешена; запрос history и прямой r1 URL
  Тогда r1 content/тайный source не раскрываются; r2 доступна
  И тест отличает ошибку: Проверить только label r2 для всего history
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test memory_read_contract --locked
cargo test -p vestrace-http --test memory_workspace_api --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Выполнить keysetquery created_at,id, лимитN+1 после disclosurepredicate.
Cursor зафиксировать на scope/filter/epoch/policy; запрещённые поля не раскрывать.
History исключает невидимый content и сохраняет максимальную revision первойстраницы.
MW01 applies M01 schema and transactional DB epoch triggers covering existing writers; read stability is tested now, not blocked on MW02. Receipt schema is installed but new correction writer is enabled only in MW02.
Добавить negative tests с concurrentmutation и одинаковыми timestamps.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A03/A04: no unnoticed duplicates; changed epoch409; direct historical URL does not bypass permissions.

Все связанные cases: A03, A04. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T01.3. Полезный context и честный учёт размера

**Files:**
- `crates/vestrace-application/src/memory/context_delivery.rs`
- `crates/vestrace-application/src/retrieval/context_builder.rs`
- `crates/vestrace-http/src/api/context_packs.rs`
- `crates/vestrace-application/tests/context_delivery.rs`

**Requirements:** MW-R05, MW-R06.

**Interfaces:**

Proposed ContextDeliveryService::build(&RequestContext, ContextRequest) -> Result<ContextResponse, ApplicationError>.
QualifiedTextCounter::count(&str) -> Result<u32, ApplicationError>, identity() -> &str; registration bound to approved model/destination. No default counter promises model token ceiling.
Consumes existing RetrievalService/ContextPack items; produces rendered_context+sections exact schema. Byte-only result is new bounded read representation, not a claim of full normative ContextPack2 qualification.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A06 Реальный context
  Дано Candidate содержит пустой content и непустой technical explanation
  Тогда Technical explanation не выдаётся как знание; explicit omission
  И тест отличает ошибку: Отрендерить explanation как факт
Сценарий: A07 Бюджет UTF-8
  Дано Русский/emoji text и длинные citations у верхнего byte cap
  Тогда Final rendered_context UTF-8 <= max, separators учтены
  И тест отличает ошибку: Считать только content до citations
Сценарий: A08 Tokenizer
  Дано token_budget задан без tokenizer или неизвестный tokenizer
  Тогда Структурный отказ либо TOKEN_COUNTER_UNAVAILABLE, не ложный enforced=true
  И тест отличает ошибку: Использовать ceil(bytes/4) как точный count
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-application --test context_delivery --locked
cargo test -p vestrace-http --test memory_workspace_api --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Search through existing generation/policy guards. Hydrate only authorized exact revisions.
Render text from realcontent, keep SourceRef; absenttext omitted never explanation fallback.
Build final deterministic renderedstring with separators/citations; enforceUTF8 wholevaluecap.
If tokenpair supplied resolve qualifiedcounter, measurefinalrenderedstring and bound selection; unavailable422.
Recheck generation/policy before send; stale once boundedretry without newexternaldispatch, otherwisetypederror.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A06–A09/A52: context body useful; unknowncounter refused; byteestimate not silently upgraded to guarantee.

Все связанные cases: A06, A07, A08. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.
