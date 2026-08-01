# Vestrace Workspace Envelope Encryption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, key issuance, encryption of existing bytes, decrypt access, rotation, rewrap, re-encryption, cryptographic erasure, backup-policy changes or public administrative operations. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved design: `docs/superpowers/specs/2026-08-01-vestrace-workspace-envelope-encryption-design.md`;
- design merge commit: `9b3ad5c274907f0be7bcf4e195ab0b2858e2e999`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- State capture profiles ADR and implementation plan;
- signed Run export design and implementation plan;
- H2 authorization/policy plan;
- H6 context, Artifact and evidence plan;
- H8 connections, credentials and secret-backend plan;
- H10 observability/evaluation plan;
- H11 universal product-surface plan;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- preceding implementation-plan merge: `941f1fc32172c8efed3fb34c856ff94a37af028e`.

**Goal:** Implement workspace-scoped envelope encryption for policy-selected sensitive payload generations, with fresh per-object DEKs, opaque H8-managed workspace KEK generations, authenticated encryption, restart-safe KEK rewrap and DEK/algorithm re-encryption, truthful backup-aware cryptographic erasure, derived-projection cleanup and safe H11 administration without weakening H2, H6, PostgreSQL RLS or owning-Horizon authority.

**Architecture:** H6 remains the owner of sensitive bytes, object identity, classification, retention, purge and storage lifecycle. H8 remains the owner of workspace KEK references, key generations, wrap/unwrap/destroy operations and operation-bound key leases; raw KEKs never enter the application layer. H2 authorizes every provision, encrypt, decrypt, rotate, rewrap, re-encrypt, erase and restore action. H10 records content-free intents, outcomes and integrity evidence. H11 provides safe management and inspection surfaces. PostgreSQL stores only opaque key references, wrapped DEKs, authenticated-encryption metadata, operation state and tombstones. Ciphertext remains in H6-managed storage. This concern does not encrypt authority fields required for RLS, ordering, uniqueness, state transitions or optimistic concurrency.

**Tech stack:** Existing Vestrace v0.1 plus H2, H6, H8, H10 and H11; Rust Edition 2024; Tokio; Serde/Schemars; SQLx; PostgreSQL 17; repository canonical encoding; SHA-256; AES-256-GCM initial profile; H8-compatible key backends; zeroizing memory types; streaming H6 stores; deterministic cryptographic fixtures and test vectors; proptest; local backup/escrow and failure-injection fixtures.

---

## Global constraints

- H6 owns logical object identity, sensitive byte lifecycle, classification, quarantine, retention, export eligibility and physical purge.
- H8 owns KEK references, key-generation lifecycle, wrap/unwrap/destroy operations and key-operation leases.
- H2 owns authorization, risk classification, approval requirements and administrative scope.
- H10 owns content-free security audit, verification results and bounded operational metrics.
- H11 owns public/admin adapters only and never receives raw key material.
- The State Engine does not become a secret store, KMS, blob store or encryption authority.
- Every workspace is an independent cryptographic domain.
- A key reference, wrapped DEK, ciphertext hash, object ID, Artifact ID or storage locator is not a capability.
- Raw KEK bytes never enter domain/application types, PostgreSQL, H6 storage, logs, telemetry, errors, Run context, exports or backups.
- Raw DEK bytes never enter domain/application persistence or ordinary DTOs. Any infrastructure-local plaintext DEK lease is opaque to application code, short-lived, operation-bound and zeroized best-effort.
- Every new encrypted object generation receives a fresh DEK.
- A DEK is never reused between workspaces or independent payload generations.
- Nonce reuse under one DEK is prohibited.
- Initial mandatory encryption profile is `AeadAes256GcmV1`.
- Algorithm selection is repository-owned and policy-selected; models, extensions, imported bundles and untrusted payloads cannot negotiate or downgrade it.
- Authenticated encryption is mandatory. Unauthenticated encryption is unsupported.
- Ciphertext substitution across workspace, object, generation, purpose or classification must fail authentication.
- Encryption does not replace TLS, RLS, H2 authorization, H6 classification, H10 audit or H11 authentication.
- Encryption failure, H8 outage, lease expiry, wrap failure or storage failure never writes plaintext as fallback.
- Optional capture may safely downgrade according to the approved capture profile contract, but mandatory payload creation fails closed when encryption is required.
- Plaintext exists only for the minimum authorized operation lifetime.
- Plaintext is never written to ordinary logs, crash reports, temporary plaintext files or shared caches.
- Decrypted buffers use bounded allocation and zeroizing wrappers where practical.
- Model and Tool adapters receive only the exact authorized slice and do not receive reusable decrypt handles.
- Associated data is deterministic, versioned and binds workspace, object identity, generation, purpose, classification and algorithm.
- Associated data never contains raw secrets.
- Existing authority/RLS/concurrency fields remain queryable and are not opaquely encrypted, including workspace IDs, object IDs, revision numbers, state/status fields, ordering sequences, cursor values, idempotency hashes, lease state, timestamps required for scheduling and foreign keys required for integrity.
- Sensitive payload columns and H6 bytes are encrypted according to exact classification policy; metadata is minimized but not falsified.
- Existing H6 plaintext-content-hash semantics are not silently replaced. Where policy permits a workspace-keyed fingerprint, it is explicitly versioned and separately authorized.
- Physical storage paths are not derived directly from global plaintext hashes for encrypted payloads.
- Cross-workspace ciphertext or wrapped DEKs are never mounted or reused.
- Cross-workspace transfer decrypts under source authorization and re-encrypts with a fresh target DEK under target authorization.
- Cross-workspace plaintext deduplication is prohibited.
- Ordinary KEK rotation changes wrapped-DEK metadata only and does not rewrite ciphertext.
- DEK rotation or algorithm migration creates a new encrypted object generation and new ciphertext.
- Existing encrypted generations are immutable; rewrap creates an immutable wrap revision or atomic current-wrap binding, not an in-place semantic rewrite without history.
- Rotation and re-encryption jobs are restart-safe, resumable, idempotent and workspace-scoped.
- A retiring KEK cannot be destroyed until coverage, active-lease, backup/escrow and restore gates pass.
- Cryptographic erasure is distinct from logical delete and physical ciphertext purge.
- An erasure request requires exact scope, H2 decision, approvals where required, protected cutoff, audit intent and reconciliation.
- New decrypt leases are denied once an erasure request reaches its protected cutoff.
- Ambiguous key-destroy results become `OutcomeUnknown`; success is never inferred.
- Approved erasure outcomes are exactly:

```text
ErasedFromAllAuthorizedKeyCopies
ErasedFromPrimaryPendingBackupExpiry
BlockedByLegalHold
BlockedByEscrowPolicy
OutcomeUnknown
```

- `ErasedFromPrimaryPendingBackupExpiry` must not be presented as complete cryptographic erasure.
- Restore cannot fabricate a replacement key for old ciphertext.
- Restore cannot reactivate an erased generation merely because a backup contains old database rows or ciphertext.
- Backup/escrow inventories and expiry evidence are part of erasure truthfulness.
- FTS documents, pgvector embeddings, excerpts, thumbnails, summaries, caches, search indexes and other plaintext-derived projections are included in erasure scope.
- Derived projection cleanup is tracked separately and must reach a terminal verified state before full erasure is claimed where policy requires.
- Run export never includes workspace KEKs, raw/wrapped DEKs usable by a target, decrypt leases or backend key identifiers that grant access.
- Signed export may include ciphertext/tombstone metadata only under the export design and never transfers source key authority.
- Hidden reasoning, provider reasoning tokens, private scratchpads and credentials remain prohibited even when encryption exists.
- Existing applied migrations are never edited.
- This concern follows the webhook extension range and reserves:

```text
0100_workspace_key_policies_generations_and_algorithm_profiles.sql
0101_encrypted_object_generations_envelopes_and_wrap_revisions.sql
0102_key_rotation_rewrap_reencryption_jobs_and_items.sql
0103_cryptographic_erasure_tombstones_backup_evidence_and_cleanup.sql
0104_envelope_encryption_rls_indexes_constraints_and_worker_state.sql
```

- No other concern may reuse migration numbers `0100`–`0104`.
- CI requires no cloud KMS, production keys, public network, external object store or permanent credential.
- Future implementation branch: `feat/workspace-envelope-encryption`.

---

## Compatibility with existing H6 and H8 contracts

This extension adds encryption metadata and cryptographic operations around existing H6 byte lifecycles. It does not create a second Artifact identity or second secret backend.

Implementation rules:

- existing `Artifact`, `ArtifactRevision`, context/capture byte records and H6 storage ports remain authoritative;
- an `EncryptedObjectGeneration` references one exact owning-H6 object/revision/generation and never replaces that identity;
- ciphertext is stored through the owning H6 store and receives H6 provenance, retention and purge behavior;
- H8 existing secret-backend/connection abstractions are extended with typed workspace-key operations rather than bypassed with direct KMS SDK calls from application services;
- one composition root selects the active key backend and envelope crypto adapter;
- key backend SDK types cannot appear in domain/application signatures or PostgreSQL schemas;
- existing H8 credential records are not reused as workspace KEK records merely because both live behind a secret backend;
- existing H6 plaintext objects are migrated only through an explicit, resumable encryption adoption job after policy approval;
- no implementation task silently changes the representation of already-applied H6 bytes;
- existing H10 audit/capture records remain content-free where specified and receive only safe encryption outcome references;
- H11 admin routes call the same application services as CLI/SDK and cannot invoke backend-specific APIs directly.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  encryption/
    mod.rs
    algorithm.rs
    policy.rs
    object_scope.rs
    key_generation.rs
    envelope.rs
    wrap_revision.rs
    object_generation.rs
    lease.rs
    operation_receipt.rs
    rotation.rs
    reencryption.rs
    erasure.rs
    backup_evidence.rs
    cleanup.rs
    error.rs

crates/vestrace-application/src/
  encryption/
    mod.rs
    ports.rs
    commands.rs
    policy_service.rs
    key_generation_service.rs
    encrypt_service.rs
    decrypt_service.rs
    rewrap_service.rs
    reencryption_service.rs
    rotation_worker.rs
    erasure_service.rs
    erasure_worker.rs
    reconciliation.rs
    backup_evidence_service.rs
    cleanup_service.rs
    verification.rs
    audit.rs
  encryption/owners/
    mod.rs
    h2.rs
    h6.rs
    h8.rs
    h10.rs
  composition/encryption.rs

crates/vestrace-application/tests/
  workspace_key_policy.rs
  workspace_key_generation.rs
  envelope_encrypt_decrypt.rs
  envelope_associated_data.rs
  envelope_failure_semantics.rs
  kek_rewrap.rs
  dek_reencryption.rs
  algorithm_migration.rs
  cryptographic_erasure.rs
  erasure_reconciliation.rs
  backup_erasure_truthfulness.rs
  derived_projection_cleanup.rs
  cross_workspace_reencryption.rs

crates/vestrace-envelope-crypto-runtime/
  Cargo.toml
  src/
    lib.rs
    algorithm_registry.rs
    aes256_gcm_v1.rs
    associated_data.rs
    nonce.rs
    streaming.rs
    ciphertext_hash.rs
    keyed_fingerprint.rs
    zeroizing_buffer.rs
    test_vectors.rs
    error.rs

crates/vestrace-envelope-crypto-test-support/
  Cargo.toml
  src/
    lib.rs
    fake_key_backend.rs
    deterministic_rng.rs
    fixtures.rs
    corruption.rs
    lease.rs
    backup.rs
    cleanup.rs
    faults.rs
    conformance.rs

crates/vestrace-infrastructure/src/
  encryption/
    mod.rs
    key_backend.rs
    lease_adapter.rs
    envelope_adapter.rs
    storage_adapter.rs
  postgres/encryption/
    mod.rs
    policy_repository.rs
    key_generation_repository.rs
    object_generation_repository.rs
    envelope_repository.rs
    wrap_revision_repository.rs
    rotation_repository.rs
    reencryption_repository.rs
    erasure_repository.rs
    backup_evidence_repository.rs
    cleanup_repository.rs
    worker_repository.rs

crates/vestrace-infrastructure/src/postgres/artifact/
  repository.rs
  revision_repository.rs
  purge_repository.rs

crates/vestrace-infrastructure/src/postgres/context/
  repository.rs

crates/vestrace-infrastructure/src/postgres/observability/
  capture_repository.rs

crates/vestrace-infrastructure/src/postgres/memory/
  repository.rs
  projection_repository.rs

crates/vestrace-channel-http/src/
  routes/encryption.rs
  dto/encryption.rs

crates/vestrace-channel-cli/src/
  commands/encryption.rs

crates/vestrace-public-schema/src/
  encryption.rs

crates/vestrace-sdk-rust/src/
  encryption.rs

packages/sdk-typescript/src/
  encryption.ts
  generated/encryption.ts

schemas/
  encryption/v1/workspace-key-policy.schema.json
  encryption/v1/key-generation.schema.json
  encryption/v1/encrypted-object-generation.schema.json
  encryption/v1/rotation-job.schema.json
  encryption/v1/erasure-request.schema.json
  encryption/v1/erasure-tombstone.schema.json
  encryption/v1/backup-evidence.schema.json
  compatibility/encryption-schema-baseline.json

migrations/
  0100_workspace_key_policies_generations_and_algorithm_profiles.sql
  0101_encrypted_object_generations_envelopes_and_wrap_revisions.sql
  0102_key_rotation_rewrap_reencryption_jobs_and_items.sql
  0103_cryptographic_erasure_tombstones_backup_evidence_and_cleanup.sql
  0104_envelope_encryption_rls_indexes_constraints_and_worker_state.sql

tests/
  encryption_policy_persistence.rs
  encryption_key_generation_persistence.rs
  encrypted_object_generation_persistence.rs
  envelope_ciphertext_corruption.rs
  envelope_cross_workspace_substitution.rs
  envelope_nonce_uniqueness.rs
  envelope_plaintext_fallback.rs
  encryption_h6_atomicity.rs
  encryption_h8_boundary.rs
  encryption_decrypt_lease_expiry.rs
  encryption_rewrap_restart.rs
  encryption_reencryption_restart.rs
  encryption_erasure_restart.rs
  encryption_erasure_backup_expiry.rs
  encryption_erasure_outcome_unknown.rs
  encryption_projection_cleanup.rs
  encryption_restore_tombstone.rs
  encryption_cross_workspace_transfer.rs
  encryption_rls.rs
  encryption_h11_parity.rs
  encryption_acceptance.rs

scripts/
  verify-envelope-encryption-boundary.sh
  verify-no-plaintext-fallback.sh
  verify-no-raw-key-persistence.sh
  verify-encryption-nonce-uniqueness.sh
  verify-encryption-erasure-truthfulness.sh
  verify-encryption-migration-ownership.sh
