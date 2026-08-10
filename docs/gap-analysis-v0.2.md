# Vestrace v0.2 — Architecture Gap Analysis

**Analysis target:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Normative target:** `docs/architecture-v0.2` Architecture Contract + specialized v0.2 specifications  
**Date:** 2026-08-10  
**Phase:** documentation / analysis only — no implementation authorization

## 1. Executive summary

The current `main` is a substantial **cognitive-runtime foundation**, not a blank P0. It already contains useful pieces of the target architecture:

- event-sourced `AgentRun` commands/events/reducer/replay;
- optimistic concurrency and run checkpoints;
- Memory identity/revisions and lifecycle;
- provenance and knowledge relations;
- idempotency, jobs and transactional outbox foundations;
- PostgreSQL RLS workspace isolation;
- FTS retrieval, RRF fusion, deterministic reranking and ContextPack structure;
- cognitive assets for agents, skills and workflows;
- model routing/execution history foundations;
- evaluation CRUD;
- capability/sensitivity/approval types and policy-engine primitives;
- diagnostics plus working `doctor` and partial `rebuild` CLI flows;
- artifact, connection, enterprise, tool and state-engine extension types;
- a broad v0.1 acceptance scenario suite.

However, **the current source does not satisfy the normative v0.2 contracts as a system**. The largest gaps are not feature count; they are semantic boundaries:

1. persistent cognition lacks the target `Claim / ClaimEvidence / ClaimAssessment / Conflict` layer;
2. generic Memory/Event temporal semantics are much weaker than the Run temporal model;
3. current retrieval has a strong orchestration shell but not ContextPack 2.0 truth/provenance/governance semantics;
4. capabilities are currently mostly coarse enums/policy helpers, not durable constrained `CapabilityGrant` authority;
5. cross-workspace sharing is an early one-sided grant table/type rather than `source grant revision + target MemoryMount acceptance`;
6. diagnostics/rebuild exist, but not the target `InvariantDefinition → HealthFinding → RepairPlan → RepairExecution → Verification` protocol;
7. tool/execution types do not provide the target external-effect `UNKNOWN / reconciliation / receipt / delivery semantics` contract;
8. Run recovery exists, but `Incident / TrustState / RevalidationRun` do not;
9. crypto/governance has schema/type seeds, but no target `SecretRef / DataPolicy / retention/deletion/export verification` system;
10. acceptance tests provide excellent scenario seeds, but no requirement-ID conformance profiles or reproducible `QualificationBundle`.

The implementation should therefore proceed by **normalizing and connecting existing foundations**, not by adding another parallel runtime or replacing the repository wholesale.

---

# 2. Analysis status vocabulary

| Status | Meaning |
|---|---|
| **IMPLEMENTED** | Target behavior is clearly represented in source and has a wired runtime path. |
| **PARTIAL** | Strong reusable foundation exists, but required target semantics/wiring are incomplete. |
| **TYPE/SCHEMA ONLY** | Domain type and/or migration exists, but no sufficient governed runtime behavior was identified. |
| **MISSING** | Target concept/runtime protocol was not found in current source. |
| **CONFLICT** | Current model/behavior materially conflicts with the normative target and must be normalized before expansion. |
| **VERIFY** | Likely present, but implementation/conformance must be proven by tests or deeper code review before claiming support. |

A type, migration or unit test alone is never counted as IMPLEMENTED.

---

# 3. Requirement-family overview

