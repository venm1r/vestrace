# Vestrace Architecture Contract v0.2

> **English reading edition · 2026-09-08.** Complete editorial translation of the [frozen original](../vestrace-architecture-contract-v0.2.md) from supplied snapshot `3e05dfbd`. Requirement IDs, normative strength, technical states, and examples are preserved. This translation does not amend the original contract or claim implementation. If wording differs, the frozen original and applicable Accepted ADRs take precedence.

**Status:** Normative architecture baseline
**Date:** 2026-08-10
**Branch:** `docs/architecture-v0.2`
**Repository:** `venm1r/vestrace`
**Purpose:** Define Vestrace's target architecture before the next implementation phase.

> This is an architecture contract, not a statement of current implementation status. The presence of an entity, invariant, or workflow here does not mean it is implemented or available through HTTP, MCP, CLI, or the worker runtime.

## 1. Product definition

Canonical definition:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

Canonical formula:

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

Vestrace stores long-lived cognitive state, provenance, temporal history, execution evidence, and governed cognitive assets so that different agents and executions can safely continue from a shared, verifiable foundation.

Vestrace is not merely a vector database, prompt-memory library, workflow engine, agent framework, or event store. These mechanisms may exist inside the system, but they do not define the product.

## 2. Normative language

Vestrace documentation uses the following normative terms:

- **MUST / MUST NOT** — a mandatory requirement;
- **SHOULD / SHOULD NOT** — a requirement from which deviation requires a documented reason;
- **MAY** — a permitted option.

Specialized documents may refine this contract, but MUST NOT weaken it without a separate ADR or a new Architecture Contract version.

## 3. Global architecture laws

### 3.1 Authority before projection

Vestrace MUST distinguish authoritative/canonical state from derived state.

```text
authoritative source
        ↓
canonical state
        ↓
derived state
        ↓
indexes / caches / projections
```

Derived state MUST be removable and rebuildable from a more authoritative layer. A less authoritative layer MUST NOT automatically rewrite a more authoritative layer merely to restore local consistency.

### 3.2 No competing runtimes

Vestrace MUST have a single execution boundary. Repair, recovery, governance operations, external effects, and qualification workflows MUST use the existing execution, policy, capability, audit, and persistence contracts.

The following require a separate architectural revision and are otherwise prohibited:

- a second orchestration runtime;
- a second event store;
- a parallel generic `Task → Action → Attempt` runtime;
- a separate Repair Runtime;
- a separate Incident Runtime;
- a separate Governance Runtime.

`Vestrace State Engine` is an internal name for the durable state/execution boundary, not a separate product or service.

### 3.3 History is corrected, not rewritten

Once recorded, significant domain facts, revisions, execution facts, audit facts, and external-effect receipts MUST NOT be edited as though the original event never happened.

Corrections use a new revision, supersession, compensation, reconciliation, or recovery fact.

### 3.4 Evidence before trust

`HEALTHY`, `RESOLVED`, `CONFIRMED`, `TRUSTED`, and qualification `PASSED` states MUST arise from verifiable evidence, not merely the absence of an error or an administrative flag.

### 3.5 Fail closed on ambiguity

Uncertainty MUST be a first-class state. Vestrace MUST NOT automatically turn `UNKNOWN` into `FAILED`, `HEALTHY`, `TRUSTED`, or permission to retry.

---

# Block 1 — Persistent Cognition Core

## 1.1 Cognitive substrate

Persistent cognition is organized around verifiable long-lived state, not the accumulation of text fragments.

The cognitive layer MUST distinguish at least:

- source evidence / originating facts;
- persistent memory identity and immutable revisions;
- semantic claims/structured assertions where an explicit assertion is required;
- provenance and derivation;
- conflicts and supersession;
- derived summaries, retrieval representations, and projections.

The specialized Domain Model v0.2 defines exact aggregates and relationship cardinalities.

## 1.2 Stable identity, immutable revisions

A persistent object SHOULD have a stable identity, and a content change MUST create a new immutable revision.

Earlier state remains available as history; a new current revision does not erase it.

## 1.3 Provenance

