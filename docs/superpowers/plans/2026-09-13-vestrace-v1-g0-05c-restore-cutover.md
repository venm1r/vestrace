# P05-C Restore and Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore one witnessed P05-B base/WAL set only into a new target directory, carry that target through a reason-bound restore hold, and make activation an explicit, plan-pinned terminal transition.

**Architecture:** The host-only supervisor owns every source/target filesystem action and the PostgreSQL physical tools. PostgreSQL stores only guarded restore attempt, target-generation, freeze, and activation descriptors whose exact successor is witnessed before commit. The active source data directory is never passed to a restore tool or target process.

**Tech Stack:** Rust workspace, PostgreSQL 17 physical base/WAL tools, existing Ed25519 safety journal/witness, sqlx guarded functions, AES-GCM P05-B archive custody, Docker-only disposable source/target qualification.

**Spec:** `docs/superpowers/specs/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation-design.md` P05-C and `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 11.6.

## Global Constraints

- P05-C follows P05-B; it must never rewrite migrations through `0211`, P01-P04 authority, or `scripts/verify-dirty-baseline.mjs`.
- The active database directory is never overwritten, mounted by the target, or accepted as a restore target.
- Source and target operations use absolute configured PostgreSQL tools through `std::process::Command`, never a shell; source DSNs, passwords, archive keys, and plaintext archive members are never logged or journaled.
- A restore selects only a `Streaming` base-complete set and first obtains one non-expiring restore hold.
- A hold releases only with the exact witnessed `TargetInitializationComplete`, atomic `SourceResumePrepared`, `RefusedTargetDestroyed`, or `SalvageInstallationCompleted` receipt.
- Freeze rejects new work, drains only pre-existing fixed identities, records one source timeline/LSN/mutation-watermark checkpoint, then seals the source; no database lock spans filesystem work or a PostgreSQL process.
- Target activation is a one-way CAS bound to one immutable activation-plan digest and target generation. A failed target never overwrites or retires the source.
- Every host target root, source root, archive root, archive-key root, journal root, witness root, bootstrap root, and signing-key path is absolute and pairwise disjoint. None is a Compose-managed archive/key/witness volume.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `crates/vestrace-domain/src/restore_cutover.rs` | Canonical restore-attempt, freeze-point, target-generation, activation-plan, and terminal-reason values. |
| `crates/vestrace-domain/src/installation_safety.rs` | Witness event/state validation for monotonic P05-C successors. |
| `crates/vestrace-application/src/restore_cutover.rs` | Guarded controller ports and exact journal/witness/permit ordering. |
| `crates/vestrace-infrastructure/src/postgres/restore_cutover_repository.rs` | SQLx adapter for the P05-C guarded procedures. |
| `crates/vestrace-infrastructure/src/restore_target/` | Host-only fresh-root validation, archive materialization, and PostgreSQL tool wrapper. |
| `crates/vestrace-cli/src/commands/safety_supervisor.rs` | Explicit supervisor-only restore/freeze/activate commands. |
| `migrations/0212_managed_restore_cutover.sql` | Forward-only assertions for the fixed P05-C guarded installer suffix. |
| `docker/postgres/init-runtime-role.sh` | P05 suffix installer for guarded restore/cutover tables and procedures. |
| `crates/vestrace-*/tests/*restore*` | Domain, guarded-authority, host-custody, and disposable source/target proofs. |

### Task 1: Approve and enforce the P05-C implementation scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`
- Create: `docs/superpowers/plans/2026-09-13-vestrace-v1-g0-05c-restore-cutover.md`

**Interfaces:** Produces the P05-C allowlist before any source, migration, Docker, or test edit.

- [ ] Write a scope test which names every P05-C path and rejects mutations to `0211` and prior authority.
- [ ] Run `node --test tests/p05_scope.test.mjs`; observe RED before the new path list is admitted.
- [ ] Add only the reviewed P05-C paths, sorted, and preserve every protected path.
- [ ] Run `node --test tests/p05_scope.test.mjs`; expect PASS.

### Task 2: Define canonical restore and activation values

**Files:**
- Create: `crates/vestrace-domain/src/restore_cutover.rs`, `crates/vestrace-domain/tests/restore_cutover_contract.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`, `crates/vestrace-domain/src/installation_safety.rs`, `crates/vestrace-domain/src/backup_archive.rs`

**Interfaces:** Produces `RestoreAttemptId`, `RestoreTargetId`, `SourceFreezePoint`, `TargetActivationPlan`, and `RestoreTerminalReceipt`; consumes `BackupSetId`, `DatabaseGenerationId`, and `SafetyJournalDigest`.

- [ ] Write failing contract cases: a target root equal to, inside, or containing the source root is refused; an activation plan cannot change after its digest is recorded; a hold release rejects a bare refusal digest.
- [ ] Run `cargo test -p vestrace-domain --test restore_cutover_contract`; expect compilation failure until the values exist.
- [ ] Implement length-delimited canonical encoding and decode rejection for duplicate IDs, zero LSN, mutable plan fields, and invalid terminal-reason kind.
- [ ] Extend `SafetyEventKind` and `WitnessStateV1` only with monotonic typed P05-C slots. Existing P05-A/B bytes must still decode exactly.
- [ ] Run the contract plus `backup_archive_contract`; expect PASS.

### Task 3: Add guarded restore-attempt and hold-release authority

**Files:**
- Create: `migrations/0212_managed_restore_cutover.sql`, `crates/vestrace-infrastructure/src/postgres/restore_cutover_repository.rs`, `crates/vestrace-infrastructure/tests/restore_cutover_authority.rs`
- Modify: `docker/postgres/init-runtime-role.sh`, `crates/vestrace-infrastructure/src/postgres/mod.rs`, `crates/vestrace-infrastructure/src/postgres/pool.rs`, `tests/p05_provisioner_prefix.test.mjs`

**Interfaces:** Produces guarded `prepare_restore_attempt`, `record_source_freeze`, `record_target_initialized`, and `release_restore_hold` procedures callable only by `vestrace_safety_supervisor`.

- [ ] Write a live authority test proving runtime/raw SQL cannot create attempts, set freeze/target states, change a plan digest, or release a hold.
- [ ] Write SQL assertions that an attempt references one Streaming base-complete set and one active source generation; only one nonterminal target exists per source slot.
- [ ] Implement installer-owned tables/functions with FORCE RLS, guarded-owner ownership, exact receipt checks, and non-owner supervisor EXECUTE grants.
- [ ] Make `0212` assert function signatures, owners, RLS, grants, and predecessor `0211`; do not recreate P05-B objects.
- [ ] Run the isolated PostgreSQL authority test; expect direct DML SQLSTATE `42501` and illegal transitions `23514`.

### Task 4: Implement host-only fresh-target restore and source freeze

**Files:**
- Create: `crates/vestrace-infrastructure/src/restore_target/mod.rs`, `crates/vestrace-infrastructure/src/restore_target/file_target.rs`, `crates/vestrace-cli/tests/safety_restore_host_custody.rs`
- Modify: `crates/vestrace-application/src/restore_cutover.rs`, `crates/vestrace-infrastructure/src/lib.rs`, `crates/vestrace-cli/src/commands/safety_supervisor.rs`, `crates/vestrace-cli/src/main.rs`

**Interfaces:** `RestoreTargetCustody::prepare_fresh(target, archive_manifest)` validates an empty new directory; `RestoreCutoverController::restore_to_target` returns a prepared target identity but cannot activate it.

- [ ] Write host tests that reject relative, existing/nonempty, overlapping, archive/key/witness, and active-source target roots before reading a key or database state.
- [ ] Write a fake absolute PostgreSQL tool test that receives separate arguments, cannot see an active source target, and leaves an unknown target layout for operator inspection.
- [ ] Materialize only the selected encrypted base/WAL members into the fresh target, verify descriptor digest/length and continuous timeline/LSN ancestry, then invoke the physical target tool without a shell.
- [ ] Acquire/record the restore hold before target preparation; on target failure retain the target only for explicit refusal cleanup and never release the hold.
- [ ] Implement `freeze-source` as witnessed Quiescing, completion-only drain check, source checkpoint capture, and guarded source-frozen record. Refuse activation if the final WAL coverage does not reach that exact freeze point.
- [ ] Run host-custody tests; expect PASS.

### Task 5: Bind target initialization and irreversible activation

**Files:**
- Modify: `crates/vestrace-domain/src/restore_cutover.rs`, `crates/vestrace-domain/src/installation_safety.rs`, `crates/vestrace-application/src/restore_cutover.rs`, `crates/vestrace-cli/src/commands/safety_supervisor.rs`
- Test: `crates/vestrace-domain/tests/restore_cutover_contract.rs`, `crates/vestrace-cli/tests/safety_restore_recovery.rs`

**Interfaces:** `TargetActivationPlan::digest()` is signed before `activate_target(attempt, plan_digest)`; only the exact witnessed target initialization receipt can release the hold.

- [ ] Write failing recovery tests for crash after target initialization receipt, stale plan digest, changed target generation, and a second activation attempt.
- [ ] Record target timeline and final applied LSN; require it to be the source freeze timeline/LSN or an authenticated descendant defined by the archived history member.
- [ ] Commit one `TargetActivating` witness successor bound to the plan digest and target generation, then call the guarded activation CAS. Retire no source generation before this CAS commits.
- [ ] On refusal, require target destruction receipt and atomically record `Abandoned + SourceResumePrepared` before release; do not provide a generic `release-hold` CLI command.
- [ ] Run domain and CLI recovery tests; expect PASS.

### Task 6: Qualify source/target behavior and record evidence

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-backup-restore-foundation.md`
- Test: `crates/vestrace-cli/tests/safety_restore_recovery.rs`, `crates/vestrace-infrastructure/tests/restore_cutover_authority.rs`

- [ ] Build a disposable PostgreSQL 17 source and a separate target volume; never mount the source volume into the target container.
- [ ] Capture a P05-B base plus WAL through a witnessed freeze point, restore into the fresh target, and prove the source directory hash is unchanged.
- [ ] Inject failures before target initialization, after target receipt, and before activation CAS; restart must resume only the exact attempt/plan or require refusal cleanup.
- [ ] Run `cargo fmt --check`, P05 scope/provisioner tests, focused domain/application/CLI/infrastructure tests, strict P05 clippy, `git diff --check`, and dirty-baseline verification.
- [ ] Record only actual command exits, fault points, target/source separation evidence, and any inherited baseline blocker.

## Self-review

- Spec coverage: Tasks 2–3 establish monotonic values and guarded authority; Task 4 protects fresh-target restore and freeze/WAL continuity; Task 5 binds activation and terminal release; Task 6 requires isolated failure qualification.
- Placeholder scan: no implementation step delegates a transition without naming its input, durable record, and required refusal condition.
- Type consistency: the plan uses `TargetActivationPlan::digest()`, `RestoreAttemptId`, `SourceFreezePoint`, and `RestoreTerminalReceipt` consistently; Task 3 persists them and Tasks 4–5 consume them.

## Execution handoff

Execute inline, task-by-task, beginning with the scope test. Do not use cdx or start a product server.
