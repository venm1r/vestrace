# Vestrace H6 Context and Artifact Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start storage services, run processors or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement a memory-first, token-aware and policy-governed Context Runtime plus an immutable Artifact Runtime with streaming ingestion, quarantine, representations, provenance, secure mounts and exports, external-artifact handling, retention, hard purge and Run consolidation into governed memory candidates.

**Architecture:** H6 extends the v0.1 Memory Plane rather than duplicating memories, retrieval or write policy. Existing v0.1 retrieval/context-pack services remain authoritative for long-term memory selection; H6 combines their exact revision references with identity, Run state, plan state, verified evidence, artifact excerpts, tool views and output contracts into a model-specific `ModelContextEnvelope` and immutable `ContextSnapshot`. Artifact metadata/lifecycle are authoritative in PostgreSQL while bytes live behind `ArtifactBlobStorePort`; all bytes enter through staged streaming, quarantine and inspection before they can be read, mounted, exported or included in context. H4 sandbox outputs and H5 remote outputs remain source candidates until H6 materializes validated Artifact revisions.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H5 Rust workspace; Rust Edition 2024; Tokio; Serde; Schemars; SQLx; PostgreSQL 17 with pgvector and FTS; SHA-256 streaming hashes; bytes/futures streams; JSON Schema validation; local content-addressed storage; S3-compatible storage adapter; Reqwest for a restricted external-artifact fetcher; H4 sandbox processing; tracing; proptest; deterministic loopback fixtures.

## Global Constraints

- Complete all five v0.1 plans and H1–H5 before implementing H6.
- Harness design sections `16. Context system and memory` and `21. Artifact system` are normative.
- v0.1 `Memory`, `MemoryRevision`, retrieval, context-pack, provenance and memory write-policy records remain authoritative and are never replaced by H6 tables.
- H6 reads and writes memory only through Vestrace application ports. It never updates v0.1 memory/retrieval tables directly.
- PostgreSQL is authoritative for Artifact identities/revisions, ingestion sessions, inspection results, representations, provenance, context snapshots, scratchpads, evidence, mounts, exports, retention, purge and consolidation records.
- Artifact, representation and context-capture bytes live only behind `ArtifactBlobStorePort`; database rows contain bounded metadata and opaque blob keys.
- Vestrace owns every domain type, application port, persisted schema, lifecycle transition, hash format and public DTO.
- S3 SDK, Reqwest, filesystem and parser-specific types may not appear in domain/application signatures or PostgreSQL schemas.
- One logical `Artifact` may have multiple immutable `ArtifactRevision` records. A finalized revision never changes bytes, media type, content hash, classification or provenance.
- A content hash identifies bytes, not permission. Knowing a hash never grants read, mount, export or cross-workspace access.
- Cross-workspace physical deduplication is disabled by default. Blob identity includes a storage namespace to avoid a cross-tenant existence oracle.
- Store paths, bucket keys, presigned URLs, staging handles and host paths are never model-visible.
- Every ingestion uses streaming byte limits and SHA-256. Large inputs are not buffered entirely in memory.
- A staged/promoted blob is not an available Artifact revision until PostgreSQL metadata finalization succeeds.
- All new bytes enter `Quarantined`. They cannot enter context, mounts, export or consolidation until required inspections pass.
- MIME declarations, filenames, extensions, archive manifests, parser output and remote metadata are untrusted.
- Secret findings store rule IDs, locations and irreversible fingerprints only; secret values never enter findings/logs.
- Executable/active content, archive expansion, parser selection and high-risk media fail closed according to policy.
- Derived representations/chunks are rebuildable, but their generator revision and source revision remain immutable provenance.
- Context assembly never stores or requests hidden chain-of-thought. Reproducibility uses typed Run state, explicit scratchpad fields, references, hashes and durable outcomes.
- Runtime safety policy, identity instructions, confirmed constraints, selected plan step and output contract are mandatory sections and are never silently truncated.
- Context allocation is deterministic for the same source revisions, model limit, estimator revision, policy revision and budget.
- Every Harness model invocation uses a persisted `ContextSnapshot`; messages/tool schemas/output contract must hash to that snapshot before dispatch.
- Reading a source, including it, transferring it to a provider and capturing it durably are distinct policy decisions.
- Context snapshots store exact sources, revisions, rendered hash, token accounting, policy/estimator revisions, omissions and redactions.
- Full/redacted capture content is stored as a purgeable `ContextCapture` Artifact revision; immutable snapshot metadata can outlive capture bytes.
- Scratchpad entries are typed summaries. Free-form reasoning traces, provider reasoning payloads and token-by-token deliberation are rejected.
- External evidence remains distinct from active memory and verified Run state through trust annotations.
- `Unknown`, rejected, unverified or policy-denied outcomes never become memory candidates.
- Consolidation submits only Candidate memories through existing v0.1 write policy; H6 never directly activates memory.
- H4 Tool/Sandbox output candidates and H5 handoff output candidates are source records, not available Artifacts.
- External URL ingestion supports policy-approved `http`/`https`, no URL credentials, no ambient cookies and no authenticated fetch before H8.
- External fetch rejects loopback, link-local, private, multicast, metadata-service and reserved destinations and prevents DNS rebinding.
- Artifact mounts are read-only, exact-revision-bound and short-lived. A mount grant is not a storage credential.
- Export uses an exact revision and operation-bound grant. Read permission never implies export permission.
- Logical delete, retention expiry, legal hold, hard purge and physical garbage collection are distinct durable states.
- Hard purge removes blobs, representations, chunks, embeddings, cached excerpts, context-capture bytes and temporary exports while retaining a content-free tombstone/audit reference.
- Replay and diagnostics never refetch URLs, rerun processors, remount sandboxes, export or write memory.
- Existing migrations `0014`–`0041` are never edited. H6 migrations are `0042`–`0047` and are created once.
- CI uses generated files, loopback fixtures and deterministic processors; no public model/storage/URL/A2A service or permanent credential is required.
- Future implementation branch: `feat/h6-context-artifact-runtime`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  artifact/{mod,artifact,revision,blob,ingestion,inspection,representation,provenance,mount,export,retention}.rs
  context/{mod,source,policy,envelope,snapshot,token,scratchpad}.rs
  evidence/{mod,item,claim,contradiction}.rs
  consolidation/{mod,candidate,report}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  artifact/{mod,ports,commands,ingestion,inspection,representation,excerpts,mounts,exports,external,retention,purge,worker}.rs
  context/{mod,ports,planner,allocator,renderer,snapshot_service,model_integration,scratchpad_service}.rs
  evidence/{mod,service,contradictions}.rs
  consolidation/{mod,ports,service,worker}.rs

