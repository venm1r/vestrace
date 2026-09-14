# V1 G0-05 P05-A through P05-D: backup, restore, Compose, and G0 evidence

**Recorded:** 2026-09-13, extended 2026-09-14 for P05-D.  
**Acceptance boundary:** P05-A through P05-D. This records the implemented archive, restore/cutover, Compose topology, and host readiness qualification; it is not deployment qualification or full G0 completion. The aggregate G0 result is whatever `scripts/p05-g0-gate.mjs` emits, and it is not a pass.

## Observed verification

| Evidence | Observed result |
| --- | --- |
| `node --test tests/p05_scope.test.mjs` | Exit 0; 4 tests passed. The fixture rejects a P04 authority mutation, confirms P04 dispatch remains intact, and checks sorted/disjoint P05 scope coverage through migration 0208. |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | Exit 0. The P05 baseline accepted the approved P05 paths while retaining the captured unrelated dirty baseline. |
| `node --test tests/p05_provisioner_prefix.test.mjs` | Exit 0; 6 tests passed. Includes the P04 prefix digest, journal digest model, exact `SMALLINT` helper binding, extension loader/link checks, and provisioner authenticated-readiness retry. |
| `cargo test -p vestrace-domain --test installation_safety_contract` | Exit 0; 5 tests passed. Covers signed-entry identity/sequence/key refusal, digest coverage, pinned receipt verification, durable decode refusal, and create-only bootstrap binding. |
| `cargo test -p vestrace-cli --test safety_supervisor_witness` | Exit 0; 2 tests passed. Covers immutable journal/witness reopen/conflict behavior and corrupt-record refusal. |
| `cargo test -p vestrace-cli --test safety_journal_compose` | Exit 0; 2 tests passed. Covers the dedicated journal initializer/mount inventory and Compose configuration inspection. |
| `cargo test -p vestrace-cli --test safety_supervisor_recovery -- --nocapture` | Exit 0; 1 test passed in a one-off Linux Rust container attached only to the isolated P05 PostgreSQL project. `initialize --fault-after-witness-advance` exited 86 after the signed entry and receipt were durable; the safety state was absent. `reconcile` then completed with one state event at sequence 1 and no second journal entry. |
| `cargo test -p vestrace-infrastructure --test installation_safety_authority -- --nocapture` | Isolated port-qualified PostgreSQL qualification: exit 0; 1 live test passed. |
| `cargo test -p vestrace-infrastructure --test safety_supervisor_provisioning -- --nocapture` | Isolated port-qualified PostgreSQL qualification: exit 0; 1 live test passed. |
| P05 Compose phase qualification | In the isolated project, the clean sequence completed: provisioner, exact 0001–0208 history, bootstrap installer, runtime re-login, and 0209 assertion migration. Manual SQL confirmed the 0207/0208/0209 ledger prefix, guarded `public` ownership, and runtime `CREATE` denial. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Exit 0; completed in 2m58s. |

The isolated qualification also observed the role-provision readiness race and
then passed after the provisioner used its bounded authenticated `psql SELECT
1` retry. The recovery qualification recreated only the disposable P05 phase
chain; it did not start a server, worker, or console.

The current evidence-recording host had no
`VESTRACE_P05_TEST_DATABASE_URL`. Its two PostgreSQL test targets each exited
0 with one test after explicitly reporting `BLOCKED: set
VESTRACE_P05_TEST_DATABASE_URL to a disposable provisioned P05 database`;
they did not substitute that blocked local result for the isolated live proof.

## Observed P05-B archive work (in progress)

