# Vestrace v0.2 — Requirement Coverage Matrix

**Implementation snapshot:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Requirement source:** `docs/specs/vestrace-normative-invariants-v0.2.md`  
**Date:** 2026-08-10

> This matrix is an architecture/code-inspection result, **not conformance evidence**. `CANDIDATE` means the implementation appears to provide the required behavior and should be converted into a real conformance case. It does not mean the requirement has passed qualification.

The matrix retains the historical `main@729d456...` comparison snapshot. Current uncommitted G4 evidence is tracked in [`documentation-gap-delta-2026-08-12-g4-cross-workspace-sharing.md`](documentation-gap-delta-2026-08-12-g4-cross-workspace-sharing.md), current G5/G6 evidence is tracked in [`documentation-gap-delta-2026-08-12-g5-g6-federation-governance.md`](documentation-gap-delta-2026-08-12-g5-g6-federation-governance.md), current H1–H5 evidence is tracked in [`documentation-gap-delta-2026-08-12-h1-h5-understand.md`](documentation-gap-delta-2026-08-12-h1-h5-understand.md), and current E1–E4 evidence is tracked in [`documentation-gap-delta-2026-08-12-e1-e4-connect.md`](documentation-gap-delta-2026-08-12-e1-e4-connect.md); none qualifies a release profile.

The current T1–T8 evidence is tracked in [`documentation-gap-delta-2026-08-12-t1-t8-trust.md`](documentation-gap-delta-2026-08-12-t1-t8-trust.md); it records additive contract evidence and explicit non-claims rather than release qualification.

## Status legend

| Status | Meaning |
|---|---|
| **CANDIDATE** | Behavior appears present and wired enough to justify a future conformance test. |
| **PARTIAL** | Some of the requirement is present, but target semantics/wiring are incomplete. |
| **TYPE_ONLY** | Type/schema/test seed exists without sufficient target runtime semantics. |
| **MISSING** | No sufficient current implementation was identified. |
| **CONFLICT** | Current behavior/model is incompatible with the target contract until normalized. |
| **VERIFY** | Evidence is insufficient to classify safely; requires implementation test/review. |

---

# 1. Architecture (`ARC-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `ARC-001` | PARTIAL | PostgreSQL canonical state and derived Run/search projections exist, but authority tiers are not encoded uniformly across domains. |
| `ARC-002` | PARTIAL | Run projections and search documents can be rebuilt; no general projection registry/rebuild contract. |
| `ARC-003` | VERIFY | No obvious reverse-projection mutation found, but no universal guard proves derived state cannot rewrite canonical state. |
| `ARC-004` | CONFLICT/PARTIAL | Run runtime exists, but current CLI rebuild executes directly rather than through the shared execution/repair protocol. |
| `ARC-005` | PARTIAL | No separate Repair/Incident runtime exists; `WorkflowExecution`/`StepExecution` ownership must be reconciled to avoid future competing runtime semantics. |
| `ARC-006` | CANDIDATE | Run events are append-oriented; generic events have append-only DB protection; Memory changes use revisions. |
| `ARC-007` | PARTIAL | Some domains have explicit stalled/failed states, but tool/external effects lack target `UNKNOWN`. |
| `ARC-008` | PARTIAL | RLS means knowing a UUID does not generally grant access; early cross-workspace sharing model needs normalization. |
| `ARC-009` | CANDIDATE | Extensive typed-ID newtypes exist throughout domain. |
| `ARC-010` | PARTIAL | Several projections are conceptually derived, but not all have explicit source generation/version/rebuild contracts. |

---

