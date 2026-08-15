# Q10 — Post-Incident Requalification and Recovery Gate

## Goal

Make `POST_INCIDENT` qualification fail closed unless it carries typed revalidation evidence, and make recovery action expectations deterministic and testable.

## Tasks

- [x] Add typed post-incident evidence with incident/revalidation identifiers and proof references.
- [x] Bind evidence to `QualificationBundle` only for `POST_INCIDENT` lifecycle.
- [x] Require the evidence file in the CLI and preserve failed artifacts before non-zero exit.
- [x] Add deterministic recovery-target observations and action evaluation.
- [x] Add focused domain and CLI tests.
- [x] Durable incident/revalidation repositories delivered by Q11 (`PgRecoveryRepository`) and isolated fault execution by Q23 (`ProcessFaultInjectionRuntime`). Runtime orchestration exists as the Q12 `StartupRecoveryService` contract only; candidate discovery and startup wiring remain open as R1 in [`2026-08-12-open-follow-up-gates.md`](2026-08-12-open-follow-up-gates.md).

## Verification

- `cargo test --test q10_post_incident_requalification -- --nocapture`
- `cargo test -p vestrace-cli --test q10_post_incident_cli -- --nocapture`
- scoped `cargo fmt` and `git diff --check`
