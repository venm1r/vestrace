# MW-06. Portable export/import — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Portable export/import.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-04, MW-05.

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
- `apps/console/src/routes/MemoryExportPage.tsx` — Bounded export intent/status/download [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/import_handler.rs` — Idempotent memory.source_import.apply handler [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/export.rs` — Pinned export selection, prepare handler, download check [proposed_new_check_name_collisions].
- `crates/vestrace-application/src/source/portable.rs` — Closed portable schema and foreign/local mapping [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/src/postgres/memory_export_repository.rs` — Pinned selection, export operation and result ref [proposed_new_check_name_collisions].
- `crates/vestrace-http/src/api/memory_exports.rs` — Export prepare/status/content routes [proposed_new_check_name_collisions].
- `crates/vestrace-cli/src/commands/sources.rs` — HTTP client CLI scan/preview/apply/status/export [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/memory_export_authority.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-application/tests/portable_contract.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-cli/tests/memory_portability.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `apps/console/e2e/memory-export.spec.ts` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `migrations/0199_memory_export_authority.sql` — PROPOSED number after reserved 0195; MW-00 reallocates atomically if occupied. Pinned selection, ordinary result ownership, portable mapping [candidate_number_not_reserved].

## T06.1. Защищённый export snapshot и скачивание

**Files:**
- `crates/vestrace-application/src/source/export.rs`
- `crates/vestrace-infrastructure/src/postgres/memory_export_repository.rs`
- `crates/vestrace-http/src/api/memory_exports.rs`
- `migrations/0199_memory_export_authority.sql`
- `crates/vestrace-infrastructure/tests/memory_export_authority.rs`

**Requirements:** MW-R33, MW-R34.

**Interfaces:**

Proposed MemoryExportService::prepare(ctx,ExportRequest,key)->ExportOperation;
MemoryExportHandler topic="memory.export.prepare" usesexistingOutboxDispatcher;
MemoryExportService::download(ctx,id)->bounded JSON/Markdown response onlyafter freshreadandexportpolicy.
PinnedselectionIDsandmaterialresult persisted; DTOserialization fromPortablePackage whitelist.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A40 Export scope
  Дано Selected2 memories + denied third; request включает только2
  Тогда В package только2 и разрешённые sources, ни tokens/grants/vectors
  И тест отличает ошибку: Сериализовать целиком внутренний domain object
Сценарий: A41 Revoke download
  Дано ExportReady; затем право на одну pinnedrevision отозвано
  Тогда Весь download403/blocked, результат retired; старый URL не обходит
  И тест отличает ошибку: Отдать cached JSON после ACL change
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test memory_export_authority --locked -- --test-threads=1
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Chooseauthorizedboundedselection withimmutableversions and safeoriginclosure.
Persistoperation+selection+audit/outbox atomic; render with ordinarymaterialstagingoutsideDB.
RecordReady only exactcompleteprotectedresult; excludeinternaldomainfields.
Atdownload freshcheckeverypinnedsource, revokeany=>blockedwholeoutput andretirematerial.
Expiry usesexistingretention. No genericpublicURL/CDN/staticdownloadpath.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A40/A41: no authorityleaks anddownloadafterrevoke refused.

Все связанные cases: A40, A41. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T06.2. Portable parser, mappings и повторяемая linking фаза

**Files:**
- `crates/vestrace-application/src/source/portable.rs`
- `crates/vestrace-application/src/source/import_handler.rs`
- `crates/vestrace-application/tests/portable_contract.rs`
- `crates/vestrace-cli/tests/memory_portability.rs`

**Requirements:** MW-R35, MW-R36, MW-R37.

**Interfaces:**

Proposed validate_portable(bytes:&[u8])->PortablePackage (closed schema + semanticlimits+refs);
PortableImportMapping stores foreign-to-localnamespace underoperation, not directlocaltrust.
Existing ImportPreviewRequest mode=portable; samehandler statephase entities→links→complete, no additionalunhandledtopic.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A42 Portable malformed
  Дано Futureversion,unknownauthorityfields,danglinglink
  Тогда Отказ до canonical apply; schema/semantic причины различимы
  И тест отличает ошибку: Игнорировать чужие permissions и всё же импортировать частично без отчёта
Сценарий: A43 Portable replay
  Дано Одна operation дважды доставлена разнымиworkers
  Тогда Те же local mappings, content/history/link counts без дублей
  И тест отличает ошибку: Повторно выделить localIDs
Сценарий: A44 Foreign trust
  Дано Imported source declared actor admin/time in past/classification public
  Тогда Local actor фактический importer; time local now, origin annotation; targetpolicy заново
  И тест отличает ошибку: Присвоить localadmin из package
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-application --test portable_contract --locked
cargo test -p vestrace-cli --test memory_portability --locked -- --test-threads=1
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Parseclosedversion; bounddepthbytesnodecount; validatedsource/memory/revisionreferencesandselection.
Allocate mappingsonceunderoperation; importeachentity/revisions withactualimportactor/currenttime plusoriginannotations.
Policy determines targetclassification; importedgrant/confirmedflag rejected, no ACLcopy.
Afterentitycommit, sametopichandler idempotentlinkingusesexistingmappings; completenesspartial untilclosure.
Sameoperationreplay finds mappings; newexplicitimportduplicatewarning, not overwriteoriginal.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A42–A45: unknownschema/invalidrefs fail early; restartlinkphase converges; newlocalIDs noauthority.

Все связанные cases: A42, A43, A44, A45. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T06.3. UI/CLI переносимости и human-readable Markdown

**Files:**
- `apps/console/src/routes/MemoryExportPage.tsx`
- `crates/vestrace-cli/src/commands/sources.rs`
- `apps/console/e2e/memory-export.spec.ts`

**Requirements:** MW-R38.

**Interfaces:**

Proposed CLI sources export submitsExportRequest,pollsExportOperation,downloadsprotectedcontent; usesexistingHTTPtransport.
UI selection/history/format/reason explicit, followsstatusandexpiry. Markdown formatter is deterministicreadrepresentation, notbackupformat.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A46 Markdown export
  Дано Content содержит delimiters/заголовки/ссылки
  Тогда Читаемый escaped document с incomplete notice; не заявляется fullroundtrip
  И тест отличает ошибку: Markdown metadata становится исполняемым кодом
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-cli --test memory_portability --locked
npm --prefix apps/console run test:e2e:memory
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Constructexportintent; create stablekeyforlogicalattempt; pollboundedwithoutnewoperation.
Fetchdownloadonlyexplicituseraction, no embeddedtokens or plaintextlocalcache.
WriteCLIoutput only userselectedsafe destination refusingoverwritebydefault;permissionsplatformtested.
EscapeMarkupdelimiters, indicatepartialhistory/provenanceandforeignannotation.
Warnexportoutside servercannotberevoked; fileisnotfullbackup norlicenseforprivileges.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A46 plusdownloadflow: exactselecteddata, noautoexport, expirytruthful.

Все связанные cases: A46. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.
