# Documentation Gap Delta — Q4 Capability Manifest

**Date:** 2026-08-12
**Scope:** machine-readable capability/deployment assertion for qualification identity
**Authoritative sources:** `vestrace-qualification-conformance-spec-v0.2.md`, sections 7–8 and 19; `vestrace-version-roadmap-v0.2-to-v1.0.md`, v1.0 exit criteria.

## Gap before Q4

Q1 produced a target-bound `QualificationBundle`, Q2 made bundles durable in PostgreSQL, and Q3 added explicit CLI persistence. The bundle still accepted an opaque `target_manifest` string, so the repository had no domain-owned machine-readable assertion for product/build capabilities or the configuration/environment identity that qualification claims should bind.

The normative specification requires a `VestraceCapabilityManifest` assertion containing product/version, source revision, build digest, schema versions, supported profiles, optional features, storage, crypto/provider, model/provider, external-effect, federation, and known-limitation declarations. Qualification must remain separate: a manifest assertion is not evidence that a deployment actually provides those capabilities.

## Q4 implementation

### Domain contract

`vestrace_domain::release::VestraceCapabilityManifest` now:

- requires non-empty manifest/product/version, source revision, build digest, configuration digest, environment manifest, schema versions, and supported profiles;
- normalizes all repeated string declarations by trimming, sorting, and de-duplicating them;
- normalizes supported profiles deterministically and removes duplicates;
- serializes the complete assertion as JSON/schema output;
- computes a deterministic `sha256:` manifest digest over the normalized assertion, excluding the digest field itself.

The digest is an integrity identifier for the assertion. It is not a signature, authorship proof, or runtime capability check.

### CLI artifact

The database-free command is:

```text
vestrace conformance manifest \
  --manifest-version manifest-v1 \
  --product vestrace \
  --product-version 0.2.0 \
  --source-revision <revision> \
  --build-digest <build-digest> \
  --configuration-digest <configuration-digest> \
  --environment-manifest <environment-ref> \
  --schema-version <schema-version> \
  --profile core \
  --output capability-manifest.json
```

Repeatable flags declare optional features, storage backends, crypto providers, model providers, external-effect adapters, federation capabilities, known limitations, additional schema versions, and additional supported profiles. The command writes the JSON artifact without loading `AppConfig` or requiring PostgreSQL.

## Verification

- Domain manifest tests: 3 passed, covering blank identity/list values, normalization, and digest sensitivity.
- CLI artifact test: 1 passed, including successful execution with `VESTRACE__DATABASE__URL` removed.
- The manifest artifact contains normalized profile declarations and a `sha256:` digest.

## Explicit non-claims

Q4 does not claim:

- automatic discovery of the running binary, configuration, schema state, dependencies, or operating environment;
- that command arguments are independently verified deployment facts;
- signed manifests or provenance/authorship verification;
- automatic insertion of the manifest into a `QualificationBundle` or release record;
- successful PostgreSQL runtime qualification, fault/recovery qualification, a passing profile, or v1.0 readiness.

The next qualification slice must consume this manifest as target identity and add runtime/provider/deployment evidence rather than treating the declaration as proof.
