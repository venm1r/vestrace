# P05-B Base-Backup Completion Plan

> **For agentic workers:** Execute this plan task by task. Preserve the inherited dirty tree and stop immediately if an edit requires a path outside the recorded P05 scope amendment.

**Goal:** Complete the missing P05-B PostgreSQL base-backup path so a streaming set has one witnessed, encrypted base archive before any WAL checkpoint or restore hold can make it a restore candidate.

**Architecture:** The host-only safety supervisor invokes an absolute `pg_basebackup` executable without a shell, captures a tar-format base backup under the protected archive root, extracts the base snapshot start LSN and timeline from its `backup_label`, then encrypts and commits each tar member through the existing guarded archive checkpoint path. PostgreSQL receives immutable descriptors, signed entries, and witness receipts only. The base-backup process never runs while a guarded PostgreSQL transaction or installation permit is held.

**Tech stack:** Existing Rust workspace, PostgreSQL 17 `pg_basebackup`, `sqlx`, host filesystem archive custody, existing AES-GCM archive framing, and the existing isolated PostgreSQL qualification harness.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 11.6 and `docs/superpowers/specs/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation-design.md` P05-B.

## Constraints

- This completes P05-B only. It does not restore a target, release a restore hold, quiesce a source, promote PostgreSQL, modify the active generation, or begin P05-C.
- Historical migrations through `0209` remain protected. `0211_managed_backup_base_capture.sql` is forward-only and may assert or extend only installer-owned P05 archive objects.
- `pg_basebackup` is an absolute host executable set through `VESTRACE_PG_BASEBACKUP_BIN`; its source connection string is supplied only through `VESTRACE_PG_BASEBACKUP_SOURCE_DSN` and is never printed, journaled, or passed through a shell.
- The capture directory is a unique exact child of the protected archive root. It is never a Compose volume, target database directory, archive object directory, key root, journal root, witness root, or bootstrap root.
- The supervisor accepts only a successful tar-format capture consisting of regular top-level tar files. It validates the `base.tar` `backup_label`, records its start timeline/LSN, and removes only known exact capture files after their checkpoints commit. An unknown output layout fails closed and is retained for explicit operator inspection.
- A set starts provisional. It has exactly one `BaseChunk` at ordinal one before WAL append, hold acquisition, sealing, deletion preparation, or any claim that the set is restorable. Every subsequent WAL descriptor must follow the recorded base timeline and base start LSN.
- The existing journal/witness/database ordering stays intact for every committed base chunk: reserve, durable host staging, exact guarded CAS, signed journal, witness advance, guarded commit, immutable promotion.

## Task 1: Record and mechanically enforce the completion scope