Every persistent cognitive conclusion not obtained directly from a person/source MUST have derivation/provenance linking it to its original inputs and the method used.

Provenance SHOULD identify:

- the source;
- the author/actor;
- the time;
- execution/model/tool reference;
- transformation/derivation method;
- the policy/version context when it affects the result.

## 1.4 Cognitive assets are not ordinary memories

Agents, skills, workflows, model/provider profiles, and other versioned cognitive definitions MUST remain separate governed assets. They MAY use memory/evidence, but MUST NOT masquerade as ordinary memories.

## 1.5 Derived cognition

Summaries, embeddings, search documents, retrieval projections, caches, and aggregated metrics are derived state and MUST be rebuildable.

---

# Block 2 — Temporal & Concurrency

## 2.1 Multiple time dimensions

Vestrace MUST distinguish when a fact occurred from when it was recorded.

At minimum, the model supports:

- `occurred_at` — when the fact is believed to have occurred;
- `recorded_at` — when Vestrace authoritatively recorded the fact;
- a validity interval (`valid_from` / `valid_until`) for knowledge that changes over time.

Missing or unreliable `occurred_at` MUST NOT prevent recording a fact when `recorded_at` is known.

## 2.2 Temporal queries

Cognitive retrieval MUST distinguish between:

- current state;
- state `as_of`;
- timeline;
- all history.

Superseded/expired knowledge MUST NOT be presented as current knowledge.

## 2.3 Optimistic concurrency

Canonical mutations MUST use version/revision preconditions. Silent last-write-wins lost updates are prohibited.

When expected state/revision does not match, the operation returns a conflict/stale result and MUST NOT automatically overwrite the newer state.

## 2.4 Lease does not equal authority

An operational lease/heartbeat MAY select the temporary worker, but MUST NOT by itself change logical version, ownership authority, or capability.

## 2.5 Ordering

Canonical append-only streams MUST have a verifiable order within their aggregate/scope. The system must not infer a global causal order solely from wall-clock timestamps unless that order is established.

---

# Block 3 — Mutation & Reconciliation

## 3.1 Mutation model

A significant change to persistent cognition MUST be represented as an explicit mutation with:

- actor;
- target;
- expected version/state;
- reason/intent;
- provenance;
- resulting revision/fact.

## 3.2 No hidden overwrite

Correction creates a new revision or assertion. Supersession preserves earlier knowledge as history. Deletion/expiry must not rewrite the past.

## 3.3 Conflict is explicit

When two admissible sources contradict each other and no deterministic selection rule exists, Vestrace MUST represent the conflict explicitly.

Universal heuristics such as `latest timestamp wins` are not allowed unless they are a policy of the specific domain and have sufficient supporting evidence.

## 3.4 Reconciliation classes

Reconciliation SHOULD distinguish:

- deterministic reconciliation — the result follows unambiguously from more authoritative state;
- policy-guided reconciliation — an explicit versioned policy authorizes the result;
- semantic reconciliation — interpretation of meaning/evidence is required;
- human-required reconciliation — automatic resolution is not permitted.

Semantic ambiguity MUST NOT be disguised as deterministic repair.

## 3.5 State Engine boundary

The State Engine uses existing authoritative aggregates, application ports, and execution runtime. It MUST NOT introduce a parallel execution hierarchy when the semantics already belong to Run/Step/model/tool/remote/human/verification contracts.

---

# Block 4 — Retrieval / ContextPack 2.0

## 4.1 Retrieval objective

Retrieval selects the smallest set of current, permitted, verifiable, and useful knowledge for the task, not simply the most similar text.

## 4.2 Security before ranking

Workspace, capability, classification, sharing, and policy filters MUST apply before denied content becomes available to ranking/model stages.

## 4.3 Retrieval request

The retrieval contract SHOULD include:

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

Candidate generation MAY use exact, full-text, vector, structured, graph, and execution-history channels. Incomparable channel scores SHOULD be combined through rank-based fusion, not arbitrary addition.

Failure of a derived channel MUST produce an accurately reported degraded mode when a safe deterministic retrieval path remains.

