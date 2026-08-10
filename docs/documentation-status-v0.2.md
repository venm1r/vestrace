# Vestrace v0.2 Documentation Status

**Branch:** `docs/architecture-v0.2`  
**Baseline:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Date:** 2026-08-10  
**Documentation state:** **FROZEN ARCHITECTURE + COMPLETED GAP ANALYSIS — implementation not started**

## 1. Purpose

This file classifies the documentation tree so target architecture, current implementation, historical design, gap analysis and implementation plans cannot be mistaken for each other.

## 2. Document classes

### A. Normative target architecture

These documents define the v0.2+ target and take precedence for future architecture work:

- `docs/specs/vestrace-architecture-contract-v0.2.md`
- `docs/specs/vestrace-domain-model-v0.2.md`
- `docs/specs/vestrace-normative-invariants-v0.2.md`
- `docs/specs/vestrace-trust-authority-model-v0.2.md`
- `docs/specs/vestrace-data-temporal-model-v0.2.md`
- `docs/specs/vestrace-execution-external-effects-contract-v0.2.md`
- `docs/specs/vestrace-health-repair-incident-contract-v0.2.md`
- `docs/specs/vestrace-crypto-data-governance-contract-v0.2.md`
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`
- `docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md`
- accepted documents under `docs/adr/`

`docs/specs/README.md` is the normative index.

### B. Architecture/domain entry points

- `README.md`
- `docs/architecture.md`
- `docs/domain-model.md`

These route readers to the correct documentation layer and should not duplicate detailed normative semantics.

### C. Current implementation reference

These describe what is actually wired in the inspected source snapshot:

- `docs/current-implementation.md`
- `docs/database-schema.md`
- `docs/security-and-rls.md`
- `docs/getting-started.md`
- `docs/acceptance/**`
- current schema artifacts where present

Source/migrations/tests remain authoritative for implementation reality.

### D. Gap-analysis / transition artifacts

These are now complete for the inspected baseline:

- `docs/gap-analysis-v0.2.md` — architecture-level gap analysis;
- `docs/requirement-coverage-v0.2.md` — requirement-family/current-code coverage matrix;
- `docs/implementation-plan-v0.2.md` — dependency, migration and PR sequence.

These are planning/analysis artifacts. They do not override the normative architecture and do not authorize code changes on this branch.

### E. Historical architecture/design artifacts

Older `docs/superpowers/specs/**`, legacy `docs/specs/r1-*.md`, v0.1 designs and earlier State Engine/Harness/federation/encryption designs are retained for history/rationale but are not normative where they conflict with v0.2.

### F. Old implementation plans

- `docs/plans/**`
- `docs/superpowers/plans/**`

These may refer to superseded architecture and must not be executed automatically after this gap analysis.

## 3. Conflict precedence

```text
Architecture Contract v0.2
    ↓
Accepted newer ADR
    ↓
Specialized normative v0.2 specification
    ↓
Normative Invariants Catalog / profile mapping
    ↓
Current implementation source/tests for implementation reality
    ↓
Current implementation documentation
    ↓
Gap analysis / implementation plan (transition guidance)
    ↓
Historical design artifacts
    ↓
Old implementation plans
```

ADR-0009 remains the explicit clarification that `SUPPRESSED` and `ACCEPTED_RISK` are operational disposition overlays, not HealthFinding integrity states.

## 4. Normative architecture completed

- [x] Canonical product definition fixed.
- [x] Twelve architecture blocks consolidated into one Architecture Contract.
- [x] Domain authority tiers and aggregate boundaries defined.
- [x] Stable normative requirement IDs created.
- [x] Capability/delegation/risk/approval/trust model documented.
- [x] Temporal/concurrency/as-of semantics documented.
- [x] Single execution boundary and external-effect semantics documented.
- [x] Findings/repair/incident/revalidation contract documented.
- [x] Crypto/classification/retention/deletion/export governance documented.
- [x] Qualification/conformance profiles and release gates documented.
- [x] v0.2 → v1.0 roadmap documented.
- [x] Key irreversible decisions captured as Accepted ADRs.
- [x] Legacy design artifacts explicitly classified.

## 5. Documentation QA completed

- [x] Normative links/indexes checked against files present on the branch.
- [x] Core terminology/casing normalized.
- [x] Finding lifecycle conflict resolved through ADR-0009.
- [x] Requirement families referenced by specs/ADRs exist in the invariant catalog.
- [x] Target specs explicitly separate specification from implementation availability.
- [x] Current implementation docs separate wired behavior from target guarantees.
- [x] Branch previously verified documentation-only against `main`.

## 6. Gap-analysis phase completed

- [x] Reconfirmed inspected `main` baseline: `729d456f70f4de93c97d05cce795c09025c62f24`.
- [x] Inspected Run, Memory, Event, Provenance, Retrieval, Security, Budget, Diagnostics, Tool, Enterprise, Artifact and evaluation foundations.
- [x] Corrected implementation snapshot: `doctor` and `rebuild` are currently wired.
- [x] Identified current ContextPack/retrieval semantic placeholders.
- [x] Classified all normative requirement families as Candidate/Partial/Type-only/Missing/Conflict/Verify without claiming conformance PASS.
- [x] Identified high-confidence schema/migration impact areas.
- [x] Defined dependency-ordered implementation sequence.
- [x] Defined first v0.2 `Correct` implementation tranche and later v0.3–v1.0 PR sequence.

## 7. Key transition conclusion

The current repository should **not be rewritten**.

Preserve:

```text
Run event sourcing
RLS
Memory revisions
idempotency / jobs / outbox
diagnostics / doctor
retrieval RRF shell
existing acceptance scenario ideas
```

Normalize before expansion:

```text
Claim/conflict semantics
Event/Memory temporal model
ContextPack exact-revision hydration
CapabilityGrant/delegation/risk
cross-workspace grant + mount
direct rebuild → RepairPlan protocol
ToolInvocation → ExternalEffect UNKNOWN/reconciliation
Run recovery → Incident/Trust/Revalidation
crypto seeds → DataPolicy/SecretRef/lifecycle
acceptance tests → requirement-driven conformance/qualification
```

## 8. Implementation start gate

No implementation work has been started by this phase.

Before code changes begin:

1. review `gap-analysis-v0.2.md`;
2. review `requirement-coverage-v0.2.md`;
3. review `implementation-plan-v0.2.md`;
4. if `main` moves materially, re-run the gap delta;
5. create a separate implementation branch/PR series;
6. use requirement IDs as PR acceptance criteria.

The documentation architecture remains frozen unless a deliberate ADR/spec amendment is required.
