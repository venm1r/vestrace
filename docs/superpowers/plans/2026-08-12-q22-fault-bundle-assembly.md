# Q22 — Fault Evidence in Qualification Bundles

## Goal

Assemble and persist a manifest-bound `QualificationBundle` that includes the
exact-target external-effect fault evidence as `QUAL-008`.

## Tasks

- [x] Add focused tests for passed and failed fault evidence, target mismatch,
  repository failure, and duplicate `QUAL-008` input.
- [x] Load fault evidence through `ExternalEffectFaultGateEvidenceService`.
- [x] Map the fault result into hard-gate evidence and a conformance case.
- [x] Persist both passed and failed bundles through `QualificationRepository`.
- [x] Document the implemented boundary and explicit non-claims.
- [ ] Add a real isolated process/provider fault driver in a later block.

## Verification

- `cargo test --test effect_fault_bundle -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
