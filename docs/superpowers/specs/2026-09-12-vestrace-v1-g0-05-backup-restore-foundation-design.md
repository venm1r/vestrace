# P05 Backup and Restore Foundation Design

## Purpose

P05 begins the G0 backup-and-restore work required by section 11.6 of the
frozen v1 design.  This document deliberately separates four independently
reviewable deliverables: (1) persisted safety authority, (2) archive and
retention, (3) restore/cutover, and (4) Compose and qualification evidence.
Only the first deliverable is planned for the first implementation slice.
The current P05 scope admits this document, its implementation plan, and the
scope/preflight guard.  Every source, migration, Compose, or test path needed
by a later slice requires an explicit P05 scope amendment before it is edited.

## Frozen inputs

| Authority | P05 use |
| --- | --- |
| `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` section 11.6 | Defines the monotonic witness, append-only journal, generation lineage, backup lifecycle, restore holds, freeze/drain order, and target activation rules. |
| P02 `InstallationMutationPermit` | Existing shared/exclusive PostgreSQL advisory-lock seam. P05 extends its use; it does not weaken its lock ordering. |
| P04 completion authority | The committed `embedding_legacy_retirement_gate` created by `vestrace_commit_legacy_plaintext_retirement()` is a prerequisite for the first product-managed backup. P05 never reopens embedding lifecycle or migration bytes. |
| `scripts/verify-dirty-baseline.mjs` | The dirty-tree and protected-authority verifier. It is frozen by P05. |

## Decomposition and delivery order

### P05-A — safety authority foundation

Persist a single installation safety row, immutable generation rows, a signed
append-only journal, and a host-owned witness interface.  The journal resides
on its own dedicated Compose safety volume, outside PostgreSQL data and every
backup; its signing key and the independent witness remain in separate host
key/witness records outside every Compose volume.  The witness has only
compare-and-advance semantics; it cannot import, reset, or replace its
installation/fingerprint identity.  Test adapters provide deterministic fault
points.  No backup object, archive key, target database volume, or public
restore command is created in this slice.

### P05-B — archive and retention

Build on P05-A with encrypted base/WAL object staging, append permits,
hash-chained checkpoint CAS, restore holds, sealing, and deletion preparation.
The archive object store is host-supervisor owned and is never a Compose
volume.  A base backup is accepted only after the current witness and the
database journal head agree.

### P05-C — restore and activation

Build on P05-B with a host supervisor that restores only into fresh target
volumes, follows source WAL, quiesces/drains the source, records one freeze
point, verifies ancestry, and performs the irreversible target-activation CAS.
The active database is never overwritten or mounted by the target.

### P05-D — Compose, installer, and G0 evidence

Give PostgreSQL, runtime, migrator, archive, and host supervisor least
privilege; wire readiness to witness/fingerprint continuity; and run the
source/target PostgreSQL fault harness. Compose describes containers and the
one dedicated `installation-safety-journal` volume only. The supervisor
executable, witness, archive root, and protected host-key roots stay outside
Compose-managed volumes; the journal stays on that dedicated volume.

## P05-A model

`InstallationSafetyState` is a singleton keyed by `InstallationId` with:

* immutable `FingerprintKeyId` and `FingerprintKeyContinuityProof`;
* exactly one `active_generation_id` and monotonic `activation_epoch`;
* the last committed `journal_sequence` and `journal_chain_digest`; and
* a `phase` of `Active`, `Quiescing`, `SourceFrozen`, or `TargetActivating`.

`DatabaseGeneration` is append-only.  Its identity, parent identity, parent
timeline/LSN, and creation reason never change.  A generation is either active,
frozen, prospective, sealed, or abandoned; an abandoned generation can never
become active.

`SafetyJournalEntry` is append-only and contains only typed identifiers,
generation/epoch, event kind, causal request id, PostgreSQL timeline/LSN,
mutation watermark, previous digest, digest, and timestamp.  It contains no
credential, plaintext, encrypted material bytes, archive key, or raw content
identifier.

`WitnessStateV1` is the versioned, canonically encoded full state carried in
every `WitnessHead`, `WitnessAdvance`, and `WitnessReceipt`. It has all frozen
section-11.6 fields from its first P05-A entry: `generation_lineage_state`,
`wal_checkpoints`, `backup_set_heads_and_restore_hold_lifecycle`,
`target_activation_plan`, and `resume_or_target_initialization_state`.
P05-A initializes one active source generation and uses explicit `None`/empty
values for fork LSN, prospective child, sealed/abandoned generations, WAL
checkpoints, backup heads/holds, target plan/progress, and resume/target-init
state. Later P05-B/C transitions may replace only their documented `None` with
the next monotonic value; they cannot introduce an unsigned field or alter the
canonical format.

