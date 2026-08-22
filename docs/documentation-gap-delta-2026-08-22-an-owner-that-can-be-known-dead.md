# An owner that can be known dead

**Date:** 2026-08-22
**Scope:** whether the system can tell that the worker which began a dispatch is
gone.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim, and it deliberately leaves fault point 3 red.

```text
fault suite (real ephemeral deployment): FAILED — 2 failures, byte-identical to the previous run, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
```

## The gap

A `Dispatching` transition records the `WorkerId` that began the call, and
nothing anywhere recorded whether that worker still existed. `worker_id` appeared
in exactly one table — `run_leases` — and only per leased run, so a worker
holding no run leases had no presence in this system at all.

Recovery therefore waited out `dispatch_expires_at` for every lost dispatch. With
the adapter-stated deadline that is twenty seconds; if the owning process has
stopped reporting, twenty seconds is spent waiting for information already
available.

It is also what an earlier adversarial review demanded before startup recovery
for effects could be considered at all: it refused "at startup every
`Dispatching` is lost" because a starting replica cannot distinguish another live
replica's in-flight call from an abandoned one. With a liveness record it can —
by checking rather than assuming.

## What was built, and the trap that was not walked into

`external_effect_worker_presence` records, per workspace, when a worker started,
when it last reported, and — if it stopped cleanly — when it stopped. The worker
registers before polling, refreshes on the cadence it already uses for run leases,
and marks itself stopped on an orderly shutdown. The lapse window is the same
sixty seconds `lease_ttl` already uses: those two numbers describe this
deployment's tolerance for a silent worker, and a third pair would be two answers
to one question.

A dispatch is lost if its owner is explicitly stopped, or if its presence has
lapsed. **A dispatch whose owner has no presence row at all is not lost on that
basis** and falls back to its deadline.

That last rule is the whole care of the slice. Absence is not death: it is also
what a dispatch written before this table existed looks like, and what a worker
that died before its first heartbeat looks like. Treating absence as evidence
would have declared lost every dispatch nobody knows anything about, and would
have repeated the compatibility error 0154 avoided with `NOT VALID` — a rule
whose meaning silently changes for rows that predate it.

The plan asked for something subtly wrong here and the implementation improved on
it. It said a clean shutdown should *remove* the presence row; deleting it would
have collapsed "stopped cleanly" into "no evidence at all", which are opposite
answers — the first is immediate evidence of death, the second is the
compatibility case that must wait out its deadline. A stopped tombstone removes
*active* presence without erasing the evidence recovery needs. Requirements 4 and
5 as written contradicted each other, and the builder noticed.

## A stricter constraint that was a failure mode

The presence table first carried `workspace_id ... REFERENCES workspaces(id)`,
and three PostgreSQL tests failed on it.

No table in this subsystem references `workspaces` — not intents, receipts,
reconciliations, lifecycle transitions, recovery attempts or authorizations. The
reference looked like integrity and was an asymmetry: a worker registers presence
for **every** configured workspace, so a workspace whose row was missing would
stop the worker from starting, while effects for that same workspace continued to
be recorded without complaint. A missing parent row would have taken down the
process over something nothing else here requires.

Isolation was never resting on it: the forced row level security policy enforces
that, as it does for every neighbouring table.

This is the third time this session a change that reads as tightening turned out
to be a new way to fail — after `BIGSERIAL` rewriting a table and destroying
0154's `NOT VALID` exemption, and after bounding the sweep turned waste into
starvation.

## Verification

Run here against a PostgreSQL 17 with pgvector; the implementing agent had
neither and listed the criteria it could not execute rather than reporting
compiled tests as run. It finished without stalling, which is worth noting after
five that did not.

**The fault suite is byte-identical and point 3 stays red — which is the
result.** The scenario's child registers no presence: it is a short-lived process
driving one effect, not a worker. Its dispatch is the compatibility case and
waits out its deadline exactly as before.

If point 3 had turned green, something would have been treating absence of
presence as death, and this delta would be reporting a defect instead.

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=2
```

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from the pre-existing `authorized_shared_reader_uses_exact_revision_under_forced_rls`;
conformance gate unchanged.

## What this does not do

**It does not make a crashed dispatch knowable instantly, and nothing can.** In a
database a crashed process is indistinguishable from a slow one until its
heartbeat lapses, and a lapse window is necessarily longer than the seconds the
fault scenario waits before sweeping. The parent knows its child died because it
waited on the process; that knowledge belongs to the harness, not to the system.

**Fault point 3 therefore needs a decision, not another mechanism.** Turning it
green means having the scenario exercise **startup** recovery — where a process
that is gone cannot still be dispatching — which this slice has now made safe
across replicas. That is a change to what the harness drives, and it was
deliberately not bundled into the slice building the thing it would depend on.

**There is no fencing.** Knowing a worker is gone is not the same as preventing
it from acting if it returns, and nothing here claims otherwise.

**Only external-effect dispatch.** Run leases keep their own ownership,
generation and heartbeat rules, untouched.

The `NOT VALID` exemption debt is open, §18's rank 1 is still not offered, no
minimum evidence strength governs settling, two workers can still both reconcile
one effect, and the release gate still reports four evidence families with no
producer.
