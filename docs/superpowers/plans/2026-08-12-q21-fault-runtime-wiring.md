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
- [x] Isolated process fault driver delivered by Q23 (`ProcessFaultInjectionRuntime`); attached to the release qualification path through Q22 and Q28.
- [x] Container fault driver delivered as `DockerFaultInjectionRuntime`, selected through `FaultInjectionDriver` (R3).
- [ ] A provider-specific sandbox driver behind `FaultInjectionEnvironment::ProviderSandbox` remains open; see [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Verification

- `cargo test --test effect_fault_runtime -- --nocapture`
- `cargo test --test effect_fault_gate_evidence -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
