# An observation already being made

**Date:** 2026-08-23
**Scope:** the release gate's last missing producer.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. **All four of the gate's missing evidence producers now exist.**

```text
release gate flags:  --runtime-evidence --crypto-evidence --release-approval
                     --capability-restoration --fault-suite-evidence --recovery-qualification
release gate tests:  24 passed, 0 failed
fault suite:         FAILED — 2 failures, unchanged
conformance gate:    199 passed, 0 failed, 0 skipped — unchanged
```

## The slice that turned out to be a quarter the size

This was expected to need a harness for eight recovery targets — work comparable
to `vestrace-fault-scenario`, which took days for five fault points of a single
target.

It did not, because the observation was already being made.
`StartupRecoveryRecord` carries `target: RecoveryTarget` and
`classification: RecoveryClassification`, and `StartupRecoveryService` produces
one per interrupted run on every restart. The server and the worker logged them
and threw them away: the type had no reference anywhere in the infrastructure
crate.

So the system has been classifying interrupted work by recovery target, on real
runs, in production, and keeping no record of having done so. The difference
between this slice and the one that was planned is the difference between
building machinery and **no longer discarding what the machinery already
produces**.

That is the ninth time this session the gap has been between *modelled* and
*used*, and the first time closing it cost almost nothing.

## The shortcut refused

`RecoveryQualificationObservation::expected(target, evidence_ref)` exists, and
`evaluate_recovery_qualification` compares an observation's action against
`classification.expected_action()`. Filling the action from that same function
would make the check agree by construction — an observation that cannot disagree
with the evaluator says nothing about what happened.

The action is written from `StartupRecoveryOutcome`, what recovery actually did:

```text
Restored               → Resume
RetryReady             → Retry
ReconciliationRequired → Reconcile
Aborted                → Abort
HumanReviewRequired    → HumanReview
```

Verified by reading rather than by report: `expected_action` and
`RecoveryQualificationObservation::expected` appear nowhere in the application,
CLI or infrastructure crates. If recovery aborts something it should have
resumed, the observation records `Abort` and the evaluator reports an unsafe
action — which is the finding the mechanism exists to produce.

The migration header states the same thing where the columns are defined:
`action` sits beside `classification` on purpose, so that the two can disagree.

## Two observations and none are different facts

There is no uniqueness constraint on target and no deduplication on read. Zero
observations for a target and two conflicting ones are both failures the
evaluator already reports, and both are worth surfacing: a target nothing has
ever exercised and a target the system answered twice about are different
problems, and tidying either into silence would be the same mistake.

Observations are append-only by trigger, as fault-suite evidence has been since
0160. An observation that can be edited afterwards is not one.

## What the gate can now say

Every evidence family has a producer:

| family | producer | answer today |
|---|---|---|
| runtime qualification | `--runtime-evidence` | collected |
| crypto qualification | `--crypto-evidence` | collected |
| release approval | `--release-approval` | passes on a qualified release |
| capability restoration | `--capability-restoration` | decision per declared capability |
| fault suite | `--fault-suite-evidence` | `failed`, two failures |
| recovery qualification | `--recovery-qualification` | `failed`, naming targets never exercised |

Two days ago the gate answered every one of these with a word meaning *nobody
looked*. It now answers with what is known, and what is known is that the release
is not qualified — which is a far more useful thing for it to say.

## What this does not do

**The gate still cannot pass, and should not.** The fault suite reports two
failures. Most recovery targets will have no observation in any deployment,
because nothing exercises verifying repair, divergent history or orphan temporary
state — and the gate now names them instead of being silent about them.

**Startup recovery reaches only some targets.** The others need something to
drive them, and that harness is still unbuilt. What changed is that its absence
is now reported rather than indistinguishable from success.

**Nothing schedules any producer.** An operator runs them deliberately and hands
the gate what they produced.

**Three Trust-phase families remain unwired** — data governance, retention and
deletion, governed export and audit integrity — as the survey of 2026-08-22
recorded. Producers for the gate are not the same as the system doing the things
the gate asks about.
