# Recovery qualification can now be driven

**Date:** 2026-08-25  
**Scope:** the operator-driven producer for the eight recovery qualification
targets.  
**Status:** working-tree delta. It closes the recovery evidence leg in an
ephemeral qualification environment; it does not qualify v1.0 or change the
fault-suite verdict.

## What changed

`conformance release --recovery-qualification` could already evaluate every
stored observation, but production startup recovery could observe only the
targets its candidate source happened to discover. Several required targets,
including `VerifyingRepair`, `OrphanTemporaryState`, and `DivergentHistory`, had
no driver. A release environment therefore had no honest way to produce the
complete eight-target set.

The CLI now exposes:

```text
vestrace conformance recovery-qualification \
  --workspace-id <uuid> \
  --principal-id <uuid> \
  --isolation ephemeral
```

The command reads its database credential from configuration, migrates the
disposable database, and refuses to start unless the deployment-wide
`recovery_qualification_observations` set is empty. It creates one canonical,
event-backed run for each required target and submits all eight candidates to
the existing `StartupRecoveryService`. Safe resume and retry paths therefore
rebuild real projections from their canonical event streams. The other paths
produce the barrier outcome the recovery service actually chose.

Evidence is still written by the existing producer. In particular, the action
comes from `StartupRecoveryOutcome`; the scenario never calls
`RecoveryQualificationObservation::expected` and never derives the persisted
action from `classification.expected_action()`. The append-only table remains
unchanged, all stored observations are passed to the evaluator, and a repeated
command is refused rather than deleting or deduplicating the first run.

## What was verified

- The CLI contract exposes only `ephemeral` isolation and accepts no database
  URL argument; connection failures redact configured credentials.
- A PostgreSQL-backed command run created eight `run_streams` and exactly eight
  observations with the independently asserted target/classification/action
  mappings.
- Repeating the command failed before creating another stream or observation;
  all first-run evidence remained present.
- The release command consuming that database reported neither
  `recovery_qualification_missing` nor `recovery_qualification_failed`.
  Other absent release evidence still failed the overall release decision.
- Focused results: recovery CLI `4 passed`; recovery release-gate cases
  `3 passed`; startup recovery `12 passed`; append-only repository `2 passed`.
- `cargo test --workspace -- --test-threads=1` exited zero against PostgreSQL
  17. Ignored LM Studio, Compose, and destructive fault tests were not part of
  that command.
- `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` passed.
- TRUSTED conformance remained `199/199`: 190 executed, 1 build-verified, and
  8 attested.

## The boundary this does not cross

This is a qualification scenario for recovery behavior, not proof that every
failure mode can be discovered from live deployment state. The scenario names
each `RecoveryTarget` as an input and observes what the real recovery service
does with it. Only the existing production candidate source proves discovery
of stale leases and unknown interrupted runs. No production detector was added
for divergent history, an interrupted repair, or orphan temporary state.

The fault suite was not changed or re-run in this slice. Its two known failures
remain a separate v1.0 blocker. A passed recovery leg therefore does not make
the release gate pass.
