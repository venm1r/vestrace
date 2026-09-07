# MW-07. Qualification полного цикла — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Qualification полного цикла.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-01, MW-02, MW-03, MW-04, MW-05, MW-06.

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

- `docker/postgres/init-runtime-role.sh` — Exact guarded ownership/execute allowlists [existing_referenced_recheck_before_edit].
- `.github/workflows/ci.yml` — Proposed memory CI coverage including docs and browser gates [existing_referenced_recheck_before_edit].
- `tests/memory_workspace_acceptance.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `crates/vestrace-infrastructure/tests/memory_workspace_upgrade.rs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `scripts/memory-workspace-acceptance.sh` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].

## T07.1. Fresh и populated-upgrade authority квалификация

**Files:**
- `crates/vestrace-infrastructure/tests/memory_workspace_upgrade.rs`
- `docker/postgres/init-runtime-role.sh`
- `tests/memory_workspace_acceptance.rs`

**Requirements:** MW-R39.

**Interfaces:**

Consumes migrationsMWM01–M04 andexistingprovisioner/schema/ordinarymaterialrequirements.
Produces qualification observations for restricted runtime, oldwirecompat, newrowconstraints, populatedupgrade; no new datarepairauthority.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A47 Upgrade
  Дано БД pinnedbaseline содержит memories/sources старогоформата; применитьcandidate migrations
  Тогда Старая история/metadata доступна; unknown provenance не сфабрикована; narrowrole работает
  И тест отличает ошибку: Проверить только fresh empty DB
Сценарий: A48 Direct SQL
  Дано Runtimerole делает прямые writes всех новых guardedтаблиц
  Тогда Запрет по проверенному privilege/constraint; lawful commands проходят
  И тест отличает ошибку: Fixture суперпользователь считается runtimeproof
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test -p vestrace-infrastructure --test memory_workspace_upgrade --locked -- --test-threads=1
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
BuildfreshdisposableDBandbaselinepopulatedDB usingrealoldinterfaces.
Applyforwardmigrations/provisionexactroles; compareexistingdataintegrity and legacyreceipts.
TryrawDML onexplicitallnewguardedtablesandforeignworkspacereferences; verifyrefusalmechanism.
Runordinarymaterialrecovery/erasurewithnewsource/exportrefs, nocascadecrosssourceleak.
Compareappliedmigrationchecksums and protocol locks unchanged exceptapprovednewpaths.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A47/A48: freshandupgrade bothlimitedroleproof. Failurecannot bepatchedby superuser testfixture.

Все связанные cases: A47, A48. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T07.2. Полный пользовательский цикл и final review

**Files:**
- `tests/memory_workspace_acceptance.rs`
- `scripts/memory-workspace-acceptance.sh`
- `.github/workflows/ci.yml`

**Requirements:** MW-R40, MW-R41, MW-R43, MW-R44, MW-R45.

**Interfaces:**

Consumesactualbuiltbinary/HTTPworker/Console, registeredtopics, scopedinputfixtures andfullmatrixA01–A54.
Produces versioned evidence manifest withsource/build/environment, exact tests andknownlimitations. Never setsprojectTRUSTED solelyfromMWcompletion.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A49 Golden end-to-end
  Дано Upload→find→edit→syncconflict→resolve→export→newtarget
  Тогда Штатные binary/worker/browser; no admin data seeding except installbootstrap
  И тест отличает ошибку: Фикстура напрямую вставила outcome
Сценарий: A50 Contract parity
  Дано ServedOpenAPI/typedclient/actualJSON сверяются на каждый newroute
  Тогда Все fields/statuses одинаковы;старый metadata-контракт не изменён
  И тест отличает ошибку: Проверить только статический файл schemas/openapi-v1.json
Сценарий: A52 Readiness
  Дано Canonical sourceapplied, generation notready
  Тогда Показывается pending/blocked и typedretrievalerror
  И тест отличает ошибку: Считать imported=searchready
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
bash scripts/memory-workspace-acceptance.sh
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console run test:e2e:memory
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Runupload→read→edit→syncconflict→resolve→export→newtarget withouttestonlyproducers.
Crashactualchildafterbusinesscommitbeforeack andduringlinking; observepersistedoutcomeswithfreshreader.
CrossverifyservedOpenAPI/JSON/SDK fields; separatedoctests/browserruntime checks.
Runmutationsreceipt/CAS/workspace/batchatomicity/override/downloadpolicy, restoreandGREEN.
Finalreviewfromindependentcontext reads wholecontractandcurrenttree; unresolvedgeneration/migration/materialblockslisted.
Do not claim v1.0 release/P04close. Only approvedsupportedscenarioqualified.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A49–A54, exactrelevantfullsuites. No inferenceofPASSfromdocs/typecheck alone.

Все связанные cases: A49, A50, A52, A53, A54. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.