| Family | Current status | Main reusable foundation | Primary gap |
|---|---|---|---|
| `ARC-*` | **PARTIAL** | modular Rust layers, Run event store/replay, rebuildable projections | unified authority/projection laws are not uniformly enforced across all domains |
| `MEM-*` | **PARTIAL / strong** | Memory + revisions + lifecycle + provenance + structured payload | Claim/evidence assessment/conflict model, richer revision metadata, provenance closure |
| `TMP-*` | **PARTIAL** | Run `occurred_at/recorded_at`, sequence/version, Memory revision numbers | generic Event and Memory lack target temporal/validity model; `AsOf` is mostly structural |
| `MUT-*` | **PARTIAL** | Memory revisions, Run expected versions, append-only events | normalized cognitive mutation/reconciliation protocol absent |
| `RET-*` | **PARTIAL / normalization required** | FTS + RRF + rerank + ContextPack + journal | real content hydration, revision provenance, temporal/conflict/security governance, multi-channel wiring |
| `LRN-*` | **PARTIAL** | evaluation CRUD, model/execution records, cognitive asset revisions | typed evaluator authority/evidence and governed learning proposal lifecycle |
| `CAP-*` | **PARTIAL / mostly primitive** | Capability enum, approval records, policy-engine helper, budgets | durable constrained CapabilityGrant, delegation attenuation, risk, PolicyDecision, universal enforcement |
| `IDW-*` | **PARTIAL + CONFLICT** | Workspace IDs/RLS, early cross-workspace grant schema | two-sided grant+mount, exact revisions, lifecycle/revoke/stale, federation trust/data separation |
| `HLT-*` | **PARTIAL + CONFLICT** | diagnostics repository, DoctorService, CLI doctor, direct projection rebuild | findings-first invariant registry and RepairPlan protocol; direct rebuild bypasses target repair execution model |
| `EXT-*` | **TYPE/SCHEMA ONLY / CONFLICT** | ToolInvocation `Prepared/Committed/Verified/Failed`, execution history | ExternalEffectIntent, receipts, UNKNOWN, delivery/idempotency/reconciliation/reversibility semantics |
| `REC-*` | **PARTIAL foundation** | Run checkpoints/replay/projection recovery | Incident, TrustState, recovery classification and evidence-backed RevalidationRun |
| `GOV-*` | **TYPE/SCHEMA ONLY / PARTIAL** | Sensitivity, redaction, WorkspaceKek, signed-export type, RLS | SecretRef, DataPolicy, lineage, retention/hold/deletion verification/export governance/key lifecycle |
| `QUAL-*` | **PARTIAL scenario foundation** | v0.1 acceptance suite, compose smoke, schema artifacts | profiles, requirement traceability, deterministic fault suite, environment/build identity, QualificationBundle |

---

# 4. Architecture foundation — what should be preserved

## 4.1 Run event-sourced kernel

Current source under `crates/vestrace-domain/src/run/` already has the right core shape:

```text
RunCommandEnvelope
→ decide(...)
→ RunEventEnvelope
→ replay/reducer
→ RunState
→ projection/recovery
```

`RunEventEnvelope` contains:

- workspace/run identity;
- monotonic `RunVersion` sequence;
- event type/version;
- actor;
- causation/correlation;
- `occurred_at`;
- `recorded_at`.

This is the best current implementation of the v0.2 temporal/history principles and should become the **shared durable execution boundary**, not be replaced by a second generic task/action runtime.

### Gap

Other execution history types (`WorkflowExecution`, `StepExecution`, tool/model attempts) must be reconciled as typed records owned by/linked to this Run-first boundary, not allowed to evolve into a competing runtime.

## 4.2 PostgreSQL + RLS + forward migrations

Current workspace-scoped persistence and RLS are strong foundations for `IDW-*`.

They should be preserved while cross-workspace sharing is implemented as an explicit application-level grant/mount protocol rather than weakening RLS.

## 4.3 Idempotency / jobs / outbox

Current infrastructure already supplies key primitives needed later by:

- derived-state rebuild;
- effect dispatch/reconciliation;
- repair/recovery jobs;
- qualification workflows.

These should be reused instead of creating specialized queues for repair, incident or governance domains.

---

# 5. Persistent Cognition gaps (`MEM-*`)

## Current foundation

Current `Memory` / `MemoryRevision` provide:

```text
Memory
├─ stable ID
├─ workspace
├─ kind
├─ lifecycle status
└─ active_revision_id

MemoryRevision
├─ memory_id
├─ revision_number
├─ content
├─ structured payload
├─ confidence
├─ importance
└─ created_at
```

