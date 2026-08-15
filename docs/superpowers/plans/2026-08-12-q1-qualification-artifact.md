# Q1 Qualification Artifact Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the existing conformance report and target-bound `QualificationBundle` contract into a reproducible machine-readable qualification artifact without claiming a qualified v1.0 deployment.

**Architecture:** Keep the existing domain hard-gate authority in `TrustedQualificationGate`. Extend `QualificationBundle` with the qualification lifecycle and the complete `ConformanceReport`, then add a CLI `conformance bundle` command that runs the existing deterministic evaluator, maps its result to local hard-gate evidence, and writes the target-bound bundle even when the profile fails. A failed bundle must remain an explicit artifact and must produce a non-zero command result.

**Tech Stack:** Rust 2024, serde/serde_json, clap, existing `vestrace-domain::conformance` and `vestrace-domain::trust` contracts, integration tests.

## Global Constraints

- Do not claim `TRUSTED`, v1.0, durable qualification persistence, signed release evidence, or deployment qualification from this slice.
- Preserve all unrelated dirty changes and generated artifacts; do not reset, clean, rebase, stage, or commit.
- Follow RED → GREEN → focused/full verification for every implementation change.
- A bundle must carry exact target identity fields: target manifest, source revision, build digest, configuration digest, environment manifest, suite version, profile, lifecycle, and conformance results.
- A failed/skipped/inconclusive required requirement must remain visible in the artifact and cannot be converted to pass by serialization.

---

### Task 1: Extend the qualification bundle with lifecycle and complete conformance results

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs:1850-1960`
- Test: `tests/q1_qualification_artifact.rs`

**Interfaces:**
- Add `QualificationLifecycle::{PreMerge, Release, Deployment, Periodic, PostIncident}`.
- Add `QualificationStatus::{Passed, Failed, Incomplete}`.
- Add `QualificationBundle::from_conformance_report(...) -> Result<Self, DomainError>` accepting lifecycle, profile, target identity, `ConformanceReport`, hard-gate evidence, known limitations, and timestamps.
- Add `QualificationBundle::lifecycle()`, `conformance_report()`, and `status()` accessors.

- [x] **Step 1: Write the failing domain integration test**

Create a complete Core report and assert that `QualificationBundle::from_conformance_report` preserves lifecycle, report results, target identity through `target_digest`, and reports `Passed`; create a report with one skipped result and assert `Failed`.

- [x] **Step 2: Run the focused test to verify RED**

Run: `cargo test --test q1_qualification_artifact -- --nocapture`

Expected: compile failure because the lifecycle/status types and report constructor/accessors do not exist.

- [x] **Step 3: Implement the minimal domain model**

Add the enums and fields to `QualificationBundle`; implement the report-backed constructor by reusing existing target identity validation and unique hard-gate evidence validation, require report profile equality, and compute status as `Incomplete` when no report exists, `Failed` when the report is not passing, otherwise `Passed`.

- [x] **Step 4: Run the focused test to verify GREEN**

Run: `cargo test --test q1_qualification_artifact -- --nocapture`

Expected: all Task 1 tests pass.

---

### Task 2: Add the machine-readable `conformance bundle` CLI command

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs:50-68`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs:1-260`
- Test: `crates/vestrace-cli/tests/q1_qualification_cli.rs`

**Interfaces:**
- Add `ConformanceAction::Bundle` with required profile/output/target identity/suite arguments, optional lifecycle and repeatable known limitations.
- Add `run_bundle(...) -> anyhow::Result<()>`.
- The command writes pretty JSON to the requested output path before returning a failure for a non-passing report.

- [x] **Step 1: Write the failing CLI integration test**

Invoke `vestrace conformance bundle --profile trusted` with deterministic target fields and an output path; assert the command writes JSON containing `profile`, `lifecycle`, `target_digest`, `conformance_report`, and a non-passing status because the current static evaluator still has skipped requirements.

- [x] **Step 2: Run the focused test to verify RED**

Run: `cargo test --test q1_qualification_cli -- --nocapture`

Expected: compile or clap failure because `bundle` is not a recognized action and no artifact is produced.

- [x] **Step 3: Implement CLI parsing and artifact emission**

Map `ConformanceCaseResult` statuses to `HardGateEvidence`, preserve the first evidence item per requirement, construct the bundle with `vestrace_domain::now()`, serialize it with `serde_json`, create only the requested parent directory when needed, write the artifact, print its status/path, and return an error when status is not `Passed`.

- [x] **Step 4: Run the focused test to verify GREEN**

Run: `cargo test --test q1_qualification_cli -- --nocapture`

Expected: the artifact is written and the command’s non-zero outcome is intentional and asserted.

---

### Task 3: Harden profile/result traceability and document the bounded gate

**Files:**
- Modify: `crates/vestrace-domain/src/trust.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Create: `docs/documentation-gap-delta-2026-08-12-q1-qualification-artifact.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`

**Interfaces:**
- The bundle constructor rejects a report whose profile differs from the requested profile.
- The bundle JSON remains sufficient to reconstruct the profile result and target identity without relying on CLI text output.

- [x] **Step 1: Add negative tests**

Cover profile mismatch, duplicate hard-gate evidence, incomplete bundle status, and preservation of skipped requirement results.

- [x] **Step 2: Run the focused domain/CLI tests**

Run: `cargo test --test q1_qualification_artifact --test q1_qualification_cli -- --nocapture`

- [x] **Step 3: Add the documentation delta**

Record the command, artifact fields, intentional non-zero behavior for failed qualification, exact verification, and explicit non-claims about durable persistence, signatures, deployment identity authority, and v1.0 readiness.

- [x] **Step 4: Update navigation only with evidence-backed claims**

Add the Q1 delta to current implementation/status documents; do not change historical baseline claims into qualification claims.

---

### Task 4: Full verification and handoff

**Files:**
- Test: `tests/q1_qualification_artifact.rs`
- Test: `crates/vestrace-cli/tests/q1_qualification_cli.rs`

- [x] **Step 1: Run formatting and focused checks**

Run rustfmt checks for changed Rust files, both Q1 integration tests, and JSON parsing assertions for the emitted artifact.

- [x] **Step 2: Run workspace verification**

Run `cargo check --workspace`, `cargo test --workspace --lib -- --nocapture`, and `cargo test --workspace --no-run`.

- [x] **Step 3: Run final diff checks**

Run `git diff --check`, inspect the scoped diff, and confirm unrelated dirty files remain untouched.

- [x] **Step 4: Record completion without release claims**

Update this plan’s checkboxes and report the exact passed gates, warnings, artifact path behavior, and remaining durable/runtime qualification gaps.