crates/vestrace-artifact-store-local/src/{lib,store,layout,staging,range,security,reconcile}.rs
crates/vestrace-artifact-store-s3/src/{lib,store,config,multipart,range,error,reconcile}.rs
crates/vestrace-artifact-inspector/src/{lib,media,archive,secrets,malware,classification}.rs
crates/vestrace-artifact-processors/src/{lib,text,json,archive_manifest,sandboxed,chunking}.rs
crates/vestrace-artifact-fetch-http/src/{lib,fetcher,config,dns,redirect,auth,error}.rs
crates/vestrace-artifact-test-support/src/{lib,fakes,streams,store_conformance,http_server,processors,fixtures}.rs

crates/vestrace-infrastructure/src/postgres/
  artifact/{mod,repository,blob_repository,ingestion_repository,inspection_repository,representation_repository,provenance_repository,mount_repository,export_repository,retention_repository}.rs
  context/{mod,snapshot_repository,scratchpad_repository}.rs
  evidence/{mod,repository,contradiction_repository}.rs
  consolidation/{mod,repository}.rs

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

### Supporting values and errors

```rust
pub struct ArtifactByteRange {
    pub start: u64,
    pub end_exclusive: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct OpaqueArtifactStagingHandle(String);

pub enum ArtifactStoreErrorKind {
    InvalidRequest,
    SizeLimitExceeded,
    HashMismatch,
    NotFound,
    PermissionDenied,
    Unavailable,
    Timeout,
    Cancelled,
    UnknownCompletion,
}

pub struct ArtifactStoreError {
    pub kind: ArtifactStoreErrorKind,
    pub safe_message: String,
    pub completion_may_have_occurred: bool,
}

pub struct ArtifactBlobDeletionObservation {
    pub key: ArtifactBlobKey,
    pub absent: bool,
}

pub struct StagedArtifactBlobMetadata {
    pub staging_handle: OpaqueArtifactStagingHandle,
    pub content_hash: ArtifactContentHash,
    pub byte_size: u64,
}

pub enum ArtifactRevisionSource {
    UserUpload { upload_reference_hash: [u8; 32] },
    ToolOutput { tool_invocation_id: ToolInvocationId, candidate_id: String },
    SandboxOutput { sandbox_session_id: SandboxSessionId, candidate_id: SandboxOutputCandidateId },
    Handoff { handoff_id: HandoffArtifactId, candidate_reference: RunReference },
    ExternalUrl { url_hash: [u8; 32] },
    Representation { representation_id: ArtifactRepresentationId },
    ContextCapture { context_snapshot_id: ContextSnapshotId },
}

pub enum ArtifactInspectionRecommendation {
    MakeAvailable,
    KeepQuarantined,
    RequireManualReview,
    Reject,
}

pub struct ArtifactProcessorRevisionRef {
    pub stable_id: String,
    pub revision: String,
    pub integrity_digest: [u8; 32],
}

pub enum ContextRenderableValue {
    Text(String),
    Json(serde_json::Value),
    References(Vec<RunReference>),
}

pub struct EvidenceFreshness {
    pub observed_at: Timestamp,
    pub valid_until: Option<Timestamp>,
}

pub enum EvidenceContradictionKind {
    MutuallyExclusiveValue,
    OverlappingValidity,
}

pub enum EvidenceContradictionStatus {
    Unresolved,
    Resolved,
    Dismissed,
}

pub enum ArtifactGrantStatus {
    Issued,
    Consumed,
    Expired,
    Revoked,
}

pub enum RunConsolidationStatus {
    Prepared,
    Extracting,
    CheckingConflicts,
    Submitting,
    Completed,
    Partial,
    Failed,
    Unknown,
}

pub enum ConsolidationExclusionReason {
    Unverified,
    UnknownOutcome,
    Rejected,
    PolicyDenied,
    ConflictUnresolved,
    RawTranscript,
    TemporaryState,
    Duplicate,
}

pub struct ConsolidationExclusion {
    pub reference: RunReference,
    pub reason: ConsolidationExclusionReason,
}
```

