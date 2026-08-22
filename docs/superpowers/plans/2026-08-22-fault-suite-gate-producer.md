# Goal

Give the fault suite a producer, so the v1.0 release gate reports what is known
instead of reporting that nobody looked.

`run_release` passes `None` for the fault suite, with a comment saying nothing in
this build produces one, and the gate therefore emits `fault_suite_missing`. That
was true when it was written. It is no longer: the suite has a real subject, runs
against a real ephemeral deployment, and returns a verdict — currently
`passed=false failures=2`.

`FaultSuiteMissing` and `FaultSuiteFailed` are both failures and only one of them
is knowledge. Reporting the first when the second is available is the same
substitution this subsystem has spent ten slices refusing.

The machinery is already built and has never been connected:
`ExternalEffectFaultSuiteService`, `ExternalEffectFaultSuiteEvidence`,
`FaultSuiteEvidenceRepository`, `ExternalEffectFaultEvidenceAdmissionService` and
`ExternalEffectFaultGateEvidenceService` all exist with no production caller —
the same shape as `synthetic_unknown`, `supports_read_back` and the others this
subsystem keeps turning up.

# Requirements

1. A command that runs the suite through the configured
   `ProcessFaultInjectionRuntime`, persists an
   `ExternalEffectFaultSuiteEvidence` bound to the deployment target it observed,
   and prints the evidence id. It refuses to run outside a non-production
   isolation, which `FaultInjectionSettings` already enforces — this must stay
   enforced rather than re-implemented.
2. `conformance release` gains `--fault-suite-evidence <uuid>`. Given one, the
   gate reports the suite's verdict; given none, it still reports
   `fault_suite_missing`, because when nobody looked that is the honest answer.
3. **The decision is re-derived from the persisted observations**, through
   `evaluate_fault_suite`, not read from the stored `passed` flag. The
   observations are the evidence; the verdict is a derivation from them, and a
   verdict stored under an older evaluator must not outrank the contract as it
   stands now. If the evaluator has since tightened, evidence that once passed
   should stop passing.
4. Evidence naming a different target is refused rather than ignored, and the
   refusal names the mismatch. `ExternalEffectFaultGateEvidenceService` already
   does this; use it rather than repeating the check.
5. The command does not decide whether the release is acceptable. It records what
   the suite observed; the gate decides.

# Non-goals

- **Making the suite pass.** It reports two failures and this changes none of
  them. The gate will say `fault_suite_failed`, which is the point.
- **Wiring the suite into CI.** It needs Docker and aborts processes.
- **The other three producers** — release approval, recovery qualification,
  capability restoration. Each is its own evidence family.
- Startup recovery for effects, fault point 3, §18's rank 1, a minimum evidence
  strength, the `NOT VALID` exemption debt.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- The shipped image must still not build `vestrace-fault-scenario`. The runtime
  invokes a program by configured path; the CLI must not gain a dependency on
  that crate, and `crates/vestrace-fault-scenario/tests/` carries a guard
  asserting the `Dockerfile` never names it — that guard must still hold.
- The database URL must not arrive as a process argument. The scenario already
  refuses that; the command must not reintroduce it by another route.
- Evidence is written once and never updated. A fault-suite result that could be
  edited after the fact is not evidence.

# Acceptance criteria

1. The command runs the suite, persists evidence carrying all five observations
   and the target digest, and prints the id. Verified against a real ephemeral
   PostgreSQL 17 with the real scenario binary.
2. `conformance release --fault-suite-evidence <id>` reports `fault_suite_failed`
   rather than `fault_suite_missing`, given the evidence from criterion 1.
3. Without the flag the gate still reports `fault_suite_missing`.
4. Evidence whose target digest does not match the release target is refused,
   and the message says so.
5. Evidence whose stored `passed` is true but whose persisted observations do not
   satisfy `evaluate_fault_suite` is reported as failed. This is requirement 3
   and it must be tested by writing such a row directly, because it cannot arise
   from the current runner.
6. The command refuses to run when isolation is not ephemeral.
7. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
8. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
9. The `Dockerfile` guard still passes: the shipped image does not build the
   scenario crate.
10. The fault suite is run against a real ephemeral deployment and reported
    verbatim.

    **Prediction, to be verified.** No change: `failures=2`, points 1, 2, 4 and 5
    agreeing, point 3 red on both counts. This slice reports the verdict; it does
    not touch what produces it.

# Implementation plan

1. Application: a conversion from persisted observation evidence back to
   `FaultObservation`, so the gate can re-derive rather than trust.
2. CLI: the run-and-persist command, taking the scenario program path, the target
   digest and the isolation as arguments, since fault injection is not a
   deployment setting.
3. CLI: `--fault-suite-evidence` on `conformance release`, loading through the
   existing gate-evidence service and re-deriving the decision.
4. Run the whole path end to end against an ephemeral PostgreSQL 17: produce
   evidence, feed it to the gate, and record both outputs.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the release gate run twice — with and without the evidence — and both outputs
  recorded
