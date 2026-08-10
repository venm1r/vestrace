# Vestrace Architecture Contract v0.2

**Статус:** normative architecture baseline  
**Дата:** 2026-08-10  
**Ветка:** `docs/architecture-v0.2`  
**Репозиторий:** `venm1r/vestrace`  
**Назначение:** зафиксировать целевую архитектуру Vestrace до начала следующего этапа реализации.

> Этот документ является архитектурным контрактом, а не утверждением о текущем implementation status. Наличие сущности, инварианта или workflow в этом документе не означает, что он уже реализован или доступен через HTTP, MCP, CLI либо worker runtime.

## 1. Product definition

Каноническое определение:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

Каноническая формула:

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

Vestrace хранит долговременное когнитивное состояние, provenance, временную историю, execution evidence и управляемые cognitive assets так, чтобы разные агенты и разные исполнения могли безопасно продолжать работу на общей доказуемой основе.

Vestrace не является только vector database, prompt-memory библиотекой, workflow engine, agent framework или event store. Эти механизмы могут существовать внутри системы, но не определяют продукт.

## 2. Normative language

В документации Vestrace используются нормативные термины:

- **MUST / MUST NOT** — обязательное требование;
- **SHOULD / SHOULD NOT** — требование, от которого допустимо отступить только по документированной причине;
- **MAY** — допустимая возможность.

Специализированные документы могут уточнять этот контракт, но MUST NOT ослаблять его без отдельного ADR или новой версии Architecture Contract.

## 3. Global architecture laws

### 3.1 Authority before projection

Vestrace MUST различать authoritative/canonical state и derived state.

```text
authoritative source
        ↓
canonical state
        ↓
derived state
        ↓
indexes / caches / projections
```

Derived state MUST быть удаляемым и перестраиваемым из более авторитетного слоя. Менее авторитетный слой MUST NOT автоматически переписывать более авторитетный только для восстановления локальной согласованности.

### 3.2 No competing runtimes

Vestrace MUST иметь единый execution boundary. Repair, recovery, governance operations, external effects и qualification workflows MUST использовать существующие execution, policy, capability, audit и persistence contracts.

Запрещено создавать без отдельного архитектурного пересмотра:

- второй orchestration runtime;
- второй event store;
- параллельный generic `Task → Action → Attempt` runtime;
- отдельный Repair Runtime;
- отдельный Incident Runtime;
- отдельный Governance Runtime.

`Vestrace State Engine` — внутреннее название durable state/execution boundary, а не отдельный продукт или сервис.

### 3.3 History is corrected, not rewritten

После фиксации значимые domain facts, revisions, execution facts, audit facts и external-effect receipts MUST NOT редактироваться так, будто исходное событие не происходило.

Исправление выполняется новой revision, supersession, compensation, reconciliation или recovery fact.

### 3.4 Evidence before trust

Состояния `HEALTHY`, `RESOLVED`, `CONFIRMED`, `TRUSTED` и qualification `PASSED` MUST возникать из проверяемого evidence, а не только из отсутствия ошибки или административного флага.

### 3.5 Fail closed on ambiguity

Неопределённость MUST быть first-class состоянием. Vestrace MUST NOT автоматически превращать `UNKNOWN` в `FAILED`, `HEALTHY`, `TRUSTED` или permission to retry.

---

# Block 1 — Persistent Cognition Core

## 1.1 Cognitive substrate

Persistent cognition строится вокруг доказуемого долговременного состояния, а не вокруг накопления текстовых фрагментов.

Когнитивный слой MUST различать как минимум:

- source evidence / originating facts;
- persistent memory identity и immutable revisions;
- semantic claims/structured assertions там, где требуется явное утверждение;
- provenance и derivation;
- conflicts и supersession;
- derived summaries, retrieval representations и projections.

Специализированный Domain Model v0.2 определяет точные агрегаты и relation cardinality.

## 1.2 Stable identity, immutable revisions

