# The lease was not a read leak

**Date:** 2026-08-14
**Scope:** fifth row-level-security batch — the run core, and the surfaces built
on it.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Four batches of this work have been about disclosure: a query with no workspace
predicate returning another tenant's rows. This one found something else in the
same place, and it is worth separating because the fix is the same and the
consequence is not.

## `PgRunLeasePort`

The run lease is the mutual-exclusion primitive that decides which worker may
advance a run. Two of its three methods took a `RequestContext`, ignored it, and
matched on `run_id` alone:

```sql
UPDATE run_leases SET heartbeat_at = $4, lease_until = $5
 WHERE run_id = $1 AND worker_id = $2 AND generation = $3 ...

DELETE FROM run_leases
 WHERE run_id = $1 AND worker_id = $2 AND generation = $3
```

and `acquire`'s takeover clause carried no workspace either:

```sql
INSERT INTO run_leases (...) VALUES (...)
ON CONFLICT (run_id) DO UPDATE SET worker_id = EXCLUDED.worker_id, ...
 WHERE run_leases.lease_until < $4
```

So an expired lease on **any** tenant's run was takeable by any worker. The row
kept its own `workspace_id` — `EXCLUDED.workspace_id` was never in the SET list —
while its `worker_id` came to belong to somebody else.

Nothing is disclosed by that. What happens instead is that one workspace can
take the lease on another workspace's run, which means the other workspace's
worker cannot: a tenant able to stop another tenant's runs from executing, and a
run that is supposed to have exactly one writer acquiring a second. The
`workspace_id` column on the row stays right while the lease it describes is
wrong, which is the shape of defect that survives an audit of the data.

All three statements carry the workspace now, in the takeover clause as well as
the predicate, and the adapter runs them in a scoped transaction.

## `PgWorkQueuePort` — the opposite case

Every statement in the work queue already carried `workspace_id = $n`, and had
all along. What it lacked was the connection: the queries went through a bare
pool, so `vestrace.workspace_id` was never set and the policy on
`run_work_items` had nothing to compare against.

That is worth naming as distinct from the lease. Nothing was wrong with its
results. The predicate was doing the whole job correctly, alone, with nothing
behind it if a later query forgot — which is precisely the state the first batch
of this work described as "tenant separation rests entirely on each query
remembering `WHERE workspace_id = $1`".

`PgStartupRecoverySource` was the same: correctly predicated in both branches
including the `NOT EXISTS`, unscoped at the connection.

## What migration 0145 forces

The run core, once those three adapters were converted:

```text
agent_runs  run_events  run_steps  run_work_items  run_leases
```

Seven more whose single adapter already scoped — `artifacts`,
`artifact_revisions`, `connections`, `connectors`, `external_triggers`,
`ag_ui_endpoints`, `cognitive_mutations` — and two with no adapter at all,
`run_exports` and `approval_records`, on the same reasoning as
`supersession_links` in the previous slice: free today, and the boundary is in
place before the first caller rather than after.

## Live verification

Rebuilt, redeployed, and a run driven end to end under the runtime role
(`NOSUPERUSER NOBYPASSRLS`) with every one of those tables forced:

```text
POST /v1/runs                     201
POST /v1/runs/{id}/steps          201   (If-Match: 3)
worker picks it up

run status        succeeded, version 7
run_steps         1  (succeeded)
run_work_items    6
run_events        7
run_leases        0  (acquired, heartbeat, released)
model_executions  1
artifacts         3
```

This exercises `PgRunLeasePort` (all three methods), `PgWorkQueuePort`
(`lease_next`, `complete`), `PgRunStore`, `PgRunCommandCommitter`,
`PgRunEventStore` and `PgArtifactRepository` against forced policies, plus
`PgStartupRecoverySource`, which swept three candidates at worker startup.

With a second workspace's credential:

```text
GET  /v1/runs/{id}          404
GET  /v1/runs               200  0 rows
GET  /v1/artifacts          200  0 rows
POST /v1/runs/{id}/cancel   {"code":"invalid_request","message":"not found: run not found"}
```

and the owner still reads the run as `succeeded` at version 7 afterwards.

**One inconsistency noticed, not fixed.** That cancel answers HTTP 400 while the
equivalent `GET` answers 404 — the handler maps a not-found run into
`invalid_request`. It refuses correctly, so this is a status-code wart rather
than a boundary problem, but a client cannot distinguish "no such run" from "bad
request" on the write paths.

## Where the schema stands

```text
forced   56   (42 after the previous slice)
exempt   31   (45 before)
none      9   (unchanged; none of them holds a workspace_id)
```

Six adapters still hold a bare pool: `fault_suite_evidence_repository`,
`idempotency_repository`, `job_repository`, `outbox_repository`,
`purge_repository`, `qualification_repository`, `recovery_repository`. Of the
thirty-one still-exempt tables, `jobs`, `outbox`, `idempotency_keys` and
`purge_audits` are process-level rather than request-level, and the rest belong
to subsystems with no adapter yet — claims, conflicts, budgets, sessions, tool
definitions, execution plans, federation.

`text_retriever` is the one remaining adapter that sets `vestrace.workspace_id`
by hand rather than through `begin_scoped`. It is correct, and it should be
converted so there is one way to do this rather than two.

## Test results

Full workspace suite against a live PostgreSQL 17: **840 passed, 0 failed, 129
suites, exit=0** — unchanged from the previous slice, which is the expected
result for a batch that adds no test. The evidence that matters here is the run
executed above, because a green suite ran against these adapters before the
change as well.