The current lifecycle correctly prevents silent reactivation of `Superseded`, `Rejected`, `Expired` and `Deleted` Memory states.

Structured memory variants and provenance provide a meaningful substrate.

## Required additions / normalization

### G-MEM-01 — Claim layer — **MISSING**

No target runtime `Claim` model was found.

Required:

- `Claim` stable identity/semantic key;
- `ClaimEvidenceLink`;
- `ClaimAssessment` history;
- statuses such as Proposed/Supported/Contested/Superseded/Expired/Rejected/Deleted;
- explicit distinction `Memory != Claim != Truth`.

### G-MEM-02 — revision semantic metadata — **PARTIAL**

Target `MemoryRevision` needs additional governed metadata:

- `valid_from` / `valid_until`;
- classification;
- change reason;
- created-by actor;
- canonical hash/integrity metadata.

### G-MEM-03 — aggregate state revision — **MISSING/PARTIAL**

Run has explicit optimistic versioning. Memory identity/lifecycle needs equivalent target state-revision semantics instead of relying only on content revision numbering/repository behavior.

### G-MEM-04 — richer evidence reference model — **PARTIAL**

Current `MemorySource` links a Memory to an Event and optional Derivation. Target evidence must address exact revisions and additional source classes:

- MemoryRevision;
- ArtifactRevision;
- ModelExecution;
- ToolResult;
- ExternalEffectReceipt;
- HumanFeedback;
- federated/external references.

### G-MEM-05 — provenance closure — **PARTIAL**

Current Derivation records method metadata, but target requires exact input/output references and sufficient closure to prove derived canonical cognition back to admissible evidence.

### G-MEM-06 — explicit conflicts — **MISSING**

Current relations include contradiction edges, but there is no target Conflict aggregate/lifecycle/reconciliation result with evidence basis.

---

# 6. Temporal & concurrency gaps (`TMP-*`)

## Strong current behavior

Run events already distinguish `occurred_at` and `recorded_at`, and Run streams use monotonic versions.

Memory revisions use unique sequential revision numbers.

## G-TMP-01 — generic Event time axes — **CONFLICT/PARTIAL**

Current generic `Event` and `events` table expose a single `created_at` rather than target:

```text
occurred_at?
recorded_at
```

plus causation/correlation/idempotency/integrity context where applicable.

This is a schema migration gap.

## G-TMP-02 — knowledge validity — **MISSING**

MemoryRevision does not currently carry target validity interval.

## G-TMP-03 — temporal retrieval semantics — **PARTIAL**

`TimePerspective` exists (`Current`, `AsOf`, `Timeline`, `AllHistory`) and flows through request normalization, but the wired FTS retriever does not use it.

Target v0.2 additionally distinguishes:

- `KNOWN_AS_OF`;
- `VALID_AS_OF`;
- `RECONSTRUCTED_AS_OF`.

## G-TMP-04 — late-arriving evidence / bitemporal behavior — **MISSING**

No current runtime protocol was found that preserves the distinction between what occurred, what was known then, and what is reconstructed now after late evidence.

---

# 7. Mutation & reconciliation gaps (`MUT-*`)

## Current foundation

- Memory correction creates revisions rather than overwriting text in place.
- Run commands use expected versions.
- events are append-only in DB.
- hard purge is an explicitly privileged special operation.

## G-MUT-01 — unified mutation envelope — **PARTIAL**

Target mutations require actor, target, expected state, reason/intent, provenance and resulting fact/revision as a consistent application boundary. Current semantics are per-service and not normalized across cognition/governance.

## G-MUT-02 — deterministic vs semantic reconciliation — **MISSING**

There is no shared classification/protocol for:

```text
DETERMINISTIC
POLICY_GUIDED
SEMANTIC
HUMAN_REQUIRED
```

## G-MUT-03 — conflict reconciliation evidence — **MISSING**

Current contradiction relations do not provide the target governed reconciliation result/basis.

---

# 8. Retrieval / ContextPack 2.0 gaps (`RET-*`)

This is one of the most important normalization areas because the current implementation looks more complete from surface APIs than its semantics actually are.

