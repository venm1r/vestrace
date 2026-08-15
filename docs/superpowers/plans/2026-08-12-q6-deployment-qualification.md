# Q6 Deployment Qualification Verifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only deployment qualification verifier that proves a persisted qualification bundle is bound to the supplied capability manifest and is being checked against compatible migrations and a restricted PostgreSQL runtime role.

**Architecture:** The domain owns exact manifest/bundle identity validation and refuses a claim when the bundle is incomplete, failed, for another profile/lifecycle, or carries identity fields that differ from the manifest. Infrastructure owns live PostgreSQL evidence: migration history compatibility and runtime role restrictions. The CLI composes these checks into a versioned JSON result and exits non-zero for any failed check; it does not mutate the database or upgrade a failed bundle into a qualification claim.

**Tech Stack:** Rust workspace, `serde`/`serde_json`, `sqlx` PostgreSQL, `clap`, existing `VestraceCapabilityManifest` and `QualificationBundle` types, Docker Compose PostgreSQL runtime.

## Global Constraints

- A manifest is an assertion; qualification evidence determines whether the assertion is confirmed.
- Exact target identity includes source revision, build digest, configuration digest, environment manifest, profile, and lifecycle.
- Required `SKIPPED`/`INCONCLUSIVE` or failed evidence cannot produce a qualified result.
- Verification is read-only: it may inspect migration metadata and role metadata but must not run migrations or mutate qualification rows.
- PostgreSQL runtime evidence must reject superuser, `rolbypassrls`, and inherited bootstrap privileges.
- Existing unrelated dirty changes, generated evidence, Docker volumes, and uncommitted Q1-Q5 slices must be preserved.

---

### Task 1: Freeze the Q6 contract and domain binding check

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs`
- Test: `tests/q6_deployment_qualification.rs`

**Interfaces:**
- Consumes: `QualificationBundle`, `VestraceCapabilityManifest`, `QualificationProfile`, `QualificationLifecycle`.
- Produces: `QualificationBundle::validate_manifest_binding(&self, manifest: &VestraceCapabilityManifest, profile: QualificationProfile, lifecycle: QualificationLifecycle) -> Result<(), DomainError>`.

- [x] **Step 1: Write the failing tests**

  Add tests that construct a valid manifest and manifest-bound bundle, then assert the new method accepts an exact profile/lifecycle/identity match, rejects a tampered bundle identity, rejects a profile mismatch, rejects a lifecycle mismatch, and rejects a bundle whose `status()` is not `Passed`.

- [x] **Step 2: Run the tests to verify they fail**

  Run `cargo test -p vestrace-domain --test q6_deployment_qualification -- --nocapture`.

  Expected: compilation failure because `validate_manifest_binding` is not defined.

- [x] **Step 3: Implement the minimal domain validation**

  Add the required identity accessors and compare every manifest-bound field, supported profile, requested profile, requested lifecycle, and bundle status. Return `DomainError::InvalidArgument` with field-specific messages. Call `manifest.validate()` before comparing values.

- [x] **Step 4: Run the focused tests to verify they pass**

  Run `cargo test -p vestrace-domain --test q6_deployment_qualification -- --nocapture`.

  Expected: all Q6 domain tests pass.

### Task 2: Add read-only PostgreSQL deployment evidence

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/pool.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Test: `crates/vestrace-infrastructure/tests/postgres.rs`

**Interfaces:**
- Consumes: `PgStore` and the existing `migrations_are_compatible()` check.
- Produces: `RuntimeQualificationEvidence { migration_history_compatible: bool, runtime_role: String, is_superuser: bool, bypasses_rls: bool, inherits_bootstrap: bool }` and `PgStore::deployment_qualification_evidence() -> Result<RuntimeQualificationEvidence, InfrastructureError>`.

- [x] **Step 1: Write the failing infrastructure test**

  Add a `#[sqlx::test(migrations = "../../migrations")]` test that calls `deployment_qualification_evidence()` and asserts migration compatibility is true, the current role name is non-empty, and the returned flags reflect the connected role. Add a second test that modifies the latest migration checksum and asserts the method returns `migration_history_compatible == false` without mutating the migration table.

