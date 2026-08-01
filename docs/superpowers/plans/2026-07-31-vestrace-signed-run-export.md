# Vestrace Signed Portable Run Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, signing-key issuance, real bundle creation, import, public endpoints, schema publication or external delivery. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved design: `docs/superpowers/specs/2026-08-01-vestrace-signed-run-export-design.md`;
- design merge commit: `c27e76fda273c6aa82e622f0b47f32e4de69d78a`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- durable event compatibility ADR: `docs/superpowers/specs/2026-07-31-vestrace-event-schema-compatibility-adr.md`;
- event compatibility implementation plan: `docs/superpowers/plans/2026-07-31-vestrace-event-schema-compatibility.md`;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- preceding implementation-plan merge: `9c2028f5746105d9e0aaf9c0444bcf592fe3b383`.

**Goal:** Implement `vestrace-run-export` as a signed, transport-neutral and policy-bounded evidence bundle for one exact H1 Run snapshot boundary, with deterministic manifests/checksums, H6-governed bytes, H8-backed signing, H10 audit proof, H11 surfaces and inert import modes that never transfer authority or resume execution.

**Architecture:** H1 remains authoritative for Run state and journal order. H2 authorizes exact export/import scope. H5 supplies immutable plan/delegation references. H6 owns every bundle, included Artifact byte and tombstone lifecycle. H8 performs signing and verification through operation-bound cryptographic ports without exposing private keys. H10 supplies audit-chain evidence and consumes verified bundles only as inert evidence/evaluation input. H11 exposes HTTP/CLI/MCP/SDK adapters over one application contract. PostgreSQL stores operation metadata, immutable manifests, references, receipts and lifecycle state; bundle bytes live only as H6 Artifact revisions.

**Tech stack:** Existing Vestrace v0.1 plus H1, H2, H5, H6, H8, H10, H11 and the durable event schema compatibility extension; Rust Edition 2024; Tokio; Serde/Schemars; SQLx; PostgreSQL 17; SHA-256; Ed25519 through H8; JSON Schema 2020-12; streaming archive/container adapters; H6 Artifact stores; deterministic clocks; proptest; local cryptographic and storage fixtures.

---

## Global constraints

- H1 remains the only owner of `AgentRun`, `RunStep`, `RunEvent`, Run version/sequence, checkpoints and logical replay.
- Export never mutates a Run and never increments `RunVersion`.
- An active Run may be exported only from a consistent exact point-in-time boundary.
- Parent Run export does not include SubRuns automatically.
- H2 separately authorizes snapshot selection, each sensitive content class, signing, local bundle creation, download/delivery and import mode.
- H6 remains the only permanent byte/provenance/retention/purge lifecycle for the completed bundle and all included content.
- H8 remains the only authority for signing private-key use. Private keys, raw secret material, credential bytes and usable leases never enter manifests, work items, PostgreSQL payloads or bundle entries.
- H10 audit-chain proof and the Run export signature are distinct evidence layers and are verified independently.
- H11 transport containers do not define semantic integrity. Compression level, archive ordering metadata, timestamps and byte offsets are excluded from the signed meaning.
- Export is an inert evidence package, not a database dump, deployment backup, replay package, authority transfer or workspace clone.
- Import never creates or resumes an H1 Run, enqueues work, activates a checkpoint, creates a credential lease, grants approval, activates memory, sends notification or dispatches a Tool/remote agent.
- Imported source identifiers remain namespaced provenance identifiers and never automatically become local authority IDs.
- A valid signature proves integrity only. Signer trust is a separate policy decision based on configured trust anchors, key status and source identity.
- Unknown event schemas may be preserved as opaque bytes after integrity verification, but block semantic import, evaluation activation and migration staging until supported.
- Bundle-supplied executable code, upcasters, templates or scripts are never executed.
- Historical event identity, original schema version, source sequence/cursor and original payload hash remain unchanged in the bundle.
- Optional omissions are explicit, signed and typed. Missing content is never represented as an empty payload that could be mistaken for the original.
- Purged, withheld, quarantined or unavailable content uses a signed tombstone or omission descriptor.
- Hidden chain-of-thought, provider reasoning-token content, private model scratchpads, credentials and unrestricted external bodies remain prohibited.
- Redaction or secret-scan failure never falls back to full unredacted content.
- External delivery is a separate operation from local bundle creation. Ambiguous external delivery never causes bundle rebuilding or blind resend without idempotency/reconciliation.
- Existing applied migrations are never edited. This concern reserves:

```text
0092_run_export_operations_snapshots_and_idempotency.sql
0093_run_export_entries_signatures_and_tombstones.sql
0094_run_export_imports_trust_and_remaps.sql
0095_run_export_rls_indexes_and_worker_state.sql
```

- No other concern may reuse migration numbers `0092`–`0095`.
- CI requires no public model, cloud KMS, external object store, public network or permanent signing key.
- Future implementation branch: `feat/signed-run-export`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  run_export/{mod,format,selection,snapshot,operation,entry,tombstone,signature,verification,import,trust,remap,error}.rs

crates/vestrace-application/src/
  run_export/{mod,ports,commands,service,snapshot_service,selection_service,builder,validator,signer,verifier,import_service,trust_service,remap_service,worker}.rs
  run_export/owners/{mod,h1,h2,h5,h6,h8,h10}.rs
  composition/run_export.rs

crates/vestrace-application/tests/
  run_export_snapshot.rs
  run_export_selection.rs
  run_export_manifest.rs
  run_export_signing.rs
  run_export_verification.rs
  run_export_import_modes.rs
  run_export_trust.rs
  run_export_remap.rs
  run_export_failure_semantics.rs

crates/vestrace-run-export-runtime/
  Cargo.toml
  src/{lib,canonical_path,manifest,checksums,root_hash,container,streaming,validator,signature,reader,error}.rs

crates/vestrace-run-export-test-support/
  Cargo.toml
  src/{lib,fixtures,run_snapshot,artifact_store,signer,trust,corruption,containers,conformance}.rs

crates/vestrace-infrastructure/src/postgres/
  run_export/{mod,operation_repository,snapshot_repository,entry_repository,tombstone_repository,signature_repository,import_repository,trust_repository,remap_repository,worker_repository}.rs

crates/vestrace-channel-http/src/
  routes/run_exports.rs
  dto/run_export.rs