## Strong reusable shell

Current runtime has:

```text
normalize request
→ text retriever
→ RRF
→ deterministic rerank
→ ContextPackBuilder
→ retrieval journal
```

The ContextPack domain type enforces `used_tokens <= token_budget`.

## G-RET-01 — only FTS channel wired — **PARTIAL**

Vector/exact/structured ports exist, but the current `RetrievalService` only owns and calls a text retriever.

## G-RET-02 — `TimePerspective` not applied by wired retriever — **PARTIAL/CONFLICT**

The request carries temporal perspective, but the PostgreSQL FTS query filters status/kind and does not implement `AsOf`/Timeline semantics.

## G-RET-03 — candidate revision provenance — **PARTIAL**

The current FTS adapter returns `revision_id: None`.

Target ContextPack must preserve exact source/revision references where reproducibility depends on them.

## G-RET-04 — policy/classification/share filtering before ranking — **PARTIAL/MISSING**

RLS provides workspace isolation, but the wired retrieval orchestration does not demonstrate the complete target pre-ranking gates for:

- effective capability;
- classification;
- cross-workspace grant/mount use;
- provider/model destination;
- unresolved conflicts.

## G-RET-05 — current ContextPackBuilder does not hydrate canonical content — **CONFLICT**

The current builder uses `candidate.explanation` as `rendered_text`. It therefore builds a structural pack from retrieval explanations rather than the exact selected MemoryRevision content/evidence representation required by ContextPack 2.0.

This must be replaced by a content-hydration/representation stage over authorized exact revisions.

## G-RET-06 — section assignment is structural placeholder — **CONFLICT/PARTIAL**

Current builder iterates all candidates under each section label while globally marking included Memory IDs. In practice candidates are consumed by the first non-empty section rather than classified semantically into constraints/facts/decisions/tasks/etc.

## G-RET-07 — compression ladder is nominal, not semantic — **PARTIAL**

Representation enum exists, but current overflow handling truncates text and labels it `Reference`; there is no target staged `Full → Summary → Atomic → Reference` transformation with provenance-preserving semantics.

## G-RET-08 — reranker metadata is placeholder — **PARTIAL**

Current reranker derives importance/confidence from candidate score and uses fixed neutral recency/provenance values because candidate metadata is unavailable.

Target reranking should consume real Memory/Claim/provenance/temporal metadata.

## G-RET-09 — conflict-aware current truth — **MISSING**

There is no target Claim/Conflict state to prevent contested cognition from silently appearing as unqualified truth.

---

# 9. Execution Feedback & Learning gaps (`LRN-*`)

## Current foundation

The repository contains:

- model routing/execution types;
- workflow/step execution records;
- HTTP + PostgreSQL Evaluation CRUD;
- outcome memory scenarios in acceptance tests;
- versioned Agent/Skill/Workflow domain models.

## G-LRN-01 — typed evaluation authority — **PARTIAL**

Current `EvaluationRecord` is primarily:

```text
model_id?
name
status
score?
summary?
```

Target needs:

- exact target execution reference;
- evaluator kind (`deterministic/human/heuristic/model-judge`);
- metric/check type;
- evidence refs;
- evaluator/model revision;
- policy version/authority.

## G-LRN-02 — raw facts vs learned projections — **MISSING/PARTIAL**

No governed distinction was identified between immutable raw evaluation facts and versioned learned conclusions/recommendations.

## G-LRN-03 — learning proposal lifecycle — **MISSING**

Current code does not provide a target proposal/review/versioned mutation protocol for changing agents, skills, workflows or routing policy from feedback.

---

# 10. Capability Governance gaps (`CAP-*`)

## Current foundation

Current source has:

- stable `Capability` enum/string names;
- `Sensitivity`;
- approval records bound to optional operation hash + expiry;
- `PolicyEngine` abstraction;
- capability-set and workspace-scoped helpers;
- a simple `BudgetAccount` hard limit/reservation primitive;
- token/approval/policy-bundle migrations.

## G-CAP-01 — durable CapabilityGrant — **MISSING**

