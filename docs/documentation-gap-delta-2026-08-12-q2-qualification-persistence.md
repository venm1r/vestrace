# Q2 qualification persistence delta — 2026-08-12

## Scope

Q2 adds the first durable runtime-facing storage boundary for the Q1 target-bound `QualificationBundle` artifact. It is deliberately a control-plane repository slice; it does not run qualification, sign release evidence, or authorize a deployment.

Implemented:

- application `QualificationRepository` port with `insert`, `find_by_id`, and target-bound `find_latest` operations;
- PostgreSQL `qualification_bundles` migration `0125` with indexed lifecycle/profile/status/target digest metadata;
- complete serialized bundle payload retained in `JSONB` so evidence and future bundle fields are not silently dropped;
- immutable write behavior: an exact repeated insert is idempotent, while reuse of an id for different evidence returns a conflict;
- read-side metadata validation that rejects a payload whose id, lifecycle, profile, status, target digest, or timestamps disagree with indexed columns;
- deterministic latest lookup ordered by qualification start time, creation time, and bundle id.

Qualification bundles remain global release/deployment evidence rather than workspace-owned records, so this repository intentionally has no `RequestContext` or workspace RLS policy. Workspace-scoped application data remains on its existing RLS paths.

## Verification

Passed:

- `cargo test -p vestrace-infrastructure --test qualification_repository --no-run`

The two PostgreSQL round-trip tests are present and compile, but execution is blocked in the current environment because `DATABASE_URL` is not set. This is the same external database prerequisite affecting the existing infrastructure integration suite; it is not reported as a passing runtime qualification gate.

## Explicit non-claims

Q2 does not establish:

- a passing CORE, TRUSTED, or federated profile;
- deployment identity or runtime environment qualification;
- signed/attested qualification evidence;
- KMS/HSM/Vault, secret lifecycle, deletion executor, or crash/recovery orchestration;
- automatic qualification persistence from the CLI or worker runtime;
- v1.0 readiness.
