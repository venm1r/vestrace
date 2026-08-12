# Documentation Gap Delta — Q9 Key Provider and Signer Trust

**Date:** 2026-08-12
**Scope:** provider-backed signing boundary and signer trust policy after Q8
**Authority:** Crypto & Data Governance Contract, Qualification / Conformance Specification, v0.2 → v1.0 roadmap, and the 36-PR execution matrix.

## Implemented bounded contract

- `SignerTrustRule` matches the complete signer identity, provider, key id, key version, key scope, and algorithm metadata.
- `SignerTrustPolicy` returns structured fail-closed decisions and rejects unusable/revoked keys, non-signing purposes, algorithm drift, and missing exact allowlist rules.
- CLI signing resolves private-key bytes through the domain `KeyProvider` port and `ResolvedKeyMaterial`; the local-file adapter rejects provider names it does not implement.
- `conformance verify-signature` and `conformance verify` distinguish cryptographic validity from trusted-signer status; an explicit `--require-trusted-signer` failure is non-zero.
- Trust policy and signature output contain metadata/status only; private key bytes are never serialized into artifacts or policy state.

## Evidence

- domain tests in `crates/vestrace-domain/src/trust.rs`;
- CLI tests in `crates/vestrace-cli/tests/q9_key_provider_trust_cli.rs`;
- provider boundary in `crates/vestrace-cli/src/commands/conformance.rs`;
- policy flags in `crates/vestrace-cli/src/main.rs`;
- implementation plan in `docs/superpowers/plans/2026-08-12-q9-key-provider-signer-trust.md`.

## Explicit non-claims

This slice does not implement KMS, HSM, Vault, OS-keyring, remote signing, key rotation execution, durable signer-policy repositories, incident/revalidation orchestration, or release approval. The local-file adapter is an explicit local/dev boundary and is not production key custody. A cryptographically valid signature without a configured trusted-signer policy remains `not_requested`, not a v1.0 trust certificate.

## Verification recorded for this slice

- signer trust domain tests: 3 passed;
- Q9 CLI provider/trust tests: 3 passed;
- full workspace and Docker/runtime verification remain required before any v1.0 completion claim.
