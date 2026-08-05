# Vestrace — P0–P4 Priority Amendment

**Status:** Approved  
**Approved:** 2026-08-05  
**Effect:** Reorders implementation priorities until P4 is complete  
**Related specification:** `2026-08-05-r1-4b-persisted-idempotency.md`

## 1. Normative decision

Until P4 is complete, Vestrace implementation follows this strict order:

```text
P0 — Restore Main
→ P1 — State Engine Vertical
→ P2 — Capability and Policy
→ P3 — Artifact CAS
→ P4 — Truthful Readiness
```

A phase may start only after the preceding phase exit gate is satisfied.

R4–R8 work and nonessential expansion of UI, memory, workflow, and multi-agent surfaces are frozen until P4.

## 2. P0 — Restore Main

### Goal

Return `main` to a compiling, testable, reproducible, and architecturally truthful state.

### Required work

- freeze new domain entities and product surfaces;
- restore compilation of the entire workspace;
- resolve incomplete or conflicting changes;
- pass formatting, Clippy, and all tests;
- remove direct SQL from HTTP handlers;
- keep HTTP as a transport adapter over application ports;
- verify migrations from an empty database;
- verify Docker Compose from a clean environment;
- remove or explicitly mark unavailable endpoints, commands, and UI actions;
- ensure documentation does not claim unavailable features.

### Excluded during P0

- new workflow nodes;
- new memory entities;
- new multi-agent runtime behavior;
- new UI sections;
- new public API surfaces;
- expanded AG-UI support;
- new external integrations.

### P0 exit gate

P0 is complete only when:

1. `main` compiles.
2. `cargo fmt --all --check` passes.
3. Clippy passes with `-D warnings`.
4. The complete Rust test suite passes.
5. Console typecheck and production build pass.
6. Docker Compose starts from a clean database.
7. HTTP handlers contain no direct SQL.
8. Migrations are reproducible.
9. Documentation matches actual behavior.
10. Incomplete features are not shown as ready.

## 3. P1 — State Engine Vertical

### Goal

Deliver a complete recoverable vertical path:

```text
CreateRun
→ RunEvent
→ reducer
→ checkpoint
→ replay
```

The State Engine becomes the sole owner of state transitions.

### P1 / PR1 — Typed Core

Required capabilities:

- typed run commands;
- typed run events;
- versioned event envelopes;
- append-only event store;
- pure reducer;
- deterministic replay;
- optimistic concurrency;
- event-derived projections;
- checkpoints;
- checkpoint validation;
- projection rebuild;
- corrupted-stream detection;
- canonical command execution.

Primary acceptance scenario:

```text
create run
→ execute several transitions
→ delete projection
→ restore from checkpoint and events
→ obtain identical state
```

### P1 / PR2 — Pause, Resume, and Retries

Required capabilities:

- typed waiting reasons;
- pause and resume;
- retry state and counters;
- retry limits;
- timeout state;
- crash-safe continuation;
- prevention of repeated completion or repeated side effects.

### P1 / PR3 — Delegated Subagents

Required capabilities:

- parent and child runs;
- explicit delegation relationship;
- deterministic parent wait for child completion;
- typed child results;
- parent recovery after child completion;
- no shared mutable memory;
- bounded recursion depth.

Capability attenuation is implemented fully in P2. Before P2, delegation may operate only in a restricted internal mode.

### P1 / PR4 — Compensation

Required capabilities:

- typed compensating actions;
- registration of completed side effects;
- deterministic reverse-order compensation;
- retryable compensation;
- partial-compensation state;
- manual-intervention state;
- compensation started/succeeded/failed events.

### P1 supporting infrastructure

The approved R1.4B persisted command idempotency specification belongs here.

It must be implemented only after the Typed Core is stable and must not displace:

- checkpoints;
- replay;
- projection rebuild;
- pause/resume;
- retry semantics.

### Excluded from P1

- full production capability policy engine;
- Artifact CAS;
- distributed scheduler and workers;
- complete AG-UI runtime;
- new long-term memory layers;
- free-form multi-agent group chat;
- uncontrolled swarm behavior.

### P1 exit gate

P1 is complete only when:

1. A run is fully reconstructable from events.
2. A projection can be deleted and rebuilt.
3. Checkpoints are acceleration data, not authority.
4. Pause/resume survives process restart.
5. Retry does not duplicate completed side effects.
6. Parent/child runs recover after crash.
7. Compensation has a deterministic journal.
8. Corrupted streams are not hidden by projections.
9. Every state transition passes through the State Engine.
10. HTTP does not mutate run state directly.

## 4. P2 — Capability and Policy

### Goal

Make action execution explicitly authorized, attenuated, bounded, and explainable.

Roles are templates only. Actual access is represented by capability tokens.

A capability may restrict:

- subject;
- tool;
- resource;
- operation;
- workspace;
- expiry;
- budget;
- risk;
- usage conditions;
- delegation rights.

### Required capabilities

- capability envelope;
- issuance and verification;
- expiry and revocation;
- resource and operation restrictions;
- budget restrictions;
- risk restrictions;
- delegation attenuation;
- policy decisions and evidence;
- explicit denial reasons;
- canonical audit events.

### Risk model

