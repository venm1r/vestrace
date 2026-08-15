# Signed Qualification and Manifest Artifacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the stringly-typed qualification signature placeholder with a validated `SignatureRecord`, attach it to capability manifests and qualification bundles, and provide a real Ed25519 sign/verify CLI path without treating a hash as a signature.

**Architecture:** The domain owns serializable signature metadata, existing `KeyReference` lifecycle/purpose semantics, canonical unsigned payloads, object digests, and attachment validation. The CLI owns the local file-backed Ed25519 adapter for this bounded gate; it signs metadata plus the unsigned object payload and verifies against an explicit public-key file. KMS/HSM/Vault resolution remains behind the existing `KeyProvider` boundary and is not claimed as implemented by this gate.

**Tech Stack:** Rust 2024, serde/serde_json, SHA-256, `ring` 0.17 Ed25519, base64 0.22, clap, Cargo workspace tests.

## Global Constraints

- A signature proves authenticity/integrity attribution; a hash proves content integrity/addressing and is not a substitute for a signature.
- Persisted crypto metadata records algorithm identifier/version and key version; verification uses the historical key reference recorded in the artifact.
- Key material is read from an explicit local adapter for this gate and is never serialized into domain artifacts or ordinary Vestrace memory/DB payloads.
- Capability-manifest and qualification identity digests exclude the optional signature field so attaching/removing a signature does not change the target identity.
- Existing unsigned Q1-Q6 artifacts remain readable; signature verification is an additional configured check, not an implicit claim that an unsigned artifact is trusted.
- Preserve unrelated dirty worktree changes, Docker volumes, generated evidence, and avoid staging/committing.

---

### Task 1: Domain signature contract and canonical payloads

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs`
- Modify: `crates/vestrace-domain/src/release/mod.rs`
- Test: `crates/vestrace-domain/src/trust.rs` module tests and `crates/vestrace-domain/src/release/mod.rs` module tests

**Interfaces:**
- Consumes: existing `KeyReference`, `KeyPurpose::Signing`, `KeyLifecycleState`, `QualificationBundle`, and `VestraceCapabilityManifest`.
- Produces: `SignatureAlgorithm::Ed25519`, `SignatureRecord::new`, `SignatureRecord::validate_for`, `QualificationBundle::{signature, unsigned_signing_payload, unsigned_signing_digest, attach_signature}`, and matching manifest methods.

- [x] **Step 1: Write the failing tests**

  Add tests that construct a signing `KeyReference` and assert:

  ```rust
  let record = SignatureRecord::new(
      bundle.unsigned_signing_digest().unwrap(),
      "issuer://release",
      key_reference,
      SignatureAlgorithm::Ed25519,
      "base64-signature",
      now(),
  ).unwrap();
  let signed = bundle.attach_signature(record).unwrap();
  assert!(signed.signature().is_some());
  assert_eq!(signed.unsigned_signing_digest().unwrap(), bundle.unsigned_signing_digest().unwrap());
  ```

  Add negative tests for a wrong object digest, a non-signing key purpose, an empty signer identity, and a tampered signed object. Repeat the attachment/digest assertions for `VestraceCapabilityManifest` and assert that a serialized manifest without `signature` still loads.

- [x] **Step 2: Run the focused tests to verify RED**

  Run: `cargo test -p vestrace-domain trust::tests::signed_qualification -- --nocapture` and `cargo test -p vestrace-domain release::tests::signed_manifest -- --nocapture`

  Expected: FAIL because `SignatureRecord`, signature fields, and canonical signing methods do not exist yet.

- [x] **Step 3: Implement the minimal domain contract**

  Add `SignatureAlgorithm` and `SignatureRecord` with serialized fields `object_digest`, `signer_identity`, `key_ref`, `algorithm`, `signature`, and `signed_at`. Validate non-blank metadata, signing purpose, and algorithm-suite agreement. Add `KeyReference` accessors needed by the validator.

  Change `QualificationBundle.signature` from `Option<String>` to `Option<SignatureRecord>`. Add `#[serde(default)] signature: Option<SignatureRecord>` to `VestraceCapabilityManifest`. Implement unsigned serialization by cloning the object and clearing its optional signature, then compute `sha256:<hex>` over those bytes. `attach_signature` must reject any record whose `object_digest` does not equal the unsigned digest. Serialize a signed payload containing the unsigned object plus all signature metadata except the signature bytes, so signer/key metadata cannot be rewritten without invalidating the Ed25519 signature.