## 4.5 Current truth and conflicts

Current retrieval MUST exclude superseded/expired knowledge as current. Unresolved conflicts MUST be returned as conflicts, not established facts.

## 4.6 ContextPack

`ContextPack` is a governed, token-bounded, provenance-preserving context representation.

It MUST:

- respect a hard token/content budget;
- preserve source/revision references;
- explain inclusion;
- preserve warnings/degradation;
- not elevate the authority of included knowledge;
- respect classification and model destination policy.

Permitted representation levels include `Full`, `Summary`, `Atomic`, and `Reference`.

## 4.7 Cache validity

Context packs and retrieval caches are derived state. Changes to canonical memory, policy, sharing state, classification, or a relevant generation MUST invalidate stale representations.

---

# Block 5 — Execution Feedback & Learning

## 5.1 Evidence-backed feedback

Execution outcomes, deterministic checks, human feedback, downstream success, model/tool metrics, and failure evidence MAY inform system learning.

Every feedback signal MUST reference the exact execution/model/tool/policy revisions to which it applies.

## 5.2 Authority of evaluators

Deterministic evidence and explicitly authorized human evaluation take precedence over heuristic/model-judge signals.

An LLM judge MAY provide an additional signal, but MUST NOT automatically become authoritative truth.

## 5.3 Learning cannot silently self-modify authority

The learning pipeline MUST NOT automatically change permissions, capabilities, governance policies, or security boundaries.

Changes to agents, skills, workflows, routing policies, or persistent claims SHOULD follow a versioned proposal/mutation lifecycle with provenance, verification, and policy gates.

## 5.4 Raw evidence vs learned projection

Raw execution/evaluation facts MUST be stored separately from derived conclusions such as performance memory, learned preferences, or routing recommendations.

A derived conclusion MUST be revisable without destroying the original measurements.

---

# Block 6 — Capability Governance

## 6.1 Roles are templates; capabilities are authority

Roles are templates for granting permissions. Effective runtime authority comes from capabilities, not role names.

The default policy is deny.

## 6.2 Capability shape

A capability MUST support constraints on:

- tool/operation;
- resource/scope;
- validity/expiry;
- budget/quota;
- risk ceiling;
- conditions/obligations.

A capability identifier does not confer authority unless its constraints have been checked successfully.

## 6.3 Delegation / attenuation

A subagent receives only an explicitly delegated subset of its parent's effective authority and MAY receive additional restrictions.

```text
child authority ⊆ parent effective authority
```

Delegation depth MUST be bounded; the safe default is one level unless policy permits more.

## 6.4 Risk model

Baseline risk categories:

- `low`;
- `medium`;
- `high`;
- `critical`.

Context MAY raise effective risk. Lowering risk below an operation's intrinsic risk requires an explicit normative basis.

## 6.5 Policy decisions

Policy engine MAY:

- deny;
- allow;
- allow prepare-only;
- require approval;
- add obligations/limits.

Approval MUST NOT override explicit denial, a hard budget, or a capability ceiling.

## 6.6 Budgets

Governance SHOULD support typed budgets for time, steps, model tokens/cost, tool effects, artifacts, subruns, and other constrained resources.

Reservation/accounting MUST be auditable.

---

# Block 7 — Identity / Workspace / Federation

## 7.1 Workspace is an authority boundary

Every canonical object belongs to a defined workspace/authority scope. Workspace isolation is the safe default and SHOULD be protected by both application policy and storage-level controls, such as RLS.

`global` MUST NOT mean cross-workspace global.

## 7.2 Identity

Users, agents, service accounts, workflow/system actors, and remote/federated participants MUST have stable typed identities. Actor identity and granted authority are distinct concepts.

## 7.3 Cross-workspace sharing is explicit

Cross-workspace memory sharing MUST NOT expand the source Memory's scope.

Canonical flow:

```text
source workspace
  ↓ MemoryShareGrant (exact target)
target workspace
  ↓ explicit acceptance
MemoryMount (read-only by default)
```

`MemoryMount` is a permitted view of source memory, not local authority or a new source-of-truth copy.

