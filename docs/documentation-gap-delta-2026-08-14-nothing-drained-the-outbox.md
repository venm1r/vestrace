# Nothing drained the outbox

**Date:** 2026-08-14
**Scope:** the outbox, its dispatcher, embedding on write, and the stub job
worker.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The outbox pattern was implemented in three of its four parts. Messages were
written transactionally with the memory that caused them. A repository declared
`claim_pending` and `mark_processed`, and Postgres implemented both. A health
invariant watched the backlog.

Nothing ever called `claim_pending`. Every message the system had ever written
was still pending, and `outbox.backlog_within_budget` fired on every deployment
with a remediation that said, in its own words:

> process the outbox queue or check worker health; **note that no component
> currently drains the outbox**

That parenthesis had been in the invariant's own definition — the system knew,
and told the truth about it, and nobody could act on it.

## The clause that read as concurrency control and was not

`claim_pending` carried `FOR UPDATE SKIP LOCKED`, the standard way for several
consumers to claim disjoint work. It ran through a bare pool, so the statement's
own implicit commit released the lock the moment the rows came back. Two drains
would have claimed the same rows and delivered every message twice, and the
query would have looked correct in review.

Holding the lock across delivery is not the fix. Delivery makes network calls,
and a transaction held open across one is how a slow provider exhausts a
connection pool. So the contract is stated instead of implied:

- **At-least-once.** A message is marked processed only after its handler
  returns `Ok`. A crash in between redelivers it.
- **Handlers must be idempotent**, and the port's documentation says so where a
  handler author will read it.
- The alternative, acknowledging before delivering, is at-most-once, which loses
  messages silently. Between a duplicate you can design for and a loss you
  cannot see, this picks the duplicate.

The query was also unscoped by workspace — it selected across all of them — which
is why it could not be given a scope: the caller that would supply one did not
exist. It now runs inside `begin_scoped` like every other adapter.

## What the drain delivers

`OutboxDispatcher` routes a message to the handler registered for its exact
topic. Two handlers exist, both `EmbedMemoryHandler`, on `memory.created` and
`memory.revised`. A revision changes what a memory means, and an index that
followed only creation would answer with the superseded meaning while every
retrieval reported success.

This closes the gap the previous slice named and left open: **the write path now
embeds**, without embedding inside the write transaction. Producing an embedding
is still a network call to a model, and it still cannot make creating a memory
fail — it happens after the commit, driven by the record the commit made.

Three decisions in the dispatcher are worth stating because the opposite of each
is the tempting one:

- **A failed delivery is not acknowledged.** It stays pending and is retried. A
  handler failing forever shows up as a growing backlog, which the invariant
  reports; a drain that discarded on failure would show up as nothing.
- **A topic no handler claims is not acknowledged either.** It is counted as
  `unhandled` and left where it is. Marking it processed would be the drain
  quietly deciding that an unimplemented consumer is the same as a delivered
  message.
- **A memory that has since been superseded or removed is acknowledged.** There
  is nothing to embed and nothing wrong; retrying forever would be the failure.

## The backlog invariant now names the topic

A bare count cannot distinguish a drain that is behind from a topic nobody
consumes, and those need different responses. The observation groups by topic, so
the message went from `{n} outbox messages have waited more than 60 seconds` to
the same sentence with `(event.recorded=7)` appended.

The remaining seven are the honest residue: `event.recorded` and
`memory.relation.linked` have no consumer, and now the doctor says which. That is
a gap made visible by the drain rather than one the drain introduced.

**A smaller thing found while checking this.** `health_occurrences` records that
an invariant fired — sequence, state, timestamp, evidence refs — but not the
detail it reported. The detail lives only on the finding row, where each
observation overwrites the last. So the eight recorded occurrences of this
finding cannot say what the backlog was at each one, and the pre-change message
could not be quoted here from storage; the counts below come from the `outbox`
table directly. Not fixed in this slice, and recorded rather than left implied.

## The job worker that completed work it had not done

