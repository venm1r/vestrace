# A recorded attempt is not a settled outcome

**Date:** 2026-08-15
**Scope:** the external-effect reconciliation sweep — and, found on the way, what
row level security does to a migration.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate:   192 passed (184 executed, 8 attested), 0 failed, 7 skipped — unchanged
suite:  899 → 900 passed, 0 failed
```

## The defect

An external effect that times out gets a synthetic `unknown` receipt, and the
reconciliation sweep exists to go and ask the provider what actually happened.
It selected candidates like this:

```sql
LEFT JOIN external_reconciliations x ON x.effect_id = r.effect_id AND ...
WHERE r.outcome_status = 'unknown'
  AND x.id IS NULL
```

An effect left the sweep as soon as **any** reconciliation row existed for it.
But `reconcile_effect` produces `Inconclusive` when the observation reports
`effect_applied: None` — the provider answered and could not tell us. Nothing
was settled, and the effect was retired from the sweep permanently. Its receipt
stayed `unknown` forever, and nothing ever asked again.

The sweep then looked clean, because the thing it should have been asking about
was no longer in the set it swept.

This contradicted a distinction the code had already drawn in
`UnreconciledEffect`'s own doc comment — that a candidate the sweep *could not
ask about* stays a candidate "because nothing about it has been settled". The
same is true of one it asked and got no answer from. Only the storage query
disagreed.

And every outcome was reported identically: `Confirmed`, `NotApplied` and
`Inconclusive` all logged at `info` as *"an unknown external effect outcome was
settled"*. "The provider could not tell us" read the same as "confirmed, all
fine".

## What changed

`ReconciliationOutcome::is_settled()` draws the line: `Confirmed` and
`NotApplied` are answers; `Inconclusive` and `HumanRequired` are not. The
candidate query now asks for effects whose **most recent** reconciliation settled
nothing, and the settled names come from the domain rather than being literals in
SQL — a renamed variant would otherwise leave the query matching nothing, which
puts every already-confirmed effect back in the sweep.

An unsettled effect has to come back, but not immediately: the sweep runs every
worker tick, and re-asking every provider several times a second is worse than
not asking. `RECONCILIATION_RETRY_AFTER` is one minute, and lives with the
recovery service rather than in the worker, because it is a property of
reconciliation and not of any particular loop driving it.

The worker now separates `settled()` from `unsettled()`: the first at `info`, the
second at `warn` naming the retry interval. Only settled outcomes count as work,
so a permanently inconclusive effect cannot keep the worker from sleeping.

`reconciled_at` — the observation's own time — is lifted out of the payload into
an indexed column (migration 0151, the same way 0128 lifted `effect_id`). It is
deliberately not `created_at`: a sweep that recovers evidence after the fact
inserts rows *now* for observations taken earlier, and backing off from the
insert time would ask again too late. The live data shows the gap — one existing
row's `reconciled_at` precedes its `created_at` by 25 seconds.

## The finding I did not go looking for

The first version of migration 0151 backfilled the new column and then set it
`NOT NULL`. It failed:

```text
UPDATE 0
ERROR:  column "reconciled_at" of relation "external_reconciliations"
        contains null values
```

`UPDATE 0` against a table with rows in it. Migration 0150 forced row level
security on 28 tables; migrations run as the runtime role, which *owns* these
tables — and a **forced** policy applies to the owner too. The policy admits only
rows matching `vestrace.workspace_id`, which a migration has no single value for,
because it is fixing every workspace at once.

So the backfill saw no rows.

**This generalises, and that is the part worth keeping.** Since 0150, any data
migration touching those 28 tables silently affects nothing. Mine failed loudly
only because it added a `NOT NULL` constraint that the un-backfilled rows
violated. A migration that normalised a column, or repaired bad values, or
populated a nullable column, would have reported success having changed nothing —
and there would be no error anywhere to find later.

0151 lifts FORCE for the backfill and restores it. That is safe in a migration
and nowhere else: `ALTER TABLE` takes an ACCESS EXCLUSIVE lock, the migration is
one transaction, and DDL in Postgres is transactional — so no other session can
read the table while the policy is lifted, and a failure rolls back to FORCE
rather than leaving it open.

I checked whether any existing migration is already silently broken by this:
**none is.** 0150 is the migration that forced RLS and it performs no backfill
afterwards, and 0151 is the first data migration since. This is a trap that has
been armed for one slice and has not yet caught anything but itself.

## Evidence

A live PostgreSQL test, `an_inconclusive_answer_does_not_retire_an_effect_from_
the_sweep`: an inconclusive answer leaves the effect out of the sweep at the
instant it was asked, brings it back once the attempt is old enough, and a later
`Confirmed` retires it for good however old that is.

Mutation-proved by restoring the old predicate, which failed with *"an
inconclusive answer retired the effect from the sweep, so its outcome stays
unknown forever and nothing ever asks again"* — and passed again once restored,
each direction with a forced rebuild.

Live: image built (exit 0) and verified to contain both the new log message and
migration 0151 by grepping the binary; migration applied to the deployed database
(`151 | a recorded attempt is not a settled outcome | t`); both existing rows
backfilled with their real observation times; `relforcerowsecurity` back to `t`;
worker and server healthy.

## What this does not do

- **Nothing acts on a settled outcome either.** `NotApplied` means the system
  dispatched an effect that did not happen. It is now logged at `info` and
  written to a table, and **the run that requested it is not told**. That was the
  standing gap before this slice and it still is; this fixed the sweep's memory,
  not what anybody does with what it remembers.
- **`HumanRequired` is produced by nothing.** `reconcile_effect` maps
  `Some(true)`/`Some(false)`/`None` onto `Confirmed`/`NotApplied`/`Inconclusive`.
  The fourth variant is unreachable, so the honest reading of the new
  `unsettled()` path is that it has exactly one real member today.
- **The retry is a fixed interval, not a backoff.** An endpoint that is down for
  a day is asked about every minute for a day. Exponential backoff is what the
  outbox does and what this should probably do; a constant was enough to stop the
  spin and I did not invent a schedule nobody asked for.
- **Nothing ever gives up.** An effect that is inconclusive forever stays a
  candidate forever. That is better than the previous behaviour of silently
  giving up immediately, but "when do we stop asking and escalate" is unanswered.
- **The RLS-and-migrations problem is recorded, not solved.** Every future
  backfill on a forced table has to remember to lift FORCE. Nothing enforces
  that, nothing tests for it, and the next person to forget gets a migration that
  reports success and changes nothing.

## Test results

Full workspace suite against a live PostgreSQL 17: **900 passed, 0 failed**.
Conformance gate: **199 total, 192 passed (184 executed, 8 attested), 0 failed,
7 skipped** — unchanged; this slice fixed a defect rather than closing a skip.

Remaining skips are unchanged: CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010,
REC-016, RET-004.
