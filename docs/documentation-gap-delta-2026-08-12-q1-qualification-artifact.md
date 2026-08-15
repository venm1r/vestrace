# Documentation Gap Delta — Q1 Qualification Artifact

**Date:** 2026-08-12
**Scope:** target-bound machine-readable qualification artifact

**Authority:** Qualification / Conformance Specification §§20–24, 25.6, 31–34; Trust T1–T8 contract; existing conformance registry and `TrustedQualificationGate`.

## Implemented bounded contract

- `QualificationBundle` now records qualification lifecycle (`PRE_MERGE`, `RELEASE`, `DEPLOYMENT`, `PERIODIC`, `POST_INCIDENT`) and the complete profile-scoped `ConformanceReport`.
- Report-backed bundle construction validates profile equality, requires every profile requirement to have a result, preserves skipped/failed results, and exposes `PASSED`, `FAILED`, or `INCOMPLETE` status.
- `vestrace conformance bundle` writes a pretty JSON artifact containing target identity, profile, lifecycle, conformance results, known limitations, evidence, and target digest.
- The command writes the artifact before returning a non-zero result for a failed/incomplete profile, so a failed qualification cannot be mistaken for missing evidence or a successful release claim.

## Evidence

- Domain tests: `tests/q1_qualification_artifact.rs`;
- CLI integration test: `crates/vestrace-cli/tests/q1_qualification_cli.rs`;
- command path: `crates/vestrace-cli/src/commands/conformance.rs`;
- bundle model: `crates/vestrace-domain/src/trust.rs`;
- plan: `docs/superpowers/plans/2026-08-12-q1-qualification-artifact.md`.

Example:

```text
vestrace conformance bundle --profile trusted --lifecycle release \
  --output artifacts/qualification.json \
  --target-manifest manifest://target \
  --source-revision <revision> \
  --build-digest <build-digest> \
  --configuration-digest <configuration-digest> \
  --environment-manifest environment://deployment \
  --suite-version suite-v1
```

## Explicit non-claims

This slice does not add durable qualification repositories, migrations, signed bundles, authoritative deployment attestation, runtime fault/recovery execution, or a passing `TRUSTED`/v1.0 profile. The current deterministic CLI evaluator still exposes skipped requirements; the emitted artifact records that failure and must not be used as a release certificate.

## Verification

- focused domain suite: 5 passed;
- focused CLI suite: 1 passed;
- workspace/full verification remains required before completion claims;
- existing unrelated warnings and dirty worktree changes remain outside this delta.
