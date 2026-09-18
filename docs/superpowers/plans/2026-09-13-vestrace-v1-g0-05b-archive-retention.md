# P05-B Archive and Retention Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add host-owned, independently encrypted PostgreSQL base/WAL archives whose witnessed checkpoint, restore holds, sealing, and deletion preparation remain monotonic and fail closed.

**Architecture:** P05-B extends the P05-A signed journal and complete `WitnessStateV1`; it does not create another safety authority. A host-supervisor archive controller owns archive-root paths, archive-key envelopes, PostgreSQL base-backup invocation, WAL staging, and reconciliation. PostgreSQL holds only immutable metadata, lifecycle/hold/append intent heads, and guarded transition functions; it never receives archive keys or archive-root credentials.

**Tech Stack:** Rust 1.85 workspace; PostgreSQL 17 with the existing guarded-owner/RLS installer pattern; `ring::aead::AES_256_GCM`; SHA-256; `sqlx`; host filesystem object-store adapter; Docker Compose only for the existing PostgreSQL test image.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` §11.6 and `docs/superpowers/specs/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation-design.md` P05-B.

## Global Constraints

- This is P05-B only: it implements archive/retention, not P05-C restore/cutover, P05-D deployment, archive export, or full G0 qualification.
- P01, P02, P03, P04, P05-A, `scripts/verify-dirty-baseline.mjs`, `scripts/p04-scope.mjs`, every historical migration through `0208`, and `migrations/0209_installation_safety_authority.sql` are protected and must remain byte-for-byte unchanged.
- No source, migration, Compose, Docker, scope, preflight, test, or evidence path in this plan is authorized until Task 1's explicit scope amendment is reviewed and approved by the user.
- The archive root, archive-key root, witness root, bootstrap root, and journal signing key are absolute protected host paths outside every Compose-managed volume. Docker Compose must never mount or create an archive/key/witness root.
- Each backup set has a random independent 32-byte archive key. AES-256-GCM associated data binds installation ID, backup-set ID, object kind, ordinal, plaintext SHA-256, timeline, and LSN range. A ciphertext/key envelope is never shared between sets.
- A base/WAL object is restorable only after file/object durability, immutable digest verification, guarded head CAS, and the exact signed journal entry plus witness receipt for its checkpoint. A staged object is not an archive member.
- Every archive transition acquires `InstallationMutationPermit::Exclusive` before a backup-set guard. No database lock spans host encryption, object I/O, PostgreSQL base-backup streaming, directory fsync, archive-key creation, or deletion.
- Append requires a set-bound `BackupArchiveAppendPermit`, lifecycle `Streaming`, expected immutable head, no seal/deletion preparation, and one unique staged-object intent. Sealing/deletion blocks all later append, verification, and restore-hold acquisition.
- Restore acquisition is allowed only from `Streaming`; it CAS-creates a non-expiring restore hold. A worker timeout, lease expiry, raw SQL edit, or ordinary `Refused` label cannot release one.
- A hold may release only with one witnessed terminal reason: `TargetInitializationComplete`, `SourceResumePrepared`, `RefusedTargetDestroyed`, or `SalvageInstallationCompleted`. P05-B implements data structures and guarded release validation only; P05-C creates target/freeze receipts.
- Retention may record `DeletionRequested` while blocked, but only a hold-free, drained `Sealed` set can obtain `ManagedBackupDeletionPrepared`. Archive-key erasure and object removal are P05-B reconciliation/finalization operations with exact witnessed receipts; automatic retention must keep another verified `Streaming` set.

---

## Current P05-A inventory and intended boundaries

`crates/vestrace-domain/src/installation_safety.rs` supplies closed `InstallationId`, `DatabaseGenerationId`, `SafetyJournalDigest`, `SignedJournalEntry`, `WitnessHead`, `WitnessReceipt`, `WitnessStateV1`, and the two current event kinds. Its `WitnessStateV1` deliberately contains empty canonical slots for WAL checkpoints and backup/hold lifecycle. `SafetyAuthorityService` in `crates/vestrace-application/src/installation_safety.rs` already enforces exclusive permit → durable journal → witness advance → guarded SQL → commit. `FileSafetyJournal` and `FileInstallationSafetyWitness` already use durable host records; `PgSafetyAuthorityRepository` is the current guarded P05-A repository. `vestrace safety-supervisor` is host-only and receives protected roots through absolute-path environment variables.

P05-B must add archive-specific types, ports, and guarded functions beside those interfaces. It must not turn the journal, witness, runtime role, or ordinary product CLI into an archive store. The P05-A repository remains responsible only for initialization and generation registration.

## Proposed scope amendment, required before implementation

The implementation worker must stop after preparing this exact amendment proposal and obtain a recorded user approval before editing any listed path. The approval record must identify the approved paths, purpose, and date; it must be added to `docs/development-evidence/v1-g0-05-preflight.json` only after approval. Then update `scripts/p05-scope.mjs` and `tests/p05_scope.test.mjs` together, preserving sorted/disjoint arrays and the protected authority list.

Proposed new paths:

```text
crates/vestrace-domain/src/backup_archive.rs
crates/vestrace-domain/src/installation_safety.rs
crates/vestrace-domain/src/lib.rs
crates/vestrace-domain/tests/backup_archive_contract.rs
crates/vestrace-application/src/backup_archive.rs
crates/vestrace-application/src/lib.rs
crates/vestrace-infrastructure/src/backup_archive/
crates/vestrace-infrastructure/src/postgres/backup_archive_repository.rs
crates/vestrace-infrastructure/src/postgres/mod.rs
crates/vestrace-infrastructure/src/lib.rs
crates/vestrace-infrastructure/tests/backup_archive_authority.rs
crates/vestrace-cli/src/commands/safety_supervisor.rs
crates/vestrace-cli/src/main.rs
crates/vestrace-cli/tests/safety_archive_host_custody.rs
crates/vestrace-cli/tests/safety_archive_recovery.rs
docker/postgres/init-runtime-role.sh
migrations/0210_managed_backup_archive_retention.sql
scripts/p05-scope.mjs
tests/p05_scope.test.mjs
tests/p05_provisioner_prefix.test.mjs
docs/development-evidence/v1-g0-05-preflight.json
docs/development-evidence/v1-g0-05-backup-restore-foundation.md
```

The scope amendment explicitly excludes `docker-compose.yml`: P05-B must prove host custody with an inventory test and must not add an archive service or archive volume. It also excludes `0209`; the bootstrap installer extension belongs after its existing body and migration `0210` only asserts the provisioned archive objects.

### Task 1: Approve and mechanically enforce the P05-B scope amendment

**Files:**
- Modify after user approval: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`
- Test after user approval: `tests/p05_scope.test.mjs`