No target `CapabilityGrant` aggregate was found.

Required constraints include:

- operation/tool;
- resource selector;
- validity interval;
- budget;
- risk ceiling;
- conditions/obligations;
- delegation policy/lifecycle.

## G-CAP-02 — runtime policy decision model — **MISSING/PARTIAL**

Current `PolicyEngine::evaluate(context, capability)` returns allow/error. Target requires evidence-producing decisions:

```text
DENY
ALLOW
PREPARE_ONLY
REQUIRE_APPROVAL
```

with exact policy version, input state, effective risk, budget/trust and obligations.

## G-CAP-03 — delegation attenuation — **MISSING**

No runtime implementation of:

```text
child authority
= parent effective authority
∩ explicit delegation
∩ child constraints
```

was found.

## G-CAP-04 — risk model — **MISSING**

Target `LOW/MEDIUM/HIGH/CRITICAL`, context elevation and operation risk gates are not represented as a central runtime authorization input.

## G-CAP-05 — universal policy wiring — **MISSING/PARTIAL**

Search evidence suggests current PolicyEngine is largely an application primitive/testable helper rather than a uniformly enforced boundary across HTTP/MCP/worker/internal execution.

This is a high-priority security normalization item.

## G-CAP-06 — typed multi-dimensional budgets — **PARTIAL**

Current `BudgetAccount` is monetary/general numeric. Target needs typed dimensions for tokens, time, tools/effects, artifacts, subruns, retries and other resources with auditable reservations/accounting.

---

# 11. Identity / Workspace / Federation gaps (`IDW-*`)

## Strong current behavior

Workspace IDs and PostgreSQL RLS provide a real isolation foundation.

## G-IDW-01 — current cross-workspace grant is one-sided — **CONFLICT**

Current enterprise type/schema:

```text
CrossWorkspaceMemoryGrant
├─ owner_workspace_id
├─ target_workspace_id
├─ memory_id
└─ granted_at
```

Target requires source-owned lifecycle identity + immutable grant revision **and separate target-owned acceptance**.

## G-IDW-02 — MemoryMount absent — **MISSING**

No target `MemoryMount` runtime model was found.

## G-IDW-03 — grant revision/lifecycle absent — **MISSING**

Target needs:

- PendingAcceptance/Active/Suspended/Revoked/Expired/Deleted;
- exact grant revisions;
- explicit operation permissions;
- content/index/model/derive/export policies;
- stale re-accept semantics.

## G-IDW-04 — SharedMemoryRef absent — **MISSING**

Cross-workspace references must be namespaced and must not be treated as local Memory IDs.

## G-IDW-05 — federation trust != data permission — **MISSING/PARTIAL**

A2A/federation-related types/plans exist historically, but no v0.2 runtime trust + local authorization composition was identified.

---

# 12. Health / Integrity / Repair gaps (`HLT-*`)

## Important current correction

Contrary to older implementation-status wording, `main@729d456…` already has functioning CLI code for:

- `vestrace doctor`;
- `vestrace rebuild search-documents` / `all`;
- post-rebuild diagnostics verification.

This is a useful implementation foundation and should be recognized.

## Current diagnostics model

```text
DiagnosticFinding
├─ code
├─ Error/Warning/Info
├─ optional object
├─ message
└─ remediation
```

`DoctorService` runs a fixed set of repository checks.

## G-HLT-01 — Findings-first target model — **PARTIAL**

Current DiagnosticFinding lacks target:

- invariant ID/version;
- normalized scope;
- evidence refs;
- 5-level severity;
- repairability;
- impact;
- finding fingerprint;
- lifecycle;
- occurrence history;
- disposition overlay.

## G-HLT-02 — Invariant registry — **MISSING**

Checks are methods on `DiagnosticsRepository`, not a versioned hybrid `InvariantDefinition + typed checker` registry.

## G-HLT-03 — hybrid scheduling — **MISSING/PARTIAL**

Current doctor is on-demand. Some DB/domain constraints are inline, but there is no target per-invariant `INLINE/REACTIVE/SWEEP/MULTI` execution profile.

