# Q25 Trusted Release Approval

## Goal

Close the application-level release decision boundary without converting a
feature count or an individual passing check into a v1.0 claim.

## Authority

- `docs/adr/0008-v1-trust-is-a-qualification-contract.md`
- `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`
- `docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md`
- Existing `QualificationBundle`, `QualificationBaseline`, `TrustedQualificationGate`,
  `VestraceCapabilityManifest`, and `SignerTrustPolicy` domain contracts.

## Design

1. Add a pure `ReleaseApprovalService` application boundary.
2. Require exact `RELEASE`/`TRUSTED` manifest-bound bundle identity and passed status.
3. Require a qualified baseline matching the bundle and `TrustState::Trusted` for
   the exact requested health scope.
4. Re-run the full Trusted hard gate and require non-blank published limitations.
5. Require structurally valid signatures, upstream cryptographic verification for
   both artifacts, and exact signer-policy trust.
6. Return all failures as structured data and approve only when the list is empty.

## Verification gate

- RED focused test before implementation.
- Focused release-approval integration tests.
- `cargo fmt -- --check`.
- Workspace library tests and no-run compilation.
- Scoped diff check and independent review.
- Docker configuration/runtime evidence where available; any unavailable live
  evidence remains explicitly blocked rather than implied by local tests.
