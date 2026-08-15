# Documentation Gap Delta — Run Model Unification

**Date:** 2026-08-13
**Scope:** moving the HTTP run surface onto the durable `RunCoordinator`
**Decision:** option B, taken by the operator
**Repository state:** dirty, implementation changes uncommitted

## The problem

Two run mechanisms existed and did not meet.

| | `/v1/runs` (before) | worker |
|---|---|---|
| path | `RunCommandService` → `PgRunCommandCommitter` | `RunCoordinator` → `PostgresRunStore` |
| writes | `agent_runs`, `run_events`, `run_streams` | `agent_runs`, `run_events`, `run_steps`, `run_work_items` |
| step model | `RunCommand::StartStep { step_id, kind, label }` | `NewRunStepDto { assigned_actor, input_references }` |

No route created a step and nothing called `AddRunSteps`, so a run created
through the API never acquired one, no work item was queued, and the worker
never saw it. After four API-created runs the live database held:

```text
agent_runs     | 4
run_events     | 4
run_steps      | 0
run_work_items | 0
run_leases     | 0
```

Both paths wrote `agent_runs` and `run_events` with independent optimistic
version counters, so they could not simply be joined: two writers would produce
version conflicts and interleaved sequences in the canonical log.

## What changed

The coordinator is now the authoritative run write path, and the HTTP surface
issues its commands.

- `AppState` gained `run_orchestrator`. `RunCoordinator` is generic over its
  four ports and cannot be held directly, so `RunOrchestrator` is an
  object-safe trait carrying **only** the commands a caller may legitimately
  issue. Step transitions and checkpoint creation are deliberately absent — they
  belong to the worker, and exposing them would let a route drive a run's
  internals by hand.
- `create`, `pause`, `resume`, `cancel` and `approve` now go through the
  coordinator. So does AG-UI's `run`, which had been issuing the legacy command.
- **`POST /v1/runs/{id}/steps` is new.** It is what makes a run executable.
  `assigned_actor` accepts `agent`, `principal` or `system`; only `agent`
  invokes a model. An unknown value is refused rather than defaulted, because
  silently turning a typo into a system step produces a run that completes
  having done nothing.
- The request id is used as the idempotency key on creation. It is required
  rather than generated: a key the server invents deduplicates nothing.
- API-created runs are `Supervised`, not `Autopilot`. Nothing enforces a budget
  on the execution path in this build, and defaulting to unattended execution
  would claim more than the system can back.

## Approval attribution was preserved deliberately

`ApprovalGranted` existed only on `LegacyRunEvent`. Moving to the coordinator
without carrying it across would have silently regressed approval to an ordinary
status change — the exact collapse that earlier work had refused.

`RunEventPayload::ApprovalGranted { approver_id }` was therefore added to the
canonical event model, emitting the same `run.approval_granted` type string so a
reader of the log need not know which writer produced a historical row. The
status change is emitted as well, so replay derives state without interpreting
the approval event; replay accepts it and applies nothing.

`ApproveRun` carries `approver_id` separately from `actor`. They are the same
principal today, but conflating them in the type would make an approval granted
on someone's behalf impossible to represent without misattributing it.

## Three defects found and fixed on the way

1. **`ResumeRunHandler` was unreachable.** `RunCoordinator::resume_run` built a
   `ResumeRun` work item and threw it away — `let _ = resume_item;`. Nothing
   ever enqueued that kind, so the handler registered in the worker could never
   run. The item is now committed with the transition rather than written
   separately, so a crash between the two cannot leave a resumed run with
   nothing scheduled.

   The `AdvanceRun` item the transition already queued is not a substitute:
   resuming re-establishes the run's position from its checkpoint, and advancing
   without that steps the run forward from stale state.

2. **The worker installed no tracing subscriber.** `init_tracing` was private to
   `server.rs`. Every `tracing::info!` and `tracing::warn!` the worker emitted
   was discarded, including the warning saying it served no workspaces, so a
   misconfigured worker looked identical to a healthy idle one.

