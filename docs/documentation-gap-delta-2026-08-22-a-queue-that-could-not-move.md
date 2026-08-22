# A queue that could not move

**Date:** 2026-08-22
**Scope:** what happens to the sweep when an effect cannot be asked about at all.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. **It repairs a regression introduced by the slice before it.**

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
starvation test against the unfixed code: second sweep reconciled 0 effects where 1 was expected
```

## The regression, and whose it was

The previous slice bounded the reconciliation sweep to eight candidates a tick.
That fixed a real defect — an unbounded query in a 500 ms loop with a sequential
provider call per row — and introduced a worse one.

`UnreconciledEffect` lived only in the sweep's in-memory report. Nothing durable
recorded that an effect could not be asked, so an effect whose adapter has no
registered route, or whose provider is persistently unreachable, produced no
reconciliation, was therefore never excluded by the `retry_unsettled_before`
cutoff, and came back as a candidate on every sweep. Oldest-first ordering kept
such effects at the head of the queue.

Unbounded, that was waste: the same effects were asked about uselessly every
tick, and everything else was still processed. Bounded to eight, **eight
unaskable effects consume the entire budget every 500 ms, permanently, and
nothing behind them is ever reached.**

Bounding without backoff turned waste into starvation. The bounding was right;
the omission was not seeing what it did to a neighbouring property. No test
failed — the queue simply stopped moving, which is the quietest failure in this
subsystem so far. A gap shows up in a report, a false record is caught by a
check, a broken migration aborts loudly. This one is only visible as a count that
does not change.

It is visible at all because the same slice was made to distinguish "there was
nothing more to do" from "we ran out of budget". Without that, the defect would
have been silent and unobservable at once.

## What changed

A recovery attempt that **could not be made** is now recorded durably, in its own
table, and the candidate query excludes an effect whose most recent failed
attempt is newer than a cutoff — the same shape as the existing exclusion for a
reconciliation that settled nothing, with its own separate argument so that a
caller cannot make one imply the other.

It is recorded as an **attempt**, not as a reconciliation, and the migration is
named `0156_a_failed_recovery_attempt_is_not_a_reconciliation` so that the
distinction is found before the question is asked.
`ReconciliationOutcome::Inconclusive` means "we asked the provider and it could
not tell us" — a finding about the world. "We could not ask" is a finding about
us. Migration 0151 exists because two things like these were once conflated, and
the domain already keeps `UnreconciledEffect` apart from a reconciliation for
exactly this reason.

Three details went beyond what the plan asked for, and each is right:

- **Failure classification is wider.** A missing route, a provider failure, and
  an unusable empty response all back off. But a persistence or adoption failure
  **propagates** instead of being recorded as a failed attempt — not being able
  to ask and not being able to write are different, and the second must not wear
  the first's clothes.
- **A success supersedes an older failure** by strict timestamp ordering, so an
  effect that becomes askable again does not serve out a cutoff it earned while
  it was not.
- **Equal timestamps keep the exclusion.** Where the order is indistinguishable
  the system waits another minute rather than hammering a provider that may still
  be down. That is the right side to round toward.

The cadence is one minute, matching the inconclusive-retry cutoff and
deliberately not sharing its constant: the same number, arrived at separately,
for a different question.

## Verification

Run here against a PostgreSQL 17 with pgvector.

The claim that matters is that the starvation test bites. Against the unfixed
code the second sweep reconciled **0** effects where 1 was expected — eight
unaskable effects had taken the whole batch and the ninth, askable one was never
reached. A starvation test that passed against the bug would have protected
nothing.

The fault suite is byte-identical, as predicted: the scenario's adapter is
registered and its stub answers, so no attempt fails and no exclusion applies.

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from `authorized_shared_reader_uses_exact_revision_under_forced_rls`,
pre-existing and verified against `cec0628` in an earlier delta. Conformance gate
unchanged.

## What this does not do

**There is no escalation.** An effect that can never be asked — a removed
adapter, a provider that is gone — now backs off politely and returns once a
minute forever. §18's evidence ladder ends at step 8, "human verification, если
deterministic read-back отсутствует", and nothing implements it. Backing off is
better than starving the queue and is not the same as resolving anything.

**There is no exponential backoff.** A flat minute fixes the starvation; whether
a provider that has been down for a day deserves the same cadence as one down for
a minute is a policy worth arguing separately.

**Fault points 2, 3 and 4 stay red.** §18's evidence ladder is unclimbed, the
`NOT VALID` exemption debt from the 2026-08-22 lost-dispatch delta is open, and
the release gate still reports four evidence families with no producer.

**This was the prerequisite for the §34 work, twice over.** Confirming
acknowledged effects would grow the candidate population by orders of magnitude
*and* make unconfirmable adapters common — either alone would have made this
starvation the dominant behaviour rather than an edge case.