`MemoryScopeRef` and `MemoryConflictReference` are reused from the v0.1 memory application contracts and are not redefined by H6.

### Artifact identity and immutable revisions

```rust
pub enum ArtifactKind {
    Input,
    Working,
    Deliverable,
    Evidence,
    Representation,
    ContextCapture,
    ExportCopy,
}

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
- media type is normalized lowercase type/subtype without parameters;
- filenames containing `/`, `\`, NUL, absolute/parent components or more than 255 UTF-8 bytes are rejected; source adapters may explicitly derive a safe basename before construction;
- Available revision content identity and provenance are immutable;
- `ArtifactBlobKey` is never serialized into model/remote/public DTOs.

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
    async fn stage(&self, request: StageArtifactBlobRequest)
        -> Result<StagedArtifactBlob, ArtifactStoreError>;
    async fn promote(&self, staged: &StagedArtifactBlob)
        -> Result<ArtifactBlobObservation, ArtifactStoreError>;
    async fn inspect(&self, key: &ArtifactBlobKey)
        -> Result<ArtifactBlobObservation, ArtifactStoreError>;
    async fn open_range(&self, key: &ArtifactBlobKey, range: ArtifactByteRange)
        -> Result<ArtifactByteStream, ArtifactStoreError>;
    async fn delete(&self, key: &ArtifactBlobKey)
        -> Result<ArtifactBlobDeletionObservation, ArtifactStoreError>;
    async fn cleanup_staging(&self, handle: &OpaqueArtifactStagingHandle)
        -> Result<(), ArtifactStoreError>;
}
```

`stage` hashes while streaming and rejects size/hash mismatch. `promote` is idempotent for the same namespace/hash. Ambiguous results use `inspect`, never blind re-upload/delete.

### Ingestion, inspection and representations

```rust
pub enum ArtifactIngestionSource {
    UserUpload { upload_reference_hash: [u8; 32] },
    Candidate { reference: ArtifactCandidateReference },
    ExternalUrl { url_hash: [u8; 32] },
    DerivedRepresentation { representation_id: ArtifactRepresentationId },
    ContextCapture { context_snapshot_id: ContextSnapshotId },
}

pub enum ArtifactCandidateReference {
    Tool { invocation_id: ToolInvocationId, candidate_id: String },
    Sandbox { session_id: SandboxSessionId, candidate_id: SandboxOutputCandidateId },
    Handoff { handoff_id: HandoffArtifactId, candidate_reference: RunReference },
}

pub struct ArtifactCandidateDescriptor {
    pub reference: ArtifactCandidateReference,
    pub declared_media_type: Option<String>,
    pub expected_size: Option<u64>,
    pub expected_hash: Option<ArtifactContentHash>,
    pub classification: DataClassification,
}

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
    pub generator: ArtifactProcessorRevisionRef,
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
```

