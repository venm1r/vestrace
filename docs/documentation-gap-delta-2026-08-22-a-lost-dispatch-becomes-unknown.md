# A lost dispatch becomes UNKNOWN

**Date:** 2026-08-22
**Scope:** declaring a dispatch lost, and what that costs to do safely.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. It closes finding 2 of the 2026-08-20 delta in production, and
the fault scenario still cannot show it.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
two-sweeper race, real PostgreSQL:        one adoption, one reconciliation
migration 0155 against a legacy row:      aborted, until it was fixed — see below
```

## What was closed

A crash between `dispatch` returning and `insert_receipt` committing left an
effect that no sweep could see. §26 of the external-effects contract states the
required behaviour verbatim — `after dispatch / before receipt → UNKNOWN →
reconcile` — and §17 names a crash between provider response and persistence as
a cause of `UNKNOWN`.

Recovery now adopts such a dispatch: an effect with no receipt whose latest
transition is a `Dispatching` past its stated deadline gets an `Unknown`
transition with cause `dispatch_lost`, naming the `Dispatching` it supersedes.
Adoption happens **before** read-back and does not depend on it — §26 does not
make the state contingent on the provider answering — so an effect whose adapter
has no route, or whose provider is unreachable, is still `UNKNOWN` and is still a
candidate on the next sweep.

Three slices built the ground for this: the lifecycle transitions that can carry
the state, read-back routed to the adapter that actually dispatched, and a
dispatch that states its owner and deadline. This is what they were for.

## The two hazards adoption creates, and what answers them

**A late receipt would have been filed and ignored.** `insert_receipt` stamps its
transition with a time taken *before* the adapter call, and "latest transition"
ordered by `recorded_at DESC`. A real provider answer arriving after adoption
would sort *earlier* than the guess that superseded it and never become the
effect's state.

Transitions now carry an `ordinal` and are ordered by it. But an ordinal is not
enough, and the reason is worth keeping: a sequence value is allocated at
`INSERT` and becomes visible at `COMMIT`, so a transaction that started earlier
can commit later carrying a *lower* number. `insert_receipt` runs the moment the
adapter returns and adoption fires once the deadline has passed — which is
exactly when a slow call is still outstanding — so the two interleave, and
ordering alone would let a guess outrank an answer.

So a receipt-derived transition outranks a `dispatch_lost` one **by kind**,
regardless of ordinal. That is a statement about evidence rather than about
clocks or sequences: a record of what came back is worth more than a record of
nobody having heard. `lower_ordinal_receipt_still_outranks_a_later_dispatch_lost_guess`
pins it by writing the two in the adverse order deliberately, rather than hoping
a race produces it.

**Two workers would both adopt.** `begin_scoped` sets no isolation level, so
transactions run at `READ COMMITTED`, where a conditional "adopt only if
`Dispatching` is still latest" is not safe: both adopters can observe it as
latest and both insert. A partial unique index on the adoption is therefore not a
belt-and-braces measure but the only primitive available, and the loser learns it
lost from a unique violation rather than from a check it performed earlier and no
longer holds. `two_concurrent_recovery_candidates_adopt_and_reconcile_exactly_once`
runs two real sweepers against PostgreSQL; a sequential test would have proved
the easy case.

Adoption also had to stay discoverable. The receipt-less candidate branch matched
only a latest `Dispatching`; once adoption makes `Unknown` latest, that branch
stops matching, and a read-back failure after adoption would have stranded the
effect in exactly the invisibility this work exists to end. The branch now also
matches a crash-derived `Unknown`.

## The migration would not have run

The sharpest finding is not in the feature. Migration 0155 first added the
ordinal as `ADD COLUMN ordinal BIGSERIAL`, and against a database holding one
pre-ownership `dispatching` row it **aborted**:

```text
ExecuteMigration(..., 155): new row for relation "external_effect_lifecycle_transitions"
violates check constraint "external_effect_lifecycle_dispatch_ownership_qualified"
Failing row: (..., dispatching, dispatch_started, ..., null, null, 1)
```

`BIGSERIAL` installs a `nextval(...)` default, which is volatile, so PostgreSQL
rewrites the table — and a rewrite **re-validates every CHECK constraint,
including ones added `NOT VALID`**. The exemption migration 0154 granted to rows
written before an owner and a deadline existed was destroyed by the very next
migration, and those rows then violated the constraint they had been exempted
from.

Any deployment that ran 0153 and 0154 and holds even one such row could not have
migrated to 0155 at all. This was a broken release, not a failing test.

Fixing it took two steps rather than one. Adding the column as nullable `BIGINT`
avoids the rewrite, but the backfill is an `UPDATE`, and PostgreSQL evaluates
CHECK constraints against updated tuples too — so the ownership constraint has to
be dropped for the duration of the backfill and reinstalled, under the
`ACCESS EXCLUSIVE` lock the migration already holds, in the single transaction it
already runs in.

**And the hazard has not gone away.** `NOT VALID` skips the initial validation
scan and nothing more. The exempted rows still violate the constraint, so any
future rewrite of this table re-exposes them, and so does any `UPDATE` that
touches one — *even an update to an unrelated column*. What 0154's delta
described as the truthful compatibility state is more precisely a deferred
failure that holds only while nobody touches those rows.

The repair is recorded and not attempted here: encode the exemption explicitly,
as a durable marker saying these rows predate ownership, then replace the
`NOT VALID` constraint with a fully validated one. It must not fabricate an owner
or a deadline for calls nobody recorded — that is the thing 0154 refused to do,
and it is still refused.

## A premise corrected, and a review miss admitted

The first draft of this plan asserted that `cause_ref` "already names the specific
`Dispatching` transition", making the unique index precise. It does not:
`record_dispatch_started` writes the **effect** id, so every dispatch of one
effect carries the same value and an index keyed on it could not tell one attempt
from another. The port now returns the transition's own identity, which the table
has had since 0153 and nothing ever exposed.

The drift was mine to catch and I did not. The 0153 plan said `cause_ref` for a
dispatch would hold the intent's idempotency key; the implementation wrote the
effect id; the review — mine — passed it; and the delta then described `cause_ref`
as "the id of the record that is the evidence", which for that one cause is not
true. It is left as it is, because changing it means rewriting rows that describe
calls already made, and it is written down here so the claim is not repeated.

## Verification

Everything below was run here, against a PostgreSQL 17 with pgvector, because
the implementing agent had neither a database nor Docker and said so plainly
rather than reporting compiled tests as executed.

The fault suite is byte-identical to the previous run, which is what the plan
predicted. Point 3's dispatch is seconds old and `DEFAULT_DISPATCH_ALLOWANCE` is
five minutes, so its deadline has not passed when the scenario sweeps, and
adoption does not fire:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

`cargo fmt` and `clippy` clean. `cargo test --workspace --no-fail-fast` green
apart from `authorized_shared_reader_uses_exact_revision_under_forced_rls`,
recorded as pre-existing and verified against `cec0628` in an earlier delta.
Conformance gate unchanged.

## What this does not do

**The property is proven by tests, not by the scenario.** Adoption fires only on
an expired deadline, and the scenario sweeps seconds after the crash, so the one
harness that drives the real lifecycle never reaches the code this slice added.
An effects sweep at startup would reach it — a process that is gone cannot still
be dispatching, so no deadline is needed there — and that is now safe to write,
which it was not before ownership existed. Until then this closes the defect for
running deployments and the fault suite cannot say so.

**Criterion 6 was not satisfiable as written.** It asked that pre-0155 rows
receive ordinals "in insertion order". No such order was stored: `NOW()` is a
transaction timestamp and UUIDs are not temporal. The migration reconstructs a
deterministic `(created_at, id)` order instead, and the implementing agent said
the requirement was impossible rather than quietly doing something adjacent and
calling it done.

**Fault points 2 and 4 stay red.** Authorization persists nothing, and an
acknowledged dispatch is still treated as a settled outcome, which §34 forbids by
name. **§18's evidence ladder is still not climbed.** The release gate still
reports four evidence families with no producer, the fault suite among them.
