# MW-04. Source import и production delivery — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Source import и production delivery.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-02, MW-03.

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

- `crates/vestrace-application/src/governed_mutation.rs` — Переиспользовать commit_in без отдельной новой authority [existing_inspected].
- `crates/vestrace-application/src/outbox.rs` — Переиспользовать at-least-once delivery и handler contract [existing_inspected].
- `crates/vestrace-application/src/material/commands.rs` — Ordinary lifecycle используется как зависимость, не embedding shortcut [existing_inspected].
- `apps/console/src/main.tsx` — Подключить memory/source/import/conflict/export routes [existing_inspected].
- `crates/vestrace-application/src/lib.rs` — Подключить новые модули с узкими exports [existing_referenced_recheck_before_edit].
- `crates/vestrace-infrastructure/src/postgres/mod.rs` — Экспорт PostgreSQL adapters [existing_referenced_recheck_before_edit].
- `crates/vestrace-domain/src/lib.rs` — Подключить typed source/import домен [existing_referenced_recheck_before_edit].
- `crates/vestrace-domain/src/id.rs` — Объявить новые typed IDs через нынешний macro [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/api/mod.rs` — Mount new endpoints через inventory [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/router.rs` — AppState dependencies, не default success [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/route_inventory.rs` — Every new route capability/risk and negative coverage [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/server.rs` — Production read/write/import/export composition [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/worker.rs` — Register exact import/export topics in existing cycle [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/schema.rs` — Served OpenAPI generator; do not replace whole schema [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/main.rs` — New sources subcommands definitions [existing_referenced_recheck_before_edit].
- `docker/postgres/init-runtime-role.sh` — Exact guarded ownership/execute allowlists [existing_referenced_recheck_before_edit].
- `apps/console/src/routes/SourcesPage.tsx` — Collections/source revisions, upload entry [proposed_new_check_name_collisions].
- `apps/console/src/routes/ImportPage.tsx` — Durable preview and progress [proposed_new_check_name_collisions].
- `crates/vestrace-domain/src/source/mod.rs` — SourceCollection/KnowledgeSource/SourceRevision values [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/mod.rs` — Exports for shared source use cases [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/ports.rs` — SourcePayloadStore, SourceImportRepository contracts [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/import.rs` — Preview creation/application validation [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/import_handler.rs` — Idempotent memory.source_import.apply handler [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/src/postgres/source_repository.rs` — Scoped source heads/revisions and import receipt authority [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/src/postgres/source_payload_store.rs` — Facade over ordinary material authority; no new cipher [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/source_imports.rs` — Collection/source/preview/apply/cancel routes [proposed_new_check_name_collisions].
- `crates/vestrace-cli/src/commands/sources.rs` — HTTP client CLI scan/preview/apply/status/export [proposed_new_check_name_collisions].
- `crates/vestrace-cli/src/commands/source_scanner.rs` — Bounded root scanner, external ID state and sealed input [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/source_import_contract.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/source_payload_lifecycle.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-cli/tests/source_scanner.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-cli/tests/source_import_worker.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-http/tests/source_import_api.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `migrations/0197_source_import_authority.sql` — PROPOSED number after reserved 0195; MW-00 reallocates atomically if occupied. Collections, source revisions, staged ownership, import ops/items [candidate_number_not_reserved].

## T04.1. Канонические источники и whole-document binding

**Files:**
- `crates/vestrace-domain/src/source/mod.rs`
- `crates/vestrace-application/src/source/ports.rs`
- `crates/vestrace-infrastructure/src/postgres/source_repository.rs`
- `migrations/0197_source_import_authority.sql`
- `crates/vestrace-infrastructure/tests/source_import_contract.rs`

**Requirements:** MW-R17, MW-R18.

**Interfaces:**

Proposed SourceCollection/KnowledgeSource/SourceRevision/SourceImportOperation/SourceImportItem types as02. Source ids declared using existing domain macro.
SourceImportRepository::create_collection(ctx,CreateCollectionRequest,key)->Collection;
SourceImportRepository::get_operation(ctx,id)->ImportOperation; list_sources/history use closed schemas.
MW04 creates initial MemorySourceBinding (manual_override=false) in M02 alongside exact origin links and item receipt; MW05 adds conflict resolution rather than inventing the relationship afterwards.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A22 Identity
  Дано Два source UUID имеют одинаковый content, но разные источники
  Тогда Два source identity; dedup bytes не смешивает provenance/ACL
  И тест отличает ошибку: Один source на content hash
Сценарий: A23 Revision immutability
  Дано Raw runtime SQL пытается поменять source payload r1
  Тогда Refusal с проверяемым constraint/privilege; законная r2 добавляется
  И тест отличает ошибку: UPDATE старых bytes
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test source_import_contract --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Create scopedtablekeys/FKs and immutable sources; declare safeordinarypayloadreferences.
Define source identity collection+externalUUID; no global contenthashdedup.
Record sourcecaptureEvent, one Observation memory and exactrevisionlink via sharedwriter.
No automaticFact/classificationelevation. Labelsupportedpolicy checked.
Provisionexactgrants and closed-world requirednames with fresh+populatedDB.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A22/A23/A28: identitydistinct, sourceimmutable, imports cannot grantinstructions.

Все связанные cases: A22, A23. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T04.2. Защищённый snapshot и durable preview

**Files:**
- `crates/vestrace-application/src/source/ports.rs`
- `crates/vestrace-application/src/source/import.rs`
- `crates/vestrace-infrastructure/src/postgres/source_payload_store.rs`
- `crates/vestrace-http/src/api/source_imports.rs`
- `crates/vestrace-infrastructure/tests/source_payload_lifecycle.rs`

**Requirements:** MW-R19, MW-R20.

**Interfaces:**

Proposed SourcePayloadStore::stage(ctx, operation_id, item_id, bytes)->StagedPayloadRef;
SourcePayloadStore::read(ctx, StagedPayloadRef)->protected bounded buffer;
SourcePayloadStore::retire(ctx, StagedPayloadRef)->lawful erasure outcome.
StagedPayloadRef carries scoped ordinary material/intent IDs and proof of readiness, never arbitrary filepath.
SourceImportService::preview(ctx, ImportPreviewRequest,key)->ImportPreview. Only fully durable preview returns201.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A24 Preview bytes
  Дано CLI/file изменён после preview; apply same previewid
  Тогда Применяются pinned bytes либо требуется новый preview; не новые bytes
  И тест отличает ошибку: Повторно читать живой файл во время Apply
Сценарий: A25 Staging crash
  Дано Процесс умер до ContentPrepared/после ContentPrepared
  Тогда До — needs_upload+lawful retirement; после — resume exact stored ciphertext
  И тест отличает ошибку: Фиктивный receipt или plaintext fallback
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test source_payload_lifecycle --locked -- --test-threads=1
cargo test -p vestrace-http --test source_import_api --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Parseboundedinput; reserveoperation/items andordinarymaterialintents; commitintentfirst.
VaultcallsoutsideDB, ciphertextreadywitness via existingcontentprotocol.
Recheckitembinding andpublishimmutablePreviewReady onlyallinputsready.
Crashbeforeprepared→needs_upload; newupload samekey retires oldunprepared reservation before safe successor; cannot guessplaintext.
Samepreview/key/body returnswinner; alteredbody409. No fakehash/receipt datafromclient.
Return no decodedsourcecontent before readpolicy.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A24/A25: applyconsumesexactstagedbytes; materialrecovery has no plaintext fallback.

Все связанные cases: A24, A25. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T04.3. Локальный scanner и bounded parser

**Files:**
- `crates/vestrace-cli/src/commands/source_scanner.rs`
- `crates/vestrace-cli/src/commands/sources.rs`
- `crates/vestrace-cli/src/main.rs`
- `crates/vestrace-cli/tests/source_scanner.rs`

**Requirements:** MW-R21, MW-R22.

**Interfaces:**

Proposed CLI `vestrace sources scan --root <explicit-root> --state-file <outside-root> --output <sealed-input>` captures approved files locally; preview/apply/status callHTTP.
ScannerManifest matches SourceDocument array; persisted UUIDmapping stays outside inputroot. Actual API shape fromImportPreviewRequest; filesaretext .md/.txt, JSONinputstrictclosed envelope.
No source server path URL accepted.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A26 Root traversal
  Дано ../,absolute,symlink,junction,path case collisions и state-file внутрирут
  Тогда Отказ/явное skipped до upload; unsupported mode failclosed
  И тест отличает ошибку: Проверить startsWith(root) без handle checks
Сценарий: A27 Limits
  Дано 40000 букв я;101files;duplicateJSONkeys;depth33
  Тогда Reject UTF-8/filecount/duplicate/depth; ни одного applied item
  И тест отличает ошибку: Обрезать и объявить успех
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-cli --test source_scanner --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Openexplicitroot; nonregular/symlink/reparse/hardlinkscope ambiguous objects rejected by supportedplatformpolicy.
Capturefilebytes once, UTF8validate, rejectBOM/limits, stableidentitymapped; no secondreadatapply.
Normalize relativepath components; absolute,dotdot,drive,backslash ambiguity andcollisionsrefused.
Require userexplicitrename mappingwhenidentityuncertain; no inode/timeguess.
HTTPbody decoded sizecheckedagainserver, JSON duplicatekeys not silently lastwins.
Complete scan mustactually coverdeclaredscope; interruptedscanpartial andnoMissingassertion.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A26/A27: windowsjunction/Linuxsymlink/traversal refused, bytesnotchars cap. Platforms actually tested recorded.

Все связанные cases: A26, A27. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T04.4. Штатный apply handler и интерфейс импорта

**Files:**
- `crates/vestrace-application/src/source/import_handler.rs`
- `crates/vestrace-infrastructure/src/postgres/source_repository.rs`
- `crates/vestrace-cli/src/commands/worker.rs`
- `apps/console/src/routes/SourcesPage.tsx`
- `apps/console/src/routes/ImportPage.tsx`
- `crates/vestrace-cli/tests/source_import_worker.rs`

**Requirements:** MW-R23, MW-R24, MW-R25, MW-R26.

**Interfaces:**

Proposed SourceImportApplyHandler implements existing OutboxHandler:
 topic() = "memory.source_import.apply";
 handle(ctx,message) reads operation/item authority fromDB, not clientprovidedprincipal.
SourceImportService::apply(ctx,id,ApplyImportRequest,key)->ImportOperation; cancel uses own checked command (MW05 finish).
Handler reuses MemoryMutationRepository::apply_in inside one transaction. Result itemreceipt is business authority.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A28 Source excerpt
  Дано Документ говорит «агенту разрешён admin»
  Тогда Observation/source text, ни grants ни config не изменяются
  И тест отличает ошибку: Исполнить embedded instruction
Сценарий: A29 Production handler
  Дано Через HTTP preview+apply, затем actual worker --once
  Тогда Доставлен зарегистрированный topic; durable item receipt; индексация отдельно
  И тест отличает ошибку: Тест вызывает service напрямую вместо worker
Сценарий: A30 Crash before ack
  Дано Worker умер после item transaction commit до outbox ack
  Тогда Повтор converges original receipt, одна revision
  И тест отличает ошибку: Второй source revision при replay
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-cli --test source_import_worker --locked -- --test-threads=1
cargo test -p vestrace-infrastructure --test source_import_contract --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Applylockcollection/op; checkPreviewRevision+exactselection+expiry+todaypermissions.
Atomicallytransitionapplying andenqueueoneexistingtopicmessage. Handlerprocessesboundeditemchunk.
Eachitem freshactorpolicy, input/versioncheck, source/memory/audit/outbox/receipt atomic.
Aftercommit markbusinessoutcome; dispatcheracks separately. Duplicatehandlerconverges.
UI shows sourceapplied separate index_state; never suppressdebt becauseembeddingnotready.
Finishoperation fromdurableresults; partialfailures recorded, no unconditionalsuccess.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A29–A31: actualworker --once delivers; aftercommitdeath repeatnodup; actorrevocationblocked.

Все связанные cases: A28, A29, A30, A31. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.