## 7.4 Sharing constraints

Cross-workspace sharing MUST ensure:

- exact source + target;
- no wildcard grants;
- no implicit transitive sharing;
- source-side and target-side policy checks;
- explicit lifecycle revoke/suspend/expire;
- namespaced shared references;
- provenance preservation;
- separate permissions for discover/read/context/model-use/derive/export.

Revocation stops future access, but MUST NOT rewrite previously recorded disclosure history.

## 7.5 Federation trust is not data permission

Recognition of a remote identity, node, or federation relationship MUST NOT automatically permit data transfer. Each disclosure also requires capability, data policy, classification compatibility, and recipient constraints.

---

# Block 8 — Health / Integrity / Repair

## 8.1 Tiered Repair

Vestrace MAY automatically repair only what can be deterministically reconstructed from a more authoritative layer.

Lifecycle:

```text
detect → diagnose → propose → repair → verify
```

Semantic, destructive, security-sensitive, and ambiguous repair MUST pass policy/capability/approval checks or a human decision path.

## 8.2 Findings-first health model

`HealthFinding` is the primary diagnostic unit. Aggregate health status is a projection.

A finding MUST contain evidence, scope, invariant, severity, and repairability.

Severity:

`info / low / medium / high / critical`

Repairability:

`NONE / DETERMINISTIC / POLICY_GATED / HUMAN_REQUIRED`

Severity and repair risk MUST be assessed separately.

## 8.3 Hybrid invariant registry

The invariant catalog stores declarative metadata; complex checks run through typed checkers.

```text
InvariantDefinition → Typed Checker → Evidence → HealthFinding
```

Integrity MUST NOT become a universal DSL without a demonstrated need.

## 8.4 Hybrid scheduling

Checks run in three modes:

1. Inline guards for inexpensive critical invariants.
2. Reactive checks after relevant mutations/rebuild/import/recovery.
3. Background sweeps for latent drift/corruption.

Not every invariant is checked on every write path.

## 8.5 Repair Plan + transactional execution

A checker MUST NOT perform mutations directly.

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

RepairPlan records scope, preconditions, operations, expected postconditions, risk, reversibility, and required authority.

Reversibility:

`REVERSIBLE / REBUILDABLE / IRREVERSIBLE`

## 8.6 Verification pipeline

`RepairExecution = SUCCEEDED` does not imply `HealthFinding = RESOLVED`.

A verifier closes a finding after rechecking the original invariant and the necessary collateral checks.

`UNKNOWN` or failed verification MUST leave the finding open or reopen it.

## 8.7 Finding occurrence history

A logical problem and its individual occurrences are represented separately as `HealthFinding` and `HealthOccurrence`.

Recurrence projection:

`ONE_OFF / RECURRENT / FLAPPING / PERSISTENT`

Automatic repair MUST have an attempt budget/cooldown. Flapping MUST stop an endless repair loop.

## 8.8 Scope propagation

Health propagates only through declared dependencies. A local finding MUST NOT automatically make the entire installation `CRITICAL`.

Aggregate states:

`HEALTHY / DEGRADED / UNHEALTHY / CRITICAL / UNKNOWN`

`UNKNOWN != HEALTHY`.

## 8.9 Suppression and accepted risk

Suppression/AcceptedRisk MUST NOT remove a finding or turn raw integrity into healthy state. They are separate policy decisions with actor, reason, expiry, and audit.

## 8.10 Repair safety

RepairPlan binds specific input state and preconditions. When state changes, it becomes `STALE_PLAN` and MUST NOT execute.

Repair steps SHOULD be idempotent, restartable, and auditable. Conflicting repair executions use leases/fencing.

---

# Block 9 — External Effects

## 9.1 Intent before effect

An external action MUST have a persisted immutable `ExternalEffectIntent` before dispatch.

```text
plan → intent → policy/capability → dispatch → receipt → reconciliation
```

## 9.2 Prepare / Authorize / Commit

An external effect follows `PREPARED → AUTHORIZED → COMMITTED`. This is not a distributed ACID transaction; `COMMITTED` means dispatch to the external system.

