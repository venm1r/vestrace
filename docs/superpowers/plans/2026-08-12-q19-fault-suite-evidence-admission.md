# Q19 — Fault-Suite Evidence Admission

## Goal

Add a fail-closed read-side boundary that admits only durable fault-suite
evidence for the exact requested deployment target and only when the suite
passed.

## Tasks

- [x] Add RED tests for exact-target pass admission, failed evidence, target mismatch, missing evidence, and repository failure.
- [x] Add `ExternalEffectFaultEvidenceAdmissionService`.
- [x] Preserve repository failures instead of converting unavailable evidence into a pass.
- [x] Document the admission boundary and explicit non-claims.
- [ ] Attach admitted evidence to the complete qualification runner and release gate later.

## Verification

- `cargo test --test effect_fault_admission -- --nocapture`
- `cargo test --test effect_fault_qualification -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
