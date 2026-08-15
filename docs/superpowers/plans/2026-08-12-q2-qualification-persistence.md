# Q2 Durable QualificationBundle Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Q1 target-bound `QualificationBundle` a durable runtime-facing PostgreSQL artifact with immutable write semantics and lossless retrieval, without claiming deployment qualification or v1.0 readiness.

**Architecture:** Expose a small application `QualificationRepository` port for global qualification evidence. Implement it with `PgQualificationRepository` over a dedicated `qualification_bundles` table. Store indexed identity/status columns plus the complete serialized bundle payload; reject corrupted or metadata-mismatched rows on read and make repeated writes of the same exact bundle idempotent while rejecting conflicting reuse of an id.

**Tech Stack:** Rust 2024, async-trait, serde_json, SQLx PostgreSQL, existing migration runner and integration-test conventions.

## Global Constraints

- Do not claim `TRUSTED`, v1.0, signed release evidence, deployment qualification, or complete runtime qualification from this slice.
- Preserve all unrelated dirty changes and generated artifacts; do not reset, clean, rebase, stage, or commit.
- Follow RED → GREEN → focused/full verification for every implementation change.
- Qualification artifacts are global control-plane evidence, not workspace-owned business data; do not add a fake `RequestContext` or workspace RLS boundary.
- Qualification bundles are immutable evidence: an exact repeated insert is idempotent; a reused id with different content is a conflict.
- The database row must retain the complete Q1 bundle JSON so future fields are not silently discarded by persistence.

---

### Task 1: Define the application persistence port and RED repository contract

**Files:**
- Create: `crates/vestrace-application/src/qualification.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: `crates/vestrace-infrastructure/tests/qualification_repository.rs`

**Interfaces:**
- Add `QualificationRepository::insert`, `find_by_id`, and `find_latest`.
- Add `SharedQualificationRepository`.
- Define a PostgreSQL-backed round-trip test for incomplete bundles, exact retry idempotency, latest-by-target lookup, and missing-id behavior.

- [x] **Step 1: Write the failing integration test**
- [x] **Step 2: Run the focused test to verify RED**
- [x] **Step 3: Add the minimal application port and exports**
- [x] **Step 4: Run the focused compile/test target and keep the database gate explicit**

### Task 2: Add the durable schema and PostgreSQL implementation

**Files:**
- Create: `migrations/0125_qualification_bundles.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/qualification_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`

**Interfaces:**
- Add `PgQualificationRepository::new(PgStore)`.
- Persist indexed lifecycle/profile/status/target digest and the complete payload.
- Validate row metadata against the decoded domain bundle; map malformed evidence to storage errors.
- Implement immutable insert behavior and deterministic latest lookup.

- [x] **Step 1: Add the migration after the RED contract exists**
- [x] **Step 2: Implement serialization, metadata validation, insert, and retrieval**
- [x] **Step 3: Run focused repository tests and migration compile checks**

### Task 3: Document the runtime boundary and verification state

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-12-q2-qualification-persistence.md`
- Modify: `docs/current-implementation.md`
- Modify: `docs/documentation-status-v0.2.md`

- [x] **Step 1: Record the durable persistence behavior and exact non-claims**
- [x] **Step 2: Update current/status navigation without converting evidence into qualification**

### Task 4: Verify and hand off

**Files:**
- Test: `crates/vestrace-infrastructure/tests/qualification_repository.rs`

- [x] **Step 1: Run rustfmt and focused tests**
- [x] **Step 2: Run `cargo check --workspace`, workspace lib tests, and `cargo test --workspace --no-run`**
- [x] **Step 3: Run `git diff --check` and inspect only the scoped diff**
- [x] **Step 4: Record PostgreSQL environment blockers separately from code results**