## 9.3 Delivery semantics

The adapter MUST declare its semantics:

`AT_MOST_ONCE / AT_LEAST_ONCE / EFFECTIVELY_ONCE / UNKNOWN`

Vestrace MUST NOT promise universal exactly-once behavior.

## 9.4 Idempotency belongs to adapter contract

Retry policy follows the adapter's idempotency/reconciliation contract, not an agent's unrestricted decision.

## 9.5 Result is not outcome

The transport/API result and the actual external outcome are separate.

States include:

`NOT_STARTED / DISPATCHING / ACKNOWLEDGED / CONFIRMED / FAILED / UNKNOWN / RECONCILING`

A timeout MUST NOT automatically mean the effect did not occur.

## 9.6 Reconciliation

An `UNKNOWN` effect SHOULD be reconciled using the strongest available read-back, idempotency key, external resource ID/hash/version, or other provider-specific evidence.

## 9.7 Reversibility

External effects are classified as:

`REVERSIBLE / COMPENSATABLE / IRREVERSIBLE / UNKNOWN_REVERSIBILITY`

Compensation MUST NOT be called rollback and MUST preserve its link to the original effect.

## 9.8 Preconditions and TOCTOU

Recheck critical preconditions before dispatch. The adapter SHOULD use the strongest available conditional primitive: version, ETag, commit SHA, resource revision, or equivalent.

A changed approved intent MUST require new authorization.

## 9.9 Effect budget and secrets

Capabilities MAY limit external-effect count, rate, cost, risk, and targets. Secrets MUST be resolved only at execution time and MUST NOT be stored as plaintext in traces/receipts.

## 9.10 Receipt

Each dispatched effect SHOULD create an immutable receipt/evidence record with provider, timestamps, external identifiers/version, and outcome state.

Principal invariant:

```text
attempted ≠ acknowledged ≠ confirmed ≠ desired outcome
```

---

# Block 10 — Incident / Recovery / Revalidation

## 10.1 Incident model

`HealthFinding` is a specific detected defect. `Incident` is a coordinated-response context that MAY group multiple findings, failures, or unknown effects.

Incident lifecycle:

`OPEN → CONTAINING → CONTAINED → RECOVERING → REVALIDATING → RESOLVED → CLOSED`

## 10.2 Containment before recovery

Incident response first limits the blast radius through the minimum necessary write freeze, capability revocation, scope isolation, worker/effect pause, resource quarantine, or switch to read-only.

## 10.3 Trust states

Trust projection:

`TRUSTED / DEGRADED_TRUST / UNTRUSTED / REVALIDATING`

Process restart MUST NOT automatically restore `TRUSTED`.

## 10.4 Crash recovery classification

After a crash, incomplete executions/effects/repairs/leases are classified as:

`SAFE_TO_RESUME / SAFE_TO_RETRY / MUST_RECONCILE / MUST_ABORT / HUMAN_REQUIRED`

Resume-all is prohibited.

## 10.5 Recovery points and reconstruction

A recovery point describes demonstrably consistent state, not merely a backup timestamp.

Recovery SHOULD follow:

```text
validated snapshot
+ canonical history replay
→ derived-state rebuild
→ revalidation
```

## 10.6 Partial recovery

Different workspaces/domains MAY have different trust states. The capability engine MUST account for the scope's current trust state.

## 10.7 Revalidation

After recovery, run an evidence-producing `RevalidationRun`.

Levels:

`LOCAL / DOMAIN / WORKSPACE / INSTALLATION / FULL_TRUST`

Results:

`PASSED / PASSED_WITH_DEGRADATION / FAILED / INCONCLUSIVE`

Trust MUST NOT increase without evidence-backed revalidation.

## 10.8 Divergent histories and corruption

Split-brain or competing histories MUST NOT be resolved automatically through `latest wins`.

Corruption is classified as logical, physical, derived, or cryptographic integrity failure because each requires different recovery strategies.

## 10.9 Recovery loops and evidence preservation

Recovery has budgets. Repeated automatic recovery MUST stop with `RECOVERY_LOOP_DETECTED`.

