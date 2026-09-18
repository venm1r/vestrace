# P05 Backup and Restore Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first independently testable P05 safety-authority slice: a
monotonic host witness and a guarded PostgreSQL journal/generation head, with
no backup or restore command exposed until their later dedicated slices.

**Architecture:** P05-A introduces a narrowly typed application authority for
the safety head, an fsyncing host witness adapter, and a guarded PostgreSQL
repository that accepts only an exact witness receipt.  The later archive,
restore, and Compose subprojects consume this authority rather than creating
parallel state, locks, or activation flags.

**Tech Stack:** Rust 2024 workspace, Tokio, SQLx/PostgreSQL 17, Ed25519
receipt verification already used by the security foundation, Docker Compose,
Node.js built-in test runner, and the existing dirty-baseline verifier.

**Spec:** `docs/superpowers/specs/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation-design.md`

## Global Constraints

- Section 11.6 of `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` is the frozen behavioral authority.
- Root `PLAN.md`, all P01-P04 authority, and every migration through `0208` are protected and must remain byte-for-byte unchanged.
- Preserve the complete existing dirty tree, including P04 work and unrelated `Cargo.toml`/`LICENSE-APACHE` drift; do not stage, commit, push, deploy, or pull.
- P05-A does not create a backup object, archive key, restore target, target volume, public API route, or an ordinary runtime access path to the host witness.
- The journal root is a dedicated Compose safety volume outside PostgreSQL data and backups. The witness, archive root, and protected signing/fingerprint keys are separate host authorities outside all Compose volumes; Compose cannot synthesize them.
- The witness receipt has its own Ed25519 signing key and pinned public key; its signature covers the canonical receipt including journal digest, sequence, generation, and activation epoch.
- Only the `vestrace_safety_supervisor` database login may use the private fixed installation-supervisor context; runtime code cannot gain safety authority by choosing its UUID values.
- Existing `InstallationMutationPermit` ordering is preserved: permit before any database guard. P05-A does not add a second global lock.
- Safety initialization refuses until P04's committed `embedding_legacy_retirement_gate` exists; it is created only by `vestrace_commit_legacy_plaintext_retirement()`.
- Scope expansion beyond the five P05 planning paths requires explicit user authorization and a recorded `scope_amendments` entry before the path is edited.

---

## File Structure

The current P05 scope contains only the plan, design, scope module, scope test,
and preflight capture.  The paths below are the exact proposed P05-A source
surface; they are **not writable** until the user approves the corresponding
scope amendment.

| Path | Responsibility |
| --- | --- |
| `crates/vestrace-domain/src/installation_safety.rs` | Immutable IDs, event vocabulary, chain digest, and full versioned witness-state value types. |
| `crates/vestrace-application/src/installation_safety.rs` | `InstallationSafetyWitness` and `SafetyAuthorityRepository` ports plus service commands. |
| `crates/vestrace-infrastructure/src/safety/journal_file.rs` | Signed append-only journal adapter for the dedicated Compose safety volume. |
| `crates/vestrace-infrastructure/src/safety/witness_file.rs` | Host-only atomic/fsync witness-record adapter. |
| `crates/vestrace-infrastructure/src/postgres/safety_authority_repository.rs` | Scoped PostgreSQL guarded-function adapter. |
| `crates/vestrace-cli/src/commands/safety_supervisor.rs` | Host-only `vestrace safety-supervisor` command that composes the witness and repository; it has no HTTP listener. |
| `migrations/0209_installation_safety_authority.sql` | Forward-only RLS tables, guarded functions, and runtime-role denials. |
| `crates/vestrace-infrastructure/tests/installation_safety_authority.rs` | PostgreSQL receipt, RLS, and retry integration tests. |
| `crates/vestrace-cli/tests/safety_supervisor_witness.rs` | Fsync, conflict, corruption, and restart unit/integration tests. |
| `docker-compose.yml` | Dedicated journal volume and journal-initializer service; no product runtime mount. |
| `crates/vestrace-cli/tests/safety_journal_compose.rs` | Compose mount/readiness inventory and normalized-config proof. |
| `docker/postgres/vestrace_safety_verify/` | PostgreSQL 17/libsodium Ed25519 receipt-verification extension, installed only in the P05 PostgreSQL image. |

## Task 1: Freeze the P05 planning authority

**Files:**
- Create: `scripts/p05-scope.mjs`
- Create: `tests/p05_scope.test.mjs`
- Create: `docs/development-evidence/v1-g0-05-preflight.json`
- Create: `docs/superpowers/specs/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation-design.md`
- Create: `docs/superpowers/plans/2026-09-12-vestrace-v1-g0-05-backup-restore-foundation.md`

