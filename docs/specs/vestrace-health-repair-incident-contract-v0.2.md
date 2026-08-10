# Vestrace Health / Repair / Incident Contract v0.2

**Статус:** normative integrity and recovery specification  
**Дата:** 2026-08-10

## 1. Назначение

Этот документ определяет, как Vestrace обнаруживает нарушения инвариантов, представляет health, выполняет безопасный repair, управляет incidents и возвращает trust после recovery.

Главный принцип:

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

Aggregate `HealthStatus` является projection.

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

Severity и repair risk независимы.

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

Complex logic реализуется typed checker, а declarative registry хранит metadata/versioning.

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

Не каждый invariant проверяется на каждом write.

---

# 4. Check scheduling

## 4.1 Inline guards

До/внутри commit проверяются дешёвые invariants, нарушение которых нельзя допустить в canonical state.

Примеры:

- identity/workspace consistency;
- revision/version precondition;
- structural schema validity;
- mandatory ownership/provenance references.

## 4.2 Reactive checks

После mutation/reconciliation/import/rebuild/recovery выполняются targeted checks соответствующего scope.

## 4.3 Background sweeps

Периодические проверки ищут latent drift:

- CAS mismatch;
- orphan refs;
- index drift;
- cross-workspace stale mounts;
- retention violations;
- long-term temporal anomalies.

---

# 5. Finding fingerprint and occurrence

Stable fingerprint вычисляется детерминированно из:

```text
invariant_id
+ normalized scope
+ affected resource class
+ defect signature
```

LLM-generated fingerprint forbidden as canonical dedup key.

Каждый episode сохраняется как `HealthOccurrence`.

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

Главное правило:

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

Checker не выполняет hidden write.

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

Material plan change создаёт новый plan.

---

# 9. Reversibility

Repair classification:

```text
REVERSIBLE
REBUILDABLE
IRREVERSIBLE
```

- `REVERSIBLE` — существует корректный inverse;
- `REBUILDABLE` — derived state можно заново получить из authoritative source;
- `IRREVERSIBLE` — требуется усиленная governance path.

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

Execution использует existing Vestrace execution runtime.

---

# 11. Repair transaction boundaries

## 11.1 Local atomic repair

Если repair полностью помещается в одну authoritative DB transaction:

```text
BEGIN
  recheck preconditions
  apply mutation
  check immediate postconditions
COMMIT
```

Failure => rollback.

## 11.2 Composite repair

CAS + DB + external/derived systems используют recoverable idempotent steps, а не ложный distributed ACID.

Каждый completed step фиксируется durable enough для resume/reconciliation.

---

# 12. Stale plan protection

Перед execution input state/preconditions проверяются снова.

Если state изменился:

```text
AUTHORIZED
→ precondition mismatch
→ STALE_PLAN
```

Старый plan не исполняется; planner создаёт новый, если repair всё ещё нужен.

---

# 13. Repair concurrency

Для конфликтующих repair executions используются lease/fencing/version preconditions.

Lease не заменяет authority и не отменяет precondition checking.

Repair step SHOULD иметь idempotency key и restartability profile.

---

# 14. Repair verification

Repair execution success и finding resolution разделены.

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

Higher repair risk требует более широкого collateral verification согласно policy.

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

`RESOLVED` может выставляться только closure logic на основе verification evidence.

Если execution succeeded, но verification failed:

```text
EXECUTION_SUCCEEDED
VERIFICATION_FAILED
→ finding remains/reopens
```

---

# 16. Flapping and repair budgets

Auto-repair имеет:

```text
max_attempts_per_window
cooldown
max_cumulative_risk
```

При repeated recurrence:

```text
FLAPPING
→ automatic repair suspended
→ root-cause escalation
```

Дополнительный finding MAY быть `REPAIR_LOOP_DETECTED`.

---

# 17. Finding relations

Допустимые relations:

```text
caused_by
contributes_to
symptom_of
correlated_with
```

Relation source MUST быть marked:

```text
DETERMINISTIC
HEURISTIC
OPERATOR
```

Heuristic causal inference не становится canonical fact без соответствующей authority/evidence.

---

# 18. Health scope and propagation

Health может агрегироваться:

```text
Resource
→ Domain/Subsystem
→ Workspace
→ Installation
```

Propagation только через declared dependency graph.

Aggregate health states:

```text
HEALTHY
DEGRADED
UNHEALTHY
CRITICAL
UNKNOWN
```

`UNKNOWN != HEALTHY`.

HealthSnapshot SHOULD содержать coverage/freshness, чтобы stale absence of findings не выглядело healthy.

---

# 19. Suppression / AcceptedRisk / MaintenanceWindow

Эти objects separate from finding truth.