```text
low
medium
high
critical
```

Context may raise risk. Lowering risk requires an explicit trusted policy rule.

### Delegation invariant

```text
child rights ⊆ explicitly delegated parent rights
```

A child capability may only narrow the parent capability.

### P2 exit gate

P2 is complete only when:

1. Protected tool calls cannot run without a capability.
2. Capability validation occurs immediately before the side effect.
3. Expired and revoked capabilities are rejected.
4. Resource and operation restrictions are enforced.
5. Budget restrictions are applied atomically.
6. A child cannot expand parent rights.
7. High and critical operations pass a policy gate.
8. Every allow or deny decision is explainable.
9. Policy events are part of the canonical trace.
10. Cross-workspace capabilities are rejected.

## 5. P3 — Artifact CAS

### Goal

Provide local content-addressed artifact storage with integrity verification and readable workspace references.

### Required capabilities

- content hashes;
- local immutable blob storage;
- artifact metadata;
- MIME type and size;
- creation provenance;
- producing run and step;
- workspace ownership;
- readable virtual paths;
- atomic writes;
- deduplication;
- integrity checks;
- garbage-collection safety boundaries.

### Storage model

Physical identity:

```text
artifact_id = content hash
```

Logical access:

```text
workspace virtual path
→ artifact reference
→ immutable CAS blob
```

One immutable blob may have multiple logical references.

### Constraints

- agents receive no direct filesystem paths;
- changing a virtual path does not mutate a blob;
- changed content produces a new hash;
- metadata alone does not prove blob existence;
- a blob is invalid until its hash is verified.

### P3 exit gate

P3 is complete only when:

1. Identical content is deduplicated.
2. Changed content receives a different hash.
3. Partial writes cannot create valid artifacts.
4. Corrupted blobs are detected.
5. Artifacts are linked to producing runs and steps.
6. Cross-workspace reads are denied.
7. Virtual paths prevent traversal.
8. Removing a reference does not remove an in-use blob.
9. State Engine records references rather than arbitrary paths.
10. Artifacts survive process restart and replay.

## 6. P4 — Truthful Readiness

### Goal

Eliminate false readiness. Every feature, endpoint, CLI command, UI action, and documentation claim must expose its actual maturity.

### Required work

- define maturity metadata;
- classify features, endpoints, CLI commands, UI sections, and integrations;
- hide or disable false actions;
- separate liveness, service readiness, and feature readiness;
- expose a machine-readable feature inventory;
- align README and architecture documentation with that inventory;
- prohibit readiness claims based only on PostgreSQL availability.

### Minimum maturity dimensions

Every capability must state whether it has:

- domain semantics;
- persistence;
- recovery;
- authorization;
- acceptance tests;
- API/UI exposure;
- production readiness.

Maturity label names require a separate P4 design decision. Until then, the word `ready` must not be used without a concrete, testable definition.

### Readiness separation

```text
process readiness
database readiness
migration readiness
state engine readiness
worker readiness
feature readiness
```

Overall service readiness must not imply that every feature is ready.

### P4 exit gate

P4 is complete only when:

1. Every declared feature has maturity metadata.
2. UI does not present unavailable actions as enabled.
3. CLI placeholders do not return success.
4. Stub endpoints explicitly report unavailability.
5. README does not describe missing features as implemented.
6. Readiness checks have concrete semantics.
7. Feature inventory is machine-readable.
8. Acceptance tests validate maturity claims.
9. Documentation is generated from or checked against the inventory.
10. No public surface creates a false impression of readiness.

## 7. Relationship to R0–R8

Until P4 is complete, P0–P4 is the primary roadmap. R0–R8 is secondary.

Approximate mapping:

```text
P0 ≈ R0 plus main stabilization
P1 ≈ R1 plus required durable runtime primitives
P2 ≈ capability, policy, and risk portions of R3
P3 ≈ Artifact CAS portion of R2
P4 ≈ truthful foundation, maturity, and readiness
```

R4–R8 implementation is deferred until P4 passes.

## 8. R1.4B status

`Vestrace R1.4B — Persisted Command Idempotency Specification v0.1` is approved.

Implementation status:

```text
approved
deferred into P1 supporting infrastructure
```

It must not be implemented before:

1. P0 completion;
2. stabilization of P1 Typed Core;
3. checkpoint/replay/rebuild vertical completion.

## 9. Immediate implementation order

```text
1. Complete P0 stabilization.
2. Integrate existing R1 branches without destabilizing main.
3. Complete P1 PR1: events, reducer, checkpoints, replay, rebuild.
4. Complete P1 PR2: pause, resume, retries.
5. Implement persisted idempotency at the canonical command boundary.
6. Complete P1 PR3: delegated subagents.
7. Complete P1 PR4: compensation.
8. Complete P2 capabilities and policy.
9. Complete P3 Artifact CAS.
10. Complete P4 truthful readiness.
```

## 10. Scope freeze

Until P4 passes, do not add:

- new memory layers;
- visual workflow builder;
- new agent patterns outside P1;
- adaptive routing;
- self-improvement;
- skill marketplace;
- distributed orchestration;
- decorative UI surfaces.

An exception is allowed only for work necessary to unblock or verify P0–P4.