**Interfaces:**
- Consumes: the exact path proposal above and the existing `changeScopePaths`/`protectedAuthorityPaths` exports.
- Produces: a reviewed allowlist that admits P05-B files while retaining every P01–P05-A authority and `0209` as protected.

- [ ] **Step 1: Write the failing scope test before changing the allowlist.**

```js
test('P05-B archive paths require an explicit approved amendment', () => {
  assert.equal(changeScopePaths.includes('migrations/0210_managed_backup_archive_retention.sql'), false);
  assert.equal(changeScopePaths.includes('migrations/0209_installation_safety_authority.sql'), false);
});
```

- [ ] **Step 2: Run the test and record the expected initial pass.**

Run: `node --test tests/p05_scope.test.mjs`

Expected: PASS because P05-A has not silently authorized P05-B.

- [ ] **Step 3: Obtain the user’s explicit approval, then make the bounded amendment.**

Add only the approved proposed paths to sorted `changeScopePaths`; retain protected paths including `migrations/0209_installation_safety_authority.sql`. Add an amendment object with the user’s approval timestamp, approver, rationale, and precisely the same paths to the preflight JSON. Replace the negative assertion with:

```js
assert.ok(changeScopePaths.includes('migrations/0210_managed_backup_archive_retention.sql'));
assert.ok(protectedAuthorityPaths.includes('migrations/0209_installation_safety_authority.sql'));
```

