# Documentation Gap Delta — Row Level Security Was Inert for the Runtime Role

**Date:** 2026-08-14
**Scope:** revision hydration, classification filtering, and what testing RLS revealed
**Repository state:** dirty, implementation changes uncommitted
**Status:** the finding is recorded and guarded; the fix is **not** applied

## The finding

**Sixty-six of the eighty-three tables in this schema enable row level security
without forcing it.** Seventeen force it.

`ENABLE ROW LEVEL SECURITY` exempts the table's owner. `FORCE` removes that
exemption. In this schema the runtime role **is** the owner of every table —
`docker/postgres/init-runtime-role.sh` hands it the database and the schema.

So the workspace isolation policies are present, readable in the migrations,
visible in `\d`, and **inert for the one role that runs queries**. On those
sixty-six tables, tenant separation rests entirely on every query remembering
`WHERE workspace_id = $1`.

That is not a theoretical concern. Twenty-four adapters in
`crates/vestrace-infrastructure/src/postgres/` never call `begin_scoped` at all,
and at least two take a `RequestContext` they ignore outright
(`PgAuditRepository`, `PgExecutionHistoryRepository`). `PgRetrievalJournal` was a
third until yesterday.

## Why nothing caught it

Every database-backed test in this repository connects as a superuser, because
`sqlx::test` provisions the connection from `DATABASE_URL` and that user is a
superuser in every developer and CI setup. Superusers bypass row level security
outright.

The policies could have been deleted wholesale from the migrations and the suite
would not have moved.

This limitation had been noted twice before, in the run-store delta and in the
test file headers, as "RLS is not exercised". What was not appreciated is that
the same blind spot was hiding a defect in the schema itself, not just an
untested path.

## What is in place now

`crates/vestrace-infrastructure/tests/row_level_security.rs` — the first tests
in this repository that run under a role which cannot bypass the policy. They
create a `NOSUPERUSER NOBYPASSRLS` role, `SET LOCAL ROLE` to it, and query
**without** the application's own workspace predicate. If a row comes back, the
policy is not doing the work, which is the only way to tell an enforced boundary
from a well-behaved query.

| test | what it shows |
| --- | --- |
| `a_scoped_reader_sees_only_its_own_workspace` | An unfiltered `SELECT` under a scoped role returns one workspace's rows |
| `an_unscoped_reader_sees_nothing` | A connection that forgets to scope itself reads an empty database, not the whole one |
| `naming_another_workspace_does_not_reach_its_rows` | The database enforces the scope it is given faithfully, and cannot know the scope was wrong — which is why it is set from an authenticated credential and never from a request body |
| `the_forced_policy_applies_to_the_owning_role` | The sixty-six exempt tables are listed by name, and the assertion is one-directional: a table *joining* the list fails, a table leaving it passes |

The last one also asserts that no table forces the policy while carrying none —
a table in that state reads as safe and denies every row to everyone.

## Why it is not fixed in one migration

A single `DO` block over the catalog forces all sixty-six. It was written, run
against a test database, and **withdrawn**.

Forcing the policy makes every unscoped write a refusal and every unscoped read
an empty result. With twenty-four adapters not scoping, that is most of the
system — and the suite would not show it, because those tests run as superuser
and would stay green while the deployed stack stopped working.

The order has to be: scope the adapters, force their tables, verify on the live
stack, repeat. Doing it the other way round replaces a silent exposure with a
broken system, and the second is not an improvement on the first.

## First batch: seven tables

Seven adapters were converted from a bare `PgPool` to a scoped transaction, and
migration `0139_force_rls_on_scoped_tables.sql` forces their tables:

| table | adapter |
| --- | --- |
| `audit_events` | `PgAuditRepository` |
| `models` | `PgModelRepository` |
| `agents` | `PgAgentRepository` |
| `skills` | `PgSkillRepository` |
| `providers` | `PgProviderRepository` |
| `routing_decisions` | `PgRoutingDecisionRepository` |
| `model_executions` | `PgModelExecutionRepository` |