- [x] **Step 4: Run the focused tests to verify GREEN**

  Run: `cargo test -p vestrace-domain trust::tests::signed_qualification release::tests::signed_manifest -- --nocapture`

  Expected: PASS, including tamper rejection and backwards-compatible unsigned JSON loading.

### Task 2: CLI Ed25519 local adapter and artifact commands

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/vestrace-cli/Cargo.toml`
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Test: `crates/vestrace-cli/tests/q7_signed_artifacts_cli.rs`

**Interfaces:**
- Consumes: domain `SignatureRecord`, canonical signed payload methods, and manifest/bundle JSON artifacts.
- Produces: `conformance sign --artifact manifest|bundle ...` and `conformance verify ... --public-key-file ... --require-signature` with nonzero exits for malformed keys, missing required signatures, metadata mismatch, and Ed25519 verification failure.

- [x] **Step 1: Write the failing CLI tests**

  Add an integration test that generates a temporary Ed25519 PKCS#8 key with `ring`, writes its private and public keys, invokes `conformance sign` for a manifest and a qualification bundle, then invokes `conformance verify` and asserts a machine-readable `signature: passed` check. Add negative cases that mutate the artifact after signing and replace the public key, asserting nonzero exit and no successful verification output.

- [x] **Step 2: Run the focused CLI tests to verify RED**

  Run: `cargo test -p vestrace-cli --test q7_signed_artifacts_cli -- --nocapture`

  Expected: FAIL because the sign subcommand, public-key options, and signature check do not exist.

- [x] **Step 3: Add the real crypto implementation**

  Add direct workspace dependencies `ring = "0.17"` and `base64 = "0.22"`, then add them to the CLI. Read PKCS#8 private keys only for signing and raw 32-byte Ed25519 public keys only for verification. Use `ring::signature::Ed25519KeyPair::sign` and `UnparsedPublicKey::<ED25519>::verify`; encode/decode signatures with standard base64. Build the `KeyReference` from explicit CLI metadata (`provider`, `key-id`, `key-version`, `scope`, and signer identity), attach the resulting signature record, and write only the signed artifact.

  Extend `conformance verify` with `--public-key-file` and `--require-signature`. If signature checking is configured, validate the artifact record, recompute the unsigned object digest, construct the same metadata-bound payload, and verify Ed25519. Keep the Q6 behavior unchanged when no public key and no `--require-signature` are provided.

- [x] **Step 4: Run the focused CLI tests to verify GREEN**

  Run: `cargo test -p vestrace-cli --test q7_signed_artifacts_cli -- --nocapture`

  Expected: PASS for manifest and bundle sign/verify plus both tamper negatives.

### Task 3: Documentation/status and regression evidence

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q7-signed-artifacts.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/superpowers/plans/2026-08-12-q7-signed-artifacts.md`

**Interfaces:**
- Consumes: fresh domain/CLI test output and the normative sections 10-13 of `vestrace-crypto-data-governance-contract-v0.2.md`, sections 20-21 of `vestrace-qualification-conformance-spec-v0.2.md`, and roadmap sections 7-10.
- Produces: an explicit Q7 evidence record distinguishing implemented Ed25519 local-file verification from deferred external KMS/HSM/Vault adapters and full TRUSTED qualification.

- [x] **Step 1: Record the implementation boundary**

  Document that manifests and qualification bundles now carry structured signature metadata, sign/verify covers the unsigned object plus metadata, unsigned artifacts remain allowed unless configured otherwise, and the local key-file adapter is test infrastructure/operational tooling rather than a production secret backend.

- [x] **Step 2: Update current status and plan checkboxes**

  Mark only the Q7 implementation and focused evidence complete. Keep v1.0/TRUSTED, automatic server/worker qualification, fault/recovery campaigns, and KMS/HSM/Vault-backed key resolution explicitly open.

### Task 4: Verification gate

**Files:**
- Inspect only: changed files and existing Q1-Q6 tests

- [x] **Step 1: Run focused Rust tests and format checks**

  Run the Q7 domain/CLI tests, `cargo check --workspace`, `cargo test --workspace --lib -- --nocapture`, the CLI integration suite, scoped `rustfmt --check`, and `git diff --check`.

- [x] **Step 2: Run the Q7 tamper path against built CLI output**

  Verify that a signed artifact passes with its matching public key and fails after changing either an asserted field or the public key. Keep generated files outside the repository or remove only Q7 temporary files.

- [x] **Step 3: Review the exact diff and unstaged boundary**

  Confirm no unrelated files were modified by Q7, no staged changes exist, and do not claim v1.0 completion from this gate.
