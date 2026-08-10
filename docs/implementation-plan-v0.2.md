# Vestrace v0.2+ — Implementation Milestone / PR Plan

**Planning baseline:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Architecture baseline:** frozen `docs/architecture-v0.2` normative set  
**Date:** 2026-08-10  
**Status:** planning artifact only — does not authorize code changes on `docs/architecture-v0.2`

## 1. Implementation strategy

The repository already contains valuable foundations. The implementation program should therefore be **incremental normalization**, not a rewrite.

Preserve and extend:

- Run event sourcing / replay / checkpoints;
- PostgreSQL + RLS;
- Memory stable identity/revisions;
- idempotency/jobs/outbox;
- retrieval RRF shell/journaling;
- cognitive asset revisions;
- diagnostics/doctor;
- existing acceptance scenario ideas.

Normalize before exposing as stable public architecture:

- ContextPack builder;
- simple cross-workspace grant;
- ToolInvocation side-effect semantics;
- direct rebuild CLI;
- coarse capability HashSet authorization;
- potentially overlapping WorkflowExecution/Run execution responsibilities.

---

# 2. Branching rule

Implementation must not occur on `docs/architecture-v0.2`.

Recommended first implementation branch after review:

```text
impl/v0.2-correct
```

or one branch per PR from updated `main`.

Before code starts:

1. merge/rebase the approved architecture docs as appropriate;
2. re-run gap analysis if `main` moved materially;
3. create PR-specific acceptance criteria from stable requirement IDs;
4. preserve forward-only migrations.

---

# 3. PR design rules

Every implementation PR must include:

```text
Normative requirement IDs
→ implementation changes
→ migration impact
→ tests / conformance cases
→ docs/current-implementation update
```

A PR is not complete because code compiles. Its targeted MUST requirements need executable evidence.

No PR may:

- edit an already-applied migration;
- create a second event/execution runtime;
- expose a target placeholder as production-safe without required semantics;
- silently weaken RLS/capability/data-policy boundaries;
- convert UNKNOWN into implicit retry permission.

---

# 4. Phase 0 — Implementation alignment / test harness

This phase converts the documentation contract into a development feedback loop before high-risk domain changes.

## PR-0.1 — Requirement-aware conformance harness skeleton

### Goal

Create test infrastructure that can name v0.2 requirement IDs without pretending the profile is already qualified.

### Scope

- conformance test metadata type/fixture;
- mapping from test case → requirement IDs;
- machine-readable local test result format;
- distinction `test result` vs `qualification result`;
- no QualificationBundle/trust claim yet.

### Requirements enabled

- `QUAL-001`, foundation for `QUAL-002`, `QUAL-013`.

### Migration impact

None expected.

### Exit gate

At least existing strong current behaviors (Run version conflict, Memory lifecycle, token budget, workspace isolation) can be run as named requirement-aware tests without changing behavior.

---

## PR-0.2 — Execution ownership reconciliation

### Goal

Prevent current `WorkflowExecution/StepExecution` from evolving into a competing runtime.

### Scope

- document/code-level ownership mapping:
  `AgentRun → RunStep → typed workflow/model/tool history refs`;
- introduce missing linkage/reference fields if necessary;
- keep legacy history readable;
- no generic Task/Action aggregate.

### Requirements

- `ARC-004`, `ARC-005`, `MUT-007`.

### Migration impact

Possible additive FKs/reference columns between current execution records and Run/RunStep identities.

### Exit gate

One authoritative lifecycle owner for durable execution; secondary execution records are typed history/operation records.

---

# 5. v0.2 — Correct

Target profile: **CORE + MEMORY**.

The first product milestone should focus on correctness of persistent cognition and retrieval before autonomy expansion.

## PR-C1 — Generic temporal event and Memory revision model

### Goal

Bring generic Event/Memory time semantics up to the standard already demonstrated by Run events.

### Domain changes

Add/normalize:

```text
Event
├─ occurred_at?
├─ recorded_at
├─ correlation_id?
├─ causation_id?
└─ integrity/classification metadata as required

MemoryRevision
├─ valid_from?
├─ valid_until?
├─ classification
├─ change_reason
├─ created_by
└─ canonical_hash

Memory
└─ state_revision
```

### Migration impact

Forward-only additive migrations for `events`, `memories`, `memory_revisions`.

Backfill policy must be explicit:

- existing `events.created_at` can seed `recorded_at`;
- existing occurrence time remains unknown rather than fabricated;
- existing memory validity remains open/unknown;
- hashes computed deterministically from canonical representation.

### Requirements

- `TMP-001..007` (applicable subset);
- `MEM-001..010`;
- `MUT-001..002`.

### Exit gate

Temporal/concurrency golden tests cover:

- unknown occurred_at;
- future validity;
- expired validity;
- stale state revision;
- historical revision preservation.

---

## PR-C2 — EvidenceRef / Derivation v2

### Goal

Make provenance exact and extensible before adding Claims.

### Domain

Introduce a typed EvidenceRef model supporting exact source revisions/operations while preserving compatibility with existing MemorySource/Event provenance.

Extend Derivation to include:

- input refs;
- output ref;
- execution/model refs;
- policy/prompt versions where applicable.

### Migration impact

Prefer additive normalized evidence-link/derivation-input tables over destructive mutation of existing provenance rows.

Existing MemorySource rows should migrate/bridge to the new evidence model.

### Requirements

- `MEM-005`, `MEM-006`, `MEM-012`;
- groundwork for `LRN-001`, governance lineage and deletion dependency analysis.

### Exit gate

Every newly created derived canonical Memory has provable closure to source evidence.

---

## PR-C3 — Claim / Assessment / Conflict core

### Goal

Implement the missing semantic layer between stored Memory and asserted current truth.

### Domain

Add:

```text
Claim
ClaimEvidenceLink
ClaimAssessment
Conflict
Supersession/Reconciliation references
```

### Important rule

Do not automatically create a Claim for every Memory kind. Assertion-like cognition uses Claim; procedures/tasks may remain non-propositional.

### Migration impact

New claim/evidence/assessment/conflict tables with RLS and version/lifecycle constraints.

### Requirements

- `MEM-013..019`;
- enables `RET-003`, `RET-008`, `RET-009`;
- foundation for evidence-deletion revalidation.

### Exit gate

Golden scenario:

```text
Memory/evidence A supports Claim X
→ evidence B contradicts X
→ X becomes Contested
→ retrieval cannot silently return X as uncontested current truth
→ reconciliation preserves both histories/evidence
```

---

## PR-C4 — Cognitive Mutation / Reconciliation service

### Goal

Unify cognitive mutations under explicit expected-state/provenance/reconciliation rules.

### Scope

- mutation envelope/command types;
- deterministic vs policy-guided vs semantic vs human-required classification;
- explicit reconciliation result with evidence/policy refs;
- no semantic latest-wins shortcut.

### Migration impact

May add reconciliation/result tables; prefer references to existing revisions/events rather than copying payloads.

### Requirements

- `MUT-001..008`;
- `TMP-006..010` applicable parts.

### Exit gate

Concurrent conflicting cognitive mutations cannot silently overwrite each other.

---

## PR-C5 — Retrieval exact-revision hydration and authorization boundary

### Goal

Fix the biggest semantic weakness in the current retrieval stack before adding more channels.

### Replace current pipeline segment

From:

```text
RetrievalCandidate explanation
→ ContextItem rendered_text
```

To:

```text
candidate refs
→ authorize/filter
→ hydrate exact revision/claim/evidence
→ temporal/conflict validation
→ representation selection
→ ContextItem
```

### Scope

- candidate `revision_id` becomes exact where required;
- canonical content hydration repository;
- current/temporal validity checks;
- unresolved conflict handling;
- inclusion evidence/explanation;
- minimum capability/RLS/classification filter hooks.

### Migration impact