`PgAuditRepository` is the one worth naming. It took a `RequestContext` and
ignored it, writing `event.workspace_id` through an unscoped pool with nothing
checking that the two agreed. An audit event written into another tenant's trail
is the least recoverable kind of leak, because afterwards it is
indistinguishable from a real one. It now refuses a mismatch explicitly, before
the policy would — an explicit refusal names the problem where a policy
violation surfaces as a storage error.

Verified on the live stack after redeploying: every one of those surfaces still
reads and writes (`providers`, `models`, `agents`, `skills`, `access-tokens`,
`runs`, `audit` all 200; provider, model, credential and run creation all 201),
and the audit trail records and reads back fifteen events.

**Schema-wide progress: 24 forced, 59 still exempt.** The exempt list in
`row_level_security.rs` shrank by seven and the test still passes, which is what
the one-directional assertion is for.

## Second batch: the memory graph

Five more tables — `memories`, `memory_revisions`, `memory_sources`,
`knowledge_relations`, `events` — under
`0140_force_rls_on_memory_graph.sql`. This is where the tenant's content
actually lives, which made it the batch that mattered most.

Their four adapters took **no `RequestContext` at all**: `save_memory(&memory)`,
`save(&event)`, `save_source(&source)`, `save_relation(&relation)`. There was no
authenticated workspace anywhere in the write path. Every entity carries its own
`workspace_id`, so an adapter could have scoped itself from the value it was
handed — worth naming as the wrong answer, because the policy enforces the scope
it is given and cannot know the scope was wrong. Scoping from the entity would
make every policy pass by construction and check nothing. The ports take the
context now and the adapters refuse an entity claiming a different workspace.

**Two unbounded reads found while converting.** `find_memory_by_id` and
`EventRepository::find_by_id` selected on `id` alone with no workspace
predicate, so a memory or event id from any tenant returned that tenant's row —
and the policy did not stop it either, for the reason this whole document is
about. Both filter on the workspace now, so the query and the policy each carry
the boundary independently.

Schema-wide after both batches: **29 forced, 54 exempt**.

## A pre-existing defect the live check surfaced

Exercising the memory write path end to end — which nothing had done here, as
`memories` held zero rows in a long-lived development database — showed that
**creating a memory through the API fails and always has**.

`MemoryService::remember_memory` activates the memory and then writes, in this
order: memory, revision, source. `tr_active_memory_has_source` is a
`DEFERRABLE INITIALLY DEFERRED` constraint trigger, so it evaluates at commit —
and `save_memory` commits on its own, before the source exists. The commit is
refused with "active memory must have at least one source", surfaced to the
caller as `storage_failure`.

This is **not** a regression from the scoping work. `execute(&self.pool)` runs a
statement in autocommit, so each of the four writes was already its own
transaction; converting them to explicit scoped transactions left the commit
boundaries exactly where they were. Reproduced directly against the database:

```text
INSERT INTO memories (... status 'active' ...);
COMMIT;
ERROR:  active memory must have at least one source
```

### Fixed, and the cycle underneath it

`MemoryRepository::save_new_memory` now writes the memory, its first revision
and its source in **one** transaction, so the deferred trigger evaluates once
the source is present — which is what deferring it was for.

That alone was not enough, and the reason is worse than the ordering. The two
tables reference each other:

- `memories.active_revision_id` → `memory_revisions.id`
- `memory_revisions.memory_id` → `memories.id`

Neither key was deferrable, so **no order works** for a new memory that already
has an active revision. Memory first is refused because the revision does not
exist; revision first is refused because the memory does not. The only way
through was to insert the memory with a NULL `active_revision_id` and update it
afterwards — a workaround for a cycle rather than a design, and nothing did it.

`0141_defer_memory_active_revision_fk.sql` makes that one key
`DEFERRABLE INITIALLY DEFERRED`, matching what the schema already says about
`tr_active_memory_has_source`. The reverse key stays immediate: only one side of
a cycle needs to yield, and leaving the other strict keeps an orphaned revision
impossible at the moment it is written rather than at commit.

Verified live — the first memory this database has ever held:

```text
memories         | 1
memory_revisions | 1
memory_sources   | 1
```