Persistent object SHOULD иметь стабильную identity, а изменение содержимого MUST создавать новую immutable revision.

Старое состояние сохраняется как history и не исчезает из-за появления новой актуальной revision.

## 1.3 Provenance

Каждый persistent cognitive вывод, полученный не непосредственно от человека/источника, MUST иметь derivation/provenance до исходных inputs и метода получения.

Provenance SHOULD позволять установить:

- источник;
- автора/actor;
- время;
- execution/model/tool reference;
- transformation/derivation method;
- policy/version context, когда он влияет на результат.

## 1.4 Cognitive assets are not ordinary memories

Agents, skills, workflows, model/provider profiles и другие versioned cognitive definitions MUST оставаться отдельными governed assets. Они MAY использовать memory/evidence, но MUST NOT маскироваться под обычные воспоминания.

## 1.5 Derived cognition

Summaries, embeddings, search documents, retrieval projections, caches и агрегированные metrics являются derived state и MUST быть rebuildable.

---

# Block 2 — Temporal & Concurrency

## 2.1 Multiple time dimensions

Vestrace MUST различать время факта и время его фиксации.

Минимально поддерживаются:

- `occurred_at` — когда факт предположительно произошёл;
- `recorded_at` — когда Vestrace авторитетно зафиксировал факт;
- validity interval (`valid_from` / `valid_until`) для знаний, меняющихся во времени.

Отсутствующий или ненадёжный `occurred_at` MUST NOT блокировать фиксацию факта, если `recorded_at` известен.

## 2.2 Temporal queries

Когнитивный retrieval MUST поддерживать различие между:

- current state;
- state `as_of`;
- timeline;
- all history.

Superseded/expired knowledge MUST NOT выдаваться как current knowledge.

## 2.3 Optimistic concurrency

Canonical mutations MUST использовать version/revision preconditions. Потерянное обновление через silent last-write-wins запрещено.

При несовпадении expected state/revision операция должна завершаться conflict/stale result и MUST NOT автоматически перетирать новое состояние.

## 2.4 Lease does not equal authority

Operational lease/heartbeat MAY определять временного исполнителя работы, но MUST NOT сам по себе менять logical version, ownership authority или capability.

## 2.5 Ordering

Canonical append-only streams MUST иметь проверяемый порядок внутри своего агрегата/scope. Системе запрещено выводить глобальный причинный порядок только из wall-clock timestamp, если такой порядок не доказан.

---

# Block 3 — Mutation & Reconciliation

## 3.1 Mutation model

Значимое изменение persistent cognition MUST быть представлено как явная mutation с:

- actor;
- target;
- expected version/state;
- reason/intent;
- provenance;
- resulting revision/fact.

## 3.2 No hidden overwrite

Correction создаёт новую revision или новое утверждение. Supersession сохраняет старое знание как history. Delete/expiry не должны переписывать прошлое.

## 3.3 Conflict is explicit

Если два допустимых источника противоречат друг другу и детерминированного правила выбора нет, Vestrace MUST представлять conflict явно.

Не допускаются универсальные эвристики вроде `latest timestamp wins`, если они не являются политикой конкретного домена и не имеют достаточного evidence.

## 3.4 Reconciliation classes

Reconciliation SHOULD различать:

- deterministic reconciliation — результат однозначно выводится из более авторитетного состояния;
- policy-guided reconciliation — разрешается явной versioned policy;
- semantic reconciliation — требует оценки смысла/evidence;
- human-required reconciliation — автоматическое решение недопустимо.

Semantic ambiguity MUST NOT маскироваться под deterministic repair.

## 3.5 State Engine boundary

State Engine использует существующие authoritative aggregates, application ports и execution runtime. Он MUST NOT вводить параллельный execution hierarchy, если семантика уже принадлежит Run/Step/model/tool/remote/human/verification contracts.

---

# Block 4 — Retrieval / ContextPack 2.0

## 4.1 Retrieval objective

Retrieval выбирает не «самый похожий текст», а минимальный набор актуальных, разрешённых, доказуемых и полезных знаний для конкретной задачи.