`SafetyJournal` owns a durable append-only log on the dedicated Compose safety
volume. The host supervisor signs the canonical journal payload with the
separately held Ed25519 key and writes it as a new
`entries/<sequence>-<digest>.cbor` file with `create_new`; it fsyncs that file
and the entries directory before it can advance a witness. There is no mutable
journal head file and no rename-overwrite path, so a journal entry is immutable
once its directory entry exists.

`InstallationSafetyWitness` owns a durable record and a distinct Ed25519
receipt-signing key outside PostgreSQL and Compose. Its witness public key is
different from the journal signer public key and is pinned in the database
singleton during initialization. Its only mutation is:

```rust
async fn compare_and_advance(
    &self,
    expected: WitnessHead,
    next: WitnessAdvance,
) -> Result<WitnessReceipt, WitnessError>;
```

`WitnessAdvance` requires `next.sequence == expected.sequence + 1`, the exact
previous chain digest, the same installation and fingerprint identities, and a
strictly permitted state transition. It first verifies the detached Ed25519
signature on `SignedJournalEntry`. It returns canonical
`WitnessReceipt { installation_id, fingerprint_key_id, continuity_proof,
sequence, journal_digest, generation_id, activation_epoch,
witness_state, witness_public_key, signature }`, where `witness_state` is the
complete canonical `WitnessStateV1` encoding and `signature` is Ed25519 over
the length-delimited canonical encoding of every preceding receipt field,
including the exact state byte length and bytes, with domain
`vestrace-installation-witness-receipt-v1`. There is no `reset`,
`import`, `replace`, or arbitrary-write method.

The host bootstrap directory contains a create-only
`installation-safety-bootstrap-v1.cbor` record with the installation ID,
fingerprint ID/proof, journal signer public key, witness public key, and a
canonical SHA-256 digest. `vestrace safety-supervisor bootstrap` creates this
record with `create_new`, fsyncs it and its parent, then invokes the one-time
database initializer with exactly those values and its digest. The repository
verifies the record before the call. The initializer may bind keys only when no
safety singleton exists; on every later call it accepts the bootstrap digest
only and requires byte equality with both stored public keys and identities.
The record has no replace/import operation. This makes the first binding an
explicit host-custody enrollment, and makes any later key replacement fail.
The record is not an independent fingerprint authority: the supervisor loads
the existing `HostInstallationFingerprintVault`, recomputes its proof from the
host key, and requires exact equality with both the record and
`installation_fingerprint_continuity` through
`vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER,
BYTEA)` before bootstrap and every readiness result. The P05 guarded SQL
functions repeat the exact table comparison. A mismatch refuses bootstrap,
receipt issuance, readiness, and activation.

The database-side repository verifies the canonical receipt signature with the
pinned witness public key before invoking a guarded SQL function. The guarded
function independently calls the server-side
`vestrace_safety_ed25519_verify(message BYTEA, signature BYTEA, public_key
BYTEA) RETURNS BOOLEAN` verifier with the canonical receipt bytes and its
pinned witness key before it reads or writes a safety row. That C extension
uses `crypto_sign_verify_detached` from libsodium, accepts only 32-byte public
keys and 64-byte signatures, and returns false for malformed input. The
supervisor login has no EXECUTE grant on the verifier; only guarded-owner
functions invoke it. The guarded function accepts only the fixed
installation-supervisor database role and its fixed transaction context, then
validates every receipt field, signed-entry digest, previous digest, sequence,
and canonical witness-state bytes against the persisted head. It never
accepts a caller-supplied public key. A receipt for a different installation,
generation, fingerprint proof, previous digest, sequence, or verifier key is
rejected. A crashed operation may retry the exact append and receive the
existing event; it may not append a sibling event at the same sequence.

## First-slice interfaces

P05-A introduces the following public application interfaces.

