# Vestrace H6 Context and Artifact Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start storage services, run processors or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement a memory-first, token-aware and policy-governed Context Runtime plus an immutable Artifact Runtime with streaming ingestion, quarantine, representations, provenance, secure mounts and exports, external-artifact handling, retention, hard purge and Run consolidation into governed memory candidates.

**Architecture:** H6 extends the v0.1 Memory Plane rather than duplicating memories, retrieval or write policy. Existing v0.1 retrieval/context-pack services remain authoritative for long-term memory selection; H6 combines their exact revision references with identity, Run state, plan state, verified evidence, artifact excerpts, tool views and output contracts into a model-specific `ModelContextEnvelope` and immutable `ContextSnapshot`. Artifact metadata and lifecycle are authoritative in PostgreSQL while bytes live behind `ArtifactBlobStorePort`; all bytes enter through staged streaming, quarantine and inspection before they can be read, mounted, exported or included in context. H4 sandbox outputs and H5 remote outputs remain source candidates until H6 materializes validated Artifact revisions.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H5 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17 with pgvector and FTS; SHA-256 streaming hashes; bytes/futures streams; JSON Schema validation; local content-addressed storage; S3-compatible storage adapter; Reqwest for a restricted external-artifact fetcher; H4 sandbox processing; tracing; proptest; deterministic loopback fixtures.

## Global Constraints

- Complete all five v0.1 plans and H1–H5 before implementing H6.
- Harness design sections `16. Context system and memory` and `21. Artifact system` are normative.
- v0.1 `Memory`, `MemoryRevision`, retrieval, context-pack, provenance and memory write-policy records remain authoritative and are never replaced by H6 tables.
- H6 reads and writes memory only through Vestrace application ports. It never updates v0.1 memory or retrieval tables directly.
- PostgreSQL is authoritative for Artifact identities/revisions, ingestion sessions, inspection results, representations, provenance edges, context snapshots, scratchpad revisions, evidence records, mounts, exports, retention, purge and consolidation records.
- Artifact bytes, representation bytes and optional context-capture bytes live only behind `ArtifactBlobStorePort`; database rows contain bounded metadata and opaque blob keys.
- Vestrace owns every domain type, application port, persisted schema, lifecycle transition, hash format and public DTO.
- S3 SDK, Reqwest, filesystem and parser-specific types may not appear in domain/application signatures or PostgreSQL schemas.
- One logical `Artifact` may have multiple immutable `ArtifactRevision` records. A revision never changes bytes, media type, content hash or provenance after finalization.
- A content hash identifies bytes, not permission. Knowing a hash never grants read, mount, export or cross-workspace access.
- Cross-workspace physical deduplication is disabled by default. Blob identity includes a storage namespace to avoid a cross-tenant existence oracle.
- Artifact-store paths, bucket keys, presigned URLs, temporary filenames and host paths are never model-visible.
- Every ingestion uses streaming byte limits and SHA-256. Large inputs are not buffered entirely in memory.
- A staged blob is not an Artifact revision. It becomes linked only after durable metadata finalization.
- All new bytes enter `Quarantined`. They cannot be used in model context, sandbox mounts, export or memory consolidation until required inspections pass.
- MIME declarations, filenames, extensions, archive manifests, parser output and remote metadata are untrusted.
- Secret findings store rule identifiers, locations and irreversible fingerprints only; secret values are never persisted in findings or logs.
- Executable/active content, archive expansion, parser selection and high-risk media fail closed according to policy.
- Derived representations and chunks are rebuildable, but their generator revision and source revision are immutable provenance.
- Model context assembly never stores or requests hidden chain-of-thought. Reproducibility uses typed Run state, explicit scratchpad fields, references, hashes and durable outcomes.
- Safety policy, identity instructions, confirmed constraints, selected plan step and output contract are mandatory context sections and are never silently truncated.
- Context allocation is deterministic for the same source revisions, model limits, tokenizer revision, policy revision and budget.
- Every model invocation uses a persisted `ContextSnapshot` reference. A model request cannot substitute unrecorded prompt content after snapshot creation.
- Reading a source, including it in a context, transferring it to a selected provider and capturing it durably are distinct policy decisions.
- Context snapshots store exact source revisions, rendered hash, token accounting, policy revision, tokenizer revision, omitted reasons and redaction records.
- Full or redacted rendered captures are stored as purgeable blob references; immutable snapshot metadata does not require retaining source content forever.
- Scratchpad entries are typed summaries such as assumptions, questions, calculations, risks and candidate actions. Free-form reasoning traces and token-by-token deliberation are rejected.
- External evidence remains distinguishable from authoritative memory and verified Run state through trust annotations.
- `Unknown`, rejected, unverified or policy-denied execution outcomes never become memory candidates.
- Run consolidation creates governed candidate memories through existing v0.1 write policy. H6 never auto-activates memory by bypassing that policy.
- H4 `SandboxOutputCandidate`, Tool output candidates and H5 remote output candidates are source records, not available Artifacts.
- External URL ingestion supports only policy-approved `http`/`https`, no URL credentials, no ambient cookies and no automatic authenticated fetch before H8.
- External URL fetching rejects loopback, link-local, private, multicast, metadata-service and reserved destinations and is protected against DNS rebinding.
- Artifact input mounts are read-only and exact-revision-bound. A mount grant is not a storage credential.
- Export uses an exact revision, operation-bound approval/policy decision and one-time/short-lived grant. Logical read permission does not imply export permission.
- Logical delete, retention expiry, legal hold, purge and physical garbage collection are distinct states.
- Hard purge removes blobs, representations, chunks, embeddings, cached excerpts, context-capture bytes and temporary export copies while retaining a content-free tombstone and audit reference.
- Replay, context rebuild diagnostics and evaluation never re-fetch external URLs, rerun artifact processors, remount sandboxes or write memory unless an explicit protected work item is executed.
- Existing migrations `0014`–`0041` are never edited. H6 migrations are `0042`–`0047` and are created once.
- CI uses generated local files, loopback HTTP fixtures and deterministic processors. It requires no public model, remote storage account, public URL, A2A server or permanent credential.
- Future implementation branch: `feat/h6-context-artifact-runtime`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  artifact/mod.rs
  artifact/artifact.rs
  artifact/revision.rs
  artifact/blob.rs
  artifact/ingestion.rs
  artifact/inspection.rs
  artifact/representation.rs
  artifact/provenance.rs
  artifact/mount.rs
  artifact/export.rs
  artifact/retention.rs
  context/mod.rs
  context/source.rs
  context/policy.rs
  context/envelope.rs
  context/snapshot.rs
  context/token.rs
  context/scratchpad.rs
  evidence/mod.rs
  evidence/item.rs
  evidence/claim.rs
  evidence/contradiction.rs
  consolidation/mod.rs
  consolidation/candidate.rs
  consolidation/report.rs
  run/event.rs
  run/work.rs
  run/checkpoint.rs
  run/mod.rs

crates/vestrace-application/src/
  lib.rs
  artifact/mod.rs
  artifact/ports.rs
  artifact/commands.rs
  artifact/ingestion.rs
  artifact/inspection.rs
  artifact/representation.rs
  artifact/excerpts.rs
  artifact/mounts.rs
  artifact/exports.rs
  artifact/external.rs
  artifact/retention.rs
  artifact/purge.rs
  artifact/worker.rs
  context/mod.rs
  context/ports.rs
  context/planner.rs
  context/allocator.rs
  context/renderer.rs
  context/snapshot_service.rs
  context/model_integration.rs
  context/scratchpad_service.rs
  evidence/mod.rs
  evidence/service.rs
  evidence/contradictions.rs
  consolidation/mod.rs
  consolidation/ports.rs
  consolidation/service.rs
  consolidation/worker.rs

crates/vestrace-application/tests/
  artifact_ingestion.rs
  artifact_inspection.rs
  artifact_representation.rs
  artifact_mounts.rs
  artifact_exports.rs
  artifact_retention.rs
  context_allocator.rs
  context_snapshot.rs
  context_model_integration.rs
  scratchpad_service.rs
  evidence_service.rs
  consolidation_service.rs

crates/vestrace-artifact-store-local/
  Cargo.toml
  src/lib.rs
  src/store.rs
  src/layout.rs
  src/staging.rs
  src/range.rs
  src/security.rs
  src/reconcile.rs

crates/vestrace-artifact-store-s3/
  Cargo.toml
  src/lib.rs
  src/store.rs
  src/config.rs
  src/multipart.rs
  src/range.rs
  src/error.rs
  src/reconcile.rs

crates/vestrace-artifact-inspector/
  Cargo.toml
  src/lib.rs
  src/media.rs
  src/archive.rs
  src/secrets.rs
  src/malware.rs
  src/classification.rs

crates/vestrace-artifact-processors/
  Cargo.toml
  src/lib.rs
  src/text.rs
  src/json.rs
  src/archive_manifest.rs
  src/sandboxed.rs
  src/chunking.rs

crates/vestrace-artifact-fetch-http/
  Cargo.toml
  src/lib.rs
  src/fetcher.rs
  src/config.rs
  src/dns.rs
  src/redirect.rs
  src/auth.rs
  src/error.rs

crates/vestrace-artifact-test-support/
  Cargo.toml
  src/lib.rs
  src/fakes.rs
  src/streams.rs
  src/store_conformance.rs
  src/http_server.rs
  src/processors.rs
  src/fixtures.rs

crates/vestrace-infrastructure/src/postgres/
  mod.rs
  artifact/mod.rs
  artifact/repository.rs
  artifact/blob_repository.rs
  artifact/ingestion_repository.rs
  artifact/inspection_repository.rs
  artifact/representation_repository.rs
  artifact/provenance_repository.rs
  artifact/mount_repository.rs
  artifact/export_repository.rs
  artifact/retention_repository.rs
  context/mod.rs
  context/snapshot_repository.rs
  context/scratchpad_repository.rs
  evidence/mod.rs
  evidence/repository.rs
  evidence/contradiction_repository.rs
  consolidation/mod.rs
  consolidation/repository.rs

migrations/
  0042_artifacts_revisions_blobs_and_provenance.sql
  0043_artifact_ingestion_inspections_representations_chunks.sql
  0044_context_snapshots_scratchpads_and_evidence.sql
  0045_artifact_mounts_exports_retention_and_purge.sql
  0046_run_consolidation_and_memory_candidate_bindings.sql
  0047_context_artifact_rls_indexes_and_run_bindings.sql