- [x] **Step 2: Run the tests to verify they fail**

  Run `cargo test -p vestrace-infrastructure --test postgres deployment_qualification -- --nocapture`.

  Expected: compilation failure because the evidence type and method do not yet exist.

- [x] **Step 3: Implement the minimal read-only query**

  Query `current_user`, `rolsuper`, `rolbypassrls`, and `pg_has_role(current_user, 'vestrace_bootstrap', 'MEMBER')`. Combine those values with `migrations_are_compatible()`. Do not call `migrate()` and do not expose passwords or database URLs in the evidence.

- [x] **Step 4: Run focused infrastructure tests**

  Run `cargo test -p vestrace-infrastructure --test postgres deployment_qualification -- --nocapture` with the repository’s configured test PostgreSQL.

  Expected: all focused infrastructure tests pass, or the missing external database is reported separately as an environment gate.

### Task 3: Add the CLI verifier and machine-readable result

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Create: `crates/vestrace-cli/tests/q6_deployment_qualification_cli.rs`

**Interfaces:**
- Consumes: `--profile`, `--lifecycle`, `--target-manifest-file`, `--bundle-file`, `--output`, `AppConfig`, `PgStore::deployment_qualification_evidence()`.
- Produces: `vestrace conformance verify` and JSON `{ "profile", "lifecycle", "target_manifest", "status", "checks" }`.

- [x] **Step 1: Write the failing CLI tests**

  Add a subprocess test that supplies a valid manifest and a failed bundle, asserts the verifier writes the JSON result before exiting non-zero, and asserts the result contains failed `bundle_status` and `runtime`/`migration` check entries. Add a second test that tampers with the manifest and asserts verification rejects it before writing the output artifact.

- [x] **Step 2: Run the tests to verify they fail**

  Run `cargo test -p vestrace-cli --test q6_deployment_qualification_cli -- --nocapture`.

  Expected: CLI parsing fails because `conformance verify` and its arguments are not defined.

- [x] **Step 3: Implement the verifier**

  Add the `Verify` action. Load the manifest with `VestraceCapabilityManifest::from_json`, load the bundle JSON, validate exact domain binding, connect with the standard application configuration, call the read-only infrastructure evidence method, serialize all checks, write the artifact, print a concise summary, and return an error when status is not `passed`. The artifact must survive failed verification just like Q1-Q5 artifacts.

- [x] **Step 4: Run the focused CLI tests**

  Run `cargo test -p vestrace-cli --test q6_deployment_qualification_cli -- --nocapture`.

  Expected: all Q6 CLI tests pass.

### Task 4: Verify the gate and update implementation status

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q6-deployment-qualification.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/superpowers/plans/2026-08-12-q6-deployment-qualification.md`

- [x] **Step 1: Run focused and workspace checks**

  Run the Q6 domain, infrastructure, and CLI tests, then `cargo check --workspace`, `cargo test --workspace --lib -- --nocapture`, `cargo test --workspace --no-run`, scoped `cargo fmt --check`, and `git diff --check`.

- [x] **Step 2: Run Docker deployment evidence**

  Start the existing Compose PostgreSQL/runtime services if needed, generate a manifest and bundle in the rebuilt image, run `conformance verify` against the restricted runtime database URL, and record the exact result. Preserve the existing volume and prior Q5 rows. Also run the verifier against a deliberately tampered manifest and record the fail-closed result.

- [x] **Step 3: Document claims and non-claims**

  Record that Q6 proves read-only deployment checks and exact target binding, while it does not provide signatures, automatic server/worker qualification, fault/recovery execution, KMS/HSM/Vault adapters, or a passing TRUSTED profile. Mark every plan item complete only after evidence is captured.

- [x] **Step 4: Review the diff without staging or committing**

  Inspect `git diff --stat`, `git diff --name-only`, and the Q6 diff for scope. Leave all changes uncommitted and do not alter unrelated dirty files.
