# Vestrace Execution & External Effects Contract v0.2

**Статус:** normative execution boundary specification  
**Дата:** 2026-08-10

## 1. Назначение

Этот документ определяет единый durable execution boundary Vestrace и правила взаимодействия с внешним миром.

Главный принцип:

> **External side effects are governed executions with explicit intent, authority, outcome uncertainty and reconciliation.**

---

# 2. Единственный execution runtime

Vestrace использует существующую Run-first модель:

```text
AgentRun
├─ ExecutionPlanRevision
├─ RunStep[]
├─ RunEvent[]
├─ RunCheckpoint[]
└─ typed operation references
```

Repair, recovery, external effects, governance workflows и verification не создают отдельный runtime.

Generic `Task → Action → Attempt` execution hierarchy запрещён, пока отдельный ADR не докажет недостающую семантику.

---

# 3. Execution identities

## 3.1 AgentRun

`AgentRun` — durable execution root.

Run хранит logical lifecycle, plan reference, principal/workspace authority context и durable execution history.

## 3.2 RunStep

`RunStep` — typed unit плана/исполнения, которая MAY ссылаться на конкретную operation family.

## 3.3 Typed operation owners

Попытки и provider-specific semantics принадлежат конкретному operation aggregate:

- ModelExecution;
- ToolInvocation;
- ExternalEffect;
- RemoteAgentInvocation;
- HumanRequest;
- VerificationAttempt;
- SubRun.

---

# 4. Execution command contract

Значимая command SHOULD иметь:

```text
command_id
idempotency_key?
workspace_id
run_id / target id
actor
expected_version
correlation_id
issued_at
command payload
```

Mutation без expected version допускается только если operation по своей природе append-only/idempotent и не создаёт lost update risk.

---

# 5. Event history

Run events являются append-only facts о logical execution transitions.

Event envelope SHOULD включать:

- event id;
- aggregate/run id;
- workspace;
- sequence/version;
- event type/version;
- actor;
- causation/correlation;
- occurred/recorded time;
- typed payload.

Reducer/replay обязан проверять sequence и event-type/version compatibility.

---

# 6. Checkpoint

Checkpoint ускоряет recovery/replay, но не заменяет canonical history.

Checkpoint SHOULD содержать:

- run state snapshot;
- logical version/sequence;
- active plan revision;
- active step/wait references;
- stable refs на context/artifacts/policy/budget/external operations;
- integrity digest;
- format version.

Secret bytes в checkpoint не сохраняются.

---

# 7. Resume / retry / replay

Эти понятия различаются:

- **resume** — продолжить durable operation с сохранённого state;
- **retry** — повторно вызвать retry-safe operation;
- **replay** — реконструировать logical state из history без повторения external side effects;
- **reconciliation** — установить фактический outcome ранее dispatched внешнего действия.

Replay MUST NOT повторять side effects.

---

# 8. Idempotency

## 8.1 Command idempotency

Повтор одного idempotency key с эквивалентным request MAY вернуть сохранённый result.

Тот же key с иным material request MUST давать idempotency conflict.

## 8.2 Operation idempotency

External/model/tool adapter отдельно объявляет idempotency semantics.

Command idempotency Vestrace не доказывает idempotency внешнего provider.

---

# 9. Cancellation

Cancellation является durable intent.

Фактическая гарантия отмены зависит от operation adapter.

После cancellation request external operation MAY всё равно завершиться; ambiguous result должен reconciliation, а не скрываться.

---

# 10. ExternalEffectIntent

До любого governed external side effect создаётся immutable intent:

```text
ExternalEffectIntent
├─ effect_id
├─ execution_ref
├─ actor
├─ adapter/tool
├─ operation
├─ target
├─ normalized arguments digest
├─ expected effect
├─ preconditions[]
├─ risk
├─ reversibility
├─ idempotency profile
├─ delivery semantics
├─ required capability
├─ budget reservation ref?
├─ policy decision ref?
└─ created_at
```

Intent после authorization не изменяется.

---

# 11. Effect lifecycle

Conceptual phases:

```text
PREPARED
  ↓
AUTHORIZED
  ↓
DISPATCHING
  ↓
ACKNOWLEDGED / FAILED / UNKNOWN
  ↓
CONFIRMED / RECONCILING / UNKNOWN
```

`COMMITTED` в transport sense означает request dispatch, но не подтверждённый business outcome.

---

# 12. Prepare

Prepare phase выполняет без side effect:

- normalize arguments;
- determine exact target;
- calculate risk;
- declare classification/data destination;
- calculate budget impact;
- resolve adapter semantics;
- build preconditions;
- produce immutable intent.

Prepare MAY быть доступен субъекту без dispatch capability.

---

# 13. Authorization

Перед dispatch проверяются:

```text
capability
∩ policy
∩ workspace/federation rules
∩ DataPolicy
∩ risk ceiling
∩ trust state
∩ budget
∩ approval obligations
∩ preconditions
```

Любой hard deny блокирует dispatch.

Approval связывается с exact intent digest.

---

# 14. Delivery semantics

Adapter MUST объявлять одно из:

## 14.1 AT_MOST_ONCE

Повтор после ambiguous dispatch небезопасен по умолчанию.

## 14.2 AT_LEAST_ONCE

Повтор допустим и provider semantics допускают duplicate processing либо operation idempotent.

## 14.3 EFFECTIVELY_ONCE

Provider поддерживает надёжный idempotency key/deduplication, позволяющий безопасные retries в заявленном scope.

## 14.4 UNKNOWN

Adapter не может дать достаточную delivery guarantee. Policy должна быть более консервативной.

Vestrace не использует термин `EXACTLY_ONCE` как универсальную runtime guarantee.

---

# 15. IdempotencyProfile

```text
IdempotencyProfile
├─ mode
├─ key_scope
├─ key_generation
├─ retry_safe_conditions
├─ duplicate_detection
├─ reconciliation_method
└─ provider_limitations
```

Effect ID SHOULD использоваться как stable idempotency anchor, когда provider позволяет.

---

# 16. Result vs Outcome

Разделяются:

```text
transport result
provider acknowledgement
external state confirmation
business desired outcome
```

Пример:

```text
HTTP 202
```

не означает автоматически:

```text
business operation completed successfully
```

---

# 17. UNKNOWN

UNKNOWN используется, когда Vestrace не может доказать факт выполнения/невыполнения effect.

Причины:

- timeout после dispatch;
- connection loss;
- process crash между provider response и persistence;
- provider async semantics без queryable result;
- inconsistent remote state.

UNKNOWN MUST NOT автоматически становиться FAILED или trigger automatic retry.

---

# 18. Effect reconciliation

Reconciliation использует strongest available evidence:

1. provider idempotency lookup;
2. exact external resource ID;
3. operation status endpoint;
4. ETag/version/revision;
5. content hash;
6. provider transaction/reference number;
7. bounded search by unique Vestrace marker;
8. human verification, если deterministic read-back отсутствует.

Reconciliation result сохраняет evidence.

---

# 19. Preconditions and TOCTOU

## 19.1 Material preconditions

Intent MAY включать:

- expected resource version;
- expected current state;
- target existence/non-existence;
- policy/share/trust generation;
- budget reservation;
- exact recipient/account/resource.

## 19.2 Recheck

Перед dispatch critical preconditions проверяются снова.

Mismatch => `STALE_INTENT` / precondition failure, без side effect.

## 19.3 Strongest provider primitive

Adapter SHOULD использовать conditional requests/ETag/revision/commit SHA/compare-and-set, если доступны.

---

# 20. Reversibility

Classification:

```text
REVERSIBLE
COMPENSATABLE
IRREVERSIBLE
UNKNOWN_REVERSIBILITY
```

## 20.1 REVERSIBLE

Существует semantic inverse, который действительно восстанавливает предыдущий state в пределах contract.

## 20.2 COMPENSATABLE

Исходный effect нельзя отменить, но можно выполнить новый effect, уменьшающий последствия.

## 20.3 IRREVERSIBLE

Факт effect невозможно отменить.

## 20.4 UNKNOWN_REVERSIBILITY

Adapter не может гарантировать модель.

Risk policy усиливается для IRREVERSIBLE/UNKNOWN.

---

# 21. Compensation

Compensation — новый внешний effect с отдельным intent/authorization/receipt.

Он сохраняет relation:

```text
compensates(effect_id)
```

История исходного effect остаётся.

---

# 22. Effect budgets

Capability/policy MAY ограничить:

- max count;
- rate;
- monetary cost;
- risk;
- target set;
- cumulative irreversible effects;
- provider quota.

Hard budget reservation SHOULD происходить до irrevocable dispatch.

---

# 23. Dry-run

Adapter объявляет:

```text
NATIVE
SIMULATED
UNSUPPORTED
```

`SIMULATED` MUST NOT выдавать guarantee, эквивалентную provider-native dry run.

---

# 24. Sensitive payloads