- [ ] **Step 4: Verify scope and baseline protection.**

Run: `node --test tests/p05_scope.test.mjs`

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`

Expected: both exit 0; a fixture mutation of `0209` remains rejected.

### Task 2: Define canonical backup, hold, and archive-head value types

**Files:**
- Create: `crates/vestrace-domain/src/backup_archive.rs`
- Modify: `crates/vestrace-domain/src/installation_safety.rs`, `crates/vestrace-domain/src/lib.rs`
- Test: `crates/vestrace-domain/tests/backup_archive_contract.rs`

**Interfaces:**
- Consumes: `InstallationId`, `DatabaseGenerationId`, `SafetyJournalDigest`, `WitnessStateV1`.
- Produces: `BackupSetId`, `BackupObjectId`, `BackupSetLifecycle`, `ArchiveObjectKind`, `ArchiveObjectDescriptor`, `ArchiveHead`, `WalArchiveCheckpoint`, `RestoreHold`, `RestoreHoldReleaseReason`, `BackupArchiveStateV1`, and the distinct signed `SafetyEventKind::ArchiveCheckpointCommitted` transition.

- [ ] **Step 1: Write failing contract tests for closed transitions and canonical bytes.**

```rust
#[test]
fn sealing_refuses_a_hold_or_append_and_checkpoint_cas_needs_the_exact_head() {
    let streaming = fixture_streaming_set();
    assert!(streaming.acquire_hold(RestoreHoldId::new()).is_ok());
    assert_eq!(streaming.begin_sealing(), Err(BackupArchiveError::RestoreHoldPresent));
    let sealed = fixture_sealed_set();
    assert_eq!(sealed.acquire_hold(RestoreHoldId::new()), Err(BackupArchiveError::NotRestorable));
    assert_eq!(sealed.reserve_append(fixture_head()), Err(BackupArchiveError::NotStreaming));
}

#[test]
fn checkpoint_bytes_bind_set_object_timeline_lsn_and_predecessor_digest() {
    let checkpoint = fixture_checkpoint();
    assert_ne!(checkpoint.canonical_bytes(), checkpoint.with_lsn(42).canonical_bytes());
    assert_ne!(checkpoint.digest(), checkpoint.with_predecessor(other_digest()).digest());
}
```

- [ ] **Step 2: Run the contract target to observe missing symbols.**

Run: `cargo test -p vestrace-domain --test backup_archive_contract`

Expected: FAIL because `BackupSetId`, lifecycle types, and checkpoint constructors do not exist.

- [ ] **Step 3: Implement closed types and encode the archive witness slot.**

Define `BackupSetLifecycle::{Streaming,Sealing,Sealed,DeletionPrepared,ArchiveKeyErased,Deleted}` and permit only `Streaming -> Sealing -> Sealed -> DeletionPrepared -> ArchiveKeyErased -> Deleted`; `DeletionRequested` is a request flag, never a lifecycle shortcut. Define `ArchiveObjectKind::{BaseChunk,WalSegment,TimelineHistory}` and a descriptor containing set/object IDs, kind, ordinal, timeline, start/end LSN, ciphertext digest, plaintext digest, length, and predecessor head digest. Make every constructor validate lengths, ordered LSN range, nonzero ordinal, and lifecycle-compatible data.

Replace the two opaque P05-A `WitnessStateV1` archive byte fields with one versioned `BackupArchiveStateV1` canonical encoding containing: per-set immutable identity/current head, hold IDs and immutable release evidence, and no archive key, object path, plaintext, or credential. Preserve the existing field position in `WitnessStateV1::canonical_bytes`; decoder rejects unknown version, duplicate set/hold ID, invalid lifecycle, non-monotonic checkpoint ordinal, or trailing bytes.

Add `ArchiveCheckpointCommitted` as the only event that can carry one exact
checkpoint successor. `WitnessHead::accept_signed` rejects archive-state
changes under the P05-A events and rejects the archive event unless generation,
epoch, sequence, and the single checkpoint successor all match exactly.

- [ ] **Step 4: Add exact public signatures and export them.**

```rust
pub struct ArchiveHead { pub checkpoint_ordinal: u64, pub digest: SafetyJournalDigest }
pub struct WalArchiveCheckpoint { /* validated private fields */ }
pub enum RestoreHoldReleaseReason {
    TargetInitializationComplete { receipt: SafetyJournalDigest },
    SourceResumePrepared { receipt: SafetyJournalDigest },
    RefusedTargetDestroyed { receipt: SafetyJournalDigest },
    SalvageInstallationCompleted { receipt: SafetyJournalDigest },
}
impl BackupArchiveStateV1 {
    pub fn reserve_append(&self, set: BackupSetId, expected: ArchiveHead, object: ArchiveObjectDescriptor) -> Result<ArchiveAppendReservation, BackupArchiveError>;
    pub fn commit_checkpoint(&self, reservation: ArchiveAppendReservation, checkpoint: WalArchiveCheckpoint) -> Result<Self, BackupArchiveError>;
    pub fn acquire_restore_hold(&self, set: BackupSetId, hold: RestoreHoldId) -> Result<Self, BackupArchiveError>;
}
```

- [ ] **Step 5: Run focused tests and formatting.**

Run: `cargo test -p vestrace-domain --test backup_archive_contract`

Run: `cargo fmt --check`

Expected: both exit 0.

### Task 3: Add archive application ports and witnessed controller ordering

**Files:**
- Create: `crates/vestrace-application/src/backup_archive.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: `crates/vestrace-domain/tests/backup_archive_contract.rs`