# 2. Memory / Persistent Cognition (`MEM-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `MEM-001..004` | CANDIDATE/PARTIAL | Stable Memory identity, immutable-style revisions and unique revision numbers exist. Need conformance proving no in-place semantic overwrite across all repositories. |
| `MEM-005` | CANDIDATE | Current DB has active-source invariant; convert to conformance test. |
| `MEM-006` | PARTIAL | Derivation type exists, but target derived-memory provenance closure/enforcement is incomplete. |
| `MEM-007` | CANDIDATE | Domain lifecycle rejects reactivation from Superseded/Rejected/Expired/Deleted. |
| `MEM-008` | MISSING | `valid_from/valid_until` not present on current MemoryRevision. |
| `MEM-009` | CANDIDATE | Confidence/Importance newtypes constrain finite values to `[0,1]`. |
| `MEM-010` | CANDIDATE/PARTIAL | StructuredMemory uses tagged variants; long-term schema/version compatibility needs conformance. |
| `MEM-011` | CANDIDATE/PARTIAL | Workspace RLS prevents scope from escaping workspace; cross-workspace model must not bypass this. |
| `MEM-012` | PARTIAL | MemorySource + Derivation exist, but exact multi-source provenance closure is incomplete. |
| `MEM-013` | MISSING | Target `Claim` concept absent. |
| `MEM-014..015` | MISSING | Claim support/assessment/evidence-loss revalidation absent. |
| `MEM-016..018` | MISSING | Explicit Conflict aggregate and evidence-backed reconciliation absent. |
| `MEM-019` | PARTIAL/CANDIDATE | Supersession preserves Memory history, but target claim/conflict supersession semantics absent. |
| `MEM-020` | CANDIDATE | Agent/Skill/Workflow are separate cognitive modules, not ordinary Memory records. |

Primary evidence paths:

- `crates/vestrace-domain/src/memory/*`
- `crates/vestrace-domain/src/provenance.rs`
- `migrations/0005_memories_and_revisions.sql`
- `migrations/0006_provenance_scopes_relations.sql`

---

# 3. Temporal / Concurrency (`TMP-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `TMP-001` | PARTIAL | RunEventEnvelope distinguishes occurred/recorded; generic Event only has created_at. |
| `TMP-002` | PARTIAL | Run model requires occurrence timestamp; generic event model lacks target optional occurrence semantics. |
| `TMP-003` | MISSING | Knowledge validity interval absent from MemoryRevision. |
| `TMP-004` | PARTIAL/CANDIDATE | Current retrieval defaults to Active status, but target temporal/conflict semantics not complete. |
| `TMP-005` | MISSING/PARTIAL | `AsOf` enum exists but wired FTS ignores it. |
| `TMP-006` | PARTIAL/strong | Run commands use expected RunVersion; Memory revisions use optimistic repository behavior, but not a uniform target state-revision model. |
| `TMP-007` | PARTIAL/strong | Run version mismatch is explicit; needs cross-domain conformance. |
| `TMP-008` | VERIFY | No architecture-wide causal-order enforcement was identified; must test that timestamp order is not treated as causality. |
| `TMP-009` | VERIFY/PARTIAL | Lease infrastructure exists; explicit conformance needed to prove lease does not alter authority/logical version. |
| `TMP-010` | CANDIDATE for Run | Run event sequence is monotonic/versioned; not generalized to every canonical append-only stream. |

---

# 4. Mutation / Reconciliation (`MUT-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `MUT-001` | PARTIAL | RunCommandEnvelope is strong; Memory/service mutations are less uniformly normalized. |
| `MUT-002` | CANDIDATE/PARTIAL | Memory content revisions and append-only events preserve history; prove all correction paths. |
| `MUT-003` | MISSING | No shared deterministic-reconciliation contract by authority tier. |
| `MUT-004` | MISSING | No central distinction semantic ambiguity vs deterministic repair. |
| `MUT-005` | MISSING/PARTIAL | Policy bundle versions exist in schema, but reconciliation does not bind exact policy decision/version. |
| `MUT-006` | MISSING | No shared human-required reconciliation protocol. |
| `MUT-007` | PARTIAL / ARCH REVIEW | No new generic Task/Action runtime exists, but WorkflowExecution/StepExecution must be reconciled to Run-first ownership. |
| `MUT-008` | MISSING | Reconciliation result/basis/evidence entity absent. |

---