## G-HLT-04 — RepairPlan protocol — **CONFLICT**

Current `rebuild` command directly executes SQL rebuild operations and only then runs Doctor verification.

Target requires:

```text
Finding
→ immutable RepairPlan
→ capability/policy
→ existing execution runtime
→ verification
→ finding closure
```

Direct deterministic rebuild can be preserved internally as a **repair operation implementation**, but it must no longer be the top-level repair protocol.

## G-HLT-05 — stale plan / repair concurrency — **MISSING**

No RepairPlan input-state precondition/fencing semantics exist.

## G-HLT-06 — recurrence/flapping/budgets — **MISSING**

No `HealthOccurrence`, recurrence classification, cooldown or repair-loop budget was found.

## G-HLT-07 — embeddings rebuild misleadingly incomplete — **PARTIAL**

Current `rebuild_embeddings` checks prerequisites and returns `0`; it does not actually regenerate embedding vectors. This should be treated as an unimplemented target operation, not completed repair support.

---

# 13. External Effects gaps (`EXT-*`)

## Current foundation

`ToolInvocation` currently models:

```text
Prepared
Committed
Verified
Failed
```

This is directionally compatible with prepare/commit/verification thinking.

## G-EXT-01 — ExternalEffectIntent — **MISSING**

No immutable target intent aggregate was found.

## G-EXT-02 — UNKNOWN outcome — **CONFLICT**

Current ToolInvocationStatus has no `UNKNOWN`/`RECONCILING` state.

Target requires ambiguity to be first-class after uncertain dispatch/crash/timeout.

## G-EXT-03 — delivery semantics — **MISSING**

No adapter declarations for:

```text
AT_MOST_ONCE
AT_LEAST_ONCE
EFFECTIVELY_ONCE
UNKNOWN
```

were identified.

## G-EXT-04 — idempotency/reconciliation profile — **MISSING**

Command idempotency exists elsewhere, but external adapter retry safety/read-back semantics are not modeled as target contract.

## G-EXT-05 — receipt/outcome separation — **MISSING**

No `ExternalEffectReceipt` with acknowledgement vs confirmed outcome evidence was found.

## G-EXT-06 — reversibility/compensation — **MISSING**

No target reversibility classification or explicit compensation relation exists.

---

# 14. Incident / Recovery / Revalidation gaps (`REC-*`)

## Strong current foundation

Run checkpoint creation/validation/replay/projection rebuild is a genuine recovery substrate.

## G-REC-01 — Incident aggregate — **MISSING**

No target incident lifecycle was found.

## G-REC-02 — TrustState — **MISSING**

No runtime `TRUSTED / DEGRADED_TRUST / UNTRUSTED / REVALIDATING` scope state exists.

## G-REC-03 — recovery classification — **MISSING/PARTIAL**

Target startup scan needs:

```text
SAFE_TO_RESUME
SAFE_TO_RETRY
MUST_RECONCILE
MUST_ABORT
HUMAN_REQUIRED
```

Current Run recovery does not provide this cross-domain classification for effects/repairs/leases.

## G-REC-04 — RevalidationRun — **MISSING**

No evidence-producing target RevalidationRun model was found.

## G-REC-05 — trust restoration barrier — **MISSING**

Current recovery can restore Run state/projections, but there is no target rule enforcement that recovery completion cannot restore TRUSTED without revalidation evidence.

---

# 15. Crypto / Data Governance gaps (`GOV-*`)

## Current seeds

Current source/schema contains:

- `Sensitivity` four-level ordering;
- destination/redaction primitives;
- `WorkspaceKek { key_alias, algorithm }`;
- early envelope-encryption/cross-share migration;
- signed-run-export type;
- artifact content hash;
- RLS.

## G-GOV-01 — SecretRef / secret lifecycle — **MISSING**

No target runtime `SecretRef` was found in source.

Connection types do not themselves model governed secret resolution/lease/use semantics.

## G-GOV-02 — DataPolicy — **MISSING**