**Interfaces:**
- Consumes: Task 2 value types; existing `InstallationMutationPermit`, `SafetyJournal`, `InstallationSafetyWitness`, `InstallationSupervisorContext`, `UnitOfWork`.
- Produces: `BackupArchiveRepository`, `BackupObjectStore`, `ArchiveKeyCustody`, `BackupArchiveController`, and typed application errors.

`ArchiveCheckpointCommitted` from Task 2 is the sole signed event this task
may advance. Backup-set start and the later lifecycle event families remain
outside this task.

- [ ] **Step 1: Extend the contract test with ordered fake ports.**

```rust
#[tokio::test]
async fn append_orders_permit_stage_fsync_cas_journal_witness_and_commit() {
    let trace = Trace::default();
    fixture_controller(trace.clone()).append_wal(fixture_append()).await.unwrap();
    assert_eq!(trace.events(), ["exclusive", "reserve", "stage", "durable", "cas", "journal", "witness", "commit"]);
}
```

- [ ] **Step 2: Run the test and confirm the controller is absent.**

Run: `cargo test -p vestrace-domain --test backup_archive_contract append_orders_permit_stage_fsync_cas_journal_witness_and_commit`

Expected: FAIL because `BackupArchiveController` and its ports do not exist.

- [ ] **Step 3: Define narrow host and guarded repository ports.**

```rust
#[async_trait]
pub trait BackupObjectStore: Send + Sync {
    async fn stage_encrypted(&self, write: ArchiveObjectWrite) -> Result<StagedArchiveObject, ApplicationError>;
    async fn verify_durable(&self, staged: &StagedArchiveObject) -> Result<(), ApplicationError>;
    async fn remove_orphan(&self, staged: &StagedArchiveObject) -> Result<(), ApplicationError>;
    async fn remove_committed(&self, object: &ArchiveObjectDescriptor) -> Result<(), ApplicationError>;
}
#[async_trait]
pub trait ArchiveKeyCustody: Send + Sync {
    async fn create_set_key(&self, set: BackupSetId) -> Result<ArchiveKeyEnvelopeRef, ApplicationError>;
    async fn erase_prepared_key(&self, prepared: ManagedBackupDeletionPrepared) -> Result<ArchiveKeyErasureReceipt, ApplicationError>;
}
#[async_trait]
pub trait BackupArchiveRepository: Send + Sync {
    async fn reserve_append_in(&self, uow: &mut dyn UnitOfWork, command: ReserveArchiveAppend) -> Result<ArchiveAppendReservation, ApplicationError>;
    async fn commit_checkpoint_in(&self, uow: &mut dyn UnitOfWork, command: CommitArchiveCheckpoint, entry: SignedJournalEntry, receipt: WitnessReceipt) -> Result<BackupArchiveSnapshot, ApplicationError>;
    async fn acquire_hold_in(&self, uow: &mut dyn UnitOfWork, command: AcquireRestoreHold, entry: SignedJournalEntry, receipt: WitnessReceipt) -> Result<BackupArchiveSnapshot, ApplicationError>;
}
```