```rust
pub struct WitnessHead {
    pub installation_id: InstallationId,
    pub fingerprint_key_id: FingerprintKeyId,
    pub continuity_proof: FingerprintKeyContinuityProof,
    pub sequence: u64,
    pub chain_digest: SafetyJournalDigest,
    pub active_generation_id: DatabaseGenerationId,
    pub activation_epoch: u64,
    pub state: WitnessStateV1,
}

pub enum SafetyEventKind { InstallationInitialized, GenerationRegistered }

pub struct WitnessAdvance {
    pub event_kind: SafetyEventKind,
    pub generation_id: DatabaseGenerationId,
    pub activation_epoch: u64,
    pub signed_entry: SignedJournalEntry,
    pub next_state: WitnessStateV1,
}

pub struct WitnessReceipt {
    pub installation_id: InstallationId,
    pub fingerprint_key_id: FingerprintKeyId,
    pub continuity_proof: FingerprintKeyContinuityProof,
    pub sequence: u64,
    pub journal_digest: SafetyJournalDigest,
    pub generation_id: DatabaseGenerationId,
    pub activation_epoch: u64,
    pub state: WitnessStateV1,
    pub witness_public_key: WitnessPublicKey,
    pub signature: WitnessReceiptSignature,
}

#[async_trait]
pub trait InstallationSafetyWitness: Send + Sync {
    async fn read_head(&self) -> Result<WitnessHead, WitnessError>;
    async fn compare_and_advance(
        &self,
        expected: WitnessHead,
        next: WitnessAdvance,
    ) -> Result<WitnessReceipt, WitnessError>;
}

#[async_trait]
pub trait SafetyAuthorityRepository: Send + Sync {
    async fn initialize_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        request: InitializeInstallationSafety,
        signed_entry: SignedJournalEntry,
        witness_receipt: WitnessReceipt,
    ) -> Result<InstallationSafetySnapshot, ApplicationError>;
    async fn register_generation_in(
        &self,
        unit_of_work: &mut dyn UnitOfWork,
        request: RegisterDatabaseGeneration,
        signed_entry: SignedJournalEntry,
        witness_receipt: WitnessReceipt,
    ) -> Result<InstallationSafetySnapshot, ApplicationError>;
    async fn current(&self) -> Result<Option<InstallationSafetySnapshot>, ApplicationError>;
}
```

`InstallationSupervisorContext` is a private application wrapper around the
fixed `(workspace_id = 00000000-0000-0000-0000-000000000005,
principal_id = 00000000-0000-0000-0000-000000000006)` request context. Only
the host supervisor composition constructs it. The P05 migration stores the
same pair in the guarded singleton, and every P05 guarded function first calls
`vestrace_assert_installation_supervisor_context()`: `session_user` must be
`vestrace_safety_supervisor`, and the transaction-local `vestrace.workspace_id`
and `vestrace.principal_id` must equal that singleton pair. The runtime role
has no EXECUTE grant, so inventing these public UUID values is insufficient to
call a safety function.

`SafetyAuthorityService` owns `InstallationMutationPermit`, `SafetyJournal`,
`InstallationSafetyWitness`, and the repository. Its public methods take an
`InstallationSupervisorContext`, acquire `PermitMode::Exclusive` through its
fixed request context before a safety-row guard,
append and fsync one signed journal entry, compare-and-advance the witness for
that exact entry, call `*_in` through the permit-owned transaction, and commit.
A witness receipt is therefore issuable only for an already fsynced signed
journal entry and an fsynced witness-record replacement: it writes the next
witness record to a same-directory temporary file, fsyncs it, renames it once,
and fsyncs the witness parent before signing/returning the receipt. If the
process dies after witness advancement but before the
database commit, `reconcile_exact_receipt` may commit only that exact signed
entry/receipt pair; it cannot issue a new sequence.

Only the P05 supervisor composition owns an `InstallationSafetyWitness`,
`SafetyJournalSigner`, `InstallationSupervisorContext`, and the corresponding
repository. Runtime HTTP, worker, and migration roles do not receive a witness
path, signing-key path, witness secret, supervisor credential, or direct
safety-table DML grant.

## First-slice failure rules

* Absence, truncation, malformed bytes, checksum/signature mismatch, an
  unpinned witness verification key, or fsync failure of the journal or
  witness record returns `WitnessError::Unavailable`
  and fails readiness.
* An out-of-date compare-and-advance returns `WitnessError::Conflict` without
  changing bytes.
* Database append failure after a witness receipt leaves a retryable exact
  signed-entry/receipt pair.  The retry must use the same identity, signature,
  sequence, previous digest, and witness head or fail closed.
* A database journal head that differs from the witness head fails readiness;
  a signed or valid journal prefix alone is insufficient.
* The P05-A schema does not claim backup/restore availability.  Its only
  observable readiness result is that the safety authority is internally
  continuous or has a typed blocker.

## Migration and privilege boundary