| Evidence | Observed result |
| --- | --- |
| `node --test tests/p05_scope.test.mjs tests/p05_provisioner_prefix.test.mjs` | Exit 0; 12 tests passed. The P05-B allowlist remained sorted, minimal, and disjoint from protected P04 authority; the archive installer regression test requires every declared guarded event kind. |
| `cargo test -p vestrace-domain --test backup_archive_contract -- --nocapture` | Exit 0; 6 tests passed. Covers checkpoint bytes, CAS head binding, archive-state encoding, the deletion-preparation digest, and the separate exact key-erasure intent signed before host custody. |
| `cargo test -p vestrace-application backup_archive --lib -- --nocapture` | Exit 0; 4 tests passed. Includes durable reservation commit before host staging. |
| `cargo test -p vestrace-cli --test safety_archive_host_custody --test safety_archive_recovery --test safety_supervisor_recovery -- --nocapture` | Exit 0 before the current streaming addition. Host custody: 4 passed; archive command boundary: 1 passed. The existing supervisor recovery test reported its documented `BLOCKED` disposable-database condition and did not provide a live archive recovery claim. |
| `cargo test -p vestrace-cli --test safety_archive_host_custody -- --nocapture` | Exit 0 after the streaming addition; 5 tests passed. A segment spanning three encrypted chunks round-tripped through the create-only ciphertext spool, digest/length verification, exact staging, and immutable promotion. |
| Isolated PostgreSQL catalog check | A fresh `p05b-task5-live` Compose project installed the guarded `vestrace_abandon_backup_archive_append` procedure and `vestrace_list_pending_backup_archive_appends` inventory for the supervisor role. The current 0210 assertion executed as `DO` after the idempotent checkpoint-retry guard and pending-intent inventory were added. The project, network, and volume were removed afterward. |
| `cargo test -p vestrace-cli --test safety_archive_recovery -- --nocapture` | Exit 0; 3 tests passed. The relative-path rejection and the hold/sealing CLI boundary executed. The new `backup begin` plus streamed `append-wal` subprocess qualification correctly printed `BLOCKED` on this Windows host because bootstrap-parent fsync returned `os error 5`; it made no live-archive claim. |
| `cargo test -p vestrace-cli --bin vestrace lifecycle_reconciliation -- --nocapture` | Exit 0; 3 tests passed. Reconciliation refuses a non-exact sealing successor and reuses the exact signed preparation and key-erasure intent digests. |
| `cargo test -p vestrace-cli --test safety_archive_host_custody -- --nocapture` | Exit 0; 6 tests passed. The added case verifies that deletion uses `ManagedBackupDeletionPrepared`, removes only one manifest-named immutable object, and resumes that same exact removal after a partial deletion. |
| `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings` | Exit 0. Strict lint completed for every target in the four P05 implementation crates. |
| Isolated `p05b-task6-live` PostgreSQL setup | The one-off Compose chain completed through the 0208 history and P05 safety bootstrap. The guarded archive installer returned one row and `0210_managed_backup_archive_retention.sql` completed as `DO`. Containers, network, and volume were removed. The host subprocess test was blocked before it could exercise the database because the protected Windows bootstrap root failed its durability preflight. |
| Isolated `p05b-task7-live` PostgreSQL setup | Exit 0. A fresh project completed role provisioning, history through 0208, safety bootstrap, and the normal 0209 service. Read-only SQL confirmed `vestrace_prepare_archive_key_erasure` and `vestrace_list_prepared_backup_archive_objects` exist, are denied to `vestrace`, and are executable by `vestrace_safety_supervisor`. An explicit `migrate --only-version 210` recorded `210|t` after its assertion. Containers, network, and volume were removed. |
| Isolated `p05b-task9-live` PostgreSQL setup | Exit 0. A newly created database completed role provisioning, 0001--0208 history, safety bootstrap, normal 0209, and explicit 0210 assertion (`210|true`). Read-only SQL observed the guarded journal event constraint accepts exactly event kinds 0 through 10. The project had no archive, key, witness, or bootstrap host-root mount; its containers, network, and volume were removed afterward. |
| `cargo test -p vestrace-cli --test safety_archive_recovery -- --nocapture` in a Linux Rust container | Exit 0; 3 tests passed against `p05b-task9-live`. The live subprocess completed one streamed WAL member, sealing, prepared deletion, durable key-erasure intent and custody erasure, exact manifest object removal, and final `Deleted`; it observed the retained immutable DB manifest row and zero final host objects. This is lifecycle proof, not a crash injection or recovery proof. |
| `cargo test -p vestrace-infrastructure --test backup_archive_authority -- --nocapture` in a Linux Rust container | Exit 0; 1 live test passed against `p05b-task9-live` as runtime role `vestrace`. Direct inserts into archive head, restore-hold, and deletion-preparation tables were refused with SQLSTATE `42501`. |
| `cargo test -p vestrace-application backup_archive --lib -- --nocapture`; `cargo test -p vestrace-cli --test safety_archive_host_custody -- --nocapture` | Exit 0; 4 application tests and 6 host-custody tests passed after the lifecycle qualification fixes. They cover source-spool cleanup, create-only staging, independent set encryption, and exact prepared deletion. |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | Exit 1 before P05 comparison: the inherited dirty tree contains `crates/vestrace-application/Cargo.toml` outside P05 scope. No baseline or non-P05 path was changed to suppress this failure. |

