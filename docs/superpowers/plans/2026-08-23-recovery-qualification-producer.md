# Goal

Keep what recovery already observes, so the gate's last producer has something
true to read.

`run_release` passes `None` for recovery qualification and the gate reports
`RecoveryQualificationMissing`. Producing that evidence looked like it needed a
harness for eight recovery targets — work comparable to `vestrace-fault-scenario`.

It does not, or not all of it. `StartupRecoveryRecord` already carries
`target: RecoveryTarget` and `classification: RecoveryClassification`, and
startup recovery produces one per interrupted run on every restart. It is
**logged and thrown away**: no reference to the type exists in the infrastructure
crate at all.

So the system already classifies interrupted work by recovery target, on real
runs, in production. It just keeps no record of having done so.

# The shortcut this must refuse

`RecoveryQualificationObservation::expected(target, evidence_ref)` exists, and
filling an observation's `action` from `classification.expected_action()` would
make the evaluator's own check vacuous — it compares the observation's action to
that same expected action. An observation built that way agrees with the
evaluator by construction and says nothing about what happened.

The action must come from `StartupRecoveryOutcome`: what recovery actually did.
If recovery aborted something it should have resumed, the observation records
`Abort`, the evaluator reports "unsafe action", and that is the finding the whole
mechanism exists to produce. `FaultObservation::expected` was the same trap in
the fault suite and the delta of 2026-08-20 is about refusing it.

# Requirements

1. A durable record of each recovery observation: target, classification, the
   action taken, when, and what it was about.
2. `StartupRecoveryService` records one per run it recovers. The action is
   derived from the outcome — `Restored → Resume`, `RetryReady → Retry`,
   `ReconciliationRequired → Reconcile`, `Aborted → Abort`,
   `HumanReviewRequired → HumanReview` — never from the classification.
3. `conformance release --recovery-qualification` reads the observations and
   evaluates them through `evaluate_recovery_qualification`. Without the flag the
   gate still reports `RecoveryQualificationMissing`.
4. A target with no observation is **not** filled in. `evaluate_recovery_qualification`
   already reports "must have exactly one observation" for it, and the report
   must name which targets those are — a deployment that has never exercised
   divergent history should be told so, not have it assumed.
5. More than one observation for a target is also a failure the evaluator already
   produces. The producer passes what is stored and does not deduplicate: two
   different answers about one target is a fact worth surfacing, not tidying.

# Non-goals

- **A harness for the targets startup recovery does not reach.** Verifying
  repair, divergent history, orphan temporary state and the rest will have no
  observations until something exercises them, and this slice makes that visible
  rather than fixing it.
- **Making the gate pass.** It cannot: the fault suite answers `failures=2` and
  most recovery targets will have no observation.
- Driving recovery differently — this records what it already does.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- Do not weaken `evaluate_recovery_qualification`.
- Observations are append-only: an observation that can be edited afterwards is
  not one. Enforce in the database, as 0160 did for fault-suite evidence.
- The recording must not change what recovery *does*. It records; it does not
  decide.

# Acceptance criteria

1. A startup recovery that restores a run records an observation whose target and
   classification are the record's, and whose action is `Resume` because the
   outcome was `Restored`.
2. An outcome of `Aborted` for a target classified `SafeToResume` records action
   `Abort`, and `evaluate_recovery_qualification` reports an unsafe action for
   it. This is the case the mechanism exists for and it must be tested
   explicitly — a producer that can only record agreement is not a producer.
3. With the flag, the gate reports `RecoveryQualificationFailed` and the report
   names every target with no observation.
4. Without the flag, `RecoveryQualificationMissing`, unchanged.
5. Raw SQL updating a recorded observation is refused.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
7. Conformance gate unchanged: 199 passed, 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** No change: `failures=2`.

# Implementation plan

1. Migration `0162`: the observations table, append-only by trigger.
2. Port and adapter: record an observation, list them.
3. `StartupRecoveryService`: record one per recovered run, action from outcome.
4. CLI: `--recovery-qualification`, reading and evaluating, naming targets with
   no observation.
5. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
