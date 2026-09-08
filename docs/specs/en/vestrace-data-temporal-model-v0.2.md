# Vestrace Data & Temporal Model v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-data-temporal-model-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative data/temporal specification
**Date:** 2026-08-10
**Foundational documents:** Architecture Contract, Domain Model, and Invariants Catalog.

## 1. Goal

This document defines how Vestrace represents time, versions, validity, historical state, provenance, and temporal queries without conflating event occurrence, recording time, and the period in which knowledge applies.

Core principle:

> **Time of occurrence, time of recording, time of validity and object revision time are separate axes.**

---

# 2. Temporal axes

## 2.1 `occurred_at`

The time at which a source asserts or demonstrates that an event occurred in the external or domain world.

Properties:

- MAY be unknown;
- MAY be imprecise;
- MAY be received later than `recorded_at`;
- does not establish authoritative ordering within Vestrace without an additional contract.

## 2.2 `recorded_at`

The time when Vestrace authoritatively recorded the fact.

For an append-only domain fact, `recorded_at` MUST be known.

`recorded_at` supports audit/history ordering when source occurrence time is unreliable.

## 2.3 `created_at`

The creation time of a particular domain entity/revision within Vestrace.

For an immutable fact this often equals `recorded_at`, but it remains a semantically separate field when the lifecycle requires that distinction.

## 2.4 `valid_from` / `valid_until`

The interval during which the knowledge applies in the domain.

Example:

```text
recorded_at: 2026-08-10
valid_from:  2026-09-01
```

means Vestrace already knows a fact that will become applicable later.

## 2.5 Processing timestamps

Operational timestamps such as `started_at`, `leased_at`, `finished_at`, `retrieved_at`, and `verified_at` describe execution/processing and MUST NOT substitute for semantic validity.

---

# 3. Unknown and uncertain time

## 3.1 Unknown is null/explicit uncertainty

When a time is unknown, Vestrace MUST NOT fill it with a fabricated timestamp such as the epoch or `now()`.

## 3.2 Time precision

A source MAY have the following precision:

- exact instant;
- minute/hour/day;
- date-only;
- interval;
- unknown.

When precision materially affects reconciliation, it SHOULD be retained as metadata.

## 3.3 Time confidence

Temporal confidence MAY be a separate evidence attribute and must not automatically be combined with semantic claim confidence.

---

# 4. Versioning model

## 4.1 Identity vs revision

Stable identity:

```text
MemoryId / AgentId / WorkflowId / PolicyId
```

Exact historical content:

```text
MemoryRevisionId / AgentRevisionId / WorkflowRevisionId / PolicyVersionId
```

A reference used for reproducibility SHOULD identify an exact revision.

## 4.2 State revision

Mutable lifecycle identities MAY have a monotonic `state_revision` for optimistic concurrency, independent of content-revision numbering.

Example:

- Memory content revision #5 unchanged;
- the `Active → Superseded` lifecycle transition changes the aggregate state revision.

## 4.3 Expected revision

A concurrent mutation MUST check expected state/revision.

```text
read v12
   ↓
prepare mutation(expected=v12)
   ↓
current=v13
   ↓
STALE / CONFLICT
```

Silent overwrite forbidden.

---

# 5. Append-only history

## 5.1 Facts

Events, immutable revisions, receipts, audit entries, assessments, and verification results do not change after commit.

## 5.2 Corrections

A correction creates a new fact/revision with a relationship:

- `corrects`;
- `supersedes`;
- `compensates`;
- `invalidates`;
- `reconciles`.

## 5.3 Tombstones

Content deletion MAY retain a content-free tombstone with typed identity/hash/time/reason when provenance/audit policy requires it.

A tombstone MUST NOT contain the prohibited deleted payload.

---

# 6. Knowledge validity

## 6.1 Current applicability

Knowledge is eligible for current retrieval only when all of the following hold:

- its lifecycle permits current use;
- the current time falls within the validity interval, or policy permits an open/unknown interval;
- its conflict state does not prohibit unqualified current use;
- its evidence/provenance remains admissible;
- authorization/governance permits disclosure.

## 6.2 Supersession

Supersession SHOULD contain:

```text
superseded_ref
superseding_ref
reason
recorded_at
semantic_effective_at?
```

`semantic_effective_at` MAY differ from the time supersession was recorded.

## 6.3 Expiry

Expiry ends current applicability according to time/policy while preserving history.

---

# 7. Claim temporal semantics

## 7.1 Claim proposition vs assessment

A claim's semantic content is separate from assessment of its support.

The same assertion may have a sequence of assessments over time without changing its proposition.

## 7.2 Claim validity

A claim MAY have its own validity interval, distinct from the timing of its supporting evidence.

## 7.3 Contest timing

A claim becomes `Contested` when an admissible contradiction is authoritatively recorded, unless policy defines another deterministic threshold.

A historical `as_of` query before the contradiction was recorded MAY show the claim as supported, even when more is known today.

This distinguishes:

- **what was true/valid then**;
- **what Vestrace knew then**;
- **what Vestrace knows now about then**.

Query policy defines the full bitemporal interpretation, but the system MUST retain the data needed to make that distinction.

---

# 8. Temporal perspectives

## 8.1 `Current`

Returns current effective cognitive state using current lifecycle, conflict, and policy semantics.

## 8.2 `AsOf`

`AsOf(t)` must explicitly identify its semantic mode:

### `KNOWN_AS_OF`

What Vestrace authoritatively knew by `recorded_at <= t`.

### `VALID_AS_OF`

