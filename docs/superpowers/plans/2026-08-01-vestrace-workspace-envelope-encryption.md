# Vestrace Workspace Envelope Encryption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, key issuance, encryption of existing bytes, decrypt access, rotation, rewrap, re-encryption, cryptographic erasure, backup-policy changes or public administrative operations. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved design: `docs/superpowers/specs/2026-08-01-vestrace-workspace-envelope-encryption-design.md`;
- design merge commit: `9b3ad5c274907f0be7bcf4e195ab0b2858e2e999`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- approved State capture profiles ADR and implementation plan;
- approved signed Run export design and implementation plan;
- H2 authorization/policy plan;
- H6 context, Artifact and evidence plan;
- H8 connections, credentials and secret-backend plan;
- H10 observability/evaluation plan;
- H11 universal product-surface plan;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- preceding implementation-plan merge: `941f1fc32172c8efed3fb34c856ff94a37af028e`.

**Goal:** Implement workspace-scoped envelope encryption for policy-selected sensitive payload generations, with fresh per-object DEKs, opaque H8-managed workspace KEK generations, authenticated encryption, restart-safe KEK rewrap and DEK/algorithm re-encryption, truthful backup-aware cryptographic erasure, derived-projection cleanup and safe H11 administration without weakening H2, H6, PostgreSQL RLS or owning-Horizon authority.

**Architecture:** H6 remains the owner of sensitive bytes, logical object identity, representations, classification, retention, purge and storage lifecycle. Ciphertext is an H6 byte representation of the existing owning object/revision, not a second Artifact identity. H8 remains the owner of workspace KEK references, key generations, wrap/unwrap/destroy operations and operation-bound key leases; raw KEKs never enter application code. H2 authorizes every provision, encrypt, decrypt, rotate, rewrap, re-encrypt, erase and restore action. H10 records content-free intents, outcomes and integrity evidence. H11 exposes only safe summaries and commands. PostgreSQL stores metadata plus restricted wrapped-DEK ciphertext, never raw keys or plaintext.

**Tech stack:** Existing Vestrace v0.1 plus H2, H6, H8, H10 and H11; Rust Edition 2024; Tokio; Serde/Schemars for safe contracts only; SQLx; PostgreSQL 17; SHA-256; AES-256-GCM initial profile; zeroizing memory types; H8-compatible key backends; streaming H6 byte stores; deterministic cryptographic fixtures and test vectors; proptest; local backup/escrow and failure-injection fixtures.

---

## Global constraints

- H6 owns logical byte-bearing resources and their representation lifecycle.
- H8 owns KEK references, key-generation lifecycle, wrap/unwrap/destroy operations and operation-bound leases.
- H2 owns authorization, risk classification, approvals and administrative scope.
- H10 owns content-free audit, verification evidence and bounded metrics.
- H11 owns adapters only and never receives key material.
- The State Engine does not become a KMS, secret store, blob store or encryption authority.
- Every workspace is an independent cryptographic domain.
- Key references, wrapped DEKs, ciphertext hashes, IDs and storage references are not capabilities.
- Raw KEK bytes never enter domain/application types, PostgreSQL, H6, logs, telemetry, errors, Run context, exports or backups.
- Raw DEK bytes never enter domain/application persistence or public DTOs.
- Any infrastructure-local plaintext DEK lease is opaque to application services, exact-scope, short-lived and zeroized best-effort.
- Secret backend key references and wrapped-DEK bytes are persistence-only secret containers. They do not implement `Debug`, `Display`, Serde, Schemars or public conversion traits.
- Public/domain status models contain only safe IDs, states, hashes and timestamps.
- Every new encrypted object generation receives a fresh DEK.
- A DEK is never reused across workspaces or independent payload generations.
- Nonce reuse under one DEK is prohibited.
- Initial mandatory profile is `AeadAes256GcmV1`.
- Algorithm selection is repository-owned and policy-bounded.
- Models, extensions, imports and webhooks cannot negotiate or downgrade cryptography.
- Authenticated encryption is mandatory.
- Ciphertext substitution across workspace, object, representation, generation, purpose or classification must fail.
- Encryption does not replace TLS, RLS, H2, H6 policy, H10 audit or H11 authentication.
- Required encryption failure never writes plaintext fallback.
- Optional capture may downgrade only according to the approved capture-profile contract.
- Plaintext exists only for the minimum authorized operation lifetime.
- Plaintext is never written to ordinary logs, crash reports, temporary plaintext files or shared caches.
- Decrypted buffers are bounded and use zeroizing wrappers where practical.
- Model/Tool adapters receive only an exact authorized slice and no reusable decrypt handle.
- Associated data is deterministic, versioned and binds workspace, owning object, representation role, generation, purpose, classification and algorithm.
- Associated data contains no secrets.
- Authority/RLS/concurrency fields remain queryable and are not opaquely encrypted, including workspace IDs, object IDs, revisions, states, sequences, cursors, idempotency hashes, lease state, required timestamps and foreign keys.
- Sensitive payload columns/bytes are encrypted according to exact policy; metadata is minimized but truthful.
- Existing H6 plaintext-content-hash semantics are not silently changed.
- A workspace-keyed fingerprint is a separate versioned feature and never a cross-workspace dedup key.
- Physical encrypted storage paths are not derived directly from globally comparable plaintext hashes.
- Ciphertext is stored as an H6 `ByteRepresentationRevision` or equivalent internal representation of the existing object; no second public Artifact is created.
- An encrypted representation ID has no independent public/model identity and cannot be mounted as another Artifact.
- Cross-workspace ciphertext, wrapped DEKs and key references are never reused.
- Cross-workspace transfer decrypts under source authorization and re-encrypts with a fresh target DEK under target authorization.
- Cross-workspace plaintext dedup is prohibited.
- Ordinary KEK rotation changes the active wrapped-DEK revision only; ciphertext remains unchanged.
- DEK rotation or algorithm migration creates a new encrypted generation and new H6 representation.
- Existing envelopes and encrypted generations are immutable.
- Rotation/re-encryption jobs are restart-safe, resumable, idempotent and workspace-scoped.
- A retiring KEK cannot be destroyed until coverage, active-lease, backup/escrow and restore gates pass.
- Cryptographic erasure is distinct from logical deletion and physical ciphertext purge.
- An erasure request requires exact scope, H2 decision, approvals where required, protected cutoff, audit intent and reconciliation.
- New decrypt leases are denied after the protected cutoff.
- Ambiguous key-destroy results are `OutcomeUnknown`; success is never inferred.
- Approved erasure outcomes are exactly:

```text
ErasedFromAllAuthorizedKeyCopies
ErasedFromPrimaryPendingBackupExpiry
BlockedByLegalHold
BlockedByEscrowPolicy
OutcomeUnknown
```

- `ErasedFromPrimaryPendingBackupExpiry` is never presented as full erasure.
- Restore cannot fabricate a key for old ciphertext.
- Restore cannot reactivate an erased generation because old rows/ciphertext reappeared.
- Backup/escrow inventories and expiry evidence are part of erasure truthfulness.
- FTS, pgvector, excerpts, previews, summaries, caches, OCR output and other plaintext-derived projections are in erasure scope.
- Full erasure requires verified projection cleanup where policy requires it.
- Run export contains no KEK, usable wrapped DEK, lease or authority-bearing backend reference.
- Exported ciphertext/tombstones do not transfer source key authority.
- Hidden reasoning, provider reasoning tokens, private scratchpads and credentials remain prohibited.
- Existing applied migrations are never edited.
- This concern follows the webhook range and reserves:

```text
0100_workspace_key_policies_generations_and_algorithm_profiles.sql
0101_encrypted_object_generations_envelopes_and_wrap_revisions.sql
0102_key_rotation_rewrap_reencryption_jobs_and_items.sql
0103_cryptographic_erasure_tombstones_backup_evidence_and_cleanup.sql
0104_envelope_encryption_rls_indexes_constraints_and_worker_state.sql
```

- No other concern may reuse `0100`–`0104`.
- CI requires no cloud KMS, production key, public network or external object store.
- Future implementation branch: `feat/workspace-envelope-encryption`.

---

## Compatibility with H6 and H8 baselines

This extension wraps existing H6 byte lifecycles. It does not create a second Artifact system or second secret backend.

Implementation rules:

- existing `Artifact`, `ArtifactRevision`, context/capture payload records and H6 stores remain authoritative;
- H6 introduces or reuses a non-public `ByteRepresentationRevisionRef` for exact stored byte representations;
- one logical H6 revision may point to a current encrypted representation without changing its public identity;
- an `EncryptedObjectGeneration` references one exact owning object/revision and one H6 byte representation;
- ciphertext representation receives H6 provenance, retention, quarantine and purge semantics;
- H8 secret-backend abstractions are extended with typed workspace-key operations;
- application code cannot call KMS/secret-backend SDKs directly;
- one composition root selects the key backend and crypto adapter;
- backend SDK types do not appear in domain/application/SQL schemas;
- existing H8 credential records are not repurposed as KEK-generation records;
- existing plaintext bytes are adopted only through an explicit resumable job;
- no task silently rewrites already-applied H6 representations;
- H10 records safe references/outcomes only;
- H11 routes use common application services and expose safe summaries only.

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
    wrap_metadata.rs
    object_generation.rs
    lease_metadata.rs
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
  encryption/owners/{mod,h2,h6,h8,h10}.rs
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
  src/{lib,algorithm_registry,aes256_gcm_v1,associated_data,nonce,streaming,ciphertext_hash,key_lease,zeroizing_buffer,test_vectors,error}.rs

crates/vestrace-envelope-crypto-test-support/
  Cargo.toml
  src/{lib,fake_key_backend,deterministic_rng,fixtures,corruption,lease,backup,cleanup,faults,conformance}.rs

crates/vestrace-infrastructure/src/encryption/
  mod.rs
  key_backend.rs
  key_backend_secret_types.rs
  lease_adapter.rs
  envelope_adapter.rs
  storage_adapter.rs