Every new representation output re-enters quarantine. Required inspection outage cannot produce Available. Chunks are bounded to 64 KiB and deterministic for fixed source/generator/parameters.

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
    pub generator: Option<ArtifactProcessorRevisionRef>,
    pub parameters_hash: Option<[u8; 32]>,
    pub created_at: Timestamp,
}
```

Derivation edge kinds form an acyclic graph. Contextual/audit links never redefine byte derivation.

### Context source and trust values

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

pub enum ContextRequirement { Mandatory, Preferred, Optional }
pub enum ContextCompressionLevel { Full, Compact, Atomic, ReferenceOnly }

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

Lower-trust sources cannot render as runtime/developer instructions. External content is delimited and labeled as data.

### Token accounting and context snapshot

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

    async fn count_model_input(
        &self,
        model_revision_id: ModelRevisionId,
        messages: &[ModelMessage],
        tools: &[ModelToolDefinitionView],
        output_contract: &ModelOutputContract,
    ) -> Result<TokenCount, ApplicationError>;
}
```

Fallback estimation is deliberately conservative: UTF-8 bytes plus configured message/tool/schema overhead, with `exact = false`.

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
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolDefinitionView>,
    pub output_contract: ModelOutputContract,
    pub total_input_tokens: u64,
    pub rendered_hash: [u8; 32],
    pub data_classification: DataClassification,
}

pub enum ContextCaptureMode { Full, Redacted, StructuredOnly, MetadataOnly }

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
    pub capture_artifact_revision_id: Option<ArtifactRevisionId>,
    pub created_at: Timestamp,
}

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
    pub policy_id: ContextAssemblyPolicyId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub rules: Vec<ContextSectionAllocationRule>,
    pub capture_mode: ContextCaptureMode,
    pub allow_unverified_observations: bool,
    pub maximum_external_evidence_items: u32,
    pub content_hash: [u8; 32],
}

pub struct ContextPlanSummary {
    pub required_modalities: std::collections::BTreeSet<ModelModality>,
    pub maximum_classification: DataClassification,
    pub conservative_input_tokens: u64,
    pub mandatory_tokens: u64,
    pub minimum_possible_tokens: u64,
}
```

Mandatory sections either fit at an allowed safe compression or build fails `mandatory_context_too_large`. Final count includes messages, tool schemas and output-contract overhead.

### Structured scratchpad and evidence

```rust
pub enum ScratchpadScope { Run, Step { step_id: RunStepId } }
pub enum ScratchpadEntryKind { Assumption, OpenQuestion, CandidateAction, Calculation, Risk, DecisionPoint }
pub enum ScratchpadEntryStatus { Open, Confirmed, Resolved, Rejected, Superseded }

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

pub enum EvidenceSourceRef {
    ArtifactRevision(ArtifactRevisionId),
    MemoryRevision(MemoryRevisionId),
    ToolInvocation(ToolInvocationId),
    Handoff(HandoffArtifactId),
    ExternalReference(String),
}

pub enum EvidenceTrustClass { Untrusted, Contextual, Corroborated, Authoritative }

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
    pub valid_from: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
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

Scratchpad entry payloads use kind-specific schemas and reject reasoning traces. H6 detects exact subject/predicate/value contradictions with overlapping validity; H10 may add richer evaluation later.

### Mounts, export, retention and purge

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
    pub policy_id: ArtifactRetentionPolicyId,
    pub workspace_id: WorkspaceId,
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

Mount/export grants are checked at the enforcement point and contain no blob credentials. Purge verifies absence in every configured backend before completion.

### Consolidation into v0.1 candidate memories

```rust
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

Candidates are submitted through existing v0.1 memory commands with derivation/provenance. Existing write policy alone decides later activation.

---

### Task 1: Add Artifact domain contracts

**Files:** create/modify `crates/vestrace-domain/src/id.rs`, `artifact/{mod,artifact,revision,blob,ingestion,provenance}.rs`, `lib.rs`.

**Interfaces:** adds all H6 IDs, including `ContextAssemblyPolicyId` and `ArtifactRetentionPolicyId`; produces Artifact/blob/ingestion/provenance values above.

- [ ] Write failing tests for immutable Available revision, rejected path-like filename, namespace-bound blob key, contiguous revisions and derivation cycle.
- [ ] Implement SHA-256/hex parsing, redacted debug and checked ranges/sizes.
- [ ] Implement lifecycle `Staging → Quarantined → Inspecting → Available|Rejected → Deleted/PurgePending/Purged`.
- [ ] Implement deterministic revision/provenance hashes.
- [ ] Run:

```bash
cargo test -p vestrace-domain artifact
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(artifact): add immutable artifact contracts"
```

### Task 2: Add Context, scratchpad and evidence domain contracts

**Files:** create `context/{mod,source,policy,envelope,snapshot,token,scratchpad}.rs`, `evidence/{mod,item,claim,contradiction}.rs`; modify `lib.rs`.

- [ ] Test that external evidence cannot become RuntimePolicy/Identity instructions.
- [ ] Test mandatory instruction rules reject `ReferenceOnly` as their sole representation.
- [ ] Test token-budget arithmetic and deterministic source hashes.
- [ ] Test scratchpad rejects provider reasoning fields, trace/token arrays and entries over 8 KiB; accept typed calculation/risk.
- [ ] Implement exact contradiction detection with overlapping validity.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain context evidence
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(context): add context scratchpad and evidence contracts"
```

