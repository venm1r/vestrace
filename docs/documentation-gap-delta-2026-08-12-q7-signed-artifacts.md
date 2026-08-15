# Documentation Gap Delta — Q7 Signed Artifacts

**Date:** 2026-08-12  
**Scope:** structured signatures for capability manifests and qualification bundles  
**Authoritative sources:** `vestrace-crypto-data-governance-contract-v0.2.md`, sections 10–13; `vestrace-qualification-conformance-spec-v0.2.md`, sections 20–21; `vestrace-version-roadmap-v0.2-to-v1.0.md`, sections 7–10.

## Gap before Q7

Q4/Q5/Q6 produced digest-bearing manifests and exact target-bound qualification bundles, but `QualificationBundle.signature` was only an unused `Option<String>`. The digest established content identity; it did not establish signer identity or authenticity. There was also no executable signature verification path for either artifact.

## Q7 implementation

### Domain contract

The domain now provides:

- `SignatureAlgorithm::Ed25519` with an explicit serialized algorithm identifier;
- `SignatureRecord` containing `object_digest`, `signer_identity`, `key_ref`, `algorithm`, `signature`, and `signed_at`;
- validation that the key reference has `Signing` purpose and an algorithm suite matching the signature algorithm;
- unsigned canonical payloads and `sha256:` object digests for `VestraceCapabilityManifest` and `QualificationBundle`;
- attachment and structural validation that reject stale object digests or a second signature;
- signature payloads covering the unsigned object plus signature metadata, so signer/key metadata is not outside the signed bytes.

The optional signature field is excluded from the manifest identity digest and qualification target digest. Existing unsigned JSON remains readable; configured verification is an additional gate.

### CLI adapter

The CLI now supports a local file-backed Ed25519 path:

```text
vestrace conformance sign \
  --artifact manifest|bundle \
  --artifact-file artifact.json \
  --private-key-file release-signing-key.pkcs8 \
  --signer-identity issuer://release \
  --key-provider local-file \
  --key-id release-signing \
  --key-version v1 \
  --key-scope release \
  --output signed-artifact.json

vestrace conformance verify-signature \
  --artifact manifest|bundle \
  --artifact-file signed-artifact.json \
  --public-key-file release-signing-key.public
```

Signing uses PKCS#8 Ed25519 private keys; verification uses the explicit raw Ed25519 public key. The private key is not copied into the artifact. `conformance verify` can additionally require a signed qualification bundle with `--require-signature --public-key-file ...` while retaining the Q6 deployment checks.

## Verification evidence

- Domain RED→GREEN tests cover structured metadata, object-digest binding, signing-purpose/algorithm validation, tamper detection, and unsigned JSON compatibility.
- CLI RED→GREEN integration tests sign and verify both manifest and bundle artifacts with real Ed25519 keys, then reject a tampered artifact and a mismatched public key.
- The local adapter uses `ring` Ed25519 signing/verification and base64-encoded signature bytes; it does not use a digest as a fake signature.

## Explicit non-claims

Q7 does not claim:

- KMS/HSM/Vault/OS-keyring production key resolution or rotation orchestration;
- signer trust policy, issuer allowlists, profile-specific signer governance, or federation trust;
- automatic server/worker qualification or a passing CORE/TRUSTED bundle;
- fault injection, crash/recovery, post-incident requalification, or progressive restoration;
- release approval, permanent certification, or v1.0 completion.

Q7 establishes a real signed-artifact contract and executable local verification gate; the relying party still decides whether the signer, key, suite, profile, and lifecycle are trusted.