`ArchiveObjectWrite` contains encrypted bytes and immutable metadata, never plaintext after the adapter boundary. `ArchiveKeyEnvelopeRef` is opaque and carries no key bytes.

- [ ] **Step 4: Implement controller methods with fixed failure handling.**

`append_wal` acquires an exclusive installation permit, asks guarded SQL to reserve one unique intent, invokes no database operation while staging/fsyncing, verifies the staged digest, reacquires the same permit-owned unit of work, CAS-commits the exact reservation/head, appends one signed `WalArchiveCheckpoint` journal entry, advances the exact witness state, and commits. On a stage failure it records no checkpoint; on a post-stage CAS conflict it removes only the exact orphan staging identity; on a post-witness commit failure it reconciles only the same signed entry/receipt pair.

`acquire_restore_hold`, `begin_sealing`, `seal_after_drain`, `prepare_deletion`, `erase_prepared_key`, and `finalize_deletion` follow the same receipt order and never synthesize a P05-C terminal receipt.

- [ ] **Step 5: Run contract tests.**

Run: `cargo test -p vestrace-domain --test backup_archive_contract`

Expected: PASS, including failure injections that prove no duplicate ordinal/head or post-seal append.

### Task 4: Implement host archive/key custody and encrypted staged objects

**Files:**
- Create: `crates/vestrace-infrastructure/src/backup_archive/mod.rs`, `crates/vestrace-infrastructure/src/backup_archive/file_store.rs`, `crates/vestrace-infrastructure/src/backup_archive/key_custody.rs`
- Modify: `crates/vestrace-infrastructure/src/lib.rs`
- Test: `crates/vestrace-cli/tests/safety_archive_host_custody.rs`

**Interfaces:**
- Consumes: Task 3 `BackupObjectStore` and `ArchiveKeyCustody`; `ring::aead::AES_256_GCM` convention from `content_material_codec.rs`.
- Produces: `FileBackupObjectStore::open(PathBuf)` and `FileArchiveKeyCustody::open(PathBuf)`.

- [ ] **Step 1: Write host-custody and corruption tests.**

```rust
#[test]
fn staged_archive_needs_file_and_parent_sync_before_it_can_be_verified() { /* inject sync failure; expect Unavailable */ }
#[test]
fn ciphertext_cannot_open_with_another_set_or_changed_object_metadata() { /* AAD mismatch */ }
#[test]
fn committed_object_uses_create_new_and_orphan_cleanup_cannot_delete_a_committed_member() { /* exact identity */ }
```

- [ ] **Step 2: Run the new test target.**

Run: `cargo test -p vestrace-cli --test safety_archive_host_custody`

Expected: FAIL because the archive adapters and environment inventory do not exist.

- [ ] **Step 3: Implement durable local-object semantics.**

Use roots `staging/<set>/<intent>` and `objects/<set>/<kind>/<ordinal>-<digest>`. Create staging files with `create_new`; write the self-describing versioned encrypted frame; call `sync_all` on the file and parent; then create the immutable final object with `create_new`, fsync it and its parent, and remove the staging file only after the guarded checkpoint commits. On Windows, parent directory flush failure returns `ApplicationError::Unavailable`, matching the existing witness/bootstrap fail-closed rule.

Generate one set key with `SystemRandom`, keep it only in `Zeroizing<[u8; 32]>` during encryption, and store an encrypted/opaque envelope under the separate archive-key root using create-only records. Reuse the repository’s AES-256-GCM framing practices but use a new archive frame domain `vestrace-managed-backup-object-v1`; do not reuse a content-material identifier or key envelope.

