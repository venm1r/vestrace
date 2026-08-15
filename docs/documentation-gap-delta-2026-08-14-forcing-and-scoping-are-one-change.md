# Forcing a table and scoping its adapter are one change

**Date:** 2026-08-14
**Scope:** third row-level-security batch — workflow execution history, the
workflow registry, evaluations, learning, and the two CLI commands that read
them.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice found row level security inert for the runtime role: sixty-six
tables enabled the policy without forcing it, and the runtime role owns them, so
`ENABLE` exempted exactly the role that runs every query. This slice continues
that work — and found the same split running the other way, where it had already
killed a surface.

## The finding: a forced table with an unscoped adapter is a dead surface

Migration `0124` forced row level security on `learned_projections` and
`learning_proposals`. `PgEvaluationRepository` — their only writer and reader —
used a bare `PgPool`, so `vestrace.workspace_id` was never set and
`vestrace_current_workspace_id()` returned NULL for every statement it issued. A
policy comparing `workspace_id` against NULL admits nothing.

**Every write to those two tables has been refused, and every read has returned
empty, since 0124.** Under the runtime role:

```text
POST /v1/learning/projections  →  500  {"code":"storage_failure"}
POST /v1/learning/proposals    →  500  {"code":"storage_failure"}
GET  /v1/learning/projections  →  200  []
```

and at the database, as the runtime role with no scope set:

```text
SELECT count(*) FROM learned_projections;          →  0
INSERT INTO learned_projections (...) VALUES (...);
ERROR:  new row violates row-level security policy for table "learned_projections"
```

This is the mirror image of the previous slice's finding, and the two together
name the actual rule: **forcing a table and scoping its adapter are two halves of
one change, and doing either half alone is silent.** Enable without forcing and
the policy is inert — tenant separation rests on each query remembering its own
predicate. Force without scoping and the surface dies — every write refused,
every read empty. Neither shows up in a suite that connects as a superuser,
because a superuser bypasses row level security in both directions.

The error reached the caller as an opaque `storage_failure` and was not logged,
which is the same trap the previous slice recorded. Nothing distinguished "the
database is unreachable" from "the policy refused this row".

## What was converted

Four adapters moved from a bare pool to a scoped transaction, and migration
`0143` forces the nine tables behind them.

| Adapter | Tables |
| --- | --- |
| `PgExecutionHistoryRepository` | `workflow_executions`, `step_executions`, `execution_artifacts`, `execution_outcomes` |
| `PgWorkflowRepository` | `workflow_definitions`, `workflow_revisions` |
| `PgEvaluationRepository` | `evaluations`, `evaluation_facts`, and the two already-forced learning tables |
| `PgDiagnosticsRepository` | owns none; counts rows in seven other tables |

`search_documents` is forced in the same migration without an adapter change:
its three callers already scope, two of them since the previous batch.

### Six unbounded queries closed

`PgExecutionHistoryRepository` took a `RequestContext` on **every** method and
ignored all of them — the writes bound `record.workspace_id`, the reads selected
on an id alone. `update_workflow_execution_status`, `get_workflow_execution` and
`update_step_status` selected on an id; `list_steps` and `list_outcomes` on a
`workflow_execution_id`. `PgWorkflowRepository::find_by_id` and `get_revision`
did the same, and the revision read returns a workflow's entire definition body.

Each carries the workspace in the statement now as well as in the transaction, so
the query and the policy hold the boundary independently and neither silently
becomes the only one holding it.

Two updates that matched no row previously reported success. A cross-tenant
update is a refusal now, deliberately indistinguishable from an update of a row
that does not exist — the caller learns the update did not apply, not whether the
id exists somewhere it may not see.

`submit_proposal` read the row back after updating it, in a second statement. It
returns the row from the `UPDATE ... RETURNING` instead, so it cannot report a
proposal some other writer produced in the gap.

## `vestrace doctor` examined a workspace that does not exist

Seven of the doctor's nine checks count rows in a tenant's tables. The command
built its context from `WorkspaceId::new()` — a fresh random id belonging to no
workspace — so those seven asked about a workspace that has never existed and
could only ever answer "nothing wrong". **A clean doctor report meant nothing had
been examined.** `vestrace rebuild` had the identical defect in its post-rebuild
verification.

Both now iterate the explicit top-level `workspaces` list, the same one the
worker polls and startup recovery sweeps, and refuse to run when it is empty.
This is the third time that random-`WorkspaceId` pattern has been found; the run
worker was the first.

Running it against a real workspace produced a real finding on the first
attempt — see below.

## `vestrace rebuild search-documents` has never run

It is the remediation two diagnostics print. It could not execute: the insert
named `memories.content_text` and `search_documents.search_tsv`, and neither
column exists. A memory's content lives on its active revision, and the index
column is `fts_vector`, generated by the schema.

```text
Error: error returned from database: column "search_tsv" of relation
"search_documents" does not exist
```

It rebuilds from `memory_revisions` through `memories.active_revision_id` now,
scoped per workspace, upserting on the `(workspace_id, memory_id)` key the
previous slice added — the same statement shape the write path uses, so a
rebuilt row and a written row agree. `fts_vector` is deliberately not written,
because computing it here would let the two disagree.

