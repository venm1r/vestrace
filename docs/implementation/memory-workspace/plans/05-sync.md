# MW-05. Sync и ручные конфликты — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Sync и ручные конфликты.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-04.

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
- `apps/console/src/main.tsx` — Подключить memory/source/import/conflict/export routes [existing_inspected].
- `crates/vestrace-application/src/lib.rs` — Подключить новые модули с узкими exports [existing_referenced_recheck_before_edit].
- `crates/vestrace-infrastructure/src/postgres/mod.rs` — Экспорт PostgreSQL adapters [existing_referenced_recheck_before_edit].
- `crates/vestrace-domain/src/lib.rs` — Подключить typed source/import домен [existing_referenced_recheck_before_edit].
- `crates/vestrace-domain/src/id.rs` — Объявить новые typed IDs через нынешний macro [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/api/mod.rs` — Mount new endpoints через inventory [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/router.rs` — AppState dependencies, не default success [existing_referenced_recheck_before_edit].
- `crates/vestrace-http/src/route_inventory.rs` — Every new route capability/risk and negative coverage [existing_referenced_recheck_before_edit].
- `crates/vestrace-cli/src/commands/schema.rs` — Served OpenAPI generator; do not replace whole schema [existing_referenced_recheck_before_edit].
- `docker/postgres/init-runtime-role.sh` — Exact guarded ownership/execute allowlists [existing_referenced_recheck_before_edit].
- `apps/console/src/routes/SourceConflictPage.tsx` — B/I/M explicit resolution [proposed_new_check_name_collisions].
- `crates/vestrace-domain/src/source/sync.rs` — Pure classify_sync and closed conflict vocabulary [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/import_handler.rs` — Idempotent memory.source_import.apply handler [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/sync.rs` — Conflict resolution through shared mutation [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/src/postgres/source_repository.rs` — Scoped source heads/revisions and import receipt authority [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/source_imports.rs` — Collection/source/preview/apply/cancel routes [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/source_conflicts.rs` — Exact conflict details/resolution routes [proposed_new_check_name_collisions].
- `crates/vestrace-domain/tests/source_sync.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/source_sync_races.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `apps/console/e2e/source-conflict.spec.ts` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `migrations/0198_source_sync_conflicts.sql` — PROPOSED number after reserved 0195; MW-00 reallocates atomically if occupied. Bindings, B/I/M conflicts, resolution guards [candidate_number_not_reserved].

## T05.1. Чистая классификация sync, rename и Missing

**Files:**
- `crates/vestrace-domain/src/source/sync.rs`
- `crates/vestrace-application/src/source/sync.rs`
- `crates/vestrace-domain/tests/source_sync.rs`
- `crates/vestrace-application/src/source/import_handler.rs`

**Requirements:** MW-R27, MW-R30.

**Interfaces:**

Proposed SyncFacts is trusted internal comparison of immutable B/I/M payloads, identity and override; never deserialized fromHTTP.
classify_sync(facts: &SyncFacts)->SyncDecision enum {Unchanged,Rename,Update,Conflict,Missing,Blocked}.
Inputs read via authorizedmaterialsource; equalitycomputedoutsideDB then exactversionrefs recheckedunderlocks. No publicdigest service.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A32 Unchanged/rename
  Дано Same externalid/content; затем явный новый locator
  Тогда Первый no new revision; второй source locator event, memory без дубля
  И тест отличает ошибку: Новая memory при каждом scan
Сценарий: A36 Partial vs missing
  Дано Partialscan не включает файл; completemanifest отдельно не включает его
  Тогда Partial ничего не объявляет missing; complete записывает Missing без Delete
  И тест отличает ошибку: Удалить память при исчезновении файла
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-domain --test source_sync --locked
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Sameexternalidandsamecontentwithnorelevantchanges→Unchanged.
Onlylocatorchangeand explicitidentity→Renameevent, no contentrevision.
SourceI !=B andnoeditorchange→Update; manualoverride or Mchanged→Conflict conservative.
Partialscan cannotproduceMissing; completedeclaredscope mayrecordMissing withoutdeletion.
Changedclassification→Blocked. Existingopenconflicts return conflict_pending, no newoverwrite.
Tests enumerate eachdecisionand demonstrate inabilityofclient tosubmit comparisonflags.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A32/A36: no newcanonicalrev forunchanged/rename; Missing neverpurges.

Все связанные cases: A32, A36. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T05.2. Конфликт B/I/M и явное разрешение

**Files:**
- `migrations/0198_source_sync_conflicts.sql`
- `crates/vestrace-application/src/source/sync.rs`
- `crates/vestrace-http/src/api/source_conflicts.rs`
- `apps/console/src/routes/SourceConflictPage.tsx`
- `crates/vestrace-infrastructure/tests/source_sync_races.rs`

**Requirements:** MW-R28, MW-R29.

**Interfaces:**

Proposed SourceSyncService::resolve(ctx, conflict_id, ResolveConflictRequest,key)->MutationReceipt.
Closed decisions accept_source/keep_manual/merge; allsame keyrulesandexpectedconflict+memory+incomingrefs.
ConflictDetail supplies exactreadableB/I/M. Source data nevermodified byeditor; oneopenconflict/source uniqueguard.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A33 Manual conflict
  Дано B импортирован, редактор создал M, upstream прислал I != B
  Тогда M остаётся current; open conflict exact B/I/M
  И тест отличает ошибку: Потерять M под видом sync
Сценарий: A34 Three resolutions
  Дано На отдельных fixtures accept_source/keep_manual/merge
  Тогда I применён и override снят / M сохранён с override / merged новаяrevision; везде audit+receipt
  И тест отличает ошибку: Изменить исходный I при keep_manual
Сценарий: A35 Stale conflict
  Дано После просмотра конфликта M изменён ещё раз
  Тогда Resolve409 без потери новой manualrevision; новые bases
  И тест отличает ошибку: Применить resolution к невиданному current
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test source_sync_races --locked -- --test-threads=1
npm --prefix apps/console run test:e2e:memory
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Lockcollection/source/conflict/memory inapprovedglobalorder; freshreauth eachbasis.
Checkimmutableconflicttriple/currentrevision/state, no silent rebase.
accept_source→newrevisionI clearoverride; keep_manual→preserveM persistoverride; merge→newusertextrevision override.
Record resolutionEvent/sourcebinding/version/receipt/audit in sharedUoW; closeconflictonlyifallcommit.
UpdateUI stalehandlerretainsdraftandrequestsnewbases; no modelmergepath.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A33–A35: threebranches andstaleresolution proven, Mneverlost.

Все связанные cases: A33, A34, A35. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T05.3. Две стороны cancel/apply и сериализация коллекции

**Files:**
- `crates/vestrace-application/src/source/import.rs`
- `crates/vestrace-application/src/source/import_handler.rs`
- `crates/vestrace-infrastructure/src/postgres/source_repository.rs`
- `crates/vestrace-infrastructure/tests/source_sync_races.rs`

**Requirements:** MW-R31, MW-R32.

**Interfaces:**

Existing proposed cancel(ctx,operation_id,CancelRequest,key)->ImportOperation, collection active_import_id serializes onlybatchadmission.
Itemstate/completion lock fence prevents post-cancel commit. Cleanup is consumer of existing operation/materialretention, not free-running new scheduler.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A37 Batch collision
  Дано Два Apply к одной collection и edit в середине items
  Тогда Один active batch; edit учитывается per-item CAS и conflict
  И тест отличает ошибку: Две независимые очереди overwrites
Сценарий: A38 Cancel race
  Дано Item A commit; cancel winner before ItemB commit
  Тогда A остаётся; B не applied; outcome partial/cancelled явно
  И тест отличает ошибку: Компенсация A как будто никогда не было
Сценарий: A39 Expiry race
  Дано Preview истекает одновременно с Apply
  Тогда Под collection/op lock ровно один legal winner; не поздний cleanup Live payload
  И тест отличает ошибку: Удалить payload уже применённого source
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test source_sync_races --locked -- --test-threads=1
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Collectionacceptance preventssecondactiveapply, individualmemoryedits stillallowed.
Beforeitemcommit recheckoperationnotcancelled andbases. Cancel lockconsistentwithitemcommit.
Ifitemcommittedfirst retainit; cancelremaining→skipped. Cancelwinner preventsitemmutation.
Expiry onlypreview_ready/staging; apply acceptedbeforeexpiry owns itsmaterial untillegalretirement.
Closeactive_import_id onlydurableterminaloutcome. Replaycancel same receipt; no inferreddeath bytimeout.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A37–A39: bothlockorders andboundedcrashes, no phantomrollback orretiredLivepayload.

Все связанные cases: A37, A38, A39. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.

## Уточнение T05.2: current vs detection revision

ConflictDetail содержит immutable original_manual_revision и отдельно актуальный manual_revision. Resolve проверяет current pointer против expected_memory_revision_id/state, но не требует, чтобы current всё ещё был M0. Именно это позволяет законно разрешить конфликт после дополнительного редактирования. UI показывает изменение M0→M. Добавить в A35 второй запрос после обновления detail: он должен успешно разрешить конфликт, оставив исходный detection evidence неизменным.

Обновление manual_override при обычной correction входит в shared writer `crates/vestrace-application/src/memory/write.rs` / `crates/vestrace-infrastructure/src/postgres/memory_mutation_repository.rs` этого пакета. Добавить оба существующих после MW-02 файла в exact MW-05 scope до изменения.