P05-A uses new forward migrations after `0208_embedding_memory_references.sql`.
Historical migrations `0001` through `0208` remain byte-for-byte protected.
Compose uses a fixed, trusted three-phase migration sequence. First the
ordinary `vestrace` migrator invokes `vestrace migrate --through-version 208`,
whose bounded `PgStore` runner uses the embedded migration prefix and rejects a
non-prefix ledger, while no product service starts. Next a bootstrap-only P05
service connected with the bootstrap credential verifies that exact ledger state
and the absence of `0209`, changes `vestrace` to `NOLOGIN`, and refuses if any
`vestrace` backend remains. It transfers the `public` schema to the guarded
NOLOGIN owner, revokes CREATE from both PUBLIC and `vestrace`, verifies that
`has_schema_privilege('vestrace', 'public', 'CREATE')` is false, and invokes the
fixed installer. Only after that succeeds may it re-enable the runtime login
and start `vestrace migrate --only-version 209`, whose runner first requires
the exact checked `0001`–`0208` ledger and then applies the sole pending 0209
entry. Product services depend on that final stage. Wrong, stale, partial, or
out-of-order ledgers fail either stage; neither stage silently runs all pending
migrations.
The bootstrap provisioner constructs the fixed safety tables/functions under
the guarded owner in the middle phase; the new migration asserts that exact
state and records it. The installed state stores the fixed supervisor context
and both pinned public keys, enables RLS, revokes runtime direct DML, and grants
only the bounded guarded functions required by `PgSafetyAuthorityRepository` to
a new non-owner host supervisor role. The role is provisioned only by
`docker/postgres/init-runtime-role.sh` from an externally supplied credential;
it is not present in a Compose service.  Initialization refuses unless the P04
`embedding_legacy_retirement_gate` row is committed through
`vestrace_commit_legacy_plaintext_retirement()`.  Direct SQL with the runtime
role must fail for every safety-table insert, update, delete, and generation
activation.

The PostgreSQL image builds and installs the P05-only
`vestrace_safety_verify` extension from `docker/postgres/vestrace_safety_verify`
using PostgreSQL 17 server headers and libsodium. Its SQL control file creates
the verifier function, revokes EXECUTE from PUBLIC/runtime/supervisor roles,
and grants it only to the guarded owner. The real
`init-runtime-role.sh` provisioner executes `CREATE EXTENSION
vestrace_safety_verify` while connected as its bootstrap-superuser, sets the
verifier owner to `vestrace_guarded_owner`, and applies its grants before
database ownership is handed to runtime and before the migrator starts.
The provisioner creates the non-owner `vestrace_safety_supervisor` login before
that `CREATE EXTENSION`, because the extension control SQL revokes verifier
access from that exact role and PostgreSQL refuses a grant or revoke naming an
unknown role. The fresh-image fixture exercises this exact order.
Migration `0209` only asserts the exact extension, verifier function, owner,
and grants before defining guarded safety functions. `docker-compose.yml`
switches `postgres` from the stock image to `docker/postgres/Dockerfile`; every
P05 PostgreSQL integration harness starts that same image and runs the real
provisioner before `0209`. A stock image or a missing verifier makes
migration/readiness fail closed; it never falls back to repository-only
verification.

The real provisioner creates, but does not yet invoke, one bootstrap-owned,
no-argument installer,
`vestrace_install_p05_safety_schema() RETURNS VOID`. It has
`SET search_path = pg_catalog` only, hard-codes the complete `0209` table and
function DDL, and schema-qualifies every intended `public` target. While
connected as bootstrap superuser it first requires that the transition service
has set `vestrace NOLOGIN` and observed no remaining runtime backend, then
alters `public` ownership to `vestrace_guarded_owner`, revokes CREATE from both
PUBLIC and `vestrace`, and asserts that `vestrace` has no effective schema
CREATE privilege. It then creates only that fixed list and transfers every object to
`vestrace_guarded_owner`, revokes PUBLIC/runtime privileges, and enables and
forces RLS where applicable. It accepts no object names, schemas, or payloads
from a caller; EXECUTE is revoked from PUBLIC and every non-bootstrap role.
The installer is idempotent only when every object has the expected guarded
owner and ACL *and* its fixed catalog-definition signature, otherwise it
raises. That signature covers table columns, types, defaults, nullability,
constraints, indexes, triggers, RLS policies, function identity/body/config,
and extension membership using the corresponding `pg_catalog` rows and
definition functions. `0209` does not create safety objects: it repeats the
same exact definition, ownership, RLS, and ACL assertions before recording the
migration. A fixture substitutes one same-named, guarded-owned object with a
wrong structural definition and requires both installer and 0209 to refuse it.
Thus the ordinary `vestrace` runtime never holds
schema CREATE during the P05 creation or migration window. No product service
starts before a fresh-image test runs the provisioner, applies exactly through
`0208`, proves a live runtime session makes the transition fail closed, retries
after that session exits, proves `public` is owned by the guarded NOLOGIN role,
and proves runtime `CREATE TABLE public.p05_injected (...)` is denied before
`0209`. It then applies the `0209`-only stage and again proves
`CREATE TABLE public.p05_escape (...)` is denied.