crates/vestrace-infrastructure/src/postgres/encryption/
  mod.rs
  policy_repository.rs
  key_generation_repository.rs
  backend_binding_repository.rs
  object_generation_repository.rs
  envelope_repository.rs
  wrap_secret_repository.rs
  rotation_repository.rs
  reencryption_repository.rs
  erasure_repository.rs
  backup_evidence_repository.rs
  cleanup_repository.rs
  worker_repository.rs

crates/vestrace-infrastructure/src/postgres/artifact/{repository,revision_repository,purge_repository}.rs
crates/vestrace-infrastructure/src/postgres/context/repository.rs
crates/vestrace-infrastructure/src/postgres/observability/capture_repository.rs
crates/vestrace-infrastructure/src/postgres/memory/{repository,projection_repository}.rs

crates/vestrace-channel-http/src/routes/encryption.rs
crates/vestrace-channel-http/src/dto/encryption.rs
crates/vestrace-channel-cli/src/commands/encryption.rs
crates/vestrace-public-schema/src/encryption.rs
crates/vestrace-sdk-rust/src/encryption.rs
packages/sdk-typescript/src/encryption.ts
packages/sdk-typescript/src/generated/encryption.ts

schemas/encryption/v1/workspace-key-policy.schema.json
schemas/encryption/v1/key-generation.schema.json
schemas/encryption/v1/encrypted-object-generation.schema.json
schemas/encryption/v1/rotation-job.schema.json
schemas/encryption/v1/erasure-request.schema.json
schemas/encryption/v1/erasure-tombstone.schema.json
schemas/encryption/v1/backup-evidence.schema.json
schemas/compatibility/encryption-schema-baseline.json

migrations/0100_workspace_key_policies_generations_and_algorithm_profiles.sql
migrations/0101_encrypted_object_generations_envelopes_and_wrap_revisions.sql
migrations/0102_key_rotation_rewrap_reencryption_jobs_and_items.sql
migrations/0103_cryptographic_erasure_tombstones_backup_evidence_and_cleanup.sql
migrations/0104_envelope_encryption_rls_indexes_constraints_and_worker_state.sql

tests/encryption_policy_persistence.rs
tests/encryption_key_generation_persistence.rs
tests/encrypted_object_generation_persistence.rs
tests/envelope_ciphertext_corruption.rs
tests/envelope_cross_workspace_substitution.rs
tests/envelope_nonce_uniqueness.rs
tests/envelope_plaintext_fallback.rs
tests/encryption_h6_representation_identity.rs
tests/encryption_h6_atomicity.rs
tests/encryption_h8_boundary.rs
tests/encryption_secret_serialization.rs
tests/encryption_decrypt_lease_expiry.rs
tests/encryption_rewrap_restart.rs
tests/encryption_reencryption_restart.rs
tests/encryption_erasure_restart.rs
tests/encryption_erasure_backup_expiry.rs
tests/encryption_erasure_outcome_unknown.rs
tests/encryption_projection_cleanup.rs
tests/encryption_restore_tombstone.rs
tests/encryption_cross_workspace_transfer.rs
tests/encryption_rls.rs
tests/encryption_h11_parity.rs
tests/encryption_acceptance.rs

scripts/verify-envelope-encryption-boundary.sh
scripts/verify-no-plaintext-fallback.sh
scripts/verify-no-raw-key-persistence.sh
scripts/verify-no-secret-serde.sh
scripts/verify-h6-representation-identity.sh
scripts/verify-encryption-nonce-uniqueness.sh
scripts/verify-encryption-erasure-truthfulness.sh
scripts/verify-encryption-migration-ownership.sh
```

---

## Safe domain contracts

### Identifiers

```text
WorkspaceKeyPolicyId
WorkspaceKeyPolicyRevisionId
WorkspaceKekGenerationId
WorkspaceKekBackendBindingId
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

All IDs follow the repository UUID strategy and reject nil values.

### Workspace policy

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

Invariants:

- immutable positive revision;
- exact workspace, never wildcard;
- `Required` has no plaintext fallback;
- algorithm profile is compiled and approved;
- durations are positive and bounded;
- backup/cleanup references exist before activation;
- semantic fields are canonical-hashed;
- history is never rewritten.

### Algorithm registry

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

Registry is compiled repository content. Runtime registration and untrusted negotiation are prohibited.

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
    pub representation_role: H6ByteRepresentationRole,
    pub purpose: EncryptionPurpose,
}
```

Scope references an existing owning object. It never creates a new Artifact identity.

### Safe KEK-generation summary

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
pub struct WorkspaceKekGenerationSummary {
    pub id: WorkspaceKekGenerationId,
    pub workspace_id: WorkspaceId,
    pub generation: u32,
    pub state: WorkspaceKekGenerationState,
    pub algorithm_profile_id: EncryptionAlgorithmProfileId,
    pub backend_class: SecretBackendClass,
    pub safe_fingerprint: BoundedFingerprint,
    pub state_revision: u64,
    pub created_at: Timestamp,
    pub activated_at: Option<Timestamp>,
    pub retired_at: Option<Timestamp>,
    pub destroyed_at: Option<Timestamp>,
}
```

The real backend key reference is absent from this type.