The current `backup append-wal` command validates an absolute local segment,
hashes it and encrypts it through host key custody as 64 KiB AES-GCM chunks,
and stages a ciphertext spool without retaining the segment in memory. The
source spool is unlinked after staging and promotion removes its exact frame
plus only an empty known set directory. The
pending-intent inventory exists and exact cleanup is wired. The lifecycle
commands now include `prepare-delete`, `erase-key`, and `finalize-delete`:
the preparation receipt derives from the sealed signed archive state; the
key-erasure intent is durable before host custody; and final deletion obtains
one guarded manifest, commits that short read, then verifies/removes each
named final object without a database lock. The complete live crash-recovery
path, including post-witness checkpoint replay and final deletion, remains
unqualified.

The host supervisor now also accepts `backup acquire-hold --set-id --hold-id`.
It records only the witnessed, non-expiring acquisition through the guarded
repository. It intentionally exposes no release operation because P05-C must
bind release to a terminal target/source receipt.

### P05-B base-capture continuation

| Evidence | Observed result |
| --- | --- |
| `node --test tests/p05_scope.test.mjs tests/p05_provisioner_prefix.test.mjs` | Exit 0; 13 tests passed. The migration 0211 route is constrained to predecessor 0210, the guarded installer is required, and a post-base WAL must retain the base timeline and start at or after the base start LSN. |
| `cargo test -p vestrace-domain --test backup_archive_contract` | Exit 0; 7 tests passed. The archive contract now rejects a WAL segment whose timeline or start LSN is outside the preceding base checkpoint ancestry. |
| `cargo test -p vestrace-application backup_archive --lib -- --nocapture` | Exit 0; 4 tests passed. |
| `cargo test -p vestrace-cli --bin vestrace` | Exit 0; 27 tests passed. The focused base-capture tests passed 7/7: malformed or duplicate `backup_label` entries, exact argument separation, bounded stderr draining, exact capture layout, and PostgreSQL 17 manifest version/system-identifier parsing. |
| `cargo test -p vestrace-cli --test safety_archive_recovery -- --nocapture` | Exit 0; 5 tests passed. A harmless fake `pg_basebackup` receives the fixed tar/no-WAL/manifest arguments, produces an unknown layout, and proves that the supervisor retains the unique protected capture directory before any database access. The opt-in PostgreSQL 17 capture test and the live archive test each printed their explicit `BLOCKED` conditions because this Windows host did not provide the required absolute tool/source or disposable database variables. |
| `cargo test -p vestrace-cli --test safety_archive_host_custody -- --nocapture` | Exit 0; 6 tests passed. |
| `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings`; `cargo fmt --check`; `git -c safe.directory=E:/Soft/vestrace diff --check` | Each exited 0. `diff --check` emitted only inherited CRLF warnings. |
| Fresh `p05basequal2` PostgreSQL qualification | The disposable Compose chain provisioned roles, installed history through 0208, safety bootstrap, 0209, 0210, and the 0211 assertion. Read-only catalog SQL observed migrations 209, 210, and 211 as applied, the three base guards owned by `vestrace_guarded_owner`, and the guarded installer owned by that role. |
| Direct disposable-database negative probes | Runtime direct archive-head insert failed with SQLSTATE `42501`; bootstrap-role WAL at ordinal 1 without a base and a restore hold without a base each failed with SQLSTATE `23514`. In a separate explicit rollback transaction, a base at ordinal 1 followed by same-timeline WAL at ordinal 2 was admitted; WAL on a different timeline then failed with SQLSTATE `23514`. A final count query observed zero retained test sets. |
| PostgreSQL 17 base-backup format probe in `p05basequal2` | A one-off `pg_basebackup --format=tar --wal-method=none --manifest-force-encode` against the disposable database completed. It produced `base.tar` plus a separate top-level `backup_manifest`; the manifest declared format version 2 and a nonzero numeric `System-Identifier`. This confirms the parser contract but is not a supervisor archive-commit proof. |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | Exit 1 before P05 comparison because inherited dirty path `crates/vestrace-application/Cargo.toml` is outside the P05 allowlist. It was neither changed nor added to the baseline. |
| Final clean `p05realqual` Compose chain | Exit 0. The fresh disposable PostgreSQL 17 project ran the role provisioner, history through 0208, safety bootstrap, 0209, 0210, and 0211. The P05-suffix initializer admitted authenticated replication only when its own `PGDATA` exists, so the separate role-provisioner cannot mutate an unmounted data directory. |
| `safety_archive_recovery` in the isolated Linux runner | Exit 0; 5 tests passed in 31.95s. The runner used only compiled test/CLI binaries in the disposable `p05realqual` network. It invoked the absolute PostgreSQL 17 `pg_basebackup` binary with tar output, no WAL, a fast checkpoint, a pinned system identifier, and the real source DSN without logging it. The format test observed the expected `base.tar`/`backup_manifest`; the lifecycle test committed that captured base through the supervisor, appended same-timeline WAL at its recorded base LSN, sealed, prepared deletion, erased the key, and finalized the set. |
| `node --test tests/p05_scope.test.mjs tests/p05_provisioner_prefix.test.mjs` after HBA placement | Exit 0; 14 tests passed. The P04 predecessor prefix remains byte-for-byte identical; the P05 suffix alone requires the instance-local authenticated replication rule. |

