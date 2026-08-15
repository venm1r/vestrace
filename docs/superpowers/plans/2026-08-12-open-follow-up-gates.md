# Open Follow-Up Gates — Q-Series Residual Scope

**Date:** 2026-08-12
**Status:** R1–R3 closed; this file now records what was built and what remains

The Q10–Q24 plans each deferred scope "to a later gate". Most of those deferrals
were delivered by a subsequent Q gate; the individual plans record which one.
The three residual items tracked here — startup recovery wiring, provider
read-back, and container-isolated qualification execution — are now implemented.

## R1 — Startup recovery: candidate discovery and process wiring — CLOSED

Two gaps were open, and both are closed:

1. **Durable run-candidate discovery.** `StartupRecoveryCandidateSource`
   (`crates/vestrace-application/src/runs/recovery.rs`) is the port;
   `PgStartupRecoverySource`
   (`crates/vestrace-infrastructure/src/postgres/startup_recovery_source.rs`) is
   the PostgreSQL implementation. Classification comes from lease state only,
   never inferred from the run payload:
   - non-terminal run with an expired lease → `RecoveryTarget::StaleLease`
     (safe to retry);
   - non-terminal run that was executing with no lease row →
     `RecoveryTarget::UnknownOutcome` (must reconcile — such a run cannot be
     proven to have stopped cleanly, so it is never silently resumed);
   - run holding a live lease → not a candidate; it belongs to another worker.

   `StartupRecoveryService::run_discovered` is fail-closed: an unconfigured
   source or a failing query is an error, never an empty recovery run.

2. **Process wiring, with an explicit scope.** `RecoveryConfig`
   (`crates/vestrace-infrastructure/src/config.rs`) adds `recovery.enabled` and
   an explicit `recovery.workspaces` list. Both `server.rs` and `worker.rs` run
   the sweep before serving work, and a failing sweep aborts startup. Enabling
   recovery without naming a workspace is rejected at config load, so the sweep
   cannot silently recover nothing.

**Chosen approach:** the explicitly configured workspace list. Startup recovery
stays workspace-scoped exactly like every other query; no cross-workspace sweep,
no system context, and no RLS bypass was introduced.

**Known adjacent defect, not fixed here:** `worker.rs` still builds its run-worker
polling context as `RequestContext::new(WorkspaceId::new(), PrincipalId::new())`
— a freshly generated random workspace. Startup recovery no longer depends on
that placeholder, but run work-item polling in a deployed worker still addresses
a workspace that cannot contain any rows. Fixing it means deciding which
workspaces a worker serves, which is a separate scoping decision from recovery.

## R2 — Provider read-back adapter — CLOSED

`HttpExternalEffectReadBackAdapter`
(`crates/vestrace-infrastructure/src/http_read_back.rs`) implements the
`ExternalEffectReadBackAdapter` port over HTTP. It issues `GET` only — it is the
observation half of reconciliation, never a second dispatch path — and it is
fail-closed in the direction that matters: an unreachable or erroring provider
is `Unavailable` evidence, never evidence that the effect did not happen. A
response with no evidence references or a blank state reference is rejected
rather than recorded as weak evidence, and an indeterminate `effect_applied`
stays indeterminate.

`ObservedEffectState` gained read-only accessors so observations can be inspected
outside the domain crate.

**Still open:** `FaultInjectionEnvironment::ProviderSandbox` remains a declarable
value with no provider-specific sandbox behind it. HTTP read-back covers the
reconciliation path; it does not make a named provider sandbox executable.

## R3 — Container-backed qualification execution — CLOSED

`DockerFaultInjectionRuntime` (`crates/vestrace-application/src/fault_runtime.rs`)
runs a fault scenario inside a disposable, network-isolated container
(`docker run --rm --network none`), forwarding the fault contract through
environment variables. `with_container_command` lets a deployment select
`podman` or an absolute binary path.

Selection is configuration, not inference: `FaultInjectionDriver`
(`Process` | `Container`) is carried by `FaultInjectionSettings`, defaults to
`Process`, and the container runtime refuses to run under settings that select
the process driver. Both runtimes share one execution and observation-parsing
path, so their contracts cannot drift apart.

## Defects found and fixed while closing R1–R3

Running the database-backed suites against a real PostgreSQL exposed six defects
that had been masked by an unset `DATABASE_URL`. All are fixed.