May extend retrieval journals/context-pack manifests to store exact revision/source refs and policy/generation metadata.

### Requirements

- `RET-001..008`, `RET-013`, `RET-015` relevant subset.

### Exit gate

ContextPack contains actual authorized MemoryRevision/Claim representation, never retrieval explanation text as canonical content.

---

## PR-C6 — ContextPack 2.0 representation/section assembly

### Goal

Implement semantic sectioning and real representation ladder.

### Scope

- classify item to constraints/facts/decisions/tasks/procedures/etc.;
- actual `Full/Summary/Atomic/Reference` transformations;
- provenance retained at every level;
- hard token/content budget;
- warnings/degradation;
- no authority elevation.

### Important rule

A `Reference` must be a reference representation, not simply truncated arbitrary content.

### Requirements

- `RET-004..008`, `RET-013`.

### Exit gate

Golden packs verify correct section placement, exact source refs and deterministic budget compliance.

---

## PR-C7 — Temporal retrieval + multi-channel wiring

### Goal

Make existing retrieval intent/time/channel abstractions real.

### Scope

- `KNOWN_AS_OF`, `VALID_AS_OF`, `RECONSTRUCTED_AS_OF` application semantics;
- exact/structured channels;
- vector channel if ready and qualified;
- RRF over real multiple channels;
- degradation and safe-fallback semantics;
- generation-based invalidation.

### Migration impact

- retrieval generation counters/metadata;
- potentially embedding-space/version metadata normalization.

### Requirements

- `TMP-004..005`;
- `RET-009..012`, `RET-014..015` where applicable within workspace.

### Exit gate

Current, historical and reconstructed results differ correctly in late-arriving-evidence tests.

---

## PR-C8 — v0.2 CORE + MEMORY conformance gate

### Goal

Turn current coverage candidates into actual evidence.

### Scope

- requirement mapping for all v0.2 release-gate MUSTs;
- wired integration tests with PostgreSQL/RLS;
- HTTP/MCP security-path checks where exposed;
- migration-from-existing-main fixture;
- derived-state delete/rebuild equivalence;
- no external model dependency.

### Migration impact

No product migration expected; test fixtures/migration validation only.

### Exit gate

No required CORE+MEMORY MUST requirement is `SKIPPED/INCONCLUSIVE`.

This is the point at which v0.2 `Correct` may be claimed.

---

# 6. v0.3 — Learn

Target profile: **COGNITION**.

## PR-L1 — Evaluation fact v2

Add typed:

- target execution ref;
- evaluator kind;
- metric/check kind;
- evidence refs;
- evaluator revision;
- policy/version authority.

Preserve existing evaluation records through compatibility/backfill.

Requirements: `LRN-001`, `LRN-002`, `LRN-006..008`.

## PR-L2 — Learned projection / proposal boundary

Introduce explicit derived learned conclusions and governed proposals for Agent/Skill/Workflow/routing changes.

No learning operation may alter capability/security authority automatically.

Requirements: `LRN-002..005`.

## PR-L3 — COGNITION benchmark/conformance gate

Convert v0.1 lifecycle fixtures into property-oriented benchmark cases with provenance, stale-memory, contradiction and long-horizon checks.

---

# 7. v0.4 — Govern

## PR-G1 — CapabilityGrant / PolicyDecision kernel

Implement durable constrained grants:

```text
operation/tool
resource selector
validity
budget
risk ceiling
conditions/obligations
delegation policy
```

PolicyDecision becomes evidence-producing and version-bound.

Requirements: `CAP-001..004`, `CAP-008..013`.

## PR-G2 — Universal authorization adapters

Route HTTP, MCP, worker/internal commands and execution dispatch through the same application authorization boundary.

Eliminate production reliance on coarse allow-all paths except explicit local-trusted configuration with declared limitations.

Requirements: `CAP-001..014` negative-path coverage.

## PR-G3 — Delegation attenuation + hierarchical budgets

Implement explicit parent/child delegation, max depth and typed budget allocations/accounting.