tests/
  artifact_store_local_conformance.rs
  artifact_store_s3_conformance.rs
  artifact_persistence.rs
  artifact_ingestion_restart.rs
  artifact_inspection_persistence.rs
  artifact_representation_persistence.rs
  artifact_excerpt_search.rs
  context_snapshot_persistence.rs
  context_budget_determinism.rs
  evidence_persistence.rs
  scratchpad_persistence.rs
  sandbox_artifact_mount.rs
  sandbox_output_ingestion.rs
  remote_artifact_ingestion.rs
  external_url_security.rs
  artifact_export_persistence.rs
  artifact_purge_restart.rs
  consolidation_persistence.rs
  context_artifact_rls.rs
  h6_acceptance.rs

scripts/
  verify-context-boundary.sh
  verify-artifact-boundary.sh
  verify-artifact-purge.sh
```

---

## Normative contracts

### Artifact identity, immutable revisions and blob keys

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Input,
    Working,
    Deliverable,
    Evidence,
    Representation,
    ContextCapture,
    ExportCopy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactLifecycleStatus {
    Staging,
    Quarantined,
    Inspecting,
    Available,
    Rejected,
    Deleted,
    PurgePending,
    Purged,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ArtifactContentHash([u8; 32]);

pub struct ArtifactBlobKey {
    pub storage_namespace_id: ArtifactStorageNamespaceId,
    pub content_hash: ArtifactContentHash,
}

pub struct Artifact {
    pub id: ArtifactId,
    pub workspace_id: WorkspaceId,
    pub owner_principal_id: PrincipalId,
    pub kind: ArtifactKind,
    pub current_revision_id: Option<ArtifactRevisionId>,
    pub lifecycle: ArtifactLifecycleStatus,
    pub created_at: Timestamp,
}

pub struct ArtifactRevision {
    pub id: ArtifactRevisionId,
    pub artifact_id: ArtifactId,
    pub revision: u32,
    pub producing_run_id: Option<AgentRunId>,
    pub producing_step_id: Option<RunStepId>,
    pub media_type: String,
    pub original_filename: Option<String>,
    pub byte_size: u64,
    pub content_hash: ArtifactContentHash,
    pub blob_key: ArtifactBlobKey,
    pub classification: DataClassification,
    pub status: ArtifactLifecycleStatus,
    pub source: ArtifactRevisionSource,
    pub provenance_root_id: ArtifactProvenanceNodeId,
    pub created_at: Timestamp,
}
```

Rules:

- revision starts at 1 and is contiguous per Artifact;
- `byte_size` is positive except for media types explicitly allowing an empty payload;
- media type is normalized lowercase type/subtype without parameters;
- filename is metadata only, stripped of path components and bounded to 255 UTF-8 bytes;
- an Available revision cannot change blob, hash, size, classification or provenance;
- Artifact lifecycle may advance to Deleted/PurgePending/Purged without mutating historical revision bytes;
- `ArtifactBlobKey` is never serialized into model or remote-agent DTOs.

### Blob-store streaming contract

```rust
pub type ArtifactByteStream = std::pin::Pin<Box<
    dyn futures::Stream<Item = Result<bytes::Bytes, ArtifactStoreError>> + Send
>>;

pub struct StageArtifactBlobRequest {
    pub session_id: ArtifactIngestionSessionId,
    pub storage_namespace_id: ArtifactStorageNamespaceId,
    pub expected_size: Option<u64>,
    pub expected_hash: Option<ArtifactContentHash>,
    pub maximum_bytes: u64,
    pub body: ArtifactByteStream,
}

pub struct StagedArtifactBlob {
    pub session_id: ArtifactIngestionSessionId,
    pub staging_handle: OpaqueArtifactStagingHandle,
    pub content_hash: ArtifactContentHash,
    pub byte_size: u64,
}

pub struct ArtifactBlobObservation {
    pub blob_key: ArtifactBlobKey,
    pub byte_size: u64,
    pub present: bool,
}

#[async_trait::async_trait]
pub trait ArtifactBlobStorePort: Send + Sync {
    async fn stage(
        &self,
        request: StageArtifactBlobRequest,
    ) -> Result<StagedArtifactBlob, ArtifactStoreError>;

    async fn promote(
        &self,
        staged: &StagedArtifactBlob,
    ) -> Result<ArtifactBlobObservation, ArtifactStoreError>;

    async fn inspect(
        &self,
        key: &ArtifactBlobKey,
    ) -> Result<ArtifactBlobObservation, ArtifactStoreError>;

    async fn open_range(
        &self,
        key: &ArtifactBlobKey,
        range: ArtifactByteRange,
    ) -> Result<ArtifactByteStream, ArtifactStoreError>;

    async fn delete(
        &self,
        key: &ArtifactBlobKey,
    ) -> Result<ArtifactBlobDeletionObservation, ArtifactStoreError>;

    async fn cleanup_staging(
        &self,
        handle: &OpaqueArtifactStagingHandle,
    ) -> Result<(), ArtifactStoreError>;
}
```

`stage` calculates the hash while streaming and rejects size/hash mismatch. `promote` is idempotent for the same namespace/hash. An ambiguous store response is reconciled with `inspect`; it is not converted to a second upload automatically.

### Ingestion source and durable state

```rust
pub enum ArtifactIngestionSource {
    UserUpload { upload_reference: String },
    ToolOutput { tool_invocation_id: ToolInvocationId, candidate_id: String },
    SandboxOutput { sandbox_session_id: SandboxSessionId, candidate_id: SandboxOutputCandidateId },
    InternalHandoff { handoff_id: HandoffArtifactId, candidate_reference: RunReference },
    RemoteHandoff { invocation_id: RemoteAgentInvocationId, candidate_reference: RunReference },
    ExternalUrl { url_hash: [u8; 32] },
    DerivedRepresentation { representation_id: ArtifactRepresentationId },
    ContextCapture { context_snapshot_id: ContextSnapshotId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactIngestionStatus {
    Created,
    Staging,
    BlobStaged,
    Promoting,
    Quarantined,
    Inspecting,
    Available,
    Rejected,
    Unknown,
    Cancelled,
}

pub struct ArtifactIngestionSession {
    pub id: ArtifactIngestionSessionId,
    pub workspace_id: WorkspaceId,
    pub artifact_id: ArtifactId,
    pub proposed_revision_id: ArtifactRevisionId,
    pub source: ArtifactIngestionSource,
    pub status: ArtifactIngestionStatus,
    pub maximum_bytes: u64,
    pub expected_media_type: Option<String>,
    pub staged_blob: Option<StagedArtifactBlobMetadata>,
    pub logical_revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

The source candidate and ingestion session use distinct IDs. A candidate can be retried only through one idempotent ingestion key and cannot create two logical Artifact revisions.

### Inspection and quarantine

```rust
pub enum ArtifactInspectionKind {
    MediaType,
    ArchiveSafety,
    Malware,
    SecretDetection,
    ContentPolicy,
    ParserSafety,
}

pub enum ArtifactInspectionOutcome {
    Passed,
    PassedWithWarnings,
    ManualReviewRequired,
    Rejected,
    ScannerUnavailable,
}

pub struct ArtifactInspectionFinding {
    pub code: String,
    pub severity: RiskLevel,
    pub byte_range: Option<ArtifactByteRange>,
    pub irreversible_fingerprint: Option<[u8; 32]>,
    pub safe_message: String,
}

pub struct ArtifactInspectionReport {
    pub id: ArtifactInspectionReportId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub inspector_revision: String,
    pub outcomes: Vec<(ArtifactInspectionKind, ArtifactInspectionOutcome)>,
    pub findings: Vec<ArtifactInspectionFinding>,
    pub detected_media_type: String,
    pub classification: DataClassification,
    pub recommendation: ArtifactInspectionRecommendation,
    pub created_at: Timestamp,
}
```

Required inspection policy is selected by declared/detected media type, source, classification and intended use. `ScannerUnavailable` cannot become Available when the applicable policy requires that scanner.

### Representations, chunks and excerpts

```rust
pub enum ArtifactRepresentationKind {
    ExtractedText,
    Markdown,
    Thumbnail,
    PageImage,
    AudioTranscript,
    StructuredTable,
    ArchiveManifest,
    CodeIndex,
    EmbeddingChunks,
    Summary,
    RedactedCopy,
    Preview,
}

pub enum ArtifactRepresentationStatus {
    Pending,
    Processing,
    Available,
    Failed,
    Rejected,
    Purged,
}

pub struct ArtifactRepresentation {
    pub id: ArtifactRepresentationId,
    pub source_revision_id: ArtifactRevisionId,
    pub kind: ArtifactRepresentationKind,
    pub generator_revision: String,
    pub output_artifact_revision_id: Option<ArtifactRevisionId>,
    pub status: ArtifactRepresentationStatus,
    pub content_hash: Option<ArtifactContentHash>,
    pub created_at: Timestamp,
}

pub struct ArtifactChunk {
    pub id: ArtifactChunkId,
    pub representation_id: ArtifactRepresentationId,
    pub ordinal: u32,
    pub source_byte_start: Option<u64>,
    pub source_byte_end: Option<u64>,
    pub text: String,
    pub text_hash: [u8; 32],
    pub estimated_tokens: u64,
    pub classification: DataClassification,
}

pub struct ArtifactExcerpt {
    pub chunk_id: ArtifactChunkId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub text: String,
    pub score_basis_points: u16,
    pub discovery_channels: Vec<String>,
    pub provenance: Vec<RunReference>,
}
```

Chunks are bounded to 64 KiB text and ordered deterministically. Embeddings and FTS documents are derived and rebuildable. Exact revision IDs and chunk hashes are retained in context snapshots.

### Artifact provenance

```rust
pub enum ArtifactProvenanceEdgeKind {
    ProducedByRunStep,
    ImportedFromExternal,
    DerivedFromArtifactRevision,
    GeneratedByProcessor,
    RedactedFrom,
    CapturedFromContextSources,
    ExportedFrom,
}

pub struct ArtifactProvenanceEdge {
    pub id: ArtifactProvenanceEdgeId,
    pub from_node: ArtifactProvenanceNodeId,
    pub to_node: ArtifactProvenanceNodeId,
    pub kind: ArtifactProvenanceEdgeKind,
    pub generator_revision: Option<String>,
    pub parameters_hash: Option<[u8; 32]>,
    pub created_at: Timestamp,
}
```

`DerivedFromArtifactRevision`, `GeneratedByProcessor`, `RedactedFrom` and `CapturedFromContextSources` form an acyclic derivation graph. Contextual links may be stored separately and never redefine byte derivation.

### Context source, trust and omission values

```rust
pub enum ContextSourceKind {
    RuntimePolicy,
    Identity,
    Objective,
    ConfirmedConstraint,
    PlanStep,
    LongTermMemory,
    RunState,
    Scratchpad,
    Evidence,
    ArtifactExcerpt,
    ToolDefinition,
    OutputContract,
    RemainingBudget,
}