## 4.2 Security before ranking

Workspace, capability, classification, sharing и policy filters MUST применяться до того, как запрещённый content станет доступен ranking/model stages.

## 4.3 Retrieval request

Retrieval contract SHOULD включать:

- query/task context;
- intent;
- workspace/actor;
- scopes;
- temporal perspective;
- filters;
- token budget;
- retrieval policy version;
- explanation requirements.

## 4.4 Hybrid retrieval

Candidate generation MAY использовать exact, full-text, vector, structured, graph и execution-history channels. Несопоставимые channel scores SHOULD объединяться rank-based fusion, а не произвольным суммированием.

Отказ одного derived channel MUST приводить к честному degraded mode, если остаётся безопасный deterministic retrieval path.

## 4.5 Current truth and conflicts

Current retrieval MUST исключать superseded/expired knowledge как актуальное. Неразрешённые conflicts MUST возвращаться как conflicts, а не как установленный факт.

## 4.6 ContextPack

`ContextPack` — governed, token-bounded, provenance-preserving representation контекста.

Он MUST:

- соблюдать hard token/content budget;
- сохранять source/revision references;
- объяснять inclusion;
- сохранять warnings/degradation;
- не повышать authority включённого знания;
- учитывать classification и model destination policy.

Допустимые representation levels включают `Full`, `Summary`, `Atomic`, `Reference`.

## 4.7 Cache validity

Context packs и retrieval caches являются derived state. Изменение canonical memory, policy, sharing state, classification или relevant generation MUST инвалидировать устаревшие representations.

---

# Block 5 — Execution Feedback & Learning

## 5.1 Evidence-backed feedback

Execution outcomes, deterministic checks, human feedback, downstream success, model/tool metrics и failure evidence MAY использоваться для обучения системы.

Каждый feedback signal MUST ссылаться на точные execution/model/tool/policy revisions, к которым он относится.

## 5.2 Authority of evaluators

Deterministic evidence и явно авторизованная human evaluation имеют приоритет над heuristic/model-judge signals.

LLM judge MAY быть дополнительным сигналом, но MUST NOT автоматически становиться авторитетной истиной.

## 5.3 Learning cannot silently self-modify authority

Learning pipeline MUST NOT автоматически менять permissions, capabilities, governance policies или security boundaries.

Изменение agents, skills, workflows, routing policies или persistent claims SHOULD проходить через versioned proposal/mutation lifecycle с provenance, verification и policy gates.

## 5.4 Raw evidence vs learned projection

Сырые execution/evaluation facts MUST сохраняться отдельно от производных выводов вроде performance memory, learned preference или routing recommendation.

Производный вывод MUST быть пересматриваемым без уничтожения исходных измерений.

---

# Block 6 — Capability Governance

## 6.1 Roles are templates; capabilities are authority

Roles используются как шаблоны выдачи прав. Фактическое runtime-разрешение определяется capability, а не названием роли.

Policy по умолчанию — deny.

## 6.2 Capability shape

Capability MUST быть ограничиваемой по:

- tool/operation;
- resource/scope;
- validity/expiry;
- budget/quota;
- risk ceiling;
- conditions/obligations.

Наличие capability identifier без успешной проверки ограничений не предоставляет authority.

## 6.3 Delegation / attenuation

Подагент получает только явно делегированное подмножество effective authority родителя и MAY получать дополнительные ограничения.

```text
child authority ⊆ parent effective authority
```

Delegation depth MUST быть bounded; безопасный default — один уровень, если policy не разрешает больше.

## 6.4 Risk model

Базовые категории риска:

- `low`;
- `medium`;
- `high`;
- `critical`.

Контекст MAY повышать effective risk. Понижение ниже intrinsic risk операции требует явного нормативного основания.

## 6.5 Policy decisions

Policy engine MAY:

- deny;
- allow;
- allow prepare-only;
- require approval;
- добавлять obligations/limits.