What applies at domain time `t` according to the state available to the query.

### `RECONSTRUCTED_AS_OF`

A present-day reconstruction of domain-world state at time `t`, which MAY use evidence obtained after `t`.

These modes MUST NOT be silently conflated.

## 8.3 `Timeline`

Returns ordered changes, claims, and evidence with an explicit ordering axis.

## 8.4 `AllHistory`

May include superseded, rejected, expired, or contested history, but MUST label lifecycle and current applicability.

---

# 9. Ordering and causality

## 9.1 Aggregate sequence

An authoritative stream/aggregate uses a monotonic sequence/version.

## 9.2 Correlation and causation

Where possible, facts retain:

- correlation id;
- causation id;
- parent execution/operation reference.

## 9.3 Clock order is not causal order

Given two independent events with wall-clock timestamps, Vestrace MUST NOT infer causality solely from timestamp comparison.

## 9.4 Concurrent facts

Concurrent/independent facts MAY remain partially ordered.

Reconciliation must handle the absence of a total order rather than fabricate one.

---

# 10. Late-arriving evidence

## 10.1 Ingestion after occurrence

An event that occurred earlier MAY be recorded later.

```text
occurred_at = T1
recorded_at = T3
```

## 10.2 Effect on current state

Late evidence MAY trigger:

- claim re-assessment;
- conflict;
- historical correction;
- derived projection invalidation;

but MUST NOT rewrite the fact that the system did not know that evidence before T3.

## 10.3 Historical reconstruction

A reconstructed historical query MAY consider late evidence, but MUST identify the corresponding temporal mode.

---

# 11. Retractions and corrections

A source MAY retract or correct previously supplied evidence.

Vestrace records a new evidence event/relationship instead of deleting the original history by default.

Claim assessment/reconciliation is then recomputed separately.

---

# 12. Temporal conflicts

Typical temporal conflicts:

- overlapping mutually exclusive validity intervals;
- contradictory values for same semantic key/time;
- event order inconsistent with declared causal chain;
- source correction with backdated validity;
- stale revision mutation.

A temporal conflict is a first-class conflict/finding under the appropriate domain owner.

---

# 13. Retrieval time rules

## 13.1 Candidate filtering

Retrieval MUST apply the requested temporal perspective before final current-truth assembly.

## 13.2 Ranking

Recency is a ranking signal, not an authority rule.

A newer item does not automatically prevail semantically over a more authoritative or better-supported item.

## 13.3 ContextPack temporal metadata

A ContextPack SHOULD retain:

- temporal perspective;
- effective query time;
- included revision validity;
- relevant conflict/supersession markers;
- generation/state refs.

---

# 14. Execution temporal semantics

The execution domain uses separate timestamps:

- issued/created;
- started;
- waiting/resumed;
- dispatched;
- acknowledged;
- completed;
- recorded.

An external-effect outcome MAY remain unknown after dispatch even when the request timestamp is known.

---

# 15. Repair and temporal preconditions

RepairPlan contains input state/version and expiry.

Elapsed time or a state mutation MAY make a plan stale without changing the finding fingerprint.

Preconditions MUST be rechecked before execution.

---

# 16. Sharing temporal semantics

MemoryShareGrant and MemoryMount have their own validity/lifecycle intervals.

Access is permitted only when:

- grant active and valid;
- exact revision accepted;
- mount active/not stale;
- target/source policies current enough according to policy;
- source content still available.

The historical fact of using memory through a previously valid grant remains after revocation.

---

# 17. Policy temporal semantics

Policy decisions always bind an exact policy version.

A new policy version applies prospectively unless a separate migration/revalidation policy requires revisiting existing state.

A historical operation is assessed against the policy applied at decision time, and MAY receive a current compliance finding without rewriting history.

---

# 18. Data lifecycle time

A retention clock MUST have an explicit trigger:

- created;
- last used;
- execution closed;
- workspace closed;
- policy event;
- explicit date.

Expiry and physical disposal may occur at different times.

DataHold temporarily blocks disposal without changing the original retention facts.

---

# 19. Recovery temporal semantics

Recovery SHOULD preserve:

- source recovery point;
- snapshot position;
- replay range;
- rebuild generation;
- revalidation time.

After restoration, the current wall clock is not a substitute for lost event ordering.

---

# 20. Generation counters

Derived systems MAY use monotonic generations:

- memory generation;
- policy generation;
- share generation;
- retrieval/index generation;
- classification generation.

A generation supports invalidation; it is neither semantic time nor a substitute for revision identity.

---

# 21. Normative mappings

Principal requirements:

- `TMP-001..TMP-010`;
- `MEM-007..MEM-019`;
- `MUT-001..MUT-008`;
- `RET-002`, `RET-012`, `RET-013`;
- `HLT-009`;
- `IDW-008`, `IDW-013`;
- `GOV-013`, `GOV-025`.

---

# 22. Forbidden shortcuts

```text
occurred_at == recorded_at
recorded_at == valid_from
newest timestamp == truth
newest timestamp == causal successor
current projection == historical state
expiry == deletion
revoke == erase history
state generation == semantic version
lease timestamp == authority
```

---

# 23. Completion criteria

The Data & Temporal Model is complete when:

1. The Retrieval specification uses explicit temporal perspectives.
2. Domain schemas distinguish occurrence, recording, validity, and revision time.
3. Reconciliation does not depend on a universal latest-wins rule.
4. Policy, sharing, and retention lifecycles use exact validity semantics.
5. The qualification suite contains late-arriving evidence, concurrent revisions, and `as_of` golden scenarios.
