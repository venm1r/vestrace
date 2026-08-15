# Q4 Capability Manifest Implementation Plan

> **For agentic workers:** follow this plan task by task, preserving unrelated dirty work and verifying each slice before moving on.

**Goal:** Publish a machine-readable `VestraceCapabilityManifest` that records the exact product/build assertion and declared capability boundaries needed to bind later deployment qualification evidence.

**Architecture:** Keep the manifest as a domain-owned, immutable assertion with deterministic normalization and digest calculation. Expose it through a database-free `vestrace conformance manifest` command that writes the JSON artifact before returning. This slice does not turn declarations into qualification evidence, add signatures, or claim runtime/provider capability.

**Tech Stack:** Rust 2024, serde, sha2, clap, existing `QualificationProfile` and CLI test conventions.

## Global Constraints

- Follow the qualification specification's manifest fields: product/version, source revision, build digest, schema versions, profiles, optional features, storage, crypto/provider, model/provider, external effects, federation, and limitations.
- Normalize repeated declaration lists deterministically; reject blank required values and blank list entries.
- Compute a stable `sha256:` manifest digest from the normalized assertion; do not present the digest as a signature or runtime proof.
- Keep the command database-free and artifact-first.
- Preserve Q1-Q3 bundle behavior and all unrelated dirty changes; do not stage, commit, reset, clean, or rebase.
- Use RED -> GREEN -> focused/full verification.

---

### Task 1: Add the domain capability-manifest contract

**Files:**
- Modify: `crates/vestrace-domain/src/release/mod.rs`
- Test: `crates/vestrace-domain/src/release/mod.rs`

- [x] Write tests for required-field/list validation, deterministic normalization, and digest sensitivity.
- [x] Run the focused domain tests to verify RED.
- [x] Implement `VestraceCapabilityManifest` with serde/schema output, accessors, normalized lists, and deterministic digest.
- [x] Run the focused tests to verify GREEN.

### Task 2: Expose a DB-free CLI manifest artifact

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Test: `crates/vestrace-cli/tests/q4_capability_manifest_cli.rs`

- [x] Add `conformance manifest` arguments and the command dispatch.
- [x] Write the failing CLI regression test, including database-free behavior and digest output.
- [x] Implement JSON serialization and file emission without loading database configuration.
- [x] Run the focused CLI test to verify GREEN.

### Task 3: Document the gate and its non-claims

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q4-capability-manifest.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`

- [x] Record command usage, normalized manifest shape, and exact digest semantics.
- [x] State that the manifest is an assertion only; signatures, runtime verification, deployment qualification, and release approval remain open.

### Task 4: Verify and hand off

- [x] Run focused domain and CLI tests plus changed-file formatting.
- [x] Run workspace check, library tests, no-run compilation, and diff checks.
- [x] Inspect the diff for Q4 scope only and record any environment-blocked gates.