This continuation adds the guarded base-checkpoint ordering and the CLI path
which invokes a configured absolute `pg_basebackup` executable, accepts only
regular top-level `base.tar` and `backup_manifest` files, extracts a bounded
`backup_label`, requires PostgreSQL 17 manifest version 2, and compares its
nonzero system identifier with the explicitly pinned
`VESTRACE_PG_BASEBACKUP_SOURCE_SYSTEM_IDENTIFIER` before it can persist the
base checkpoint. Its stderr reader drains the pipe while retaining at most 4
KiB and never exposes that diagnostic, so a source DSN cannot appear in the
error output.

P05-B is qualified for the implemented capture boundary: a real PostgreSQL 17
base archive was committed through the host supervisor to a clean disposable
database, then used as the ancestry boundary for WAL and the guarded archive
lifecycle. P05-C is qualified by the later evidence in this record; P05-D
remains outside the approved implementation scope.

## Limits and next slices

P05-D deployment and the G0 aggregate gate result require a separately reviewed
plan and scope amendment. This document contains no release, deployment, or
full-G0 claim.

The native Windows attempt to create the test bootstrap record correctly failed
closed because its temporary filesystem would not flush the parent directory
(`os error 5`). The passing Linux recovery qualification above supplies the
required durable-write and guarded-reconcile proof without weakening that
Windows durability invariant.

This evidence includes archive lifecycle, authority, and restore/cutover qualification. It does not claim deployment or the aggregate G0 gate result.

### P05-C restore/cutover intermediate increment

| Evidence | Observed result |
| --- | --- |
| `cargo test -p vestrace-domain --test restore_cutover_contract` | Exit 0; 6 tests passed. The activation-plan test now proves a signed `GenerationRegistered` transition is rejected before an exact `TargetInitializationComplete` terminal receipt, rejects a different target generation, and accepts only the planned target afterward. |
| `cargo test -p vestrace-application restore_cutover::tests --lib` | Exit 0; 1 test passed. Target creation remains unavailable for overlapping roots and non-prepared attempts. |
| `cargo test -p vestrace-cli --test safety_restore_host_custody -- --nocapture` | Exit 0; 4 tests passed. Fresh-target separation, descriptor-bound authenticated materialization, protected-source argument rejection, and a fake absolute target tool receiving only explicit target-side contract arguments plus `--pgdata` were observed. |
| `cargo test -p vestrace-cli --bin vestrace source_freeze_receipt -- --nocapture` | Exit 0; 2 tests passed. A freeze receipt with a different attempt ID is refused; the accepted receipt must also name the prepared source generation. When no receipt exists, the supervisor invokes only an absolute configured source quiescer with derived root/receipt/attempt/generation arguments and accepts its exact output. |
| `cargo test -p vestrace-cli --bin vestrace target_initialization_receipt_tests -- --nocapture` | Exit 0; 2 tests passed. The target tool's durable receipt must name the plan digest, the frozen timeline, and a final LSN at or after the frozen LSN. The CLI derives and supplies that exact target receipt path, plan digest, and frozen point as separate tool arguments; the supervisor no longer creates the receipt itself. |
| `cargo test -p vestrace-cli --test safety_restore_recovery` | Exit 0; 2 tests passed. The process surface exposes the terminal restore flow but no `release-hold` command, and neither `freeze-source` nor `activate` accepts operator-supplied timeline, watermark, or receipt flags. |
| `cargo test -p vestrace-application terminal_receipt_replay --lib` | Exit 0; 1 test passed. The controller replays only the exact guarded `target_initialized` SQL effect after a crash leaves the witnessed terminal receipt durable but before SQL commits. A mismatched attempt, target, or non-initialization terminal reason is refused before opening a permit. |
| `cargo test -p vestrace-domain --test restore_cutover_contract`; `cargo test -p vestrace-cli --bin vestrace -- --nocapture` | Exit 0; 6 domain contracts and all 31 CLI unit tests passed. `TargetActivating` is a plan-digest marker witnessed after the exact target receipt and before the guarded generation CAS; direct generation registration from the terminal state is rejected. Restart reconciliation applies the exact `target_initialized` SQL effect for a terminal receipt and treats the activation marker as a pending CAS boundary. |
| `cargo test -p vestrace-cli --bin vestrace target_locator -- --nocapture` | Exit 0; 1 test passed. A create-only digest locator in the protected journal root binds an attempt to one absolute target path. Replays accept that same path and reject a replacement, so later materialization, activation, and refusal cleanup cannot redirect host target operations. |
| `cargo test -p vestrace-cli --test safety_restore_recovery -- --nocapture`; strict P05 clippy | Exit 0; 2 recovery interface tests passed and strict clippy passed for CLI, application, and infrastructure. `restore refuse` exposes no generic hold release: it requires the locator-bound target, persists a destruction marker, requires an attempt-bound source-resume receipt from the configured absolute tool, records `source_resume_prepared`, then releases only that matching hold. |
| `cargo test -p vestrace-cli --bin vestrace refusal_marker -- --nocapture` | Exit 0; 1 test passed. Refusal destroys the locator-bound target once, persists one idempotent destruction marker for restart, and refuses a source-resume receipt unless it names the exact restore attempt and destruction digest. |
| `cargo run -p vestrace-cli -- safety-supervisor restore --help` | Exit 0. The five explicit commands `prepare`, `freeze-source`, `materialize`, `plan-activation`, and `activate` are available. The CLI entry point runs parsing on an 8 MiB Rust thread because the full command tree overflowed the native Windows main-thread stack even for `--help`; the subprocess then printed help normally. |
| `cargo clippy -p vestrace-cli --bin vestrace -- -D warnings`; `cargo clippy -p vestrace-domain --test restore_cutover_contract -- -D warnings` | Both exited 0. |
| `cargo test -p vestrace-infrastructure --test restore_cutover_authority` | Exit 0; 1 test passed locally. It had no `VESTRACE_P05_TEST_DATABASE_URL`, so it did not constitute a live authority claim. |
| Disposable `p05cqual-postgres-1` migration route | Exit 0. After rerunning the isolated suffix initializer, `0212_managed_restore_cutover.sql` executed as `vestrace` (`SELECT`, `REVOKE`, `DO`). The subsequent ACL probe returned `f|t|f|f`: guarded owner has no `CREATE` on `public`; the supervisor can execute the manifest procedure; runtime and the migration installer cannot. |
| `node --test tests/p05_scope.test.mjs`; `git -c safe.directory=E:/Soft/vestrace diff --check` | Both exited 0. The scope suite reported 6 passing tests; `diff --check` emitted only inherited CRLF warnings. |