3. **A correlation regression introduced by this very change**, caught by a
   dead-code warning on `CORRELATION_ID_HEADER`. The legacy envelope carried the
   caller's `x-correlation-id` into every run event; the coordinator minted its
   own with `CorrelationId::new()`. Moving the surface across would therefore
   have silently severed the link between an HTTP request and the events it
   caused — the failure would not have surfaced as an error, only as traces that
   no longer joined up.

   The commands now carry `correlation_id: Option<CorrelationId>`. `None` means
   the caller supplied none, and the coordinator mints one rather than leaving
   the event uncorrelated. The approval event deliberately reuses the
   correlation of the transition it accompanies, so both halves trace to one
   request.

## The durable store had never worked against the current schema

Moving the HTTP surface onto the coordinator exposed something the split had
been hiding: `PostgresRunStore` — the adapter the worker depends on — could not
write to this database at all. The first live request returned
`storage_failure`, and every green test run had missed it because the
coordinator's tests use an in-memory store rather than PostgreSQL.

Six drifts, in the order they surfaced:

1. **`agent_runs` insert omitted `principal_id` and `title`**, both NOT NULL.
   The durable `AgentRun` carries no principal at all, so it now comes from the
   request context — the run belongs to whoever asked for it. `title` takes the
   objective, which is what the read path already reports as the title.
2. **`run_events` insert omitted `event_version` and `causation_id`**, both NOT
   NULL, and named a `causation_event_id` column that does not exist. An
   uncaused event now points its causation at itself, which says "the chain
   starts here" rather than inventing a link. `event_version` is stamped with
   the same value the legacy writer used, so a reader cannot tell the two
   writers apart by that column.
3. **`run_steps` insert omitted `step_number`**, NOT NULL, no default, unique
   per run. It is derived inside the statement rather than in Rust so two
   inserts in one transaction cannot choose the same number.
4. **`run_checkpoints` was addressed by its pre-0112 shape entirely.** Migration
   0112 renamed `run_version` to `sequence` and `state_snapshot` to `state`,
   dropped `id`, and added a mandatory `state_hash`; migration 0131 dropped
   `resume_cursor`, `payload` and `payload_version`. Both the insert and the
   select named columns that no longer exist — and the select runs on **every**
   load, so nothing at all could be read back.

   The domain type still carries a checkpoint id, so it is now derived from
   `(run_id, sequence)`, the checkpoint's actual identity after 0112. A random
   id would make two reads of the same checkpoint compare unequal, which replay
   and recovery both depend on.

A fifth drift sits underneath those four. Migration 0112 states that
`run_streams` is authoritative and that `agent_runs` and `run_checkpoints` are
derived from it; `run_checkpoints` carries a foreign key onto it, and
`PgRunRecoveryStore` reads a run's version **from the stream, not from
`agent_runs`**.

A trigger, `agent_runs_seed_run_stream`, seeds the stream at version 0 when a
projection row is inserted, and the writer is expected to advance it as events
are appended — which is what the legacy committer's `advance_stream` did. This
store never advanced it. The stream would therefore have stayed at 0 for the
life of every run while `agent_runs` climbed, and recovery would have acted on
a version of 0.

The first attempt at this used `ON CONFLICT DO NOTHING`, which silently did
nothing at all because the trigger had already inserted the row — a test
asserting the stream opens at the initial version is what caught it. Both
`create` and `commit` now advance the stream to the version of the event being
appended, in the same transaction, and a test asserts the authoritative stream
and the derived projection do not diverge.

### The store never participated in row-level security


The most fundamental drift, and the one the database-backed tests could not
catch. `PostgresRunStore` held a bare `PgPool` and opened plain transactions,
so `vestrace.workspace_id` was never set. Every table it touches is under
`FORCE ROW LEVEL SECURITY` with policies of the form
`workspace_id = vestrace_current_workspace_id()`, which under the runtime role
match nothing when that setting is absent — so every read and write was refused.

It failed closed rather than leaking, which is the right direction, but it means
the adapter could not function under the deployment's own security model. The
legacy committer used `PgStore::begin_scoped`; this one did not.

The store now holds the `PgStore` and routes every statement through
`begin_scoped`, which is what sets the workspace for the transaction.

**Why the tests missed it, stated plainly:** `sqlx::test` connects as the
database owner, and row-level security does not apply to a superuser. The six
tests in `run_store.rs` therefore prove the statements match the schema — which
is what they were written for — but they do **not** exercise RLS. Only the
deployed stack, running as the `vestrace` runtime role, does. That gap is why
five schema drifts and this policy gap all survived a fully green suite.

