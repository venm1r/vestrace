# Q27 Crypto/Provider Adapter Qualification

## Goal

Prevent a structurally valid signature or local development key from being
treated as production crypto/provider qualification.

## Authority

- `docs/specs/vestrace-crypto-data-governance-contract-v0.2.md`, §§6, 10–13
- `docs/specs/vestrace-normative-invariants-v0.2.md`, `GOV-005..009`, `GOV-023`
- `docs/specs/vestrace-normative-invariants-v0.2.md`, `QUAL-011`, `QUAL-014`
- Existing `KeyReference`, `KeyProvider`, `SignerTrustPolicy`, and signature
  verification contracts.

## Design

1. Declare provider, custody, algorithm, purpose, and exact scope in a typed
   qualification target.
2. Reject `LocalDevelopment` custody for a production decision.
3. Compare observed key metadata against the target without normalization that
   could hide drift.
4. Require typed observations for resolution, isolation, cryptographic
   round-trip, lifecycle, rotation, non-disclosure, and unique evidence refs.
5. Expose a probe seam for real backend collection without pretending the pure
   evaluator is a live KMS/HSM/Vault implementation.
6. Keep backend implementation and live infrastructure qualification separate
   from this pure decision boundary.

## Verification gate

- RED test before implementation.
- Focused crypto qualification tests.
- Formatting, workspace tests, no-run compilation, scoped diff check.
- Independent review before commit.