Requirements: `CAP-005..007`, `CAP-013`.

## PR-G4 — Cross-workspace sharing v2 schema

Do **not** expose existing simple `cross_workspace_memory_grants` as final API.

Add:

- MemoryShareGrant lifecycle;
- immutable grant revisions;
- target MemoryMount acceptance;
- SharedMemoryRef;
- stale/revoke/expire semantics;
- source + target policy decisions.

Migrate existing grant rows conservatively as inactive/legacy proposals requiring explicit acceptance, not automatically active mounts.

Requirements: `IDW-004..014`.

## PR-G5 — Federation trust boundary

Implement remote identity/qualification evidence ingestion with local policy evaluation; never import remote permissions directly.

## PR-G6 — Govern/Federation conformance gate

Security bypass matrix across all surfaces.

---

# 8. v0.5 — Understand

## PR-H1 — Invariant Registry + HealthFinding

Replace fixed diagnostics as the top-level semantic model with:

```text
InvariantDefinition metadata
+ typed checker
→ HealthFinding
→ HealthOccurrence
```

Existing PgDiagnostics checks become typed checker implementations.

Requirements: `HLT-001..005`, `HLT-014`, `HLT-017`.

## PR-H2 — RepairPlan / execution protocol

Wrap current rebuild implementations as low-level deterministic repair operations.

Top-level flow becomes:

```text
finding
→ plan
→ precondition check
→ capability/policy
→ Run/operation execution
→ verification
```

Current direct SQL rebuild logic can remain internally but must no longer bypass plan/authority semantics.

Requirements: `HLT-006..013`, `HLT-020`.

## PR-H3 — recurrence / disposition / repair budgets

Add occurrences, recurrence/flapping, cooldown/budgets and separate suppression/accepted-risk disposition per ADR-0009.

Requirements: `HLT-014..019`.

## PR-H4 — doctor/rebuild CLI v2

Conceptual behavior:

```text
vestrace doctor            # inspect
vestrace doctor --plan     # produce plan
vestrace repair <plan>     # governed execute
```

`rebuild` may remain as an explicit specialized command, but must use or delegate to the same RepairPlan/verification semantics.

## PR-H5 — Understand conformance gate

Fault/repeat/stale-plan/flapping scenarios.

---

# 9. v0.6 — Connect

## PR-E1 — ExternalEffectIntent + adapter contract

Add immutable prepare semantics and adapter declarations:

- delivery;
- idempotency;
- reconciliation;
- reversibility;
- dry run;
- preconditions;
- secret/data destination.

## PR-E2 — Effect dispatch + receipt + UNKNOWN

Implement target lifecycle and the invariant:

```text
attempted ≠ acknowledged ≠ confirmed ≠ desired outcome
```

Timeout/crash after dispatch must be able to produce UNKNOWN.

## PR-E3 — reconciliation / compensation

Read-back strongest evidence, safe retry decisions and explicit compensation-as-new-effect.

## PR-E4 — external-effect fault suite / adapter qualification

Named fault points around dispatch/persistence boundaries.

This gate must pass before higher-risk autonomous effects are enabled.

---

# 10. v1.0 — Trust

## PR-T1 — Incident + containment + TrustState

Implement incident lifecycle, scope-specific trust and minimum-blast-radius containment.

## PR-T2 — Recovery classification + RevalidationRun

Build on existing Run recovery:

```text
SAFE_TO_RESUME
SAFE_TO_RETRY
MUST_RECONCILE
MUST_ABORT
HUMAN_REQUIRED
```

Add evidence-producing revalidation and trust transition barrier.

## PR-T3 — SecretRef / KeyProvider / key lifecycle

Normalize existing WorkspaceKek seed into governed key references/providers and implement secret-reference boundaries.

## PR-T4 — DataPolicy / classification lineage / model boundary

DataPolicy intersects capability and applies purpose/destination/locality/redaction rules.

## PR-T5 — retention / hold / deletion verification

Implement lifecycle and dependency-aware deletion semantics.