The host-only `SafetySupervisorConfig` reads
`VESTRACE_SAFETY_SUPERVISOR_DATABASE_URL` from the supervisor's protected host
configuration. It constructs one `PgStore` from that DSN and gives that same
store to `PgInstallationMutationPermit` and `PgSafetyAuthorityRepository`, so
the permit-owned transaction and guarded call use `session_user =
vestrace_safety_supervisor`. No Compose service receives this DSN. Supplying
the normal runtime DSN fails the guarded context assertion before journal
append.

Both outer safety functions are `SECURITY DEFINER`, owned by
`vestrace_guarded_owner`, and declare `SET search_path = pg_catalog, public`.
They call only the schema-qualified
`public.vestrace_safety_ed25519_verify(...)`; `public` has CREATE revoked from
all caller roles. They never resolve an unqualified helper, temporary object,
or caller-controlled search-path symbol.

`docker-compose.yml` declares exactly one named
`installation-safety-journal` volume and a one-shot
`vestrace-safety-journal-init` service that mounts only that volume to create
the journal directory with owner `10001` and mode `0700`. No server, worker,
migrator, PostgreSQL, or console service mounts it. The host supervisor receives
the engine-resolved path through `VESTRACE_SAFETY_JOURNAL_ROOT`; it refuses a
path that is not the declared volume mount. Readiness remains blocked when the
volume, initializer receipt, or host-root equivalence proof is absent.

The two executable functions are exactly
`vestrace_initialize_installation_safety(BYTEA, UUID, UUID, BYTEA, BYTEA,
BYTEA, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) RETURNS JSONB` and
`vestrace_register_database_generation(UUID, UUID, BYTEA, BIGINT, BYTEA,
BYTEA, UUID, BIGINT, BYTEA, BYTEA) RETURNS JSONB`. Their byte arguments are,
respectively, the bootstrap digest, continuity proof, journal signer public
key, witness public key, journal digest, journal signature, and witness
state bytes and receipt signature; the initializer accepts key bytes only for
the create-only bootstrap binding, while the generation function accepts none.
The singleton persists the canonical state bytes and state digest, and both
functions reject a receipt whose signed state bytes do not equal the proposed
monotonic transition. Both invoke
`vestrace_assert_installation_supervisor_context()` before reading or writing
a safety row.

## Evidence required before P05-A acceptance

1. Unit tests prove immutable per-sequence journal fsync-before-witness
   ordering, witness-record and parent fsync before receipt issuance, witness
   monotonicity, exact retry, conflict, malformed record/signature refusal, and
   no reset/import surface.
2. PostgreSQL integration tests prove RLS/privilege refusal, that a stale,
   forged, or signature-tampered receipt cannot advance the database head, that
   an incorrect supervisor context is refused, and that an existing shared
   permit blocks a safety transition before it appends a journal entry.
   They also prove a bootstrap fingerprint mismatch against either the host
   vault or `installation_fingerprint_continuity` is refused.
   A direct connection as `vestrace_safety_supervisor` with a bit-flipped
   receipt signature must be rejected by the guarded function, proving the
   server-side verifier rather than repository convention is decisive.
3. A process crash test kills the supervisor after witness advancement and
   before database commit; restart resolves only the same signed entry/receipt.
4. `node --test tests/p05_scope.test.mjs` proves the P05 scope protects P01-P04
   authority, root `PLAN.md`, and all migrations through `0208`.
5. A provisioner-backed PostgreSQL test proves `vestrace_safety_supervisor`
   has exactly the two P05 guarded-function grants, no safety-table privilege,
   no membership in owner/runtime roles, and all of `rolsuper`, `rolbypassrls`,
   `rolcreaterole`, `rolcreatedb`, and `rolreplication` false. Its actual login
   must fail database/schema CREATE, `SET ROLE`, and safety-table insert/update/delete.
6. A Compose test proves the dedicated volume/init-service mount inventory and
   readiness dependency; `docker compose config --quiet` must pass when Docker
   Compose is available, otherwise the evidence records it as blocked.
7. The dirty-baseline verifier passes for P05 while all P04 and unrelated dirty
   files retain their captured bytes.

## External corpus impact

| Decision | Registered external corpus impact | Rationale |
| --- | --- | --- |
| compatibility seam | No new corpus entry; P05-A exposes internal traits only. | The frozen v1 design remains the contract and no external protocol is added. |
