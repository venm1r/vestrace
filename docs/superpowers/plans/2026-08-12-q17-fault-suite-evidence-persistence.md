# Q17 — Durable Fault-Suite Evidence Persistence

## Goal

Persist deterministic external-effect fault-suite results as immutable,
target-bound evidence that can be inspected after restart and consumed by a
later qualification runner.

## Tasks

- [x] Add RED SQLx tests for round-trip/idempotent insert, immutable conflict, and failed-evidence preservation.
- [x] Add `ExternalEffectFaultSuiteEvidence` snapshot with exact target identity, observations, decision and timestamp.
- [x] Add `FaultSuiteEvidenceRepository` application port.
- [x] Add migration `0129_external_effect_fault_suite_evidence.sql`.
- [x] Add `PgFaultSuiteEvidenceRepository` with indexed-payload integrity checks.
- [x] Document evidence semantics and explicit non-claims.
- [x] Production fault injection delivered by Q21/Q23; qualification-runner consumption by Q18 (`ExternalEffectFaultQualificationService`) and Q22 (bundle assembly).

## Safety boundary

Failed or unsafe evidence is persisted rather than converted into success. The
repository does not execute the fault suite, dispatch external effects, alter
trust state, or issue a qualification claim.

## Verification

- `cargo test --test effect_fault_evidence -- --nocapture`
- `cargo test -p vestrace-infrastructure --test external_effect_repository -- --nocapture` (Docker PostgreSQL)
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