**Interfaces:**
- Consumes: P04 scope/protected authority and `scripts/verify-dirty-baseline.mjs`.
- Produces: `changeScopePaths`, `protectedAuthorityPaths`, and a byte-pinned P05 preflight.

- [ ] **Step 1: Write the failing P05 scope assertions**

```js
assert.deepEqual(changeScopePaths, [...changeScopePaths].sort());
assert.equal(changeScopePaths.length, 5);
assert.ok(protectedAuthorityPaths.includes('scripts/p04-scope.mjs'));
assert.ok(protectedAuthorityPaths.includes('migrations/0208_embedding_memory_references.sql'));
```

- [ ] **Step 2: Run the RED scope test**

Run: `node --test tests/p05_scope.test.mjs`

Expected: failure before `scripts/p05-scope.mjs` and its preflight exist.

- [ ] **Step 3: Capture and implement the scope**

Export the sorted five planning paths from `scripts/p05-scope.mjs`.  Export the
P04 protected list plus P04 completion authority, root `PLAN.md`, and every
existing migration `0001` through `0208`.  Capture `HEAD`, raw porcelain-v1
status bytes, every dirty file's status/size/SHA-256, and every protected
authority digest in the preflight JSON.

- [ ] **Step 4: Verify the scope and baseline**

Run: `node --test tests/p05_scope.test.mjs`

Expected: PASS, including a fixture mutation of protected P04 authority and
checksums for all protected historical migrations.

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`

Expected: PASS with the existing P04/unrelated dirty files treated as baseline.

## Task 2: Define the safety-head value model and ports

**Files:**
- Create: `crates/vestrace-domain/src/installation_safety.rs`
- Modify: `crates/vestrace-domain/Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Create: `crates/vestrace-application/src/installation_safety.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: `crates/vestrace-domain/tests/installation_safety_contract.rs`

**Interfaces:**
- Consumes: existing `InstallationId`, `RequestId`, `ApplicationError`, and P02 permit vocabulary.
- Produces: `SignedJournalEntry`, `WitnessHead`, `WitnessAdvance`, `WitnessReceipt`, `SafetyEventKind`, `InstallationSupervisorContext`, `SafetyAuthorityService`, `InstallationSafetyWitness`, `SafetyJournal`, and `SafetyAuthorityRepository` exactly as defined in the P05-A design.

- [ ] **Step 1: Write contract tests**

```rust
#[test]
fn signed_entry_requires_the_next_sequence_same_identity_and_valid_signature() {
    let head = fixture_head(7);
    assert!(head.accept_signed(fixture_entry(8)).is_ok());
    assert!(head.accept_signed(fixture_entry(9)).is_err());
    assert!(head.accept_signed(with_other_installation(fixture_entry(8))).is_err());
    assert!(head.accept_signed(tampered_signature(fixture_entry(8))).is_err());
}

#[test]
fn genesis_state_has_explicit_future_fields_and_cannot_rewrite_them() {
    let state = WitnessStateV1::genesis(active_generation());
    assert!(state.wal_checkpoints().is_empty());
    assert!(state.backup_set_heads().is_empty());
    assert!(state.target_activation_plan().is_none());
    assert!(state.advance_with_rewritten_lineage().is_err());
}

