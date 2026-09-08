# Vestrace Health / Repair / Incident Contract v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-health-repair-incident-contract-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative integrity and recovery specification
**Date:** 2026-08-10

## 1. Purpose

This document defines how Vestrace detects invariant violations, represents health, performs safe repair, manages incidents, and restores trust after recovery.

Main principle:

> **Repair restores deterministic integrity. Reconciliation resolves semantic ambiguity. Recovery reconstructs state. Revalidation restores evidence-backed trust. These are distinct operations.**

---

# 2. Findings-first health

## 2.1 HealthFinding

Primary diagnostic unit:

```text
HealthFinding
├─ finding_id
├─ invariant_id
├─ fingerprint
├─ category
├─ severity
├─ impact
├─ scope
├─ evidence_refs[]
├─ repairability
├─ lifecycle_status
├─ first_seen_at
└─ last_seen_at
```

Aggregate `HealthStatus` is a projection.

## 2.2 Severity

```text
INFO
LOW
MEDIUM
HIGH
CRITICAL
```

## 2.3 Repairability

```text
NONE
DETERMINISTIC
POLICY_GATED
HUMAN_REQUIRED
```

Severity and repair risk are independent.

---

# 3. Invariant registry

## 3.1 InvariantDefinition

```text
InvariantDefinition
├─ invariant_id
├─ domain
├─ scope_kind
├─ description
├─ default_severity
├─ checker_kind
├─ repair_strategy
├─ check_mode
├─ cost_class
├─ max_staleness?
└─ version
```

Typed checkers implement complex logic; the declarative registry stores metadata/versioning.

## 3.2 Check modes

```text
INLINE
REACTIVE
SWEEP
MULTI
```

## 3.3 Cost classes

```text
CHEAP
MODERATE
EXPENSIVE
```

Not every invariant is checked on every write.

---

# 4. Check scheduling

## 4.1 Inline guards

Before or during commit, check inexpensive invariants whose violation must not enter canonical state.

Examples:

- identity/workspace consistency;
- revision/version precondition;
- structural schema validity;
- mandatory ownership/provenance references.

## 4.2 Reactive checks

After mutation, reconciliation, import, rebuild, or recovery, run targeted checks on the relevant scope.

## 4.3 Background sweeps

Periodic checks look for latent drift:

- CAS mismatch;
- orphan refs;
- index drift;
- cross-workspace stale mounts;
- retention violations;
- long-term temporal anomalies.

---

# 5. Finding fingerprint and occurrence

Calculate a stable fingerprint deterministically from:

```text
invariant_id
+ normalized scope
+ affected resource class
+ defect signature
```

LLM-generated fingerprint forbidden as canonical dedup key.

Record every episode as a `HealthOccurrence`.

Recurrence projection:

```text
ONE_OFF
RECURRENT
FLAPPING
PERSISTENT
```

---

# 6. Repair autonomy

Tiered repair:

```text
DETERMINISTIC + low allowed risk
→ may auto-repair

POLICY_GATED
→ policy/capability/approval

HUMAN_REQUIRED
→ proposal only

NONE
→ incident/escalation
```

Principal rule:

> Vestrace may automatically repair only what is deterministically reconstructible from a more authoritative layer.

---

# 7. Repair lifecycle

```text
detect
→ diagnose
→ propose
→ authorize
→ execute
→ verify
→ close/reopen
```

A checker performs no hidden write.

---

# 8. RepairPlan

Immutable plan:

```text
RepairPlan
├─ plan_id
├─ finding_ids[]
├─ scope
├─ input_state_ref
├─ finding_fingerprint
├─ preconditions[]
├─ operations[]
├─ expected_postconditions[]
├─ verification_checks[]
├─ verification_scope
├─ risk
├─ reversibility
├─ required_capability
├─ created_at
└─ expires_at
```

A material change to a plan creates a new plan.

---

# 9. Reversibility

Repair classification:

```text
REVERSIBLE
REBUILDABLE
IRREVERSIBLE
```