### Persistence-only KEK binding

Infrastructure defines a non-Serde, non-Schemars, non-Debug type:

```rust
pub struct PersistedWorkspaceKekBackendBinding {
    id: WorkspaceKekBackendBindingId,
    workspace_id: WorkspaceId,
    kek_generation_id: WorkspaceKekGenerationId,
    backend_binding_revision_id: SecretBackendBindingRevisionId,
    opaque_backend_key_ref: SecretBackendKeyReference,
    created_at: Timestamp,
}
```

Rules:

- `SecretBackendKeyReference` has no `Clone`, `Debug`, `Display`, Serde or Schemars implementation unless the backend requires a narrowly audited internal clone;
- repository methods return it only to the H8 adapter role;
- application/domain/public crates cannot import its module;
- database column is inaccessible to ordinary application/reporting roles;
- value is never exported or logged.

### H6 representation reference

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct H6ByteRepresentationRevisionRef {
    pub owning_object_type: H6OwningObjectType,
    pub owning_object_id: uuid::Uuid,
    pub owning_revision: u64,
    pub representation_revision_id: ByteRepresentationRevisionId,
    pub role: H6ByteRepresentationRole,
}
```

This ref is internal provenance. It does not represent a second public Artifact and cannot be independently mounted/exported without the owning object policy.

### Encrypted generation

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
    pub ciphertext_representation: H6ByteRepresentationRevisionRef,
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

Generation rules:

- positive unique generation per exact scope;
- representation belongs to the same owning object/workspace;
- no second Artifact identity;
- `Available` requires verified ciphertext, authenticated envelope and durable wrap;
- state changes use optimistic concurrency;
- worker leases do not increment logical revision;
- plaintext is absent.

### Safe wrap metadata

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct WrappedDekRevisionMetadata {
    pub id: WrappedDekRevisionId,
    pub workspace_id: WorkspaceId,
    pub encrypted_object_generation_id: EncryptedObjectGenerationId,
    pub revision: u32,
    pub kek_generation_id: WorkspaceKekGenerationId,
    pub wrap_algorithm_profile: String,
    pub wrapped_dek_hash: [u8; 32],
    pub associated_data_hash: [u8; 32],
    pub key_operation_receipt_id: KeyOperationReceiptId,
    pub created_at: Timestamp,
}
```

### Persistence-only wrapped DEK

```rust
pub struct PersistedWrappedDekSecret {
    metadata: WrappedDekRevisionMetadata,
    wrapped_dek: WrappedDekCiphertext,
}
```

`PersistedWrappedDekSecret` and `WrappedDekCiphertext`:

- are infrastructure-private;
- have no Serde, Schemars, Debug or Display implementation;
- are never returned through application queries;
- are read only by the dedicated H8/encryption adapter role;
- are rejected on workspace mismatch before unwrap;
- preserve immutable revision history according to retention/erasure policy.

### Envelope

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

Envelope excludes raw/wrapped keys and backend locators.

### Associated data

Canonical fields:

```text
workspace_id
object_type
object_id
object_revision_or_generation
representation_role
purpose
classification
algorithm_profile_id
encrypted_object_generation_id
producer_revision
associated_data_schema_version
```

A mismatch fails before plaintext is accepted.

### Lease metadata

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyOperationKind {
    GenerateAndWrapDek,
    UnwrapForDecrypt,
    RewrapDek,
    VerifyCiphertext,
    DestroyKeyMaterial,
    ComputeWorkspaceFingerprint,
}

pub struct KeyOperationLeaseMetadata {
    pub id: KeyOperationLeaseId,
    pub workspace_id: WorkspaceId,
    pub operation: KeyOperationKind,
    pub scope_hash: [u8; 32],
    pub kek_generation_id: WorkspaceKekGenerationId,
    pub policy_decision_id: PolicyDecisionId,
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
    pub one_use: bool,
}
```

The usable lease handle is an infrastructure-private non-serializable value and is never persisted.

### Operation receipt

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

Receipt contains no backend error body, path or secret.

---

## Cryptographic ports

### H8 key backend

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
        source_wrapped_dek: WrappedDekCiphertext,
        target_generation: WorkspaceKekGenerationId,
        associated_data: CanonicalAssociatedData,
    ) -> Result<RewrappedDekSecret, WorkspaceKeyBackendError>;

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

Opaque lease/secret results remain inside infrastructure adapters and are converted only to safe metadata/receipts for application code.

### Envelope crypto

```rust
#[async_trait::async_trait]
pub trait EnvelopeCryptoPort: Send + Sync {
    async fn encrypt_stream(
        &self,
        request: EncryptStreamRequest,
        plaintext: BoundedByteStream,
        ciphertext_sink: H6RepresentationSink,
    ) -> Result<EncryptStreamSafeResult, EnvelopeCryptoError>;

    async fn decrypt_stream(
        &self,
        request: DecryptStreamRequest,
        ciphertext: BoundedByteStream,
        plaintext_sink: AuthorizedPlaintextConsumer,
    ) -> Result<DecryptStreamSafeResult, EnvelopeCryptoError>;