#[test]
fn receipt_signature_covers_every_witness_state_field() {
    let receipt = signed_receipt(WitnessStateV1::genesis(active_generation()));
    assert!(receipt.verify_against(witness_public_key()).is_ok());
    assert!(receipt.with_wal_checkpoint(fake_checkpoint()).verify_against(witness_public_key()).is_err());
    assert!(receipt.with_target_plan(fake_plan()).verify_against(witness_public_key()).is_err());
}
```

- [ ] **Step 2: Run the RED contract test**

Run: `cargo test -p vestrace-domain --test installation_safety_contract`

Expected: FAIL because safety types do not exist.

- [ ] **Step 3: Implement closed types and ports**

Make sequence, digest, installation identity, fingerprint identity/proof,
generation identity, activation epoch, canonical signed payload, and Ed25519
public key private validated fields.  Reuse the workspace `ring` Ed25519
support already used by CLI conformance rather than adding a crypto dependency.
Expose no constructor that can fabricate a receipt, no reset/import operation,
and no event kind outside `InstallationInitialized | GenerationRegistered`.
Add `SafetyBootstrapRecord::open_or_create(root)` using `create_new`, file and
parent `sync_all`, and exact later-byte comparison; it binds the two public keys
only during the first initializer call and exposes only its digest afterward.
Make `WitnessStateV1` a length-delimited canonical structure containing lineage,
WAL checkpoints, backup heads/holds, target plan/progress, and resume/target
initialization state from genesis, with only documented monotonic transitions.
Include the exact `WitnessStateV1` bytes and their length in the
`WitnessReceipt` Ed25519 signing payload; verification must fail if any state
field changes after signing.

- [ ] **Step 4: Run the GREEN test**

Run: `cargo test -p vestrace-domain --test installation_safety_contract`

Expected: PASS.

## Task 3: Add guarded PostgreSQL safety authority

**Files:**
- Create: `migrations/0209_installation_safety_authority.sql`
- Create: `docker/postgres/vestrace_safety_verify/vestrace_safety_verify.c`
- Create: `docker/postgres/vestrace_safety_verify/Makefile`
- Create: `docker/postgres/vestrace_safety_verify/vestrace_safety_verify.control`
- Create: `docker/postgres/vestrace_safety_verify/vestrace_safety_verify--1.0.sql`
- Create: `docker/postgres/Dockerfile`
- Create: `crates/vestrace-infrastructure/src/postgres/safety_authority_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/pool.rs`
- Modify: `crates/vestrace-cli/src/commands/migrate.rs`
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `docker-compose.yml`
- Test: `crates/vestrace-infrastructure/tests/installation_safety_authority.rs`
- Test: `crates/vestrace-infrastructure/tests/safety_supervisor_provisioning.rs`
- Test: `tests/p05_provisioner_prefix.test.mjs`

**Interfaces:**
- Consumes: Task 2 ports and `PermitMode::Exclusive`.
- Produces: `PgSafetyAuthorityRepository::new(PgStore)` implementing `SafetyAuthorityRepository` through guarded SQL functions only.

- [ ] **Step 1: Write integration tests for receipt and role rejection**

```rust
#[tokio::test]
async fn shared_holder_blocks_safety_before_journal_append_and_forged_receipt_is_refused() {
    let (service, permit, runtime_pool) = fixture().await;
    let shared = permit.acquire(PermitMode::Shared, &ordinary_context()).await.unwrap();
    let blocked = tokio::spawn(service.initialize(supervisor_context(), initialization()));
    assert_no_journal_entry_was_appended().await;
    shared.rollback().await.unwrap();
    blocked.await.unwrap().unwrap();
    assert_runtime_dml_is_refused(&runtime_pool, "installation_safety_state").await;
}
```

- [ ] **Step 2: Run the RED integration test**

Run: `cargo test -p vestrace-infrastructure --test installation_safety_authority -- --nocapture`

Expected: FAIL because migration `0209` and the repository do not exist.

- [ ] **Step 3: Implement migration and repository**

Create singleton state, append-only generations, and database-pinned journal
heads under the guarded owner. `PgSafetyAuthorityRepository` verifies the
exact signed entry and canonical witness receipt with the two pinned Ed25519
public keys before calling SQL. The guarded functions independently call
`vestrace_safety_ed25519_verify(canonical_receipt, receipt_signature,
pinned_witness_public_key)` before they inspect a safety row; no Rust-side
check is accepted as a substitute. Build the PostgreSQL 17 image extension
with `libsodium-dev` and `postgresql-server-dev-17`; its C entry point calls
`crypto_sign_verify_detached`, rejects non-32/64-byte key/signature inputs, and
has no SQL-visible mutable state. Revoke verifier EXECUTE from PUBLIC,
`vestrace`, and `vestrace_safety_supervisor`; grant it only to the guarded
owner. Migration `0209` asserts the extension and refuses if it is absent.
Do not create the extension in `0209`: append to the real bootstrap provisioner
a creation block for the non-owner `vestrace_safety_supervisor` login, followed
by `CREATE EXTENSION vestrace_safety_verify`, `ALTER FUNCTION ... OWNER TO
vestrace_guarded_owner`, and the bounded grant/revoke block before the runtime
migrator can start. The role must exist before `CREATE EXTENSION`, because the
extension control SQL revokes verifier access from it. Make `0209` assert that
exact installation. Switch Compose `postgres`
to build `docker/postgres/Dockerfile`, and make every P05 integration fixture
run that image plus the real provisioner before applying migrations.
In the same provisioner suffix create but do not invoke bootstrap-owned
`vestrace_install_p05_safety_schema() RETURNS VOID`, with no arguments, fixed
`search_path = pg_catalog`, and a hard-coded complete 0209 table/function DDL
and allowlist. It must schema-qualify every `public` target, create only that
list under bootstrap, transfer every object to `vestrace_guarded_owner`, revoke
runtime/PUBLIC ACLs, and force RLS. The P05 transition service invokes it only after verifying
`0001`вЂ“`0208` are applied, `0209` is absent, setting `vestrace NOLOGIN`, and
observing no active `vestrace` backend; it must fail rather than terminate a
session. Before installer creation it must `ALTER SCHEMA public OWNER TO
vestrace_guarded_owner`, revoke CREATE from PUBLIC and `vestrace`, and assert
both `pg_namespace.nspowner` and
`has_schema_privilege('vestrace', 'public', 'CREATE') = false`; an ACL revoke
alone is insufficient because `vestrace` initially owns `public`. Revoke
installer EXECUTE from PUBLIC and every non-bootstrap role; it
never accepts a caller-supplied schema, object name, or payload. Its idempotent
path accepts only exactly that owner/ACL outcome *and* a fixed catalog
definition signature: table columns/types/defaults/nullability, constraints,
indexes, triggers, RLS policies, function identity/body/config, and extension
membership. `0209` must repeat those assertions and refuse a same-named
structurally substituted object; it must not create safety objects or restore
schema CREATE. Add ledger-aware `PgStore` migration APIs and CLI flags
`vestrace migrate --through-version 208` and
`vestrace migrate --only-version 209`; phase one must use only the checked
embedded prefix, while phase three first requires that exact successful prefix
ledger and then applies only pending 0209. Both reject wrong, stale, partial,
or out-of-order ledgers and never fall through to a full pending migration run.
Implement the fixed Compose sequence: phase one uses `--through-version 208`
as `vestrace`; phase two, with no product service eligible to start, performs
the bootstrap transition above; phase three enables the runtime login and uses
`--only-version 209`. The fresh-image test must exercise that order, first
prove a live runtime session makes phase two fail closed, then after the session
exits retry phase two, assert `public` ownership by the guarded NOLOGIN role
and effective runtime schema-CREATE denial, and attempt runtime
`CREATE TABLE public.p05_injected (...)` before `0209` to observe denial; it
then applies 0209 and proves the same denial after migration. Add rejected
wrong/stale/partial ledger fixtures and normalized Compose dependency checks
for all three phase services.
The guarded functions require
`session_user = vestrace_safety_supervisor`, call
`vestrace_assert_installation_supervisor_context()`, validate the pinned key
identities, receipt fields, signed-entry digest, sequence, and predecessor
digest, plus the receipt's signed canonical `WitnessStateV1` bytes against the
stored state digest and the allowed monotonic transition, then update the
database journal head atomically and reject
initialization unless
`embedding_legacy_retirement_gate` exists.  Provision a separate non-owner
`vestrace_safety_supervisor` login only in `init-runtime-role.sh` from an
external credential, grant it EXECUTE only on the two P05 guarded functions,
and grant no runtime direct DML.  Acquire `PermitMode::Exclusive` before any
safety-row guard.

Define `SafetySupervisorConfig` with only
`VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL`; it is read by the host CLI, never a
Compose service. Build one `PgStore` from it and pass that store to both the
permit and repository. The safety commands reject the ordinary runtime DSN
because its `session_user` fails the guarded context assertion before journal
append. Make both outer SQL functions `SECURITY DEFINER` under
`vestrace_guarded_owner`, set `search_path = pg_catalog, public`, and invoke
only `public.vestrace_safety_ed25519_verify`; no unqualified helper is allowed.

Before any bootstrap record is accepted, load `HostInstallationFingerprintVault`,
recompute `FingerprintKeyContinuityProof`, require it equals the bootstrap
record, and call `vestrace_installation_fingerprint_continuity_matches` with
the same installation ID, key ID, version, and proof. The guarded initializer
repeats the exact comparison against `installation_fingerprint_continuity` in
its transaction. Add a mismatch fixture for both a replaced host-vault record
and a mismatched database row; both must fail before journal append.

Create the literal marker `# P05 safety supervisor role вЂ” do not move` as the
first byte of a newly appended suffix, followed by the P05 role block; do not
edit preceding bytes. The marker is absent from the captured P04 file by
design, so its first occurrence defines the predecessor/P05 boundary.
`tests/p05_provisioner_prefix.test.mjs` must read the P05 preflight's captured
digest for `docker/postgres/init-runtime-role.sh`, split the current file at
that marker, and require SHA-256 equality for the complete predecessor prefix.