Forensic evidence SHOULD be preserved before destructive recovery.

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

Key distinctions:

```text
AVAILABLE ≠ HEALTHY
HEALTHY ≠ TRUSTED
RECOVERED ≠ REVALIDATED
```

---

# Block 11 — Crypto / Data Governance

## 11.1 Crypto and governance are separate

Cryptography protects confidentiality, integrity, and authenticity. Data Governance controls ownership, classification, retention, export, deletion, purpose, and sharing.

Encryption MUST NOT replace authorization or governance.

## 11.2 Data classification

Baseline sensitivity levels:

`PUBLIC / INTERNAL / CONFIDENTIAL / RESTRICTED`

Classification also includes category, jurisdiction, and handling metadata.

Derived data MUST NOT automatically receive lower sensitivity than its sources. Declassification is a separate governed decision.

## 11.3 Lineage-aware handling

Derived objects SHOULD retain source references and effective classification lineage so policy decisions can be explained.

## 11.4 Secrets are not memory

Credentials, tokens, and private-key material MUST NOT be stored as ordinary memory. Canonical state contains `SecretRef`; plaintext is resolved through a controlled secret backend only for the duration of use.

## 11.5 Encryption and keys

Vestrace SHOULD support encryption at rest and envelope encryption for large/CAS objects.

The key hierarchy SHOULD be scoped by installation, workspace, and purpose. Key material SHOULD reside behind `KeyProvider`, not in the ordinary application database.

Key lifecycle:

`ACTIVE / ROTATING / RETIRED / REVOKED / DESTROYED`

Cryptographic schemas MUST support algorithm agility through algorithm/version identifiers.

## 11.6 Signatures and audit integrity

Signing and encryption serve different purposes.

Critical exports, manifests, checkpoints, and qualification bundles MAY be signed. Audit history SHOULD support tamper-evident chaining and signed checkpoints.

A hash match establishes content integrity, not provenance or authorship.

## 11.7 DataPolicy

DataPolicy controls classification constraints, retention, export, deletion, residency, and sharing. Capability and DataPolicy are independent mandatory gates.

## 11.8 Retention / hold / disposal

Retention clocks and triggers are explicit.

Lifecycle:

`ACTIVE → EXPIRED → PENDING_DISPOSAL → DISPOSED`

DataHold MAY temporarily block disposal and MUST have authority, reason, and audit.

Deletion semantics distinguish:

`LOGICAL_DELETE / PHYSICAL_DELETE / CRYPTO_ERASURE`

These methods MUST NOT falsely claim destruction of copies belonging to a different class.

## 11.9 Dependency-aware forgetting

Deleting a source/evidence MUST trigger checks of dependent claims, derived artifacts, indexes, and exports. A claim that loses admissible evidence requires reevaluation.

## 11.10 Governed export and federation

Export is a separate governed operation with a manifest, scope, classification, recipient, redaction, encryption/signature, and provenance.

A federation relationship MUST NOT automatically permit data transfer.

## 11.11 Model-boundary governance

Before sending a ContextPack/model input to an external provider, Vestrace MUST check classification, destination/locality, redaction, and DataPolicy. Policy MAY require local-only execution.

## 11.12 Crypto anomaly is a trust event

Hash, signature, audit-chain, or key-compromise anomalies SHOULD raise a trust-sensitive Incident and initiate containment/revalidation.

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

Tests verify an implementation. Conformance checks a component against a normative contract. Qualification determines whether a particular build/deployment is suitable for a defined trust profile.

## 12.2 Normative requirements

Critical guarantees SHOULD have stable requirement IDs (`MEM-*`, `RET-*`, `EXT-*`, `REC-*`, `GOV-*`, `QUAL-*`, and others).

Every MUST requirement MUST have a verification path.

## 12.3 Requirement traceability

```text
Requirement → Implementation → Conformance Test → Evidence
```

Implementation without conformance evidence is insufficient to claim profile support.

## 12.4 Profiles

Baseline conformance profiles:

- `CORE`;
- `MEMORY`;
- `COGNITION`;
- `AUTONOMY`;
- `FEDERATION`;
- `TRUSTED`.