# 5. Retrieval / ContextPack (`RET-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `RET-001` | PARTIAL | Workspace RLS is applied before FTS results, but capability/classification/share filtering is not complete. |
| `RET-002` | PARTIAL/CANDIDATE | Active status is default/current filter; temporal/conflict model incomplete. |
| `RET-003` | MISSING | No target Conflict state to surface unresolved semantic conflicts. |
| `RET-004` | CANDIDATE | ContextPack constructor enforces used tokens <= token budget. |
| `RET-005` | CONFLICT/PARTIAL | ContextItem can carry revision_id, but wired FTS returns `None` and source IDs are channel strings. |
| `RET-006` | PARTIAL | Inclusion explanations exist but current builder explanation is structural and not based on exact content-selection evidence. |
| `RET-007` | MISSING/PARTIAL | Sensitivity/redaction primitives exist, but ContextPack build does not enforce full model-destination policy. |
| `RET-008` | VERIFY | No explicit authority elevation observed, but no Claim authority model exists to prove preservation. |
| `RET-009` | CANDIDATE/PARTIAL | Text-channel failure produces degraded flag/warnings; only one wired channel limits proof of multi-channel degradation semantics. |
| `RET-010` | VERIFY | Fail-closed behavior when no safe fallback remains must be explicitly tested. |
| `RET-011` | CANDIDATE | RRF exists. |
| `RET-012` | MISSING/PARTIAL | Retrieval journals exist; target generation-based cache invalidation is not implemented. |
| `RET-013` | CONFLICT/PARTIAL | Representation enum exists, but truncation is labeled Reference; target semantic ladder is not implemented. |
| `RET-014` | MISSING | Target grant+mount access checks do not exist. |
| `RET-015` | PARTIAL | Retrieval run/context pack journaling exists, but policy/version/degradation/evidence metadata is incomplete. |

---

# 6. Feedback / Learning (`LRN-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `LRN-001` | IMPLEMENTED/PARTIAL | Typed `EvaluationFact` requires exact model/tool revision refs and a workflow revision ID at the domain boundary; provider/runtime evidence that those refs match executed artifacts remains separate. |
| `LRN-002` | IMPLEMENTED | Raw `EvaluationFact` and advisory `LearnedProjection` are separate domain types, repositories and tables. |
| `LRN-003` | IMPLEMENTED/PARTIAL | Learning changes reject governance fields and no automatic capability/permission mutation path exists; universal governance closure is later CAP work. |
| `LRN-004` | IMPLEMENTED/PARTIAL | Proposal payloads cannot represent or smuggle governance changes; full approval/application policy remains later governance work. |
| `LRN-005` | PARTIAL | Projections/proposals carry source generation, policy, expected revision and evidence lineage; asset application/version publication remains later work. |
| `LRN-006` | IMPLEMENTED | Deterministic and human-authorized authority explicitly outrank advisory/model-judge signals. |
| `LRN-007` | IMPLEMENTED | Learned projections retain exact source evaluation-fact and evidence references; deterministic rebuild preserves underlying measurements. |
| `LRN-008` | PARTIAL | Projection storage has no destructive delete path and raw facts are independent; PostgreSQL deletion/recovery runtime evidence remains open and the CLI keeps this case non-passing. |

---

# 7. Capability Governance (`CAP-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `CAP-001` | PARTIAL | G1 typed grant authority is now consumed by the shared G2 boundary on governed HTTP, MCP, and run worker/internal paths; generic job-poller context and federation adapter coverage remain open. |
| `CAP-002` | PARTIAL | `evaluate_capability_grants`, production constructors, and the G2 boundary default to deny; the legacy allow-all helper is test-only, while durable policy loading and full qualification remain open. |
| `CAP-003` | PARTIAL | G1 constraints are mapped by G2 to exact HTTP method/path, MCP tool/resource, and worker kind/run selectors; durable persistence, delegation, and broader selector semantics remain open. |
| `CAP-004` | PARTIAL | G1 rejects grants outside their validity interval and revoked grants; durable lifecycle persistence remains open. |
| `CAP-005..007` | PARTIAL | G3 provides an explicit `capability.delegate` permission, fail-closed child attenuation across capability/operation/resource/validity/risk/budget constraints, and bounded depth through `DelegatedCapability`; durable delegation revisions, audit history, and every agent-facing entry point remain open. |
| `CAP-008..009` | PARTIAL | G1 adds ordered `low/medium/high/critical` categories and context-risk elevation; G2 carries typed risk requests through the governed entry points, while richer runtime risk composition remains open. |
| `CAP-010` | PARTIAL | Approval records exist, but effective authority composition/hard-deny precedence is not fully implemented. |
| `CAP-011` | PARTIAL | G1 `PolicyDecision` retains exact policy version and typed input state, and G2 requires it before governed dispatch; durable decision/audit persistence remains open. |
| `CAP-012` | PARTIAL | Approval operation_hash exists; exact material intent binding semantics need normalization/conformance. |
| `CAP-013` | PARTIAL | G3 adds executable hierarchical reservation/charge checks for one unit dimension plus a `BudgetPolicyEngine` adapter that denies after allocation is charged; BudgetAccount remains a separate primitive, while durable auditable reservations, releases/corrections, and multi-dimensional accounting remain open. |
| `CAP-014` | PARTIAL/VERIFY | G3 denies self-delegation in the domain kernel and maps the negative case, but uniform agent-facing enforcement and durable audit still need runtime proof. |

