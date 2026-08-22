# Goal

Give the release gate its third producer: capability restoration.

`run_release` passes an empty vector for capability restoration, so the gate
reports `CapabilityRestorationMissing`. `CapabilityRestorationService::evaluate`
needs four things, and three of them already exist:

- the trust state — persisted, and the release approval producer already reads it;
- a restoration policy mapping capability to stage — **no source today**;
- restoration evidence — `qualification_passed`, `revalidation_passed` and
  evidence references;
- the capability being asked about.

# The failure mode to avoid

`RestorationEvidence` carries two `bool`s, exactly like the signature evidence of
the previous slice, and exactly as easy to assert. `qualification_passed` must
come from the bundle's own status and `revalidation_passed` from a persisted
`RevalidationRun` — never from a literal, and never from the absence of a record
read as success.

An absent revalidation run means *we do not know that revalidation passed*, which
is not the same as it having failed and is certainly not the same as it having
passed. It must produce a blocked decision naming what was missing.

# Requirements

1. The restoration policy comes from configuration — `policy.capabilities`
   already establishes that capabilities are configured, so a stage per
   capability follows the same shape. A deployment that configures none cannot
   have this evidence collected, and the gate keeps saying `missing`.
2. `conformance release --capability-restoration` collects a decision **per
   capability the policy declares**. Without the flag the gate still reports
   `CapabilityRestorationMissing`.
3. `qualification_passed` is read from the bundle under release.
   `revalidation_passed` is read from a persisted `RevalidationRun` for the
   scope. Neither is a literal.
4. A capability with no stage in the policy is `CapabilityNotDeclared` — which
   the service already produces — rather than skipped. A capability nobody
   declared a stage for is not a capability that quietly restores.
5. An absent revalidation run produces a blocked decision naming the scope it
   looked in, not an evidence of `false` that reads as a failed revalidation.
   "We could not tell" and "it failed" are different, and this subsystem has
   spent a day keeping such pairs apart.

# Non-goals

- **Recovery qualification**, the last missing producer, which needs observations
  from eight recovery targets and therefore a harness of its own.
- **Making the gate pass.** The fault suite still answers `failures=2`.
- Driving restoration — nothing here restores a capability; it reports what the
  policy and the evidence say about restoring one.
- T4, T5, T6 — the three unwired Trust families the survey recorded.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- Do not weaken `CapabilityRestorationService`. If a check cannot be fed
  honestly, report that rather than feeding it something that passes.
- The report must name which capability was blocked and why, as the previous
  slice made the approval's reasons visible. One word for many capabilities and
  many stages is the message that costs an afternoon.

# Acceptance criteria

1. With the flag and a configured policy, the gate reports a decision per
   declared capability, and the report names any that are blocked with their
   stage and reason.
2. Without the flag, `CapabilityRestorationMissing`, unchanged.
3. With the flag and no configured policy, the run is refused with a message
   saying so — not silently treated as no capabilities.
4. `revalidation_passed` reflects a persisted `RevalidationRun`: a test with a
   passing run and one with none produce different decisions, and the one with
   none names the scope.
5. A capability outside the policy is reported `CapabilityNotDeclared`.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
7. Conformance gate unchanged: 199 passed, 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** No change: `failures=2`. This slice touches
   the release gate, not the scenario.

# Implementation plan

1. Configuration: a restoration stage per capability, validated like
   `policy.capabilities` is.
2. CLI: `--capability-restoration`, reading the trust state and the revalidation
   run, deriving evidence from the bundle and that run, and evaluating per
   declared capability.
3. Report: name blocked capabilities with stage and reason.
4. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the release gate run with and without `--capability-restoration`, both recorded
