# Vestrace v0.2 → v1.0 — Consolidated Implementation Plan

**Inspected implementation baseline:** `729d456f70f4de93c97d05cce795c09025c62f24`  
**Architecture baseline:** frozen v0.2 documentation integrated into `main`  
**Status:** documentation/planning only

## Strategy

Do not rewrite Vestrace. Preserve and extend the strongest existing foundations:

- Run event sourcing, replay and checkpoints;
- PostgreSQL + RLS;
- Memory stable identity/revisions;
- idempotency/jobs/outbox;
- retrieval RRF shell and journaling;
- cognitive asset revisions;
- diagnostics/doctor;
- existing acceptance scenarios.

Normalize before expanding public/autonomous behavior:

- execution ownership;
- temporal/event semantics;
- Claim/Conflict/provenance;
- ContextPack exact-revision hydration;
- constrained capability authority;
- grant+mount sharing;
- findings-first repair;
- external effects with UNKNOWN/reconciliation;
- trust/revalidation;
- crypto/data governance;
- qualification evidence.

## Planned sequence

### Phase 0 — Alignment

- **PR-0.1** Requirement-aware conformance harness.
- **PR-0.2** Single execution owner: `AgentRun` authoritative; workflow/step execution becomes typed history/projection.

### v0.2 Correct — CORE + MEMORY

- **C1** Generic temporal Event + Memory revision/state revision.
- **C2** EvidenceRef / Derivation v2.
- **C3** Claim / Assessment / Conflict core.
- **C4** Cognitive Mutation / Reconciliation service.
- **C5** Exact-revision retrieval hydration + authorization boundary.
- **C6** ContextPack 2.0 sections and Full/Summary/Atomic/Reference representations.
- **C7** Temporal retrieval + multi-channel wiring/degradation/invalidation.
- **C8** CORE + MEMORY conformance gate.

### v0.3 Learn — COGNITION

- **L1** Evaluation Fact v2 with exact target/evaluator/evidence refs.
- **L2** Learned projection/proposal boundary; no authority self-escalation.
- **L3** COGNITION benchmark/conformance gate.

### v0.4 Govern

- **G1** CapabilityGrant / PolicyDecision kernel.
- **G2** Universal authorization boundary across HTTP/MCP/worker/internal paths.
- **G3** Delegation attenuation + hierarchical budgets.
- **G4** Cross-workspace sharing v2: source grant revision + target MemoryMount.
- **G5** Federation trust boundary; remote trust != local permission.
- **G6** Govern/Federation hard gate.

### v0.5 Understand

- **H1** Invariant Registry + HealthFinding/Occurrence.
- **H2** RepairPlan → authorization → execution → verification.
- **H3** recurrence/flapping/disposition/repair budgets.
- **H4** doctor/plan/repair operator contract.
- **H5** Understand conformance gate.

### v0.6 Connect — AUTONOMY external-effect scope

- **E1** ExternalEffectIntent + adapter semantics.
- **E2** dispatch + receipt + first-class UNKNOWN.
- **E3** reconciliation + compensation as new effect.
- **E4** deterministic effect fault suite/adapter qualification.

### v1.0 Trust

- **T1** Incident + containment + TrustState.
- **T2** recovery classification + RevalidationRun.
- **T3** SecretRef / KeyProvider / key lifecycle.
- **T4** DataPolicy / classification lineage / model boundary.
- **T5** retention / hold / dependency-aware deletion verification.
- **T6** governed export / audit integrity.
- **T7** QualificationBundle / baseline lifecycle.
- **T8** TRUSTED qualification gate.

## Migration rules

```text
additive before destructive
unknown historical data stays unknown
applied migrations are immutable
legacy broad authority never widens during migration
cross-workspace legacy grants never auto-become accepted mounts
derived state may be rebuilt; canonical history may not be reconstructed from derived state
```

## PR completion rule

Every PR must answer:

1. Which requirement IDs are targeted?
2. Which authoritative state owner changes?
3. What forward-only migration/backfill is required?
4. What happens on stale state, crash, retry or ambiguity?
5. What security/governance boundary changes?
6. What executable evidence proves the requirement?
7. Which current-implementation docs must be updated?

## Dependency chain

```text
0.1 → 0.2
→ C1-C8
→ L1-L3
→ G1-G6
→ H1-H5
→ E1-E4
→ T1-T8
```

The architecture baseline is already in `main`. Before implementation begins, review current `main`; if runtime code or migrations have moved materially from the inspected baseline, re-run the gap delta first.