---

# 8. Identity / Workspace / Federation (`IDW-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `IDW-001` | CANDIDATE/strong | Workspace-scoped RLS is real and pervasive. |
| `IDW-002` | PARTIAL/VERIFY | No evidence current local scope deliberately means federation-global; add conformance. |
| `IDW-003` | PARTIAL | RequestContext identity/workspace exists; authority composition is incomplete. |
| `IDW-004` | IMPLEMENTED/PARTIAL | In-memory G4 tests require an exact source grant revision plus target-owned acceptance; durable/runtime wiring remains open. |
| `IDW-005` | IMPLEMENTED/PARTIAL | `ShareTarget::AnyWorkspace` is rejected and exact target workspaces are required; transport/schema enforcement is not wired. |
| `IDW-006` | IMPLEMENTED/PARTIAL | `ReShare` and transitive mount-of-mount authority are rejected in G4; G5 also rejects transitive federation trust. Downstream retrieval/export paths remain open. |
| `IDW-007` | IMPLEMENTED/PARTIAL | G4 independently evaluates source grant, mount and target policy; G5 validates exact remote identity, relationship status, operation and policy version. Universal entry-point integration remains open. |
| `IDW-008` | IMPLEMENTED/PARTIAL | G4 and G5 fail closed for revoke/expiry/stale identity and suspended/revoked/expired federation relationships; durable propagation/cache invalidation is open. |
| `IDW-009..010` | IMPLEMENTED/PARTIAL | `SharedMemoryRef` preserves source workspace, exact memory/revision/grant context and has no local conversion; local derivation wiring remains open. |
| `IDW-011` | MISSING/PARTIAL | Provenance primitives exist, but local derivation from mounted content protocol absent. |
| `IDW-012` | IMPLEMENTED/PARTIAL | G5 provides an in-memory federation relationship/trust boundary and explicit trust-vs-local-permission intersection; remote transport, cryptographic attestation and adapter runtime remain open. |
| `IDW-013` | IMPLEMENTED/PARTIAL | Immutable in-memory `ShareDisclosure` records survive source revoke; durable audit persistence remains open. |
| `IDW-014` | IMPLEMENTED/PARTIAL | G4 does not alter RLS or introduce SQL bypass; PostgreSQL runtime evidence and migration normalization remain open. |

---

# 9. Health / Integrity / Repair (`HLT-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `HLT-001` | IMPLEMENTED/PARTIAL | `HealthFinding` carries invariant/version, scope, evidence, severity and repairability; durable persistence and adapter wiring remain open. |
| `HLT-002` | IMPLEMENTED/PARTIAL | `HealthProjection` is an explicit dependency-aware projection over findings; no durable projection/rebuild adapter exists. |
| `HLT-003` | IMPLEMENTED/PARTIAL | `Unknown` is first-class and never combined into `Healthy`; runtime coverage/freshness remains open. |
| `HLT-004` | IMPLEMENTED | `HealthSeverity` and `RepairRisk` are separate dimensions. |
| `HLT-005` | IMPLEMENTED/PARTIAL | Versioned `InvariantDefinition` and unique in-memory `InvariantRegistry` exist; durable registry lifecycle remains open. |
| `HLT-006` | IMPLEMENTED/PARTIAL | Plans are derived from findings and preserve an input state reference; an authoritative-source adapter is not wired. |
| `HLT-007` | CONFLICT | CLI rebuild directly mutates derived state rather than checker → plan → executor separation. |
| `HLT-008..010` | IMPLEMENTED/PARTIAL | Immutable `RepairPlan`, stale rejection and explicit authorization/capability input exist; durable policy/execution integration remains open. |
| `HLT-011` | IMPLEMENTED | `RepairExecution::Succeeded` does not close a finding. |
| `HLT-012..013` | IMPLEMENTED/PARTIAL | `VerificationRun` controls resolve/reopen transitions; durable evidence storage and runtime verification adapters remain open. |
| `HLT-014..016` | IMPLEMENTED/PARTIAL | Occurrences, recurrence/flapping assessment and bounded budget/cooldown exist in memory; durable retry history remains open. |
| `HLT-017` | IMPLEMENTED/PARTIAL | Projection follows declared dependencies and ignores unrelated findings; full runtime health graph is not wired. |
| `HLT-018..019` | IMPLEMENTED/PARTIAL | Suppression/AcceptedRisk require actor/reason/expiry-or-policy/audit metadata and preserve observed state; persistence/revocation runtime remains open. |
| `HLT-020` | PARTIAL | Repair operations are represented as immutable plan steps, but idempotency/restartability metadata and adapter proof remain open. |

