# MW-03. Console memory workspace — Implementation Plan

> **Для agentic исполнителя:** выполнять последовательно с RED/GREEN и независимой проверкой контракта; использовать принятый в проекте persistent builder/reviewer workflow. Чекбоксы не отмечены: реализация в этой работе не выполнялась.

**Goal:** Console memory workspace.
**Architecture:** существующие memory/material/mutation/outbox authorities; нет нового event store, scheduler или клиента с прямым SQL.
**Tech Stack:** текущие Rust workspace/PostgreSQL/HTTP и React/TypeScript Console; dependency updates только отдельным утверждённым diff.
**Spec:** [проект](../01-design.md), [транзакции](../02-data-and-transactions.md), [API](../03-api.md), [приёмка](../09-acceptance.md).
**Dependencies:** MW-01, MW-02.

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

- `apps/console/src/memory/MemoryConsole.tsx` — Переиспользуемое представление memory detail [existing_inspected].
- `apps/console/src/sdk/client.ts` — Существующий HTTP transport/error/auth, re-export memory client [existing_inspected].
- `apps/console/src/main.tsx` — Подключить memory/source/import/conflict/export routes [existing_inspected].
- `apps/console/package.json` — Явные scripts и согласованные dev-only зависимости [existing_inspected].
- `apps/console/src/shell/AppLayout.tsx` — Navigation under existing shell, not new app [existing_referenced_recheck_before_edit].
- `apps/console/package-lock.json` — Dev-dependency exact pin only after approved diff [existing_referenced_recheck_before_edit].
- `apps/console/src/sdk/memoryClient.ts` — Typed memory API wrapper over existing transport [proposed_new_check_name_collisions].
- `apps/console/src/memory/editorModel.ts` — Pure editor state transitions; unknown/conflict safety [proposed_new_check_name_collisions].
- `apps/console/src/routes/MemoryPage.tsx` — Browse/detail/history/editor routes [proposed_new_check_name_collisions].
- `apps/console/tests/memoryEditor.test.mjs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `apps/console/tests/memoryClient.test.mjs` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `apps/console/e2e/memory.spec.ts` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].
- `apps/console/playwright.config.ts` — Planned verification target, not an existing passed suite [proposed_new_check_name_collisions].

## T03.1. Типизированный клиент и маршруты библиотеки

**Files:**
- `apps/console/src/sdk/memoryClient.ts`
- `apps/console/src/sdk/client.ts`
- `apps/console/src/main.tsx`
- `apps/console/src/routes/MemoryPage.tsx`
- `apps/console/tests/memoryClient.test.mjs`

**Requirements:** MW-R13.

**Interfaces:**

Proposed MemoryClient methods: list(BrowseRequest),detail(memoryId),history(memoryId,cursor),correct(memoryId,CorrectionRequest,key),context(ContextRequest). Types from contracts; existing transport owns auth/error parsing.
Produces fetchable /memory routes and read/detail/history UI states. T03.1 creates test:memory and exact Node/TS test compilation script before its first run; T03.3 adds the browser runner.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A17 SDK error
  Дано Сервер отдаёт 503 unavailable, не []
  Тогда UI unavailable; стандартный request_id доступен
  И тест отличает ошибку: Empty библиотека вместо ошибки
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
npm --prefix apps/console run typecheck
npm --prefix apps/console run test:memory
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Reusecurrenttransport, no newbrowser bearerstorage.
ImplementclosedDTO mapping and request abort on scopechange.
Differentiate browse vsquery, empty vs503 and404vs403 safeerrors.
Mount existingdesigncomponents; no hardcodedmockdatainsuccess UI.
Check actualservedJSONfields in HTTP integration.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A17: new script added under MW03, error not shown as empty, contractfields match.

Все связанные cases: A17. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T03.2. Редактор и источник конфликтующего черновика

**Files:**
- `apps/console/src/memory/editorModel.ts`
- `apps/console/src/memory/MemoryConsole.tsx`
- `apps/console/src/routes/MemoryPage.tsx`
- `apps/console/tests/memoryEditor.test.mjs`

**Requirements:** MW-R14, MW-R15.

**Interfaces:**

Proposed EditorState union: viewing | editing(base,draft) | submitting(base,draft,key,request) | conflict(base,current,draft) | result_unknown(base,draft,key,request) | failed(base,draft,error).
reduceEditor(state,event) pure function; browser memory only. No draft persistentcache.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A18 Unknown draft
  Дано POST timeout после отправки; пользователь жмёт «проверить/повторить»
  Тогда Тот же key/body, draft сохранён; не новый save
  И тест отличает ошибку: Автоматически создать новый UUIDkey
Сценарий: A19 Scope switching
  Дано W1 request медленный, пользователь переключается на W2
  Тогда Late W1 response отброшен, caches/draft W1 очищены
  И тест отличает ошибку: Показать W1 текст внутри W2
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
npm --prefix apps/console run test:memory
npm --prefix apps/console run typecheck
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Savecaptures exactbodyandUUIDkey once; repeatsubmituses same body/key.
409 retainsbase/current/draft; useraccepts newbase then newkeyexplicitintent.
Timeout→result_unknown, not failed-with-nochange. Restore actionreasonrequired.
Scopechange/logoutcancelpending andclearplaintextstate.
can_correct is hint, server rechecks. readonlyclassification renderedwithreason.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A18/A19: replay stable and lateworkspace response ignored.

Все связанные cases: A18, A19. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## T03.3. Реальный browser gate и безопасное отображение

**Files:**
- `apps/console/package.json`
- `apps/console/package-lock.json`
- `apps/console/playwright.config.ts`
- `apps/console/e2e/memory.spec.ts`

**Requirements:** MW-R16.

**Interfaces:**

Proposed scripts test:memory and test:e2e:memory. Test-only Playwright exactversion chosen and lock verified atimplementation scope approval; no unsupported claim about existing installedbrowser.
E2E targets actual prepared HTTPserver, test data via endpoints. Env addresses and credentials not embedded in tests/screenshots.

- [ ] **1. Написать/подготовить отрицательный сценарий до изменения поведения.**

Создать указанный test target в пределах разрешённого scope, довести setup до штатно запускаемого процесса и записать exact baseline. Основной контракт теста:

```gherkin
Сценарий: A20 Hostile rendering
  Дано Markdown содержит script/img/javascripts URL и чужой tracking pixel
  Тогда Text view не исполняет и не загружает remote ресурс
  И тест отличает ошибку: dangerouslySetInnerHTML