At this intermediate point, source/target physical PostgreSQL 17 qualification was still outstanding. The later encrypted-manifest supervisor recovery section records the completed physical and crash-boundary qualification.

### P05-C disposable PostgreSQL 17 physical-media probe

| Evidence | Observed result |
| --- | --- |
| Disposable source capture | A new PostgreSQL 17 source in `p05cphysical-source-data` accepted one `qualification_rows` value, then `pg_basebackup --format=tar --wal-method=stream --checkpoint=fast` wrote nonempty `base.tar`, `pg_wal.tar`, and `backup_manifest` into the separate `p05cphysical-archive` volume. The initial capture first refused replication because source HBA had no replication rule; after adding one disposable `host replication postgres 172.21.0.0/16 scram-sha-256` entry and reloading, capture exited 0. |
| Fresh target restore | A target restore helper mounted only `p05cphysical-archive:ro` and a new target volume, extracted `base.tar` and `pg_wal.tar`, and a PostgreSQL 17 target started from that target volume. It returned `p05-c-physical-source`, timeline `1`, and LSN `0/30000D8`. `docker inspect` showed only `p05cphysical-target-verify-data:/var/lib/postgresql/data`; it had no source-volume mount. |
| Source separation hash | The source was paused before the second fresh target restore. Its numeric tar SHA-256 was `92c87a9433579697de2b9f5ce9c36a29c3ccdfa4bfded221f1be099b6497b10b` both before pausing and after unpausing; the target again read the seeded value. |

This is a physical PostgreSQL media/separation probe, not a complete P05-C
supervisor qualification: it did not move this base/WAL set through the
encrypted P05-B manifest, witnessed freeze/target receipt, activation CAS, or
the requested crash points.

### P05-C isolated PostgreSQL 17 target-recovery probe

| Evidence | Observed result |
| --- | --- |
| Physical source capture | Disposable `p05c-final-source` on `p05c-final-source-data` accepted a seeded `p05-c-final-source` row. `pg_basebackup -F t -X stream -c fast` created `base.tar`, `pg_wal.tar`, and `backup_manifest` in the separate `p05c-final-archive` volume. |
| Fresh target recovery | A helper extracted only those archive members into `p05c-final-target-data`; PostgreSQL 17 started from that target volume using a target-local `restore_command` and returned `p05-c-final-source`. |
| Mount separation | `docker inspect p05c-final-target` observed exactly `p05c-final-target-data:/var/lib/postgresql/data`; no source or archive volume was mounted by the running target. |