### Task 3: Define H6 ports, commands and deterministic fixtures

**Files:** create application artifact/context/evidence/consolidation modules and `vestrace-artifact-test-support` crate.

**Interfaces:** produces Artifact repositories/store/inspection/processor/candidate/fetch/mount/export ports; identity/memory/run/context/estimator/snapshot/scratchpad/evidence ports; v0.1 `MemoryCandidatePort`; deterministic fakes and fault injection.

```rust
#[async_trait::async_trait]
pub trait ArtifactCandidateSourcePort: Send + Sync {
    async fn describe(&self, context: &RequestContext, reference: ArtifactCandidateReference)
        -> Result<ArtifactCandidateDescriptor, ApplicationError>;
    async fn open(&self, context: &RequestContext, reference: ArtifactCandidateReference, maximum_bytes: u64)
        -> Result<ArtifactByteStream, ApplicationError>;
}

#[async_trait::async_trait]
pub trait MemoryContextPort: Send + Sync {
    async fn retrieve(&self, context: &RequestContext, request: MemoryContextRequest)
        -> Result<MemoryContextSelection, ApplicationError>;
}

#[async_trait::async_trait]
pub trait MemoryCandidatePort: Send + Sync {
    async fn check_conflicts(&self, context: &RequestContext, drafts: &[MemoryCandidateDraft])
        -> Result<Vec<MemoryConflictReference>, ApplicationError>;
    async fn submit_candidates(&self, context: &RequestContext, request: SubmitMemoryCandidates)
        -> Result<Vec<MemoryId>, ApplicationError>;
}
```

- [ ] Add object-safety compile tests for every port.
- [ ] Implement H4/H5 candidate adapters without copying their tables/SDK types.
- [ ] Implement v0.1 memory adapters preserving exact MemoryRevision/provenance references.
- [ ] Add one-shot fault points after ingestion creation, stage, promote, quarantine, inspection, representation, snapshot, mount consumption, purge deletion and candidate submission.
- [ ] Run/commit.

### Task 4: Implement Local CAS and S3-compatible stores

**Files:** create `vestrace-artifact-store-local`, `vestrace-artifact-store-s3`, conformance support/tests.

- [ ] One unchanged suite covers streaming, max bytes, expected size/hash, idempotent promote/delete, range reads, missing inspect, ambiguous completion reconciliation, staging cleanup, namespace isolation and redacted errors.
- [ ] Local CAS uses descriptor-relative safe paths, exclusive staging, fsync and atomic rename; filenames/media never influence paths.
- [ ] S3 adapter uses session staging prefix and namespace/hash final key, multipart streaming hash, HEAD/range/copy/delete reconciliation; credentials remain deployment configuration.
- [ ] Mandatory CI uses a loopback S3-compatible fixture; MinIO smoke is optional.
- [ ] Run/commit:

```bash
cargo test --test artifact_store_local_conformance --test artifact_store_s3_conformance
cargo clippy -p vestrace-artifact-store-local -p vestrace-artifact-store-s3 --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates tests
git commit -m "feat(artifact-store): add local and S3 blob backends"
```

### Task 5: Persist Artifact revisions, blobs and provenance

**Files:** create migration `0042`, PostgreSQL Artifact/blob/provenance repositories, `tests/artifact_persistence.rs`.

- [ ] Test immutable revisions/edges, workspace namespace isolation, contiguous revisions, current-revision ownership and cycle rejection.
- [ ] Create `artifact_storage_namespaces`, `artifacts`, `artifact_revisions`, `artifact_blobs`, `artifact_blob_references`, `artifact_provenance_nodes`, `artifact_provenance_edges`.
- [ ] Create Artifact identity/proposed revision/provenance root atomically. Ingestion binding is created later by `0043`, not here.
- [ ] Finalization atomically links an existing promoted blob and updates current revision.
- [ ] Add bounded provenance traversal.
- [ ] Run/commit.