Approval MUST NOT преодолевать explicit deny, hard budget или capability ceiling.

## 6.6 Budgets

Governance SHOULD поддерживать typed budgets для времени, steps, model tokens/cost, tool effects, artifacts, subruns и других ограничиваемых ресурсов.

Reservation/accounting MUST быть auditable.

---

# Block 7 — Identity / Workspace / Federation

## 7.1 Workspace is an authority boundary

Каждый canonical object принадлежит определённому workspace/authority scope. Workspace isolation является безопасным default и SHOULD защищаться одновременно application policy и storage-level controls (например RLS).

`global` MUST NOT означать cross-workspace global.

## 7.2 Identity

User, agent, service account, workflow/system actor и remote/federated participant MUST иметь устойчивую typed identity. Actor identity и granted authority являются разными понятиями.

## 7.3 Cross-workspace sharing is explicit

Cross-workspace memory sharing MUST NOT расширять scope исходной Memory.

Канонический flow:

```text
source workspace
  ↓ MemoryShareGrant (exact target)
target workspace
  ↓ explicit acceptance
MemoryMount (read-only by default)
```

`MemoryMount` — разрешённое представление source memory, а не локальная authority и не новая source-of-truth копия.

## 7.4 Sharing constraints

Cross-workspace sharing MUST обеспечивать:

- exact source + target;
- отсутствие wildcard grants;
- отсутствие implicit transitive sharing;
- source-side и target-side policy checks;
- explicit lifecycle revoke/suspend/expire;
- namespaced shared references;
- provenance preservation;
- separate permissions для discover/read/context/model-use/derive/export.

Revoke прекращает будущий доступ, но MUST NOT переписывать уже зафиксированную историю disclosure.

## 7.5 Federation trust is not data permission

Признание remote identity/node/federation relationship MUST NOT автоматически разрешать передачу данных. Для каждого disclosure дополнительно требуются capability, data policy, classification compatibility и recipient constraints.

---

# Block 8 — Health / Integrity / Repair

## 8.1 Tiered Repair

Vestrace MAY автоматически исправлять только то, что детерминированно реконструируется из более авторитетного слоя.

Lifecycle:

```text
detect → diagnose → propose → repair → verify
```

Semantic, destructive, security-sensitive и ambiguous repair MUST проходить policy/capability/approval или human path.

## 8.2 Findings-first health model

`HealthFinding` — primary diagnostic unit. Aggregate health status является projection.

Finding MUST содержать evidence, scope, invariant, severity и repairability.

Severity:

`info / low / medium / high / critical`

Repairability:

`NONE / DETERMINISTIC / POLICY_GATED / HUMAN_REQUIRED`

Severity и repair risk MUST оцениваться отдельно.

## 8.3 Hybrid invariant registry

Invariant catalog хранит declarative metadata, а сложные проверки выполняются typed checkers.

```text
InvariantDefinition → Typed Checker → Evidence → HealthFinding
```

Integrity MUST NOT превращаться в универсальный DSL без необходимости.

## 8.4 Hybrid scheduling

Checks выполняются в трёх режимах:

1. inline guards для дешёвых критических invariants;
2. reactive checks после релевантных mutations/rebuild/import/recovery;
3. background sweeps для latent drift/corruption.

Не каждый invariant проверяется на каждом write path.

## 8.5 Repair Plan + transactional execution

Checker MUST NOT напрямую выполнять mutation.

```text
HealthFinding
  ↓
immutable RepairPlan
  ↓
Policy / Capability
  ↓
RepairExecution
  ↓
Verification
```

RepairPlan фиксирует scope, preconditions, operations, expected postconditions, risk, reversibility и required authority.

Reversibility:

`REVERSIBLE / REBUILDABLE / IRREVERSIBLE`

## 8.6 Verification pipeline

`RepairExecution = SUCCEEDED` не означает `HealthFinding = RESOLVED`.

Finding закрывает verifier после повторной проверки исходного invariant и необходимых collateral checks.