Сценарий: A21 Keyboard
  Дано Открыть editor/restore/conflict только клавиатурой
  Тогда Focus/label/error/status доступны, после dialog focus возвращается
  И тест отличает ошибку: Недоступная кнопка без причины или потерянный focus
```

- [ ] **2. Запустить RED и проверить причину.**

```bash
npm --prefix apps/console run test:memory
npm --prefix apps/console run test:e2e:memory
npm --prefix apps/console run build
```

Для нового поведения ожидается assertion mismatch на отсутствующей гарантии. Compilation/setup failure сначала устраняется без реализации требуемого поведения. Для MW-00 проверки наблюдают состояние и не требуют искусственного красного прогона.

- [ ] **3. Реализовать минимальный объявленный путь.**

Алгоритм (проектная последовательность, не готовый исполняемый source):

```text
Reuse pureTS test build installed by T03.1; add only the browser runner and fixture in this task.
Browserfixtures perform legitimateinstallbootstrap then POSToperations.
Test keyboardeditor, restore, twousersCAS, unavailable, XSStext and scopelatefetch.
Screenshots/logs sanitized; no mock server qualifies backend.
Passonlyif realresponse observed, typechecknotbrowserproof.
```

- [ ] **4. Повторить команду GREEN и все привязанные acceptance cases.**

A20/A21: no hostileexecution/networkimages, accessible editorflow; browseractualrun recorded.

Все связанные cases: A20, A21. Не ограничиваться первыми примерами выше; полный контракт находится в traceability.

- [ ] **5. Проверить чувствительность и совместимость.**

Для data/security task временно нарушить именованный guard/receipt/CAS и убедиться в behavioral RED неизменённого теста; затем byte-exact restore и GREEN. Для UI — controlled regression в reducer или stale-response fence. Проверить legacy endpoints и build/typecheck затронутого crate/Console. Документационный MW-00 mutation не требует.

- [ ] **6. Review, evidence и scoped сохранение.**

Независимый reviewer читает требования, реализации и фактический diff. Записать команды/выводы/границы доказанного; сверить protected paths и git diff --check. Commit выполняется только при отдельном разрешении и перечисленными файлами, не git add -A. Push/deploy не разрешены этим пакетом.

## Exit gate

Нет нерассмотренных BLOCKER/MAJOR; полные связанные scenarios наблюдены; данные/API/SDK/docs согласованы; scope и source versions записаны. Если зависимость не готова, фиксируется частичный результат, но package completion не выводится из одних checkboxes.

## Exact T03.1 test-runner additions

Create `apps/console/tsconfig.memory-tests.json` and `apps/console/scripts/run-memory-tests.mjs` in the same approved scope as package.json. The script compiles pure editor/client helpers (without React DOM mounting) to an isolated temporary build directory, runs `node --test` on named memory tests and removes this build output. It must preserve a nonzero compile or test exit code. Do not rely on Node supporting TypeScript features outside the configured transpiler. `test:memory` calls this script. No production dependency is added for these pure tests.