crates/vestrace-channel-cli/src/
  commands/run_export.rs

crates/vestrace-channel-mcp/src/
  tools/run_export.rs
  resources/run_export.rs

crates/vestrace-public-schema/src/
  run_export.rs

crates/vestrace-sdk-rust/src/
  run_exports.rs

packages/sdk-typescript/src/
  run_exports.ts
  generated/run-export.ts

schemas/
  run-export/v1/manifest.schema.json
  run-export/v1/run.schema.json
  run-export/v1/event-line.schema.json
  run-export/v1/artifact-manifest.schema.json
  run-export/v1/tombstone.schema.json
  run-export/v1/signature.schema.json
  run-export/v1/import-receipt.schema.json
  compatibility/run-export-baseline.json

migrations/
  0092_run_export_operations_snapshots_and_idempotency.sql
  0093_run_export_entries_signatures_and_tombstones.sql
  0094_run_export_imports_trust_and_remaps.sql
  0095_run_export_rls_indexes_and_worker_state.sql

tests/
  run_export_operation_persistence.rs
  run_export_snapshot_consistency.rs
  run_export_entry_streaming.rs
  run_export_signature_integrity.rs
  run_export_tamper_detection.rs
  run_export_unknown_schema.rs
  run_export_import_inertness.rs
  run_export_workspace_remap.rs
  run_export_restart.rs
  run_export_idempotency.rs
  run_export_rls.rs
  run_export_h11_parity.rs
  run_export_acceptance.rs

scripts/
  verify-run-export-format.sh
  verify-run-export-signature-scope.sh
  verify-run-export-inert-import.sh
  verify-run-export-no-secrets.sh
  verify-run-export-migration-ownership.sh
```

---

## Normative domain contracts

### Identifiers

Add strongly typed IDs:

```text
RunExportOperationId
RunExportSnapshotId
RunExportEntryId
RunExportTombstoneId
RunExportSignatureId
RunExportImportId
RunExportTrustDecisionId
RunExportWorkspaceRemapId
RunExportWorkerLeaseId
```

All IDs use the repository UUID strategy and reject nil values.

### Format identity

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunExportFormatVersion {
    pub name: RunExportFormatName,
    pub major: u16,
    pub minor: u16,
    pub manifest_schema_version: u16,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct RunExportFormatName(String);
```

Initial format:

```text
name = vestrace-run-export
major = 1
minor = 0
manifest_schema_version = 1
```

Rules:

- name is exactly `vestrace-run-export` for the initial format;
- major/minor/schema versions are positive where applicable;
- incompatible manifest or root-hash semantics require a new major;
- additive optional capability publication may increase minor only when old readers can safely reject or ignore the extension namespace;
- a format identifier is not authority or trust evidence.

### Export profile and exact selection

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportProfile {
    AuditEvidence,
    PortableOffline,
    SupportDiagnostic,
    EvaluationFixture,
}

pub enum RunJournalSelection {
    FullThroughBoundary,
    BoundedRange { first_sequence: u64, last_sequence: u64 },
}

pub enum RunPlanSelection {
    ReferencesOnly,
    Included,
}

pub enum RunCheckpointSelection {
    None,
    Selected { checkpoint_ids: Vec<RunCheckpointId> },
    Latest,
    AllAuthorized,
}

pub enum RunContextSelection {
    MetadataOnly,
    SelectedRedacted { snapshot_ids: Vec<ContextSnapshotId> },
    SelectedFull { snapshot_ids: Vec<ContextSnapshotId> },
}

pub enum RunArtifactSelection {
    ManifestOnly,
    Selected { revision_ids: Vec<ArtifactRevisionId> },
    AllAuthorized,
}

pub enum RunAuditProofSelection {
    None,
    SelectedRange { stream_id: SecurityAuditStreamId, from: u64, through: u64 },
    RunRelevantRange,
}

pub enum RunSubrunSelection {
    ReferencesOnly,
    Selected { run_ids: Vec<AgentRunId> },
    FullAuthorizedClosure,
}

pub struct RunExportSelection {
    pub profile: RunExportProfile,
    pub journal: RunJournalSelection,
    pub plans: RunPlanSelection,
    pub checkpoints: RunCheckpointSelection,
    pub context: RunContextSelection,
    pub artifacts: RunArtifactSelection,
    pub audit_proof: RunAuditProofSelection,
    pub subruns: RunSubrunSelection,
    pub destination_classification: DataClassification,
}
```

Selection invariants:

- ranges are positive and ordered;
- selected IDs are unique and bounded;
- `PortableOffline` includes every required event/export schema;
- `EvaluationFixture` never implies effectful replay permission;
- `SelectedFull` still requires exact H2/H6 authorization and secret-scan eligibility;
- parent export never silently expands to SubRuns;
- `FullAuthorizedClosure` means full only inside exact policy/snapshot boundaries;
- selection is canonicalized and hashed before authorization/idempotency binding.

### Snapshot boundary

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RunExportSnapshotBoundary {
    pub run_id: AgentRunId,
    pub run_version: RunVersion,
    pub last_included_run_sequence: u64,
    pub active_plan_revision_id: Option<ExecutionPlanRevisionId>,
    pub latest_included_checkpoint_id: Option<RunCheckpointId>,
    pub run_status: RunStatus,
    pub snapshot_is_terminal: bool,
    pub captured_at: Timestamp,
    pub source_snapshot_hash: [u8; 32],
}
```

Boundary rules:

- `last_included_run_sequence` equals the H1 sequence visible in the same consistent read;
- terminal flag must agree with H1 status and terminal event evidence;
- active Run export includes no source record created after the boundary;
- builder refuses a mixed snapshot when exact consistency cannot be proven;
- `captured_at` is a trusted Vestrace timestamp, not source content;
- boundary hash covers exact IDs, version, sequence, status and selected immutable references.

Every selected SubRun receives its own boundary and authorization result.

### Create command

```rust
pub struct CreateRunExport {
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub expected_run_version: Option<RunVersion>,
    pub selection: RunExportSelection,
    pub requested_format: RunExportFormatVersion,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}
```

`expected_run_version` belongs to the command and is not stored as mutable aggregate state. The persisted snapshot stores the actual exact Run version.

### Export lifecycle

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportStatus {
    Requested,
    Authorizing,
    Snapshotting,
    Building,
    Validating,
    Signing,
    Succeeded,
    Denied,
    Failed,
    Cancelled,
}

