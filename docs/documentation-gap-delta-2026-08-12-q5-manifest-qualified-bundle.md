# Documentation Gap Delta — Q5 Manifest-Bound Qualification Bundle

**Date:** 2026-08-12
**Scope:** bind a validated capability manifest to target-bound qualification evidence
**Authoritative sources:** `vestrace-qualification-conformance-spec-v0.2.md`, sections 7–8 and 19–24; `vestrace-version-roadmap-v0.2-to-v1.0.md`, v1.0 exit criteria.

## Gap before Q5

Q4 emitted a machine-readable `VestraceCapabilityManifest`, but Q1–Q3 bundle creation still accepted an opaque `--target-manifest` string and independently supplied source/build/configuration/environment fields. That allowed a caller to accidentally combine a manifest from one target with identity values from another target.

## Q5 implementation

### Validated manifest input

`VestraceCapabilityManifest::from_json` now rejects:

- invalid JSON;
- blank required identity values;
- missing schema versions or supported profiles;
- blank, unsorted, or duplicate declaration lists;
- unsorted or duplicate supported profiles;
- a `manifest_digest` that does not match the normalized assertion.

The loader validates the serialized assertion without exposing mutable fields.

### Manifest-bound bundle factory

`QualificationBundle::from_conformance_report_for_manifest`:

- validates the manifest;
- requires the requested profile to be listed in `supported_profiles`;
- uses the manifest digest as `QualificationBundle.target_manifest`;
- copies source revision, build digest, configuration digest, and environment manifest directly from the validated manifest;
- retains the existing report/evidence/status and target digest calculation.

### CLI

The canonical file mode is:

```text
vestrace conformance bundle \
  --profile core \
  --target-manifest-file capability-manifest.json \
  --suite-version suite-v1 \
  --output qualification-bundle.json
```

`--target-manifest-file` is mutually exclusive with the legacy `--target-manifest` and rejects duplicate identity overrides. The legacy explicit-string mode remains available for compatibility. In either mode, artifact-first and `--persist` semantics remain unchanged.

## Live Docker evidence

Against the Compose PostgreSQL runtime:

- embedded migration history was compatible: `55` applied migrations, latest version `125`;
- the deployed database login was `vestrace|rolsuper=f|rolbypassrls=f`;
- workspace context returned one scoped workspace row and missing context returned zero rows;
- `conformance bundle --persist` wrote a failed `deployment/core` bundle to `qualification_bundles` and exited non-zero, preserving fail-closed status.

The initial standard RLS shell script could not run unchanged because this Windows checkout stores it with CRLF and Bash rejected `pipefail\r`; equivalent runtime SQL checks were executed directly without modifying that existing script.

## Explicit non-claims

Q5 does not claim:

- a signed manifest or provenance/authorship verification;
- automatic discovery or independent attestation of manifest fields;
- a passing CORE/TRUSTED/deployment profile;
- worker/server automatic qualification or fault/recovery evidence;
- release approval or v1.0 readiness.

The next gate can build on an exact manifest-bound target, but still needs runtime capability checks, signed evidence where configured, and complete hard-gate closure.
