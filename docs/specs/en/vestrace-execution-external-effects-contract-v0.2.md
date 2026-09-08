# Vestrace Execution & External Effects Contract v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-execution-external-effects-contract-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative execution-boundary specification
**Date:** 2026-08-10

## 1. Purpose

This document defines Vestrace's single durable execution boundary and rules for interaction with the external world.

Main principle:

> **External side effects are governed executions with explicit intent, authority, outcome uncertainty and reconciliation.**

---

# 2. Single execution runtime

Vestrace uses the existing Run-first model:

```text
AgentRun
├─ ExecutionPlanRevision
├─ RunStep[]
├─ RunEvent[]
├─ RunCheckpoint[]
└─ typed operation references
```

Repair, recovery, external effects, governance workflows, and verification do not create separate runtimes.

A generic `Task → Action → Attempt` execution hierarchy is prohibited unless a separate ADR establishes the missing semantics.

---

# 3. Execution identities

## 3.1 AgentRun

`AgentRun` — durable execution root.

Run stores logical lifecycle, plan reference, principal/workspace authority context, and durable execution history.

## 3.2 RunStep

`RunStep` is a typed plan/execution unit that MAY reference a particular operation family.

## 3.3 Typed operation owners

Attempts and provider-specific semantics belong to the specific operation aggregate:

- ModelExecution;
- ToolInvocation;
- ExternalEffect;
- RemoteAgentInvocation;
- HumanRequest;
- VerificationAttempt;
- SubRun.

---

# 4. Execution command contract

A significant command SHOULD have:

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

A mutation without an expected version is permitted only when the operation is inherently append-only/idempotent and creates no lost-update risk.

---

# 5. Event history

Run events are append-only facts about logical execution transitions.

An event envelope SHOULD contain:

- event id;
- aggregate/run id;
- workspace;
- sequence/version;
- event type/version;
- actor;
- causation/correlation;
- occurred/recorded time;
- typed payload.

Reducer/replay must check sequence and event-type/version compatibility.

---

# 6. Checkpoint

A checkpoint accelerates recovery/replay without replacing canonical history.

A checkpoint SHOULD contain:

- run state snapshot;
- logical version/sequence;
- active plan revision;
- active step/wait references;
- stable references to context, artifacts, policy, budget, and external operations;
- integrity digest;
- format version.

Checkpoints do not store secret bytes.

---

# 7. Resume / retry / replay

Distinguish the following concepts:

- **resume** — continue a durable operation from its persisted state;
- **retry** — invoke a retry-safe operation again;
- **replay** — reconstruct logical state from history without repeating external side effects;
- **reconciliation** — establish the actual outcome of an earlier dispatched external action.

Replay MUST NOT repeat side effects.

---

# 8. Idempotency

## 8.1 Command idempotency

An equivalent request with the same idempotency key MAY return its stored result.

The same key with a materially different request MUST produce an idempotency conflict.

## 8.2 Operation idempotency

Each external/model/tool adapter separately declares its idempotency semantics.

Vestrace command idempotency does not establish an external provider's idempotency.

---

# 9. Cancellation

Cancellation is a durable intent.

The actual cancellation guarantee depends on the operation adapter.

An external operation MAY complete after cancellation is requested; an ambiguous result requires reconciliation, not concealment.

---

# 10. ExternalEffectIntent

Before any governed external side effect, create an immutable intent:

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

An intent does not change after authorization.

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

`COMMITTED` in the transport sense means request dispatch, not a confirmed business outcome.

---

# 12. Prepare

The prepare phase performs the following without a side effect:

- normalize arguments;
- determine exact target;
- calculate risk;
- declare classification/data destination;
- calculate budget impact;
- resolve adapter semantics;
- build preconditions;
- produce immutable intent.

Prepare MAY be available to a principal without dispatch capability.

---

# 13. Authorization

Before dispatch, check:

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

Any hard denial blocks dispatch.

Approval binds the exact intent digest.

---

# 14. Delivery semantics

The adapter MUST declare one of:

## 14.1 AT_MOST_ONCE

Repeating an ambiguously dispatched request is unsafe by default.

## 14.2 AT_LEAST_ONCE

Retry is permitted, and provider semantics either allow duplicate processing or make the operation idempotent.

## 14.3 EFFECTIVELY_ONCE

The provider supports a reliable idempotency key/deduplication mechanism that permits safe retries within the declared scope.

## 14.4 UNKNOWN

The adapter cannot provide a sufficient delivery guarantee. Policy must be more conservative.

Vestrace does not use `EXACTLY_ONCE` as a universal runtime guarantee.

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