---

# 10. External Effects (`EXT-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `EXT-001` | IMPLEMENTED/PARTIAL | Immutable `ExternalEffectIntent` exists before adapter dispatch; durable persistence remains open. |
| `EXT-002` | IMPLEMENTED/PARTIAL | Shared application `ExternalEffectService` uses the existing authorization boundary; budget persistence and all provider entry-point wiring remain open. |
| `EXT-003..005` | IMPLEMENTED/PARTIAL | Adapter descriptors declare delivery, idempotency, reversibility, reconciliation and dry-run semantics; provider proof remains open. |
| `EXT-006` | IMPLEMENTED | Dispatch produces explicit `UNKNOWN` for ambiguous adapter outcomes and rechecks preconditions before dispatch. |
| `EXT-007..010` | IMPLEMENTED/PARTIAL | UNKNOWN, no-auto-retry, strongest-evidence reconciliation and evidence preservation exist in memory; durable recovery integration remains open. |
| `EXT-011` | IMPLEMENTED | Reversibility is an explicit per-effect declaration. |
| `EXT-012..013` | IMPLEMENTED | Compensation creates a new effect identity and preserves the compensated-effect relation. |
| `EXT-014..015` | IMPLEMENTED/PARTIAL | Final precondition and authorization checks exist; deployed provider commit-time integration remains open. |
| `EXT-016` | IMPLEMENTED/PARTIAL | Immutable receipt/evidence record exists after adapter dispatch; durable receipt storage remains open. |
| `EXT-017` | IMPLEMENTED | Receipt acknowledgement is not treated as business confirmation. |
| `EXT-018` | IMPLEMENTED/PARTIAL | Intent stores normalized argument digest rather than raw payload; full SecretRef/DataPolicy enforcement remains open. |

---

# 11. Incident / Recovery / Revalidation (`REC-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `REC-001` | IMPLEMENTED/PARTIAL | Typed `Incident` aggregate preserves type, scope, severity, triggering findings and evidence; durable incident persistence remains open. |
| `REC-002` | IMPLEMENTED/PARTIAL | `ContainmentAction` and Incident lifecycle enforce containment before recovery; runtime capability/workspace isolation adapters remain open. |
| `REC-003` | IMPLEMENTED/PARTIAL | Explicit `TrustStateRecord` distinguishes `TRUSTED`, `DEGRADED_TRUST`, `UNTRUSTED` and `REVALIDATING`; durable propagation remains open. |
| `REC-004` | IMPLEMENTED/PARTIAL | Recovery classification and `RevalidationRun` prevent restart from directly restoring trust; durable startup orchestration remains open. |
| `REC-005..006` | IMPLEMENTED/PARTIAL | E1–E4 exposes UNKNOWN/reconciliation and unsafe-retry denial; restart classification and durable recovery orchestration remain open. |
| `REC-007..009` | PARTIAL/strong for Run | Run checkpoint validation/replay/projection rebuild exists; installation/domain recovery point semantics incomplete. |
| `REC-010` | IMPLEMENTED/PARTIAL | `RecoveryPoint` records state/sequence/scope/integrity/provenance and rejects invalid restore points; runtime canonical-history adapter remains open. |
| `REC-011..013` | IMPLEMENTED/PARTIAL | Typed `RevalidationCheck`/`RevalidationRun` and trust transition reject failed/inconclusive promotion; full invariant runner remains open. |
| `REC-014` | IMPLEMENTED/PARTIAL | `DivergentHistory` classifies to `HUMAN_REQUIRED`; domain reconciliation workflow is not wired. |
| `REC-015` | PARTIAL | Recovery classification is bounded and explicit; retry budgets/loop detection remain runtime work. |
| `REC-016` | PARTIAL/VERIFY | Logs/events/checkpoints exist; forensic-preservation policy before destructive recovery is not formal. |
| `REC-017` | PARTIAL | Trust state remains separate from capability restoration; progressive runtime policy stages remain open. |
| `REC-018` | IMPLEMENTED/PARTIAL | Incident, revalidation and recovery models retain typed evidence and references; durable replay/persistence remains open. |