```

---

## Normative domain contracts

### Identifiers

Add strongly typed IDs:

```text
WorkspaceKeyPolicyId
WorkspaceKeyPolicyRevisionId
WorkspaceKekGenerationId
EncryptionAlgorithmProfileId
EncryptedObjectGenerationId
EncryptedPayloadEnvelopeId
WrappedDekRevisionId
KeyOperationLeaseId
KeyOperationReceiptId
KeyRotationJobId
KeyRotationItemId
ReencryptionJobId
ReencryptionItemId
CryptographicErasureRequestId
CryptographicErasureTombstoneId
BackupKeyEvidenceId
DerivedProjectionCleanupJobId
DerivedProjectionCleanupItemId
EncryptionWorkerLeaseId
```

All IDs use the repository UUID strategy and reject nil values.

### Workspace key policy

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionRequirement {
    Forbidden,
    Optional,
    Required,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct WorkspaceKeyPolicyRevision {
    pub id: WorkspaceKeyPolicyRevisionId,
    pub policy_id: WorkspaceKeyPolicyId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub classification_requirements: Vec<ClassificationEncryptionRule>,
    pub default_algorithm_profile_id: EncryptionAlgorithmProfileId,
    pub decrypt_lease_max_duration: Duration,
    pub rotation_interval: Option<Duration>,
    pub maximum_retiring_duration: Duration,
    pub erasure_approval_profile_id: ApprovalProfileId,
    pub backup_key_policy_revision_id: BackupKeyPolicyRevisionId,
    pub projection_cleanup_policy_revision_id: ProjectionCleanupPolicyRevisionId,
    pub canonical_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub created_at: Timestamp,
}
```

Policy invariants:

- revision is positive and immutable;
- workspace ID is exact and cannot be wildcarded;
- `Required` never falls back to plaintext;
- `Forbidden` rejects encryption for payload classes that must remain content-free rather than storing unnecessary bytes;
- algorithm profile exists in the compiled registry;
- durations are positive and bounded by deployment policy;
- backup and cleanup policy references exist before activation;
- policy hash covers every semantic field in canonical order;
- changing a policy creates a new revision and never rewrites prior encryption decisions.

### Algorithm profile registry

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionAlgorithmKind {
    AeadAes256GcmV1,
}

pub struct EncryptionAlgorithmProfile {
    pub id: EncryptionAlgorithmProfileId,
    pub kind: EncryptionAlgorithmKind,
    pub key_length_bytes: u16,
    pub nonce_length_bytes: u16,
    pub tag_length_bytes: u16,
    pub associated_data_schema_version: u16,
    pub envelope_version: u16,
    pub maximum_plaintext_bytes: u64,
    pub streaming_contract_revision: String,
    pub test_vector_hash: [u8; 32],
}
```

Registry rules:

- registry is compiled repository content;
- runtime registration is prohibited;
- initial profile is exactly AES-256-GCM with a 256-bit DEK;
- profile metadata and test-vector hashes are release-baselined;
- changing cryptographic semantics creates a new profile ID;
- no untrusted payload may choose a profile outside current policy.

### Object scope

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EncryptedObjectType {
    ArtifactRevision,
    ContextSnapshotPayload,
    CapturePayload,
    MemoryContent,
    WebhookPayload,
    EvaluationFixture,
    RunExportBundle,
    BoundedSidePayload,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EncryptedObjectScope {
    pub workspace_id: WorkspaceId,
    pub object_type: EncryptedObjectType,
    pub object_id: uuid::Uuid,
    pub object_revision_or_generation: u64,
    pub purpose: EncryptionPurpose,
}
```

Scope rules:

- object ID and revision are exact owning-Horizon references;
- referenced object belongs to the same workspace;
- scope cannot be changed in place;
- a reference does not grant read/decrypt permission;
- object type determines the owning H6/Horizon adapter;
- one owning logical generation may have at most one current encrypted generation per exact representation role.

