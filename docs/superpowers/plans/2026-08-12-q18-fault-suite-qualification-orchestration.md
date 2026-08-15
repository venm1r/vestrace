# Q18 — Fault-Suite Qualification Orchestration

## Goal

Compose the deterministic effect fault-suite executor with durable evidence
persistence so one application operation produces an auditable target-bound
artifact.

## Tasks

- [x] Add RED tests for passed evidence persistence, failed evidence preservation, and executor failure ordering.
- [x] Add `ExternalEffectFaultQualificationService`.
- [x] Run the existing fault suite before constructing or persisting evidence.
- [x] Preserve failed decisions as stored failed evidence rather than returning a false success.
- [x] Keep executor failures fail-closed and avoid partial evidence insertion.
- [x] Document the orchestration boundary and explicit non-claims.
- [x] Production fault injector delivered by Q21/Q23; the artifact is consumed by Q19 admission, Q20 gate evidence, Q22 bundle assembly, and the Q28 v1.0 release-evidence composition.

## Verification

- `cargo test --test effect_fault_qualification -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