### Task 6: Implement restart-safe ingestion, quarantine and inspection

**Files:** create migration `0043`, domain `artifact/inspection.rs`, ingestion/inspection services, inspector crate, repositories/tests.

- [ ] Test flow:

```text
create ingestion session + Artifact draft
→ stage bytes
→ persist staged metadata
→ promote deterministic key
→ inspect final key
→ atomically link blob and mark Quarantined
→ enqueue inspection
```

- [ ] Crash after stage resumes/cleans same handle; crash after promote inspects/links same blob and never creates another revision.
- [ ] Migration creates ingestion/events, inspection reports/outcomes/findings, representation/chunk/work tables.
- [ ] Implement declared/detected MIME checks, polyglot policy, archive entry/depth/ratio/path/link/device limits.
- [ ] Implement secret findings without values and `MalwareScannerPort`; required scanner outage keeps Quarantined/manual review.
- [ ] Only required Passed/PassedWithWarnings permits Available.
- [ ] Run/commit.

### Task 7: Build representations, chunks and excerpts

**Files:** create domain `artifact/representation.rs`, processor crate, representation/excerpt services/repository/tests.

- [ ] Same source/generator/parameters yields same idempotency key/chunks; changed generator revision creates a new representation.
- [ ] Native processors handle UTF-8 text, JSON and safe archive manifests. Other parsers use digest-pinned H4 sandbox with DenyAll/read-only input.
- [ ] Flow: Available source → H2 authorization/budget → processing → native/sandbox processor → output re-enters quarantine → provenance → deterministic chunks → Available representation.
- [ ] Excerpt retrieval uses exact/FTS/optional pgvector + RRF and applies workspace/lifecycle/classification/policy before candidate generation.
- [ ] Run/commit.

### Task 8: Persist context build attempts/snapshots, scratchpads and evidence

**Files:** create migration `0044`, context/scratchpad/evidence services/repositories/tests.

- [ ] Migration creates `context_assembly_policies`, revisions, `context_build_attempts`, snapshots/sections/sources/omissions/redactions/capture references, scratchpad revisions/entries and evidence/claims/contradictions.
- [ ] Test append-only snapshots, MetadataOnly no capture, Redacted only redacted capture Artifact, Full still policy/secret checked, capture purge preserves metadata hashes/references.
- [ ] Scratchpad updates create full immutable revision with optimistic expected revision; model drafts cannot self-confirm or change actor/source refs.
- [ ] Evidence source/hash must exist; trust upgrades are protected and source claims cannot self-declare Authoritative.
- [ ] Contradictions preserve both claims and require an authoritative resolution reference.
- [ ] Run/commit.

### Task 9: Implement deterministic context preflight, allocation, rendering and capture

**Files:** create planner/allocator/renderer/snapshot services and tests.

- [ ] Too-small budget for mandatory policy+identity+constraint+plan+output fails `mandatory_context_too_large`, persists a failed build attempt and creates no model request.
- [ ] Randomized retrieval order with fixed IDs/hashes produces identical selection, compression, omissions, messages and hashes.
- [ ] Preflight returns `ContextPlanSummary` without full optional rendering for H3 route filtering.
- [ ] Each source passes exact revision/lifecycle, read permission, provider-transfer eligibility, redaction and trust annotation.
- [ ] Allocate reserves output/protocol/safety; fit mandatory compression ladder; stable preferred/optional minimum/target/maximum rounds; final `count_model_input` includes messages/tools/output contract.
- [ ] Render authoritative instructions separately; external data is delimited/labeled/escaped.
- [ ] Full/Redacted capture creates an internal UTF-8 JSON `ContextCapture` Artifact through synchronous quarantine + secret/content inspection before snapshot commit. StructuredOnly/MetadataOnly retain no rendered capture bytes.
- [ ] Run/commit.

### Task 10: Bind H6 snapshots to H3 model execution

**Files:** create `context/model_integration.rs`; modify H3 orchestrator/fallback/runtime; add tests.

- [ ] Two-phase flow: preflight → route → exact envelope for model → exact H2 provider-transfer authorization → snapshot → ModelExecution/provider attempt.
- [ ] Before dispatch compare messages/tools/output-contract hash to snapshot rendered hash; mismatch fails before adapter.
- [ ] Exact-build incompatibility selects another already eligible larger-context model before provider attempt, creating a model-specific new snapshot.
- [ ] Provider `ContextTooLarge` with known non-completion creates immutable lower-ceiling/larger-model rebuild and new attempt; old snapshot remains.
- [ ] Prove no Rig/provider SDK types in context contracts.
- [ ] Run/commit.