pub enum ContextTrustClass {
    RuntimeAuthoritative,
    UserConfirmed,
    ActiveMemory,
    VerifiedExecution,
    CorroboratedEvidence,
    ExternalUntrusted,
    ModelProposed,
}

pub enum ContextRequirement {
    Mandatory,
    Preferred,
    Optional,
}

pub enum ContextCompressionLevel {
    Full,
    Compact,
    Atomic,
    ReferenceOnly,
}

pub enum ContextOmissionReason {
    PolicyDenied,
    RedactedEntirely,
    TokenBudget,
    LowerPriority,
    Superseded,
    Expired,
    ConflictUnresolved,
    Duplicate,
    MissingRepresentation,
    UnsupportedModality,
    DelegationScope,
    SourceUnavailable,
    CapturePolicy,
}

pub struct ContextSourceItem {
    pub id: ContextSourceItemId,
    pub kind: ContextSourceKind,
    pub requirement: ContextRequirement,
    pub trust: ContextTrustClass,
    pub classification: DataClassification,
    pub references: Vec<RunReference>,
    pub full: ContextRenderableValue,
    pub compact: Option<ContextRenderableValue>,
    pub atomic: Option<ContextRenderableValue>,
    pub content_hash: [u8; 32],
}
```

Sources from lower-trust layers cannot be rendered as runtime instructions. External content is wrapped and labeled as data.

### Token estimator and context budget

```rust
pub struct ContextTokenBudget {
    pub model_context_limit: u64,
    pub reserved_output_tokens: u64,
    pub provider_protocol_reserve: u64,
    pub safety_margin_tokens: u64,
}

pub struct TokenCount {
    pub tokens: u64,
    pub exact: bool,
    pub estimator_revision: String,
}

#[async_trait::async_trait]
pub trait TokenEstimatorPort: Send + Sync {
    async fn count_text(
        &self,
        model_revision_id: ModelRevisionId,
        text: &str,
    ) -> Result<TokenCount, ApplicationError>;

    async fn count_messages(
        &self,
        model_revision_id: ModelRevisionId,
        messages: &[ModelMessage],
    ) -> Result<TokenCount, ApplicationError>;
}
```

The fallback estimator is deliberately conservative: UTF-8 byte count plus configured message overhead. It records `exact = false`. It may waste capacity but cannot claim a source fits by using an optimistic estimate.

### Model context envelope and snapshot

```rust
pub struct ContextEnvelopeSection {
    pub kind: ContextSourceKind,
    pub rendered: String,
    pub source_item_ids: Vec<ContextSourceItemId>,
    pub trust: ContextTrustClass,
    pub classification: DataClassification,
    pub compression: ContextCompressionLevel,
    pub token_count: u64,
}

pub struct ModelContextEnvelope {
    pub id: ModelContextEnvelopeId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub model_revision_id: ModelRevisionId,
    pub sections: Vec<ContextEnvelopeSection>,
    pub tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub total_input_tokens: u64,
    pub rendered_hash: [u8; 32],
    pub data_classification: DataClassification,
}

pub enum ContextCaptureMode {
    Full,
    Redacted,
    StructuredOnly,
    MetadataOnly,
}

pub struct ContextTokenAccounting {
    pub limit: u64,
    pub reserved_output: u64,
    pub provider_reserve: u64,
    pub safety_margin: u64,
    pub included: u64,
    pub omitted_estimate: u64,
    pub estimator_revision: String,
    pub exact: bool,
}

pub struct ContextSnapshot {
    pub id: ContextSnapshotId,
    pub envelope_id: ModelContextEnvelopeId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub model_revision_id: ModelRevisionId,
    pub policy_revision_id: ContextAssemblyPolicyRevisionId,
    pub source_manifest_hash: [u8; 32],
    pub rendered_hash: [u8; 32],
    pub token_accounting: ContextTokenAccounting,
    pub capture_mode: ContextCaptureMode,
    pub capture_blob_key: Option<ArtifactBlobKey>,
    pub created_at: Timestamp,
}
```

A snapshot stores included sources, omitted sources/reasons and redaction records in normalized child rows. `capture_blob_key` is absent for MetadataOnly and may later be purged without deleting snapshot metadata.

### Context assembly policy

```rust
pub struct ContextSectionAllocationRule {
    pub kind: ContextSourceKind,
    pub priority: u16,
    pub minimum_tokens: u64,
    pub target_tokens: u64,
    pub maximum_tokens: u64,
    pub allowed_compression: Vec<ContextCompressionLevel>,
}

pub struct ContextAssemblyPolicyRevision {
    pub id: ContextAssemblyPolicyRevisionId,
    pub revision: u32,
    pub rules: Vec<ContextSectionAllocationRule>,
    pub capture_mode: ContextCaptureMode,
    pub allow_unverified_observations: bool,
    pub maximum_external_evidence_items: u32,
    pub content_hash: [u8; 32],
}
```

Allocation order is stable by mandatory status, priority, source kind and source item ID. Mandatory sections either fit at an allowed safe representation or context build fails with `mandatory_context_too_large`.

### Structured scratchpad

```rust
pub enum ScratchpadScope {
    Run,
    Step { step_id: RunStepId },
}

pub enum ScratchpadEntryKind {
    Assumption,
    OpenQuestion,
    CandidateAction,
    Calculation,
    Risk,
    DecisionPoint,
}

pub enum ScratchpadEntryStatus {
    Open,
    Confirmed,
    Resolved,
    Rejected,
    Superseded,
}

pub struct ScratchpadEntry {
    pub id: ScratchpadEntryId,
    pub kind: ScratchpadEntryKind,
    pub status: ScratchpadEntryStatus,
    pub title: String,
    pub structured: serde_json::Value,
    pub supporting_references: Vec<RunReference>,
    pub created_by: RunActorRef,
}

