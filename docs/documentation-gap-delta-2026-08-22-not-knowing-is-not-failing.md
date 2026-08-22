# Not knowing is not failing

**Date:** 2026-08-22
**Scope:** the release gate's third evidence producer, and the distinction it
turns on.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. Three of the gate's four missing producers now exist.

```text
release gate, with --capability-restoration: a decision per declared capability, blocked ones named
release gate, without the flag:              capability_restoration_missing
fault suite:                                 FAILED — 2 failures, unchanged
conformance gate:                            199 passed, 0 failed, 0 skipped — unchanged
```

## What was built

`run_release` passed an empty vector for capability restoration, so the gate
reported `CapabilityRestorationMissing`. Three of the four inputs
`CapabilityRestorationService::evaluate` needs already existed; the restoration
policy had no source.

Stages now come from `[policy.capability_restoration]`, following
`policy.capabilities`, which already established that capabilities are
configured. Every capability the deployment configures gets a decision. One
configured without a stage is `CapabilityNotDeclared` — the service's own
answer — rather than skipped, because a capability nobody declared a stage for is
not a capability that quietly restores. Asking for the flag with no policy at all
is refused outright rather than read as "no capabilities".

## The distinction the slice turns on

`RestorationEvidence` carries two booleans, exactly like the signature evidence
of the slice before it and exactly as easy to assert. They are produced from
records:

```rust
qualification_passed = bundle.status() == QualificationStatus::Passed;
revalidation_passed  = revalidation_run.is_successful();
```

The revalidation run is loaded through the persisted trust state's
`revalidation_run_id`. And when that record is absent, **no `RestorationEvidence`
is constructed at all**: the decision is blocked as `RevalidationEvidenceMissing`,
naming the workspace and the run id it looked for.

That is the point. Writing `revalidation_passed: false` would have been an
assertion that revalidation was observed to fail. Nobody observed anything. "We
do not know" and "it failed" are different facts, and only one of them is true
here.

This pair has now come up five times in two days, and each time the convenient
collapse would have been a lie in the direction that passes or the direction that
looks rigorous:

- `UnreconciledEffect` versus `ReconciliationOutcome::Inconclusive` — we could not
  ask, versus we asked and were not told;
- a worker with no presence record versus one whose heartbeat lapsed — no
  evidence, versus evidence of death;
- `fault_suite_missing` versus `fault_suite_failed` — nobody looked, versus we
  looked and it failed;
- an absent baseline reported as a failed approval rather than a missing
  producer — we looked and there is nothing;
- and now an absent revalidation run.

A completely absent trust state is treated the same way: collection fails,
because no trust value can be supplied honestly. Taking a default there would
have produced a passing path out of nothing.

## The proof that the booleans are real

The implementation replaced the revalidation read with a literal `true`,
confirmed the new test failed, and restored it.

That is the only kind of evidence that settles this question. A test that passes
proves the code compiles and returns something; a test that fails when the value
is faked proves the value comes from where it claims. Nothing in the type system
distinguishes a read `true` from a written one, so the distinction has to be
demonstrated by breaking it.

## Verification

Run here against a PostgreSQL 17 with pgvector, including the three new CLI
acceptance tests the implementing agent could compile but not execute. Only the
pre-existing `authorized_shared_reader_uses_exact_revision_under_forced_rls`
fails. `cargo fmt` clean, conformance unchanged at 199/199, fault suite unchanged
at `failures=2` as predicted — this slice touches the gate, not the scenario.

## What this does not do

**The gate still cannot pass.** One producer remains absent: recovery
qualification, which needs observations for eight recovery targets and therefore
a harness comparable to `vestrace-fault-scenario`. And the fault suite, which the
gate can now hear from, answers `failures=2`.

**Nothing restores anything.** This reports what the policy and the evidence say
about restoring a capability; it does not restore one.

**The scope is supplied by the operator** (`--trust-workspace-id`), not derived.
Deriving it would mean inventing a rule about which record a release reads, and
inventing one quietly is how every release ends up reading the same row.

**Stages are configuration, not policy history.** Which stage a capability sits
at is stated by the deployment, and nothing records how it came to be stated or
when it last changed.