No target runtime DataPolicy abstraction covering classification, retention, export, deletion, residency, sharing and purpose was found.

## G-GOV-03 — classification lineage — **MISSING**

Derived ContextPack/artifact/retrieval objects do not carry target source-classification lineage and declassification provenance.

## G-GOV-04 — key lifecycle/provider abstraction — **TYPE/SCHEMA ONLY**

WorkspaceKek contains alias/algorithm, but target key states, rotation/revocation/destruction and provider abstraction are not implemented as a governed runtime contract.

## G-GOV-05 — retention/hold/disposal — **MISSING**

No target `RetentionPolicy`, `DataHold`, disposal lifecycle or verification protocol was found.

## G-GOV-06 — deletion semantics/verification — **PARTIAL/MISSING**

Hard purge exists, but target distinguishes logical delete, physical delete and crypto erasure and requires dependency-aware `DeletionPlan → Execution → Verification`.

## G-GOV-07 — governed export bundle — **TYPE ONLY/PARTIAL**

SignedRunExport exists as a small domain type, but target DataExportPlan/manifest/classification/recipient/purpose/encryption/signing/provenance workflow is absent.

## G-GOV-08 — crypto anomaly → trust incident — **MISSING**

No integrated Health/Incident/Revalidation path exists yet.

---

# 16. Qualification / Conformance gaps (`QUAL-*`)

## Valuable current foundation

`tests/v01_acceptance.rs` and `docs/acceptance/v0.1.md` already contain strong scenario seeds:

- memory evolution;
- workspace denial;
- restricted-provider rejection;
- self-elevation rejection;
- purge approval;
- degraded retrieval;
- fallback;
- audit;
- capability checks;
- compose doctor/health/metrics.

This work should be preserved and reclassified rather than discarded.

## G-QUAL-01 — tests are not conformance profiles — **PARTIAL**

Current acceptance tests are not mapped to stable v0.2 requirement IDs/profile dependency closure.

## G-QUAL-02 — many acceptance scenarios are in-memory/domain composition — **PARTIAL**

They prove useful domain behavior but do not uniformly prove wired HTTP/MCP/worker/storage runtime semantics.

## G-QUAL-03 — deterministic fault injection framework — **MISSING**

No target named crash boundaries such as `FAULT_AFTER_EFFECT_DISPATCH` were identified.

## G-QUAL-04 — QualificationBundle — **MISSING**

No target build/config/environment/profile evidence bundle exists.

## G-QUAL-05 — capability manifest / known limitations — **MISSING/PARTIAL**

Schema artifacts exist, but no target machine-readable deployment capability/profile qualification manifest was found.

## G-QUAL-06 — baseline invalidation/requalification — **MISSING**

No runtime/system for stale qualification after policy/backend/provider/crypto/schema changes exists.

---

# 17. Current architectural conflicts that must be normalized early

These are higher priority than adding new product surfaces.

## C-1 — ContextPack is structurally present but semantically under-hydrated

Do not build more retrieval features on top of explanation-text packs. First create exact revision hydration + provenance/governance/temporal assembly.

## C-2 — Direct rebuild is not the final Repair protocol

Keep low-level rebuild logic, but route it under RepairPlan/authorization/verification before expanding auto-repair.

## C-3 — Cross-workspace grant schema is insufficient for two-sided authority

Do not expose the current simple grant table as production sharing API. Introduce target grant revision + target mount semantics first.

## C-4 — ToolInvocation lacks UNKNOWN

Do not add aggressive retries/external-effect automation until ambiguous completion and reconciliation are represented.

## C-5 — PolicyEngine is too coarse for autonomy

Do not expose higher-risk autonomous execution based only on a `HashSet<Capability>` allow check. Durable constrained grants, risk/budget/trust and decision evidence must precede it.

## C-6 — WorkflowExecution must not become a second runtime

Before expanding workflow execution, explicitly map WorkflowExecution/StepExecution to Run-first execution ownership or retire duplicate state-machine responsibility.

---

# 18. Migration impact summary

The target cannot be reached with Rust-only changes. Significant schema evolution is required.