```text
AcceptedRisk
├─ finding/scope
├─ reason
├─ actor/authority
├─ created_at
├─ expires_at
└─ policy_ref
```

Suppression может влиять на operational routing/alerting, но не удаляет raw finding.

Бессрочное ignore без отдельной governance policy не является safe default.

---

# 20. Incident creation

Incident создаётся, когда проблема требует coordinated response, например:

- critical invariant failure;
- multi-domain impact;
- recovery workflow;
- unknown critical external effect;
- trust boundary violation;
- crypto integrity incident;
- failed/repeating repair;
- broad corruption.

Finding может существовать без Incident.

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

Containment цель — остановить propagation до полного repair.

Actions MAY включать:

- freeze writes;
- revoke/suspend capabilities;
- isolate workspace/domain;
- pause workers;
- stop external effects;
- revoke leases;
- quarantine resources;
- force read-only.

Containment SHOULD быть минимально достаточным, а не глобальным shutdown по умолчанию.

---

# 23. Trust states

```text
TRUSTED
DEGRADED_TRUST
UNTRUSTED
REVALIDATING
```

Availability/readiness и trust различны.

```text
AVAILABLE ≠ HEALTHY
HEALTHY ≠ TRUSTED
RECOVERED ≠ REVALIDATED
```

---

# 24. Startup crash recovery

После restart recovery scan ищет:

- RUNNING executions;
- DISPATCHING external effects;
- VERIFYING repairs;
- stale leases/locks;
- unfinished workflows/transactions;
- orphan temporary state;
- UNKNOWN outcomes.

Каждый object классифицируется:

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

RecoveryPoint — доказанно согласованная точка, а не просто backup timestamp.

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

Snapshot validation включает:

- integrity/hash;
- format/schema compatibility;
- event continuity;
- dependency/scope consistency.

---

# 27. Partial recovery

Trust восстанавливается per scope:

```text
workspace A → TRUSTED
workspace B → REVALIDATING
workspace C → UNTRUSTED
```

Capability engine учитывает trust state каждой операции.

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

`INCONCLUSIVE` не повышает trust до TRUSTED.

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

Каждый stage выдаёт evidence.

---

# 30. Trust transition barrier

```text
Recovery complete
→ Revalidation PASSED
→ policy evaluates evidence
→ trust transition
→ capabilities progressively restored
```

Admin command не может напрямую заменить этот barrier.

---

# 31. Divergent histories

Competing histories получают explicit `HISTORY_DIVERGENCE` finding/incident.

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

Recovery strategy зависит от class/authority layer.

Derived corruption SHOULD repair by rebuild. Canonical logical corruption MAY require reconciliation/recovery rather than repair.

---

# 33. UNKNOWN external effects after crash

`DISPATCHING` without conclusive receipt becomes UNKNOWN/MUST_RECONCILE.

Связанные unsafe operations MAY быть blocked до resolution.

Пример: unknown payment => no automatic duplicate payment.

---

# 34. Recovery budgets

Recovery MAY ограничивать:

- retry count;
- rebuild cost;
- external call count;
- parallelism;
- time/deadline.

Repeated failure => `RECOVERY_LOOP_DETECTED` и stop automatic loop.

---

# 35. Evidence preservation

Перед destructive recovery SHOULD сохраняться:

- relevant state refs;
- hashes;
- logs/events required for diagnosis;
- finding evidence;
- external receipts;
- snapshot/sequence positions.

Recovery не должна стирать возможность расследовать root cause, кроме случаев, где privacy/data policy требует удаления content; тогда сохраняются допустимые content-free proofs.

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

Policy MAY выбрать иной ordering, но restoration должна быть evidence/trust aware.

---

# 37. Incident review

После technical resolution создаётся structured `IncidentReview` с:

- root causes;
- contributing factors;
- failed invariants;
- detection gaps;
- repair/recovery actions;
- recurrence prevention;
- proposed new checks/policy changes.

Review conclusions являются derived/author-attributed knowledge, а не автоматически canonical truth.

---

# 38. Operator interface contract

`doctor` по умолчанию read-only.

Conceptual CLI:

```text
vestrace doctor
vestrace doctor --plan
vestrace repair <plan>
```

`doctor --plan` MAY создавать RepairPlan, но не исполняет repair скрыто.

Фактическая command surface уточняется позже; semantic boundary является нормативной.

---

# 39. Normative mappings

Основные IDs:

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

Документ считается полным, когда:

1. invariant registry requirements отражены в qualification spec;
2. RepairPlan/Verification semantics используются во всех specialized repairs;
3. external UNKNOWN outcome входит в incident/recovery scenarios;
4. trust transitions ссылаются на Trust & Authority Model;
5. doctor/operator docs не обещают скрытый auto-fix;
6. existing diagnostics documentation будет помечена implementation-state отдельно от target contract.
