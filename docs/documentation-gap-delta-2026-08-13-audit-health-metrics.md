# Documentation Gap Delta — Audit, System Health and Metrics APIs

**Date:** 2026-08-13
**Scope:** three of the eight surfaces that previously answered 501
**Repository state:** dirty, implementation changes uncommitted

## Implemented

### `GET /v1/audit`

Reads `audit_events` for the caller's workspace, newest first, capped at 200
rows. Mapped to `Capability::AuditRead`.

Reading alone would have been a facade: the `AuditRepository::record` port had
existed since migration 0013 and **no code ever called it**, so the table was
empty. This slice therefore also produces events — a settings change records
`workspace_settings.updated` with the previous and new revision and every
changed value. A failure to record fails the request rather than being
swallowed, because a trail that silently drops entries is worse than no trail.

### `GET /v1/system/health`

Returns the `DoctorService` findings plus properties of the environment the
process is running in: the database role, whether it is a superuser, whether it
bypasses row-level security, and whether the migration history is compatible.

Added the `RuntimeEvidenceProvider` port for this. Values are queried at request
time rather than stored, so they cannot drift from reality the way a copied
configuration value would.

### `GET /v1/metrics/summary`

Returns `live_runs`, `runs_today`, `registered_agents`, `registered_models` and
`run_budget_cap_micros`, through the new `WorkspaceCountsProvider` port.

**The contract was deliberately narrowed.** The previous client shape declared
seven fields including `avg_latency` and `budget_spent`. Nothing in this system
measures latency or accounts spend, so those fields had no honest source and
were removed rather than filled with plausible numbers. `budget_limit` was
replaced by the real per-run ceiling from workspace settings. This is a breaking
change for the console, and the home page cards were rewritten accordingly.

## Console

The Audit page renders real events. The settings **Environment** tab now shows
observed values from `/v1/system/health` instead of fixed captions, with two of
them phrased as expectations — a role that bypasses RLS could read across every
workspace, so "must be no" is stated rather than implied.

## Evidence

Verified against the running stack:

- a settings change produced an audit event visible both in the API response and
  in `audit_events` (`count = 1`), and the Audit page renders it;
- `system/health` reported `database_role=vestrace`, superuser `false`,
  bypasses RLS `false`, migrations compatible, no findings;
- `metrics/summary` reported `live_runs=2, runs_today=2,
  run_budget_cap_micros=4000000`, matching the two runs created earlier and the
  configured ceiling.

## Not implemented, and why

Five surfaces still answer 501: `run approval`, `artifacts`, `triggers`,
`connections`, `profile`. Each needs a subsystem that does not exist —
artifact storage, a scheduler, a credential broker, an API-key model, a run
approval decision.

**Run approval specifically.** The existing `ApprovalRecord` domain covers
governance approvals (hard purge, permission change, export) and does not model
approving a waiting run. In the run domain a run leaves `WaitingForApproval`
through the same `Resumed` event as an ordinary un-pause, and no approver
identity is captured anywhere. Implementing `/approve` on top of `Resume` would
make approval indistinguishable from resumption in the canonical event history
and would lose the approver — the very fact the approval exists to record. Doing
it faithfully requires a new run event carrying the approver, which is a change
to the canonical event schema and its compatibility tests.

A related consequence: the development compose file sets the policy risk ceiling
to `high` precisely so `/approve` (Critical) stays denied. That ceiling should
only be raised once approval is genuinely implemented.