External history SHOULD хранить:

- redacted normalized arguments;
- arguments digest;
- secret refs;
- safe provider metadata.

Secret plaintext MUST NOT сохраняться в intent/receipt/audit.

Raw sensitive response storage регулируется DataPolicy/retention.

---

# 25. ExternalEffectReceipt

```text
ExternalEffectReceipt
├─ effect_id
├─ adapter/provider
├─ dispatched_at
├─ acknowledgement_at?
├─ response_class
├─ external_resource_id?
├─ external_version?
├─ response_digest?
├─ outcome_status
├─ evidence_refs[]
└─ recorded_at
```

Receipt immutable; последующие outcome updates создают новые outcome/reconciliation facts либо append-oriented status records.

---

# 26. Crash boundaries

Critical fault points:

```text
before intent persistence
before authorization persistence
after authorization / before dispatch
after dispatch / before receipt
after receipt / before outcome confirmation
after outcome / before run-step commit
```

Каждый boundary должен иметь определённую recovery semantic.

Особенно:

```text
after dispatch / before receipt
→ UNKNOWN
→ reconcile
```

а не auto-retry.

---

# 27. Step RecoveryProfile

```text
RecoveryProfile
├─ restartable
├─ idempotent
├─ replay_safe
├─ reconciliation_required
├─ cancellation_semantics
├─ external_effect_semantics
└─ recovery_strategy
```

Classification after crash:

```text
SAFE_TO_RESUME
SAFE_TO_RETRY
MUST_RECONCILE
MUST_ABORT
HUMAN_REQUIRED
```

---

# 28. Model execution boundary

Model invocation является external/provider interaction, но model generation itself не обязательно business side effect.

Model execution MUST сохранять:

- exact provider/model profile revision;
- policy/routing decision;
- input/context refs;
- classification/destination decision;
- request/response integrity metadata;
- usage/latency/cost;
- failure/retry/fallback history.

Model output не становится canonical cognition автоматически.

---

# 29. Tool execution boundary

Tool definition/revision должен объявлять:

- operation semantics;
- side-effect class;
- risk baseline;
- input/output schema;
- idempotency;
- timeout;
- cancellation;
- reconciliation;
- dry-run;
- required capabilities.

Agent MUST NOT самостоятельно переопределять эти adapter properties.

---

# 30. Human operations

Human approval/input/request также durable и correlatable.

Human response MAY быть authoritative по policy, но must preserve actor/time/scope and exact requested intent.

---

# 31. Observability vs canonical history

Tracing/logs/metrics являются operational telemetry и MAY быть lossy/retained separately.

Canonical execution facts, receipts, approvals и required audit MUST NOT зависеть от logging verbosity.

---

# 32. Normative mappings

Основные IDs:

- `ARC-004..007`;
- `TMP-006..010`;
- `CAP-010..013`;
- `EXT-001..018`;
- `REC-004..006`;
- `GOV-021..022`.

---

# 33. Golden scenarios

## 33.1 Idempotent create with timeout

```text
prepare
→ authorize
→ dispatch(idempotency_key=effect_id)
→ timeout
→ UNKNOWN
→ provider lookup by key
→ exists
→ CONFIRMED
```

No duplicate create.

## 33.2 Unsafe email send timeout

```text
send email
→ connection lost after dispatch
→ UNKNOWN
→ adapter cannot prove delivery
→ no automatic retry
→ human/policy reconciliation
```

## 33.3 Stale approved target

```text
approve update resource v7
→ resource becomes v8
→ precondition recheck fails
→ STALE_INTENT
→ no dispatch
```

## 33.4 Compensation

```text
Effect A confirmed
→ undesirable but not reversible
→ create Effect B intent
→ authorize B
→ execute B
→ relation B compensates A
```

A remains in history.

---

# 34. Forbidden shortcuts

```text
HTTP success == business outcome
transport failure == effect not applied
UNKNOWN == FAILED
retry == resume
replay == re-execute side effects
approval == permission to change intent
compensation == rollback
idempotency key == universal exactly-once
process crash == safe retry
```

---

# 35. Completion criteria

Contract считается завершённым, когда:

1. adapter specifications обязаны объявлять delivery/idempotency/reconciliation/reversibility;
2. Incident/Recovery spec использует UNKNOWN effect semantics без дублирования;
3. capability catalog разделяет prepare и dispatch;
4. qualification spec содержит fault scenarios на каждом critical dispatch boundary;
5. current implementation documentation clearly distinguishes implemented execution foundations from this target contract.