    async fn verify_envelope(
        &self,
        request: VerifyEnvelopeRequest,
        ciphertext: BoundedByteStream,
    ) -> Result<EnvelopeVerificationResult, EnvelopeCryptoError>;
}
```

Safe results contain hashes, lengths, envelope metadata and receipt IDs, not keys/wrapped bytes.

### H6 representations

```rust
#[async_trait::async_trait]
pub trait EncryptedRepresentationStorePort: Send + Sync {
    async fn begin_ciphertext_representation(
        &self,
        request: BeginEncryptedRepresentation,
    ) -> Result<H6RepresentationSink, ArtifactStoreError>;

    async fn finalize_ciphertext_representation(
        &self,
        request: FinalizeEncryptedRepresentation,
    ) -> Result<H6ByteRepresentationRevisionRef, ArtifactStoreError>;

    async fn open_ciphertext(
        &self,
        representation: &H6ByteRepresentationRevisionRef,
    ) -> Result<BoundedByteStream, ArtifactStoreError>;

    async fn purge_ciphertext(
        &self,
        representation: &H6ByteRepresentationRevisionRef,
    ) -> Result<PurgeDisposition, ArtifactStoreError>;
}
```

H6 remains authoritative for representation availability/quarantine/purge and owning-object policy.

---

## Encrypt/decrypt workflows

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

1. load exact owning object/source bytes and active policy;
2. verify workspace, classification, immutability and H2 permission;
3. select compiled profile and active KEK generation;
4. reserve generation `Preparing`;
5. acquire exact one-use H8 lease;
6. begin H6 ciphertext representation for the same owning object;
7. stream plaintext through crypto adapter;
8. verify hashes, lengths, AAD and nonce rules;
9. finalize H6 representation with encryption provenance;
10. persist safe envelope/generation/wrap metadata and restricted secret rows atomically where DB state is concerned;
11. atomically update the owning H6 current-representation binding;
12. emit content-free audit.

Failure:

- no plaintext fallback;
- partial representation is quarantined/purged;
- finalized unattached representation is reconciled/purged;
- ambiguous H8 result does not make generation available;
- idempotent retry reuses the original logical operation only when canonical input matches.

### Decrypt query

```rust
pub struct DecryptObjectGeneration {
    pub workspace_id: WorkspaceId,
    pub encrypted_object_generation_id: EncryptedObjectGenerationId,
    pub exact_purpose: DecryptPurpose,
    pub consumer: AuthorizedPlaintextConsumerRef,
    pub requested_by: PrincipalId,
}
```

Flow:

1. authorize exact object, purpose, consumer and classification;
2. reject erased/quarantined/purge-pending state;
3. enforce erasure fence;
4. verify ciphertext hash;
5. load restricted wrap secret through dedicated adapter role;
6. acquire short-lived exact H8 lease;
7. stream authenticated plaintext only to exact consumer;
8. zeroize infrastructure buffers;
9. consume/expire lease;
10. record content-free outcome.

No reusable plaintext buffer, key, wrap or lease is returned through H11.

---

## Rotation and re-encryption

### Types

```rust
pub enum KeyRotationType {
    KekRewrap,
    DekRotation,
    AlgorithmMigration,
    EncryptionAdoption,
}
```

### Job

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

Expected revision belongs to commands, not persisted state.

### KEK rewrap

For each item:

1. lock exact current wrap metadata;
2. verify source/target KEK states;
3. load restricted wrapped-DEK secret through adapter role;
4. acquire H8 rewrap lease;
5. rewrap the same DEK without reading payload plaintext;
6. persist a new restricted wrap secret plus safe metadata;
7. atomically advance `current_wrap_revision_id`;
8. verify target unwrap where backend permits;
9. record safe receipt/item outcome;
10. leave ciphertext representation, nonce, envelope and ciphertext hash unchanged.

Source retirement requires complete coverage, no unresolved item, expired leases, backup/restore readiness and H10 verification.

### DEK/algorithm migration

For each item:

1. authorize exact source and target policy;
2. decrypt source using a bounded lease;
3. allocate fresh DEK/nonce;
4. stream decrypt output directly into target encrypt adapter;
5. create a new H6 representation of the same owning object;
6. create new envelope/wrap/generation;
7. independently verify target;
8. atomically switch owning H6 current representation;
9. supersede old generation;
10. schedule old representation/wrap cleanup.

No old envelope/ciphertext is edited in place.

### Restart semantics

- unique item = job + source generation;
- bounded worker leases;
- expired worker lease allows resume;
- cursor is advisory;
- existing target is verified/resumed rather than duplicated;
- ambiguous H8/H6 outcome requires reconciliation;
- job completion derives from durable item states.

---

## Cryptographic erasure

### Scope and request

```rust
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

Validation:

- exact mutually consistent scope;
- broad scope requires strongest policy profile;
- legal hold/escrow evaluated before destruction;
- immutable cutoff/scope;
- new decrypt leases denied after cutoff;
- conflicting jobs explicitly paused/rejected.

### Lifecycle