- `REVERSIBLE` — a valid inverse exists;
- `REBUILDABLE` — derived state can be regenerated from authoritative sources;
- `IRREVERSIBLE` — stronger governance is required.

---

# 10. RepairExecution

```text
RepairExecution
├─ execution_id
├─ plan_id
├─ actor
├─ capability/policy refs
├─ status
├─ started_at
├─ completed_at?
├─ applied_operations[]
├─ execution_evidence[]
└─ failure?
```

Statuses:

```text
PLANNED
AUTHORIZED
RUNNING
VERIFYING
SUCCEEDED
FAILED
PARTIALLY_APPLIED
ABORTED
STALE_PLAN
```

Execution uses Vestrace's existing execution runtime.

---

# 11. Repair transaction boundaries

## 11.1 Local atomic repair

When a repair fits entirely within one authoritative database transaction:

```text
BEGIN
  recheck preconditions
  apply mutation
  check immediate postconditions
COMMIT
```

Failure => rollback.

## 11.2 Composite repair

CAS, the database, and external/derived systems use recoverable idempotent steps, not a fictitious distributed ACID transaction.

Every completed step is recorded with sufficient durability for resume/reconciliation.

---

# 12. Stale plan protection

Recheck input state and preconditions before execution.

When state changes:

```text
AUTHORIZED
→ precondition mismatch
→ STALE_PLAN
```

Do not execute the old plan; the planner creates another if repair is still needed.

---

# 13. Repair concurrency

Conflicting repair executions use leases, fencing, and version preconditions.

A lease does not replace authority or waive precondition checks.

A repair step SHOULD have an idempotency key and a restartability profile.

---

# 14. Repair verification

Repair execution success and finding resolution are separate.

```text
RepairExecution
→ Targeted Verification
→ Invariant Re-check
→ Collateral Checks
→ Closure Decision
```

Verification levels:

```text
DIRECT
NEIGHBORHOOD
DOMAIN
```

Higher repair risk requires broader collateral verification according to policy.

---

# 15. Finding lifecycle

```text
OPEN
ACKNOWLEDGED
REPAIR_PLANNED
REPAIRING
VERIFYING
RESOLVED
SUPPRESSED
ACCEPTED_RISK
REOPENED
```

Only closure logic based on verification evidence can assign `RESOLVED`.

When execution succeeds but verification fails:

```text
EXECUTION_SUCCEEDED
VERIFICATION_FAILED
→ finding remains/reopens
```

---

# 16. Flapping and repair budgets

Automatic repair has:

```text
max_attempts_per_window
cooldown
max_cumulative_risk
```

On repeated recurrence:

```text
FLAPPING
→ automatic repair suspended
→ root-cause escalation
```

An additional finding MAY be `REPAIR_LOOP_DETECTED`.

---

# 17. Finding relations

Permitted relationships:

```text
caused_by
contributes_to
symptom_of
correlated_with
```

The relationship's source MUST be marked as:

```text
DETERMINISTIC
HEURISTIC
OPERATOR
```

Heuristic causal inference does not become a canonical fact without the corresponding authority/evidence.

---

# 18. Health scope and propagation

Health can be aggregated:

```text
Resource
→ Domain/Subsystem
→ Workspace
→ Installation
```

Propagation follows only the declared dependency graph.

Aggregate health states:

```text
HEALTHY
DEGRADED
UNHEALTHY
CRITICAL
UNKNOWN
```

`UNKNOWN != HEALTHY`.

HealthSnapshot SHOULD include coverage/freshness so a stale absence of findings does not appear healthy.

---

# 19. Suppression / AcceptedRisk / MaintenanceWindow

These objects are separate from the truth of a finding.

```text
AcceptedRisk
├─ finding/scope
├─ reason
├─ actor/authority
├─ created_at
├─ expires_at
└─ policy_ref
```

Suppression may affect operational routing/alerting without deleting the raw finding.

Indefinite ignoring without a separate governance policy is not a safe default.

---

# 20. Incident creation

Create an Incident when a problem requires coordinated response, for example:

- critical invariant failure;
- multi-domain impact;
- recovery workflow;
- unknown critical external effect;
- trust boundary violation;
- crypto integrity incident;
- failed/repeating repair;
- broad corruption.

A finding may exist without an Incident.

---

# 21. Incident model

```text
Incident
├─ incident_id
├─ type
├─ severity
├─ scope
├─ triggering_findings[]
├─ affected_resources[]
├─ opened_at
├─ status
├─ containment_state
├─ recovery_state
└─ revalidation_state
```

Lifecycle:

```text
OPEN
→ CONTAINING
→ CONTAINED
→ RECOVERING
→ REVALIDATING
→ RESOLVED
→ CLOSED
```

`RESOLVED` = technical restoration.  
`CLOSED` = review/audit/post-incident actions complete.

---

# 22. Containment

Containment aims to stop propagation before complete repair.

Actions MAY include:

- freeze writes;
- revoke/suspend capabilities;
- isolate workspace/domain;
- pause workers;
- stop external effects;
- revoke leases;
- quarantine resources;
- force read-only.

Containment SHOULD be the minimum sufficient response, not a global shutdown by default.

---

# 23. Trust states

```text
TRUSTED
DEGRADED_TRUST
UNTRUSTED
REVALIDATING
```

Availability/readiness and trust are distinct.

```text
AVAILABLE ≠ HEALTHY
HEALTHY ≠ TRUSTED
RECOVERED ≠ REVALIDATED
```

---

# 24. Startup crash recovery

After restart, a recovery scan looks for:

- RUNNING executions;
- DISPATCHING external effects;
- VERIFYING repairs;
- stale leases/locks;
- unfinished workflows/transactions;
- orphan temporary state;
- UNKNOWN outcomes.

Each object is classified as:

```text
SAFE_TO_RESUME
SAFE_TO_RETRY
MUST_RECONCILE
MUST_ABORT
HUMAN_REQUIRED
```

Resume-all prohibited.

---

# 25. RecoveryPoint

```text
RecoveryPoint
├─ state_ref
├─ event/sequence position
├─ consistency_scope
├─ integrity_status
├─ provenance
└─ created_at
```

RecoveryPoint is a demonstrably consistent point, not merely a backup timestamp.

---

# 26. Restore pipeline

```text
validated snapshot
+
canonical history tail
→ replay
→ derived state rebuild
→ revalidation
```

Snapshot validation includes:

- integrity/hash;
- format/schema compatibility;
- event continuity;
- dependency/scope consistency.

---

# 27. Partial recovery

Restore trust per scope:

```text
workspace A → TRUSTED
workspace B → REVALIDATING
workspace C → UNTRUSTED
```

The capability engine accounts for the trust state relevant to each operation.

---

# 28. RevalidationRun

```text
RevalidationRun
├─ run_id
├─ incident_id?
├─ scope
├─ level
├─ baseline_state_ref
├─ checks[]
├─ evidence_refs[]
├─ result
└─ completed_at
```

Levels:

```text
LOCAL
DOMAIN
WORKSPACE
INSTALLATION
FULL_TRUST
```

Results:

```text
PASSED
PASSED_WITH_DEGRADATION
FAILED
INCONCLUSIVE
```

`INCONCLUSIVE` does not increase trust to TRUSTED.

---

# 29. Revalidation pipeline

Recommended stages:

```text
Structural Validation
→ Integrity Validation
→ Execution Reconciliation
→ External Effect Reconciliation
→ Derived State Validation
→ Security/Capability Validation
→ Cognitive State Validation
```

Every stage produces evidence.

---

# 30. Trust transition barrier

```text
Recovery complete
→ Revalidation PASSED
→ policy evaluates evidence
→ trust transition
→ capabilities progressively restored
```

An administrative command cannot directly replace this barrier.

---

# 31. Divergent histories

Competing histories produce an explicit `HISTORY_DIVERGENCE` finding/incident.

Flow:

```text
branch/history A
branch/history B
→ deterministic comparison
→ domain reconciliation policy
→ human required if not provable
```

Universal latest-wins prohibited.

---

# 32. Corruption classes