## High-confidence new/expanded schema areas

### v0.2 Correct

- generic events: occurred/recorded/causation/correlation/integrity fields;
- memory revisions: validity/classification/change reason/actor/hash;
- memory lifecycle state version;
- Claims, claim evidence, assessments, conflicts/reconciliation;
- retrieval source/revision/generation/governance metadata.

### v0.3 Learn

- typed evaluation facts/evidence;
- learned/proposal/review records as required.

### v0.4 Govern

- capability grants/revisions/delegations;
- policy decisions/obligations;
- typed budget reservations/accounting;
- MemoryShareGrant lifecycle/revisions;
- MemoryMount;
- SharedMemoryRef/import proposals.

### v0.5 Understand

- invariant definitions/versions;
- health findings/occurrences;
- repair plans/executions/verifications;
- accepted-risk/suppression disposition records.

### v0.6 Connect

- external effect intents;
- effect receipts/outcome facts;
- reconciliation/compensation records;
- adapter semantic/version declarations where persistence is required.

### v1.0 Trust

- incidents/trust state/revalidation;
- data policy/classification lineage;
- secret refs/crypto key metadata/lifecycle;
- retention/holds/deletion/export plans and verification;
- audit integrity checkpoints;
- qualification baselines/bundles/results.

## Existing tables to evolve carefully

- `events`;
- `memories`;
- `memory_revisions`;
- `memory_sources` / derivation structures;
- `search_documents` / context journals;
- `approval_records`;
- policy/budget tables;
- `cross_workspace_memory_grants`;
- tool/execution history tables;
- workspace key metadata;
- artifact tables.

Forward-only migration discipline must be preserved. Existing migrations must not be edited in place.

---

# 19. Recommended dependency order

The implementation dependency graph should be:

```text
A. Domain/temporal correctness
       ↓
B. Cognition claims + reconciliation
       ↓
C. Retrieval 2.0 exact-revision assembly
       ↓
D. Capability / policy enforcement kernel
       ↓
E. Feedback learning + governed proposals
       ↓
F. Sharing/federation authority
       ↓
G. Health findings + RepairPlan
       ↓
H. External effects + UNKNOWN/reconciliation
       ↓
I. Incident/trust/revalidation
       ↓
J. Crypto/data governance
       ↓
K. Qualification/conformance hardening
```

Some infrastructure can be developed in parallel, but **security/authority dependencies must not be bypassed** simply to expose later APIs sooner.

---

# 20. Release readiness conclusion

## v0.2 `Correct`

The repository has a strong foundation, but is **not yet CORE+MEMORY qualified** against the new contract because the target temporal/claim/reconciliation/ContextPack semantics are incomplete.

This should be the first implementation focus.

## v0.3 `Learn`

Evaluation/routing/execution primitives exist, but target evidence/authority/learning lifecycle is not ready.

## v0.4 `Govern`

Workspace RLS is strong, but capability authority/delegation and two-sided sharing are not ready.

## v0.5 `Understand`

Doctor/rebuild provide a valuable head start, but target findings/repair protocol is not ready.

## v0.6 `Connect`

Tool invocation primitives exist, but safe external-effect semantics are not ready.

## v1.0 `Trust`

Recovery foundation exists, but trust/revalidation, full governance and qualification are not ready.

---

# 21. Architectural recommendation

The current repository should **not** be rewritten. The best path is an incremental normalization program:

1. preserve Run event sourcing, RLS, Memory revisions, idempotency/jobs/outbox and diagnostics;
2. promote their strongest patterns into shared v0.2 contracts;
3. prevent early placeholder models (simple grants, simple ToolInvocation, explanation-based ContextPack, direct rebuild) from becoming public/stable architecture;
4. implement v0.2 Correct first and qualify it before adding autonomous external effects;
5. treat each later version as an evidence-backed conformance milestone.

The detailed requirement coverage matrix is in `docs/requirement-coverage-v0.2.md`. The proposed implementation/PR sequence is in `docs/implementation-plan-v0.2.md`.
