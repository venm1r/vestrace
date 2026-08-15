# A guard for the trap in the last slice

**Date:** 2026-08-15
**Scope:** enforcing what the previous delta could only record — that a
migration writing to a table under a forced policy has to lift it first.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate:   192 passed (184 executed, 8 attested), 0 failed, 7 skipped — unchanged
suite:  900 → 903 passed, 0 failed
```

## Why a guard rather than a note

The previous slice found that migration 0150 forced row level security on 28
tables, and that migrations run as the runtime role — which *owns* those tables,
and a forced policy applies to owners. A backfill in any later migration
therefore sees no rows.

That delta ended with an honest admission: *"every future backfill on a forced
table has to remember to lift FORCE. Nothing enforces that, nothing tests for it,
and the next person to forget gets a migration that reports success and changes
nothing."*

The failure mode is what makes a note insufficient. `UPDATE 0` is not an error.
A migration that normalises a column, repairs bad values, or populates a nullable
one succeeds, the deploy goes green, and the data is untouched. There is nothing
to find afterwards. 0151 only failed loudly because it went on to add a
`NOT NULL` that the un-backfilled rows violated — which was luck, not design.

## What the guard checks

Two assertions, deliberately kept apart because their failures are opposite:

- **A migration that writes to a forced table lifts the policy first.** Catches a
  write nobody can see.
- **A migration that lifts `FORCE` restores it.** Catches a table left readable
  across every workspace afterwards — the worse of the two, and the risk the
  first fix introduces.

Whether a table is forced is computed by replaying the migrations in order up to
the one being checked, tracking the last `FORCE` or `NO FORCE` in each file, so
the answer is "was this table forced *by the time this migration ran*" rather
than "is it forced today".

A third test asserts the guard is looking at something: that the directory
resolved, that 0150 is among the files, and that the case which prompted all this
is visible to it.

## What the guard taught me on its first run

It flagged `0136_access_token_authentication_policy.sql` for an `UPDATE` on
`access_tokens`, which 0135 had forced.

That is a **false positive**, and an instructive one. The `UPDATE` is the body of
`vestrace_touch_access_token`, a function that runs at request time, as the
caller, under a policy 0136 wrote for exactly that call — keyed on
`vestrace.authenticating_token_id` so it can touch precisely the row the caller
already resolved. It is not a migration-time write and it sees the row it is
aimed at.

So the guard now strips dollar-quoted bodies before looking for writes. A
function definition is not a migration statement, and the difference is the
difference between a check that finds defects and one that finds text.

This also confirms the previous delta's claim rather than contradicting it: **no
existing migration is silently broken.** 0136 was the only hit, and it is not a
backfill.

## Evidence

Mutation-proved in both directions against the real migration:

- Removing the `NO FORCE` from 0151 failed the first check with *"UPDATE on
  `external_reconciliations`, which is under a forced policy — the runtime role
  owns the table and a forced policy applies to owners, so this affects no rows
  and reports success"*.
- Removing the restore failed the second with *"lifts the forced policy on
  `external_reconciliations` and does not restore it, so every later read by the
  owning role sees every workspace's rows"*.

0151 was restored byte-for-byte afterwards — it is already applied to the
deployed database, and a single changed byte would be a checksum mismatch on the
next start. Verified: `151 | t` still recorded, and the guard passes.

## What this does not do

- **It reads SQL as text.** No parser. It recognises `UPDATE <table>`,
  `INSERT INTO <table>` and `DELETE FROM <table>` — the shapes this repository
  writes — and would miss a write behind a CTE, a trigger, or `EXECUTE`. A
  passing run means no migration writes to a forced table *in a shape the guard
  recognises*.
- **It cannot check that a lift was scoped correctly.** It counts a `NO FORCE`
  and a later `FORCE` for the same table in the same file. A migration that
  lifted the policy on one table and wrote to another would satisfy it.
- **It says nothing about whether the write is right.** Only that it can see the
  rows it is aimed at.
- **It does not fix the underlying awkwardness.** The reason a migration has to
  lift a policy at all is that the runtime role owns the tables and there is no
  privileged migration identity. A role that owned the schema and was separate
  from the role that serves requests would make this whole class of problem go
  away, and would be a deployment change rather than a migration — which is why
  0136 rejected the same idea for `SECURITY DEFINER`. That remains the real
  answer and nobody has taken it.

## Test results

Full workspace suite against a live PostgreSQL 17: **903 passed, 0 failed** (900
before; the increase is this guard's three tests). Conformance gate: **199 total,
192 passed (184 executed, 8 attested), 0 failed, 7 skipped** — unchanged.

Remaining skips are unchanged: CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010,
REC-016, RET-004.