The source hash comparison is not claimed: the initial pre-pause hash and a
post-unpause hash differed because PostgreSQL wrote WAL while resuming. A later
paused read-only helper hash was obtained, but the corresponding full
post-restart tar output was unavailable. The encrypted P05-B manifest path,
host witness, and restore-specific crash injection remain required before
P05-C qualification can close.

### P05-C repeatable target recovery and refusal-upgrade evidence

| Evidence | Observed result |
| --- | --- |
| Forward-only `0213_managed_restore_refusal.sql` route | The existing disposable `p05cqual-postgres-1` had the pre-resume `managed_restore_attempts_state_check` and `managed_restore_attempts_check1`. Re-running the fixed provisioner exposed the one-shot refusal installer; invoking it as `vestrace` replaced only those state checks, created `vestrace_record_source_resume_prepared(uuid,bytea)`, and left its grants `vestrace=false`, `vestrace_safety_supervisor=true`. The final installer privilege for runtime was `false`. This is an upgrade probe for the 0212-to-0213 boundary, not a production migration run. |
| `node --test tests/p05_scope.test.mjs` | Exit 0; 6 tests passed. P05-C now explicitly admits the forward-only 0213 refusal and 0214 safety-event migrations and has 67 reviewed paths. |
| Focused refusal and restore contracts | Exit 0: 6 domain restore contracts, 2 application restore tests, and the CLI `refusal_marker` test passed. The refusal marker is idempotent, target cleanup remains locator-bound, and a source-resume receipt must bind both the exact attempt and target-destruction digest. |
| Pre-terminal target crash boundary | `restore materialize` now has the named `before-target-initialization` fault point after the target-only tool returns and before the receipt is consumed. On restart, an existing receipt is accepted only if it matches the immutable plan and source freeze; the focused `target_materialization_recovers_only_an_exact_existing_receipt` test passed. |
| Strict P05 clippy | Exit 0 for domain, application, CLI, and infrastructure with `--all-targets -- -D warnings`. |
| Live runtime authority refusals | In disposable `p05cqual-postgres-1`, direct runtime insertion into `managed_restore_events` and runtime invocation of `vestrace_record_source_resume_prepared` each failed with PostgreSQL SQLSTATE `42501`. |
| Independent target recovery with frozen source | The source `p05c-final-source` was paused before the deterministic read-only numeric tar hash and stayed paused through creating a new `p05c-final-target-verify-data` from `p05c-final-archive`. The source hash was `1dc11eda7b330482867db8f7fdb28ab6e78b47782c3518bc789f6570794965f2` both before and after target recovery. PostgreSQL 17 recovered the target, and `SELECT * FROM public.p05c_probe` returned `p05-c-final-source`. Its runtime mount inventory was exactly `p05c-final-target-verify-data:/var/lib/postgresql/data`; the running target had no source or archive mount. The source was unpaused immediately after the second hash. |
| Dirty-baseline verification | The preflight scope now matches `p05-scope.mjs`. The check still exits 1 solely because inherited dirty `crates/vestrace-application/Cargo.toml` lies outside the P05 allowlist; it was not changed. |
| Linux supervisor recovery harness | The disposable `p05cqual` Linux runner initially exposed an obsolete fixture cleanup which PostgreSQL rejected with SQLSTATE `0A000` because `managed_backup_sets` now references `installation_safety_state`. The fixture now uses disposable-only `TRUNCATE ... CASCADE`; its live `fault_after_witness_advance_reconciles_the_exact_single_entry` run passed in 2.27 s. |

This intermediate probe proved source-volume non-mutation across an independent target recovery. The following encrypted-manifest supervisor recovery section supplies the subsequently completed host witness/receipt and crash-boundary qualification.

### P05-C encrypted-manifest supervisor recovery

