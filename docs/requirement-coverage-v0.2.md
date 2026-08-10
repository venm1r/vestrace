# Vestrace v0.2 — Requirement Coverage Matrix

**Implementation snapshot:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Requirement source:** `docs/specs/vestrace-normative-invariants-v0.2.md`  
**Date:** 2026-08-10

> This matrix is an architecture/code-inspection result, **not conformance evidence**. `CANDIDATE` means the implementation appears to provide the required behavior and should be converted into a real conformance case. It does not mean the requirement has passed qualification.

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

Primary evidence:

- `crates/vestrace-domain/src/run/event.rs`
- `crates/vestrace-domain/src/run/*`
- `crates/vestrace-domain/src/event.rs`
- `crates/vestrace-domain/src/memory/revision.rs`

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

Critical implementation evidence:

- `crates/vestrace-application/src/retrieval/service.rs`
- `crates/vestrace-application/src/retrieval/context_builder.rs`
- `crates/vestrace-application/src/retrieval/rerank.rs`
- `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- `crates/vestrace-domain/src/retrieval/mod.rs`

---

# 6. Feedback / Learning (`LRN-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `LRN-001` | MISSING/PARTIAL | EvaluationRecord does not bind exact execution/model/tool/policy revisions comprehensively. |
| `LRN-002` | MISSING/PARTIAL | Evaluation records exist, but raw-evidence vs learned-projection boundary not modeled. |
| `LRN-003` | VERIFY/PARTIAL | No automatic authority self-modification found; target hard guarantee not centrally enforced. |
| `LRN-004` | VERIFY/PARTIAL | Cognitive assets are versioned, but no governed learning mutation protocol exists. |
| `LRN-005` | PARTIAL | Agent/Skill/Workflow revisions exist; feedback-driven change lifecycle absent. |
| `LRN-006` | MISSING | No evaluator-authority ordering encoded in evaluation model. |
| `LRN-007` | MISSING/PARTIAL | Learned performance conclusion with exact underlying measurements not modeled. |
| `LRN-008` | VERIFY | No destructive learned-projection path identified; requires conformance once projection model exists. |

Current evidence:

- `crates/vestrace-application/src/cognitive_ports.rs`
- `crates/vestrace-http/src/api/evaluations.rs`
- model/execution domain modules

---

# 7. Capability Governance (`CAP-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `CAP-001` | PARTIAL | Capability enum/policy helpers exist; roles are not shown as sole runtime check, but universal effective-authority boundary is absent. |
| `CAP-002` | CONFLICT/VERIFY | `AllowAllPolicyEngine` exists for trusted/local use; default-deny must be proven for production paths. |
| `CAP-003` | MISSING | Current Capability enum does not contain resource/validity/budget/risk/condition constraints. |
| `CAP-004` | MISSING/PARTIAL | Access token expiry exists; target capability-grant expiry does not. |
| `CAP-005..007` | MISSING | Delegation attenuation/depth absent. |
| `CAP-008..009` | MISSING | Central risk categories/context elevation absent. |
| `CAP-010` | PARTIAL | Approval records exist, but effective authority composition/hard-deny precedence is not fully implemented. |
| `CAP-011` | MISSING/PARTIAL | Policy bundles have versions; target PolicyDecision evidence object absent. |
| `CAP-012` | PARTIAL | Approval operation_hash exists; exact material intent binding semantics need normalization/conformance. |
| `CAP-013` | PARTIAL | BudgetAccount reserve exists; auditable multi-dimensional reservation/accounting not complete. |
| `CAP-014` | PARTIAL/VERIFY | Acceptance suite has self-elevation scenario, but uniform agent-facing enforcement needs runtime proof. |

Primary evidence:

- `crates/vestrace-domain/src/security/mod.rs`
- `crates/vestrace-application/src/security/policy_engine.rs`
- `crates/vestrace-domain/src/budget/mod.rs`
- `migrations/0012_tokens_policies_and_approvals.sql`
- `migrations/0023_policy_bundles_and_snapshots.sql`

---

# 8. Identity / Workspace / Federation (`IDW-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `IDW-001` | CANDIDATE/strong | Workspace-scoped RLS is real and pervasive. |
| `IDW-002` | PARTIAL/VERIFY | No evidence current local scope deliberately means federation-global; add conformance. |
| `IDW-003` | PARTIAL | RequestContext identity/workspace exists; authority composition is incomplete. |
| `IDW-004` | CONFLICT | Current one-sided CrossWorkspaceMemoryGrant lacks target target-side acceptance. |
| `IDW-005` | CANDIDATE/PARTIAL | Current grant has exact target workspace; lifecycle/API still undeveloped. |
| `IDW-006` | MISSING | No target anti-transitive mount semantics. |
| `IDW-007` | MISSING | Independent source + target policy checks absent. |
| `IDW-008` | MISSING | No mount lifecycle/stale/revoke enforcement. |
| `IDW-009..010` | MISSING | SharedMemoryRef absent. |
| `IDW-011` | MISSING/PARTIAL | Provenance primitives exist, but local derivation from mounted content protocol absent. |
| `IDW-012` | MISSING/PARTIAL | Federation/A2A types exist historically, but local trust vs data permission target runtime not identified. |
| `IDW-013` | VERIFY | No completed sharing lifecycle to test historical disclosure semantics. |
| `IDW-014` | PARTIAL/strong foundation | RLS remains in place, but current sharing table policy must be normalized rather than used as full authorization contract. |