`rebuild embeddings` never rebuilt an embedding either. It counted memories
missing a search document and returned zero. It still does, named honestly:
`memory_embeddings` has no writer anywhere in the system, so a command claiming
to rebuild embeddings would claim a capability that does not exist.

## The outbox has no drain

With the doctor finally examining a real workspace, its first run reported:

```text
[WARN ] OUTBOX_LAG: 7 outbox messages pending for more than 60 seconds
         remediation: process the outbox queue or check worker health
```

The remediation is not actionable. `OutboxRepository::fetch_unprocessed` and
`mark_processed` are declared on the port and implemented on the adapter, and
**nothing calls either**. The outbox accumulates forever, so this warning fires
for every deployment that has ever written a memory. Recorded, not fixed: a
processor is a subsystem, not a line. The warning is at least true now.

## A third category the guard test could not see

The guard test compared *enabled* against *forced*. Thirteen tables have no row
level security **at all** — not enabled, not forced, no policy — so they were
invisible to it, and "sixty-six exempt" undercounted the hole by exactly that
number.

Most are process-level rather than request-level, and that distinction is real
rather than an excuse: `qualification_bundles`, `incidents`, `recovery_points`,
`release_manifests`, `revalidation_runs`, `trust_state_records`,
`product_releases` and `external_effect_fault_suite_evidence` describe a
deployment, carry no `workspace_id`, and a workspace policy on them would mean
nothing.

Two do carry one. `a_table_holding_a_workspace_id_has_row_level_security` asserts
the narrower property that needs no judgement — a table with a `workspace_id`
column belongs to a tenant, and a tenant table with no policy has no boundary
except the queries that remember to write one — and names the two exceptions:

- **`external_effect_intents`** (migration 0127). `PgExternalEffectRepository::find_intent`
  reads it by id alone across every tenant. Closing it means giving its two
  sibling tables a tenant first: `external_effect_receipts` and
  `external_reconciliations` carry no `workspace_id` to scope by at all, and
  inherit their tenant only through a join.
- **`supersession_links`** (migration 0118).

## Live verification

The stack was rebuilt and redeployed, and every surface exercised under the
runtime role, which is `NOSUPERUSER NOBYPASSRLS`.

```text
POST /v1/evaluation-facts                  201
POST /v1/learning/projections              201     (500 before this change)
GET  /v1/learning/projections              200  1 row
POST /v1/learning/proposals                201     (500 before this change)
POST /v1/learning/proposals/{id}/submit    200  status=submitted
POST /v1/workflows                         201
GET  /v1/workflows/{id}                    200
POST /v1/evaluations                       201
POST /v1/workflow-executions               201
POST /v1/workflow-executions/{id}/complete 204
POST /v1/retrieval/search                  200  1 candidate
```

A second workspace was seeded with its own access token — there is no API for
issuing a credential into another workspace — and used to read the first
workspace's rows:

```text
GET  /v1/workflows/{id}                    404
GET  /v1/evaluations/{id}                  404
GET  /v1/learning/projections/{id}         404
GET  /v1/learning/proposals/{id}           404
GET  /v1/workflow-executions/{id}          404
GET  /v1/workflows                         200  0 rows
GET  /v1/learning/projections              200  0 rows
GET  /v1/evaluation-facts                  200  0 rows
POST /v1/retrieval/search                  200  0 candidates

POST /v1/workflow-executions/{id}/complete 409
  {"code":"conflict","message":"the workflow execution does not exist in this workspace"}
```

That last line is the one worth reading twice. Before this change the same
request returned `204`: `WHERE id = $1` with no workspace predicate, on a table
whose policy was enabled but not forced. Another tenant's workflow execution
would have been completed, and the caller told it succeeded.

This is the first cross-tenant probe run through the HTTP surface with a real
second credential rather than at the database. Note that the workspace is taken
from the authenticated token and a caller cannot assert it — an
`X-Vestrace-Workspace` header is ignored, which is why the probe needed a second
token rather than a second header.

CLI, in the deployed container:

```text
vestrace rebuild search-documents
  Rebuilding search documents for workspace 1000…0001 ... done (2 documents rebuilt)
vestrace doctor
  Running diagnostics for workspace 1000…0001 ... done
  [WARN ] OUTBOX_LAG: 7 outbox messages pending for more than 60 seconds
```

## Where the schema stands

```text
forced   38   (29 before this slice)
exempt   45   (54 before)
none     13   (unchanged; two of them hold a workspace_id)
```

Nine adapters still hold a bare pool. `PgExternalEffectRepository` is the next by
value and needs a schema change first, since two of its three tables have no
tenant column. Jobs, the outbox, qualification, recovery and release evidence are
legitimately process-level.

## Test results

Full workspace suite against a live PostgreSQL 17: **838 passed, 0 failed, 129
suites, exit=0** (837 before this slice; the new test is the workspace-id guard).

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      61 passed (54 executed,  7 attested),   2 skipped
trusted     70 passed (54 executed, 16 attested), 129 skipped, 0 N/A
```

Unchanged by this slice, which is the expected result: it moved no requirement's
evidence. What it moved is the gap between what the suite proves and what the
deployed system does — three of the defects here were invisible to a green suite,
and one of them had made a documented surface answer 500 for every request ever
made to it.