| Evidence | Observed result |
| --- | --- |
| Linux `safety_archive_recovery` lifecycle | Exit 0; the disposable `p05cqual` PostgreSQL runner committed one encrypted base member and one WAL member through the guarded supervisor lifecycle. The fixture cleanup uses `TRUNCATE ... CASCADE` only for disposable authority state. |
| Linux `safety_restore_recovery` crash/restart suite | Exit 0; 3 tests passed in 2.57 s. The subprocess flow created an encrypted base+WAL manifest, acquired its exact hold, froze the source, pinned the activation plan, and injected exit 87 before target initialization, after target initialization, and after `TargetActivating`. Each restart used the same target receipt and plan; the target tool ran once, the target generation became active, exactly one initialization event persisted, and only the matching hold was released. The suite also confirms that freeze/activate expose no operator-supplied receipt values and no generic hold-release command. |
| Clean forward-only 0209–0214 route, including `0214_managed_restore_safety_events.sql` | Exit 0 for the isolated PostgreSQL sequence: runtime applied `--through-version 208`, bootstrap installed the P05 safety and archive schemas while runtime was `NOLOGIN`, then runtime applied exactly one assertion at a time through 0209, 0210, 0211, 0212, 0213, and 0214. SQLx ledger ended `144:1:214:true`; each P05 row was successful. The new mirror guard was runtime `EXECUTE=false`, supervisor `EXECUTE=true`, and the journal event constraint admitted the required codes through 15. |
| P05 scope/provisioner and dispatcher regressions | Exit 0: `node --test tests/p05_scope.test.mjs tests/p05_provisioner_prefix.test.mjs` passed 16/16; isolated Linux `cargo test -p vestrace-infrastructure p05_only_paths_are_pinned_to_their_transactional_predecessors --lib -- --nocapture` passed 1/1. The latter covers the exact 0212→0213 and 0213→0214 paths as well as rejection of non-successor pairs. |
| P05 dirty-baseline verifier | Exit 1, as expected from the inherited dirty tree: `crates/vestrace-application/Cargo.toml` is outside P05 scope. No P05 path was reported as out of scope. |
| Current strict P05 clippy | Exit 0 in the isolated Linux container: `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings`. |
| Physical encrypted capture toward Task 6 | Exit 0 in the isolated `p05c-physical-supervisor`: `pg_basebackup` connected to a separate PostgreSQL 17 source through a replication DSN, with no source-data volume mounted in the supervisor. Its guarded set `01a09d5b-2f08-7153-b652-e3c9c061df2e` began with the encrypted physical base and two receiver-streamed 16 MiB WAL members, with descriptor ranges `100663336→117440512→134217728`; the next row records its frozen manifest and target result. |
| Physical encrypted restore, freeze, and activation | Exit 0 in the separate `p05c-physical-supervisor`, source, and target containers. The source quiescer terminated active `vestrace` sessions, denied new `vestrace` connections, checkpointed and switched WAL, then recorded a receipt at `150994944` (`0/9000000`). The guarded manifest added its third 16 MiB WAL member through that point. `restore materialize` subsequently recovered its exact existing target receipt, rather than rerunning the target tool. PostgreSQL 17 started from the fresh target volume, recovered through `0/9000028`, retained system identifier `7685079405851729958`, and contained all 20,000 rows of the WAL-created `postgres.p05c_wal_fill` table. Source PGDATA hash was `ffe1e8bbef3efaf60de07e7491de6066c96e9f22c61c1ff00aac85e85bde1cbb` before and after activation and target startup. Guarded state finished with target generation `82b74b76-ad36-4e87-908b-a8c952419127` at epoch 1, restore state `released`, and zero live restore holds. |
| Correctness defects found by the live harness | The harness exposed and the increment fixes two defects: restore descriptor queries omitted `backup_set_id`, and pre-CAS restore witness advances were not mirrored to PostgreSQL, causing the generation guard to reject a sequence gap. Neither fix weakens the generation CAS or host custody boundary. |

## P05-D Compose topology, host readiness, and the G0 aggregate

**Recorded:** 2026-09-14. Every row below is a command that was run, with its
observed exit. Nothing here is inferred from an earlier package.

### Task 2 — least-privilege Compose topology

| Evidence | Observed result |
| --- | --- |
| `docker compose -f docker-compose.yml config --quiet` | Exit 0 against Docker Compose v5.3.1. |
| `cargo test -p vestrace-cli --test safety_journal_compose` | Exit 0; 7 tests passed. Four assertions are new: every named stage declares an explicit prerequisite with an ordered condition, every health-gated edge names a service that actually declares a health check, only `vestrace-safety-journal-init` claims a root effective user, and no service requests privileged execution, added capabilities, or the host PID/network namespace. |
| Observed RED before repair | `every_declared_stage_names_an_explicit_ordered_prerequisite` failed with `vestrace-fingerprint-vault-init starts without an explicit prerequisite`. That service had no `depends_on` at all, so Compose was free to create the installation-fingerprint vault root before migration 0209 created the safety authority its continuity record binds to. |
| Repair | `vestrace-fingerprint-vault-init` now waits for `vestrace-migrate-p05` to complete successfully. No volume, mount, or privilege was added; `installation-safety-journal` still has exactly one declared consumer, and no supervisor, witness, archive, or host-key volume exists in Compose. |

