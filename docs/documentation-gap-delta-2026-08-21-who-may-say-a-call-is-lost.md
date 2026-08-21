# Who may say a call is lost

**Date:** 2026-08-21
**Scope:** ownership and a stated deadline for an external effect's dispatch.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. It changes no observable behaviour and is not supposed to: it
replaces a guess with a statement, so that the slice after it can be safe.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, twice, deliberately
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
cargo fmt / clippy:                      clean
```

## Why this slice exists at all

It was not the plan. The plan was to close finding 2 of the 2026-08-20 delta by
adopting a lost dispatch into `UNKNOWN`, and an adversarial review refused it on
four counts. Three of them followed from the first.

**An external effect's `Dispatching` transition carried no owner, no generation,
no deadline and no lease.** Run recovery has had exactly these since migration
0021 — `run_leases` holds `worker_id`, `generation`, `heartbeat_at`,
`lease_until` — and excludes live leases for that reason. Effects had nothing.
So `DISPATCH_CONSIDERED_LOST_AFTER` was five minutes of guessing, and both
`vestrace server` and `vestrace worker` call `run_startup_recovery`, which means
a starting replica would have been guessing about another live replica's
in-flight call — inside, the review noted, the shipped HTTP adapter's own
fifteen-second timeout.

Every slice before this one added evidence: the worst case was a gap in what the
system knew. This is the first that would have had the system **judge somebody
else's work in progress**, and a wrong judgement there writes a falsehood about
a call that is still running.

## What was built

`external_effect_lifecycle_transitions` gains `dispatch_owner` and
`dispatch_expires_at`. Only a `dispatching` transition may carry them, and a new
one must carry both.

The owner is the `WorkerId` the process already mints for run leases — not a
second vocabulary for the same idea — and it is created at the command boundary
in `main.rs` and threaded down. The repository never invents one. A repository
that names the owner is a repository asserting something it cannot know, and an
`unwrap_or_else(WorkerId::new)` anywhere on this path would have made every
dispatch look owned by a process that never existed.

The deadline is stated by the dispatcher before the adapter is called, and
persisted. The question recovery asks changes shape entirely: not *"has enough
time passed for me to assume this is dead"* but *"did the process that took this
on say it would be finished by now"*. The first is a threshold one replica
applies to another's work. The second is a promise anybody can check, which is
why the startup special case disappeared rather than being made safe — an
expired dispatch is expired whoever notices.

`DISPATCH_CONSIDERED_LOST_AFTER` is now `DEFAULT_DISPATCH_ALLOWANCE`. The
duration is unchanged; who is entitled to state it is not, and a constant whose
doc comment has to open with "despite the historical name" is a name that
outlived its meaning.

### The CHECK is `NOT VALID`, and the exemption is counted

Migration 0153 wrote `dispatching` transitions before an owner or a deadline
existed. Backfilling them would mean inventing the identity of a process and the
promise that process supposedly made about a call which may already have touched
the world. `NOT VALID` exempts those rows while refusing every new malformed
one.

An exemption nobody can count is a leak wearing correctness as a costume, so the
port carries `count_deadline_less_dispatching_transitions` and the worker reports
it **once at startup**, per workspace, saying plainly that those dispatches need
an operator decision rather than a timeout. Once rather than per sweep: nothing
creates a deadline-less transition any more, so the number only falls, and
repeating it every tick would bury it.

That reporting was a review finding, not something the implementation arrived
with. The count existed and only tests called it — the same shape this subsystem
has been caught in three times now: `synthetic_unknown` has no production caller,
`supports_read_back` is declared and never read, and `worker.rs` carries a doc
comment admitting that `ExternalEffectRecoveryService` and
`HttpExternalEffectReadBackAdapter` "were both written and neither was
constructed".

## A correction to the plan that preceded this one

The refused plan rejected `ExternalEffectReceipt::synthetic_unknown` on the
grounds that fault point 3 expects `receipt_persisted: false`, and asserted that
"the suite says so". That is false. `evaluate_fault_suite` compares status,
`retry_attempted`, and — for point 3 only — `reconciliation_started`. **It never
reads `receipt_persisted`.** `FaultObservation::expected` declares the field and
the evaluator ignores it.

This is the second time these slices have read `FaultObservation::expected` as if
it were a specification; the first was the 2026-08-21 delta's claim that the
expectations demanded ignorance. It is a structure the evaluator draws some of
its comparisons from, and the rest of it is documentation. Which mechanism
adoption should use is therefore still open, and the review made the case for the
synthetic receipt that the refused plan did not answer: it lands the effect in
the unknown-receipt branch that already works, rather than in a state nothing
queries.

## Verification

The prediction here was inverted: **nothing should change**, because nothing
adopts a lost dispatch yet. It held. The five observations are byte-identical to
the previous delta's run, and were confirmed twice — once on the implementation
as delivered, and once after the two review fixes above:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

`cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17 with
pgvector apart from `authorized_shared_reader_uses_exact_revision_under_forced_rls`,
recorded as pre-existing in an earlier delta and verified there against `cec0628`.

**A flake was found and is recorded rather than chased.**
`tests/effect_fault_runtime.rs` reported 12 of 14 in one workspace run and 14 of
14 both standalone and on re-run. The two that failed are the container-driver
tests, and the run that failed them shared a Docker daemon with a database
container provisioned for this verification. A suite whose result depends on who
else is holding Docker is worth a look independently of this slice.

**A process note, recorded because it changes how much the report is worth.** The
implementing agent stalled — for the second time this session, at the same phase
— with the work complete in the tree but no report delivered. Everything above
was verified by running it here rather than by reading a summary, and the two
review findings were fixed by hand rather than sent back, because there was
nothing to send them back to. Across four slices no report has been accepted on
its own; three times the evidence had to be produced separately, and once that is
what surfaced a disagreement with the plan the report did not mention.

## What this does not do

**Nothing adopts a lost dispatch.** Fault point 3 still reads `Dispatching`. The
next slice does that, and the review named its three conditions: an atomic claim
so two workers cannot both adopt; a rule for a real receipt arriving after
adoption, which today would insert successfully and sort *earlier* than the
adoption because its timestamp is taken before dispatch; and a candidate set that
still contains crash-derived `UNKNOWN`, or a failed read-back after adoption
strands the effect in exactly the invisibility this work exists to end.

**Fault points 2 and 4 stay red** — authorization persists nothing, and an
acknowledged dispatch is still taken as a settled outcome, which §34 forbids by
name. **§18's evidence ladder is still not climbed.** The release gate still
reports four evidence families with no producer.