---

# 12. Crypto / Data Governance (`GOV-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `GOV-001` | IMPLEMENTED/PARTIAL | `DataPolicy` and crypto/key metadata are separate domain gates; deployment-specific crypto enforcement remains open. |
| `GOV-002` | CANDIDATE | Public/Internal/Confidential/Restricted ordering exists. |
| `GOV-003..004` | IMPLEMENTED/PARTIAL | `DataClassification`, conservative `ClassificationLineage` and evidence-backed `DeclassificationDecision` are implemented in memory. |
| `GOV-005..006` | IMPLEMENTED/PARTIAL | Opaque `SecretRef`, authorization-bound lease metadata and `KeyProvider` boundary prevent secret bytes in durable-shaped models; backend wiring remains open. |
| `GOV-007` | TYPE_ONLY/PARTIAL | WorkspaceKek/migration exists; actual governed CAS envelope encryption runtime not proven. |
| `GOV-008` | IMPLEMENTED/PARTIAL | `KeyReference` stores provider/key/version/purpose/scope/algorithm metadata and lifecycle; provider rotation runtime remains open. |
| `GOV-009` | PARTIAL | Artifact content hash and signed-export type exist; formal guarantee separation not uniformly encoded. |
| `GOV-010` | IMPLEMENTED/PARTIAL | `AuditIntegrityChain` stores content digests and verifies append-only continuity; signed deployment audit checkpoints remain open. |
| `GOV-011..012` | MISSING | Capability × DataPolicy intersection impossible until DataPolicy exists. |
| `GOV-013..014` | IMPLEMENTED/PARTIAL | `RetentionPolicy` and `DataHold` model lifecycle/authority/reason; durable policy enforcement remains open. |
| `GOV-015..016` | IMPLEMENTED/PARTIAL | Logical/physical/crypto deletion semantics and remaining-copy evidence are explicit; backup execution remains open. |
| `GOV-017` | PARTIAL | Deletion verification preserves dependency evidence and remaining copies; dependent-claim revalidation adapter remains open. |
| `GOV-018..019` | IMPLEMENTED/PARTIAL | `DataExportPlan`/`ExportBundle` enforce exact scope, recipient, classification, provenance and integrity metadata; transport remains open. |
| `GOV-020` | PARTIAL | G5 prevents federation identity/trust from authorizing local memory and G6 makes the governance hard-gate case explicit; complete DataPolicy/classification/destination enforcement is absent. |
| `GOV-021..022` | PARTIAL | Sensitivity/DataDestination/redaction foundations and acceptance scenarios exist; universal provider boundary enforcement incomplete. |
| `GOV-023` | MISSING | Crypto anomaly → trust Incident/Revalidation absent. |
| `GOV-024..025` | IMPLEMENTED/PARTIAL | G6 requires exact policy-version evidence for the bounded governance gate; durable policy decision history and retroactive-policy runtime semantics remain open. |
| `GOV-026` | IMPLEMENTED/PARTIAL | `DeletionVerification` records checked refs, semantics, holds, remaining copies and evidence; storage/backup adapters remain open. |

T1–T8 also closes the bounded `GOV-011..012` model boundary through `DataPolicy` evaluation of classification, destination and required-capability presence, and provides the Incident/Revalidation target for `GOV-023`; universal entry-point wiring and anomaly detection remain open.

---