```text
LOGICAL_CORRUPTION
PHYSICAL_CORRUPTION
DERIVED_CORRUPTION
CRYPTO_INTEGRITY_FAILURE
```

Recovery strategy depends on the failure class and authority layer.

Derived corruption SHOULD repair by rebuild. Canonical logical corruption MAY require reconciliation/recovery rather than repair.

---

# 33. UNKNOWN external effects after crash

`DISPATCHING` without conclusive receipt becomes UNKNOWN/MUST_RECONCILE.

Related unsafe operations MAY be blocked until resolution.

For example: an unknown payment outcome means no automatic duplicate payment.

---

# 34. Recovery budgets

Recovery MAY limit:

- retry count;
- rebuild cost;
- external call count;
- parallelism;
- time/deadline.

Repeated failure produces `RECOVERY_LOOP_DETECTED` and stops the automatic loop.

---

# 35. Evidence preservation

Before destructive recovery, the following SHOULD be preserved:

- relevant state refs;
- hashes;
- logs/events required for diagnosis;
- finding evidence;
- external receipts;
- snapshot/sequence positions.

Recovery should preserve the ability to investigate the root cause, except where privacy/data policy requires removing content; in that case retain permissible content-free proofs.

---

# 36. Progressive capability restoration

Possible stages:

```text
1. diagnostics / read-only
2. internal deterministic writes
3. semantic mutation
4. reversible external effects
5. irreversible/high-risk external effects
```

Policy MAY choose another ordering, but restoration must account for evidence and trust.

---

# 37. Incident review

After technical resolution, create a structured `IncidentReview` containing:

- root causes;
- contributing factors;
- failed invariants;
- detection gaps;
- repair/recovery actions;
- recurrence prevention;
- proposed new checks/policy changes.

Review conclusions are derived or author-attributed knowledge, not automatically canonical truth.

---

# 38. Operator interface contract

`doctor` is read-only by default.

Conceptual CLI:

```text
vestrace doctor
vestrace doctor --plan
vestrace repair <plan>
```

`doctor --plan` MAY create a RepairPlan but does not secretly execute repair.

The actual command surface is specified later; the semantic boundary is normative.

---

# 39. Normative mappings

Principal IDs:

- `HLT-001..020`;
- `REC-001..018`;
- `ARC-001..007`;
- `EXT-006..010`;
- `CAP-010..012`.

---

# 40. Golden scenarios

## 40.1 Missing retrieval projection

```text
canonical Memory healthy
→ projection rows missing
→ finding
→ deterministic RepairPlan
→ rebuild missing projection
→ verification coverage=100%
→ RESOLVED
```

Canonical Memory unchanged.

## 40.2 Repair became stale

```text
finding detected
→ plan built on state S1
→ another mutation creates S2
→ repair precondition fails
→ STALE_PLAN
```

No old repair mutation.

## 40.3 Repair flapping

```text
repair
→ resolved
→ recurrence
→ repair
→ recurrence
→ budget exhausted
→ FLAPPING
→ auto-repair suspended
```

## 40.4 Crash after external dispatch

```text
dispatch
→ crash
→ restart
→ UNKNOWN
→ reconcile
→ revalidate affected scope
```

## 40.5 Snapshot restore

```text
restore validated snapshot
→ replay tail
→ rebuild derived indexes
→ revalidate workspace
→ TRUSTED only after PASSED
```

---

# 41. Forbidden shortcuts

```text
no active alert == HEALTHY
UNKNOWN == HEALTHY
repair execution success == resolved
operator accepts risk == trusted
process restarted == recovered
recovered == revalidated
latest history == correct history
checker == repair executor
repair == semantic reconciliation
```

---

# 42. Completion criteria

The document is complete when:

1. Invariant-registry requirements are reflected in the qualification specification.
2. All specialized repairs use RepairPlan/Verification semantics.
3. Incident/recovery scenarios include external UNKNOWN outcomes.
4. Trust transitions reference the Trust & Authority Model.
5. Doctor/operator documentation does not promise hidden automatic fixes.
6. Existing diagnostic documentation distinguishes implementation state from the target contract.