Worth noting how it stayed hidden: the failure is invisible to the test suite
because the service is exercised against in-memory doubles, and invisible in the
logs because the error is mapped to an opaque `storage_failure` body — the
`tracing::error!` that should accompany it did not appear either, which is its
own small gap.

### And one more thing behind it

With a memory finally in the database, retrieval still returned nothing:
`search_documents` held zero rows. **Nothing had ever populated the search
index.** It is the only table the text channel reads, so every retrieval this
system has served returned nothing — successfully, with no warning and no
degraded flag, because from the channel's point of view an empty index and an
empty result set are the same thing.

That could not be seen until a memory could be created at all, which is the
pattern this whole document is about: each defect was hidden behind the one in
front of it.

The index is written in the same transaction now, because it is a projection of
the active revision — an index updated afterwards is an index that is wrong
whenever the step after fails. `0142_search_document_is_one_per_memory.sql`
gives it the key it needed: there was an *index* on `(workspace_id, memory_id)`
but no constraint, so nothing stopped a second row appearing and an upsert had
nothing to conflict on.

Verified live, end to end for the first time:

```text
POST /v1/events    -> 201
POST /v1/memories  -> 201
POST /v1/retrieval/search {"query":"retrieval"}
  candidates: 1   fused 0.0205  active
```

`revise_memory` had the identical defect and shares the fix — but it could not
be verified live, because **there is no HTTP route for revising a memory**. The
service method exists and nothing exposes it. That is one more thing sitting
behind the others.

## What is left

Thirteen adapters still hold a bare pool, and `PgExternalEffectRepository`,
`PgExecutionHistoryRepository`, `PgWorkflowEvaluationRepository` and
`PgDiagnosticsRepository` are the substantial ones by query count. Some are
legitimately unscoped: jobs, the outbox and qualification are process-level
rather than request-level, and forcing their tables would be wrong rather than
merely disruptive — that distinction has to be made per table rather than swept
along with the rest.

## An operational trap found on the way

`sqlx::migrate!` embeds the migration set at **compile time**, and adding a
migration file does not reliably trigger a rebuild. Three tests failed with
"migration history incompatible" purely because the test binary held the
previous set; touching any source file in the crate fixed it. Worth knowing
before diagnosing a real incompatibility.

## Also in this slice: revision hydration

RET-001, RET-002 and RET-015 needed a hydration step that did not exist.
Candidates carried content because the retriever happened to select it alongside
the reference, so nothing checked that the text handed to a reader was the text
of the revision the reference named.

`RevisionHydrator` resolves a reference to the exact revision, selecting on the
revision id with nothing resembling `ORDER BY revision_number DESC LIMIT 1`. Six
database-backed tests cover it, including the one that matters: asking for
revision 1 while revision 2 exists returns revision 1.

`ClassificationPolicy` enforces sensitivity at that boundary. Three properties
are deliberate:

- **Unclassified is governed separately from the label set.** A revision with no
  classification has not been assessed, which is not the same as having been
  assessed as public.
- **There is no "allow everything" variant to drift into.**
  `ClassificationPolicy::permissive()` exists, is named for what it is, and has
  to be chosen — findable by searching for the call.
- **Withholding is not filtering.** A refused revision comes back as its
  identity and the reason, never its content. A boundary that silently drops
  rows produces a smaller answer that looks complete, and a reader cannot tell
  "there was nothing" from "there was something you may not see".

## Where MEMORY stands

Two requirements remain, and both skips now explain themselves rather than
saying "no conformance case registered yet" — a phrase that covers both a
requirement nobody has looked at and one whose evidence exists but cannot run in
this runner, which call for entirely different work.

- **RET-004** — verified by the RLS tests above, which need a live database. The
  conformance runner is assembled from the domain and application layers; an
  infrastructure source would be a third.
- **RET-011** — no invalidation mechanism exists. Superseding a memory does not
  propagate into retrieval results or into previously issued context packs.
  Missing implementation, not a missing case.

## Test results

Full workspace suite against a live PostgreSQL 17: **837 passed, 0 failed, 129
suites, exit=0** (819 before this slice).

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      61 passed (54 executed,  7 attested),   2 skipped
trusted     70 passed (54 executed, 16 attested), 129 skipped, 0 N/A
```