- [ ] **Step 4: Add an inventory test that rejects Compose custody.**

The test runs `docker compose config`, asserts no service volume/source contains `VESTRACE_BACKUP_ARCHIVE_ROOT` or `VESTRACE_BACKUP_ARCHIVE_KEY_ROOT`, and asserts `safety-supervisor` remains a host CLI subcommand rather than a service.

- [ ] **Step 5: Run focused tests.**

Run: `cargo test -p vestrace-cli --test safety_archive_host_custody`

Expected: PASS.

### Task 5: Provision guarded PostgreSQL archive metadata and RLS functions

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/backup_archive_repository.rs`, `crates/vestrace-infrastructure/tests/backup_archive_authority.rs`, `migrations/0210_managed_backup_archive_retention.sql`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`, `crates/vestrace-infrastructure/src/lib.rs`, `docker/postgres/init-runtime-role.sh`, `tests/p05_provisioner_prefix.test.mjs`

**Interfaces:**
- Consumes: Tasks 2–3 commands and the existing P05 supervisor transaction context/verifier.
- Produces: `PgBackupArchiveRepository` and installer-owned tables/functions asserted by migration `0210`.

- [ ] **Step 1: Write live authority tests before DDL.**

```rust
#[tokio::test]
async fn runtime_cannot_insert_head_hold_or_deletion_preparation() { /* runtime SQL gets 42501 */ }
#[tokio::test]
async fn append_cas_accepts_one_exact_staged_intent_and_rejects_second_ordinal() { /* two supervisor transactions */ }
#[tokio::test]
async fn sealing_vs_hold_has_one_winner_and_non_streaming_sets_refuse_holds() { /* concurrent race */ }
```

- [ ] **Step 2: Run live target with the existing isolated P05 image.**

Run: `cargo test -p vestrace-infrastructure --test backup_archive_authority -- --nocapture`

Expected: FAIL because `0210` objects and guarded functions are absent.

- [ ] **Step 3: Add installer-owned archive tables and immutable constraints.**

Extend only the P05 suffix of `init-runtime-role.sh` with a no-argument guarded-owner installer `vestrace_install_p05_backup_archive_schema()`. It creates: `managed_backup_sets`, `managed_backup_archive_heads`, `managed_backup_archive_objects`, `managed_backup_append_intents`, `managed_backup_restore_holds`, `managed_backup_deletion_preparations`, and append-only `managed_backup_events`. Every table is FORCE RLS, owned by `vestrace_guarded_owner`, and has no runtime/supervisor direct DML grant.

Use unique `(backup_set_id, ordinal)`, unique object digest per set, immutable object/hold/event triggers, FK ties from checkpoint/event to the exact P05-A installation/generation/head, and lifecycle check constraints. Store envelope reference/digest only, never a key or object-store path. `0210` must only assert all exact tables, owners, RLS, grants, and guarded procedure signatures exist; it creates no archive state object.

- [ ] **Step 4: Add guarded functions and repository binding.**

All outer functions are `SECURITY DEFINER`, call `public.vestrace_assert_installation_supervisor_context()` first, set only `pg_catalog, public`, and verify the canonical P05 witness receipt using the pinned key before reading archive state. Implement `vestrace_reserve_backup_archive_append`, `vestrace_commit_backup_archive_checkpoint`, `vestrace_acquire_managed_backup_restore_hold`, `vestrace_begin_managed_backup_sealing`, `vestrace_commit_managed_backup_sealed`, `vestrace_prepare_managed_backup_deletion`, `vestrace_record_archive_key_erased`, and `vestrace_finalize_managed_backup_deleted`.

Each function locks the set row, validates exact installation/generation/epoch/journal head and expected archive head, checks the lifecycle and hold/append-intent blockers, and appends one immutable event. `commit_checkpoint` requires the same reserve intent, object digest, predecessor digest, ordinal, timeline/LSN, signed entry, and receipt; no caller key or unconstrained receipt is accepted. The Rust repository uses only these guarded calls and repeats typed entry/receipt verification before binding.