1. **Checkpoints could never be written.** Migration 0112 reshaped
   `run_checkpoints` (renaming `run_version` to `sequence`) but left
   `resume_cursor` NOT NULL with no default, still constrained
   `resume_cursor = sequence` and never written by the current code. Every
   checkpoint insert failed with 23502, so checkpoint-based recovery could not
   work at all. Migration `0131_drop_vestigial_run_checkpoint_columns.sql` drops
   `resume_cursor`, `active_plan_revision_id`, `payload` and `payload_version`,
   which 0112 left behind and which hold no data.
2. **The recovery repository rejected its own records.** `find_incident` and its
   three siblings compared a payload timestamp against its indexed
   `TIMESTAMPTZ` column at nanosecond precision. `now()` is `Utc::now()`, which
   carries nanoseconds; the column stores microseconds. Any record written with
   a real clock reading failed read-back with "indexed metadata does not match
   payload" — the Q11 durable repository could not read anything back.
   Comparison now happens at the column's precision.
3. **Rebuilt projections were incomplete.** `PgRunRecoveryStore::replace_projection`
   omitted `objective` (NOT NULL), `root_run_id`, `coordinator_snapshot_id`,
   `execution_mode` and `finished_at`, so rebuilding a projection either failed
   outright or produced a row the reader could not decode. It now writes the same
   columns as the command committer.
4. **Terminal runs could not be committed.** `PgRunCommandCommitter` never wrote
   `finished_at`, violating `chk_agent_runs_terminal_has_finished_at` whenever a
   command moved a run into a terminal state.
5. **The read path discarded `finished_at`.** `PgRunRepository` hardcoded
   `finished_at: None` and did not select the column, so a replayed projection
   could never equal the stored one.
6. **Stale test fixtures.** Several `agent_runs` and `run_events` fixtures
   predated later NOT NULL columns (`objective`, `run_version`,
   `sequence_value`, `recorded_at`), and one migrations assertion depended on the
   server's collation order for `ORDER BY column_name`.

## Adjacent fixes

- **Worker workspace scope.** The run worker polled
  `RequestContext::new(WorkspaceId::new(), …)` — a freshly generated random
  workspace — so it could never claim run work. It now polls each configured
  workspace, logs a warning when none is configured, and a failure in one
  workspace no longer stalls the others. The workspace list was promoted from
  `recovery.workspaces` to a top-level `workspaces` setting so recovery and run
  polling cannot drift onto different sets.
- **`ProviderSandbox` can no longer be claimed falsely.** It named an isolation
  guarantee that no driver provides, yet a deployment could declare it and then
  execute in a host process. Both runtimes now refuse it. A provider sandbox
  driver remains unimplemented — but it can no longer be misrepresented.

## Verification

- `cargo test --test startup_recovery` — 7 passed;
- `cargo test -p vestrace-infrastructure --test startup_recovery_schema_contract` — 5 passed;
- `cargo test -p vestrace-infrastructure --test startup_recovery_source` — 2 passed
  against a disposable PostgreSQL 17 (pgvector image) with the full migration set;
- `cargo test -p vestrace-infrastructure --test http_read_back` — 6 passed;
- `cargo test -p vestrace-infrastructure --lib config` — 5 passed;
- `cargo test -p vestrace-cli --test command_contract` — 9 passed;
- `cargo test --test effect_fault_runtime` — 14 passed;
- **`cargo test --workspace --no-fail-fast` against a live PostgreSQL 17
  (pgvector image) with the full migration set — 674 passed, 0 failed**, which
  is the first fully green database-backed run of this workspace;
- `cargo fmt --all -- --check`, `cargo test --workspace --no-run` and
  `git diff --check` — clean.

The verification database was a disposable container (`--rm`, no volume) on a
non-default port; no existing PostgreSQL volume was stopped or deleted.

## Non-claims

These are implemented and test-covered boundaries, not a deployment
qualification. No live provider, container image, or deployed environment has
been qualified.

One item stays open by design: there is still no provider sandbox fault driver.
The difference from before is that `ProviderSandbox` can no longer be declared
and then quietly executed somewhere else — the claim is now refused rather than
approximated. Implementing it needs a named provider.

The green database run proves the schema and repository paths agree; it does not
prove any deployment. The `recovery.enabled` sweep and the worker's workspace
polling were verified by contract and unit tests, not by observing a deployed
server or worker process recover real interrupted work.
