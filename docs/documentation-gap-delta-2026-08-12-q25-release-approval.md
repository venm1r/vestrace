# Documentation Gap Delta — Q25 Trusted Release Approval

**Date:** 2026-08-12
**Scope:** aggregate release approval over exact qualification evidence
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `ReleaseApprovalService` evaluates the release contract as one fail-closed
  decision instead of treating individual green checks as a release claim.
- Approval requires a valid `RELEASE`/`TRUSTED` `QualificationBundle`, exact
  capability-manifest identity, passed bundle status, a qualified matching
  `QualificationBaseline`, `TrustState::Trusted`, and a passing
  `TrustedQualificationGate`.
- Both manifest and bundle signatures must be structurally valid, cryptographically
  verified by the existing signer/verifier boundary, and accepted by the exact
  `SignerTrustPolicy`.
- The bundle must publish at least one non-blank known limitation. All failures
  remain structured in `ReleaseApprovalDecision`; no partial score can approve a
  release.
- The service is pure application orchestration. It consumes crypto-verification
  evidence from the existing Ed25519 verifier; it does not claim KMS/HSM/Vault
  custody, provider adapter qualification, or deployment-wide runtime evidence.

## Evidence

- Focused release-approval tests: 3/3 passed.
- Coverage includes complete approval, untrusted/unsigned rejection, and exact
  scope rejection.
- Formatting and workspace verification remain required before commit.

## Explicit non-claims

This slice does not itself produce a passed v1.0 qualification bundle, execute a
live Docker deployment qualification, wire every provider adapter, or establish
permanent certification. A v1.0 claim still requires fresh evidence for the
exact build/configuration/environment and all roadmap exit criteria described by
ADR-0008.
