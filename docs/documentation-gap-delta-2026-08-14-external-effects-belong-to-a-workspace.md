# External effects belong to a workspace

**Date:** 2026-08-14
**Scope:** fourth row-level-security batch — external effect intents, receipts
and reconciliations, plus `supersession_links`.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice's guard test named two tables holding a `workspace_id` with
no policy at all. This closes both, and the larger problem behind one of them.

## Three tables with no boundary of any kind

Migration 0127 created `external_effect_intents`, `external_effect_receipts` and
`external_reconciliations` with **no row level security at all** — not enabled,
not forced, no policy. This is a third state, distinct from the "enabled but not
forced" that the last two batches dealt with, and it is worse: there was nothing
to force.

`PgExternalEffectRepository` read all three by id alone:

```sql
SELECT ... FROM external_effect_intents      WHERE id = $1
SELECT ... FROM external_effect_receipts     WHERE id = $1
SELECT ... FROM external_reconciliations     WHERE id = $1
```

An `ExternalEffectReceiptId` from any tenant returned that tenant's receipt,
which carries the external resource id, the external version and the provider's
response digest — the effect itself, not metadata about it. The port took no
`RequestContext` on any method, so there was no authenticated workspace anywhere
in the path to check against.

## Why the receipts had no workspace to begin with

A receipt belongs to an effect, and the effect belongs to a tenant. The domain
says so: `ExternalEffectReceipt` holds an `effect_id` and no `workspace_id`, and
that is the right model — a receipt with an independently settable workspace
would be a receipt that can be wrong about whose effect it acknowledges.

It is also unusable as storage, because a table with no tenant column cannot
have a tenant policy. So the two facts sat in contradiction and the resolution
was to have no policy, which is how a domain decision becomes a security hole
without anyone deciding anything.

Migration `0144` adds `workspace_id` to both tables and makes it impossible for
that column to disagree with the intent, rather than asking an adapter to
remember:

```sql
ALTER TABLE external_effect_intents
    ADD CONSTRAINT external_effect_intents_id_workspace_unique UNIQUE (id, workspace_id);

ALTER TABLE external_effect_receipts
    ADD CONSTRAINT external_effect_receipts_effect_workspace_fk
        FOREIGN KEY (effect_id, workspace_id)
        REFERENCES external_effect_intents (id, workspace_id);
```

A reconciliation names both an effect and a receipt, so it references both the
same way — either one drifting would make it evidence about a pair that never
existed.

**Why not a policy that joins.** `EXISTS (SELECT 1 FROM external_effect_intents
...)` would express the boundary without a new column. It evaluates a subquery
per row on every read, and — the reason that matters — it leaves the tenant
derivable but not stated, so nothing can index it and nothing can constrain it.
The denormalized column plus the composite key states it once and enforces it in
the two places that count.

## What the adapter can and cannot check

The adapter binds the workspace from the request context, never from the record.
For an intent it can also compare, because an intent carries its own workspace,
and it refuses a mismatch by name. For a receipt there is nothing to compare —
the receipt has no workspace — so the adapter's binding would be an unchecked
assertion on its own.

The composite foreign key is what makes it safe, and the two mechanisms refuse
the two shapes of the mistake independently. Verified against the deployed
database as the runtime role (`NOSUPERUSER NOBYPASSRLS`):

```text
workspace A sees its own rows            intents 1   receipts 1
workspace B, unfiltered query            intents 0   receipts 0
unscoped connection, unfiltered query    intents 0

B files a receipt for A's effect, under B:
  ERROR:  insert or update on table "external_effect_receipts" violates foreign
          key constraint "external_effect_receipts_effect_workspace_fk"

B files a receipt for A's effect, claiming A:
  ERROR:  new row violates row-level security policy for table
          "external_effect_receipts"
```

The first is refused by referential integrity, the second by the policy. Neither
alone covers both.

## `find_reconciliation_candidates` lost an argument

It took both a context (upstream) and a `workspace_id` (as a parameter), which
let a caller pass one workspace and be scoped to another. No legitimate call
wants those to differ, so the parameter is gone and the context is the only
source. Its joins also carry the workspace now, so a candidate cannot pair an
intent with a receipt from another tenant even if the ids happened to match.

## `supersession_links`

The other table on the list. It has no adapter at all — no reader, no writer,
anywhere in the workspace — so enabling and forcing its policy costs nothing
today and means the boundary is in place before the first caller arrives rather
than after.

## What this does not do

`PgExternalEffectRepository` is constructed **nowhere** in the server, the
worker, or the CLI, and no HTTP route reaches it. The external-effect subsystem
remains unwired in the deployed system, as the Q13–Q16 deltas recorded. So this
slice makes the storage boundary correct for a path that nothing currently
walks; there is no request to exercise end to end, and none is claimed.

The backfill applied to zero rows, because the deployed database holds no
external-effect evidence. It is written to be correct rather than proven correct
on real data, and that distinction should not be lost if this schema is ever
migrated somewhere that has rows.

## Where the schema stands

```text
forced   42   (38 after the previous slice)
exempt   45   (unchanged)
none      9   (13 before; four closed)
```

**No table holding a `workspace_id` is without a policy.**
`a_table_holding_a_workspace_id_has_row_level_security` now asserts that against
an empty exception list. The nine remaining tables without row level security
carry no tenant column: `_sqlx_migrations`, and the eight process-level tables
describing a deployment rather than a workspace — `qualification_bundles`,
`incidents`, `recovery_points`, `release_manifests`, `revalidation_runs`,
`trust_state_records`, `product_releases` and
`external_effect_fault_suite_evidence`.

Forty-five tables still enable the policy without forcing it, behind nine
adapters that still hold a bare pool. Jobs, the outbox, qualification, recovery
and release evidence are legitimately process-level; the rest is the remaining
work.

## Test results

Full workspace suite against a live PostgreSQL 17: **840 passed, 0 failed, 129
suites, exit=0** (838 before this slice).

Two new tests, both of which hold under the superuser connection `sqlx::test`
provides — deliberately, because they exercise the half of the boundary that
does not depend on the policy:

- `external_effect_evidence_is_not_readable_from_another_workspace` — the query
  predicate, which has to hold for any future caller that reaches the database
  through a path that does not scope.
- `a_receipt_cannot_be_filed_against_another_workspaces_effect` — the composite
  foreign key, which is not a policy and so is not bypassed by a superuser
  either.