`safety_supervisor_provisioning.rs` must load the appended block from the real
`include_str!("../../../docker/postgres/init-runtime-role.sh")`, substitute a
test-only SQL string literal for `:'safety_supervisor_password'`, execute that
exact SQL against the fixture superuser pool, and connect as the resulting
`vestrace_safety_supervisor` login. It must assert EXECUTE on exactly
`vestrace_initialize_installation_safety` and
`vestrace_register_database_generation`, no privileges on any
`installation_safety_%` table, no membership in `vestrace` or
`vestrace_guarded_owner`, all of `rolsuper`, `rolbypassrls`, `rolcreaterole`,
`rolcreatedb`, and `rolreplication` false, and failed direct table DML,
database/schema CREATE, and `SET ROLE` attempts through the actual supervisor
connection.

Add a fresh P05-image fixture that runs the real provisioner, then attempts
`CREATE TABLE public.p05_injected(id INTEGER)` through the concurrent runtime
connection before `0209` and observes denial. Migrate through 0209 as
`vestrace`, then assert every 0209 object owner is `vestrace_guarded_owner`,
runtime/PUBLIC table and function ACLs are denied, and
`CREATE TABLE public.p05_escape(id INTEGER)` fails through the runtime
connection. This proves the runtime never gains schema CREATE during either
the provisioning or migration windows.