Effect ID SHOULD serve as a stable idempotency anchor when the provider supports it.

---

# 16. Result vs Outcome

Distinguish:

```text
transport result
provider acknowledgement
external state confirmation
business desired outcome
```

Example:

```text
HTTP 202
```

does not automatically imply:

```text
business operation completed successfully
```

---

# 17. UNKNOWN

UNKNOWN applies when Vestrace cannot establish whether the effect occurred.

Causes:

- timeout after dispatch;
- connection loss;
- process crash between the provider response and persistence;
- asynchronous provider semantics without a queryable result;
- inconsistent remote state.

UNKNOWN MUST NOT automatically become FAILED or trigger an automatic retry.

---

# 18. Effect reconciliation

Reconciliation uses the strongest available evidence:

1. provider idempotency lookup;
2. exact external resource ID;
3. operation status endpoint;
4. ETag/version/revision;
5. content hash;
6. provider transaction/reference number;
7. bounded search by unique Vestrace marker;
8. Human verification when deterministic read-back is unavailable.

A reconciliation result retains its evidence.

---

# 19. Preconditions and TOCTOU

## 19.1 Material preconditions

An intent MAY include:

- expected resource version;
- expected current state;
- target existence/non-existence;
- policy/share/trust generation;
- budget reservation;
- exact recipient/account/resource.

## 19.2 Recheck

Recheck critical preconditions before dispatch.

A mismatch produces `STALE_INTENT` / precondition failure without a side effect.

## 19.3 Strongest provider primitive

The adapter SHOULD use conditional requests, ETags, revisions, commit SHAs, or compare-and-set where available.

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

A semantic inverse exists that genuinely restores the previous state within the contract.

## 20.2 COMPENSATABLE

The original effect cannot be undone, but a new effect can reduce its consequences.

## 20.3 IRREVERSIBLE

The occurrence of the effect cannot be undone.

## 20.4 UNKNOWN_REVERSIBILITY

The adapter cannot guarantee the model.

Risk policy is strengthened for IRREVERSIBLE/UNKNOWN.

---

# 21. Compensation

Compensation is a new external effect with its own intent, authorization, and receipt.

It retains the relationship:

```text
compensates(effect_id)
```

The original effect's history remains intact.

---

# 22. Effect budgets

Capability/policy MAY limit:

- max count;
- rate;
- monetary cost;
- risk;
- target set;
- cumulative irreversible effects;
- provider quota.

Hard-budget reservation SHOULD occur before irrevocable dispatch.

---

# 23. Dry-run

The adapter declares:

```text
NATIVE
SIMULATED
UNSUPPORTED
```

`SIMULATED` MUST NOT imply a guarantee equivalent to provider-native dry run.

---

# 24. Sensitive payloads

External history SHOULD retain:

- redacted normalized arguments;
- arguments digest;
- secret refs;
- safe provider metadata.

Secret plaintext MUST NOT be stored in intent, receipt, or audit.

DataPolicy/retention governs storage of raw sensitive responses.

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

A receipt is immutable; later outcome updates create new outcome/reconciliation facts or append-oriented status records.

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

Every boundary must define its recovery semantics.

In particular:

```text
after dispatch / before receipt
→ UNKNOWN
→ reconcile
```

rather than automatic retry.

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

Model invocation is an external/provider interaction, but model generation itself is not necessarily a business side effect.

Model execution MUST retain:

- exact provider/model profile revision;
- policy/routing decision;
- input/context refs;
- classification/destination decision;
- request/response integrity metadata;
- usage/latency/cost;
- failure/retry/fallback history.

Model output does not automatically become canonical cognition.

---

# 29. Tool execution boundary

A tool definition/revision must declare:

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

An agent MUST NOT independently override these adapter properties.

---

# 30. Human operations

Human approval/input/requests are also durable and correlatable.

A human response MAY be authoritative under policy, but must preserve actor, time, scope, and the exact requested intent.

---

# 31. Observability vs canonical history

Tracing, logs, and metrics are operational telemetry and MAY be lossy or retained separately.

Canonical execution facts, receipts, approvals, and required audit MUST NOT depend on logging verbosity.

---

# 32. Normative mappings

Principal IDs:

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

The contract is complete when:

1. Adapter specifications require delivery, idempotency, reconciliation, and reversibility declarations.
2. The Incident/Recovery specification reuses UNKNOWN-effect semantics without duplication.
3. The capability catalog separates prepare from dispatch.
4. The qualification specification includes fault scenarios at every critical dispatch boundary.
5. current implementation documentation clearly distinguishes implemented execution foundations from this target contract.