Profiles have dependency closure; a higher profile cannot be claimed without its required lower-level dependencies.

## 12.5 Capability manifest

An installation SHOULD publish a machine-readable manifest containing implementation/schema versions, supported profiles, optional features, crypto/storage/model/effect-adapter capabilities, and known limitations.

## 12.6 Conformance suite

Categories:

`STATIC / BEHAVIORAL / STATEFUL / FAULT / SECURITY / RECOVERY / INTEROPERABILITY`

Golden end-to-end scenarios and deterministic fault-injection points are mandatory for crash/retry/recovery semantics.

Chaos testing supplements rather than replaces reproducible fault tests.

## 12.7 Model nondeterminism

Qualification SHOULD check deterministic core properties and evidence requirements, not exact model wording.

Cognitive benchmarks MAY measure recall, temporal correctness, provenance coverage, stale-memory/conflict rates, and context selection.

Security/governance requirements are hard gates and MUST NOT be offset by a high quality score.

## 12.8 Migration and interoperability

Qualification SHOULD include migration scenarios, schema/event compatibility, and adapter/backend qualification. Unsupported compatibility MUST be explicit rather than silent best effort.

## 12.9 Qualification bundle

A qualification result SHOULD be a reproducible bundle containing:

- implementation/source/build digest;
- profile;
- environment manifest;
- suite version;
- results/evidence;
- failures/limitations;
- optional signature.

Qualification applies to a particular build, configuration, and environment.

## 12.10 Continuous qualification

Qualification lifecycle:

`PRE-MERGE / RELEASE / DEPLOYMENT / PERIODIC / POST-INCIDENT`

Significant backend/provider/policy/crypto/schema/adapter changes MAY make a baseline `STALE` or `INVALIDATED` and require requalification.

## 12.11 Release trust gates

Architectural maturity sequence:

- **v0.2 — Correct:** CORE + MEMORY correctness;
- **v0.3 — Learn:** COGNITION/feedback quality;
- **v0.4 — Govern:** capabilities/workspaces/federation governance;
- **v0.5 — Understand:** health/integrity/repair;
- **v0.6 — Connect:** external effects/interoperability;
- **v1.0 — Trust:** TRUSTED profile, recovery/revalidation, crypto/governance, and the complete conformance contract.

`v1.0` is defined by fulfillment of the Trust Contract, not a feature count.

---

# 13. Storage and authority baseline

## 13.1 Canonical state

PostgreSQL is the primary authoritative transactional store for domain state, identities, policies, histories, revisions, and journals.

Artifact content MAY reside in local content-addressed storage. A CAS hash identifies/integrity-checks bytes; it grants no permission and does not replace domain metadata/provenance in PostgreSQL.

## 13.2 SQL boundary

Only Vestrace code performs SQL writes through controlled application/repository transactions. Arbitrary write SQL is not an agent API.

Restricted read-only views MAY be provided to analytics/operations principals under policy/RLS.

## 13.3 Core implementation language

Vestrace's core is implemented in Rust. This architecture contract does not require a Python runtime for the authoritative core.

---

# 14. Documentation hierarchy

After approval of this baseline, specialized documents must be aligned in the following order:

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
11. A consistency pass across README and existing docs/specs.

A conflict between an older design/specification and this contract MUST be resolved explicitly during the consistency pass. Do not delete old documents merely to hide architectural evolution: mark them historical/superseded, or update them if they remain normative.

# 15. Implementation freeze during documentation phase

Until the documentation set is complete and internally reviewed:

- Rust code MUST NOT change as part of this architecture work;
- migrations MUST NOT be added;
- runtime/API behavior MUST NOT change;
- the documentation branch MUST remain separate from `main`;
- implementation gap analysis takes place only after the documentation consistency pass.

# 16. Completion criterion for this contract

Architecture Contract v0.2 is a complete baseline when specialized documents elaborate it without contradictions and every significant MUST requirement is represented in the Normative Invariants / Conformance catalog.

This document establishes 12 completed architectural blocks:

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
