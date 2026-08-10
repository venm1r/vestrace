# Vestrace v0.2 Documentation Status

**Branch:** `docs/architecture-v0.2`  
**Baseline:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Date:** 2026-08-10  
**Documentation state:** **FROZEN BASELINE — ready for review/gap analysis, not implementation**

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

- older `docs/superpowers/specs/**` design/ADR artifacts (directory now contains an explicit historical-archive README);
- old State Engine/Harness horizon designs;
- the 2026-07-31 v0.1 design;
- earlier cross-workspace/encryption/export designs;
- legacy `docs/specs/r1-*.md` implementation-oriented specs (explicitly classified by `docs/specs/LEGACY.md`).

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

ADR-0009 is an intentional clarification within the normative baseline: `SUPPRESSED` and `ACCEPTED_RISK` are operational disposition overlays, not `HealthFinding` integrity lifecycle states. It takes precedence over the older lifecycle listing in the Health contract.

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
- [x] Legacy `r1-*` and `docs/superpowers/specs/**` design artifacts explicitly marked historical.
- [x] Documentation-only branch verified against `main` with no code/migration changes.

## 5. Documentation-quality gates

Final documentation QA completed:

- [x] Relative links used by the new normative indexes/entry points resolve to files/directories present on `docs/architecture-v0.2`.
- [x] Core terminology/casing is normalized: `Memory`, `Claim`, `HealthFinding`, `UNKNOWN`; health states; trust states; repairability/reversibility enums; conformance profile names.
- [x] The identified finding-lifecycle contradiction was resolved by Accepted ADR-0009 with explicit precedence in the normative index.
- [x] Requirement-family/range references used by the new ADRs and specialized specs map to families present in the Normative Invariants Catalog (`ARC`, `MEM`, `TMP`, `MUT`, `RET`, `LRN`, `CAP`, `IDW`, `HLT`, `EXT`, `REC`, `GOV`, `QUAL`).
- [x] Roadmap milestones and conformance profiles are intentionally separated: release names (`Correct/Learn/Govern/Understand/Connect/Trust`) are not invented runtime profile names; qualification profiles remain `CORE/MEMORY/COGNITION/AUTONOMY/FEDERATION/TRUSTED`.
- [x] Target specs contain explicit disclaimers that specification does not imply current implementation availability.
- [x] Current-implementation docs explicitly distinguish wired behavior from target guarantees.
- [x] Legacy `r1-*` specs and older `docs/superpowers/specs/**` are explicitly classified as historical rather than normative v0.2.

## 6. Documentation freeze criterion

The v0.2 architecture documentation baseline is now considered **frozen for review** because the quality gates above pass and the branch remains documentation-only.

“Frozen” means architectural changes should now be made deliberately through a new/updated normative specification or ADR rather than by silently editing assumptions during implementation planning.

It does **not** authorize implementation work in this branch.

## 7. Next allowed phase

The next phase may produce documentation/analysis only:

1. gap analysis: current `main` vs v0.2 normative requirements;
2. requirement-to-current-implementation coverage matrix;
3. migration/schema impact analysis;
4. implementation milestone/PR plan.

Only after those artifacts are reviewed should code changes begin, and they must occur in a separate implementation branch.
