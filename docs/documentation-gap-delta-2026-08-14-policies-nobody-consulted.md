# Policies nobody consulted

**Date:** 2026-08-14
**Scope:** the last twenty-eight inert row-level-security policies, the purge
that removed nothing, and idempotency — which had never once worked.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice counted twenty-eight tables carrying `ENABLE ROW LEVEL
SECURITY` without `FORCE` and left them named rather than fixed. This closes
them, and the two adapters that had to be scoped first turned out to be hiding
worse things than an inert policy.

## The count, and why it was survivable

The runtime role owns every table in the schema, and ownership bypasses an
unforced policy. Each of those twenty-eight had a workspace-isolation policy that
had never been consulted: the schema asserted isolation, and the only thing
actually keeping workspaces apart was a `WHERE workspace_id = $1` a caller had to
remember.

Most of them are reached by **no code at all** — schema for capabilities that
were designed and never built. `agent_packages`, `budget_accounts`, `conflicts`,
`interaction_sessions`, `policy_bundles`, `redaction_rules`,
`webhook_subscriptions` and eighteen more have no adapter. Forcing a table
nothing queries costs nothing and removes a false statement, so all twenty-eight
are forced in migration `0150`. Every one already carried a policy, which was
checked first: a table with RLS forced and no policy denies everything.

Four are reached by code. Two were already scoped. Two were not, and those are
the rest of this document.

```text
before:  28 tables ENABLE without FORCE
after:    0
```

## The purge that deleted nothing and recorded that it had

`PgPurgeRepository` ran on a bare pool. Every table it deletes from — `memories`,
`memory_revisions`, `memory_sources`, `knowledge_relations` — has forced
row-level security. On a connection where `vestrace.workspace_id` was never set,
`vestrace_current_workspace_id()` is NULL, the policy matches no row, and each
`DELETE` removes nothing while succeeding.

The audit insert then went through, because `purge_audits` was one of the
twenty-eight.

Demonstrated against the deployed database, inside a rolled-back transaction,
before the fix:

```text
SELECT count(*) FROM memories;   →  0          (nothing visible, unscoped)
DELETE FROM memories WHERE …     →  DELETE 0
INSERT INTO purge_audits …       →  INSERT 0 1
```

So a hard purge — the one irreversible operation in the system, the one a data
subject's erasure request ends at — would have reported success, destroyed
nothing, and written a durable record saying it had.

The adapter is now scoped, and three further things changed because the fix
would have been dishonest without them:

- **`purge_memory` returns what it removed, per table**, and the service refuses
  to report success when nothing was removed. A caller reading `Ok` can now
  believe the memory is gone.
- **The approval is recorded.** `HardPurgeMemoryService` takes an `approval_id`,
  passes it down, and the adapter ended with `let _ = approval_id;`. An audit of
  an irreversible act that cannot name what authorized it is not an audit;
  `purge_audits` now carries `approval_id` and `removed_counts`.
- **The relation delete is parenthesised.** It read
  `source = $1 AND workspace = $2 OR target = $1 AND workspace = $2`, correct
  only because `AND` binds tighter than `OR` — one edit away from deleting
  another workspace's relations, on the single statement that cannot be undone.
  It also now removes the embedding and search document, which it never did:
  a purged memory used to leave its vector and its index entry behind.

**Still true, and worth saying plainly:** `PgPurgeRepository` is constructed
nowhere and there is no HTTP surface for purge, so nothing in this deployment has
ever purged anything. What changed is that the mechanism no longer lies when it
is eventually wired up.

## Idempotency had never worked, in two independent ways

`idempotency_keys` held thirty-two rows and had never served a single replay.
Scoping its adapter meant checking that replays still worked, which is how this
was found.

**First: the key was being replaced.** `x-request-id` doubles as the idempotency
key for every write endpoint, and `add_request_context` parses it as a UUIDv7 and
substitutes a freshly generated one for anything else — deliberately, with a test
that says so, because every request should have a well-formed tracing identity.
The consequence nobody connected: a client using its own key had it silently
swapped on every attempt, so the retry carried a different key, the write
happened twice, and the request succeeded both times.

Rather than break tracing's guarantee, the two roles are now separate.
`idempotency-key` is the caller's and is kept exactly as sent; `x-request-id`
remains normalised and remains the fallback, which is safe precisely because by
then it is a UUID.

**Second: the hash could never match.** `compute_hash(&cmd)` serialised the whole
command, including the identifier the HTTP layer mints per request —
`EventId::new()`, `MemoryId::new()`, `RelationId::new()`. Two byte-identical
requests produced two different hashes, so a replay under a valid key was
rejected:

```text
first : {"id":"01a00176-0702-…","event_type":"observation", …}
replay: {"code":"conflict","message":"idempotency key reused with different request"}
```

There was no request that could ever replay successfully. The check could only
pass through to a new write (key replaced) or fail as a conflict (key kept).

Commands now declare an `IdempotentRequest` fingerprint — what the caller
actually asked for, excluding server-minted identifiers and the key itself.
Adding a field to a command forces a decision about whether it belongs, which is
the point: a silently unhashed field makes two different requests look like a
replay of one another.

## Live

**Replay, for the first time:**

```text
POST /v1/events  idempotency-key: order-2026-08-14-0042
first : id 01a0018a-db25-7852-8e12-bd9ac2ba1ed7
replay: id 01a0018a-db25-7852-8e12-bd9ac2ba1ed7   ← the same event
different body, same key → 409 idempotency key reused with different request
```

The same holds for memory writes: two identical `POST /v1/memories` under one key
return one memory.

**The policies are no longer decorative.** From an unscoped connection as the
runtime role:

```text
idempotency_keys visible: 0     (37 rows are actually stored)
purge_audits visible:     0
INSERT INTO purge_audits … → ERROR: new row violates row-level security policy
```

That last line is the whole point of the slice: the same statement inserted
successfully two hours earlier.

**Nothing else broke.** Events, memory writes, retrieval (14 candidates, top at
cosine 0.1593) and the health surface all behave as before, with the outbox drain
still embedding on write.

## What this does not do

- **Purge remains unwired.** No HTTP surface, no construction site. The
  mechanism is honest now; it is still unreachable.
- **`removed_counts` is not verified against a second reader.** It reports what
  the `DELETE` statements said they affected, which is the best available
  evidence inside the transaction and not the same as an independent check that
  the rows are gone.
- **Only the memory endpoints take `idempotency-key`.** The run endpoints still
  read `x-request-id` and require a UUID, so a caller there still cannot use its
  own key. That is a smaller trap than the silent one — it is at least rejected
  loudly — but it is not fixed.
- **Eighteen forced tables have no reader and no writer.** Forcing them is
  correct and free, and it does not make them less unimplemented. A schema this
  far ahead of its adapters is worth a slice of its own.

## Test results

Seven new tests: three on the purge service (removing nothing is not success,
the outcome reports what was removed, the approval reaches the repository) and
four on the idempotency fingerprint. Three of the four fingerprint tests were
verified to fail against the previous hashing — the one that passed is the one
asserting a *changed* request hashes differently, which the old code did by
accident, being unable to hash anything the same twice.

Full workspace suite against a live PostgreSQL 17: **875 passed, 0 failed,
exit=0**.