- [ ] **Step 5: Run live/RLS tests.**

Run: `cargo test -p vestrace-infrastructure --test backup_archive_authority -- --nocapture`

Expected: PASS, including raw SQL refusal, single-winner CAS, exact receipt refusal, and migration assertion checks.

### Task 6: Compose host supervisor archive commands and reconciliation

**Files:**
- Modify: `crates/vestrace-cli/src/commands/safety_supervisor.rs`, `crates/vestrace-cli/src/main.rs`
- Create: `crates/vestrace-cli/tests/safety_archive_recovery.rs`
- Test: `crates/vestrace-cli/tests/safety_archive_recovery.rs`

**Interfaces:**
- Consumes: `BackupArchiveController`, `FileBackupObjectStore`, `FileArchiveKeyCustody`, existing bootstrap/witness/journal roots.
- Produces: host-only `vestrace safety-supervisor backup begin|append-wal|acquire-hold|seal|prepare-delete|reconcile` commands.

- [ ] **Step 1: Write subprocess failure/recovery tests.**

```rust
#[test]
fn crash_after_stage_before_checkpoint_reconcile_removes_only_the_orphan() { /* no archive member/event */ }
#[test]
fn crash_after_witness_before_checkpoint_commit_reuses_the_exact_receipt() { /* one ordinal and one journal sequence */ }
#[test]
fn seal_and_hold_race_leaves_exactly_one_streaming_or_sealing_outcome() { /* no lapsed hold */ }
```

- [ ] **Step 2: Run the test target.**

Run: `cargo test -p vestrace-cli --test safety_archive_recovery -- --nocapture`

Expected: FAIL because the archive command tree and controller composition are absent.

- [ ] **Step 3: Wire only host-owned roots and fixed command semantics.**

Require absolute `VESTRACE_BACKUP_ARCHIVE_ROOT` and `VESTRACE_BACKUP_ARCHIVE_KEY_ROOT` in `Roots::load`; reject a path equal to or below journal/witness/bootstrap roots. `backup begin` creates a set/key envelope under the exclusive permit and emits the witnessed start state only after the base snapshot metadata is committed. `append-wal` accepts a supervisor-provided local segment path, streams/encrypts/stages it, and never exposes the archive key. `reconcile` scans only staged intents named by PostgreSQL and witness state, completing the exact commit or removing only exact uncommitted staging objects.

No command mounts a target, restores a base backup, promotes PostgreSQL, changes the active generation, releases a P05-C hold reason, or accepts ordinary runtime credentials.

- [ ] **Step 4: Run host/recovery tests.**

Run: `cargo test -p vestrace-cli --test safety_archive_host_custody --test safety_archive_recovery -- --nocapture`

Expected: PASS; tests skip only when their disposable P05 database variables are absent and print an explicit `BLOCKED` reason.

### Task 7: Implement sealing, deletion preparation, and key/object finalization

**Files:**
- Modify: Task 3 application port/controller, Task 5 repository/installer/tests, Task 6 supervisor/recovery tests
- Test: `crates/vestrace-infrastructure/tests/backup_archive_authority.rs`, `crates/vestrace-cli/tests/safety_archive_recovery.rs`

**Interfaces:**
- Consumes: verified `Streaming` set with CAS head, append intent inventory, and restore holds.
- Produces: hold-safe `Sealed`, `DeletionPrepared`, `ArchiveKeyErased`, and `Deleted` outcomes.

- [ ] **Step 1: Add failing retention boundary cases.**

```rust
#[tokio::test]
async fn deletion_request_is_recorded_but_cannot_seal_while_hold_or_append_exists() { /* typed blocker */ }
#[tokio::test]
async fn prepared_deletion_refuses_restore_and_append_before_key_erasure() { /* raw and guarded */ }
#[tokio::test]
async fn automatic_retention_refuses_to_seal_the_last_verified_streaming_set() { /* explicit conflict */ }
```

- [ ] **Step 2: Run the targeted tests and observe failures.**

