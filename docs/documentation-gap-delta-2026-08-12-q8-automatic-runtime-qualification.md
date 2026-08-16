# Documentation Gap Delta — Q8 Automatic Runtime Qualification

**Date:** 2026-08-12  
**Scope:** automatic deployment qualification before server readiness or worker polling  
**Authoritative sources:** `vestrace-qualification-conformance-spec-v0.2.md`, sections 2–4, 8, 20–22, 29–31; `vestrace-version-roadmap-v0.2-to-v1.0.md`, sections 7–10.

## Gap before Q8

Q6 could verify a supplied manifest/bundle against a live PostgreSQL login, but the running `server` and `worker` never invoked that boundary. Q7 added real local signatures, but neither service automatically collected deployment evidence, persisted its own bundle, or stopped before accepting work when qualification failed.

## Q8 implementation

The infrastructure configuration now has an explicit, default-disabled `[qualification]` section:

```toml
[qualification]
enabled = true
manifest_file = "capability-manifest.json"
profile = "core"
lifecycle = "deployment"
suite_version = "runtime-v1"
output = "qualification-server.json"
```

When enabled, both `vestrace server` and `vestrace worker` call the same automatic runner after migrations are applied and verified. The runner:

- loads and validates the target-bound `VestraceCapabilityManifest`;
- runs the existing deterministic profile evaluator;
- collects migration compatibility and the actual PostgreSQL role flags through the read-only Q6 path;
- adds live deployment evidence to the report;
- builds a deployment-lifecycle `QualificationBundle`;
- writes a machine-readable artifact before returning a failed qualification result;
- persists the complete bundle through `QualificationRepository`; and
- fails closed before the HTTP listener is bound or worker polling starts.

The shared decision requires manifest integrity, exact bundle binding, a passed bundle, compatible migration history, and a non-empty `NOSUPERUSER`/`NOBYPASSRLS` runtime role without bootstrap inheritance. Unavailable runtime evidence is represented as failed evidence, not as an implicit pass.

## Verification evidence

- application tests cover exact-target restricted-runtime success, superuser/RLS-bypass denial, and unavailable-runtime fail-closed behavior;
- infrastructure tests cover default-disabled and typed explicit qualification configuration;
- CLI tests cover worker artifact emission and persistence of failed evidence before the startup error;
- `cargo check --workspace` passes.

Live Compose verification is a separate environment gate: it requires a manifest mounted into the service and `[qualification]` enabled, and must report the actual restricted runtime role. This slice does not convert the existing failed conformance fixture into a passing profile.

## Explicit non-claims

Q8 does not claim:

- a passed CORE, TRUSTED, or v1.0 profile;
- KMS/HSM/Vault or OS-keyring key resolution, rotation, or signer trust policy;
- automatic signature verification or issuer/profile allowlists;
- deterministic fault injection, crash/recovery execution, or post-incident requalification;
- release approval, permanent certification, or progressive trust restoration.