Hard purge becomes one governed disposal mechanism, not the complete deletion contract.

## PR-T6 — governed export / audit integrity

Implement exact export manifests, encryption/signature options and tamper-evident audit checkpoints according to TRUSTED profile.

## PR-T7 — QualificationBundle / baseline lifecycle

Full build/config/environment identity, profile result, evidence refs, known limitations, optional signature.

## PR-T8 — TRUSTED qualification gate

Run full security/fault/recovery/governance suite; publish known limitations.

Only here may v1.0 Trust be claimed for the qualified target configuration.

---

# 11. Migration sequence constraints

## 11.1 Additive before destructive

Where legacy schemas already exist, prefer:

```text
add new tables/columns
→ dual-read/write or compatibility projection if needed
→ backfill
→ verify
→ switch authority
→ retire legacy path later
```

Avoid one-shot destructive schema conversion.

## 11.2 Existing cross-workspace grants

The current migration 0087 representation must **not** be auto-promoted to target Active sharing.

Safe migration strategy:

- preserve rows as legacy source proposals/references;
- require target acceptance to create MemoryMount;
- no implicit permission expansion.

## 11.3 Existing events

Backfill:

```text
recorded_at = existing created_at
occurred_at = NULL unless source evidence proves it
```

Never invent historical occurrence timestamps.

## 11.4 Existing Memory revisions

Backfill:

- open/unknown validity;
- classification according to safe current policy/default;
- canonical hash generated deterministically;
- created_by unknown/system migration actor if historical author cannot be proven — do not invent a person/agent.

## 11.5 Existing approvals

Preserve history. Do not retroactively claim old approvals satisfy new exact-intent/capability semantics unless their stored data proves it.

---

# 12. Parallelization opportunities

After v0.2 core domain contracts stabilize, some work can proceed in parallel:

```text
Evaluation v2 ───────────────┐
                              │
CapabilityGrant kernel ──────┼─ after core identity/version contracts
                              │
Invariant registry metadata ─┘
```

But these dependencies remain sequential:

```text
Claim/conflict
→ conflict-aware ContextPack

CapabilityGrant
→ safe delegation/sharing/effects

ExternalEffect UNKNOWN
→ cross-domain recovery/revalidation

Revalidation
→ TRUSTED qualification
```

---

# 13. Suggested PR sizing

Keep individual PRs reviewable:

- one domain boundary;
- one migration concern;
- one application/runtime path;
- explicit conformance tests.

Avoid mega-PRs that simultaneously change Memory, retrieval, capabilities and external execution.

A useful target is that reviewers can answer for each PR:

1. Which requirement IDs change status?
2. Which authoritative state changes?
3. What migration/backfill occurs?
4. What happens on crash/retry?
5. What new security boundary exists?
6. What test proves it?

---

# 14. First implementation tranche recommendation

Do **not** start with Health, federation or external effects despite their visible domain types.

The highest-leverage first tranche is:

```text
PR-0.1 requirement-aware tests
PR-0.2 execution ownership reconciliation
PR-C1 temporal schema
PR-C2 evidence v2
PR-C3 Claim/conflict
PR-C4 mutation/reconciliation
PR-C5 exact-revision retrieval hydration
PR-C6 ContextPack 2.0
PR-C7 temporal/multi-channel retrieval
PR-C8 CORE+MEMORY conformance gate
```

Why:

- every later subsystem depends on authoritative cognition and exact provenance;
- repair needs stable invariants over stable data;
- sharing needs exact revision/provenance identity;
- learning needs trustworthy evaluation targets;
- external effects and recovery need stable execution authority;
- qualification needs stable requirement semantics.

---

# 15. Implementation start gate

Code work should begin only after reviewers accept:

- `docs/gap-analysis-v0.2.md`;
- `docs/requirement-coverage-v0.2.md`;
- this implementation plan;
- the correction to current implementation status.

At that point the architecture documentation branch remains frozen, and implementation proceeds in a separate branch with requirement-driven PRs.
