# MW-02. Atomic corrections и replay — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Atomic corrections и replay.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-00, MW-01.

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
- `crates/vestrace-application/src/memory/ports.rs` — Совместимость существующего MemoryRepository, caller-owned write contract [existing_inspected].
- `crates/vestrace-application/src/memory/services.rs` — Перевод затронутых canonical mutations на общий atomic writer [existing_inspected].
- `crates/vestrace-infrastructure/src/postgres/memory_repository.rs` — Существующий CAS и write in UoW; не вкладывать commit [existing_inspected].
- `crates/vestrace-application/src/governed_mutation.rs` — Переиспользовать commit_in без отдельной новой authority [existing_inspected].
- `crates/vestrace-application/src/idempotency.rs` — Compatibility existing scoped receipts [existing_inspected].
- `crates/vestrace-http/src/api/memory.rs` — Сохранить old wire; подключить новый correction writer [existing_inspected].
- `crates/vestrace-application/src/lib.rs` — Подключить новые модули с узкими exports [existing_referenced_recheck_before_edit].
- `crates/vestrace-infrastructure/src/postgres/mod.rs` — Экспорт PostgreSQL adapters [existing_referenced_recheck_before_edit].
- `crates/vestrace-infrastructure/src/postgres/pool.rs` — Изучить existing permit/UoW; менять только при доказанной необходимости [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/api/mod.rs` — Mount new endpoints через inventory [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/router.rs` — AppState dependencies, не default success [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/route_inventory.rs` — Every new route capability/risk and negative coverage [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/server.rs` — Production read/write/import/export composition [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/schema.rs` — Served OpenAPI generator; do not replace whole schema [existing_referenced_recheck_before_edit].
- `docker/postgres/init-runtime-role.sh` — Exact guarded ownership/execute allowlists [existing_referenced_recheck_before_edit].
- `crates/vestrace-application/src/memory/write.rs` — MemoryMutationPlan/receipt writer over shared UoW [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/src/postgres/memory_mutation_repository.rs` — Caller-owned atomic correction and import participant [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/memory_workspace.rs` — New detail/history/correction routes and DTO conversion [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/memory_mutation_atomicity.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/memory_mutation_replay.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-http/tests/memory_correction_api.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `migrations/0196_memory_workspace_receipts.sql` — PROPOSED number after reserved 0195; MW-00 reallocates atomically if occupied. Exact revision links, epochs, replay/writer guards [candidate_number_not_reserved].

## T02.1. Единый transaction participant для memory mutation

**Files:**
- `crates/vestrace-application/src/memory/write.rs`
- `crates/vestrace-infrastructure/src/postgres/memory_mutation_repository.rs`
- `crates/vestrace-infrastructure/src/postgres/memory_repository.rs`
- `crates/vestrace-infrastructure/tests/memory_mutation_atomicity.rs`

M01 migration is an already-applied read dependency, not a writable task path.

**Requirements:** MW-R04, MW-R08.

**Interfaces:**

Proposed MemoryMutationPlan: authenticated context, target/base revision and state, action payload, reason, exact source link and semantic key.
MemoryMutationRepository::apply_in(&RequestContext, &mut dyn UnitOfWork, &MemoryMutationPlan) -> Result<MutationReceipt, ApplicationError>.
Consumes existing InstallationMutationPermit/UoW and GovernedMutationRepository::commit_in. Result IDs become authoritative only at caller commit. No child method commits independently.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A05 Точная provenance
  Дано Старая memory_sources без revision link и новая correction с link
  Тогда Старое legacy_unattributed; новое exact; timestamps не создают link
  И тест отличает ошибку: Привязать legacy source к ближайшей revision
Сценарий: A10 Atomic rollback
  Дано Fault после revision INSERT, перед Audit/outbox/receipt
  Тогда Rollback всех canonical изменений; прежний active head и epoch
  И тест отличает ошибку: Memory committed, а outbox отсутствует
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test memory_mutation_atomicity --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Take approvedguardorder; perform existing databaseCAS with bounded revisionconversion.
Write new capture/correctionEvent, MemoryRevision, MemorySource and exact link in sameUoW.
Use shared governed mutation for audit+outbox+receipt, updatebrowseepoch.
Retain storage projection write in sameUoW. No provider/vault/network inside transaction.
Use M01 provenance/receipt schema already applied by MW01; never edit its checksum. Old rows not guessed. Verify activeheadFKs, epoch trigger and rollbackall.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A05/A10: injected midcommitfailure leaves no revision/audit/outbox/receipt/epochchange.

Все связанные cases: A05, A10. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T02.2. Устойчивый replay и конфликт разных намерений

**Files:**
- `crates/vestrace-application/src/memory/write.rs`
- `crates/vestrace-infrastructure/src/postgres/memory_mutation_repository.rs`
- `crates/vestrace-application/src/idempotency.rs`
- `crates/vestrace-infrastructure/tests/memory_mutation_replay.rs`

**Requirements:** MW-R09, MW-R10.

**Interfaces:**

Proposed MemoryMutationRepository::commit(&RequestContext, MemoryMutationPlan) -> Result<MutationReceipt, ApplicationError> coordinates existing UoW and apply_in.
Input key scoped per02.5; normalized input and approved keyedcommitment not randomresultIDs. Exact result receipt uses MutationReceipt schema only, no plaintextbody.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A11 Lost response replay
  Дано Correction committed; HTTP response потерян; повтор same body/key
  Тогда Exact original receipt; одна revision/audit/outbox
  И тест отличает ошибку: Повторно проверить старый CAS и вернуть ложный conflict
Сценарий: A12 Same key race
  Дано Два независимых соединения same key+same body, разные generated UUID
  Тогда Один winner, тот же receipt у второго, без дублей
  И тест отличает ошибку: Сравнить случайный audit UUID и конфликтовать
Сценарий: A13 Changed intent
  Дано Два body с одним key; разные target/content/reason
  Тогда 409 без второй mutation
  И тест отличает ошибку: Считать одинаковыми только key
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test memory_mutation_replay --locked -- --test-threads=1
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Freshreauth; lockexisting canonicalkey beforecheckingresult.
If exactcommitted winner, return same receipt before staleCAS; never call applyagain.
If differingintent409; pendingcontenderboundedwait/busy, not neweffect.
If no receipt then apply_in+commit; lostreplyreplayedfromDB.
Add twoconnectionrace and processdeath aftercommit before response. Mutation removingwinnercheck mustfail.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A11–A13: oneeffect, samewinner receipt, changedbody refuses. Wrongprincipal cannot read receipt.

Все связанные cases: A11, A12, A13. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T02.3. Correction/restore и legacy-adapter closure

**Files:**
- `crates/vestrace-http/src/api/memory_workspace.rs`
- `crates/vestrace-http/src/api/memory.rs`
- `crates/vestrace-application/src/memory/services.rs`
- `crates/vestrace-http/tests/memory_correction_api.rs`

**Requirements:** MW-R11, MW-R12.

**Interfaces:**

POST corrections consumes closed CorrectionRequest and required UUIDkey; returns MutationReceipt201. action=restore reads old revision with todaypolicy then creates newtextrevision.
Legacy remember/revise signatures remain, adapt into same participant and map result to existing MemoryResponse. Exact labels inherited, no classificationwritefield.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A14 Two editors
  Дано A/B читают r3/state4, A commits, B sends same base
  Тогда B получает conflict, A content сохранён
  И тест отличает ошибку: Silent last-write-wins
Сценарий: A15 Restore
  Дано Активная r4; разрешённая r1; restore r1 под base r4
  Тогда Новая r5 с content r1; r2–r4 остаются; label не понижается
  И тест отличает ошибку: Переместить active на r1 и уничтожить историю
Сценарий: A16 Legacy compatibility
  Дано Повтор старого create/revise и новый correction той же memory
  Тогда Старый wire shape/числовой If-Match работают через общий writer
  И тест отличает ошибку: Legacy route сохраняет неатомарный bypass
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-http --test memory_correction_api --locked
cargo test -p vestrace-infrastructure --test memory_classification --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Validateclosedshape; legacy numericIfMatch unchanged; newbodypreconditions only.
Read desiredrestorecontent under currentpolicy. If targetclassification would weaken sourceobligation, refuse.
Execute commonwriter; mismatch409. Resolve no implicit additionalstatusaction.
Routeinventory, OpenAPIgenerator and composition updated together. Replay beforeCAS throughsharedwriter.
Run legacyregressionnotonlynewroute suite.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A14–A16: concurrenteditor protected, restore newr5, oldresponse shapes stable and no bypass.

Все связанные cases: A14, A15, A16. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.