`UNKNOWN`/failed verification MUST оставлять или reopen finding.

## 8.7 Finding occurrence history

Одна logical проблема и её отдельные проявления разделяются на `HealthFinding` и `HealthOccurrence`.

Recurrence projection:

`ONE_OFF / RECURRENT / FLAPPING / PERSISTENT`

Auto-repair MUST иметь attempt budget/cooldown. Flapping MUST останавливать бесконечный repair loop.

## 8.8 Scope propagation

Health propagates только через объявленные dependencies. Локальный finding MUST NOT автоматически делать всю installation `CRITICAL`.

Aggregate states:

`HEALTHY / DEGRADED / UNHEALTHY / CRITICAL / UNKNOWN`

`UNKNOWN != HEALTHY`.

## 8.9 Suppression and accepted risk

Suppression/AcceptedRisk MUST NOT удалять finding или превращать raw integrity в healthy state. Они являются отдельными policy decisions с actor, reason, expiry и audit.

## 8.10 Repair safety

RepairPlan привязан к конкретному input state и preconditions. При изменении состояния он становится `STALE_PLAN` и MUST NOT исполняться.

Repair steps SHOULD быть idempotent, restartable и auditable. Для конфликтующих repair executions используются leases/fencing.

---

# Block 9 — External Effects

## 9.1 Intent before effect

Внешнее действие MUST иметь сохранённый immutable `ExternalEffectIntent` до dispatch.

```text
plan → intent → policy/capability → dispatch → receipt → reconciliation
```

## 9.2 Prepare / Authorize / Commit

External effect проходит `PREPARED → AUTHORIZED → COMMITTED`. Это не distributed ACID transaction; `COMMITTED` означает dispatch во внешнюю систему.

## 9.3 Delivery semantics

Adapter MUST объявлять semantics:

`AT_MOST_ONCE / AT_LEAST_ONCE / EFFECTIVELY_ONCE / UNKNOWN`

Vestrace MUST NOT обещать universal exactly-once.

## 9.4 Idempotency belongs to adapter contract

Retry policy определяется adapter idempotency/reconciliation contract, а не свободным решением агента.

## 9.5 Result is not outcome

Transport/API result и фактический external outcome разделены.

Состояния включают:

`NOT_STARTED / DISPATCHING / ACKNOWLEDGED / CONFIRMED / FAILED / UNKNOWN / RECONCILING`

Timeout MUST NOT автоматически означать, что effect не произошёл.

## 9.6 Reconciliation

`UNKNOWN` effect SHOULD быть reconciled через strongest available read-back, idempotency key, external resource id/hash/version или другой provider-specific evidence.

## 9.7 Reversibility

External effects классифицируются:

`REVERSIBLE / COMPENSATABLE / IRREVERSIBLE / UNKNOWN_REVERSIBILITY`

Compensation MUST NOT называться rollback и MUST сохранять связь с исходным effect.

## 9.8 Preconditions and TOCTOU

Перед dispatch критические preconditions проверяются повторно. Adapter SHOULD использовать strongest available conditional primitive: version, ETag, commit SHA, resource revision и т.п.

Изменённый approved intent MUST требовать новой authorization.

## 9.9 Effect budget and secrets

Capabilities MAY ограничивать count, rate, cost, risk и targets внешних действий. Secrets MUST разрешаться только на момент исполнения и MUST NOT сохраняться в traces/receipts как plaintext.

## 9.10 Receipt

Каждый dispatched effect SHOULD создавать immutable receipt/evidence record с provider, timestamps, external identifiers/version и outcome state.

Главный invariant:

```text
attempted ≠ acknowledged ≠ confirmed ≠ desired outcome
```

---

# Block 10 — Incident / Recovery / Revalidation

## 10.1 Incident model

`HealthFinding` — конкретный обнаруженный дефект. `Incident` — coordinated response context, который MAY объединять несколько findings/failures/unknown effects.

Incident lifecycle:

`OPEN → CONTAINING → CONTAINED → RECOVERING → REVALIDATING → RESOLVED → CLOSED`

## 10.2 Containment before recovery

Incident handling сначала ограничивает blast radius: freeze writes, revoke capabilities, isolate scope, pause workers/effects, quarantine resources или switch to read-only — в минимально достаточном объёме.

## 10.3 Trust states

Trust projection:

`TRUSTED / DEGRADED_TRUST / UNTRUSTED / REVALIDATING`

Process restart MUST NOT автоматически возвращать `TRUSTED`.

## 10.4 Crash recovery classification

После crash незавершённые executions/effects/repairs/leases классифицируются:

`SAFE_TO_RESUME / SAFE_TO_RETRY / MUST_RECONCILE / MUST_ABORT / HUMAN_REQUIRED`

Resume-all запрещён.

## 10.5 Recovery points and reconstruction

Recovery point описывает доказанно согласованное состояние, а не просто timestamp backup.

Восстановление SHOULD следовать:

```text
validated snapshot
+ canonical history replay
→ derived-state rebuild
→ revalidation
```

## 10.6 Partial recovery

Разные workspace/domains MAY иметь разные trust states. Capability engine MUST учитывать текущий trust state scope.

## 10.7 Revalidation

После recovery выполняется evidence-producing `RevalidationRun`.

Уровни:

`LOCAL / DOMAIN / WORKSPACE / INSTALLATION / FULL_TRUST`

Результаты:

`PASSED / PASSED_WITH_DEGRADATION / FAILED / INCONCLUSIVE`

Trust MUST NOT повышаться без evidence-backed revalidation.

## 10.8 Divergent histories and corruption

Split-brain/competing histories MUST NOT автоматически разрешаться `latest wins`.

Corruption различается как logical, physical, derived и cryptographic integrity failure, поскольку recovery strategies различаются.

## 10.9 Recovery loops and evidence preservation

Recovery имеет budgets. Повторяющееся автоматическое восстановление MUST останавливаться как `RECOVERY_LOOP_DETECTED`.

Перед destructive recovery forensic evidence SHOULD быть сохранено.

## 10.10 Revalidation barrier

```text
Recovery complete
  ↓
Revalidation PASSED
  ↓
Trust transition
  ↓
Capabilities restored progressively
```

Основные различия:

```text
AVAILABLE ≠ HEALTHY
HEALTHY ≠ TRUSTED
RECOVERED ≠ REVALIDATED
```

---

# Block 11 — Crypto / Data Governance

## 11.1 Crypto and governance are separate

Crypto отвечает за confidentiality/integrity/authenticity. Data Governance отвечает за ownership/classification/retention/export/deletion/purpose/sharing.

Encryption MUST NOT заменять authorization или governance.

## 11.2 Data classification

Базовые sensitivity levels:

`PUBLIC / INTERNAL / CONFIDENTIAL / RESTRICTED`

Classification включает также category/jurisdiction/handling metadata.

Derived data MUST NOT автоматически получать более низкую sensitivity, чем его источники. Declassification — отдельное governed decision.

## 11.3 Lineage-aware handling

Derived objects SHOULD сохранять source refs и effective classification lineage, чтобы policy decision был объяснимым.

## 11.4 Secrets are not memory

Credential/token/private-key material MUST NOT храниться как ordinary memory. Canonical state содержит `SecretRef`, а plaintext разрешается через controlled secret backend только на время использования.

## 11.5 Encryption and keys

Vestrace SHOULD поддерживать encryption at rest и envelope encryption для больших/CAS объектов.

Key hierarchy SHOULD быть scoped по installation/workspace/purpose. Key material SHOULD находиться за `KeyProvider`, а не в ordinary application DB.

Key lifecycle:

`ACTIVE / ROTATING / RETIRED / REVOKED / DESTROYED`

Crypto schemas MUST поддерживать algorithm agility через algorithm/version identifiers.

## 11.6 Signatures and audit integrity

Signature и encryption — разные функции.