Primary evidence:

- `crates/vestrace-domain/src/enterprise/mod.rs`
- `migrations/0087_workspace_envelope_encryption_and_cross_sharing.sql`
- `tests/workspace_envelope_encryption.rs`

---

# 9. Health / Integrity / Repair (`HLT-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `HLT-001` | PARTIAL | DiagnosticFinding exists but lacks invariant/scope/evidence/repairability target shape. |
| `HLT-002` | PARTIAL | DiagnosticReport is aggregation; no formal projection/raw finding contract. |
| `HLT-003` | MISSING/CONFLICT RISK | Current report can be clean based on configured checks; no explicit UNKNOWN/coverage freshness state. |
| `HLT-004` | MISSING | Severity vs repair risk separation absent. |
| `HLT-005` | MISSING | Versioned InvariantDefinition registry absent. |
| `HLT-006` | PARTIAL | Search-document rebuild is deterministic downward reconstruction, but not governed by target repair protocol. |
| `HLT-007` | CONFLICT | CLI rebuild directly mutates derived state rather than checker → plan → executor separation. |
| `HLT-008..010` | MISSING | RepairPlan, stale plan and repair capability/policy gate absent. |
| `HLT-011` | PARTIAL | Current rebuild runs post-checks; no finding lifecycle means success/closure separation is not formal. |
| `HLT-012..013` | PARTIAL/MISSING | Verification exists procedurally after rebuild, but no evidence-backed finding closure/reopen. |
| `HLT-014..016` | MISSING | Occurrences, recurrence/flapping, repair budgets absent. |
| `HLT-017` | MISSING | Declared health dependency propagation absent. |
| `HLT-018..019` | MISSING | AcceptedRisk/Suppression governed overlays absent. |
| `HLT-020` | PARTIAL | Low-level rebuild SQL is repeat-tolerant (`ON CONFLICT DO NOTHING`), but target repair-step idempotency contract not generalized. |

Primary evidence:

- `crates/vestrace-domain/src/diagnostics.rs`
- `crates/vestrace-application/src/diagnostics.rs`
- `crates/vestrace-cli/src/commands/doctor.rs`
- `crates/vestrace-cli/src/commands/rebuild.rs`

---

# 10. External Effects (`EXT-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `EXT-001` | MISSING | ExternalEffectIntent absent. |
| `EXT-002` | MISSING/PARTIAL | Capability primitives exist; exact effect authorization boundary absent. |
| `EXT-003..005` | MISSING | Adapter delivery/idempotency semantics absent. |
| `EXT-006` | MISSING/CONFLICT RISK | Tool state model has no target ambiguity contract for transport timeout. |
| `EXT-007..010` | MISSING/CONFLICT | `UNKNOWN`, no-auto-retry and reconciliation protocol absent. |
| `EXT-011` | MISSING | Reversibility classification absent. |
| `EXT-012..013` | MISSING | Compensation distinction/relation absent. |
| `EXT-014..015` | MISSING | Target precondition recheck/stale authorized intent absent. |
| `EXT-016` | MISSING | ExternalEffectReceipt absent. |
| `EXT-017` | MISSING | ACK vs desired outcome evidence model absent. |
| `EXT-018` | PARTIAL/VERIFY | Secret-handling foundations/config practices exist, but receipt/trace secret-safety target contract does not yet exist. |

Current seed:

- `crates/vestrace-domain/src/tool/mod.rs` (`Prepared/Committed/Verified/Failed`)

---

# 11. Incident / Recovery / Revalidation (`REC-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `REC-001` | MISSING | Incident aggregate absent. |
| `REC-002` | MISSING | Containment-first Incident lifecycle absent. |
| `REC-003` | MISSING | TrustState absent; restart/trust distinction cannot be enforced. |
| `REC-004` | PARTIAL | Run recovery/checkpoints exist, but cross-domain unfinished-work classification absent. |
| `REC-005..006` | MISSING | External effect UNKNOWN/reconciliation absent. |
| `REC-007..009` | PARTIAL/strong for Run | Run checkpoint validation/replay/projection rebuild exists; installation/domain recovery point semantics incomplete. |
| `REC-010` | MISSING | Availability/health/trust/revalidation states not represented together. |
| `REC-011..013` | MISSING | RevalidationRun/trust barrier absent. |
| `REC-014` | MISSING | Divergent-history reconciliation protocol absent. |
| `REC-015` | MISSING/PARTIAL | Jobs retries exist, but recovery-loop budget/detection not target-wide. |
| `REC-016` | PARTIAL/VERIFY | Logs/events/checkpoints exist; forensic-preservation policy before destructive recovery is not formal. |
| `REC-017` | MISSING | Progressive capability restoration absent. |
| `REC-018` | PARTIAL/CANDIDATE | Run history is append-only/replayable; incident/recovery facts do not yet exist. |