pub struct ScratchpadRevision {
    pub id: ScratchpadRevisionId,
    pub run_id: AgentRunId,
    pub scope: ScratchpadScope,
    pub revision: u32,
    pub entries: Vec<ScratchpadEntry>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

`structured` is validated by an entry-kind schema. It may contain concise statements, expressions, results, units and mitigation summaries, but not arrays of private reasoning tokens, hidden deliberation transcripts or provider reasoning fields.

### Evidence and contradiction records

```rust
pub enum EvidenceSourceRef {
    ArtifactRevision(ArtifactRevisionId),
    MemoryRevision(MemoryRevisionId),
    ToolInvocation(ToolInvocationId),
    Handoff(HandoffArtifactId),
    ExternalReference(String),
}

pub enum EvidenceTrustClass {
    Untrusted,
    Contextual,
    Corroborated,
    Authoritative,
}

pub struct EvidenceItem {
    pub id: EvidenceItemId,
    pub workspace_id: WorkspaceId,
    pub source: EvidenceSourceRef,
    pub retrieval_time: Timestamp,
    pub content_hash: [u8; 32],
    pub excerpt: String,
    pub authority_metadata: serde_json::Value,
    pub freshness: EvidenceFreshness,
    pub classification: DataClassification,
    pub trust: EvidenceTrustClass,
    pub artifact_revision_id: Option<ArtifactRevisionId>,
}

pub struct EvidenceClaim {
    pub id: EvidenceClaimId,
    pub evidence_item_id: EvidenceItemId,
    pub subject: String,
    pub predicate: String,
    pub value: serde_json::Value,
    pub claim_hash: [u8; 32],
}

pub struct EvidenceContradiction {
    pub id: EvidenceContradictionId,
    pub left_claim_id: EvidenceClaimId,
    pub right_claim_id: EvidenceClaimId,
    pub kind: EvidenceContradictionKind,
    pub status: EvidenceContradictionStatus,
    pub resolution_reference: Option<RunReference>,
}
```

H6 performs deterministic exact-key contradiction detection and stores unresolved conflicts explicitly. H10 may add richer evaluators without rewriting these claims.

### Mounts, exports and retention

```rust
pub struct ArtifactMountGrant {
    pub id: ArtifactMountGrantId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub sandbox_session_id: SandboxSessionId,
    pub mount_path: String,
    pub read_only: bool,
    pub maximum_uses: u16,
    pub expires_at: Timestamp,
    pub status: ArtifactGrantStatus,
}

pub struct ArtifactExportGrant {
    pub id: ArtifactExportGrantId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub principal_id: PrincipalId,
    pub run_id: Option<AgentRunId>,
    pub purpose: String,
    pub maximum_uses: u16,
    pub expires_at: Timestamp,
    pub status: ArtifactGrantStatus,
}

pub struct ArtifactRetentionPolicyRevision {
    pub id: ArtifactRetentionPolicyRevisionId,
    pub revision: u32,
    pub default_retention_days: u32,
    pub quarantine_retention_days: u32,
    pub export_copy_retention_hours: u32,
    pub purge_context_captures: bool,
    pub content_hash: [u8; 32],
}

pub enum ArtifactPurgeStatus {
    Requested,
    BlockedByLegalHold,
    Enumerating,
    DeletingDerivedData,
    DeletingBlobs,
    Verifying,
    Completed,
    Failed,
    Unknown,
}
```

Mount/export grants are references checked at the enforcement point, not direct blob credentials. Hard purge is restart-safe and verifies absence through every configured blob backend before completion.

### Run consolidation into memory candidates

```rust
pub enum ConsolidationSourceKind {
    VerifiedRunOutcome,
    AcceptedHandoff,
    ConfirmedDecision,
    ReusableProcedure,
    UnresolvedTask,
    EvaluatedEvidence,
}

pub struct MemoryCandidateDraft {
    pub kind: MemoryKind,
    pub content: String,
    pub structured: serde_json::Value,
    pub source_references: Vec<RunReference>,
    pub confidence_micros: u32,
    pub importance_micros: u32,
    pub classifications: DataClassification,
    pub proposed_scopes: Vec<MemoryScopeRef>,
}

pub struct RunConsolidationReport {
    pub id: RunConsolidationReportId,
    pub run_id: AgentRunId,
    pub included_sources: Vec<RunReference>,
    pub excluded_sources: Vec<ConsolidationExclusion>,
    pub submitted_candidate_ids: Vec<MemoryId>,
    pub conflicts: Vec<MemoryConflictReference>,
    pub status: RunConsolidationStatus,
    pub created_at: Timestamp,
}
```

Every draft is submitted through existing v0.1 memory commands as `Candidate`. Existing manual/assisted/automatic write policy alone determines later activation.

---

### Task 1: Add Artifact identities, revisions, blob and provenance domain contracts

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/artifact/mod.rs`
- Create: `crates/vestrace-domain/src/artifact/artifact.rs`
- Create: `crates/vestrace-domain/src/artifact/revision.rs`
- Create: `crates/vestrace-domain/src/artifact/blob.rs`
- Create: `crates/vestrace-domain/src/artifact/ingestion.rs`
- Create: `crates/vestrace-domain/src/artifact/provenance.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline domain unit and property tests

**Interfaces:**
- Adds `ArtifactId`, `ArtifactRevisionId`, `ArtifactStorageNamespaceId`, `ArtifactIngestionSessionId`, `ArtifactInspectionReportId`, `ArtifactRepresentationId`, `ArtifactChunkId`, `ArtifactProvenanceNodeId`, `ArtifactProvenanceEdgeId`, `ArtifactMountGrantId`, `ArtifactExportGrantId`, `ArtifactRetentionPolicyRevisionId`, `ArtifactLegalHoldId`, `ArtifactPurgeRequestId`, `ArtifactPurgeEventId`, `ContextSourceItemId`, `ModelContextEnvelopeId`, `ContextSnapshotId`, `ContextAssemblyPolicyRevisionId`, `ScratchpadEntryId`, `ScratchpadRevisionId`, `EvidenceItemId`, `EvidenceClaimId`, `EvidenceContradictionId`, `RunConsolidationReportId` and `MemoryCandidateBindingId`.
- Produces Artifact identity/revision/blob/ingestion/provenance types from the normative sections above.

- [ ] **Step 1: Write failing Artifact invariants**

```rust
#[test]
fn available_revision_cannot_change_content_identity() {
    let revision = ArtifactRevisionBuilder::available_fixture().build().unwrap();
    assert!(revision.revise_bytes(ArtifactContentHash::test_other()).is_err());
}

#[test]
fn filename_never_preserves_a_path() {
    let revision = ArtifactRevisionBuilder::fixture()
        .original_filename("../../secret.txt")
        .build();
    assert!(revision.is_err());
}

#[test]
fn derived_provenance_rejects_a_cycle() {
    let graph = ArtifactProvenanceGraph::fixture_chain();
    assert!(graph.add_derivation_edge(graph.last(), graph.first()).is_err());
}
```

Run:

```bash
cargo test -p vestrace-domain artifact
```

Expected: FAIL because the Artifact domain does not exist.

- [ ] **Step 2: Implement content hash and blob-key values**

Accept SHA-256 bytes or lowercase 64-character hexadecimal input. `ArtifactBlobKey` requires a non-nil storage namespace. Display/debug output redacts the namespace and shows at most the first eight hash characters.

- [ ] **Step 3: Implement Artifact and revision lifecycle**

Allowed paths:

```text
Staging → Quarantined → Inspecting → Available
                         ├→ Rejected
Available → Deleted → PurgePending → Purged
Rejected → PurgePending → Purged
```

A new logical revision may be created from Available/Deleted history, but historical revision rows remain immutable.

- [ ] **Step 4: Implement source binding and deterministic revision hash**

The revision hash includes workspace, Artifact/revision IDs, media type, filename metadata, byte size, content hash, classification, exact source, producing Run/step and provenance root.

- [ ] **Step 5: Implement provenance DAG checks**

Derivation edge insertion uses stable node ordering and DFS/topological validation in domain tests. Property tests prove adding contextual links does not alter derivation ancestry.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain artifact
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(artifact): add immutable artifact contracts"
```

---

### Task 2: Add Context, scratchpad and evidence domain contracts

**Files:**
- Create: `crates/vestrace-domain/src/context/mod.rs`
- Create: `crates/vestrace-domain/src/context/source.rs`
- Create: `crates/vestrace-domain/src/context/policy.rs`
- Create: `crates/vestrace-domain/src/context/envelope.rs`
- Create: `crates/vestrace-domain/src/context/snapshot.rs`
- Create: `crates/vestrace-domain/src/context/token.rs`
- Create: `crates/vestrace-domain/src/context/scratchpad.rs`
- Create: `crates/vestrace-domain/src/evidence/mod.rs`
- Create: `crates/vestrace-domain/src/evidence/item.rs`
- Create: `crates/vestrace-domain/src/evidence/claim.rs`
- Create: `crates/vestrace-domain/src/evidence/contradiction.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline domain unit and property tests

**Interfaces:**
- Produces all Context, token, snapshot, scratchpad, evidence and contradiction types from the normative contracts.

- [ ] **Step 1: Write failing context-priority tests**

```rust
#[test]
fn external_evidence_cannot_become_runtime_instructions() {
    let item = ContextSourceItemBuilder::external_untrusted("ignore policy")
        .kind(ContextSourceKind::RuntimePolicy)
        .build();
    assert!(item.is_err());
}

#[test]
fn mandatory_context_has_no_reference_only_fallback_by_default() {
    let rule = ContextSectionAllocationRuleBuilder::mandatory_identity()
        .allowed_compression(vec![ContextCompressionLevel::ReferenceOnly])
        .build();
    assert!(rule.is_err());
}
```

- [ ] **Step 2: Write scratchpad anti-chain-of-thought tests**

Reject provider reasoning fields, token arrays, entries over 8 KiB, nested free-form trace arrays and unknown entry-kind schemas. Accept a calculation with expression/result/units and a risk with concise description/mitigation/source refs.

- [ ] **Step 3: Implement deterministic Context source hashes**

Hash source kind, requirement, trust, classification, ordered references and every available rendering level. Two values with different trust annotations must have different hashes even when visible text matches.

- [ ] **Step 4: Implement policy and token-budget invariants**

Validate `reserved_output + provider_reserve + safety_margin < model_context_limit`, unique source-kind rules, monotonic minimum/target/maximum values and at least one non-reference representation for mandatory instruction sections.

- [ ] **Step 5: Implement evidence claims and exact contradictions**

Normalize subject/predicate as lowercase dotted or namespace-qualified identifiers. Canonicalize values using H2 JSON canonicalization. Claims with the same subject/predicate and non-equal canonical values become an unresolved exact contradiction unless their validity intervals do not overlap.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-domain context evidence
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(context): add context scratchpad and evidence contracts"
```

---

### Task 3: Define H6 application ports, commands and deterministic test support

**Files:**
- Create: `crates/vestrace-application/src/artifact/mod.rs`
- Create: `crates/vestrace-application/src/artifact/ports.rs`
- Create: `crates/vestrace-application/src/artifact/commands.rs`
- Create: `crates/vestrace-application/src/context/mod.rs`
- Create: `crates/vestrace-application/src/context/ports.rs`
- Create: `crates/vestrace-application/src/evidence/mod.rs`
- Create: `crates/vestrace-application/src/consolidation/mod.rs`
- Create: `crates/vestrace-application/src/consolidation/ports.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-artifact-test-support/Cargo.toml`
- Create: `crates/vestrace-artifact-test-support/src/lib.rs`
- Create: `crates/vestrace-artifact-test-support/src/fakes.rs`
- Create: `crates/vestrace-artifact-test-support/src/streams.rs`
- Create: `crates/vestrace-artifact-test-support/src/fixtures.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `ArtifactBlobStorePort`, `ArtifactRepositoryPort`, `ArtifactIngestionRepositoryPort`, `ArtifactInspectionPort`, `ArtifactProcessorPort`, `ArtifactExcerptSearchPort`, `ArtifactCandidateSourcePort`, `ExternalArtifactFetchPort`, `ArtifactMountBrokerPort`, `ArtifactExportStreamPort`, `ContextSourceProviderPort`, `MemoryContextPort`, `IdentityContextPort`, `RunContextPort`, `TokenEstimatorPort`, `ContextSnapshotRepositoryPort`, `ScratchpadRepositoryPort`, `EvidenceRepositoryPort` and `MemoryCandidatePort`.
- Produces deterministic fake stores, source providers, token estimators, processors, scanners and fault injection.

- [ ] **Step 1: Write object-safety compile tests**

```rust
fn accepts_blob_store(_v: std::sync::Arc<dyn ArtifactBlobStorePort>) {}
fn accepts_processor(_v: std::sync::Arc<dyn ArtifactProcessorPort>) {}
fn accepts_context_source(_v: std::sync::Arc<dyn ContextSourceProviderPort>) {}
fn accepts_estimator(_v: std::sync::Arc<dyn TokenEstimatorPort>) {}
fn accepts_memory(_v: std::sync::Arc<dyn MemoryCandidatePort>) {}
```

- [ ] **Step 2: Define exact candidate-source reads**

```rust
#[async_trait::async_trait]
pub trait ArtifactCandidateSourcePort: Send + Sync {
    async fn describe(
        &self,
        context: &RequestContext,
        reference: ArtifactCandidateReference,
    ) -> Result<ArtifactCandidateDescriptor, ApplicationError>;

    async fn open(
        &self,
        context: &RequestContext,
        reference: ArtifactCandidateReference,
        maximum_bytes: u64,
    ) -> Result<ArtifactByteStream, ApplicationError>;
}
```

Implement test adapters for H4 sandbox/tool candidates and H5 handoff candidates without copying their SDK/infrastructure types into H6 domain.

- [ ] **Step 3: Define v0.1 Memory adapters**

```rust
#[async_trait::async_trait]
pub trait MemoryContextPort: Send + Sync {
    async fn retrieve(
        &self,
        context: &RequestContext,
        request: MemoryContextRequest,
    ) -> Result<MemoryContextSelection, ApplicationError>;
}

#[async_trait::async_trait]
pub trait MemoryCandidatePort: Send + Sync {
    async fn check_conflicts(
        &self,
        context: &RequestContext,
        drafts: &[MemoryCandidateDraft],
    ) -> Result<Vec<MemoryConflictReference>, ApplicationError>;

    async fn submit_candidates(
        &self,
        context: &RequestContext,
        request: SubmitMemoryCandidates,
    ) -> Result<Vec<MemoryId>, ApplicationError>;
}
```

Production adapters call existing v0.1 application services and preserve exact MemoryRevision/provenance references.

- [ ] **Step 4: Add deterministic H6 fault points**

```rust
pub enum H6FaultPoint {
    AfterIngestionCreated,
    AfterBlobStaged,
    AfterBlobPromoted,
    AfterRevisionQuarantined,
    AfterInspectionStored,
    AfterRepresentationBlobPromoted,
    AfterContextSnapshotStored,
    AfterMountGrantConsumed,
    AfterPurgeBlobDeleted,
    AfterMemoryCandidatesSubmitted,
}
```

Each fault fires once to support restart tests.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application artifact context evidence consolidation
cargo test -p vestrace-artifact-test-support
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-artifact-test-support
git commit -m "feat(context): define H6 ports and deterministic fixtures"
```

---

### Task 4: Implement Local CAS and S3-compatible blob stores with one conformance suite

**Files:**
- Create: `crates/vestrace-artifact-store-local/Cargo.toml`
- Create: `crates/vestrace-artifact-store-local/src/lib.rs`
- Create: `crates/vestrace-artifact-store-local/src/store.rs`
- Create: `crates/vestrace-artifact-store-local/src/layout.rs`
- Create: `crates/vestrace-artifact-store-local/src/staging.rs`
- Create: `crates/vestrace-artifact-store-local/src/range.rs`
- Create: `crates/vestrace-artifact-store-local/src/security.rs`
- Create: `crates/vestrace-artifact-store-local/src/reconcile.rs`
- Create: `crates/vestrace-artifact-store-s3/Cargo.toml`
- Create: `crates/vestrace-artifact-store-s3/src/lib.rs`
- Create: `crates/vestrace-artifact-store-s3/src/store.rs`
- Create: `crates/vestrace-artifact-store-s3/src/config.rs`
- Create: `crates/vestrace-artifact-store-s3/src/multipart.rs`
- Create: `crates/vestrace-artifact-store-s3/src/range.rs`
- Create: `crates/vestrace-artifact-store-s3/src/error.rs`
- Create: `crates/vestrace-artifact-store-s3/src/reconcile.rs`
- Create: `crates/vestrace-artifact-test-support/src/store_conformance.rs`
- Create: `tests/artifact_store_local_conformance.rs`
- Create: `tests/artifact_store_s3_conformance.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `LocalContentAddressedStore`, `S3CompatibleArtifactStore` and one unchanged `ArtifactBlobStoreConformanceSuite`.

- [ ] **Step 1: Define mandatory conformance cases**

```text
streaming upload without full buffering
maximum-byte enforcement
expected-size mismatch
expected-hash mismatch
idempotent promotion
range read boundaries
missing blob inspection
ambiguous promotion reconciliation
idempotent deletion
staging cleanup
same hash in different namespaces remains isolated
secret/path data absent from errors
```

- [ ] **Step 2: Implement secure Local CAS layout**

Use deployment-owned root plus opaque namespace directory and two-level hash fanout. Open directories/files descriptor-relatively where supported, reject symlink traversal, create staging files with exclusive permissions, fsync file and parent directory, then atomically rename to final hash path. Never derive paths from filenames or media types.

- [ ] **Step 3: Implement Local CAS range reads and cleanup**

Validate inclusive/exclusive bounds with checked arithmetic. Cleanup only removes a staging handle issued by the store and located below the configured staging root. A forged handle fails.

- [ ] **Step 4: Implement S3 staging and promotion**

Use a staging prefix keyed by ingestion session and a final prefix keyed by namespace/hash. Multipart upload computes SHA-256 locally while streaming. Promotion copies or completes to deterministic final key, verifies object size/checksum metadata and safely deletes staging. Bucket, endpoint and service credential resolution remain deployment configuration, never Artifact metadata.

- [ ] **Step 5: Add loopback S3 fixture tests**

Mandatory CI uses a deterministic local S3-compatible fixture for multipart, HEAD, range GET, copy and delete semantics. A separate optional smoke profile may use MinIO, but no public account is required.

- [ ] **Step 6: Run and commit**

```bash
cargo test --test artifact_store_local_conformance --test artifact_store_s3_conformance
cargo clippy -p vestrace-artifact-store-local -p vestrace-artifact-store-s3 \
  --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/vestrace-artifact-store-local \
  crates/vestrace-artifact-store-s3 crates/vestrace-artifact-test-support tests
git commit -m "feat(artifact-store): add local and S3 blob backends"
```

---

### Task 5: Persist Artifact identities, revisions, blobs and provenance

**Files:**
- Create: `migrations/0042_artifacts_revisions_blobs_and_provenance.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/blob_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/provenance_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `tests/artifact_persistence.rs`

**Interfaces:**
- Creates `artifact_storage_namespaces`, `artifacts`, `artifact_revisions`, `artifact_blobs`, `artifact_blob_references`, `artifact_provenance_nodes` and `artifact_provenance_edges`.
- Produces PostgreSQL Artifact/blob/provenance repositories.

- [ ] **Step 1: Write immutability and workspace-isolation tests**

Assert revision content columns and derivation edges cannot update/delete through application roles; a blob key from another workspace namespace cannot be attached; revision numbers are contiguous; an Artifact current revision belongs to that Artifact; derivation cycles fail.

- [ ] **Step 2: Create migration `0042`**

Store SHA-256 as fixed 32-byte values. `artifact_blob_references` owns the many-to-one link between immutable revisions/representations/captures and physical blobs. Do not use a manually trusted ref-count column as the sole GC authority.

- [ ] **Step 3: Implement atomic metadata creation**

Create Artifact identity, proposed revision, provenance root and ingestion binding in one transaction. Available revision finalization later atomically links an existing promoted blob and updates the current-revision pointer.

- [ ] **Step 4: Implement provenance traversal**

Provide bounded ancestor/descendant queries, derivation-cycle checks and a complete provenance projection for one Artifact revision. Queries always remain inside one workspace.

- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test artifact_persistence
git add migrations/0042_artifacts_revisions_blobs_and_provenance.sql crates tests/artifact_persistence.rs
git commit -m "feat(artifact): persist revisions blobs and provenance"
```

---

### Task 6: Implement restart-safe ingestion, quarantine and inspection

**Files:**
- Create: `migrations/0043_artifact_ingestion_inspections_representations_chunks.sql`
- Create: `crates/vestrace-application/src/artifact/ingestion.rs`
- Create: `crates/vestrace-application/src/artifact/inspection.rs`
- Create: `crates/vestrace-artifact-inspector/Cargo.toml`
- Create: `crates/vestrace-artifact-inspector/src/lib.rs`
- Create: `crates/vestrace-artifact-inspector/src/media.rs`
- Create: `crates/vestrace-artifact-inspector/src/archive.rs`
- Create: `crates/vestrace-artifact-inspector/src/secrets.rs`
- Create: `crates/vestrace-artifact-inspector/src/malware.rs`
- Create: `crates/vestrace-artifact-inspector/src/classification.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/ingestion_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/inspection_repository.rs`
- Create: `crates/vestrace-application/tests/artifact_ingestion.rs`
- Create: `crates/vestrace-application/tests/artifact_inspection.rs`
- Create: `tests/artifact_ingestion_restart.rs`
- Create: `tests/artifact_inspection_persistence.rs`

**Interfaces:**
- Creates `artifact_ingestion_sessions`, `artifact_ingestion_events`, `artifact_inspection_reports`, `artifact_inspection_outcomes`, `artifact_inspection_findings`, `artifact_representations`, `artifact_chunks` and processor-work tables.
- Produces `ArtifactIngestionService` and `ArtifactInspectionService`.

- [ ] **Step 1: Write ingestion order and crash-window tests**

```text
create session and Artifact draft
→ stage bytes
→ persist staged hash/size/handle metadata
→ promote deterministic blob
→ inspect final blob presence
→ atomically link blob and mark revision Quarantined
→ enqueue inspection
```

Crash after staging cleans or resumes the same staging handle. Crash after promotion inspects the final key and links it; it never uploads a second logical revision.

- [ ] **Step 2: Create migration `0043`**

Ingestion events and inspection reports are append-only. Staging handles are bounded opaque encrypted-or-metadata-only values according to deployment policy and are never returned through public/application read models.

- [ ] **Step 3: Implement media validation**

Compare declared media type, extension hint and magic bytes. A mismatch emits a stable finding and uses detected type for processor eligibility. Reject polyglot/active content according to policy.

- [ ] **Step 4: Implement archive limits**

Inspect manifests without extracting to host paths. Enforce maximum entries, total expanded bytes, per-entry bytes, depth, compression ratio, duplicate names, traversal, absolute paths, links, devices and nested-archive limits.

- [ ] **Step 5: Implement secret and malware scanner ports**

Built-in secret inspection covers configured known-secret hashes plus credential/private-key patterns. Findings contain only fingerprints. Malware scanning uses `MalwareScannerPort`; deterministic CI scanner recognizes safe and malicious fixtures. Required scanner outage leaves content Quarantined or ManualReviewRequired.

- [ ] **Step 6: Implement final recommendation**

Only all-required Passed/PassedWithWarnings reports permit `Available`. Rejected stores a content-free safe reason and schedules quarantine retention cleanup. Manual review stays Quarantined.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test artifact_ingestion --test artifact_inspection
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test artifact_ingestion_restart --test artifact_inspection_persistence
cargo clippy -p vestrace-artifact-inspector --all-targets -- -D warnings
git add migrations/0043_artifact_ingestion_inspections_representations_chunks.sql \
  crates tests
git commit -m "feat(artifact): ingest quarantine and inspect bytes"
```

---

### Task 7: Build representations, deterministic chunks and excerpt retrieval

**Files:**
- Create: `crates/vestrace-domain/src/artifact/inspection.rs`
- Create: `crates/vestrace-domain/src/artifact/representation.rs`
- Modify: `crates/vestrace-domain/src/artifact/mod.rs`
- Create: `crates/vestrace-application/src/artifact/representation.rs`
- Create: `crates/vestrace-application/src/artifact/excerpts.rs`
- Create: `crates/vestrace-artifact-processors/Cargo.toml`
- Create: `crates/vestrace-artifact-processors/src/lib.rs`
- Create: `crates/vestrace-artifact-processors/src/text.rs`
- Create: `crates/vestrace-artifact-processors/src/json.rs`
- Create: `crates/vestrace-artifact-processors/src/archive_manifest.rs`
- Create: `crates/vestrace-artifact-processors/src/sandboxed.rs`
- Create: `crates/vestrace-artifact-processors/src/chunking.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/representation_repository.rs`
- Create: `crates/vestrace-application/tests/artifact_representation.rs`
- Create: `tests/artifact_representation_persistence.rs`
- Create: `tests/artifact_excerpt_search.rs`

**Interfaces:**
- Produces `ArtifactRepresentationService`, `ArtifactChunkingService`, `ArtifactExcerptSearchService`, built-in text/JSON/archive-manifest processors and an H4 sandboxed-processor adapter.

- [ ] **Step 1: Write deterministic representation tests**

The same source revision, processor revision and parameters produce the same representation idempotency key, output hash and chunk boundaries. A changed processor revision creates a new representation instead of replacing history.

- [ ] **Step 2: Implement processor eligibility**

Select by detected media type, intended representation, classification and policy. Native processors handle UTF-8 text, JSON and safe archive manifests. Other parser types require a digest-pinned H4 sandbox processor with DenyAll networking and read-only source mount.

- [ ] **Step 3: Implement representation execution order**

```text
load Available source revision
→ H2 artifact.process authorization and budget reservation
→ create Processing representation
→ run native or H4 sandbox processor
→ ingest processor output through the same quarantine path
→ link output revision and provenance
→ create deterministic chunks/indexes
→ mark representation Available
```

Processor output never bypasses quarantine because it was produced internally.

- [ ] **Step 4: Implement deterministic chunking**

Normalize line endings without changing stored representation bytes. Split on structural boundaries, then bounded UTF-8-safe windows with configured overlap. Store source offsets where exact. Token counts use the configured estimator revision.

- [ ] **Step 5: Implement excerpt retrieval**

Use exact filters, PostgreSQL FTS and optional pgvector, fused with RRF like v0.1 retrieval. Apply workspace, Artifact lifecycle, classification and policy filters before candidate generation. Degraded FTS-only retrieval is explicit.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test artifact_representation
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test artifact_representation_persistence --test artifact_excerpt_search
cargo clippy -p vestrace-artifact-processors --all-targets -- -D warnings
git add crates tests
git commit -m "feat(artifact): build representations chunks and excerpts"
```

---

### Task 8: Persist and manage context snapshots, structured scratchpads and evidence

**Files:**
- Create: `migrations/0044_context_snapshots_scratchpads_and_evidence.sql`
- Create: `crates/vestrace-application/src/context/scratchpad_service.rs`
- Create: `crates/vestrace-application/src/evidence/service.rs`
- Create: `crates/vestrace-application/src/evidence/contradictions.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/context/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/context/snapshot_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/context/scratchpad_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/evidence/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/evidence/repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/evidence/contradiction_repository.rs`
- Create: `crates/vestrace-application/tests/scratchpad_service.rs`
- Create: `crates/vestrace-application/tests/evidence_service.rs`
- Create: `tests/context_snapshot_persistence.rs`
- Create: `tests/evidence_persistence.rs`
- Create: `tests/scratchpad_persistence.rs`

**Interfaces:**
- Creates `context_assembly_policy_revisions`, `context_snapshots`, `context_snapshot_sections`, `context_snapshot_sources`, `context_snapshot_omissions`, `context_redaction_records`, `context_capture_references`, `scratchpad_revisions`, `scratchpad_entries`, `evidence_items`, `evidence_claims` and `evidence_contradictions`.
- Produces PostgreSQL snapshot/scratchpad/evidence ports and services.

- [ ] **Step 1: Write snapshot append-only and capture-mode tests**

Assert snapshot source/omission rows cannot update; MetadataOnly has no capture blob; Redacted capture references only redacted bytes; Full capture is still classification/policy checked; capture blob purge does not delete snapshot hashes or source references.

- [ ] **Step 2: Create migration `0044`**

Store rendered content only through optional capture blob references. Section rows store hashes, token count, compression, trust and classification. Redaction records store applied rule revisions and affected source IDs, not removed secret text.

- [ ] **Step 3: Implement scratchpad revision service**

Updates create a full immutable new revision with optimistic expected revision. Model-proposed changes are drafts and cannot change `created_by`, source refs or confirmed status. Confirmation requires an authoritative user/system/verified-result reference.

- [ ] **Step 4: Implement Evidence service**

Evidence creation verifies the source exists and hash matches. Excerpts are bounded and redacted according to classification. Trust upgrades are explicit protected commands with supporting references; ingestion source claims cannot self-declare Authoritative.

- [ ] **Step 5: Implement exact contradiction detection**

On claim insert, compare same normalized subject/predicate and overlapping validity. Store all conflicts, never overwrite a prior claim. Resolution requires a new authoritative reference and preserves both claims.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test scratchpad_service --test evidence_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test context_snapshot_persistence --test evidence_persistence \
             --test scratchpad_persistence
git add migrations/0044_context_snapshots_scratchpads_and_evidence.sql crates tests
git commit -m "feat(context): persist snapshots scratchpads and evidence"
```

---

### Task 9: Implement deterministic context planning, allocation, rendering and snapshot capture

**Files:**
- Create: `crates/vestrace-application/src/context/planner.rs`
- Create: `crates/vestrace-application/src/context/allocator.rs`
- Create: `crates/vestrace-application/src/context/renderer.rs`
- Create: `crates/vestrace-application/src/context/snapshot_service.rs`
- Create: `crates/vestrace-application/tests/context_allocator.rs`
- Create: `crates/vestrace-application/tests/context_snapshot.rs`
- Create: `tests/context_budget_determinism.rs`

**Interfaces:**
- Produces `ContextPreflightPlanner`, `ContextAllocationService`, `CanonicalContextRenderer` and `ContextSnapshotService`.
- Consumes identity, v0.1 memory retrieval, H1/H5 Run/plan state, scratchpad, evidence, Artifact excerpts, H4 Tool views, H2 policy and token estimator ports.

- [ ] **Step 1: Write mandatory-section failure tests**

Create a model budget too small for runtime policy + identity + confirmed constraint + plan step + output contract. Build must return `mandatory_context_too_large`, create no model request and persist an unsuccessful context-build record with estimates/causes.

- [ ] **Step 2: Write deterministic allocation tests**

Randomize source retrieval order while holding source IDs/hashes fixed. Included sources, compression levels, omissions, token accounting, rendered bytes and snapshot hash must remain identical.

- [ ] **Step 3: Implement context preflight**

Preflight collects source descriptors and conservative size/classification/modality estimates without rendering full optional content. It produces `ContextPlanSummary` for H3 routing: required modalities, maximum classification, conservative input estimate, mandatory estimate and available compression range.

- [ ] **Step 4: Implement fail-closed source selection**

For every source:

```text
resolve exact revision/reference
→ current lifecycle/freshness check
→ read authorization
→ provider-transfer eligibility for selected model/provider class
→ redaction obligation
→ trust annotation
→ include candidate or durable omission reason
```

Unresolved memory/evidence conflicts are omitted or explicitly rendered as conflicts according to policy; they are never rendered as settled fact.

- [ ] **Step 5: Implement deterministic token allocation**

Reserve output/protocol/safety first. Fit mandatory sections using their allowed compression ladder. Allocate minimums to Preferred/Optional by priority, then fill toward target and maximum in stable rounds. Count final rendered messages again; if over budget, deterministically step down the lowest-priority eligible section until fit or fail.

- [ ] **Step 6: Implement canonical rendering**

Render runtime/identity instructions separately from user objective and untrusted data. External text is delimited, labeled and escaped. Preserve source IDs in section metadata, not inside opaque prose. Produce H3 `ModelMessage` values plus one canonical envelope hash.

- [ ] **Step 7: Implement capture modes**

Full stores the exact rendered canonical messages as a ContextCapture Artifact blob. Redacted applies the recorded redaction pipeline before storage. StructuredOnly stores bounded section structures without rendered prose. MetadataOnly stores no content bytes. All modes persist the same source/omission/token metadata.

- [ ] **Step 8: Run and commit**

```bash
cargo test -p vestrace-application --test context_allocator --test context_snapshot
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test context_budget_determinism
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application tests
git commit -m "feat(context): assemble deterministic model context"
```

---

### Task 10: Integrate H6 context with H3 routing, invocation and recontextualization

**Files:**
- Create: `crates/vestrace-application/src/context/model_integration.rs`
- Modify: `crates/vestrace-application/src/model_runtime/orchestrator.rs`
- Modify: `crates/vestrace-application/src/model_runtime/fallback.rs`
- Modify: `crates/vestrace-domain/src/models/runtime.rs`
- Create: `crates/vestrace-application/tests/context_model_integration.rs`

**Interfaces:**
- Produces `ContextAwareModelExecutionService` and `ContextRebuildRequest`.
- Makes `ModelInvocationRequest.context_snapshot_reference` mandatory for Harness model work while preserving v0.1 external execution compatibility where explicitly configured.

- [ ] **Step 1: Write two-phase routing tests**

```text
context preflight
→ H3 route model using conservative estimate/modalities/classification
→ build exact envelope for selected model revision
→ H2 authorize exact provider transfer manifest
→ create ModelExecution request with ContextSnapshot reference
→ dispatch through H3
```

Assert no provider attempt begins when snapshot persistence or transfer authorization fails.

- [ ] **Step 2: Implement exact snapshot binding**

The ModelExecution stores snapshot ID, source-manifest hash and rendered hash. Before dispatch, H3 reloads snapshot metadata and compares the request messages hash. A mismatch is `context_snapshot_mismatch` and never reaches an adapter.

- [ ] **Step 3: Implement context-size incompatibility before dispatch**

If exact build does not fit the routed model but another already eligible model has sufficient limit, return a routing exclusion and select the larger model before creating a provider attempt. Every selected model receives its own new ContextSnapshot because tokenization/limits may differ.

- [ ] **Step 4: Implement provider `ContextTooLarge` recovery**

Only when H3 knows completion did not occur, create `ContextRebuildRequest` with a lower input ceiling or larger selected model. Preserve source pool and omission history, create a new snapshot and new provider attempt under the same logical ModelExecution. Never mutate the old snapshot.

- [ ] **Step 5: Prove provider/Rig independence**

Context domain/application crates contain no Rig or provider request types. Native and Rig provider adapters receive the same canonical H3 messages and snapshot reference.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test context_model_integration
cargo test -p vestrace-application --test model_runtime_orchestrator --test model_fallback
git add crates/vestrace-domain crates/vestrace-application
git commit -m "feat(model-runtime): bind invocations to context snapshots"
```

---

### Task 11: Integrate H4 sandbox mounts/outputs and H5 handoff candidates

**Files:**
- Create: `crates/vestrace-domain/src/artifact/mount.rs`
- Create: `crates/vestrace-application/src/artifact/mounts.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/mount_repository.rs`
- Create: `crates/vestrace-application/tests/artifact_mounts.rs`
- Create: `tests/sandbox_artifact_mount.rs`
- Create: `tests/sandbox_output_ingestion.rs`
- Create: `tests/remote_artifact_ingestion.rs`
- Modify: `crates/vestrace-application/src/sandbox/service.rs`
- Modify: `crates/vestrace-application/src/delegation/handoff_service.rs`

**Interfaces:**
- Produces `ArtifactMountService`, H4 `ArtifactMountBrokerPort` adapter and candidate-source adapters for H4/H5 records.

- [ ] **Step 1: Write exact mount-grant tests**

A grant for Artifact revision A cannot mount revision B, another sandbox, another Run/step, a deleted revision or a path outside the declared mount root. Concurrent consumption of a one-use grant yields one staging operation.

- [ ] **Step 2: Implement mount order**

```text
load Available exact revision
→ H2 artifact.mount authorization
→ issue/consume mount grant
→ stream blob to H4 broker-owned staging
→ re-hash staged bytes
→ create read-only SandboxInputMount
→ record mount receipt
```

The sandbox receives no blob key/store credential. Cleanup removes broker staging without deleting the Artifact.

- [ ] **Step 3: Implement H4 output-candidate ingestion**

Read descriptor/hash/size from H4 candidate port, open a bounded stream, verify the candidate hash while staging and create a Working Artifact in Quarantine. A mismatch rejects ingestion and preserves the original H4 candidate/evidence.

- [ ] **Step 4: Implement H5 handoff candidate ingestion**

Internal/remote handoff references resolve through H5-owned candidate ports. Remote text, JSON and bytes all become quarantined bytes with `RemoteHandoff` provenance. Remote success or accepted handoff does not bypass Artifact inspection.

- [ ] **Step 5: Preserve ownership boundaries**

H4 remains owner of sandbox execution history; H5 remains owner of delegation/handoff history; H6 stores only source references and Artifact provenance. No table is copied wholesale across subsystems.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test artifact_mounts
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test sandbox_artifact_mount --test sandbox_output_ingestion \
             --test remote_artifact_ingestion
git add crates tests
git commit -m "feat(artifact): integrate sandbox and handoff candidates"
```

---

### Task 12: Implement restricted external URL and remote-part ingestion

**Files:**
- Create: `crates/vestrace-application/src/artifact/external.rs`
- Create: `crates/vestrace-artifact-fetch-http/Cargo.toml`
- Create: `crates/vestrace-artifact-fetch-http/src/lib.rs`
- Create: `crates/vestrace-artifact-fetch-http/src/fetcher.rs`
- Create: `crates/vestrace-artifact-fetch-http/src/config.rs`
- Create: `crates/vestrace-artifact-fetch-http/src/dns.rs`
- Create: `crates/vestrace-artifact-fetch-http/src/redirect.rs`
- Create: `crates/vestrace-artifact-fetch-http/src/auth.rs`
- Create: `crates/vestrace-artifact-fetch-http/src/error.rs`
- Create: `crates/vestrace-artifact-test-support/src/http_server.rs`
- Create: `tests/external_url_security.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces `ExternalArtifactIngestionService`, `RestrictedHttpArtifactFetcher` and `NoExternalArtifactAuth`.

- [ ] **Step 1: Write SSRF and redirect tests**

Reject URL userinfo, fragments, non-HTTP schemes, IP literals, localhost, private/link-local/multicast/reserved ranges, metadata service addresses, DNS answers changing to a forbidden range, cross-origin redirects and redirects exceeding zero-by-default policy.

- [ ] **Step 2: Implement request policy**

Use H4 egress policy/address-classification library. Resolve DNS in the fetcher, validate every address, pin one approved address for the connection and preserve the original hostname for TLS/Host verification. Do not use environment proxy variables, ambient cookie stores or caller-supplied authorization headers.

- [ ] **Step 3: Implement bounded streaming**

Apply connect/total timeout, response-header limit, content-length precheck, streaming maximum bytes, accepted media policy and decompression ratio limits. Hash while streaming directly into H6 ingestion; never follow an HTML/meta refresh.

- [ ] **Step 4: Implement remote-part normalization**

```rust
pub enum ExternalArtifactPart {
    Text { text: String, media_type: String },
    Json { value: serde_json::Value },
    Candidate { reference: ArtifactCandidateReference },
    Url { url: String },
}
```

Text/JSON are serialized deterministically and quarantined. Candidate uses Task 11. URL uses the restricted fetcher. A URL requiring authentication returns `AuthenticationRequired` until H8 supplies a request-scoped auth decorator.

- [ ] **Step 5: Ensure no fetch-on-replay**

Persist URL hash, policy decision, resolved/approved destination metadata and resulting Artifact reference. Replay reads those records and never refetches.

- [ ] **Step 6: Run and commit**

```bash
cargo test --test external_url_security
cargo clippy -p vestrace-artifact-fetch-http --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/vestrace-artifact-fetch-http \
  crates/vestrace-artifact-test-support crates/vestrace-application tests/external_url_security.rs
git commit -m "feat(artifact): securely ingest external content"
```

---

### Task 13: Implement export grants, retention, legal hold, purge and blob GC

**Files:**
- Create: `migrations/0045_artifact_mounts_exports_retention_and_purge.sql`
- Create: `crates/vestrace-domain/src/artifact/export.rs`
- Create: `crates/vestrace-domain/src/artifact/retention.rs`
- Create: `crates/vestrace-application/src/artifact/exports.rs`
- Create: `crates/vestrace-application/src/artifact/retention.rs`
- Create: `crates/vestrace-application/src/artifact/purge.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/export_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/artifact/retention_repository.rs`
- Create: `crates/vestrace-application/tests/artifact_exports.rs`
- Create: `crates/vestrace-application/tests/artifact_retention.rs`
- Create: `tests/artifact_export_persistence.rs`
- Create: `tests/artifact_purge_restart.rs`
- Create: `scripts/verify-artifact-purge.sh`

**Interfaces:**
- Creates `artifact_mount_grants`, `artifact_mount_receipts`, `artifact_export_grants`, `artifact_export_receipts`, `artifact_retention_policy_revisions`, `artifact_retention_assignments`, `artifact_legal_holds`, `artifact_purge_requests`, `artifact_purge_events`, `artifact_purge_targets` and GC leases.
- Produces `ArtifactExportService`, `ArtifactRetentionService`, `ArtifactPurgeService` and `ArtifactGarbageCollector`.

- [ ] **Step 1: Write export authority tests**

Read permission without `artifact.export` fails. Grant binds exact revision/principal/purpose/Run, expiry and use count. Deleted/Quarantined/PurgePending revisions cannot export. Streamed bytes are re-hashed and receipt stores byte count/hash without exposing backend details.

- [ ] **Step 2: Create migration `0045`**

Grant consumption is atomic. Legal holds are append-only revisions with release records. Purge targets enumerate Artifact revisions, representations, chunks, embeddings, capture blobs, export copies and physical blob keys before deletion starts.

- [ ] **Step 3: Implement logical delete and retention expiry**

Logical delete removes availability for new context/mount/export but preserves references/history. Retention worker uses policy revision fixed on each assignment and creates a protected purge request; it does not silently hard-delete.

- [ ] **Step 4: Implement legal-hold enforcement**

Purge checks Artifact, owner, Run, workspace and classification holds. A hold blocks deletion and records `BlockedByLegalHold`. Approval cannot bypass a valid hold.

- [ ] **Step 5: Implement restart-safe purge order**

```text
freeze new reads/mounts/exports
→ enumerate immutable target set
→ delete derived indexes/chunks/embeddings/cache
→ purge context capture bytes containing source content
→ delete representation/export/temp blobs
→ delete source blob when no live references remain
→ inspect every blob key
→ mark content-free tombstones and Completed
```

Each target has independent status/idempotency. Ambiguous deletion becomes Unknown and uses store `inspect` before retry.

- [ ] **Step 6: Implement garbage collection**

GC only considers promoted blobs with no live `artifact_blob_references`, no active staging/ingestion/purge claim and age above safety horizon. It leases targets and reconciles deletion. Cross-workspace namespace isolation remains intact.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test artifact_exports --test artifact_retention
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test artifact_export_persistence --test artifact_purge_restart
bash scripts/verify-artifact-purge.sh
git add migrations/0045_artifact_mounts_exports_retention_and_purge.sql \
  crates tests scripts/verify-artifact-purge.sh
git commit -m "feat(artifact): add export retention and hard purge"
```

---

### Task 14: Consolidate evaluated Run outcomes through v0.1 memory write policy

**Files:**
- Create: `migrations/0046_run_consolidation_and_memory_candidate_bindings.sql`
- Create: `crates/vestrace-domain/src/consolidation/mod.rs`
- Create: `crates/vestrace-domain/src/consolidation/candidate.rs`
- Create: `crates/vestrace-domain/src/consolidation/report.rs`
- Create: `crates/vestrace-application/src/consolidation/service.rs`
- Create: `crates/vestrace-application/src/consolidation/worker.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/consolidation/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/consolidation/repository.rs`
- Create: `crates/vestrace-application/tests/consolidation_service.rs`
- Create: `tests/consolidation_persistence.rs`

**Interfaces:**
- Creates `run_consolidation_requests`, `run_consolidation_reports`, `consolidation_included_sources`, `consolidation_excluded_sources`, `memory_candidate_bindings` and consolidation events.
- Produces `RunConsolidationService` and `ConsolidationWorkHandler`.

- [ ] **Step 1: Write eligible-source tests**

Include verified terminal outcome, accepted handoff, confirmed decision, verified procedure and evaluated evidence. Exclude Unknown/failed tool observations, rejected handoffs, unresolved contradictions, model-only confidence claims, raw transcripts and unconfirmed scratchpad assumptions.

- [ ] **Step 2: Create migration `0046`**

Reports are append-only and reference source hashes/revisions plus returned v0.1 Memory IDs. Do not copy Memory content/revision rows into H6 tables.

- [ ] **Step 3: Implement deterministic source manifest**

Build a sorted manifest of eligible exact references with evaluation/trust/classification metadata. The manifest hash is the consolidation idempotency key for one Run and policy revision.

- [ ] **Step 4: Implement candidate extraction boundary**

Deterministic templates create obvious Outcome/Task/Decision/Procedure drafts. Optional H3 structured extraction may propose additional drafts from the bounded manifest. Every draft is schema/type checked, source-linked, classification checked and conflict-checked through v0.1 ports.

- [ ] **Step 5: Submit only Candidate memories**

Call the existing memory command with derivation `consolidation`, exact Run/evidence sources and current memory write-policy revision. Store returned Memory IDs. Do not set Active status from H6.

- [ ] **Step 6: Handle restart after memory submission**

Use the manifest idempotency key in the v0.1 command. On restart, query existing candidate bindings/result before resubmitting. A conflicting response becomes explicit reconciliation, not duplicate memory.

- [ ] **Step 7: Run and commit**

```bash
cargo test -p vestrace-application --test consolidation_service
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test consolidation_persistence
git add migrations/0046_run_consolidation_and_memory_candidate_bindings.sql crates tests
git commit -m "feat(memory): consolidate verified runs into candidates"
```

---

### Task 15: Integrate H6 work, checkpoint V4, RLS, boundary gates and acceptance

**Files:**
- Create: `migrations/0047_context_artifact_rls_indexes_and_run_bindings.sql`
- Create: `crates/vestrace-application/src/artifact/worker.rs`
- Modify: `crates/vestrace-domain/src/run/event.rs`
- Modify: `crates/vestrace-domain/src/run/work.rs`
- Modify: `crates/vestrace-domain/src/run/checkpoint.rs`
- Modify: `crates/vestrace-domain/src/run/mod.rs`
- Modify: `crates/vestrace-application/src/run/worker.rs`
- Create: `scripts/verify-context-boundary.sh`
- Create: `scripts/verify-artifact-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `tests/context_artifact_rls.rs`
- Create: `tests/h6_acceptance.rs`
- Modify: schema snapshots where stored

**Interfaces:**
- Produces H6 work handlers, `RunCheckpointV4` fields, forced RLS, consistency/index gates and the H6 vertical acceptance scenario.

- [ ] **Step 1: Add H6 work kinds**

```text
StageArtifact
InspectArtifact
BuildRepresentation
IndexArtifactChunks
IngestArtifactCandidate
FetchExternalArtifact
ReconcileArtifactStore
PrepareArtifactMount
CleanupArtifactMount
PurgeArtifact
GarbageCollectArtifactBlob
BuildContextSnapshot
ConsolidateRunMemory
```

Work payloads contain stable IDs, expected logical revisions and deadlines, never bytes, prompts, external URLs in cleartext, store credentials or host paths.

- [ ] **Step 2: Add canonical Run events**

```rust
ArtifactAttached { artifact_revision_id: ArtifactRevisionId },
ArtifactDeliverablePublished { artifact_revision_id: ArtifactRevisionId },
ContextSnapshotBound { step_id: RunStepId, snapshot_id: ContextSnapshotId },
RunConsolidationRequested { report_id: RunConsolidationReportId },
RunConsolidationCompleted { report_id: RunConsolidationReportId },
```

Only events that change Run references/status increment RunVersion. Ingestion progress, chunking, retrieval scoring, capture writes and purge target progress stay in H6 journals.

- [ ] **Step 3: Define `RunCheckpointV4` additions**

```rust
pub struct ContextArtifactCheckpoint {
    pub latest_context_snapshot_by_step: Vec<(RunStepId, ContextSnapshotId)>,
    pub attached_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub active_ingestion_session_ids: Vec<ArtifactIngestionSessionId>,
    pub pending_representation_ids: Vec<ArtifactRepresentationId>,
    pub active_mount_grant_ids: Vec<ArtifactMountGrantId>,
    pub pending_consolidation_report_id: Option<RunConsolidationReportId>,
}
```

Older checkpoint payloads remain readable. H6 tables remain authoritative and checkpoint references accelerate resume.

- [ ] **Step 4: Create migration `0047`**

Force RLS on all H6 workspace tables. Add same-workspace Run/step/Artifact/source checks, append-only guards, active ingestion/inspection/representation indexes, FTS/vector indexes, expiry/retention/purge/GC indexes and constraints preventing captures/mounts/exports from referencing unavailable revisions.

- [ ] **Step 5: Add boundary scripts**

`verify-context-boundary.sh` rejects direct SQL/memory-table access, provider/Rig types, hidden reasoning fields and context messages without snapshot binding. `verify-artifact-boundary.sh` rejects filesystem/S3/Reqwest types in domain/application APIs, storage paths in public/model DTOs, direct H4/H5 table copying and Available states created without inspection.

- [ ] **Step 6: Write mandatory H6 acceptance scenario**

Use a generated large Markdown document, deterministic memory fixtures, loopback URL server and H4/H5 fixtures. Assert:

1. upload streams under a maximum and creates one quarantined revision with correct hash;
2. restart after blob promotion links the same blob without a duplicate revision;
3. inspection detects media type, performs secret/malware/archive policy and marks the safe document Available;
4. extracted text and chunks are deterministic and relevant excerpts are retrieved under workspace/policy filters;
5. context preflight routes to an eligible model limit, mandatory sections remain intact and lower-priority sources are deterministically compressed/omitted;
6. v0.1 active Memory revisions are referenced rather than copied;
7. untrusted evidence is labeled as data and cannot alter runtime instructions;
8. unresolved contradictory claims are not rendered as an established fact;
9. the exact rendered messages match the persisted ContextSnapshot hash and token accounting;
10. a provider request cannot dispatch with altered messages or missing snapshot reference;
11. a read-only exact-revision mount reaches an H4 sandbox without exposing a blob key or host path;
12. sandbox output becomes a new quarantined Working Artifact and only a declared validated output becomes a Deliverable;
13. a remote text/JSON candidate follows the same quarantine path;
14. a remote URL targeting private/metadata addresses is rejected before connection;
15. export requires a separate exact grant and produces a matching receipt;
16. hard purge removes representations, chunks, capture bytes, export copies and unreferenced blobs while retaining content-free tombstones;
17. legal hold blocks purge;
18. Run consolidation excludes Unknown/rejected/unverified sources, submits idempotent Candidate memories and defers activation to v0.1 write policy;
19. restart at every ingestion, representation, snapshot, mount, purge and consolidation fault point creates no duplicate blob, Artifact revision, mount use, purge or Memory candidate;
20. replay performs no blob write, processor, external fetch, sandbox mount, export or memory submission.

- [ ] **Step 7: Add required CI jobs**

```text
artifact-domain-and-postgres
artifact-store-local-conformance
artifact-store-s3-loopback-conformance
artifact-inspection-and-processing
context-allocation-and-snapshots
sandbox-and-remote-artifact-integration
artifact-retention-and-purge
run-consolidation
context-artifact-boundaries
h6-acceptance
```

- [ ] **Step 8: Run all H6 gates and commit**

```bash
bash scripts/verify-context-boundary.sh
bash scripts/verify-artifact-boundary.sh
bash scripts/verify-artifact-purge.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h6_acceptance --test context_artifact_rls \
             --test artifact_ingestion_restart --test artifact_purge_restart
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

git add migrations/0047_context_artifact_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml crates scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(context): add H6 acceptance and safety gates"
```

---

## H6 completion definition

H6 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
bytes or source candidate
→ staged streaming hash
→ promoted content-addressed blob
→ Quarantined ArtifactRevision
→ inspection
→ Available immutable revision
→ representation/chunks/excerpts
→ policy-filtered Context sources
→ deterministic token allocation
→ ContextSnapshot
→ H3 model invocation
→ verified outputs/evidence
→ sandbox/remote output ingestion
→ deliverable/export
→ retention/purge
→ governed memory candidates
```

The completed implementation must satisfy all of these statements:

1. v0.1 Memory and retrieval remain authoritative and are consumed only through application ports.
2. Artifact metadata and bytes have separate replaceable storage contracts.
3. Every Artifact revision is immutable and content-addressed inside a workspace/deployment namespace.
4. Staged/promoted blob crash windows do not duplicate revisions or silently lose bytes.
5. Every new byte source begins in Quarantine and required inspection cannot be bypassed.
6. Filenames, MIME types, archives, parser output, tool output and remote output are untrusted.
7. Representation provenance identifies the exact source and generator revision.
8. Artifact excerpts are filtered by workspace, lifecycle, classification and policy before retrieval.
9. Mandatory context sections are never silently omitted or demoted to unsafe representations.
10. Context allocation and rendering are deterministic for fixed inputs and revisions.
11. Every Harness model invocation binds exact messages to an immutable ContextSnapshot.
12. Full/redacted context capture bytes can be purged without deleting immutable snapshot metadata.
13. Hidden chain-of-thought is neither requested nor stored; explicit scratchpad state is typed and bounded.
14. Untrusted evidence cannot become runtime instruction or authoritative memory by self-assertion.
15. Contradictions remain explicit until resolved through a new supported record.
16. H4 mounts are exact-revision, read-only, short-lived and reveal no backend credential/path.
17. H4/H5 output candidates do not become available Artifacts without H6 ingestion and inspection.
18. External URL ingestion rejects SSRF, DNS rebinding, ambient credentials and unbounded bodies.
19. Export permission is distinct from read permission and grants are exact/short-lived.
20. Logical delete, retention, legal hold, hard purge and physical GC remain separate durable operations.
21. Hard purge removes every configured content-bearing derivative and verifies blob deletion.
22. Run consolidation accepts only evaluated sources and submits Candidate memories through v0.1 write policy.
23. Restart at every critical H6 window does not duplicate bytes, revisions, mounts, exports, purge actions or memory candidates.
24. Replay performs no external or content-mutating H6 action.
25. H7 can expose conversations/uploads/downloads/human review without changing H6 domain contracts.
26. H8 can add request-scoped authenticated fetch/store/processor credentials without putting secrets in H6 persistence.
27. H9 can select Artifact processors and memory profiles through immutable package revisions.
28. H9A can map A2A artifact parts to H6 external-part contracts without leaking `a2a-rs` types.
29. H10 can add richer evidence/evaluation scoring without rewriting accepted evidence or Artifact history.
30. H6 tests require no public provider, public storage, public URL or permanent credential.

## Explicit non-goals

H6 does not implement:

- HTTP upload/download UI or conversation attachments;
- user-facing human review channels;
- OAuth/API-key acquisition or Credential Broker behavior;
- A2A transport types or Agent Card handling;
- unrestricted external URL fetching;
- automatic execution of document instructions;
- arbitrary in-process third-party parsers;
- production browser automation;
- global cross-workspace deduplication;
- mutable Artifact revisions;
- hidden chain-of-thought retention;
- automatic activation of memory outside v0.1 policy;
- semantic contradiction resolution owned by H10;
- permanent public download URLs;
- production replay of processors, exports or external fetches.

These remain assigned to H7–H10/H9A or are intentionally excluded.

## Documentation-only boundary

Creating this document does not authorize implementation. During the current documentation phase, do not:

- create `feat/h6-context-artifact-runtime`;
- change Cargo dependencies or workspace members;
- create migrations `0042`–`0047`;
- create local/S3 Artifact stores or buckets;
- stage, scan, process, mount, export or purge any real file;
- modify memory, model, tool, sandbox or delegation code;
- change CI;
- execute PostgreSQL, storage, processor, network or acceptance tests.
