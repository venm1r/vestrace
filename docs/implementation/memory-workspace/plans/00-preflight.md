# MW-00. Baseline, authority и исполнимые gates — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Baseline, authority и исполнимые gates.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** принятие документационного проекта.

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

- `crates/vestrace-infrastructure/src/postgres/pool.rs` — Изучить existing permit/UoW; менять только при доказанной необходимости [existing_referenced_recheck_before_edit].

## T00.1. Установить свежую архитектурную и файловую границу

**Files:**
- `file-plan.json`
- `source-manifest.json`
- `00-baseline.md`

**Requirements:** MW-R01.

**Interfaces:**

Вход: remote main, этот source-manifest и нынешние accepted specs. Выход: новый preflight только MW, согласованные exact пути и delta каждой затронутой authority. Это документационный результат, не новый runtime API.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A01 Сверка revision
  Дано main отличается от pinned SHA либо найден одноимённый module/migration
  Тогда Не начинать запись; обновить baseline/gap/scope явным diff
  И тест отличает ошибку: Продолжить по старому scope
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
git status --porcelain=v1 -z --untracked-files=all
git rev-parse HEAD
git diff 6f6102536e9a535b7086db14573bf45fe750ad71 --stat
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Получить HEAD и diff к 6f610253; перечитать затронутые memory/retrieval/material/pool/worker/Console.
Проверить git tracked files, SQL names, API inventory и pending branches на collisions.
Проверить, что 0195/14E не заняты MW. Уточнить candidate0196–0199 целиком.
Зафиксировать прочитанные vs недоступные sources, scope и требуемые amendments.
Не изменять PLAN.md/P04 preflights/frozen spec для удовлетворения локального verifier.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

Команды только читают checkout. Неизвестные dirty files сохраняются; main drift сначала рассмотрен и записан.

Все связанные cases: A01. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T00.2. Установить честный verification entrypoint

**Files:**
- `apps/console/package.json`
- `.github/workflows/ci.yml`

**Requirements:** MW-R42.

**Interfaces:**

Вход: существующие toolchain/scripts; выход: матрица реальных test commands, PostgreSQL roles и необходимых dev-only зависимостей. Источник норм Cargo — E01. Браузерная инфраструктура реализуется T03.3.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A51 Docs tests
  Дано В type doc compile_fail, в CI alltargets отдельным шагом
  Тогда Отдельный --doc реально run; запущенные scripts существуют
  И тест отличает ошибку: Засчитать alltargetsкакdoctests
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
cargo test --workspace --doc --all-features --locked
npm --prefix apps/console run typecheck
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Проверить доступность configured test PostgreSQL/runtime login и clean disposable namespace.
Записать, какие current suites реально удалось запустить; не менять результаты старого evidence.
Проверить doctest invocation отдельно. Не запускать отсутствующий npm test.
Прочитать existing auth flow и утвердить supported labels/material read/counter boundaries.
Если среда недоступна, записать BLOCKED; не присваивать PASS из предыдущих коммитов.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

Только наблюдённый exit становится evidence. Unavailable DB/browser остаются blockers, не тестовыми doubles.

Все связанные cases: A51. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.