### Task 11: Create mount/export/retention schema and integrate H4/H5 candidates

**Files:** create migration `0045`; domain `artifact/mount.rs`; mount service/repository/tests; modify H4 sandbox integration and H5 handoff-to-work integration.

**Migration ownership:** Task 11 creates `0045` once, including all mount/export/retention/purge tables. Task 13 uses these tables and never edits the applied migration.

- [ ] Migration creates mount grants/receipts, export grants/receipts, retention policies/assignments, legal holds, purge requests/events/targets and GC leases.
- [ ] Exact mount grant cannot mount another revision/sandbox/Run/step/path; one-use concurrent consumption yields one staging.
- [ ] Flow: Available revision → H2 `artifact.mount` → consume grant → stream to H4 broker staging → rehash → read-only `SandboxInputMount`; no blob key/path/credential reaches workload.
- [ ] H4 Tool/Sandbox candidates are opened through candidate port, hash/size verified and ingested as Quarantined Working Artifact.
- [ ] H5 internal/remote candidate references are opened through H5 port and ingested with handoff provenance; accepted handoff does not bypass inspection.
- [ ] Run/commit.

### Task 12: Implement restricted external URL/part ingestion

**Files:** create external service, restricted HTTP fetcher crate/fixtures/tests.

- [ ] Reject userinfo/fragments/non-HTTP schemes/IP literals/localhost/private/link-local/multicast/reserved/metadata ranges, DNS rebinding and forbidden redirects.
- [ ] Reuse H4 egress address/domain policy; resolve/validate/pin destination while preserving TLS hostname; disable environment proxies, cookies and caller auth headers.
- [ ] Enforce connect/total/header/body/decompression/media limits and stream directly into H6 ingestion.
- [ ] Normalize `Text`, `Json`, `Candidate`, `Url`; every result enters Quarantine. Auth-required URL waits for H8.
- [ ] Persist URL hash/policy/approved destination/result reference; replay never refetches.
- [ ] Run/commit.

### Task 13: Implement export, retention, legal hold, purge and GC

**Files:** create export/retention domain/services/repositories/tests and purge script. Consume migration `0045` without editing it.

- [ ] Read without `artifact.export` fails; grant binds exact revision/principal/purpose/Run/expiry/uses; unavailable revision cannot export.
- [ ] Export stream rehashes and records receipt without backend details.
- [ ] Logical delete blocks new context/mount/export; retention creates protected purge request rather than direct hard delete.
- [ ] Legal hold blocks purge and cannot be overridden by approval.
- [ ] Purge order:

```text
freeze new reads/mounts/exports
→ enumerate immutable targets
→ delete chunks/embeddings/cache
→ purge context capture Artifacts
→ delete representation/export/temp blobs
→ delete source blob only with no live references
→ inspect all keys
→ content-free tombstone + Completed
```

- [ ] Per-target idempotency and Unknown reconciliation; GC only no-live-reference blobs beyond safety horizon with lease.
- [ ] Run/commit.

### Task 14: Consolidate evaluated Runs through v0.1 memory policy

**Files:** create migration `0046`, consolidation domain/service/worker/repository/tests.

- [ ] Include verified outcome, accepted handoff, confirmed decision/procedure/task and evaluated evidence; exclude Unknown/failed/rejected/unresolved/raw transcript/unconfirmed scratchpad.
- [ ] Migration stores requests/reports/included/excluded sources/candidate bindings/events, not copied memory content.
- [ ] Sorted exact-source manifest hash is idempotency key.
- [ ] Deterministic templates create obvious Outcome/Task/Decision/Procedure; optional H3 extraction proposes bounded additional drafts.
- [ ] Every draft is type/schema/classification/source/conflict checked, then submitted as Candidate through v0.1 command with derivation `consolidation`.
- [ ] Restart queries idempotent v0.1 result/binding before resubmission; conflict becomes reconciliation, not duplicate memory.
- [ ] Run/commit.

### Task 15: Integrate H6 workers, checkpoint V4, RLS and acceptance

**Files:** create migration `0047`, worker/checkpoint/Run event/work changes, boundary scripts, CI, RLS/acceptance tests.

- [ ] Add work kinds:

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