The same test must call `vestrace_initialize_installation_safety` through the
actual supervisor login with one bit flipped in the canonical receipt
signature. It must receive the guarded signature-refusal error even though the
login has EXECUTE on the outer function; direct EXECUTE on
`vestrace_safety_ed25519_verify` must be denied. Run this against the P05
PostgreSQL image built from `docker/postgres/Dockerfile`; if Docker is
unavailable, record this required qualification as blocked rather than pass.
It must also prove a runtime DSN fails before journal append, inspect
`pg_get_functiondef` to require the fixed `SECURITY DEFINER` search path and
schema-qualified verifier call, and prove direct supervisor EXECUTE on the
verifier is denied even after `SET search_path` and temporary-object attempts.

- [ ] **Step 4: Run migration, integration, and privilege checks**

Run: `cargo test -p vestrace-infrastructure --test installation_safety_authority -- --nocapture`

Expected: PASS, including direct runtime-role DML denial.

Run: `cargo test -p vestrace-infrastructure --test safety_supervisor_provisioning -- --nocapture`

Expected: PASS, including the exact real-provisioner ACL inventory.

## Task 4: Implement the host witness and supervisor composition

**Files:**
- Create: `crates/vestrace-infrastructure/src/safety/witness_file.rs`
- Create: `crates/vestrace-infrastructure/src/safety/journal_file.rs`
- Create: `crates/vestrace-infrastructure/src/safety/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/lib.rs`
- Create: `crates/vestrace-cli/src/commands/safety_supervisor.rs`
- Modify: `crates/vestrace-cli/src/commands/mod.rs`
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `docker-compose.yml`
- Test: `crates/vestrace-cli/tests/safety_supervisor_witness.rs`
- Test: `crates/vestrace-cli/tests/safety_supervisor_recovery.rs`
- Test: `crates/vestrace-cli/tests/safety_journal_compose.rs`

**Interfaces:**
- Consumes: Task 2 `InstallationSafetyWitness`; Task 3 repository.
- Produces: `FileInstallationSafetyWitness::open(root: &Path) -> Result<Self, WitnessError>` and a host-only `vestrace safety-supervisor initialize` command.

- [ ] **Step 1: Write witness durability and conflict tests**

```rust
#[tokio::test]
async fn immutable_journal_and_fsynced_witness_precede_a_signed_receipt() {
    let root = tempfile::tempdir().unwrap();
    let (journal, witness) = fixture_with_fault_log(root.path()).unwrap();
    let entry = journal.sign_and_append(head(), payload()).await.unwrap();
    assert_eq!(fault_log(), ["entry_file_fsync", "entries_directory_fsync"]);
    let first = witness.compare_and_advance(head(), advance(entry)).await.unwrap();
    assert_eq!(fault_log().last_two(), ["witness_file_fsync", "witness_parent_fsync"]);
    assert!(first.verify_against(witness.public_key()).is_ok());
    drop(witness);
    let reopened = FileInstallationSafetyWitness::open(root.path()).unwrap();
    assert_eq!(reopened.read_head().await.unwrap().sequence(), first.sequence());
    assert!(reopened.compare_and_advance(head(), advance(entry)).await.is_err());
}
```

