# Q9 Key Provider and Signer Trust Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicit provider-resolution boundary and signer trust policy so cryptographic validity is distinct from trusted release/deployment evidence.

**Architecture:** Keep secret/key bytes behind the existing domain `KeyProvider` port and make the CLI local-file implementation an explicit adapter. Add a domain-owned allowlist policy that matches signer identity, provider, key identity/version/scope, algorithm, purpose, and lifecycle; expose policy results as evidence in signature verification without treating a valid signature as authorization.

**Tech Stack:** Rust 2024, serde, ring Ed25519, clap, existing Vestrace domain/application/CLI crates.

## Global Constraints

- Secret material MUST NOT be persisted in artifacts, logs, or policy state.
- A valid cryptographic signature is not sufficient to establish trust or permission.
- Revoked, retired, destroyed, or otherwise unusable signing keys MUST fail closed.
- Existing unsigned/local explicit command behavior remains compatible unless the new trust policy is requested.
- Preserve unrelated dirty worktree changes; do not stage, commit, reset, or clean.

### Task 1: RED tests for signer trust policy and provider boundary

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Modify: `crates/vestrace-cli/src/main.rs`

**Interfaces:**
- Produces `SignerTrustPolicy`, `SignerTrustDecision`, and provider-backed signing helpers used by later tasks.

- [x] Add domain tests proving matching signer/key metadata passes, mismatched issuer/provider/key/version/scope/algorithm fails, and revoked keys fail closed.
- [x] Add CLI tests proving signature verification reports cryptographic validity separately from trusted policy status and local provider resolution rejects unsupported providers.
- [x] Run focused tests and record the expected RED failures before implementation.

### Task 2: Implement domain signer trust policy

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs`

**Interfaces:**
- `SignerTrustPolicy::new(...) -> Result<Self, DomainError>`
- `SignerTrustPolicy::evaluate(&self, signature: &SignatureRecord) -> SignerTrustDecision`
- `SignerTrustDecision::is_trusted()` and structured failure accessors.

- [x] Implement exact allowlist matching and lifecycle/purpose checks.
- [x] Keep policy serializable and free of secret/key bytes.
- [x] Run domain focused tests GREEN.

### Task 3: Implement provider-backed local adapter and CLI policy wiring

**Files:**
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Modify: `crates/vestrace-cli/src/main.rs`

**Interfaces:**
- `LocalFileKeyProvider` implements the existing `KeyProvider` port.
- `run_sign` resolves key bytes through the provider boundary.
- `run_verify_signature` accepts optional exact trust-policy arguments and emits `cryptographic_status` plus `trust_status`.

- [x] Reject provider names other than the explicit `local-file` adapter in this slice.
- [x] Use `SecretResolutionRequest` and `ResolvedKeyMaterial` for ephemeral resolution.
- [x] Preserve the existing default cryptographic verification mode when no trust policy is supplied.
- [x] Run CLI focused tests GREEN.

### Task 4: Documentation and verification

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q9-key-provider-signer-trust.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/superpowers/plans/2026-08-12-q9-key-provider-signer-trust.md`

- [x] Document implemented boundary, evidence, and explicit non-claims for real KMS/HSM/Vault adapters and release approval.
- [x] Run workspace library tests, CLI tests, compile, rustfmt, and `git diff --check`.
- [x] Inspect Docker state read-only and do not replace running services unless a focused runtime check is required.