```text
Requested
→ Authorizing
→ FencingDecrypts
→ DestroyingPrimaryKeyCopies
→ CleaningDerivedProjections
→ EvaluatingBackupAndEscrow
→ Verifying
→ Completed

Alternatives:
Denied
BlockedByLegalHold
BlockedByEscrowPolicy
OutcomeUnknown
Failed
CancelledBeforeCutoff
```

### Object erasure

- fence exact decrypt scope;
- remove/retire every usable current/historical wrapped-DEK secret in primary authorized stores;
- inventory recoverable backup/escrow copies;
- create tombstone;
- schedule H6 representation purge separately;
- remove plaintext-derived projections;
- never claim all-copy erasure while a recoverable authorized key copy remains.

### KEK-generation destruction

Requires:

- no new wraps;
- retained payloads fully rewrapped;
- payloads intentionally erased are in approved scope;
- no active lease;
- definite H8 destroy receipt;
- truthful backup/escrow evidence;
- restore cannot reactivate generation.

Ambiguous response sets generation/request to `OutcomeUnknown` and triggers reconciliation.

### Outcome

```rust
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

No key/wrap/backend secret data is included.

---

## Backup, escrow and restore

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

- content-free; no key bytes;
- unknown inventory blocks all-copy claim;
- expiry is verified, not inferred solely from time;
- restore checks tombstones before activation;
- ciphertext without usable key is recorded as non-decryptable;
- old wrapped DEK + usable KEK is a recoverable copy;
- no fabricated replacement key.

---

## Derived projection cleanup

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

Adapters cover implemented FTS, pgvector, excerpts, previews, summaries, OCR/extraction caches, context/model caches, evaluation fixtures, rendered webhook payload caches, retrieval indexes and processing remnants.

Item dispositions:

```text
Removed
AlreadyAbsent
Blocked
Failed
OutcomeUnknown
```

Required unresolved items block full erasure verification.

---

## Persistence model

### `0100`

Create:

```text
workspace_key_policies
workspace_key_policy_revisions
workspace_kek_generations
workspace_kek_backend_bindings
encryption_algorithm_profile_bindings
workspace_key_generation_state_history
```

`workspace_kek_backend_bindings` is accessible only to the dedicated H8 adapter role and is excluded from ordinary views/dumps where supported.

### `0101`

Create:

```text
encrypted_object_generations
encrypted_payload_envelopes
wrapped_dek_revision_metadata
wrapped_dek_secret_values
encrypted_object_current_bindings
key_operation_receipts
```

`wrapped_dek_secret_values` is restricted to the dedicated adapter role. Safe metadata and secret ciphertext are separated.

Constraints:

- exact workspace composite keys;
- unique generation per scope;
- one current representation binding;
- representation belongs to owning object/workspace;
- current wrap metadata/secret pair match;
- exact hash lengths and bounded sizes;
- no public view includes secret values.

### `0102`

Create rotation, re-encryption, adoption and reconciliation jobs/items with unique item identities, bounded worker leases and non-resettable `OutcomeUnknown` without reconciliation.

### `0103`

Create erasure requests/tombstones, backup evidence, cleanup jobs/items and scope fences. All-copy completion requires verified evidence/cleanup.

### `0104`

Add forced RLS, composite workspace FKs, partial indexes, idempotency/current-binding uniqueness, worker lease indexes, terminal-state checks, migration ownership and database-role grants/denials.

Migration tests prove:

- no cross-workspace join through supported roles;
- ordinary application/reporting role cannot read backend key refs or wrapped-DEK secret values;
- public views contain safe metadata only;
- representation does not create a second Artifact identity.

---

## H11 surfaces

Potential admin endpoints:

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

Rules:

- idempotency and expected revision for mutations;
- safe IDs, states, counts, timestamps and bounded reason codes only;
- no key/wrap/backend ref/lease/plaintext;
- broad erasure requires typed confirmation and approvals;
- primary-only vs all-copy erasure clearly distinguished;
- `OutcomeUnknown` visibly nonterminal;
- HTTP/CLI/SDK use common services.

CLI:

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

CLI never accepts raw KEKs.

---

## Failure semantics

### Required encrypt with H8 unavailable

Fail closed; no plaintext fallback; staged representation quarantined/purged; safe audit fact.

### Storage failure after crypto begins

No available generation; partial representation non-authoritative; uncertain nonce/DEK pair never reused for different bytes.

### DB commit failure after representation finalized

Unattached representation is discovered by provenance/idempotency and either attached after exact verification or purged.

### Ciphertext hash mismatch

Block decrypt before plaintext; quarantine generation; record bounded integrity event.

### AEAD failure

No plaintext; quarantine; no automatic re-encryption from corrupted bytes.

### Lease expiry

Abort safely; partial output non-authoritative; retry obtains new exact lease and fresh material when outcome is uncertain.

### Rewrap unknown

Current wrap remains unchanged unless target is durably verified; item `OutcomeUnknown`; reconcile before retry.

### Destroy unknown

Generation/request `OutcomeUnknown`; fences remain; no complete-erasure claim.

### Backup inventory unavailable

Primary destruction only if exact policy permits; all-copy claim blocked; explicit safe outcome.

### Projection cleanup failure

Key/ciphertext outcomes stay accurate, but full erasure remains incomplete.

### Secret serialization attempt

Compile-fail/boundary tests reject Serde/Schemars/Debug/Display implementations and public conversion paths for persistence-only secret types.

---

## Implementation sequence

### Task 1: Boundary tests

- [ ] Compile-fail raw KEK/DEK/public-type tests.
- [ ] Compile-fail secret Serde/Schemars/Debug/Display tests.
- [ ] H6 representation identity tests.
- [ ] AES profile test-vector failures.
- [ ] AAD substitution failures.
- [ ] Nonce uniqueness property tests.
- [ ] No-plaintext-fallback tests.
- [ ] Migration ownership/backend SDK import scripts.

### Task 2: Safe domain primitives

- [ ] Typed IDs.
- [ ] Policy/profile/scope/generation/envelope/safe receipt types.
- [ ] Safe summaries separate from secret persistence types.
- [ ] Constructors/canonical hashes.
- [ ] Initial tests pass.

### Task 3: Infrastructure-private secret types

- [ ] Backend key binding secret type.
- [ ] Wrapped-DEK ciphertext secret type.
- [ ] Opaque lease handle.
- [ ] No public traits/conversions.
- [ ] Dedicated module/crate visibility and database role tests.

### Task 4: Crypto runtime

- [ ] Canonical AAD.
- [ ] AES-256-GCM profile.
- [ ] Nonce generation/validation.
- [ ] Streaming encrypt/decrypt/verify.
- [ ] Zeroizing buffers.
- [ ] Corruption/truncation/substitution vectors.

### Task 5: H8 ports/fake backend

- [ ] Create KEK generation.
- [ ] Exact one-use leases.
- [ ] Rewrap/destroy/reconcile.
- [ ] Safe receipt hashes/errors.
- [ ] Scope/expiry enforcement.

### Task 6: Migrations `0100`–`0101`

- [ ] Safe/secret table separation.
- [ ] Composite workspace constraints.
- [ ] Dedicated adapter role.
- [ ] H6 representation binding.
- [ ] Migration/startup compatibility tests.

### Task 7: H6 encrypt/decrypt integration

- [ ] Representation staging/finalization.
- [ ] Atomic safe/secret metadata plus current binding.
- [ ] Unattached representation reconciliation.
- [ ] Exact plaintext consumer boundary.
- [ ] H10 safe audit.
- [ ] Prove no second Artifact identity.

### Task 8: Existing-byte adoption

- [ ] Explicit job/items.
- [ ] Exact source revision binding.
- [ ] Restart/idempotency.
- [ ] Mixed representation rollout gates.
- [ ] Coverage verification.

### Task 9: Migration `0102` and KEK rewrap

- [ ] Job/item tables.
- [ ] Worker leases/uniqueness.
- [ ] Target generation activation.
- [ ] Restricted secret rewrap.
- [ ] Safe metadata/current switch.
- [ ] Retirement gate and unknown tests.

### Task 10: DEK rotation/algorithm migration

- [ ] Streaming decrypt-to-encrypt.
- [ ] Fresh DEK/nonce.
- [ ] New H6 representation, not Artifact.
- [ ] Atomic current switch.
- [ ] Old cleanup.
- [ ] Crash-boundary tests.

### Task 11: Migrations `0103`–`0104`

- [ ] Erasure/tombstone/evidence/cleanup/fence tables.
- [ ] Forced RLS/indexes/terminal constraints.
- [ ] Role and cross-workspace tests.

### Task 12: Erasure/reconciliation

- [ ] Exact scope/approval validation.
- [ ] Cutoff fences.
- [ ] Primary secret/key destruction.
- [ ] Destroy reconciliation.
- [ ] Five-outcome model.
- [ ] Tombstones.
- [ ] Separate physical purge.

### Task 13: Backup/restore gates

- [ ] Recoverable-copy inventory.
- [ ] Verified expiry.
- [ ] Block unknown all-copy claims.
- [ ] Restore tombstone checks.
- [ ] No fabricated keys.

### Task 14: Projection cleanup

- [ ] Adapter registry.
- [ ] Implemented FTS/pgvector/excerpt/summary/cache adapters.
- [ ] Idempotent restart.
- [ ] Bind to erasure verification.

### Task 15: Cross-workspace re-encryption

- [ ] Separate source/target authorization.
- [ ] No persisted plaintext intermediate.
- [ ] Fresh target DEK/representation.
- [ ] Provenance without source authority.
- [ ] No ciphertext/wrap/dedup reuse.

### Task 16: H11 surfaces

- [ ] HTTP admin routes/safe DTOs.
- [ ] CLI.
- [ ] SDK status types.
- [ ] Schemas/baseline.
- [ ] Parity/error-redaction.
- [ ] Secret serialization negative tests.

### Task 17: Verification

- [ ] Unit/integration/property/migration tests.
- [ ] Restart/fault injection.
- [ ] RLS/database roles.
- [ ] No-key/no-plaintext/no-secret-Serde scripts.
- [ ] Backup/restore/erasure truthfulness.
- [ ] Release schema baseline.
- [ ] `superpowers:verification-before-completion`.

---

## Rollout order

1. ship readers/runtime support disabled;
2. apply `0100`–`0104`;
3. configure H8 backend bindings;
4. create policy revisions/inactive KEKs;
5. run backend/crypto health tests;
6. activate one test-workspace KEK;
7. enable new-data encryption for selected classes;
8. verify decrypt/restart/audit/purge;
9. adopt existing bytes by cohorts;
10. verify coverage before `Optional → Required`;
11. exercise rewrap and restore before retirement;
12. enable erasure only after backup/projection integrations;
13. keep workspace-wide erasure behind strongest approvals;
14. publish H11 surfaces after redaction/parity tests.

Rollback:

- disabling new encryption does not decrypt/rewrite existing data;
- incompatible readers are blocked by release gates;
- KEK destruction is never rollback;
- no down migrations;
- H6 remains authoritative while activation is paused.

---

## Acceptance scenarios

1. Required encryption creates fresh DEK, envelope and H6 representation; no raw key persists.
2. H8 outage fails closed with no plaintext fallback.
3. Optional capture safely downgrades without payload.
4. Same plaintext in two workspaces produces unrelated crypto/storage identity.
5. Cross-workspace ciphertext substitution fails.
6. Cross-object substitution fails.
7. Nonce reuse is rejected.
8. Ciphertext hash corruption blocks decrypt.
9. AEAD tag corruption releases no plaintext.
10. KEK rotation changes only wrap revision.
11. Rewrap crash before switch leaves old wrap current.
12. Lost rewrap response becomes `OutcomeUnknown`.
13. DEK rotation creates fresh generation/representation.
14. Algorithm migration preserves immutable source.
15. New decrypt lease after erasure cutoff is denied.
16. Lost destroy response is not reported as erased.
17. Recoverable backup yields `ErasedFromPrimaryPendingBackupExpiry`.
18. Legal hold blocks destruction.
19. Escrow policy blocks destruction.
20. Verified all-copy destruction/cleanup permits full outcome.
21. Projection failure blocks full verification.
22. Restore after erasure cannot activate data.
23. Restore after rewrap uses target generation.
24. Run export contains no usable key/wrap/lease/backend ref.
25. Cross-workspace transfer creates fresh target representation.
26. RLS rejects foreign envelope/wrap/key access.
27. Public DTO contains no persistence-only secret type.
28. Debug/error contains no secret/plaintext.
29. Database leak without H8 cannot decrypt through supported interfaces.
30. Blob leak lacks usable key/authority.
31. Adoption resumes without duplicate current representation.
32. Erasure restart keeps cutoff/fences.
33. Compromised active key stops new wraps and starts authorized migration.
34. H8 decrypt outage does not mark ciphertext corrupt.
35. Plaintext temp-file implementation is rejected.
36. Large streaming payload remains bounded.
37. HTTP/CLI parity holds.
38. Same idempotency command returns original; changed content conflicts.
39. Retiring KEK with unresolved item cannot be destroyed.
40. Unknown backup inventory blocks all-copy claim.
41. Secret backend ref cannot derive Serde/Schemars/Debug/Display.
42. Wrapped-DEK secret cannot enter public/application query.
43. Ciphertext representation cannot be independently mounted as Artifact.
44. H6 owning object ID remains unchanged after adoption/re-encryption.

---

## Definition of done

1. Raw KEKs never enter application/domain/persistence/public values.
2. Raw DEKs are never persisted/exposed beyond opaque infrastructure lease.
3. Backend key refs/wrapped-DEK secrets are non-Serde/non-Schemars/non-Debug and role-restricted.
4. Ciphertext is an H6 representation, not a second Artifact identity.
5. Every generation uses fresh DEK/valid nonce.
6. AES test vectors/substitution/corruption tests pass.
7. No plaintext fallback exists.
8. H6 remains sole byte/provenance lifecycle.
9. H8 remains sole key-operation authority.
10. RLS/composite workspace constraints pass.
11. KEK rewrap leaves ciphertext unchanged.
12. DEK/algorithm migration creates new generation/representation.
13. Jobs resume safely.
14. Ambiguous operations remain `OutcomeUnknown` until reconciled.
15. Erasure claims are backup/escrow truthful.
16. Projection cleanup is verified.
17. Restore respects tombstones and cannot fabricate keys.
18. Export/public surfaces contain no usable key material.
19. Cross-workspace transfer re-encrypts fresh target data.
20. Migrations `0100`–`0104` are forward-only/uniquely owned.
21. All acceptance/security/migration/restart/parity tests pass.
22. Implementation starts only after explicit authorization ending documentation-only phase.

---

## Documentation-only boundary

Merging this plan authorizes only the plan. It does not authorize:

- creating `feat/workspace-envelope-encryption`;
- adding crypto/KMS dependencies;
- applying `0100`–`0104`;
- creating KEKs/leases;
- encrypting/decrypting production bytes;
- rotating/rewrapping/destroying keys;
- changing backup/escrow policy;
- performing erasure;
- publishing admin endpoints.

A separate explicit instruction is required before implementation.