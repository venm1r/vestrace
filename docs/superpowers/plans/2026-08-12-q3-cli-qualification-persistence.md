# Q3 CLI Qualification Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Q1 `conformance bundle` CLI to the Q2 durable repository through the normal application configuration and PostgreSQL composition, while preserving the artifact-first and fail-closed qualification behavior.

**Architecture:** Persistence is explicit via `--persist`, so report generation and local artifact emission remain usable without a database. When requested, the CLI loads the standard `AppConfig`, connects/migrates/verifies the database, and inserts the complete bundle through `QualificationRepository`; persistence occurs before the command reports a failed qualification status, but a persistence failure still returns an error and never claims success.

**Tech Stack:** Rust 2024, clap, Tokio, existing `AppConfig`/`PgStore` composition, `QualificationRepository`, integration and unit tests.

## Global Constraints

- Preserve Q1 artifact-first behavior: write the JSON artifact before database configuration or persistence errors are returned.
- Never convert a failed/skipped/inconclusive report into a passed qualification result.
- Do not claim signed deployment evidence, automatic worker qualification, TRUSTED closure, or v1.0 readiness.
- Reuse the existing CLI `AppConfig` → `PgStore` migration/compatibility path; do not introduce a second database configuration format.
- Preserve unrelated dirty changes and generated artifacts; do not reset, clean, rebase, stage, or commit.
- Follow RED → GREEN → focused/full verification for every implementation change.

---

### Task 1: Add the persistence delegation contract

**Files:**
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`

**Interfaces:**
- Add async `persist_bundle(&QualificationBundle, &dyn QualificationRepository)`.
- Preserve the full bundle, including failed conformance results, when delegating to the repository.

- [x] **Step 1: Write the failing unit test**
- [x] **Step 2: Run the focused test to verify RED**
- [x] **Step 3: Implement the minimal repository delegation**
- [x] **Step 4: Run the focused test to verify GREEN**

### Task 2: Wire `--persist` through standard CLI composition

**Files:**
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Test: `crates/vestrace-cli/tests/q3_qualification_persistence_cli.rs`

**Interfaces:**
- Add `conformance bundle --persist`.
- Keep `list`, `check`, and bundle-without-persist database-free.
- On `--persist`, load `AppConfig`, connect `PgStore`, run and verify migrations, then use `PgQualificationRepository`.
- Write the local JSON artifact before attempting configuration/database persistence.

- [x] **Step 1: Add the failing CLI integration test**
- [x] **Step 2: Run the focused test to verify RED**
- [x] **Step 3: Implement async command dispatch and PostgreSQL composition**
- [x] **Step 4: Run Q1 and Q3 CLI tests to verify GREEN**

### Task 3: Document the new runtime gate

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q3-cli-qualification-persistence.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`

- [x] **Step 1: Document command usage, artifact-first ordering, and persistence failure semantics**
- [x] **Step 2: Record exact non-claims and the PostgreSQL environment requirement**

### Task 4: Verify and hand off

**Files:**
- Test: `crates/vestrace-cli/src/commands/conformance.rs`
- Test: `crates/vestrace-cli/tests/q1_qualification_cli.rs`
- Test: `crates/vestrace-cli/tests/q3_qualification_persistence_cli.rs`

- [x] **Step 1: Run focused tests and changed-file formatting checks**
- [x] **Step 2: Run `cargo check --workspace`, workspace lib tests, and workspace no-run**
- [x] **Step 3: Run `git diff --check` and inspect only Q3/Q2/Q1 scope**
- [x] **Step 4: Record live PostgreSQL test status separately from compile/CLI evidence**