`crates/vestrace-infrastructure/tests/run_store.rs` covers this adapter against
a real PostgreSQL for the first time: a run can be created, it is attributed to
the requesting principal, creation enqueues the work that advances it, the
creation event preserves its correlation, and advancing a run advances the
authoritative stream in step with the projection.

### Three more defects, in the worker

Once the store could write, the run still did not move. Three further defects,
all pre-existing and all unreachable while no run reached the worker:

7. **The main loop cancelled its own work.** `job_worker.process_one()` and
   `poll_run_work(...)` were branches of a `tokio::select!`, which cancels the
   losing branch. An empty job table returns immediately, so the job branch won
   almost every race and the run-work future was dropped at whatever await point
   it had reached. An item leased a moment earlier was abandoned: still `leased`,
   with no run lease, no error and nothing in the log. The two pollers now run in
   sequence; only shutdown is raced, where cancelling is what is wanted.

8. **An expired lease was never reclaimed.** `lease_next` selected only
   `status = 'ready'`, so an item left `leased` by a stopped worker was stranded
   permanently — which makes the lease expiry meaningless. Expired leases are now
   reclaimable.

9. **The worker hardcoded `DenyAllPolicyEngine`.** `policy.engine` was never
   applied to it, so every work item it leased was refused with
   `authorization_denied: DefaultDeny` and dead-lettered. It now builds the
   engine from configuration exactly as the server does, and the development
   compose grants it `execution.write` — the capability
   `work_item_authorization_request` asks for.

10. **`insert_step` was a plain insert.** `CommitRun::new_steps` carries steps to
    *persist*, which includes existing ones with a changed status —
    `ExecuteStepHandler` puts the running step there and then the finished one.
    The first execution of any step therefore failed on a duplicate key, and no
    run could progress past its first step. It is now an upsert that updates
    progress while leaving `step_number`, `run_id` and `created_at` alone.

## Live evidence

Against the running stack, a run created through `POST /v1/runs` now reaches the
worker and executes:

```text
run_version | event_type              | status
          1 | run.created             | created
          2 | run.status_changed      | (worker: created -> running)
          3 | run.status_changed      |
          4 | run.steps_added         | (POST /v1/runs/{id}/steps, actor=agent)
          5 | run.step_status_changed | step -> running
          6 | run.step_status_changed | step -> failed
          7 | run.status_changed      | run -> failed
```

The step failed with `model_executor_unconfigured`, `retryable: false`, which is
the intended outcome with no provider credential configured — and is the
behaviour that replaced silently reporting success. All six work items reached
`completed`; none was stranded or dead-lettered.

`agent_runs.run_version` and `run_streams.current_version` both read 7 — the
authoritative stream and the derived projection agree.

A run created with `x-correlation-id: 019ffade-0000-7000-8000-0000deadbeef`
recorded exactly that correlation on its `run.created` event. An earlier attempt
using a v4-shaped id was replaced by the router with a fresh v7: `validated_id`
requires `Version::SortRand`, which is the router behaving as specified rather
than a propagation failure. Verification data was removed afterwards.

## What was not removed

The legacy state engine in the domain — `LegacyRunEvent`, `decide()`, `apply()`
and the reducer — remains. It carries the conformance cases for run state
transitions, and deleting it would remove executable evidence rather than
duplicate machinery. What was retired is its use as a **write path**: no HTTP
handler issues `RunCommandEnvelope` any more.

## Evidence

- `vestrace-http` unit tests — 19 passed, including a test asserting that run
  creation reaches the coordinator, and tests covering the new steps route and
  its actor mapping. That the legacy executor is off the write path is now a
  compiler guarantee rather than an assertion: `AppState` no longer holds one.
- Full workspace suite against live PostgreSQL 17 — **780 passed, 0 failed,
  126 suites, exit 0**.
- Live-stack verification of a run acquiring steps and reaching the worker —
  recorded below.

## Remaining gaps

- Model execution still requires a provider credential; without one an
  agent-assigned step fails with `model_executor_unconfigured`, which is the
  intended behaviour and not a working execution.
- Single-shot invocation only: no tool use, no multi-turn loop, no planning.
- `input_references` cannot be supplied through the steps route, so step
  dependencies cannot yet be expressed over HTTP.
- Crypto custody, v1.0 evidence collection and conformance coverage are
  unchanged by this work; see the execution-and-secrets delta.