`Worker::process_one`, in the application layer, was:

```rust
if let Some(job) = self.job_repo.lease_next().await? {
    // Process job execution
    self.job_repo.complete(job.id).await?;
    Ok(true)
}
```

It leased a job, executed nothing, and wrote `state = 'completed'` — a durable
claim in the database that work had been done. It was wired into the worker
binary's poll loop, so it ran on every deployment.

Nothing has ever enqueued a job, so the lie stayed latent. Any future producer
would have watched its work vanish into a row that said it succeeded. The stub is
deleted, along with its Postgres adapter; the port and the `jobs` table remain,
because `jobs.no_dead_letters` still reads the table and removing a table is a
separate decision from removing a stub that claimed to drain it.

Asynchronous work now has exactly two real mechanisms, both of which execute
something: `run_work_items`, leased by the run worker, and the outbox.

## Live

Against the deployed stack, with LM Studio on the host serving
`text-embedding-nomic-embed-text-v1.5`:

**The historical backlog, on worker startup**, counted from the `outbox` table.

```text
topic          | pending | processed        topic          | pending | processed
event.recorded |       6 |         0        event.recorded |       6 |         0
memory.created |       3 |         0   →    memory.created |       0 |         3
memory.revised |       1 |         0        memory.revised |       0 |         1
```

**A memory written over HTTP, with no `rebuild` run.**

```text
POST /v1/memories  "a peregrine falcon dives faster than any other bird"
outbox   memory.created  created 14:33:15.865  processed 14:33:16.264  (399 ms)
memory_embeddings  →  new row, space nomic-768, width 768, at 14:33:16.254
```

Then a query sharing no words with it:

```text
POST /v1/retrieval/search {"query":"which raptor stoops at the greatest speed"}
  rank 1 — that memory, cosine distance 0.2094 in space nomic-768
```

**A revision moves the vector.** The same memory revised to "the giant sequoia is
the largest tree by volume":

```text
embedding md5   8a528b16…  →  903fa590…
"which raptor stoops at the greatest speed"  distance 0.2094 → 0.3648
"what is the most massive plant on earth"    distance        → 0.2289
```

The index follows the active revision rather than the text the memory was born
with.

**The doctor, after.**

```json
{"code": "outbox.backlog_within_budget",
 "message": "7 outbox messages have waited more than 60 seconds (event.recorded=7)",
 "occurrence_count": 8, "status": "open"}
```

Still open, and correctly so — with a message that now names what is actually
stuck.

## What this does not do

- **`event.recorded` and `memory.relation.linked` have no handler.** The backlog
  will not reach zero, and it should not: nothing has been decided about what
  those messages are for. They are visible, counted and named, which is the
  difference between an open question and a silent one.
- **No retry limit and no dead-letter path.** A message whose handler always
  fails is retried forever. That is deliberate for now — the failure is visible
  in the backlog and in a warning per attempt — but a poison message will occupy
  a batch slot indefinitely, and a `dead_letter` state for the outbox is the
  obvious next thing.
- **No vector index still.** Unchanged from the previous slice, and now with
  rows arriving continuously rather than only on a manual backfill, it is closer
  to being measurable.
- **Delivery is at-least-once and the handler is the only thing making it safe.**
  `EmbedMemoryHandler` is idempotent because the store upserts on
  `(workspace, memory, space)`. A future handler that is not idempotent will be
  wrong, and nothing mechanical prevents it.

## Test results

Four new unit tests cover the dispatcher against fakes: a delivered message is
acknowledged, a failed delivery is not, an unhandled topic is counted and left
pending, and one failing topic does not block another. The third was verified to
fail by making the dispatcher acknowledge unhandled messages — it failed, and
passed again when reverted.

Two contract tests assert the worker binary builds a dispatcher, drains it in its
poll loop, handles both memory topics, and no longer runs the stub job worker.

Full workspace suite against a live PostgreSQL 17: **861 passed, 0 failed,
exit=0**.