# 13. Qualification / Conformance (`QUAL-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `QUAL-001` | IMPLEMENTED/PARTIAL | `QualificationBundle` and the existing conformance registry model explicit profile/evidence boundaries; durable qualification storage remains open. |
| `QUAL-002` | IMPLEMENTED/PARTIAL | G6 derives required MUST closure for FEDERATION/TRUSTED and denies missing evidence; complete per-requirement QualificationBundle storage is absent. |
| `QUAL-003` | IMPLEMENTED/PARTIAL | `QualificationBundle` binds source/build/configuration/environment/manifest identity into a target digest. |
| `QUAL-004` | IMPLEMENTED/PARTIAL | TRUSTED gate composes the existing profile dependency closure; baseline lifecycle is now explicit, while full release harness remains open. |
| `QUAL-005` | MISSING/PARTIAL | Schema artifacts exist, but no target CapabilityManifest with qualified profiles/limitations. |
| `QUAL-006` | IMPLEMENTED/PARTIAL | G6 treats skipped and inconclusive required evidence as hard failures; it is not yet the durable qualification runner. |
| `QUAL-007` | IMPLEMENTED/PARTIAL | G6 hard-denies failed/not-applicable/skipped/inconclusive federation/governance evidence; no metric path can compensate, while broader runtime gates remain open. |
| `QUAL-008` | IMPLEMENTED/PARTIAL | Existing deterministic fault evidence plus T1–T8 recovery/revalidation contracts provide the Trust-side gate input; deployment fault harness remains open. |
| `QUAL-009` | IMPLEMENTED/PARTIAL | TRUSTED gate requires complete evidence and a qualified matching baseline; end-to-end golden scenario execution remains open. |
| `QUAL-010` | IMPLEMENTED/PARTIAL | T1–T8 fixture is machine-readable and reproducible for the bounded contract; full profile framework remains open. |
| `QUAL-011` | PARTIAL | Adapter/provider metadata boundaries exist across prior slices; deployment adapter qualification remains open. |
| `QUAL-012` | PARTIAL/VERIFY | Current unsupported HTTP surfaces often fail truthfully, but compatibility policy is not target-wide. |
| `QUAL-013` | IMPLEMENTED/PARTIAL | `QualificationBundle` consumes machine-readable per-requirement evidence and retains known limitations; signing/storage remain open. |
| `QUAL-014..015` | IMPLEMENTED/PARTIAL | `QualificationBaseline` models qualified/stale/invalidated/failed lifecycle and target matching. |
| `QUAL-016` | PARTIAL | Compose/test separation exists; explicit destructive qualification isolation contract absent. |
| `QUAL-017` | IMPLEMENTED/PARTIAL | G5 binds remote qualification metadata to local issuer/profile policy and G6 rejects `RemoteSelfAsserted`; cryptographic attestation and remote qualification lifecycle remain open. |
| `QUAL-018` | IMPLEMENTED/PARTIAL | `TrustedQualificationGate` rejects incomplete/mismatched/self-asserted evidence and requires full MUST closure; no deployment qualification claim is made. |

---

# 14. Coverage by release milestone

## v0.2 — Correct

**Foundation readiness:** strong, but target not yet conformant.

Most reusable:

- Run versioned history/replay;
- Memory revisions/lifecycle;
- provenance seed;
- RLS;
- retrieval shell/RRF/token bound.

Blocking gaps:

- Claim/conflict/reconciliation;
- generic temporal validity;
- exact revision hydration in retrieval;
- real ContextPack 2.0 representations;
- current/as-of semantics;
- minimum authorization gates around retrieval/mutation.

## v0.3 — Learn

**Foundation readiness:** medium.

Evaluation and execution data exist, but evaluator authority, evidence and governed learning changes are missing.

## v0.4 — Govern

**Foundation readiness:** medium-low.

RLS/approvals/capability enum are useful, but constrained grants/delegation/risk and grant+mount sharing need substantial work.

## v0.5 — Understand

**Foundation readiness:** medium.

Doctor/rebuild are valuable, but findings/invariant registry/RepairPlan/verification lifecycle require normalization.

## v0.6 — Connect

**Foundation readiness:** low-medium.

Tool execution types exist, but target side-effect semantics are largely absent.

## v1.0 — Trust

**Foundation readiness:** low for target contract.

Run recovery and sensitivity/key seeds exist, but trust/revalidation/governance/qualification systems are missing.

---

# 15. Conversion rule for future implementation work

A coverage entry may only move from `PARTIAL/CANDIDATE` to a real conformance `PASS` when:

```text
requirement ID
→ implementation path
→ executable conformance case
→ evidence artifact
```

exists for the exact build/profile under test.

This matrix should therefore become the source for implementation PR acceptance criteria rather than being treated as a one-time architecture report.