Critical exports, manifests, checkpoints и qualification bundles MAY быть подписаны. Audit history SHOULD поддерживать tamper-evident chaining и signed checkpoints.

Hash match доказывает content integrity, но не provenance/authorship.

## 11.7 DataPolicy

DataPolicy регулирует classification constraints, retention, export, deletion, residency и sharing. Capability и DataPolicy являются независимыми обязательными gates.

## 11.8 Retention / hold / disposal

Retention clock и trigger задаются явно.

Lifecycle:

`ACTIVE → EXPIRED → PENDING_DISPOSAL → DISPOSED`

DataHold MAY временно блокировать disposal и MUST иметь authority/reason/audit.

Deletion semantics различают:

`LOGICAL_DELETE / PHYSICAL_DELETE / CRYPTO_ERASURE`

Ни один из них MUST NOT ложно заявлять уничтожение копий другого класса.

## 11.9 Dependency-aware forgetting

Удаление source/evidence MUST инициировать проверку зависимых claims/derived artifacts/indexes/exports. Claim, потерявший допустимое evidence, требует re-evaluation.

## 11.10 Governed export and federation

Export является отдельной governed operation с manifest, scope, classification, recipient, redaction, encryption/signature и provenance.

Federation relationship MUST NOT автоматически разрешать data transfer.

## 11.11 Model-boundary governance

Перед передачей ContextPack/model input внешнему provider Vestrace MUST проверять classification, destination/locality, redaction и DataPolicy. Policy MAY требовать local-only execution.

## 11.12 Crypto anomaly is a trust event

Hash/signature/audit-chain/key-compromise anomalies SHOULD поднимать trust-sensitive Incident и запускать containment/revalidation.

---

# Block 12 — Qualification / Conformance

## 12.1 Tests, conformance and qualification differ

```text
Unit / Integration Tests
        ↓
Conformance
        ↓
Qualification
```

Tests проверяют реализацию. Conformance проверяет компонент против нормативного контракта. Qualification отвечает, может ли конкретный build/deployment считаться пригодным для определённого trust profile.

## 12.2 Normative requirements

Критические guarantees SHOULD получить стабильные requirement IDs (`MEM-*`, `RET-*`, `EXT-*`, `REC-*`, `GOV-*`, `QUAL-*` и др.).

Каждый MUST requirement MUST иметь verification path.

## 12.3 Requirement traceability

```text
Requirement → Implementation → Conformance Test → Evidence
```

Наличие реализации без conformance evidence недостаточно для заявления поддержки профиля.

## 12.4 Profiles

Базовые conformance profiles:

- `CORE`;
- `MEMORY`;
- `COGNITION`;
- `AUTONOMY`;
- `FEDERATION`;
- `TRUSTED`.

Profiles имеют dependency closure; верхний profile нельзя заявить без обязательных нижних зависимостей.

## 12.5 Capability manifest

Installation SHOULD публиковать machine-readable manifest с implementation/schema versions, supported profiles, optional features, crypto/storage/model/effect adapter capabilities и known limitations.

## 12.6 Conformance suite

Категории:

`STATIC / BEHAVIORAL / STATEFUL / FAULT / SECURITY / RECOVERY / INTEROPERABILITY`

Обязательны golden end-to-end scenarios и deterministic fault injection points для crash/retry/recovery semantics.

Chaos testing является дополнением, а не заменой воспроизводимых fault tests.

## 12.7 Model nondeterminism

Qualification SHOULD проверять deterministic core properties и evidence requirements, а не exact model wording.

Cognitive benchmarks MAY измерять recall, temporal correctness, provenance coverage, stale-memory/conflict rates и context selection.

Security/governance requirements являются hard gates и MUST NOT компенсироваться высоким quality score.

## 12.8 Migration and interoperability

Qualification SHOULD включать migration scenarios, schema/event compatibility и adapter/backend qualification. Unsupported compatibility MUST быть явной, а не silent best-effort.

## 12.9 Qualification bundle