**Files:**
- Create: `docs/superpowers/plans/2026-09-13-vestrace-v1-g0-05b-base-backup-completion.md`
- Create: `migrations/0211_managed_backup_base_capture.sql`
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`

- [ ] Add only the plan and migration to the P05 allowlist, keep `0209` protected, and record the standing user authorization with the exact two paths.
- [ ] Make the scope test expect `0210` and `0211` as the complete writable migration suffix and reject `0209`.
- [ ] Run `node --test tests/p05_scope.test.mjs`.

## Task 2: Make a base snapshot a closed archive invariant

**Files:**
- Modify: `crates/vestrace-domain/src/backup_archive.rs`, `crates/vestrace-domain/src/installation_safety.rs`, `crates/vestrace-domain/src/lib.rs`
- Modify: `crates/vestrace-application/src/backup_archive.rs`
- Modify: `crates/vestrace-domain/tests/backup_archive_contract.rs`

- [ ] Write failing contract cases: WAL-before-base is refused; a second base object is refused; a restore hold is refused before one base object; a base object has ordinal one; and a base object's start timeline/LSN is retained when a later WAL object is admitted.
- [ ] Replace the WAL-specific command/controller naming with an object-neutral append command while retaining the `append_wal` facade for existing callers. The controller must derive the signed `ArchiveCheckpointCommitted` state from the object descriptor without trusting a caller-provided kind transition.
- [ ] Extend `BackupArchiveStateV1` canonical state with the exact first base descriptor/checkpoint metadata required for the rules above. Preserve decoding of current witnessed P05-B states; an existing nonempty set lacking a base must remain provisional and may not gain a hold or WAL checkpoint.
- [ ] Run `cargo test -p vestrace-domain --test backup_archive_contract`.

## Task 3: Enforce the base boundary in guarded PostgreSQL

**Files:**
- Create: `migrations/0211_managed_backup_base_capture.sql`
- Modify: `docker/postgres/init-runtime-role.sh`, `crates/vestrace-infrastructure/src/postgres/backup_archive_repository.rs`
- Modify: `crates/vestrace-infrastructure/tests/backup_archive_authority.rs`, `tests/p05_provisioner_prefix.test.mjs`

- [ ] Add a P05 suffix installer with security-definer trigger functions that validate first-ordinal/base-before-WAL, one base object, hold readiness, and lifecycle readiness. The normal Rust path rejects bad reservations before host staging; the PostgreSQL triggers defend the immutable commit/hold/lifecycle tables from stale guarded callers. All functions are owned by `vestrace_guarded_owner`, and no direct runtime DML is granted.
- [ ] Make migration `0211` assert the exact installer-owned functions, constraints, owners, RLS, and grants; it must not recreate `0210` objects or alter protected migration text.
- [ ] Add live authority tests for direct DML refusal, WAL-before-base refusal, duplicate-base refusal, and one exact base then WAL chain.
- [ ] Run `cargo test -p vestrace-infrastructure --test backup_archive_authority -- --nocapture` against a clean isolated P05 database migrated through `0211`.

## Task 4: Capture and archive a real base backup on the host

**Files:**
- Modify: `crates/vestrace-cli/src/commands/safety_supervisor.rs`, `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-infrastructure/src/backup_archive/file_store.rs`, `crates/vestrace-infrastructure/src/backup_archive/key_custody.rs`
- Modify: `crates/vestrace-cli/tests/safety_archive_host_custody.rs`, `crates/vestrace-cli/tests/safety_archive_recovery.rs`

- [ ] Add `vestrace safety-supervisor backup capture-base --set-id <uuid>`. It rejects absent/non-absolute/non-executable tool paths and protected-root collisions before it reads authority state.
- [ ] Launch the configured tool through `std::process::Command` with tar output and no shell. Capture standard error only for a bounded diagnostic that excludes the source DSN. No database transaction, advisory permit, or archive append reservation survives across process execution.
- [ ] Parse only the `backup_label` member of `base.tar` using bounded tar-header parsing. Reject malformed sizes, duplicate labels, missing `START WAL LOCATION`, missing timeline filename, overflow, or a changed source identity. Do not unpack or execute archive content.
- [ ] Encrypt each validated base tar file as a `BaseChunk` using the existing per-set key and generic append path. Remove a capture file only after its exact immutable archive promotion succeeds; failure leaves either a named uncommitted staging identity for reconciliation or the exact protected capture path for operator inspection.
- [ ] Keep `append-wal` host-file behavior, but require the committed base checkpoint and verify its timeline/LSN ancestry before it can reserve an ordinal.
- [ ] Add integration tests with a harmless fake `pg_basebackup` executable for argument isolation, output-layout refusal, `backup_label` parsing, and exact cleanup. Add a real Linux-qualified path that invokes PostgreSQL 17 `pg_basebackup` when the harness provides its absolute tool path and source DSN; otherwise print `BLOCKED` and make no availability claim.

## Task 5: Qualify the completed P05-B boundary

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-backup-restore-foundation.md`

- [ ] Run `cargo fmt --check` and `node --test tests/p05_scope.test.mjs tests/p05_provisioner_prefix.test.mjs`.
- [ ] Run domain, host-custody, recovery, and live archive-authority targets. The live evidence must show one real base checkpoint before WAL, guarded raw-DML refusal, and no target restore/promotion action.
- [ ] Run `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings` and `git -c safe.directory=E:/Soft/vestrace diff --check`.
- [ ] Run `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`; if inherited out-of-scope dirt blocks it, record the exact blocker without changing the baseline.

## Execution handoff

Task 1 is authorized by the user's standing scope authorization and begins immediately. Subsequent tasks start only after the preceding focused checks pass. A successful fake-tool test is not evidence of a usable recovery base; P05-C remains blocked until the real source/target qualification required by section 11.6 exists.