---

# 12. Crypto / Data Governance (`GOV-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `GOV-001` | PARTIAL | Sensitivity/redaction and workspace key types exist, but DataPolicy is absent. |
| `GOV-002` | CANDIDATE | Public/Internal/Confidential/Restricted ordering exists. |
| `GOV-003..004` | MISSING | Classification lineage/declassification decision absent. |
| `GOV-005..006` | MISSING/PARTIAL | No runtime SecretRef; config secret practices exist but target durable secret contract absent. |
| `GOV-007` | TYPE_ONLY/PARTIAL | WorkspaceKek/migration exists; actual governed CAS envelope encryption runtime not proven. |
| `GOV-008` | PARTIAL | Algorithm is stored as string in WorkspaceKek; lifecycle/agility contract incomplete. |
| `GOV-009` | PARTIAL | Artifact content hash and signed-export type exist; formal guarantee separation not uniformly encoded. |
| `GOV-010` | MISSING | Tamper-evident audit chaining/checkpoint signing absent. |
| `GOV-011..012` | MISSING | Capability × DataPolicy intersection impossible until DataPolicy exists. |
| `GOV-013..014` | MISSING | Retention lifecycle/DataHold absent. |
| `GOV-015..016` | MISSING/PARTIAL | Hard purge exists but deletion semantic classes/backup-aware claims absent. |
| `GOV-017` | MISSING | Evidence-deletion dependent-claim revalidation absent. |
| `GOV-018..019` | TYPE_ONLY/PARTIAL | SignedRunExport seed exists; governed export plan/manifest/provenance incomplete. |
| `GOV-020` | MISSING | Federation trust/data-policy composition absent. |
| `GOV-021..022` | PARTIAL | Sensitivity/DataDestination/redaction foundations and acceptance scenarios exist; universal provider boundary enforcement incomplete. |
| `GOV-023` | MISSING | Crypto anomaly → trust Incident/Revalidation absent. |
| `GOV-024..025` | PARTIAL | Policy bundle versions exist; decisions do not yet persist target evidence/version semantics. |
| `GOV-026` | MISSING | DeletionVerification target protocol absent. |

---

# 13. Qualification / Conformance (`QUAL-*`)

| Requirement | Status | Current evidence / gap |
|---|---|---|
| `QUAL-001` | MISSING/PARTIAL | Tests/acceptance exist, but explicit tests-vs-conformance-vs-qualification product model is new documentation only. |
| `QUAL-002` | MISSING | MUST requirements are not mapped to executable verification paths yet. |
| `QUAL-003` | MISSING | No qualification target identity bundle. |
| `QUAL-004` | MISSING | Profile dependency closure not implemented. |
| `QUAL-005` | MISSING/PARTIAL | Schema artifacts exist, but no target CapabilityManifest with qualified profiles/limitations. |
| `QUAL-006` | MISSING | No profile runner interpreting skipped MUST as failure. |
| `QUAL-007` | PARTIAL conceptually | Acceptance has security negative scenarios, but no hard-gate qualification engine. |
| `QUAL-008` | MISSING | Deterministic fault suite for AUTONOMY/TRUSTED absent. |
| `QUAL-009` | MISSING | TRUSTED recovery+revalidation qualification impossible until RevalidationRun exists. |
| `QUAL-010` | PARTIAL | v0.1 acceptance uses deterministic fixtures/properties; no formal cognition profile framework. |
| `QUAL-011` | MISSING | Adapter qualification framework absent. |
| `QUAL-012` | PARTIAL/VERIFY | Current unsupported HTTP surfaces often fail truthfully, but compatibility policy is not target-wide. |
| `QUAL-013` | MISSING | QualificationBundle absent. |
| `QUAL-014..015` | MISSING | Qualification baseline drift/lifecycle absent. |
| `QUAL-016` | PARTIAL | Compose/test separation exists; explicit destructive qualification isolation contract absent. |
| `QUAL-017` | MISSING | Federation qualification trust policy absent. |
| `QUAL-018` | MISSING | No TRUSTED profile claim/bundle exists. |

Current scenario seeds:

- `tests/v01_acceptance.rs`
- `docs/acceptance/v0.1.md`
- compose smoke tests / current schema generation

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
