# Q8 Automatic Server/Worker Qualification

**Date:** 2026-08-12
**Status:** implemented and verified
**Scope:** automatic deployment qualification at `server` and `worker` startup

## Authority and boundaries

- Follow `docs/specs/vestrace-qualification-conformance-spec-v0.2.md` sections 2–4, 8, 20–22, 29–31.
- Follow `docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md` sections 7–10.
- Continue the Q1–Q7 bounded qualification slices without claiming a passing profile or v1.0.
- Preserve Q7 local Ed25519 verification as an optional separate gate; Q8 does not add KMS/HSM/Vault resolution or signer trust policy.
- Keep fault injection, crash/recovery execution, post-incident requalification, release approval, and permanent certification out of scope.

## Goal

When explicitly enabled by configuration, both the HTTP server and background worker must:

1. load the target capability manifest;
2. run the existing deterministic conformance evaluator for the configured profile;
3. collect live PostgreSQL migration/runtime-role evidence through the existing read-only path;
4. build and persist a target-bound `QualificationBundle` including the runtime result;
5. write a machine-readable startup qualification artifact; and
6. fail closed before accepting work when any conformance or deployment check fails.

The default configuration remains disabled, so existing local startup behavior is unchanged.

## Implementation steps

### 1. Configuration and shared decision model

- [x] Add an explicit `qualification.enabled` configuration section with manifest, profile, deployment lifecycle, suite version, and output artifact path.
- [x] Add a shared runtime role/evidence/decision model in the application boundary so Q6 verification and Q8 startup wiring use the same checks.
- [x] Do not copy database secrets or connection strings into evidence.

### 2. RED tests

- [x] Test runtime qualification passes only with exact manifest identity, passed conformance, compatible migrations, and a restricted runtime role.
- [x] Test superuser, RLS bypass, bootstrap inheritance, migration incompatibility, and failed conformance produce a failed decision.
- [x] Test automatic startup artifacts are written before a non-zero result and failed bundles are persisted through the repository port.
- [x] Test server/worker startup invokes the shared qualification hook only when enabled.

### 3. Runtime wiring

- [x] Implement one automatic runner used by both `server` and `worker` after migration compatibility is established and before binding/polling.
- [x] Persist failed evidence as well as passed evidence, then return an error for failed qualification.
- [x] Keep output deterministic and machine-readable, with component, target digest, bundle id/status, runtime checks, and the complete bundle.

### 4. Documentation and verification

- [x] Add Q8 gap delta and update `current-implementation.md` and `documentation-status-v0.2.md`.
- [x] Run focused RED→GREEN tests, workspace checks, CLI integration tests, rustfmt, and `git diff --check`.
- [x] Record live Docker evidence separately; do not convert a local or unit checkpoint into a v1.0 claim.

## Exit criteria

- [x] `server` and `worker` share the same enabled qualification path.
- [x] Failed startup qualification produces durable evidence and prevents service readiness/work polling.
- [x] Disabled qualification does not alter existing startup behavior.
- [x] Tests cover the security-critical runtime-role and identity checks.
- [x] Documentation states Q8's exact claims and remaining non-claims.
