# Q15 — Deterministic Effect Fault Suite

## Goal

Provide a deterministic application harness boundary for effect fault points without hiding unsafe retry or UNKNOWN outcomes.

## Tasks

- [x] Add RED tests for required-point execution, unsafe retry evidence, executor failure, and UNKNOWN dispatch semantics.
- [x] Add injected `EffectFaultScenarioExecutor` port.
- [x] Add `ExternalEffectFaultSuiteService` and typed report.
- [x] Preserve domain fault-suite decision semantics and fail closed on mismatches/errors.
- [x] Document the harness boundary and explicit non-claims.
- [x] Production fault injection delivered by Q21 (`FaultInjectionRuntime`) and Q23 (`ProcessFaultInjectionRuntime`); qualification evidence persisted by Q17 (`PgFaultSuiteEvidenceRepository`, migration `0129`).

## Verification

- `cargo test --test effect_fault_suite -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