Результат qualification SHOULD быть воспроизводимым bundle с:

- implementation/source/build digest;
- profile;
- environment manifest;
- suite version;
- results/evidence;
- failures/limitations;
- optional signature.

Qualification относится к конкретному build + configuration + environment.

## 12.10 Continuous qualification

Qualification lifecycle:

`PRE-MERGE / RELEASE / DEPLOYMENT / PERIODIC / POST-INCIDENT`

Значимые изменения backend/provider/policy/crypto/schema/adapters MAY переводить baseline в `STALE` или `INVALIDATED` и требовать requalification.

## 12.11 Release trust gates

Архитектурная последовательность зрелости:

- **v0.2 — Correct:** CORE + MEMORY correctness;
- **v0.3 — Learn:** COGNITION/feedback quality;
- **v0.4 — Govern:** capabilities/workspaces/federation governance;
- **v0.5 — Understand:** health/integrity/repair;
- **v0.6 — Connect:** external effects/interoperability;
- **v1.0 — Trust:** TRUSTED profile, recovery/revalidation, crypto/governance и полный conformance contract.

`v1.0` определяется выполнением Trust Contract, а не количеством features.

---

# 13. Storage and authority baseline

## 13.1 Canonical state

PostgreSQL является основным authoritative transactional store для domain state, identity, policies, histories, revisions и journals.

Artifact content MAY храниться в локальном content-addressed storage. CAS hash является identity/integrity reference для bytes, но не предоставляет permission и не заменяет domain metadata/provenance в PostgreSQL.

## 13.2 SQL boundary

SQL writes выполняет только код Vestrace через контролируемые application/repository transactions. Произвольный write SQL не является agent API.

Ограниченные read-only views MAY предоставляться аналитическим/операционным субъектам под policy/RLS.

## 13.3 Core implementation language

Core Vestrace реализуется на Rust. Архитектурный контракт не требует Python runtime для authoritative core.

---

# 14. Documentation hierarchy

После утверждения этого baseline специализированные документы должны быть приведены в соответствие в следующем порядке:

1. `Domain Model v0.2`;
2. `Normative Invariants Catalog`;
3. `Trust & Authority Model`;
4. `Data & Temporal Model`;
5. `Execution & External Effects Contract`;
6. `Health / Repair / Incident Contract`;
7. `Crypto & Data Governance Contract`;
8. `Qualification / Conformance Specification`;
9. ADR set;
10. version roadmap;
11. consistency pass по README и существующим docs/specs.

Если старый design/spec противоречит этому контракту, конфликт MUST быть явно разрешён при consistency pass. Старые документы не удаляются только ради сокрытия архитектурной эволюции; они либо получают historical/superseded status, либо обновляются, если остаются normative.

# 15. Implementation freeze during documentation phase

До завершения и внутренней проверки документационного набора:

- Rust-код MUST NOT изменяться в рамках этой архитектурной работы;
- migrations MUST NOT добавляться;
- runtime/API behavior MUST NOT изменяться;
- documentation branch MUST оставаться отделённой от `main`;
- implementation gap analysis выполняется только после documentation consistency pass.

# 16. Completion criterion for this contract

Architecture Contract v0.2 считается полным baseline, когда специализированные документы раскрывают его без противоречий и каждое значимое MUST-требование получает место в Normative Invariants / Conformance catalog.

Этот документ фиксирует 12 завершённых архитектурных блоков:

```text
[✓] 1. Persistent Cognition Core
[✓] 2. Temporal & Concurrency
[✓] 3. Mutation & Reconciliation
[✓] 4. Retrieval / ContextPack 2.0
[✓] 5. Execution Feedback & Learning
[✓] 6. Capability Governance
[✓] 7. Identity / Workspace / Federation
[✓] 8. Health / Integrity / Repair
[✓] 9. External Effects
[✓] 10. Incident / Recovery / Revalidation
[✓] 11. Crypto / Data Governance
[✓] 12. Qualification / Conformance
```
