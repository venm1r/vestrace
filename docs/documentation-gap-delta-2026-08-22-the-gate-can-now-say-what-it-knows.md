# The gate can now say what it knows

**Date:** 2026-08-22
**Scope:** the v1.0 release gate's first evidence producer.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim — the gate still fails, and now it fails for a reason it can
state.

```text
release gate, with fault-suite evidence:    fault_suite_failed
release gate, without it:                   fault_suite_missing
fault suite (real ephemeral deployment):    FAILED — 2 failures, byte-identical to the previous run, as predicted
conformance gate:                           199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
```

## The gap

`run_release` passed `None` for the fault suite, with a comment saying nothing in
this build produces one, so the gate emitted `fault_suite_missing`. That was true
when it was written. It stopped being true when the suite got a real subject: it
runs against a real ephemeral deployment and returns a verdict.

`FaultSuiteMissing` and `FaultSuiteFailed` are both failures and only one of them
is knowledge. Reporting the first where the second is available is a gate lying
about itself rather than about the system.

Everything needed was already built and had never been connected:
`ExternalEffectFaultSuiteService`, `ExternalEffectFaultSuiteEvidence`,
`FaultSuiteEvidenceRepository`, the admission service and the gate-evidence
service — five components with no production caller, the same shape as
`synthetic_unknown`, `supports_read_back`, the deadline-less transition count and
the table migration 0024's filename promised.

## What changed

`vestrace conformance fault-suite` runs the suite through the configured process
runtime, persists the evidence bound to the target it observed, and prints its
id. `vestrace conformance release --fault-suite-evidence <uuid>` reports the
verdict; without the flag the gate still says `fault_suite_missing`, because when
nobody looked that is the honest answer.

**The verdict is re-derived from the persisted observations**, through
`evaluate_fault_suite`, rather than read from the stored `passed` flag. The
observations are the evidence; the verdict is a derivation from them. A verdict
recorded under an older, laxer evaluator must not outrank the contract as it
stands — so evidence that once passed stops passing when the evaluator tightens.
A test writes exactly that row directly, since the runner cannot produce one.

Migration 0160 makes the evidence append-only with triggers rather than
convention: a fault-suite result that can be edited after the fact is not
evidence. Its header notes that a stored verdict remains available for historical
inspection while never becoming the source of truth.

Two constraints held by construction rather than by care. The CLI gained **no**
dependency on `vestrace-fault-scenario` — `cargo tree -p vestrace-cli` does not
contain it, the runtime invokes an operator-supplied program path, and the guard
asserting the `Dockerfile` never names that crate still passes, so a program
whose whole purpose is `std::process::abort()` stays out of the shipped image.
And the database URL reaches the scenario only as `--database-url-file <path>`,
never in `argv`, with the argument builder regression-tested.

## The defect that surfaced on first use

The stale-verdict test failed, and not for the reason it was testing.
`PgFaultSuiteEvidenceRepository::decode` compared `evidence.created_at()` to the
stored column exactly. PostgreSQL stores `TIMESTAMPTZ` at microsecond precision
and **rounds**; chrono carries nanoseconds. So the check rejected any evidence
whose creation time had a sub-microsecond remainder — nearly all of it.

That code has been there since the evidence store was written, fully tested, and
never once exercised against a real database in production, because nothing ever
read or wrote this table. It would have rejected the first genuine fault-suite
evidence any deployment produced.

This is the same defect fixed hours earlier in the authorization read-back check,
in a different repository, found the same way. Two occurrences make it worth
stating as a rule: **a projection compared to a payload must be compared at the
column's precision**, and where a column rounds, exactness is a bug.

It is also the clearest illustration of something this session has now met eight
times. An affordance that is written and never called does not have no defects —
it has undiscovered ones. "Built and ready" and "known to work" are different
claims, and only the second survives a caller.

## Verification

Run here against a PostgreSQL 17 with pgvector.

The headline result is covered by tests rather than by a one-off run, which is
stronger: `v1_release_gate_cli` asserts that with evidence the gate reports
`fault_suite_failed` and does **not** report `fault_suite_missing`, and that
without it the reverse holds. Seven of seven passed.

The fault suite itself is byte-identical, as predicted — this slice reports the
verdict and does not touch what produces it:

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

**The release gate still cannot pass, and this brings it no closer to passing.**
It brings it closer to being *truthful*. Three evidence families remain without
producers — release approval, recovery qualification, capability restoration —
and the fault suite it can now hear from reports two failures.

**The suite still does not run in CI.** It needs Docker and aborts five
processes.

**Nothing schedules the producer.** An operator runs it deliberately, against a
deployment provisioned for the purpose, and hands the gate the resulting id.

**Fault point 3 is still red**, and a previous delta established that no
mechanism closes it: its expectation requires the system to have enrolled an
effect in reconciliation and still not know the outcome, which cannot hold
against a stub that always answers. That is a question about the expectation, and
changing the judge needs its own argument.
