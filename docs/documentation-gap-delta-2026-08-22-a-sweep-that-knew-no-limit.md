# A sweep that knew no limit

**Date:** 2026-08-22
**Scope:** how much of the reconciliation backlog one tick takes.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
candidate query:                         unbounded → LIMIT 8, oldest first
```

## The defect

`find_reconciliation_candidates` had no `LIMIT` and used `fetch_all`. The worker
calls it once per workspace on every tick of a 500 ms loop
(`crates/vestrace-cli/src/commands/worker.rs:212`, sleeping at `:219`), and each
candidate then costs a **sequential provider read-back**.

A workspace holding a thousand effects whose outcome nobody knows would load a
thousand rows and make a thousand sequential HTTP calls every half second, in the
same loop that claims run work and drains the outbox. The tick would not finish,
and the other work in it would not happen.

What makes this worth recording is not the missing keyword but where it was
missing from. `find_undelivered_outcomes` sits in the same file, answers the same
shape of question — take from a queue what is due — and has taken a limit all
along, driven in batches of 32. One bounded, one not, with nothing saying why.

That is the signature of a query written without looking at its neighbour, and it
is also why it survived: an unbounded `SELECT` looks like an ordinary one right
up to the moment the queue has a thousand rows in it.

## What changed

The limit is applied in SQL. Bounding in the service would still have loaded
every row, which is the part that hurts.

The ordering — oldest candidate first — is unchanged and is now load-bearing in a
way it was not before. With an unbounded query the order was cosmetic; with a
bounded one it is the thing that stops a backlog from starving the effects that
have waited longest. That is written where the ordering is, because a later
change to it would silently become a fairness change.

`RECONCILIATION_BATCH` is **8**, deliberately not the delivery batch's 32, and
the reason is the cost of an item rather than a preference: delivering a settled
outcome touches only the database, while reconciling one makes a sequential
network call to a provider. Reusing the neighbouring constant would have made the
numbers agree and the reasoning disappear — the same mistake as the missing
limit, committed from the other side.

A sweep that fills its batch says so. "There was nothing more to do" and "we ran
out of budget" are identical in a count, and only one of them means a backlog is
not draining; the worker warns when the budget is exhausted, so a queue that
never empties is visible rather than inferred.

## Verification

Run here against a PostgreSQL 17 with pgvector; the implementing agent had
neither a database nor Docker and listed the criteria it could not execute rather
than reporting compiled tests as run.

The fault suite is byte-identical to the previous run, as predicted: the scenario
holds one effect per point, far below a batch of eight, so a limit cannot change
what it observes.

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

The test that matters is the one showing successive sweeps **drain** the backlog
— returning the next oldest because the first sweep settled what it took. A test
that checked only the first batch would have passed against a sweep that makes no
progress at all, which is the failure mode a limit introduces.

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from `authorized_shared_reader_uses_exact_revision_under_forced_rls`,
pre-existing and verified against `cec0628` in an earlier delta. Conformance gate
unchanged.

## What this does not do

**Read-back is still sequential.** Bounding the batch bounds a tick; it does not
make the work faster. Concurrency here needs an answer about provider-side load
that this slice does not have.

**There is no backoff and no per-provider rate limit.** An effect that stays
unsettled comes back under the existing one-minute retry cutoff, which is what it
did before.

**Eight is a starting point, not a measurement.** Nothing here observes how long
a read-back actually takes against a real provider, so the batch bounds a tick by
assumption rather than by evidence.

**This is a prerequisite for the §34 work, not that work.** Confirming
acknowledged effects — the forbidden shortcut `HTTP success == business outcome`,
still committed by the system — would grow the candidate population by orders of
magnitude, and doing it against an unbounded query would have been the same
defect at a much larger scale. Fault points 2, 3 and 4 stay red, §18's evidence
ladder is unclimbed, the `NOT VALID` exemption debt is open, and the release gate
still reports four evidence families with no producer.