pub struct RunExportOperation {
    pub id: RunExportOperationId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub status: RunExportStatus,
    pub format: RunExportFormatVersion,
    pub selection_hash: [u8; 32],
    pub snapshot_id: Option<RunExportSnapshotId>,
    pub authorization_decision_id: Option<PolicyDecisionId>,
    pub bundle_artifact_revision_id: Option<ArtifactRevisionId>,
    pub manifest_hash: Option<[u8; 32]>,
    pub checksums_hash: Option<[u8; 32]>,
    pub root_hash: Option<[u8; 32]>,
    pub signature_id: Option<RunExportSignatureId>,
    pub failure_code: Option<RunExportFailureCode>,
    pub state_revision: u64,
    pub requested_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Lifecycle rules:

- state transitions require expected `state_revision` in commands/repositories;
- terminal states are immutable;
- `Succeeded` requires a finalized Available-or-policy-approved H6 bundle Artifact revision, validated manifest/checksums and valid local signature record;
- cancellation after signing cannot delete the signed bundle; it becomes a separate retention/purge action;
- failure details are bounded safe codes and references, never raw content or secret errors;
- operational worker lease/heartbeat state does not change logical state revision unless a lifecycle transition occurs.

### Manifest and entries

```rust
pub struct RunExportManifest {
    pub format: RunExportFormatVersion,
    pub export_id: RunExportOperationId,
    pub created_at: Timestamp,
    pub builder_revision: String,
    pub source: RunExportSourceIdentity,
    pub snapshot_boundary: RunExportSnapshotBoundary,
    pub selection: RunExportSelectionManifest,
    pub included_entries: Vec<RunExportEntryManifest>,
    pub omitted_entries: Vec<RunExportOmission>,
    pub tombstone_ids: Vec<RunExportTombstoneId>,
    pub event_schema_inventory: Vec<EventSchemaInventoryItem>,
    pub classification_summary: ClassificationSummary,
    pub redaction_summary: RedactionSummary,
    pub audit_proof_summary: AuditProofSummary,
    pub canonicalization_revision: String,
    pub checksum_algorithm: ChecksumAlgorithm,
    pub signature_algorithm: SignatureAlgorithm,
}

pub struct RunExportEntryManifest {
    pub entry_id: RunExportEntryId,
    pub canonical_path: RunExportCanonicalPath,
    pub media_type: String,
    pub byte_length: u64,
    pub content_hash: [u8; 32],
    pub source_reference: Option<RunExportSourceReference>,
    pub classification: DataClassification,
    pub representation: RunExportEntryRepresentation,
}
```

The serialized `manifest.json` does not contain `root_hash`; root hash is stored in `signature.json` and persisted operation metadata. This avoids a self-referential manifest hash while preserving the approved root-hash formula.

Entry rules:

- canonical paths use UTF-8 normalized relative paths with `/` separators;
- absolute paths, `..`, empty segments, backslashes, NUL and duplicate normalized paths are rejected;
- entry order in manifests/checksums is lexicographic by canonical UTF-8 bytes;
- media types are bounded and normalized;
- byte length and hash are computed while streaming;
- source references are provenance only;
- manifest claims and physical container entries must match exactly;
- undeclared entries are rejected except a versioned explicit extension namespace permitted by the format.

### Required logical paths

Format V1 requires:

```text
manifest.json
run.json
events.ndjson
artifact-manifest.json
schemas/export/manifest.schema.json
schemas/export/run.schema.json
schemas/export/event-line.schema.json
schemas/export/artifact-manifest.schema.json
schemas/export/tombstone.schema.json
schemas/export/signature.schema.json
checksums.sha256
signature.json
```

Conditionally required paths are enforced from manifest claims for plans, checkpoints, context, Artifacts, audit proof and tombstones.

### Tombstones and omissions

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportOmissionKind {
    OmittedByPolicy,
    Purged,
    Unavailable,
    Quarantined,
    SecretDetected,
    ClassificationExceeded,
    UnsupportedSchema,
    NotSelected,
}

pub struct RunExportTombstone {
    pub id: RunExportTombstoneId,
    pub source_reference: RunExportSourceReference,
    pub omission_kind: RunExportOmissionKind,
    pub historical_content_hash: Option<[u8; 32]>,
    pub safe_reason_code: String,
    pub policy_decision_id: Option<PolicyDecisionId>,
    pub created_at: Timestamp,
    pub tombstone_hash: [u8; 32],
}
```

Rules:

- tombstones contain no omitted content;
- historical hash is included only when retention/policy permits;
- omitted payload is never replaced with `{}`, `null` or empty bytes pretending to be original content;
- every manifest omission has either a tombstone or exact typed omission record;
- tombstones are included in checksums and signature scope.

### Checksums and root hash

`checksums.sha256` contains one normalized line for every semantic entry except `checksums.sha256` and `signature.json`:

```text
<lowercase-hex-sha256><two spaces><canonical-path>\n
```

The file is sorted by canonical path and ends with one LF.

Root hash V1 is:

```text
root_hash = SHA-256(
  domain_separator
  || canonical_format_identity
  || SHA-256(manifest.json bytes)
  || SHA-256(checksums.sha256 bytes)
)
```

Where:

- domain separator is exact ASCII `vestrace-run-export-root/v1\0`;
- canonical format identity is the deterministic encoding of name/major/minor/manifest schema version;
- manifest bytes are exact canonical JSON bytes included in the container;
- checksums bytes are exact normalized bytes included in the container;
- archive metadata and `signature.json` bytes are outside root-hash construction;
- `signature.json` signs the resulting root hash and identifies the manifest/checksum hashes.

### Signature record

```rust
pub struct RunExportSignature {
    pub id: RunExportSignatureId,
    pub export_operation_id: RunExportOperationId,
    pub signature_schema_version: u16,
    pub algorithm: SignatureAlgorithm,
    pub root_hash: [u8; 32],
    pub manifest_hash: [u8; 32],
    pub checksums_hash: [u8; 32],
    pub signing_key_revision: SigningKeyRevisionRef,
    pub public_key_artifact_revision_id: ArtifactRevisionId,
    pub signer_identity: RunExportSignerIdentity,
    pub signature: Vec<u8>,
    pub signed_at: Timestamp,
}
```

Initial algorithm is `Ed25519`.

Signature rules:

- H8 receives only root hash, algorithm, exact key revision and operation binding;
- private key bytes never cross the H8 adapter boundary;
- signing lease is one-use and bound to export operation/root hash;
- signature length is validated for the algorithm;
- public-key Artifact revision is exact and immutable;
- key revision/fingerprint is evidence, not a capability;
- signer trust is not encoded as a boolean in the signature record.

### Verification result

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportIntegrityDisposition {
    Valid,
    TamperedOrCorrupt,
    UnsupportedFormat,
    UnsupportedSchema,
    Incomplete,
}

pub struct RunExportVerificationResult {
    pub format: RunExportFormatVersion,
    pub integrity: RunExportIntegrityDisposition,
    pub root_hash: Option<[u8; 32]>,
    pub manifest_hash: Option<[u8; 32]>,
    pub checksums_hash: Option<[u8; 32]>,
    pub signature_valid: bool,
    pub signer_key_revision: Option<SigningKeyRevisionRef>,
    pub event_schema_support: EventSchemaSupportSummary,
    pub first_failure: Option<RunExportVerificationFailure>,
    pub verified_at: Timestamp,
}
```

Verification order is fixed:

```text
1. container/path/size limits
2. required/declared entry set
3. manifest schema and canonical encoding
4. checksums syntax/order
5. every entry checksum/length
6. root hash reconstruction
7. signature syntax and cryptographic verification
8. event/export schema inventory
9. audit proof verification
10. semantic cross-reference validation
```

No semantic parsing of user/event payload occurs before steps 1–7 succeed.

### Import lifecycle

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportImportMode {
    InspectOnly,
    SupportCase,
    EvaluationFixture,
    MigrationStaging,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportImportDisposition {
    Rejected,
    Quarantined,
    IntegrityVerifiedTrustUnknown,
    InspectOnly,
    SupportCaseReady,
    EvaluationFixtureReady,
    MigrationStagingReady,
}

pub struct RunExportImport {
    pub id: RunExportImportId,
    pub workspace_id: WorkspaceId,
    pub bundle_artifact_revision_id: ArtifactRevisionId,
    pub requested_mode: RunExportImportMode,
    pub disposition: RunExportImportDisposition,
    pub verification_hash: Option<[u8; 32]>,
    pub trust_decision_id: Option<RunExportTrustDecisionId>,
    pub workspace_remap_id: Option<RunExportWorkspaceRemapId>,
    pub source_identity: Option<RunExportSourceIdentity>,
    pub state_revision: u64,
    pub requested_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Import invariants:

- incoming bytes enter H6 quarantine before verification;
- integrity failure remains rejected/quarantined and never gets partially imported;
- valid signature without trust anchor may become `IntegrityVerifiedTrustUnknown` only for policy-approved limited inspection;
- `InspectOnly` creates read-only namespaced views and no local Run state;
- `SupportCase` creates support evidence references only;
- `EvaluationFixture` creates immutable H10 dataset/source candidates only after explicit H10 validation and never effectful replay permission;
- `MigrationStaging` prepares a remap proposal only; it does not activate local state;
- imported approvals, policies, budgets and credentials are historical evidence only;
- imported checkpoint never authorizes resume;
- source event schemas unsupported locally block semantic modes.

### Trust decision

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunExportSignerTrust {
    TrustedForInspection,
    TrustedForSupport,
    TrustedForEvaluation,
    TrustedForMigrationStaging,
    Untrusted,
    Revoked,
}

pub struct RunExportTrustDecision {
    pub id: RunExportTrustDecisionId,
    pub workspace_id: WorkspaceId,
    pub import_id: RunExportImportId,
    pub source_deployment_identity_hash: [u8; 32],
    pub signer_key_revision: SigningKeyRevisionRef,
    pub trust: RunExportSignerTrust,
    pub policy_decision_id: PolicyDecisionId,
    pub decided_by: PrincipalId,
    pub valid_until: Option<Timestamp>,
    pub decision_hash: [u8; 32],
    pub decided_at: Timestamp,
}
```

Trust is exact to source identity, signer key revision, requested use and workspace policy. It is never inferred from cryptographic validity alone.

### Workspace remap

```rust
pub struct RunExportWorkspaceRemap {
    pub id: RunExportWorkspaceRemapId,
    pub import_id: RunExportImportId,
    pub target_workspace_id: WorkspaceId,
    pub source_workspace_identity: NamespacedSourceId,
    pub source_run_identity: NamespacedSourceId,
    pub proposed_local_namespace: String,
    pub mapping_entries: Vec<RunExportRemapEntry>,
    pub policy_decision_id: PolicyDecisionId,
    pub approved_by: PrincipalId,
    pub remap_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub enum RunExportRemapEntry {
    PreserveForeignReference { source: NamespacedSourceId },
    BindExistingArtifact { source: NamespacedSourceId, target_revision_id: ArtifactRevisionId },
    StageEvaluationReference { source: NamespacedSourceId, target_fixture_key: String },
}
```

Rules:

- source UUIDs never overwrite or occupy local IDs;
- no entry maps a source Run to an active local `AgentRun`;
- no credential, approval grant, authorization ticket, lease or queue reference is remapped;
- Artifact binding requires target H6 ownership/hash/classification verification;
- every remap is explicit, bounded, hashed and H2-authorized;
- transitive or wildcard mapping is rejected.

---

## Application ports

```rust
#[async_trait::async_trait]
pub trait RunExportSnapshotPort: Send + Sync {
    async fn capture_boundary(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: Option<RunVersion>,
        selection: &RunExportSelection,
    ) -> Result<RunExportSnapshotMaterial, ApplicationError>;
}

#[async_trait::async_trait]
pub trait RunExportAuthorizationPort: Send + Sync {
    async fn authorize_export(
        &self,
        context: &RequestContext,
        command: &CreateRunExport,
        snapshot: &RunExportSnapshotMaterial,
    ) -> Result<RunExportAuthorization, ApplicationError>;

    async fn authorize_import(
        &self,
        context: &RequestContext,
        request: &CreateRunExportImport,
        verification: &RunExportVerificationResult,
    ) -> Result<RunExportImportAuthorization, ApplicationError>;
}

#[async_trait::async_trait]
pub trait RunExportArtifactPort: Send + Sync {
    async fn stage_bundle(
        &self,
        context: &RequestContext,
        export_id: RunExportOperationId,
    ) -> Result<RunExportBundleWriter, ApplicationError>;

    async fn finalize_bundle(
        &self,
        context: &RequestContext,
        writer: RunExportBundleWriter,
        provenance: RunExportArtifactProvenance,
    ) -> Result<ArtifactRevisionId, ApplicationError>;

    async fn open_quarantined_import(
        &self,
        context: &RequestContext,
        artifact_revision_id: ArtifactRevisionId,
    ) -> Result<RunExportBundleReader, ApplicationError>;
}

#[async_trait::async_trait]
pub trait RunExportSigningPort: Send + Sync {
    async fn sign_root_hash(
        &self,
        context: &RequestContext,
        request: RunExportSignRequest,
    ) -> Result<RunExportSignResult, ApplicationError>;

    async fn verify_signature(
        &self,
        context: &RequestContext,
        request: RunExportVerifySignatureRequest,
    ) -> Result<RunExportVerifySignatureResult, ApplicationError>;
}

#[async_trait::async_trait]
pub trait RunExportAuditProofPort: Send + Sync {
    async fn collect_proof(
        &self,
        context: &RequestContext,
        selection: &RunAuditProofSelection,
        boundary: &RunExportSnapshotBoundary,
    ) -> Result<RunExportAuditProofMaterial, ApplicationError>;

    async fn verify_proof(
        &self,
        context: &RequestContext,
        proof: RunExportAuditProofView,
    ) -> Result<AuditIntegrityDisposition, ApplicationError>;
}

#[async_trait::async_trait]
pub trait RunExportEventSchemaPort: Send + Sync {
    async fn required_inventory(
        &self,
        events: &[RunExportEventView],
        profile: RunExportProfile,
    ) -> Result<EventSchemaInventory, ApplicationError>;

    async fn validate_inventory(
        &self,
        inventory: &EventSchemaInventory,
    ) -> Result<EventSchemaSupportSummary, ApplicationError>;
}
```

Ports expose typed references and streams, not database rows, storage paths, private keys or provider SDK types.

---

## Persistence model

### Migration `0092_run_export_operations_snapshots_and_idempotency.sql`

Create:

```text
run_export_operations
run_export_snapshots
run_export_idempotency_bindings
run_export_selected_subrun_snapshots
```

Required constraints:

- operation workspace/run composite ownership;
- unique idempotency binding by workspace + principal + operation + key hash;
- same key with different canonical command hash conflicts;
- snapshot immutable after insertion;
- snapshot run version/sequence positive and consistent;
- one primary snapshot per export operation;
- selected SubRun snapshots unique by export/run;
- operation lifecycle revision positive;
- terminal operation rows cannot transition through application repositories;
- no raw content, secret or bundle bytes in these tables.

### Migration `0093_run_export_entries_signatures_and_tombstones.sql`

Create:

```text
run_export_entries
run_export_omissions
run_export_tombstones
run_export_signatures
run_export_audit_bindings
```

Required constraints:

- canonical path unique within export;
- content hash and byte length immutable;
- tombstones append-only;
- one active successful signature per exact root hash/export;
- signature references exact public-key Artifact revision and signing-key revision;
- audit binding contains references/ranges/hashes only;
- no private key, credential material, omitted content or archive bytes.

### Migration `0094_run_export_imports_trust_and_remaps.sql`

Create:

```text
run_export_imports
run_export_verification_results
run_export_trust_decisions
run_export_workspace_remaps
run_export_workspace_remap_entries
run_export_import_receipts
```

Required constraints:

- import bundle revision belongs to target workspace ingestion/quarantine scope;
- verification result immutable;
- trust decision exact to import, signer revision and source identity hash;
- remap exact to one import and target workspace;
- wildcard and credential/approval/lease mapping kinds absent from schema;
- one receipt per successful inert mode activation;
- imported bytes remain H6 Artifact references.

### Migration `0095_run_export_rls_indexes_and_worker_state.sql`

Create:

```text
run_export_work_items
run_export_worker_leases
run_export_reconciliation_records
```

Apply:

- forced RLS to every workspace-owned table;
- deployment-admin-only path for configured trust-anchor administration outside ordinary workspace APIs;
- indexes for lifecycle polling, run lookup, root hash, source identity and Artifact references;
- deterministic worker leasing with fencing tokens;
- bounded retries for pure local build/verification stages;
- `OutcomeUnknown` for ambiguous signing/finalization/external-delivery boundaries;
- no automatic retry of a consumed signing lease;
- cascade behavior that never deletes H6 Artifacts or source Run history through metadata deletion.

---

## Task 1: Add format, IDs, selection and lifecycle domain types

**Files:**

- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/run_export/mod.rs`
- Create: `crates/vestrace-domain/src/run_export/format.rs`
- Create: `crates/vestrace-domain/src/run_export/selection.rs`
- Create: `crates/vestrace-domain/src/run_export/snapshot.rs`
- Create: `crates/vestrace-domain/src/run_export/operation.rs`
- Create: `crates/vestrace-domain/src/run_export/error.rs`

- [ ] **Step 1: Write failing unit/property tests**

Cover:

- format identity validation;
- canonical selection hash independence from map/set insertion order;
- invalid ranges and duplicate selections;
- terminal lifecycle transitions;
- expected Run version remaining command-only;
- Run/SubRun boundary consistency.

Run:

```bash
cargo test -p vestrace-domain run_export
```

Expected: FAIL because the module/types do not exist.

- [ ] **Step 2: Implement validated IDs and enums**

Use existing UUID/timestamp/classification types. Do not add a generic Task/Action/Attempt hierarchy.

- [ ] **Step 3: Implement canonical selection hashing and lifecycle validation**

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p vestrace-domain run_export
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(run-export): add domain contracts"
```

---

## Task 2: Implement canonical paths, manifest schemas, checksums and root hash

**Files:**

- Create: `crates/vestrace-run-export-runtime/Cargo.toml`
- Create: `crates/vestrace-run-export-runtime/src/{lib,canonical_path,manifest,checksums,root_hash,validator,error}.rs`
- Create: `schemas/run-export/v1/*.schema.json`
- Create: `crates/vestrace-run-export-test-support/src/{lib,fixtures,corruption,conformance}.rs`

- [ ] **Step 1: Write failing conformance tests**

Test:

- path traversal/backslash/NUL rejection;
- duplicate normalized path rejection;
- deterministic manifest bytes;
- exact checksum line format and sorting;
- approved V1 root-hash vector;
- extra/missing entry failure;
- container metadata independence;
- manifest self-reference absence.

- [ ] **Step 2: Implement canonical JSON/path/checksum helpers**

Reuse repository canonical JSON primitives where compatible. Do not create a second incompatible canonicalizer.

- [ ] **Step 3: Implement generated schemas and golden vectors**

Schema generation must be deterministic. The baseline hashes are reviewed assets.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-run-export-runtime
cargo clippy -p vestrace-run-export-runtime --all-targets -- -D warnings
./scripts/verify-run-export-format.sh
git add Cargo.toml Cargo.lock crates/vestrace-run-export-runtime crates/vestrace-run-export-test-support schemas/run-export
 git commit -m "feat(run-export): add canonical bundle format"
```

---

## Task 3: Persist export operations, snapshots and idempotency

**Files:**

- Create: `migrations/0092_run_export_operations_snapshots_and_idempotency.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/run_export/{mod,operation_repository,snapshot_repository}.rs`
- Create: `crates/vestrace-application/src/run_export/{mod,ports,commands,service}.rs`
- Test: `tests/run_export_operation_persistence.rs`
- Test: `tests/run_export_idempotency.rs`

- [ ] **Step 1: Write failing PostgreSQL tests**

Cover:

- create/idempotent replay/conflict;
- optimistic lifecycle transition;
- terminal immutability;
- workspace/run composite ownership;
- snapshot append-only behavior;
- no bundle bytes in metadata tables.

- [ ] **Step 2: Add migration and repositories**

- [ ] **Step 3: Implement command service with H2 authorization placeholder port**

The service records `Requested`; it does not build bytes synchronously in the HTTP transaction.

- [ ] **Step 4: Run and commit**

```bash
cargo test --test run_export_operation_persistence
cargo test --test run_export_idempotency
sqlx migrate run --source migrations
./scripts/verify-run-export-migration-ownership.sh
git add migrations/0092* crates/vestrace-application/src/run_export crates/vestrace-infrastructure/src/postgres/run_export tests/run_export_operation_persistence.rs tests/run_export_idempotency.rs
git commit -m "feat(run-export): persist export operations"
```

---

## Task 4: Capture exact H1/H5 snapshot material

**Files:**

- Create: `crates/vestrace-application/src/run_export/snapshot_service.rs`
- Create: `crates/vestrace-application/src/run_export/owners/{mod,h1,h5}.rs`
- Test: `crates/vestrace-application/tests/run_export_snapshot.rs`
- Test: `tests/run_export_snapshot_consistency.rs`

- [ ] **Step 1: Write failing snapshot tests**

Scenarios:

- terminal Run through terminal sequence;
- active Run consistent boundary;
- expected version mismatch;
- concurrent Run mutation during snapshot;
- selected checkpoint after boundary rejected;
- parent does not include SubRun by default;
- each selected SubRun gets independent boundary;
- plan revision references immutable and boundary-valid.

- [ ] **Step 2: Implement H1 consistent-read port**

Use H1-owned transaction/query boundary; do not read arbitrary tables directly from the export service.

- [ ] **Step 3: Implement H5 plan/SubRun reference collection**

- [ ] **Step 4: Persist immutable snapshot metadata and commit**

```bash
cargo test -p vestrace-application run_export_snapshot
cargo test --test run_export_snapshot_consistency
git add crates/vestrace-application/src/run_export tests/run_export_snapshot_consistency.rs
git commit -m "feat(run-export): capture exact Run snapshots"
```

---

## Task 5: Resolve H2/H6/H10 selection, omissions and tombstones

**Files:**

- Create: `crates/vestrace-application/src/run_export/selection_service.rs`
- Create: `crates/vestrace-application/src/run_export/owners/{h2,h6,h10}.rs`
- Create: `crates/vestrace-domain/src/run_export/{entry,tombstone}.rs`
- Create: `migrations/0093_run_export_entries_signatures_and_tombstones.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/run_export/{entry_repository,tombstone_repository}.rs`
- Test: `crates/vestrace-application/tests/run_export_selection.rs`

- [ ] **Step 1: Write failing selection tests**

Cover policy exclusion, classification downgrade, secret finding, purge, quarantine, unsupported schema, selected SubRuns and audit ranges.

- [ ] **Step 2: Implement exact per-item authorization and H6 eligibility checks**

Reading, including, exporting and externally delivering remain separate decisions.

- [ ] **Step 3: Implement signed omission/tombstone records**

No omitted content may enter safe reason strings.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application run_export_selection
cargo test --test run_export_operation_persistence
./scripts/verify-run-export-no-secrets.sh
git add migrations/0093* crates/vestrace-domain/src/run_export crates/vestrace-application/src/run_export crates/vestrace-infrastructure/src/postgres/run_export
git commit -m "feat(run-export): select governed export content"
```

---

## Task 6: Build the bundle as a streaming H6 Artifact

**Files:**

- Create: `crates/vestrace-application/src/run_export/builder.rs`
- Create: `crates/vestrace-run-export-runtime/src/{container,streaming,reader}.rs`
- Create: `crates/vestrace-run-export-test-support/src/{artifact_store,containers}.rs`
- Test: `crates/vestrace-application/tests/run_export_manifest.rs`
- Test: `tests/run_export_entry_streaming.rs`

- [ ] **Step 1: Write failing streaming tests**

Cover:

- large Artifact without full buffering;
- deterministic logical entries across ZIP/tar-compatible test containers;
- declared/physical entry equality;
- byte/entry/decompression limits;
- failure cleanup of staged H6 bytes;
- no host/storage path leakage;
- exact original event schema/version/hash.

- [ ] **Step 2: Implement streaming writer and logical entry builder**

Builder writes only policy-approved data and computes hashes while streaming.

- [ ] **Step 3: Finalize bundle through H6 quarantine/inspection/provenance**

`Succeeded` cannot be set until the bundle Artifact reaches the exact approved lifecycle state.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-run-export-runtime
cargo test -p vestrace-application run_export_manifest
cargo test --test run_export_entry_streaming
git add crates/vestrace-run-export-runtime crates/vestrace-run-export-test-support crates/vestrace-application/src/run_export tests/run_export_entry_streaming.rs
git commit -m "feat(run-export): stream bundle artifacts"
```

---

## Task 7: Sign root hashes through H8 and validate local bundles

**Files:**

- Create: `crates/vestrace-application/src/run_export/{signer,validator}.rs`
- Create: `crates/vestrace-application/src/run_export/owners/h8.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/run_export/signature_repository.rs`
- Test: `crates/vestrace-application/tests/run_export_signing.rs`
- Test: `tests/run_export_signature_integrity.rs`
- Test: `tests/run_export_tamper_detection.rs`

- [ ] **Step 1: Write failing signing/tamper tests**

Cover:

- exact Ed25519 vector;
- one-use operation-bound signing lease;
- ambiguous signing result becomes `OutcomeUnknown` and is not blindly retried;
- modified manifest/checksum/entry/signature failure;
- valid signature with unknown signer trust remains integrity-valid only;
- public-key Artifact mismatch;
- archive metadata change does not affect semantic verification.

- [ ] **Step 2: Implement H8 signing adapter boundary**

No raw key material in application/domain types.

- [ ] **Step 3: Persist signature metadata and complete local operation atomically**

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application run_export_signing
cargo test --test run_export_signature_integrity
cargo test --test run_export_tamper_detection
./scripts/verify-run-export-signature-scope.sh
git add crates/vestrace-application/src/run_export crates/vestrace-infrastructure/src/postgres/run_export tests/run_export_signature_integrity.rs tests/run_export_tamper_detection.rs
git commit -m "feat(run-export): sign and validate bundles"
```

---

## Task 8: Implement fail-closed import verification and quarantine

**Files:**

- Create: `crates/vestrace-application/src/run_export/verifier.rs`
- Create: `crates/vestrace-application/src/run_export/import_service.rs`
- Create: `crates/vestrace-domain/src/run_export/{verification,import}.rs`
- Create: `migrations/0094_run_export_imports_trust_and_remaps.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/run_export/{import_repository,trust_repository,remap_repository}.rs`
- Test: `crates/vestrace-application/tests/run_export_verification.rs`
- Test: `tests/run_export_unknown_schema.rs`

- [ ] **Step 1: Write failing verification-order tests**

Prove payload semantic parsing does not occur before structural/checksum/root/signature verification.

- [ ] **Step 2: Implement bounded container reader and verification pipeline**

Reject zip bombs, duplicate paths, traversal, undeclared entries, oversized manifests, unsupported algorithms and invalid canonical bytes.

- [ ] **Step 3: Integrate event-schema inventory validation and H10 audit proof verification**

Unknown event schema remains opaque and blocks semantic modes.

- [ ] **Step 4: Persist immutable verification result and commit**

```bash
cargo test -p vestrace-application run_export_verification
cargo test --test run_export_unknown_schema
cargo test --test run_export_tamper_detection
git add migrations/0094* crates/vestrace-domain/src/run_export crates/vestrace-application/src/run_export crates/vestrace-infrastructure/src/postgres/run_export tests/run_export_unknown_schema.rs
git commit -m "feat(run-export): verify imported bundles"
```

---

## Task 9: Implement inert modes, trust decisions and explicit remap proposals

**Files:**

- Create: `crates/vestrace-application/src/run_export/{trust_service,remap_service}.rs`
- Test: `crates/vestrace-application/tests/{run_export_import_modes,run_export_trust,run_export_remap}.rs`
- Test: `tests/run_export_import_inertness.rs`
- Test: `tests/run_export_workspace_remap.rs`

- [ ] **Step 1: Write failing inertness tests**

Assert zero creation/use of:

```text
AgentRun
RunStep
Run work item
credential lease
approval grant
budget reservation
Tool invocation
remote invocation
notification intent
active memory
trigger occurrence
checkpoint resume
```

for every import mode.

- [ ] **Step 2: Implement exact signer trust policy**

Trust is scoped by source identity, key revision, target workspace and intended mode.

- [ ] **Step 3: Implement read-only/support/evaluation/migration-staging receipts**

Evaluation creates only an H10 candidate/reference pending its own validation. Migration staging creates only a remap proposal.

- [ ] **Step 4: Implement explicit workspace remaps and commit**

```bash
cargo test -p vestrace-application run_export_import_modes
cargo test -p vestrace-application run_export_trust
cargo test -p vestrace-application run_export_remap
cargo test --test run_export_import_inertness
cargo test --test run_export_workspace_remap
./scripts/verify-run-export-inert-import.sh
git add crates/vestrace-application/src/run_export tests/run_export_import_inertness.rs tests/run_export_workspace_remap.rs
git commit -m "feat(run-export): add inert import modes"
```

---

## Task 10: Add restart-safe workers, RLS and reconciliation

**Files:**

- Create: `migrations/0095_run_export_rls_indexes_and_worker_state.sql`
- Create: `crates/vestrace-application/src/run_export/worker.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/run_export/{worker_repository,reconciliation_repository}.rs`
- Test: `tests/run_export_restart.rs`
- Test: `tests/run_export_rls.rs`

- [ ] **Step 1: Write failing crash-point tests**

Crash after:

- snapshot persisted;
- staged entries written;
- checksums generated;
- signing lease consumed;
- signature returned but not persisted;
- H6 finalize requested;
- import verification persisted;
- trust decision persisted.

- [ ] **Step 2: Implement fenced workers and safe reconciliation**

Pure deterministic stages may retry. Ambiguous cryptographic/finalization/delivery stages require status/readback/reconciliation.

- [ ] **Step 3: Implement forced RLS and cross-workspace denial tests**

- [ ] **Step 4: Run and commit**

```bash
cargo test --test run_export_restart
cargo test --test run_export_rls
sqlx migrate run --source migrations
git add migrations/0095* crates/vestrace-application/src/run_export/worker.rs crates/vestrace-infrastructure/src/postgres/run_export tests/run_export_restart.rs tests/run_export_rls.rs
git commit -m "feat(run-export): add restart-safe processing"
```

---

## Task 11: Add H11 HTTP, CLI, MCP and SDK surfaces

**Files:**

- Create: `crates/vestrace-channel-http/src/routes/run_exports.rs`
- Create: `crates/vestrace-channel-http/src/dto/run_export.rs`
- Create: `crates/vestrace-channel-cli/src/commands/run_export.rs`
- Create: `crates/vestrace-channel-mcp/src/{tools,resources}/run_export.rs`
- Create: `crates/vestrace-public-schema/src/run_export.rs`
- Create: `crates/vestrace-sdk-rust/src/run_exports.rs`
- Create: `packages/sdk-typescript/src/run_exports.ts`
- Create: `packages/sdk-typescript/src/generated/run-export.ts`
- Test: `tests/run_export_h11_parity.rs`

- [ ] **Step 1: Write failing transport parity tests**

HTTP/CLI/MCP call the same services and produce equivalent operation/import receipts.

- [ ] **Step 2: Add public commands/queries**

Initial operations:

```text
create export
get export status
inspect manifest summary
download through existing H6 authorization
create import from quarantined Artifact
get verification/trust status
activate one inert import mode
create explicit remap proposal
```

No endpoint directly sets lifecycle state, signer trust or local Run state.

- [ ] **Step 3: Add opaque unknown event/schema SDK representations**

- [ ] **Step 4: Run and commit**

```bash
cargo test --test run_export_h11_parity
cargo test -p vestrace-sdk-rust
npm test --prefix packages/sdk-typescript
git add crates/vestrace-channel-http crates/vestrace-channel-cli crates/vestrace-channel-mcp crates/vestrace-public-schema crates/vestrace-sdk-rust packages/sdk-typescript tests/run_export_h11_parity.rs
git commit -m "feat(run-export): expose product surfaces"
```

---

## Task 12: Add complete acceptance, security and release-baseline gates

**Files:**

- Create: `tests/run_export_acceptance.rs`
- Create: `scripts/verify-run-export-format.sh`
- Create: `scripts/verify-run-export-signature-scope.sh`
- Create: `scripts/verify-run-export-inert-import.sh`
- Create: `scripts/verify-run-export-no-secrets.sh`
- Create: `scripts/verify-run-export-migration-ownership.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `schemas/compatibility/run-export-baseline.json`
- Modify: release manifest asset list under H11-owned files

- [ ] **Step 1: Implement approved-design acceptance scenarios**

At minimum:

1. terminal Run full authorized export;
2. active Run consistent snapshot;
3. concurrent mutation mismatch;
4. SubRun references-only default;
5. selected SubRun separate boundary;
6. purged Artifact signed tombstone;
7. policy-withheld event payload explicit omission;
8. exact event schema inventory;
9. original event identity preserved;
10. H10 audit proof verified separately;
11. Ed25519 signature valid;
12. manifest tampering rejected;
13. entry tampering rejected;
14. undeclared/missing entry rejected;
15. path traversal/zip bomb rejected;
16. valid signature with unknown trust limited to inspection;
17. revoked signer denied for configured modes;
18. unknown event schema blocks semantic import;
19. `InspectOnly` creates no authority/work;
20. `EvaluationFixture` creates candidate only;
21. `MigrationStaging` creates remap proposal only;
22. checkpoint never resumes;
23. imported approvals/policies remain evidence only;
24. workspace remap preserves foreign IDs;
25. H6 purge propagates to bundle/import views;
26. RLS prevents cross-workspace access;
27. restart does not duplicate bundle/signature/import receipt;
28. no secret/private key/hidden reasoning leakage;
29. HTTP/CLI/MCP parity;
30. release baseline detects same-version schema drift.

- [ ] **Step 2: Run the full verification matrix**

```bash
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/verify-run-export-format.sh
./scripts/verify-run-export-signature-scope.sh
./scripts/verify-run-export-inert-import.sh
./scripts/verify-run-export-no-secrets.sh
./scripts/verify-run-export-migration-ownership.sh
./scripts/verify-event-schema-baseline.sh
```

- [ ] **Step 3: Perform manual threat-model review**

Review:

- archive traversal/decompression bombs;
- manifest/checksum/signature circularity;
- signer trust confusion;
- source/local ID collision;
- authority transfer through imported records;
- secret/path leakage;
- partial-verification parsing;
- duplicate delivery/signing after crash;
- stale policy/classification at export/download/import time;
- purge/retention propagation;
- SDK handling of opaque future schemas.

- [ ] **Step 4: Commit final verification assets**

```bash
git add .github/workflows/ci.yml schemas/compatibility tests/run_export_acceptance.rs scripts/verify-run-export-*.sh
git commit -m "test(run-export): add acceptance and security gates"
```

---

## Rollout order

Implementation rollout is readers/verifiers first:

```text
1. domain/runtime format and golden vectors
2. schema publication and verification-only reader
3. PostgreSQL metadata migrations
4. import quarantine/verifier in disabled mode
5. H1/H2/H5/H6/H8/H10 owner adapters
6. local export builder behind disabled feature/policy
7. signing and local acceptance
8. inert InspectOnly import
9. SupportCase/EvaluationFixture/MigrationStaging gates
10. H11 public surfaces
11. release baseline and production activation
```

No producer is enabled until the deployed verifier understands the exact format/event schema inventory.

---

## Definition of done

The implementation is complete only when:

1. every bundle binds one exact H1 Run snapshot boundary;
2. active and terminal Run snapshots are consistent and tested;
3. parent export does not silently include SubRuns;
4. all included/omitted entries are exact, deterministic and signed;
5. original event identity/schema/hash remain preserved;
6. H6 owns every bundle/content byte and purge lifecycle;
7. H8 private keys never cross the cryptographic adapter boundary;
8. Ed25519 root-hash verification passes approved vectors;
9. audit proof and export signature verify independently;
10. tampered/incomplete/unsupported bundles fail closed;
11. signer trust remains separate from cryptographic validity;
12. all four import modes are demonstrably inert;
13. source IDs never become local authority IDs automatically;
14. imported approvals, policies, budgets, checkpoints and credentials remain evidence only;
15. unknown event schemas block semantic modes;
16. retries/restart do not duplicate bundles, signatures or receipts;
17. forced RLS and H2 authorization protect every workspace operation;
18. HTTP/CLI/MCP/SDK surfaces share one application contract;
19. migrations are limited to `0092`–`0095`;
20. no implementation begins merely because this plan is merged.

Merging this plan approves only the implementation sequence and contracts. It does not authorize production code, migrations, signing-key use, bundle creation, import or public activation.