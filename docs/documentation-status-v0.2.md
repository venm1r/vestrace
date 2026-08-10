# Vestrace v0.2 Documentation Status

**Branch:** `docs/architecture-v0.2`  
**Baseline:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Date:** 2026-08-10

## 1. Purpose

This file classifies the documentation tree so target architecture, current implementation, historical design and implementation plans cannot be mistaken for each other.

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

These route readers to the correct documentation layer:

- `README.md`
- `docs/architecture.md`
- `docs/domain-model.md`

They must not duplicate detailed normative semantics that belong to specialized specs.

### C. Current implementation reference

These describe what is actually wired in the baseline source snapshot:

- `docs/current-implementation.md`
- `docs/database-schema.md`
- `docs/security-and-rls.md`
- `docs/getting-started.md`
- `docs/acceptance/**`
- generated/current schema artifacts where present

Current implementation documents must never imply that a target feature is available merely because it is specified or a type exists.

### D. Historical architecture/design artifacts

Documents created before the v0.2 normative baseline remain useful for rationale/history but do not override the new contract when they conflict.

This includes especially:

- older `docs/superpowers/specs/**` design/ADR artifacts;
- old State Engine/Harness horizon designs;
- the 2026-07-31 v0.1 design;
- earlier cross-workspace/encryption/export designs;
- legacy `docs/specs/r1-*.md` implementation-oriented specs.

A historical document may still be referenced for rationale, but its conflicting normative statements are superseded by the v0.2 Architecture Contract and Accepted ADRs.

### E. Implementation plans

The following are plans, not architecture truth:

- `docs/plans/**`
- `docs/superpowers/plans/**`

They may refer to older architecture and must not be executed automatically after the v0.2 documentation baseline without a new gap-analysis/implementation-plan phase.

## 3. Conflict precedence

When documents disagree, use this order:

```text
Architecture Contract v0.2
    ↓
Accepted newer ADR
    ↓
Specialized normative v0.2 specification
    ↓
Normative Invariants Catalog / profile mapping
    ↓
Current implementation documentation (describes reality, not target)
    ↓
Historical design artifacts
    ↓
Old implementation plans
```

Current implementation truth is special: source/migrations/tests remain authoritative for what is actually implemented, even when the target architecture requires different future behavior.

## 4. Documentation completed in this branch

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
- [x] README/architecture/domain entry points normalized.
- [x] Current implementation snapshot separated from target architecture.
- [x] Security/schema/getting-started marked as implementation references.
- [x] Documentation-only branch verified against `main` with no code/migration changes.

## 5. Remaining documentation-quality gates

Before declaring the documentation branch final, perform a final review for:

- [ ] broken relative links among new normative docs;
- [ ] inconsistent terminology/casing (`Memory`, `Claim`, `HealthFinding`, `UNKNOWN`, trust states, profiles);
- [ ] duplicate or contradictory MUST requirements across specialized specs;
- [ ] requirement IDs referenced but absent from the Invariants Catalog;
- [ ] roadmap/profile mapping contradictions;
- [ ] accidental implementation claims in target specs;
- [ ] accidental target guarantees in current-implementation docs;
- [ ] legacy `r1-*` specs being mistaken for v0.2 normative specs.

These are documentation QA tasks only and must not trigger code changes.

## 6. Documentation freeze criterion

Documentation phase is complete when all quality gates above pass and the branch diff remains documentation-only.

Only after that point may a separate phase produce:

1. gap analysis: current `main` vs v0.2 normative requirements;
2. migration/schema impact analysis;
3. implementation milestone/PR plan;
4. code changes in a separate implementation branch.

No implementation work starts merely because the target documents now exist.
