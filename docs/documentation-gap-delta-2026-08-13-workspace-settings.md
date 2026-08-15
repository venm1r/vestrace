# Documentation Gap Delta — Workspace Settings

**Date:** 2026-08-13
**Scope:** operator-changeable workspace settings, end to end
**Repository state:** dirty, implementation changes uncommitted

## Implemented bounded contract

A complete vertical slice: domain → migration → repository → HTTP → console.

- `WorkspaceSettings` (`crates/vestrace-domain/src/settings.rs`) holds
  `max_concurrent_runs`, `run_budget_cap_micros` (zero means no cap),
  `log_level` and a positive `version`. Validation runs before any field is
  replaced, so a rejected change leaves no partial update.
- Migration `0132_workspace_settings.sql` stores them per workspace with
  `FORCE ROW LEVEL SECURITY` and CHECK constraints mirroring the domain rules,
  so a direct writer cannot leave a value the domain would reject on read.
- `PgWorkspaceSettingsRepository` reads and writes inside the caller's scoped
  transaction. An unconfigured workspace reads as the conservative defaults, so
  callers never distinguish "absent" from "unset".
- `GET`/`PUT /v1/settings` map to `Capability::WorkspaceAdmin`. Updates are
  compare-and-set through `If-Match`; a stale revision is a `409` and writes
  nothing.
- The console settings panel loads, edits and saves against the API, shows the
  saved revision, and re-reads on conflict so the operator edits against the
  current revision instead of retrying blindly.

## A deliberate design choice

The previous panel presented six controls. Three could not be made honest:

- **RLS enforced** — enforced by PostgreSQL policies; an application toggle
  would change nothing;
- **Database pool size** — process configuration applied at startup;
- **MFA enforced** — this build has no authentication at all.

Storing those as settings would give an operator controls that do nothing. They
are now shown under an **Environment** tab as reported values with an explicit
note that they are not configurable there. Only values the runtime genuinely
consults are stored and editable.

## Evidence

- `tests/workspace_settings.rs` — 7 domain tests: conservative defaults, bounds
  on concurrency, positive version, zero-means-unlimited budget, version bump on
  change, no partial update on invalid input, log-level round trip.
- `crates/vestrace-infrastructure/tests/settings_repository.rs` — 5 tests
  against PostgreSQL 17 with the full migration set: defaults for an
  unconfigured workspace, round trip with version advance, stale version
  rejected without writing, schema refuses domain-invalid values, no cross-
  workspace leakage.
- `crates/vestrace-http/tests/router_contract.rs` — settings routes map to
  `WorkspaceAdmin`. A route with no capability mapping bypasses the
  authorization middleware entirely, so this is a security assertion, not a
  cosmetic one.
- Verified in the running stack: `GET` returned defaults at revision 1, `PUT`
  with `If-Match: 1` advanced to revision 2, a repeated stale `If-Match: 1`
  returned `409 conflict`. In the browser the panel loaded revision 2, an edit
  saved as revision 3, and the database holds `15 | 12500000 | debug | 3`.

## Explicit non-claims

Settings are stored and served; **no runtime component consults them yet**.
`max_concurrent_runs` does not currently limit the worker, and
`run_budget_cap_micros` is not enforced by any budget check. This slice makes
the values durable, governed and editable; wiring them into execution is
separate work.

The remaining 501 surfaces are untouched: run approval, artifacts, triggers,
connections, audit, metrics, system health, profile and AG-UI.

## Note for a future metrics API

The console's declared `MetricsSummary` contract has seven fields. Only
`live_runs`, `active_agents` and `total_runs_today` are computable from stored
data. There is no latency measurement and no budget accounting, so
`avg_latency` and `budget_spent` have no honest source; `budget_limit` gained
one with this slice. Implementing that endpoint should narrow the contract to
what is observable rather than fill the gaps with plausible numbers.