### KEK generation reference

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKekGenerationState {
    Planned,
    Active,
    Retiring,
    Retired,
    DestroyPending,
    Destroyed,
    Compromised,
    Unavailable,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct WorkspaceKekGeneration {
    pub id: WorkspaceKekGenerationId,
    pub workspace_id: WorkspaceId,
    pub backend_binding_revision_id: SecretBackendBindingRevisionId,
    pub opaque_backend_key_ref: OpaqueSecretBackendKeyRef,
    pub generation: u32,
    pub state: WorkspaceKekGenerationState,
    pub algorithm_profile_id: EncryptionAlgorithmProfileId,
    pub state_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
    pub activated_at: Option<Timestamp>,
    pub retired_at: Option<Timestamp>,
    pub destroyed_at: Option<Timestamp>,
}
```

KEK rules:

- raw KEK is not constructible in domain/application code;
- opaque backend reference is workspace-bound and non-exportable;
- one generation number is unique per workspace;
- only one generation is active for new wrap operations unless an explicit migration policy temporarily allows a bounded overlap;
- `Destroyed` is terminal and irreversible;
- `OutcomeUnknown` blocks new wraps, retirement completion and claims of erasure;
- lifecycle changes use expected state revision in commands;
- expected revisions are not persisted as historical expectation fields.

### Encrypted object generation

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EncryptedObjectGenerationState {
    Preparing,
    Available,
    Superseded,
    Quarantined,
    ErasurePending,
    Erased,
    Purged,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EncryptedObjectGeneration {
    pub id: EncryptedObjectGenerationId,
    pub workspace_id: WorkspaceId,
    pub scope: EncryptedObjectScope,
    pub generation: u32,
    pub state: EncryptedObjectGenerationState,
    pub algorithm_profile_id: EncryptionAlgorithmProfileId,
    pub current_wrap_revision_id: WrappedDekRevisionId,
    pub envelope_id: EncryptedPayloadEnvelopeId,
    pub ciphertext_artifact_revision_id: ArtifactRevisionId,
    pub classification: DataClassification,
    pub plaintext_length: u64,
    pub ciphertext_length: u64,
    pub ciphertext_hash: [u8; 32],
    pub plaintext_content_hash_ref: Option<ClassifiedHashRef>,
    pub producer_revision: String,
    pub state_revision: u64,
    pub created_at: Timestamp,
    pub available_at: Option<Timestamp>,
    pub superseded_at: Option<Timestamp>,
}
```

Generation invariants:

- generation number is positive and unique per exact scope;
- ciphertext Artifact revision belongs to the same workspace and has encryption provenance;
- `Available` requires successful ciphertext hash verification, AEAD test-decrypt/verification according to policy and durable current wrap;
- `Superseded` never becomes current again except through an explicit disaster-recovery procedure that still respects erasure tombstones;
- state changes require optimistic concurrency;
- operational worker leases do not increment logical state revision;
- plaintext bytes are never persisted in this record.

### Wrapped DEK revision

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct WrappedDekRevision {
    pub id: WrappedDekRevisionId,
    pub workspace_id: WorkspaceId,
    pub encrypted_object_generation_id: EncryptedObjectGenerationId,
    pub revision: u32,
    pub kek_generation_id: WorkspaceKekGenerationId,
    pub wrap_algorithm_profile: String,
    pub wrapped_dek: SecretBytesContainer,
    pub wrapped_dek_hash: [u8; 32],
    pub associated_data_hash: [u8; 32],
    pub key_operation_receipt_id: KeyOperationReceiptId,
    pub created_at: Timestamp,
}
```

Rules:

- `SecretBytesContainer` is an infrastructure/persistence-safe opaque byte container that never implements ordinary display/debug output;
- wrapped DEK is unusable without the exact H8 KEK generation and policy-authorized operation;
- revision is immutable;
- ordinary KEK rewrap creates a new revision and atomically advances `current_wrap_revision_id`;
- old wrap revisions are retained or purged according to backup/erasure policy and never treated as active;
- a wrapped DEK from another workspace is rejected before unwrap.

### Encrypted payload envelope

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EncryptedPayloadEnvelope {
    pub id: EncryptedPayloadEnvelopeId,
    pub envelope_version: u16,
    pub workspace_id: WorkspaceId,
    pub scope: EncryptedObjectScope,
    pub classification: DataClassification,
    pub algorithm_profile_id: EncryptionAlgorithmProfileId,
    pub payload_nonce_or_stream_header: BoundedBytes,
    pub associated_data_schema_version: u16,
    pub associated_data_hash: [u8; 32],
    pub plaintext_length: u64,
    pub ciphertext_length: u64,
    pub ciphertext_hash: [u8; 32],
    pub producer_revision: String,
    pub created_at: Timestamp,
}
```

Envelope excludes raw/wrapped DEK because wrap revisions have an independent lifecycle. It includes no backend storage locator exposed to public surfaces.

### Associated data

Canonical associated data includes:

```text
workspace_id
object_type
object_id
object_revision_or_generation
purpose
classification
algorithm_profile_id
encrypted_object_generation_id
producer_revision
associated_data_schema_version
```

Optional fields such as Artifact revision, retention class or classified plaintext hash are included only when the owning policy requires them.

Rules:

- canonical order and encoding are repository-versioned;
- associated-data hash is verified before decrypt;
- immutable identity change requires a new encrypted generation;
- a mismatched workspace/object/purpose/classification fails AEAD verification;
- current mutable status, lease owner and retry count are not included.

### Key operation lease

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyOperationKind {
    GenerateAndWrapDek,
    UnwrapForEncrypt,
    UnwrapForDecrypt,
    RewrapDek,
    VerifyCiphertext,
    DestroyKeyMaterial,
    ComputeWorkspaceFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyOperationLease {
    pub id: KeyOperationLeaseId,
    pub workspace_id: WorkspaceId,
    pub operation: KeyOperationKind,
    pub scope: EncryptedObjectScope,
    pub kek_generation_id: WorkspaceKekGenerationId,
    pub policy_decision_id: PolicyDecisionId,
    pub authorization_ticket_id: AuthorizationTicketId,
    pub exact_operation_fingerprint: [u8; 32],
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
    pub one_use: bool,
}
```

Lease rules:

- lease is opaque outside the H8/application boundary;
- lease cannot change workspace, object, purpose, key generation or operation;
- expiry and one-use consumption are enforced by H8;
- erasure cutoff revokes or denies new decrypt leases;
- lease bytes/tokens are never persisted in ordinary domain rows;
- a lease is not returned through public API or included in Run context/export.

### Key operation receipt

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyOperationOutcome {
    Succeeded,
    Denied,
    FailedRetryable,
    FailedPermanent,
    OutcomeUnknown,
}

pub struct KeyOperationReceipt {
    pub id: KeyOperationReceiptId,
    pub workspace_id: WorkspaceId,
    pub operation: KeyOperationKind,
    pub scope_hash: [u8; 32],
    pub key_generation_id: WorkspaceKekGenerationId,
    pub outcome: KeyOperationOutcome,
    pub backend_receipt_hash: Option<[u8; 32]>,
    pub policy_decision_id: PolicyDecisionId,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}
```

Receipt stores no raw key, wrapped DEK, backend error body or secret path.

---

## Cryptographic ports

### Workspace key backend

```rust
#[async_trait::async_trait]
pub trait WorkspaceKeyBackendPort: Send + Sync {
    async fn create_kek_generation(
        &self,
        request: CreateWorkspaceKekRequest,
    ) -> Result<CreateWorkspaceKekResult, WorkspaceKeyBackendError>;

    async fn acquire_operation_lease(
        &self,
        request: AcquireKeyOperationLease,
    ) -> Result<OpaqueKeyOperationLeaseHandle, WorkspaceKeyBackendError>;

    async fn rewrap_dek(
        &self,
        lease: OpaqueKeyOperationLeaseHandle,
        source_wrapped_dek: SecretBytesContainer,
        target_generation: WorkspaceKekGenerationId,
        associated_data: CanonicalAssociatedData,
    ) -> Result<RewrappedDek, WorkspaceKeyBackendError>;

    async fn destroy_kek_generation(
        &self,
        request: DestroyWorkspaceKekRequest,
    ) -> Result<KeyDestroyDisposition, WorkspaceKeyBackendError>;

    async fn reconcile_key_operation(
        &self,
        receipt_ref: KeyBackendReceiptRef,
    ) -> Result<KeyOperationReconciliation, WorkspaceKeyBackendError>;
}
```

### Envelope crypto adapter

```rust
#[async_trait::async_trait]
pub trait EnvelopeCryptoPort: Send + Sync {
    async fn encrypt_stream(
        &self,
        request: EncryptStreamRequest,
        plaintext: BoundedByteStream,
        ciphertext_sink: EncryptedByteSink,
    ) -> Result<EncryptStreamResult, EnvelopeCryptoError>;

    async fn decrypt_stream(
        &self,
        request: DecryptStreamRequest,
        ciphertext: BoundedByteStream,
        plaintext_sink: PlaintextConsumer,
    ) -> Result<DecryptStreamResult, EnvelopeCryptoError>;

    async fn verify_envelope(
        &self,
        request: VerifyEnvelopeRequest,
        ciphertext: BoundedByteStream,
    ) -> Result<EnvelopeVerificationResult, EnvelopeCryptoError>;
}
```

Port rules:

- application services never receive raw KEK bytes;
- application/domain persistence never receives raw DEK bytes;
- adapter may use an infrastructure-local zeroizing data-key lease but cannot expose it through trait results;
- plaintext streaming is bounded and never materialized as an unbounded `Vec<u8>`;
- crypto errors map to bounded categories;
- adapter verifies associated data, nonce rules, lengths and ciphertext hash;
- no plaintext is emitted before authentication semantics permit it under the chosen streaming profile.

### H6 encrypted storage

```rust
#[async_trait::async_trait]
pub trait EncryptedObjectStorePort: Send + Sync {
    async fn begin_ciphertext_revision(
        &self,
        request: BeginEncryptedRevision,
    ) -> Result<EncryptedRevisionSink, ArtifactStoreError>;

    async fn finalize_ciphertext_revision(
        &self,
        request: FinalizeEncryptedRevision,
    ) -> Result<ArtifactRevisionId, ArtifactStoreError>;

    async fn open_ciphertext(
        &self,
        revision_id: ArtifactRevisionId,
    ) -> Result<BoundedByteStream, ArtifactStoreError>;

    async fn purge_ciphertext(
        &self,
        revision_id: ArtifactRevisionId,
    ) -> Result<PurgeDisposition, ArtifactStoreError>;
}
```

H6 remains authoritative for quarantine/available/purge state.

---

## Encryption and decryption services

### Encrypt command

```rust
pub struct EncryptObjectGeneration {
    pub workspace_id: WorkspaceId,
    pub scope: EncryptedObjectScope,
    pub classification: DataClassification,
    pub source_revision: ExactSourceByteRevision,
    pub expected_source_state: ExactSourceState,
    pub requested_algorithm_profile_id: Option<EncryptionAlgorithmProfileId>,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}
```

Flow:

1. load exact H6/source object and active policy revision;
2. verify workspace, classification, source immutability and H2 permission;
3. select compiled algorithm profile and active workspace KEK generation;
4. reserve an `EncryptedObjectGeneration` in `Preparing` state;
5. acquire an exact one-use H8 generate/wrap operation lease;
6. begin H6 ciphertext revision;
7. stream plaintext through the envelope crypto adapter;
8. calculate/verify ciphertext hash, lengths, associated-data hash and nonce rules;
9. finalize H6 ciphertext revision with encryption provenance;
10. persist envelope, wrapped-DEK revision, key receipt and available generation atomically where database state is concerned;
11. mark source representation transition according to owning H6 policy;
12. emit content-free H10 audit fact.

Failure rules:

- no plaintext fallback;
- incomplete ciphertext staging is quarantined/purged;
- a finalized ciphertext revision without committed metadata remains unattached and is reconciled/purged;
- an operation with ambiguous H8 result does not mark generation available;
- idempotent retry returns the original operation/generation when canonical input matches.

### Decrypt query

```rust
pub struct DecryptObjectGeneration {
    pub workspace_id: WorkspaceId,
    pub encrypted_object_generation_id: EncryptedObjectGenerationId,
    pub exact_purpose: DecryptPurpose,
    pub consumer: AuthorizedPlaintextConsumer,
    pub requested_by: PrincipalId,
}
```

Flow:

1. authorize exact object, purpose, consumer and current classification;
2. reject erased, purge-pending, quarantined or superseded generation when policy disallows access;
3. reject if erasure cutoff denies new decrypt leases;
4. verify ciphertext hash before decrypt;
5. verify current wrap revision and KEK state;
6. acquire short-lived H8 unwrap/decrypt lease;
7. stream authenticated plaintext only to the exact consumer;
8. zeroize infrastructure buffers best-effort;
9. consume/expire lease;
10. persist content-free access outcome and bounded metrics.

Decrypt never returns a reusable raw buffer, key or lease through H11.

---

## KEK rotation and DEK re-encryption

### Rotation types

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyRotationType {
    KekRewrap,
    DekRotation,
    AlgorithmMigration,
    EncryptionAdoption,
}
```

### Rotation job

```rust
pub struct KeyRotationJob {
    pub id: KeyRotationJobId,
    pub workspace_id: WorkspaceId,
    pub rotation_type: KeyRotationType,
    pub source_kek_generation_id: Option<WorkspaceKekGenerationId>,
    pub target_kek_generation_id: WorkspaceKekGenerationId,
    pub source_algorithm_profile_id: Option<EncryptionAlgorithmProfileId>,
    pub target_algorithm_profile_id: EncryptionAlgorithmProfileId,
    pub policy_revision_id: WorkspaceKeyPolicyRevisionId,
    pub authorization_decision_id: PolicyDecisionId,
    pub status: RotationJobStatus,
    pub protected_cutoff: Timestamp,
    pub cursor: Option<OpaqueRotationCursor>,
    pub total_items: u64,
    pub completed_items: u64,
    pub failed_items: u64,
    pub outcome_unknown_items: u64,
    pub state_revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

`expected_state_revision` belongs to transition commands, not persisted state.

### KEK rewrap

For each item:

1. lock exact current wrap revision with optimistic concurrency;
2. verify source generation and target generation states;
3. acquire H8 rewrap lease;
4. rewrap the same DEK from source KEK to target KEK without decrypting payload bytes;
5. create immutable target `WrappedDekRevision`;
6. atomically advance current wrap revision;
7. verify target unwrap through a bounded verification operation where backend permits;
8. record item result and content-free receipt;
9. leave ciphertext, nonce, envelope and ciphertext hash unchanged.

Source KEK retirement gate requires:

- every eligible current generation points to a target wrap revision;
- no unresolved or `OutcomeUnknown` item remains;
- no active decrypt/rewrap lease uses source generation;
- backup/escrow policy is satisfied;
- restore tests recognize target generation;
- H10 verification passes.

### DEK rotation and algorithm migration

For each item:

1. authorize exact source generation and target policy;
2. decrypt source through an operation-bound lease;
3. allocate a fresh DEK and nonce/stream header;
4. stream plaintext directly from decrypt adapter to encrypt adapter without ordinary plaintext persistence;
5. write a new H6 ciphertext revision;
6. create a new envelope, wrap revision and encrypted generation;
7. verify new ciphertext independently;
8. atomically switch the owning H6 current encrypted representation binding;
9. mark old generation superseded;
10. schedule old ciphertext/wrap cleanup according to retention/erasure policy.

Re-encryption never edits the old envelope or ciphertext in place.

### Restart and idempotency

- item identity is unique by job + exact source generation;
- workers lease bounded batches;
- lease expiry permits another worker to resume;
- cursor is advisory and never substitutes for item uniqueness;
- retry with an existing target generation verifies and resumes rather than creating another generation;
- ambiguous H8 or H6 outcomes require reconciliation before retry;
- job completion is computed from durable item states.

---

## Cryptographic erasure

### Request

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CryptographicErasureScope {
    ObjectGeneration,
    ObjectAllGenerations,
    WorkspaceKekGeneration,
    WorkspaceAllEncryptedPayloads,
}

pub struct CryptographicErasureRequest {
    pub id: CryptographicErasureRequestId,
    pub workspace_id: WorkspaceId,
    pub scope: CryptographicErasureScope,
    pub object_scope: Option<EncryptedObjectScope>,
    pub encrypted_object_generation_id: Option<EncryptedObjectGenerationId>,
    pub kek_generation_id: Option<WorkspaceKekGenerationId>,
    pub reason: BoundedText,
    pub policy_revision_id: WorkspaceKeyPolicyRevisionId,
    pub policy_decision_id: PolicyDecisionId,
    pub authorization_ticket_id: AuthorizationTicketId,
    pub approval_grant_ids: Vec<ApprovalGrantId>,
    pub protected_cutoff: Timestamp,
    pub status: CryptographicErasureStatus,
    pub state_revision: u64,
    pub requested_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Request validation:

- exact scope fields are mutually consistent;
- broad workspace scope requires stronger policy/approval profile;
- legal hold and escrow policy are evaluated before execution;
- request cannot silently widen after creation;
- protected cutoff is immutable;
- new decrypt leases for the scope are denied after cutoff;
- conflicting rewrap/re-encryption work is paused or rejected through explicit arbitration.

### Erasure lifecycle

```text
Requested
→ Authorizing
→ FencingDecrypts
→ DestroyingPrimaryKeyCopies
→ CleaningDerivedProjections
→ EvaluatingBackupAndEscrow
→ Verifying
→ Completed

Terminal/blocked alternatives:
Denied
BlockedByLegalHold
BlockedByEscrowPolicy
OutcomeUnknown
Failed
CancelledBeforeCutoff
```

Cancellation is not allowed after protected cutoff unless policy defines a safe pre-destruction state and no destruction operation has begun.

### Object-level erasure

Object-level erasure:

- fences decrypt leases for exact generation/scope;
- retires/removes every usable current and historical wrapped-DEK copy from primary authorized stores;
- records backup/escrow copies that can still restore the wrapped DEK;
- creates a content-free tombstone;
- schedules H6 ciphertext purge separately;
- removes plaintext-derived projections;
- never claims all-copy erasure while a recoverable authorized key copy remains.

### KEK-generation erasure

Workspace KEK generation destruction requires:

- generation is not active for new wraps;
- no current encrypted generation depends solely on it unless those payloads are within the approved erasure scope;
- rewrap coverage is complete for retained payloads;
- active leases are expired/revoked;
- H8 destroy operation has a definite successful receipt;
- backup/escrow evidence supports the claimed outcome;
- restore policy cannot reactivate the generation.

Any ambiguous backend response sets generation and erasure request to `OutcomeUnknown` and starts reconciliation.

### Erasure outcome

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CryptographicErasureOutcome {
    ErasedFromAllAuthorizedKeyCopies,
    ErasedFromPrimaryPendingBackupExpiry,
    BlockedByLegalHold,
    BlockedByEscrowPolicy,
    OutcomeUnknown,
}
```

### Tombstone

```rust
pub struct CryptographicErasureTombstone {
    pub id: CryptographicErasureTombstoneId,
    pub workspace_id: WorkspaceId,
    pub scope_hash: [u8; 32],
    pub request_id: CryptographicErasureRequestId,
    pub outcome: CryptographicErasureOutcome,
    pub protected_cutoff: Timestamp,
    pub key_operation_receipt_ids: Vec<KeyOperationReceiptId>,
    pub backup_evidence_ids: Vec<BackupKeyEvidenceId>,
    pub projection_cleanup_job_id: DerivedProjectionCleanupJobId,
    pub ciphertext_purge_state: PurgeDisposition,
    pub verified_at: Option<Timestamp>,
    pub created_at: Timestamp,
}
```

Tombstone contains no key material, wrapped DEK, plaintext hash that policy forbids, backend secret path or raw failure message.

---

## Backup, escrow and restore truthfulness

### Backup key evidence

```rust
pub struct BackupKeyEvidence {
    pub id: BackupKeyEvidenceId,
    pub workspace_id: WorkspaceId,
    pub key_scope_hash: [u8; 32],
    pub backup_set_id: BackupSetId,
    pub escrow_policy_revision_id: BackupKeyPolicyRevisionId,
    pub recoverability: BackupKeyRecoverability,
    pub expires_at: Option<Timestamp>,
    pub verified_at: Timestamp,
    pub evidence_hash: [u8; 32],
}
```

Rules:

- evidence is content-free and does not contain key bytes;
- unknown inventory blocks all-copy erasure claims;
- backup expiry must be verified, not inferred from elapsed wall time alone;
- restore validates erasure tombstones before making encrypted generations available;
- a backup containing ciphertext without a usable key remains non-decryptable and is recorded accurately;
- a backup containing an old wrapped DEK and a usable KEK is a recoverable key copy and must be reflected;
- restore never creates a new key that decrypts historical ciphertext unless that key is a verified restoration of authorized retained material.

---

## Derived projection cleanup

### Cleanup job

```rust
pub struct DerivedProjectionCleanupJob {
    pub id: DerivedProjectionCleanupJobId,
    pub workspace_id: WorkspaceId,
    pub erasure_request_id: CryptographicErasureRequestId,
    pub scope_hash: [u8; 32],
    pub status: CleanupJobStatus,
    pub total_items: u64,
    pub completed_items: u64,
    pub failed_items: u64,
    pub state_revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Required cleanup adapters cover implemented instances of:

- PostgreSQL full-text vectors/documents;
- pgvector embeddings;
- excerpts and safe previews;
- summaries and consolidations;
- OCR/text extraction cache;
- model/context cache entries;
- evaluation fixtures and derived reports;
- webhook/rendered payload caches;
- local search and retrieval indexes;
- temporary encrypted/plaintext processing artifacts.

A cleanup adapter is idempotent and reports `Removed`, `AlreadyAbsent`, `Blocked`, `Failed` or `OutcomeUnknown`.

Full erasure verification fails while required projection items remain unresolved.

---

## Persistence model

### Migration `0100`

Create:

```text
workspace_key_policies
workspace_key_policy_revisions
workspace_kek_generations
encryption_algorithm_profile_bindings
workspace_key_generation_state_history
```

Constraints:

- one active policy revision per workspace;
- unique policy revision numbers;
- unique KEK generation numbers per workspace;
- only one active KEK generation for new wraps by default;
- raw key values are impossible columns;
- backend reference fields use opaque bounded containers;
- state transitions are append-audited.

### Migration `0101`

Create:

```text
encrypted_object_generations
encrypted_payload_envelopes
wrapped_dek_revisions
encrypted_object_current_bindings
key_operation_receipts
```

Constraints:

- unique object generation per exact scope;
- one current binding per exact owning representation;
- envelope and wrap revision workspace must match generation workspace;
- ciphertext Artifact revision workspace must match;
- lengths are nonnegative and bounded;
- hash lengths are exact;
- current wrap revision belongs to the generation;
- no public/debug projection includes wrapped DEK bytes.

### Migration `0102`

Create:

```text
key_rotation_jobs
key_rotation_items
reencryption_jobs
reencryption_items
key_operation_reconciliation_records
encryption_adoption_items
```

Constraints:

- unique item per job/source generation;
- bounded worker leases and retry counters;
- `OutcomeUnknown` items cannot be reset to pending without reconciliation decision;
- job counters are derived/verified from items;
- expected revisions are command inputs only.

### Migration `0103`

Create:

```text
cryptographic_erasure_requests
cryptographic_erasure_tombstones
backup_key_evidence
derived_projection_cleanup_jobs
derived_projection_cleanup_items
erasure_scope_fences
```

Constraints:

- exact immutable scope hash;
- one protected cutoff per request;
- tombstone unique per request;
- all-copy completion requires verified backup evidence and cleanup state;
- legal-hold/escrow blocks are explicit outcomes;
- raw keys/wrapped DEKs are absent from tombstones/evidence.

### Migration `0104`

Add:

- forced RLS to all workspace-scoped tables;
- composite workspace foreign keys;
- partial indexes for active/retiring/unknown work;
- uniqueness constraints for idempotency and current bindings;
- worker lease indexes;
- checks preventing terminal-state reopening;
- migration-ownership comments;
- database roles denying ordinary application reads of wrapped-DEK bytes except the dedicated encryption adapter role.

Migration tests must prove cross-workspace joins and direct raw-key persistence are impossible through supported roles.

---

## H11 surfaces

### Administrative HTTP surface

Versioned endpoints may include:

```text
GET  /admin/v1/workspaces/{workspace}/encryption/policy
POST /admin/v1/workspaces/{workspace}/encryption/policy-revisions
GET  /admin/v1/workspaces/{workspace}/encryption/key-generations
POST /admin/v1/workspaces/{workspace}/encryption/key-generations
POST /admin/v1/workspaces/{workspace}/encryption/rotations
GET  /admin/v1/workspaces/{workspace}/encryption/rotations/{job}
POST /admin/v1/workspaces/{workspace}/encryption/erasures
GET  /admin/v1/workspaces/{workspace}/encryption/erasures/{request}
GET  /admin/v1/workspaces/{workspace}/encryption/health
```

Surface rules:

- every mutation requires idempotency and expected revision where applicable;
- responses expose opaque IDs, states, counts, safe timestamps and bounded reason codes only;
- no raw/wrapped key bytes, backend key paths, leases or plaintext are returned;
- broad erasure requests require explicit typed confirmation and approval references;
- UI labels distinguish primary erasure from all-authorized-copy erasure;
- `OutcomeUnknown` is prominently nonterminal;
- HTTP, CLI and SDK call the same application services.

### CLI

```text
vestrace encryption policy show
vestrace encryption key-generation list
vestrace encryption rotate plan
vestrace encryption rotate start
vestrace encryption rotate status
vestrace encryption erasure request
vestrace encryption erasure status
vestrace encryption doctor
```

CLI never accepts raw keys on command line or environment variables for workspace KEKs.

### Public schemas and SDKs

Generated types expose management/status contracts only. Secret containers and backend adapters are excluded from public schemas.

---

## Failure semantics

### H8 unavailable during required encrypt

- operation fails closed;
- no plaintext fallback;
- staged ciphertext is quarantined/purged;
- content-free audit records `key_backend_unavailable`.

### Storage fails after encryption

- no available generation is committed;
- key/wrap operation receipt is retained safely;
- staged objects are reconciled/purged;
- retry uses idempotency and never reuses an uncertain nonce/DEK pair for different bytes.

### Database commit fails after finalized ciphertext

- unattached H6 ciphertext revision is discoverable by provenance/idempotency binding;
- reconciliation either attaches the exact verified revision or purges it;
- plaintext is not regenerated automatically without a new authorized operation.

### Ciphertext hash mismatch

- decrypt is blocked before plaintext output;
- generation becomes quarantined/degraded according to policy;
- AEAD is not treated as optional;
- bounded security audit is emitted.

### AEAD authentication failure

- no plaintext is returned;
- object is quarantined;
- substitution/corruption investigation begins;
- automatic re-encryption from corrupted bytes is prohibited.

### Lease expires mid-operation

- no new key operation begins;
- streaming operation aborts safely;
- partial outputs remain non-authoritative;
- retry obtains a new exact lease and new nonce/DEK when encryption outcome is uncertain.

### Rewrap outcome unknown

- current wrap binding remains unchanged unless target revision was durably verified;
- item becomes `OutcomeUnknown`;
- reconciliation checks backend receipt and target unwrap capability;
- blind repeat is prohibited.

### Destroy outcome unknown

- KEK generation and erasure request become `OutcomeUnknown`;
- new wrap/decrypt operations remain fenced according to protected cutoff;
- backend reconciliation is mandatory;
- erasure is not reported complete.

### Backup inventory unavailable

- primary destruction may proceed only if exact policy permits;
- all-copy erasure claim is blocked;
- outcome is `ErasedFromPrimaryPendingBackupExpiry`, `BlockedByEscrowPolicy` or `OutcomeUnknown` according to evidence.

### Projection cleanup failure

- key/ciphertext outcomes remain recorded accurately;
- full erasure verification remains incomplete;
- retry is idempotent and does not restore erased content.

---

## Implementation sequence

### Task 1: Add failing boundary and registry tests

- [ ] Add compile-fail tests proving raw KEK/DEK types cannot enter domain/application DTOs.
- [ ] Add `AeadAes256GcmV1` test-vector failures.
- [ ] Add associated-data substitution failures.
- [ ] Add nonce uniqueness property tests.
- [ ] Add no-plaintext-fallback tests.
- [ ] Add script checks for migration ownership and forbidden backend SDK imports.
- [ ] Commit tests only.

### Task 2: Implement domain primitives and algorithm registry

- [ ] Add typed IDs.
- [ ] Add policy, scope, profile, generation, envelope and receipt types.
- [ ] Add constructor invariants and canonical hashes.
- [ ] Make secret containers non-displayable/non-debuggable.
- [ ] Make initial tests pass.

### Task 3: Implement crypto runtime and deterministic conformance fixtures

- [ ] Implement canonical associated data.
- [ ] Implement AES-256-GCM profile.
- [ ] Implement nonce generation/validation.
- [ ] Implement streaming encrypt/decrypt/verify adapters.
- [ ] Implement zeroizing buffers.
- [ ] Add corruption, truncation, substitution and boundary test vectors.
- [ ] Verify no plaintext before authentication contract permits it.

### Task 4: Implement H8 workspace-key ports and fake backend

- [ ] Add opaque KEK generation references.
- [ ] Add operation-bound leases.
- [ ] Add create, rewrap, destroy and reconciliation operations.
- [ ] Add backend receipt hashing and safe error mapping.
- [ ] Add one-use/expiry/scope enforcement tests.
- [ ] Verify application code never observes raw key bytes.

### Task 5: Create migrations `0100` and `0101`

- [ ] Add workspace policy/key-generation tables.
- [ ] Add object generation/envelope/wrap/receipt tables.
- [ ] Add exact constraints and composite workspace foreign keys.
- [ ] Add restricted database role for wrapped-DEK access.
- [ ] Add migration and rollback-startup compatibility tests without down migrations.

### Task 6: Implement H6 encrypt/decrypt integration

- [ ] Add encrypted revision staging/finalization.
- [ ] Add atomic metadata/current-binding commit.
- [ ] Add unattached-ciphertext reconciliation.
- [ ] Add decrypt consumer boundary and buffer-lifetime tests.
- [ ] Add H10 content-free audit integration.
- [ ] Prove H6 remains the only byte lifecycle.

### Task 7: Add encryption adoption for existing eligible bytes

- [ ] Add explicit adoption job/item contracts.
- [ ] Add point-in-time source revision binding.
- [ ] Add restart/idempotency tests.
- [ ] Add mixed plaintext/ciphertext rollout gates.
- [ ] Prevent policy from claiming completion before coverage verification.

### Task 8: Create migration `0102` and implement KEK rewrap

- [ ] Add rotation/re-encryption job tables.
- [ ] Add worker leases and item uniqueness.
- [ ] Implement target KEK provisioning and activation.
- [ ] Implement immutable wrap revisions.
- [ ] Add source retirement coverage gate.
- [ ] Add restart, concurrent-worker and `OutcomeUnknown` tests.

### Task 9: Implement DEK rotation and algorithm migration

- [ ] Add streaming decrypt-to-encrypt pipeline.
- [ ] Require fresh DEK and nonce.
- [ ] Create immutable target generations.
- [ ] Atomically switch current binding.
- [ ] Schedule old generation cleanup.
- [ ] Add crash-at-every-boundary tests.

### Task 10: Create migrations `0103` and `0104`

- [ ] Add erasure, tombstone, backup-evidence and cleanup tables.
- [ ] Add forced RLS, indexes, worker state and terminal constraints.
- [ ] Add protected-scope decrypt fences.
- [ ] Add database-role and cross-workspace isolation tests.

### Task 11: Implement cryptographic erasure and reconciliation

- [ ] Implement exact scope validation and approval binding.
- [ ] Fence decrypt leases at protected cutoff.
- [ ] Implement primary wrapped-DEK/key destruction.
- [ ] Implement H8 destroy receipt reconciliation.
- [ ] Implement approved five-outcome model.
- [ ] Create immutable tombstones.
- [ ] Keep physical ciphertext purge separate.

### Task 12: Implement backup/escrow evidence and restore gates

- [ ] Inventory recoverable key copies.
- [ ] Verify backup expiry evidence.
- [ ] Block all-copy claims on unknown inventory.
- [ ] Make restore consult tombstones before activation.
- [ ] Prohibit fabricated replacement keys.
- [ ] Add disaster-recovery fixtures.

### Task 13: Implement derived projection cleanup

- [ ] Add cleanup adapter registry.
- [ ] Integrate implemented FTS/pgvector/excerpt/summary/cache stores.
- [ ] Make adapters idempotent and restart-safe.
- [ ] Bind cleanup completion to erasure verification.
- [ ] Add unknown/blocked projection tests.

### Task 14: Implement cross-workspace re-encryption boundary

- [ ] Authorize source decrypt and target encrypt separately.
- [ ] Stream plaintext without persisted intermediate.
- [ ] Allocate fresh target DEK and target envelope.
- [ ] Preserve provenance without source key authority.
- [ ] Prohibit cross-workspace ciphertext/wrap/dedup reuse.

### Task 15: Add H11 management surfaces

- [ ] Add HTTP admin routes and safe DTOs.
- [ ] Add CLI commands.
- [ ] Add Rust/TypeScript SDK status models.
- [ ] Generate public schemas and compatibility baseline.
- [ ] Add parity and error-redaction tests.
- [ ] Ensure no raw/wrapped key bytes are serializable publicly.

### Task 16: Add acceptance and operational verification

- [ ] Run unit, integration, property and migration tests.
- [ ] Run restart/fault-injection suites.
- [ ] Run RLS and database-role suites.
- [ ] Run no-key/no-plaintext scripts.
- [ ] Run backup/restore/erasure truthfulness suites.
- [ ] Verify documentation and release schema baselines.
- [ ] Use `superpowers:verification-before-completion` before reporting success.

---

## Rollout order

1. ship domain/runtime readers and schema support with encryption disabled;
2. apply migrations `0100`–`0104`;
3. configure and verify H8 workspace-key backend bindings;
4. create policy revisions and inactive KEK generations;
5. run deterministic backend/crypto health checks;
6. activate KEK generation for a test workspace;
7. enable encryption for newly created selected payload classes;
8. verify decrypt, restart, audit and H6 purge behavior;
9. run explicit adoption jobs for existing data by bounded cohorts;
10. verify coverage before tightening policy from optional to required;
11. exercise KEK rewrap and restore tests before retiring source generation;
12. enable erasure administration only after backup/escrow and projection-cleanup integrations pass;
13. keep broad workspace erasure behind the strongest H2 approval profile;
14. publish H11 surfaces only after secret-redaction and parity tests pass.

Rollback rules:

- disabling new encryption does not decrypt or rewrite existing encrypted generations;
- old readers that cannot understand encrypted representations are blocked by release compatibility gates;
- a KEK generation is not destroyed as a rollback mechanism;
- database down migrations are not generated;
- ciphertext remains governed by H6 even when feature activation is paused.

---

## Acceptance scenarios

### Scenario 1: Required encryption succeeds

A sensitive H6 payload is streamed to ciphertext, receives a fresh DEK, exact envelope and available H6 revision. No raw key is persisted.

### Scenario 2: H8 unavailable

Required payload creation fails closed and no plaintext representation is written as fallback.

### Scenario 3: Optional capture downgrade

Optional capture whose policy permits downgrade records safe metadata-only outcome without payload.

### Scenario 4: Same plaintext in two workspaces

Ciphertext, DEK, nonce and storage identity differ; no cross-workspace equality/dedup binding is created.

### Scenario 5: Ciphertext moved to another workspace

Associated-data/AEAD verification fails before plaintext output.

### Scenario 6: Ciphertext moved to another object

Object identity mismatch causes authentication failure.

### Scenario 7: Nonce reuse attempt

Constructor/runtime rejects reused nonce under the same DEK and security test fails the build.

### Scenario 8: Corrupted ciphertext hash

Decrypt stops before plaintext and object is quarantined.

### Scenario 9: Corrupted AEAD tag

No plaintext is released and a bounded integrity event is recorded.

### Scenario 10: KEK rotation

Wrapped DEK revision changes while ciphertext hash, nonce and encrypted object generation remain unchanged.

### Scenario 11: Rewrap crash before current-binding switch

Old wrap remains current; verified target revision is safely resumed or discarded.

### Scenario 12: Rewrap response lost

Item becomes `OutcomeUnknown`; no blind rewrap or retirement completion occurs.

### Scenario 13: DEK rotation

A fresh DEK, nonce, envelope, ciphertext Artifact and encrypted generation are created.

### Scenario 14: Algorithm migration

Source generation remains immutable; target profile creates a new generation and current binding switches only after verification.

### Scenario 15: Concurrent decrypt and erasure cutoff

Decrypt lease acquired before permitted cutoff follows exact policy; new leases after cutoff are denied.

### Scenario 16: Destroy response lost

Generation/request become `OutcomeUnknown`; UI does not claim erasure.

### Scenario 17: Primary erased, backup still recoverable

Outcome is `ErasedFromPrimaryPendingBackupExpiry`, not full erasure.

### Scenario 18: Legal hold

Erasure is `BlockedByLegalHold`; no key destruction begins.

### Scenario 19: Escrow policy block

Outcome is `BlockedByEscrowPolicy` with content-free evidence.

### Scenario 20: All authorized copies erased

Verified H8 receipts, backup evidence, expired leases and projection cleanup allow `ErasedFromAllAuthorizedKeyCopies`.

### Scenario 21: Projection cleanup fails

Full erasure verification remains incomplete even when primary key material is unavailable.

### Scenario 22: Restore after erasure

Tombstone prevents activation; restore does not fabricate a key.

### Scenario 23: Restore after KEK rewrap

Target generation is available and retained payload decrypts; retired source generation is unnecessary.

### Scenario 24: Run export

Bundle contains no KEK, usable wrapped DEK, lease or backend key path.

### Scenario 25: Cross-workspace transfer

Source decrypt and target encryption are separately authorized; fresh target DEK/ciphertext are produced.

### Scenario 26: Cross-workspace RLS attack

Repository query and database constraints reject access to another workspace envelope/wrap/key generation.

### Scenario 27: Public DTO serialization

No secret container, raw key, wrapped DEK or backend path is present.

### Scenario 28: Debug/error path

Errors contain bounded categories and references only; no key or plaintext bytes appear.

### Scenario 29: Database leak without H8

Stored ciphertext and wrapped DEKs cannot be decrypted through supported interfaces.

### Scenario 30: Blob-store leak without PostgreSQL/H8

Ciphertext has no usable key or authority metadata sufficient for decrypt.

### Scenario 31: Worker restart during adoption

Unique item binding resumes without duplicate current generations.

### Scenario 32: Worker restart during erasure

Protected cutoff and tombstone intent remain durable; no decrypt reopening occurs.

### Scenario 33: Active key compromised

Generation moves to `Compromised`; new wraps stop and authorized migration begins without silent destruction.

### Scenario 34: Key backend unavailable during decrypt

Plaintext is not returned; object state is not rewritten as corrupt solely because backend is unavailable.

### Scenario 35: Plaintext temporary file attempt

Boundary script/test rejects unsupported plaintext-temp implementation.

### Scenario 36: Large streaming payload

Encryption/decryption stays within bounded memory and preserves exact lengths/hashes.

### Scenario 37: H11 parity

HTTP and CLI produce the same policy/rotation/erasure application outcomes.

### Scenario 38: Idempotent request replay

Same key and canonical command returns the original operation; changed command conflicts.

### Scenario 39: Retiring KEK with unresolved item

Destroy gate refuses progression.

### Scenario 40: Unknown backup inventory

All-copy erasure claim is blocked and safe outcome remains explicit.

---

## Definition of done

Implementation is complete only when:

1. raw KEKs never enter application/domain/persistence/public types;
2. raw DEKs are never persisted or exposed beyond the opaque infrastructure lease boundary;
3. every encrypted generation uses a fresh DEK and valid nonce rules;
4. AES-256-GCM test vectors and substitution/corruption tests pass;
5. no plaintext fallback exists;
6. H6 remains the only byte/provenance lifecycle;
7. H8 remains the only workspace-key operation authority;
8. RLS and composite workspace constraints pass;
9. KEK rewrap leaves ciphertext unchanged;
10. DEK/algorithm migration creates a new generation;
11. jobs resume safely after restart;
12. ambiguous key operations remain `OutcomeUnknown` until reconciled;
13. erasure outcomes are backup/escrow truthful;
14. derived projections are included in erasure verification;
15. restore respects tombstones and cannot fabricate keys;
16. Run export/public surfaces contain no usable key material;
17. cross-workspace transfer always re-encrypts with fresh target material;
18. migrations `0100`–`0104` are forward-only and uniquely owned;
19. all acceptance, security, migration, restart and parity tests pass;
20. implementation work starts only after explicit authorization ending the documentation-only phase.

---

## Documentation-only boundary

Merging this plan authorizes only the implementation sequence and contracts. It does not authorize:

- creating `feat/workspace-envelope-encryption`;
- adding crypto/KMS dependencies;
- applying migrations `0100`–`0104`;
- creating KEK generations or leases;
- encrypting/decrypting production bytes;
- rotating, rewrapping or destroying keys;
- changing backup/escrow policy;
- performing cryptographic erasure;
- publishing administrative endpoints.

A separate explicit instruction is required before implementation.