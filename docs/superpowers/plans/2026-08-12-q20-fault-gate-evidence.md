# Q20 — Fault-Gate Evidence

## Goal

Expose target-bound durable fault-suite results as local executable `QUAL-008`
hard-gate evidence while preserving failed results and fail-closed read errors.

## Tasks

- [x] Add RED tests for passed mapping, failed mapping, target mismatch, and missing evidence.
- [x] Add `ExternalEffectFaultGateEvidenceService`.
- [x] Map passed/failed evidence to `QUAL-008` `Pass`/`Fail` without promotion.
- [x] Preserve target and repository failure boundaries.
- [x] Document the adapter boundary and explicit non-claims.
- [x] Assembled into a qualification bundle by Q22 and into the release gate by Q25 (release approval) and Q28 (v1.0 release evidence).

## Verification

- `cargo test --test effect_fault_gate_evidence -- --nocapture`
- `cargo test --test effect_fault_admission -- --nocapture`
- `cargo test --test effect_fault_qualification -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
