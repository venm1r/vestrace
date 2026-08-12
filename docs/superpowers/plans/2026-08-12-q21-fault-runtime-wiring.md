# Q21 — Fault-Runtime Wiring

## Goal

Provide an explicit, target-bound non-production runtime hook for the existing
deterministic fault-suite executor.

## Tasks

- [x] Add RED tests for disabled execution, exact target/point forwarding, runtime failure, and point mismatch.
- [x] Add `FaultInjectionSettings` with explicit isolation environment.
- [x] Add `FaultInjectionRuntime` and configured executor adapter.
- [x] Keep disabled injection fail-closed and validate returned point identity.
- [x] Document the wiring boundary and explicit non-claims.
- [ ] Implement an isolated process/provider fault driver and attach it to a complete release qualification runner later.

## Verification

- `cargo test --test effect_fault_runtime -- --nocapture`
- `cargo test --test effect_fault_gate_evidence -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