Run: `cargo test -p vestrace-infrastructure --test backup_archive_authority deletion`

Expected: FAIL until the guarded lifecycle functions exist.

- [ ] **Step 3: Implement lifecycle operations with exact receipts.**

`begin_sealing` atomically refuses new append permits, leaves a durable `seal_pending` blocker while permits/intents exist, and records the immutable final branch/head only after drain. `commit_sealed` requires no holds and no active intent. `prepare_deletion` requires sealed/no-hold state and records `ManagedBackupDeletionPrepared`, making restore, append, and verification fail immediately. The controller then issues `PreparedArchiveKeyErasure`, calls custody erasure without a DB lock, issues/records `ArchiveKeyErased`, removes only manifest-verified committed objects, and finalizes `ManagedBackupDeleted` with the same receipt chain. A retry resumes the one prepared identity; it never reuses another set's key or deletes a member whose digest/index does not match.

- [ ] **Step 4: Run lifecycle tests.**

Run: `cargo test -p vestrace-infrastructure --test backup_archive_authority -- --nocapture`

Run: `cargo test -p vestrace-cli --test safety_archive_recovery -- --nocapture`

Expected: both exit 0.

### Task 8: Qualify the isolated archive lifecycle and record bounded evidence

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-backup-restore-foundation.md`
- Test: existing and Tasks 4–7 targets

**Interfaces:**
- Consumes: a disposable P05-qualified PostgreSQL database, protected host archive/key/witness/journal roots, and the real guarded installer.
- Produces: P05-B evidence only; no claim of restore/cutover availability.

- [ ] **Step 1: Run static boundary and focused domain/application checks.**

Run: `cargo fmt --check`

Run: `node --test tests/p05_scope.test.mjs tests/p05_provisioner_prefix.test.mjs`

Run: `cargo test -p vestrace-domain --test backup_archive_contract`

Expected: exit 0.

- [ ] **Step 2: Run isolated PostgreSQL and host-supervisor qualification.**

Run: `cargo test -p vestrace-infrastructure --test backup_archive_authority -- --nocapture`

Run: `cargo test -p vestrace-cli --test safety_archive_host_custody --test safety_archive_recovery -- --nocapture`

Expected: explicit live proof of one base/WAL checkpoint chain, wrong-digest/timeline/LSN rejection, direct-DML refusal, hold-vs-seal winner, staged-object crash convergence, prepared-deletion refusal, and set-isolated key/object removal.

- [ ] **Step 3: Run workspace quality and preserved-baseline checks.**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`

Expected: exit 0. Do not claim a successful test whose environment emitted `BLOCKED`.

- [ ] **Step 4: Record only observed P05-B evidence.**

Record commands, exits, test counts, actual environment boundaries, and the verified absence of archive/key roots from Compose. State explicitly that P05-C restore, target promotion, source freeze/drain, archive export, and full-G0 qualification remain untested and unclaimed.

## Self-review

- [ ] **Spec coverage:** §11.6 base/WAL encryption and independent set keys map to Tasks 2 and 4; append permits/CAS/checkpoints map to Tasks 2, 3, and 5; restore holds and reason-bound release map to Tasks 2, 5, and 7; sealing/deletion preparation/key erasure map to Task 7; host-only ownership and fault qualification map to Tasks 4, 6, and 8. P05-C target/freeze work is expressly excluded.
- [ ] **Placeholder scan:** search the completed plan for the forbidden placeholder markers and generic test instructions; replace any occurrence with a named file, signature, command, and expected result before execution.
- [ ] **Type consistency:** verify every Task 3/5/6 command uses the exact Task 2 `BackupSetId`, `ArchiveHead`, `RestoreHoldReleaseReason`, and `WalArchiveCheckpoint` names, and that each guarded function consumes a `SignedJournalEntry` plus `WitnessReceipt`.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-09-13-vestrace-v1-g0-05b-archive-retention.md`. Execution must first obtain the Task 1 scope-amendment approval; only then may a worker begin Task 1 and proceed task-by-task with review gates.
