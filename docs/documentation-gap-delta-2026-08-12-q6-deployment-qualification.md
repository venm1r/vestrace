# Documentation Gap Delta — Q6 Deployment Qualification Verifier

**Date:** 2026-08-12  
**Scope:** verify exact manifest-bound deployment evidence against the live PostgreSQL runtime  
**Authoritative sources:** `vestrace-qualification-conformance-spec-v0.2.md`, sections 2–3, 8, 20, 22, 24, 29–33; `vestrace-version-roadmap-v0.2-to-v1.0.md`, sections 7, 9–10.

## Gap before Q6

Q5 prevented a bundle from being built with mismatched manifest identity, but the persisted artifact was still only evidence from the evaluator invocation. There was no read-only command that rechecked the exact profile/lifecycle/manifest binding against the database migration state and the login actually used by the deployment.

## Q6 implementation

### Domain binding verification

`QualificationBundle::validate_manifest_identity` validates the manifest and compares the requested profile/lifecycle plus target manifest digest, source revision, build digest, configuration digest, and environment manifest. `validate_manifest_binding` adds the hard status check and rejects anything other than `Passed`.

### Runtime evidence

`PgStore::deployment_qualification_evidence` is read-only. It reports migration-history compatibility and the runtime PostgreSQL role flags: role name, superuser status, `rolbypassrls`, and inherited bootstrap membership. A runtime role is accepted only when it is not superuser, does not bypass RLS, and does not inherit bootstrap privileges.

### CLI

The new command is:

```text
vestrace conformance verify \
  --profile core \
  --lifecycle deployment \
  --target-manifest-file capability-manifest.json \
  --bundle-file qualification-bundle.json \
  --output deployment-verification.json
```

It emits a machine-readable result with `manifest_integrity`, `bundle_target_binding`, `bundle_status`, `migration_history`, and `runtime_database` checks. Failed verification still writes the result artifact; a tampered manifest fails before artifact emission.

## Verification evidence

- Q6 domain tests pass: exact binding is accepted; identity, profile, lifecycle, and failed-status mismatches are rejected.
- Q6 CLI tests pass: failed evidence is serialized before a non-zero exit and tampered manifests are rejected before output.
- `cargo check --workspace` passes.
- Docker rebuilt the image and ran the verifier against the Compose restricted runtime. The result was deliberately `status: failed` because the current conformance bundle contains failed/skipped evidence, while `manifest_integrity`, `bundle_target_binding`, `migration_history`, and `runtime_database` all passed.
- Live runtime evidence resolved to `vestrace` with `rolsuper=false`, `rolbypassrls=false`, no bootstrap inheritance, and migration history `55` applied / latest `125`.
- A tampered manifest exited non-zero with no verification artifact and reported a digest mismatch.

## Explicit non-claims

Q6 does not claim:

- a passed CORE, TRUSTED, or v1.0 profile;
- signed manifests/bundles or signer trust;
- automatic server/worker qualification;
- fault injection, crash/recovery, post-incident requalification, or progressive restoration;
- KMS/HSM/Vault or external provider adapters;
- release approval or permanent certification.

Q6 establishes a reusable deployment verification gate, not a release qualification decision.
