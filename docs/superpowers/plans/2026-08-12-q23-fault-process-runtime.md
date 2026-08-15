# Q23 — Isolated Fault-Process Runtime

## Goal

Replace the in-process-only fault runtime boundary with a controlled child
process adapter that can execute deterministic fault scenarios without
inheriting the parent process environment.

## Tasks

- [x] Add RED tests for child-process forwarding, non-zero exit, timeout,
  malformed JSON, and invalid process configuration.
- [x] Add `ProcessFaultInjectionRuntime` with explicit command, arguments,
  timeout, target, and isolation settings.
- [x] Parse strict structured observations and fail closed on identity or
  lifecycle mismatches.
- [x] Ensure timed-out/dropped child processes are killed.
- [x] Document the runtime boundary and explicit non-claims.
- [x] Docker-backed qualification execution delivered as
  `DockerFaultInjectionRuntime`, running `--rm --network none` and selected
  through `FaultInjectionDriver::Container` (R3).
- [ ] Provider sandbox adapters remain open:
  `FaultInjectionEnvironment::ProviderSandbox` is declarable but still has no
  adapter behind it. See
  [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Verification

- `cargo test --test effect_fault_runtime -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