- [ ] Add logical Run events only for attached/published Artifact reference, bound ContextSnapshot and requested/completed consolidation. Progress/index/scan/purge-target events stay H6-local.
- [ ] `RunCheckpointV4` adds latest snapshot per step, attached revisions, active ingestions, pending representations, active mounts and pending consolidation; older payloads remain readable.
- [ ] Migration `0047` forces RLS, same-workspace consistency, append-only guards and active/retrieval/expiry/purge/GC indexes; unavailable revisions cannot receive new capture/mount/export references.
- [ ] Boundary scripts reject direct memory SQL, hidden-reasoning fields, unbound model messages, SDK/filesystem/store types in domain/application, storage paths in DTOs, direct H4/H5 table copies and Available without inspection.
- [ ] Acceptance scenario proves streaming large Markdown ingestion, restart after promote, inspection/representation/chunks, deterministic budgeted context with v0.1 Memory references, contradiction handling, exact snapshot binding, H4 mount/output ingestion, remote text/JSON ingestion, SSRF rejection, export grant, purge/legal hold, governed memory candidates and no I/O during replay.
- [ ] Required CI jobs: artifact domain/Postgres, local/S3 conformance, inspection/processors, context/snapshots, sandbox/remote integration, purge, consolidation, boundaries, H6 acceptance.
- [ ] Run/commit:

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

## Migration ownership

```text
0042 Task 5  Artifact identities, revisions, blobs and provenance
0043 Task 6  Ingestion, inspections, representations and chunks
0044 Task 8  Context attempts/snapshots, scratchpads and evidence
0045 Task 11 Mounts, exports, retention, purge and GC schema
0046 Task 14 Consolidation and memory candidate bindings
0047 Task 15 RLS, indexes and Run bindings
```

No later task edits a migration after its owner task commits it.

## H6 completion definition

H6 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
bytes/source candidate
→ streaming stage/hash
→ promoted content-addressed blob
→ Quarantined ArtifactRevision
→ inspection
→ Available immutable revision
→ representation/chunks/excerpts
→ policy-filtered Context sources
→ deterministic token allocation
→ ContextSnapshot
→ H3 invocation
→ verified outputs/evidence
→ sandbox/remote output ingestion
→ deliverable/export
→ retention/purge
→ governed memory candidates
```

Required invariants:

1. v0.1 Memory/retrieval remain authoritative and are used through ports.
2. Metadata/bytes have separate replaceable contracts.
3. Artifact revisions are immutable/content-addressed inside a storage namespace.
4. Store crash windows do not duplicate revisions or silently lose bytes.
5. Every new byte source starts Quarantined and required inspection cannot be bypassed.
6. Representation provenance fixes source and generator revision.
7. Excerpts are policy/workspace/lifecycle/classification filtered before retrieval.
8. Mandatory context cannot be silently omitted or demoted unsafely.
9. Final token count includes messages, tools and output contract.
10. Context allocation/rendering is deterministic for fixed inputs.
11. Every Harness model invocation binds exact input to an immutable snapshot.
12. Capture bytes can be purged without removing snapshot metadata.
13. Hidden chain-of-thought is not requested/stored; scratchpad is typed/bounded.
14. Untrusted evidence cannot self-promote to instruction/authority.
15. Contradictions remain explicit until supported resolution.
16. H4 mounts reveal no backend path/credential and are exact/read-only/short-lived.
17. H4/H5 candidates do not become Available without H6 inspection.
18. External fetching rejects SSRF/rebinding/ambient credentials/unbounded bodies.
19. Export is distinct exact authority.
20. Delete/retention/hold/purge/GC remain separate durable operations.
21. Hard purge removes all configured content-bearing derivatives and verifies deletion.
22. Consolidation accepts evaluated sources only and submits Candidate memory through v0.1 policy.
23. Restart does not duplicate blob/revision/mount/export/purge/candidate effects.
24. Replay performs no external/content-mutating H6 action.
25. H7 can expose uploads/downloads/review without changing H6 core.
26. H8 can supply request-scoped credentials without storing secrets in H6 records.
27. H9 can select processors/memory profiles through immutable package revisions.
28. H9A can map A2A parts to `ExternalArtifactPart` without `a2a-rs` leakage.
29. H10 can enrich evidence/evaluation without rewriting H6 history.
30. H6 tests require no public service or permanent credential.

## Explicit non-goals

H6 does not implement HTTP UI/channels, human-review delivery, OAuth/API-key acquisition, A2A transport types, unrestricted fetching, execution of document instructions, arbitrary in-process third-party parsers, browser automation, global cross-workspace deduplication, mutable revisions, hidden chain-of-thought retention, memory-policy bypass, semantic contradiction resolution, permanent public URLs or production replay of processors/exports/fetches.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h6-context-artifact-runtime`, change dependencies, create migrations `0042`–`0047`, create stores/buckets, stage/scan/process/mount/export/purge real files, modify production memory/model/tool/sandbox/delegation code, change CI or execute H6 tests.