### Task 3 — host-supervisor continuity readiness

| Evidence | Observed result |
| --- | --- |
| Clean forward-only route on a disposable PostgreSQL 17 | A fresh `p05d-postgres` built from the current `docker/postgres` applied `migrate --through-version 208` (exit 0, ledger 138/208), then the bootstrap stage installed the P05 safety schema while `vestrace` was `NOLOGIN`, then `migrate --only-version` applied 209, 210, 211, 212, 213, 214 and 215 one at a time, each exit 0. The final ledger was 145 rows, head 215, all successful. |
| Migration 0215 grant surface | `vestrace` EXECUTE `false`, `vestrace_safety_supervisor` EXECUTE `true`, supervisor `SELECT` on `installation_safety_state` `false`, volatility `s`, `SECURITY DEFINER` true, owner `vestrace_guarded_owner`. |
| `cargo test -p vestrace-cli --test safety_supervisor_readiness --test command_contract` (Linux runner) | Exit 0; 9 readiness tests and 20 command contracts passed with no BLOCKED path. The readiness cases cover absent roots, a relative root, an exact healthy state, non-mutation across two runs, a corrupt journal entry, a forged witness record, a mismatched persisted fingerprint, a stale persisted sequence and digest, and an uninitialized authority. |
| Windows | The seven database-backed readiness cases report blocked on this host: the bootstrap record's parent-directory fsync fails with `os error 5`, the same durability refusal P05-C recorded. They are proved in the Linux runner instead, and the refusal is not weakened to make them run. |
| Readiness is a read | `readiness_never_mutates_the_authority` runs readiness twice and compares the witness sequence, the journal entry count, and the persisted journal digest before and after. Readiness takes no permit, opens the witness and journal without creating either, reaches the bootstrap decoder only on the branch that compares, and rolls its database read back. |

### Task 4 — the G0 aggregate, exactly as emitted

| Evidence | Observed result |
| --- | --- |
| `node --test tests/p05_g0_gate.test.mjs` | Exit 0; 16 tests passed. The mutations each fail closed: a missing source file, a source whose bytes no longer match its digest, a nonzero recorded exit, evidence marked blocked and relabelled as a pass, a claim with no source at all, and an omitted criterion are all non-pass; an undeclared criterion id, a duplicated criterion, an unsupported schema version or gate, an unrecognised claim, and an absolute or escaping source path are refused outright with a nonzero exit. |
| `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json` | **Exit 1. Aggregate `blocked`: 1 pass, 1 blocked, 17 unknown.** |
| `g0-19` — current dirty work preserved, change boundary documented | `pass`. Sources: the dirty-baseline verifier (exit 0, no findings) and `tests/p05_scope.test.mjs` (exit 0, 9 tests). |
| `g0-17` — installer and Compose provision least-privilege roles, stores, readiness, grants | `blocked`. P05-D proves the Compose topology, the independent host safety stores, the supervisor-only readiness read and its grants, and the forward-only route. It does not re-prove the archiver or the create-only fingerprint-key identity/proof conjuncts, which belong to P05-A and P05-B, so the conjunction is not closed. |
| The other 17 criteria | `unknown`, each with a reason naming the package that owns it. In particular `g0-16` — permit, witnessed backup/restore, and crypto-erasure — is `unknown` even though P05-A through P05-C recorded evidence for it, because inheriting a pass from an earlier package is precisely the inference this collector exists to prevent. |

This is the whole G0 result P05 may report. It is not a G0 pass, and no part of
it should be read as one.

### Two inherited defects repaired

| Evidence | Observed result |
| --- | --- |
| Unadmitted P05-B manifest edit | P05-B gave `vestrace-application` a `ring` dev-dependency for the archive signing tests in `backup_archive.rs` and never admitted `crates/vestrace-application/Cargo.toml`. P05-C recorded the resulting verifier exit 1 as expected and left it. It is now admitted, with a `scope_amendments` entry stating what it is and why. |
| Preflight/scope collation divergence | The captured `change_scope_paths` held the same paths as `scripts/p05-scope.mjs` in a different collation. The verifier compares the two element by element, so this was a standing finding; the capture is now the module verbatim, and a regression test asserts it. |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | Exit 0, no findings. This is the first P05 package for which the verifier is clean. |

### What P05-D does not claim

No product server or worker was started, and no Compose service was brought up:
the topology is proved as resolved configuration only. No provider call was
made. Readiness was exercised against a disposable instance, not a deployment.
The archive, restore, and crypto-erasure criteria are not revisited. Full G0
closure is not claimed, inferred, or implied by any row above.