- [ ] **Step 2: Run the RED witness tests**

Run: `cargo test -p vestrace-cli --test safety_supervisor_witness`

Expected: FAIL because the package and file adapter do not exist.

- [ ] **Step 3: Implement atomic record replacement and host composition**

Write a signed journal entry with `create_new` at the immutable
`entries/<sequence>-<digest>.cbor` path on the dedicated journal volume, then
`sync_all` its file and entries directory before advancing the host witness.
Write the witness replacement to a same-directory temporary file, `sync_all`
it, rename it once, and `sync_all` its parent before signing a receipt. Verify
the stored checksum, signature, and pinned installation/fingerprint identity
on every open.  The supervisor accepts separate journal and host roots,
refuses a witness/signing-key root inside a Compose volume, and exposes no HTTP
listener or credential/key export.

Add a subprocess test using `CARGO_BIN_EXE_vestrace` and a disposable
PostgreSQL fixture.  `vestrace safety-supervisor initialize --fault-after-witness-advance`
must abort after witness advancement and before the guarded database function;
`vestrace safety-supervisor reconcile` must verify and persist the existing
signed entry and signed receipt, with no second journal sequence.

Add `installation-safety-journal` and the one-shot journal initializer to
`docker-compose.yml`. Its normalized Compose configuration must show that only
the initializer mounts the journal volume; PostgreSQL, migrate, server, worker,
and console do not. The host command accepts only the engine-resolved root in
`VESTRACE_SAFETY_JOURNAL_ROOT` and refuses startup until it matches the declared
volume and the initializer receipt. `safety_journal_compose.rs` parses the
normalized configuration and asserts that inventory; when Compose is absent,
its evidence test reports a blocked environment rather than a pass.

- [ ] **Step 4: Run the GREEN witness tests and clippy**

Run: `cargo test -p vestrace-cli --test safety_supervisor_witness`

Expected: PASS.

Run: `cargo test -p vestrace-cli --test safety_journal_compose`

Expected: PASS, or a recorded blocked-environment result when Docker Compose is unavailable.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

## Task 5: P05-A acceptance review and next-slice boundary

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-backup-restore-foundation.md`
- Modify: `docs/development-evidence/v1-g0-05-preflight.json`
- Modify: `scripts/p05-scope.mjs`
- Modify: `tests/p05_scope.test.mjs`

**Interfaces:**
- Consumes: completed Tasks 1вЂ“4 and their exact test outputs.
- Produces: evidence that P05-A is accepted without claiming archive, restore, Compose qualification, or full G0 completion.

- [ ] **Step 1: Run final baseline and focused suites**

Run: `node --test tests/p05_scope.test.mjs && node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`

Expected: PASS.

Run: `cargo test -p vestrace-domain --test installation_safety_contract && cargo test -p vestrace-infrastructure --test installation_safety_authority && cargo test -p vestrace-cli --test safety_supervisor_witness`

Expected: PASS.

- [ ] **Step 2: Record only observed evidence**

Record command exits, test counts, environment availability, and the exact
dirty-baseline result.  State that P05-B/P05-C/P05-D remain unimplemented until
their separately reviewed plans and scope amendments are accepted.

## Self-Review

* **Spec coverage:** P05-A maps section 11.6 witness monotonicity, identity
  continuity, receipt-bound journal head, generation lineage root, exclusive
  permit ordering, and P04 retirement prerequisite to Tasks 2вЂ“4.  Archive,
  restore, and deployment evidence are intentionally isolated into P05-B/C/D
  and are not claimed by this slice.
* **Placeholder scan:** No task uses a generic error-handling instruction;
  each failure case, interface, test target, and command is named.
* **Type consistency:** `WitnessHead`, `WitnessAdvance`, `WitnessReceipt`,
  `InstallationSafetyWitness`, and `SafetyAuthorityRepository` are defined in
  the design and Task 2 before Tasks 3вЂ“4 consume them.

## Execution Handoff

This plan is the P05-A authority foundation only.  It requires a P05 scope
amendment before Task 2 can edit a source path, and another amendment before
each later independent P05 subproject.  No implementation code is authorized
by this planning artifact alone.

**External corpus impact:** compatibility seam вЂ” no registered external corpus
entry is adopted in P05-A; it adds internal safety authority required by the
frozen v1 design.
