# Q5 Manifest-Bound Qualification Bundle Implementation Plan

> **For agentic workers:** follow this plan task by task, preserving unrelated dirty work and verifying each slice before moving on.

**Goal:** Bind a validated Q4 `VestraceCapabilityManifest` to `QualificationBundle` so deployment qualification cannot silently mix a manifest with different build, configuration, or environment identity.

**Architecture:** Add a domain factory that consumes the manifest as the canonical target identity and retains the manifest digest as the bundle's `target_manifest` reference. Add `--target-manifest-file` to the bundle command; it validates the JSON, derives all exact identity fields, and keeps the existing explicit-string path unchanged for backward compatibility. Persistence remains the existing Q3 repository path.

**Tech Stack:** Rust 2024, serde/serde_json, clap, existing QualificationBundle and PostgreSQL repository, Docker Compose PostgreSQL runtime.

## Global Constraints

- A manifest file is an assertion and integrity-checked input, not a signature or runtime proof.
- Reject tampered digest, blank identity, non-normalized declaration lists, and a profile absent from `supported_profiles`.
- File mode must use the manifest's source/build/configuration/environment values exactly; do not accept duplicate override values.
- Preserve the legacy explicit `--target-manifest` plus identity flags and Q1-Q4 behavior.
- Persist failed evidence before the command returns its non-zero qualification result.
- Preserve unrelated dirty changes and Docker data; do not stage, commit, reset, rebase, or remove volumes.
- Use RED -> GREEN -> focused/full verification.

---

### Task 1: Validate serialized capability manifests

**Files:**
- Modify: `crates/vestrace-domain/src/release/mod.rs`
- Test: `crates/vestrace-domain/src/release/mod.rs`

- [x] Add `from_json`/integrity validation tests for tampered digest and non-normalized payloads.
- [x] Run focused domain tests to verify RED.
- [x] Implement validated JSON loading without exposing mutable manifest fields.
- [x] Run focused tests to verify GREEN.

### Task 2: Add the manifest-bound QualificationBundle factory

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs`
- Test: `tests/q5_manifest_binding.rs`

- [x] Write the failing test that binds exact manifest identity and rejects unsupported profile.
- [x] Run the focused integration test to verify RED.
- [x] Implement `QualificationBundle::from_conformance_report_for_manifest` and target reference accessor.
- [x] Run the focused test to verify GREEN.

### Task 3: Wire the CLI manifest-file mode

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Test: `crates/vestrace-cli/tests/q5_manifest_binding_cli.rs`

- [x] Add mutually exclusive `--target-manifest` / `--target-manifest-file` arguments with file-mode identity derivation.
- [x] Add CLI tests for valid binding and tampered-file rejection before artifact emission.
- [x] Implement file loading and call the domain factory; keep persistence and artifact-first ordering intact.
- [x] Run Q1, Q3, Q4, and Q5 CLI tests.

### Task 4: Document and verify

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q5-manifest-qualified-bundle.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`

- [x] Document exact file-mode command and non-claims.
- [x] Run focused tests, workspace check/library tests/no-run, scoped formatting/diff checks.
- [x] Run Docker live `conformance bundle --persist` with a generated Q4 manifest and verify persisted target identity through restricted PostgreSQL.
- [x] Record runtime role/RLS and migration evidence separately from qualification status.
