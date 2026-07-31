# Vestrace H11 Universal Vertical Slice and Product Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add frontend/SDK/release dependencies, create migrations, start services, build images, install reference packages, publish schemas, run the vertical slice, create backups, modify CI or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Assemble H1–H10 and H9A into one production-shaped self-hosted Vestrace v0.2 surface with equivalent HTTP/CLI/MCP application behavior, Rust and TypeScript SDKs, resumable Artifact transfer, signed webhooks, a minimal secure web console, reference Agent Packages, Personal/Team/Embedded deployment profiles, upgrade/backup/recovery tooling and restart-safe end-to-end release acceptance.

**Architecture:** H11 is an integration and productization layer, not a new execution authority. Existing H1–H10 and H9A aggregates, journals, policies, budgets, credentials, Artifacts, evaluations and checkpoints remain authoritative. Public surfaces translate versioned DTOs into shared application commands and queries, then return durable operation/resource references. The web console and SDKs consume only those public contracts. Deployment, backup and release operations use independent durable product-operation records; `RunCheckpointV9` is not extended by H11. Reference packages are ordinary H9 packages with exact H10 gate evidence, and the release vertical slice proves the complete system through real application boundaries and deterministic local fixtures.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H10 and H9A; Rust Edition 2024; Tokio; Axum/Tower; `rmcp`; Serde/Schemars; SQLx and PostgreSQL 17 with pgvector; H6 Local CAS and S3-compatible Artifact stores; H8 encrypted secret backend; Docker/Compose with a rootless-compatible sandbox manager; OpenAPI 3.1 and JSON Schema 2020-12; Server-Sent Events; HMAC-SHA-256 signed webhooks; Rust SDK using Reqwest; TypeScript SDK and web console using TypeScript, React and Vite; deterministic local OpenAI-compatible, Tool, A2A, webhook and object-store fixtures; cargo-nextest-compatible test layout; CycloneDX/SPDX-compatible SBOM output.

## Global Constraints

- Complete all five v0.1 plans, H1–H10, H9A and the binding ADR-0002 outcome before implementing H11.
- The Harness design sections `27. Public API, event stream, MCP и SDK`, `28. Self-hosted product boundary`, `29. Reference Agent Packages`, `30. Обязательный vertical slice`, `31. Release acceptance criteria` and `32. Нормативные security invariants` are normative.
- H11 never creates a second Run, plan, Tool, model, memory, Artifact, approval, policy, budget, credential, remote-agent, evaluation or audit aggregate.
- H1 `AgentRun`, `RunStep`, `RunEvent` and `RunCheckpointV9` remain authoritative. H11 adds no `RunCheckpointV10`.
- Product-surface operations such as upload, webhook delivery, backup, restore and upgrade have independent durable state and must not increment `RunVersion` unless they cause an explicit logical Run command through an existing application service.
- H7 `PublicEventRecord` and durable workspace cursor remain the only event-stream source. HTTP SSE, SDK streams, CLI follow mode and console timelines are projections over H7.
- H6 remains the only Artifact byte/provenance lifecycle. Upload/download/product surfaces never introduce a second blob identity or bypass quarantine, inspection, export authorization, retention or purge.
- H8 remains the only secret/credential authority. Webhook signing, SDK tokens, release signing and external storage credentials are references or request-scoped leases, never plaintext configuration or database fields.
- H9 remains the package/profile/extension authority. Built-in reference packages are installed, resolved, evaluated and activated through normal H9/H10 flows; release bundling does not silently grant capabilities.
- H9A remains the A2A gateway. H11 only mounts/configures it, publishes one selected reference agent and includes it in release acceptance.
- H10 verification, one-use `VerifiedRunCompletion`, regression evidence, audit and explanation remain mandatory. H11 cannot bypass them for product demos or reference packages.
- Public DTOs, generated SDK types and console state are separate from domain, SQLx, provider, adapter and internal checkpoint types.
- `/v1` is the only stable product API prefix. `/admin/v1` remains administrative and capability-gated. No unrestricted state `PATCH` endpoint is introduced.
- Every state-changing HTTP command requires `Idempotency-Key`. Commands changing a versioned or mutable resource require `If-Match` or an explicit expected version.
- The same idempotency key with the same canonical request returns the original receipt/result; reuse with changed canonical content returns `idempotency_conflict`.
- Async commands return `202 Accepted` with an existing `OperationId` or resource ID, current state/version and canonical status/events links.
- Public errors use one bounded envelope and never expose SQL, stack traces, prompts, secrets, raw external errors, internal paths or adapter Debug output.
- SSE uses H7 cursor values. `Last-Event-ID` and `after_cursor` may not disagree. Reconnect is at-least-once and clients deduplicate by event ID.
- HTTP, CLI and MCP call the same application command/query services and therefore produce equivalent canonical state for overlapping operations.
- A2A is not forced into HTTP/MCP parity where protocol semantics differ. Equivalence is required only at shared application outcomes and references.
- Artifact upload is streaming/resumable, bounded and hash-validated. A completed upload becomes an H6 quarantined ingestion candidate, not an Available Artifact.
- Artifact download requires current H2 authorization and either a direct authenticated stream or a one-purpose short-lived download grant. Grants are audience/resource/range/expiry bound and never bearer access to other revisions.
- Active HTML, SVG with scripts, executable content and untrusted rich documents are never rendered directly by the console. The console uses H6 Preview/RedactedCopy/PageImage representations or forces attachment download.
- Outgoing webhooks are at-least-once notifications, not commands. Retries reuse the same immutable body, event IDs and delivery ID so receivers can deduplicate.
- Webhook redirects and ambient proxies are disabled. Destination validation follows H6/H9A SSRF protections; private/reserved targets require an explicit local deployment policy.
- Every webhook attempt uses a fresh H8 credential/signing lease. Signing secrets never enter subscription rows, delivery bodies, logs or console DTOs.
- The reference signature format is `v1=<lowercase hex HMAC-SHA-256>` over `timestamp + "." + delivery_id + "." + exact_body_bytes`.
- SDKs never automatically retry an ambiguous non-idempotent operation. They expose `operation_unknown` and reconciliation/status helpers.
- SDKs do not implement policy, approval matching, budget logic, Tool execution or credential resolution locally.
- The TypeScript browser SDK and console never store bearer tokens in localStorage, sessionStorage or IndexedDB. `BearerPrompt` keeps the token in memory only; reload requires re-authentication.
- Initial authentication remains v0.1 local trusted mode for loopback/stdio and bearer-token mode for HTTP. OIDC, public signup and browser SSO remain out of scope.
- Personal deployment binds externally reachable services to loopback by default and refuses wildcard binding without an explicit unsafe-development override or configured TLS termination.
- Team deployment supports multiple principals/workspaces using existing bearer-token and capability contracts; it must not pretend to provide OIDC.
- Embedded deployment disables the console and interactive local bootstrap by default and exposes HTTP/MCP/SDK/A2A surfaces according to explicit configuration.
- Configuration precedence remains `CLI → environment → configuration file → safe defaults`. Environment variables contain secret references or bootstrap inputs only, never long-lived generated credentials in rendered diagnostics.
- Docker images run as non-root where the role permits, use read-only root filesystems where practical, expose explicit writable volumes and contain no build credentials.
- Untrusted sandbox containers never receive the Docker socket. The sandbox manager may access a dedicated rootless-compatible engine endpoint under H4 policy.
- Forward-only migrations remain explicit. Server startup never auto-applies migrations unless the operator enabled the exact Personal-development setting.
- Backups are content-addressed, manifest-driven and verified. Clear secret material and the secret-backend master key are never placed in the same backup bundle.
- Restore targets an empty or explicitly disposable deployment. In-place destructive restore is not a release feature.
- Binary rollback is allowed only when the old binary declares compatibility with the current schema/release manifest. Database down-migrations are not generated.
- Release artifacts contain exact public schema hashes, migration head, package hashes, image digests, dependency lock hash and SBOM references.
- Reference packages and release fixtures must pass H10 regression gates before release bundling. Passing evidence does not activate them in existing workspaces.
- The mandatory vertical slice uses deterministic local provider/tool/A2A/webhook/storage fixtures in CI and requires no public internet, permanent external credential or managed service.
- Existing migrations `0014`–`0080` are never edited. H11 migrations are `0081`–`0086`, each created once by one task.
- Future implementation branch: `feat/h11-universal-product-surface`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
.github/workflows/ci.yml
.github/workflows/release.yml

crates/vestrace-domain/src/
  id.rs
  product/{mod,profile,release,bootstrap,operation,error}.rs
  public_api/{mod,version,request,event,pagination,schema}.rs
  transfer/{mod,upload,download}.rs
  webhook/{mod,subscription,delivery,signature}.rs
  backup/{mod,manifest,run,restore,upgrade}.rs

crates/vestrace-application/src/
  product/{mod,ports,bootstrap,readiness,release}.rs
  surface/{mod,commands,queries,receipts,parity}.rs
  transfer/{mod,ports,upload_service,download_service}.rs
  webhook/{mod,ports,subscription_service,delivery_service,worker}.rs
  backup/{mod,ports,create,verify,restore,upgrade}.rs
  composition/product.rs

crates/vestrace-channel-http/src/
  lib.rs
  router.rs
  auth.rs
  middleware/{request_id,idempotency,concurrency,error,limits,cors,csp}.rs
  dto/{mod,common,run,plan,approval,artifact,agent,trigger,connection,remote,evaluation,audit}.rs
  routes/{mod,workspaces,conversations,runs,steps,events,approvals,artifacts,agents,skills,workflows,tools,triggers,connections,policies,models,extensions,evaluations,audit,operations,webhooks,schemas}.rs
  sse.rs
  upload.rs
  download.rs

crates/vestrace-channel-cli/src/
  lib.rs
  output.rs
  follow.rs
  commands/{run,approval,artifact,agent,trigger,connection,remote,evaluation,audit,backup,upgrade,doctor}.rs

crates/vestrace-channel-mcp/src/
  lib.rs
  server.rs
  auth.rs
  tools/{mod,memory,run,artifact,agent,workflow,approval,evaluation}.rs
  resources/{mod,memory,run,artifact,agent,workflow,operation}.rs
  mapping.rs
  error.rs

crates/vestrace-sdk-rust/
  Cargo.toml
  src/{lib,client,config,error,models,operations,runs,events,artifacts,approvals,agents,triggers,evaluations,webhooks}.rs
  tests/{contract,sse,upload,unknown}.rs

packages/sdk-typescript/
  package.json
  tsconfig.json
  src/{index,client,errors,generated,operations,runs,events,artifacts,approvals,agents,triggers,evaluations}.ts
  tests/{contract,sse,upload,unknown}.test.ts

apps/console/
  package.json
  vite.config.ts
  tsconfig.json
  index.html
  src/
    main.tsx
    app.tsx
    auth/{mode,token}.ts
    api/client.ts
    events/useRunEvents.ts
    routes.tsx
    components/{ErrorBoundary,ConfirmDialog,StatusBadge,ReferenceLink,SafePreview}.tsx
    pages/{Runs,RunDetail,Approvals,Artifacts,Agents,Connections,RemoteAgents,Triggers,Evaluations,Audit,Settings}.tsx
    styles.css
  tests/{auth,run_detail,approval,artifact_preview,a11y}.test.tsx

schemas/
  openapi/v1.json
  events/v1/*.json
  json/v1/*.json
  mcp/v1.json
  extensions/v1.json
  a2a/v1-reference.json
  product/release-manifest.schema.json
  product/config.schema.json
  compatibility/public-schema-baseline.json

packages/reference/
  universal-assistant/
    vestrace-package.json
    profiles/*.json
    skills/*.json
    workflows/*.json
    policies/*.json
    schemas/*.json
    evaluations/*.json
    examples/*.json
  research/
    vestrace-package.json
    profiles/*.json
    skills/*.json
    workflows/*.json
    policies/*.json
    schemas/*.json
    evaluations/*.json
    examples/*.json
  workspace-automation/
    vestrace-package.json
    profiles/*.json
    skills/*.json
    workflows/*.json
    policies/*.json
    schemas/*.json
    evaluations/*.json
    examples/*.json

fixtures/product/
  provider/
  tools/
  a2a/
  webhook/
  object_store/
  vertical_slice/

config/
  vestrace.example.toml
  personal.toml
  team.toml
  embedded.toml

deploy/
  Dockerfile
  Dockerfile.console
  compose.personal.yml
  compose.team.yml
  compose.embedded.yml
  healthcheck.sh
  entrypoint.sh
  README.md

docs/
  api.md
  mcp.md
  sdk-rust.md
  sdk-typescript.md
  console.md
  deployment-personal.md
  deployment-team.md
  deployment-embedded.md
  backup-restore.md
  upgrade.md
  operations.md
  security.md
  release-checklist.md

crates/vestrace-infrastructure/src/postgres/
  product/{mod,request_binding_repository,schema_repository,bootstrap_repository,release_repository}.rs
  transfer/{mod,upload_repository,download_repository}.rs
  webhook/{mod,subscription_repository,delivery_repository}.rs
  backup/{mod,run_repository,manifest_repository}.rs

migrations/
  0081_public_request_bindings_schema_bundles_and_release_surfaces.sql
  0082_artifact_upload_sessions_parts_and_download_grants.sql
  0083_webhook_subscriptions_deliveries_and_attempts.sql
  0084_product_profiles_bootstrap_and_release_manifests.sql
  0085_backup_restore_and_upgrade_runs.sql
  0086_product_surface_rls_indexes_and_bindings.sql

tests/
  public_api_contract.rs
  public_api_idempotency.rs
  public_api_concurrency.rs
  public_error_envelope.rs
  public_event_stream.rs
  surface_parity.rs
  artifact_upload_resume.rs
  artifact_download_range.rs
  artifact_transfer_security.rs
  webhook_signature.rs
  webhook_delivery_restart.rs
  webhook_ssrf.rs
  mcp_contract.rs
  mcp_policy_parity.rs
  rust_sdk_contract.rs
  typescript_sdk_contract.rs
  schema_compatibility.rs
  reference_package_install.rs
  reference_package_gate.rs
  product_bootstrap.rs
  console_security.rs
  deployment_profiles.rs
  compose_smoke.rs
  backup_consistency.rs
  restore_empty_target.rs
  upgrade_preflight.rs
  release_manifest.rs
  product_surface_rls.rs
  h11_vertical_slice.rs
  h11_inbound_a2a.rs
  h11_restart_matrix.rs

scripts/
  generate-public-schemas.sh
  verify-public-schema-compatibility.sh
  verify-surface-parity.sh
  verify-sdk-generation.sh
  verify-console-security.sh
  verify-reference-package-locks.sh
  verify-release-manifest.sh
  verify-image-boundary.sh
  verify-compose-profiles.sh
  verify-backup-manifest.sh
  run-h11-vertical-slice.sh
```

---

## Normative contracts

### Product profiles and release identity

```rust
pub enum ProductDeploymentProfile {
    Personal,
    Team,
    Embedded,
}

pub struct ProductProfileRevision {
    pub id: ProductProfileRevisionId,
    pub profile: ProductDeploymentProfile,
    pub revision: u32,
    pub enabled_roles: std::collections::BTreeSet<DeploymentRole>,
    pub enabled_surfaces: std::collections::BTreeSet<ProductSurface>,
    pub authentication_mode: ProductAuthenticationMode,
    pub artifact_backend_kind: String,
    pub secret_backend_kind: String,
    pub sandbox_required: bool,
    pub console_enabled: bool,
    pub safe_defaults_hash: [u8; 32],
    pub content_hash: [u8; 32],
}

pub enum ProductSurface {
    HttpApi,
    Sse,
    Cli,
    McpStdio,
    McpHttp,
    A2AInbound,
    A2AOutbound,
    WebConsole,
    OutgoingWebhooks,
}

pub enum ProductAuthenticationMode {
    LocalTrusted,
    BearerToken,
}

pub struct ProductReleaseManifest {
    pub id: ProductReleaseManifestId,
    pub product_version: String,
    pub source_revision: String,
    pub rust_toolchain: String,
    pub cargo_lock_hash: [u8; 32],
    pub migration_head: String,
    pub public_schema_bundle_revision_id: PublicSchemaBundleRevisionId,
    pub config_schema_hash: [u8; 32],
    pub reference_package_revision_hashes: Vec<[u8; 32]>,
    pub container_image_digests: Vec<String>,
    pub sbom_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub compatibility: ReleaseCompatibilityRange,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub struct ReleaseCompatibilityRange {
    pub minimum_schema_head: String,
    pub maximum_schema_head: String,
    pub minimum_public_api_major: u16,
    pub maximum_public_api_major: u16,
}
```

Release manifests are immutable. Image digests use normalized `sha256:<64 lowercase hex>` form. A release manifest contains no registry credential, signing private key, host path or environment value.

### Public request identity and receipts

H11 extends the existing v0.1 idempotency/operation layer rather than creating another generic operation system.

```rust
pub enum PublicSurfaceKind {
    Http,
    Cli,
    Mcp,
    A2A,
    Console,
    RustSdk,
    TypeScriptSdk,
}

pub struct PublicRequestBinding {
    pub id: PublicRequestBindingId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub surface: PublicSurfaceKind,
    pub operation_name: String,
    pub idempotency_key_hash: [u8; 32],
    pub canonical_request_hash: [u8; 32],
    pub operation_id: Option<OperationId>,
    pub resource: Option<PublicResourceRef>,
    pub response_hash: Option<[u8; 32]>,
    pub created_at: Timestamp,
}

pub enum PublicResourceRef {
    Run(AgentRunId),
    Artifact(ArtifactRevisionId),
    Upload(ArtifactUploadSessionId),
    Trigger(TriggerDefinitionId),
    Evaluation(EvaluationRunId),
    Webhook(WebhookSubscriptionId),
    Backup(BackupRunId),
}

pub struct PublicCommandReceipt {
    pub request_id: PublicRequestId,
    pub operation_id: Option<OperationId>,
    pub resource: Option<PublicResourceRef>,
    pub status: PublicCommandStatus,
    pub resource_version: Option<u64>,
    pub status_url: Option<String>,
    pub events_url: Option<String>,
    pub accepted_at: Timestamp,
}

pub enum PublicCommandStatus {
    Accepted,
    Completed,
    WaitingForInput,
    WaitingForApproval,
    Rejected,
}
```

Idempotency keys are 8–200 visible ASCII bytes before hashing. The canonical request hash includes API major, operation name, authenticated workspace/principal, normalized body and expected version. Authorization headers, trace headers and transport metadata are excluded.

### Public error envelope

```rust
pub enum PublicErrorCategory {
    Validation,
    Authentication,
    Authorization,
    ApprovalRequired,
    VersionConflict,
    IdempotencyConflict,
    BudgetExceeded,
    PolicyChanged,
    OperationUnknown,
    RateLimited,
    Unavailable,
    NotFound,
    Internal,
}

pub struct PublicErrorEnvelope {
    pub request_id: PublicRequestId,
    pub error: PublicErrorBody,
}

pub struct PublicErrorBody {
    pub code: String,
    pub category: PublicErrorCategory,
    pub message: String,
    pub details: Vec<PublicErrorDetail>,
    pub operation_id: Option<OperationId>,
    pub human_request_id: Option<HumanRequestId>,
    pub current_version: Option<u64>,
    pub retry_after_ms: Option<u64>,
}

pub struct PublicErrorDetail {
    pub path: Option<String>,
    pub code: String,
    pub safe_message: String,
}
```

Messages are bounded to 8 KiB, detail count to 32 and detail messages to 2 KiB. Internal errors return a stable code and request ID only.

### Public event stream

```rust
pub struct PublicEventEnvelopeV1 {
    pub schema_version: u16,
    pub cursor: PublicEventCursor,
    pub event_id: PublicEventId,
    pub event_type: String,
    pub workspace_id: WorkspaceId,
    pub resource: PublicEventResourceRef,
    pub causation_id: CausationId,
    pub data: serde_json::Value,
    pub occurred_at: Timestamp,
}

pub enum PublicEventResourceRef {
    Run(AgentRunId),
    HumanRequest(HumanRequestId),
    Artifact(ArtifactRevisionId),
    Trigger(TriggerDefinitionId),
    Evaluation(EvaluationRunId),
    RemoteAgent(RemoteAgentInvocationId),
    ProductOperation(OperationId),
}
```

`data` must validate against the exact event schema selected by `event_type` and schema bundle revision. SSE frame `id` is the cursor, `event` is the event type and `data` is the canonical envelope. Heartbeats are SSE comments and never advance the cursor.

### Public schema bundle

```rust
pub struct PublicSchemaBundleRevision {
    pub id: PublicSchemaBundleRevisionId,
    pub bundle_id: PublicSchemaBundleId,
    pub revision: u32,
    pub api_major: u16,
    pub openapi_artifact_revision_id: ArtifactRevisionId,
    pub event_schema_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub json_schema_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub mcp_schema_artifact_revision_id: ArtifactRevisionId,
    pub extension_schema_artifact_revision_id: ArtifactRevisionId,
    pub a2a_reference_schema_artifact_revision_id: ArtifactRevisionId,
    pub compatibility_baseline_hash: [u8; 32],
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Within `/v1`, additive optional fields and new enum values declared forward-compatible are allowed. Removing required fields, changing meaning/type, narrowing accepted input or changing operation IDs is breaking and requires a new API major.

### Surface command/query facade

Every product surface maps to these application-owned interfaces:

```rust
#[async_trait::async_trait]
pub trait ProductCommandFacade: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        command: ProductCommand,
    ) -> Result<PublicCommandReceipt, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ProductQueryFacade: Send + Sync {
    async fn query(
        &self,
        context: &RequestContext,
        query: ProductQuery,
    ) -> Result<ProductQueryResult, ApplicationError>;
}
```

`ProductCommand` is a closed enum of references to existing commands:

```text
CreateRun
PauseRun
ResumeRun
CancelRun
SubmitHumanResponse
GrantApproval
RevisePlan
RequestBudgetIncrease
FireTrigger
AcceptRunProposal
RejectRunProposal
CreateEvaluationRun
VerifyAuditIntegrity
CreateWebhookSubscription
DisableWebhookSubscription
```

Artifact upload/download and backup/restore use dedicated services because they are streaming or deployment-scoped. No surface may bypass these facades to call repositories directly.

### Artifact upload

```rust
pub enum ArtifactUploadSessionStatus {
    Created,
    Receiving,
    CompletePendingHash,
    Finalizing,
    Quarantined,
    Rejected,
    Expired,
    Cancelled,
}

pub struct ArtifactUploadSession {
    pub id: ArtifactUploadSessionId,
    pub workspace_id: WorkspaceId,
    pub owner_principal_id: PrincipalId,
    pub declared_media_type: Option<String>,
    pub original_filename: Option<String>,
    pub expected_size: Option<u64>,
    pub expected_hash: Option<ArtifactContentHash>,
    pub maximum_bytes: u64,
    pub part_size: u64,
    pub status: ArtifactUploadSessionStatus,
    pub ingestion_session_id: ArtifactIngestionSessionId,
    pub expires_at: Timestamp,
    pub logical_revision: u64,
}

pub struct ArtifactUploadPart {
    pub session_id: ArtifactUploadSessionId,
    pub ordinal: u32,
    pub byte_start: u64,
    pub byte_end_exclusive: u64,
    pub byte_size: u64,
    pub content_hash: ArtifactContentHash,
    pub staged_handle_hash: [u8; 32],
    pub received_at: Timestamp,
}
```

Parts are contiguous, non-overlapping and immutable. Default part size is 8 MiB; configurable range is 1–64 MiB. Maximum session size follows workspace/H6 policy. Re-uploading an ordinal with the same range/hash is idempotent; changed content conflicts. Finalization verifies total size, ordered rolling hash and H6 ingestion binding before promotion/quarantine.

### Artifact download grants

```rust
pub struct ArtifactDownloadGrant {
    pub id: ArtifactDownloadGrantId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub artifact_revision_id: ArtifactRevisionId,
    pub allowed_range: Option<ArtifactByteRange>,
    pub audience_hash: [u8; 32],
    pub token_hash: [u8; 32],
    pub maximum_uses: u16,
    pub used_count: u16,
    pub expires_at: Timestamp,
}
```

Default maximum uses is one and maximum lifetime is five minutes. The clear token is returned once and never persisted. Range responses include immutable ETag from the Artifact content hash and enforce `If-Range` correctly.

### Webhook subscriptions and delivery

```rust
pub enum WebhookSubscriptionLifecycle {
    Draft,
    Active,
    Disabled,
    Revoked,
}

pub struct WebhookSubscriptionRevision {
    pub id: WebhookSubscriptionRevisionId,
    pub subscription_id: WebhookSubscriptionId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub destination: NormalizedOriginAndPath,
    pub event_type_filters: std::collections::BTreeSet<String>,
    pub resource_filters: Vec<WebhookResourceFilter>,
    pub signing_service_binding_revision_id: ServiceCredentialBindingRevisionId,
    pub maximum_classification: DataClassification,
    pub timeout_ms: u64,
    pub retry_policy: WebhookRetryPolicy,
    pub content_hash: [u8; 32],
}

pub struct WebhookDelivery {
    pub id: WebhookDeliveryId,
    pub workspace_id: WorkspaceId,
    pub subscription_revision_id: WebhookSubscriptionRevisionId,
    pub event_ids: Vec<PublicEventId>,
    pub exact_body_hash: [u8; 32],
    pub status: WebhookDeliveryStatus,
    pub next_attempt_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

pub enum WebhookDeliveryStatus {
    Pending,
    Delivering,
    Delivered,
    RetryScheduled,
    Failed,
    Disabled,
}

pub struct WebhookDeliveryAttempt {
    pub id: WebhookDeliveryAttemptId,
    pub delivery_id: WebhookDeliveryId,
    pub attempt_number: u16,
    pub request_timestamp: Timestamp,
    pub response_status: Option<u16>,
    pub outcome: WebhookAttemptOutcome,
    pub safe_code: String,
    pub observed_at: Timestamp,
}
```

One delivery contains 1–100 events and at most 1 MiB canonical JSON. Attempts are capped at 12 over 24 hours with exponential backoff and jitter. HTTP `2xx` is delivered; `408`, `425`, `429` and `5xx` are retryable; other `4xx` fail. Response bodies are not stored. A lost response schedules the same immutable delivery again under at-least-once semantics.

### SDK retry contract

```rust
pub enum SdkRetryClass {
    Never,
    SafeRead,
    SameIdempotentCommand,
    StatusOrReconciliationOnly,
}
```

- `GET`, range reads and SSE reconnect use `SafeRead`.
- A command with a stable caller-supplied idempotency key may use `SameIdempotentCommand` only after a transport failure known to occur before any response body and only against the same endpoint/body/hash.
- `operation_unknown`, H4/H9A Unknown and webhook/Artifact finalization ambiguity use `StatusOrReconciliationOnly`.
- SDKs never generate a new idempotency key during retry.

### Reference package release set

```rust
pub struct ReferencePackageReleaseEntry {
    pub stable_id: String,
    pub version: String,
    pub package_artifact_revision_id: ArtifactRevisionId,
    pub package_revision_id: AgentPackageRevisionId,
    pub package_lock_id: AgentPackageLockId,
    pub regression_gate_evidence_id: RegressionGateEvidenceId,
    pub default_profile_stable_name: String,
    pub content_hash: [u8; 32],
}
```

Required stable IDs:

```text
vestrace.universal-assistant
vestrace.research
vestrace.workspace-automation
```

The release set is pinned in the ProductReleaseManifest. Workspace activation is explicit and creates normal H9 activation revisions.

### Product bootstrap

```rust
pub enum ProductBootstrapStatus {
    Created,
    InstallingReferencePackages,
    CreatingWorkspace,
    CreatingPrincipal,
    ApplyingSafePolicies,
    Ready,
    Failed,
}

pub struct ProductBootstrapRun {
    pub id: ProductBootstrapRunId,
    pub deployment_profile_revision_id: ProductProfileRevisionId,
    pub status: ProductBootstrapStatus,
    pub default_workspace_id: Option<WorkspaceId>,
    pub initial_principal_id: Option<PrincipalId>,
    pub installed_package_revision_ids: Vec<AgentPackageRevisionId>,
    pub activated_package_revision_ids: Vec<PackageActivationRevisionId>,
    pub safe_policy_snapshot_id: Option<PolicySnapshotId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Personal bootstrap may create one default workspace/principal and activate the Universal Assistant package under safe `Prepare` autonomy. Team bootstrap creates the administrative workspace/principal but does not activate workspace packages without an explicit admin command. Embedded bootstrap creates no interactive principal or package activation by default.

### Backup, restore and upgrade

```rust
pub enum ProductMaintenanceOperationStatus {
    Created,
    Preflight,
    Quiescing,
    Capturing,
    Verifying,
    Ready,
    Applying,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

pub struct BackupManifest {
    pub id: BackupManifestId,
    pub deployment_id: DeploymentInstallationId,
    pub product_release_manifest_id: ProductReleaseManifestId,
    pub database_snapshot_artifact_revision_id: ArtifactRevisionId,
    pub artifact_inventory_revision_id: ArtifactRevisionId,
    pub secret_backend_ciphertext_inventory_revision_id: ArtifactRevisionId,
    pub configuration_snapshot_artifact_revision_id: ArtifactRevisionId,
    pub database_schema_head: String,
    pub database_snapshot_lsn: String,
    pub artifact_generation: u64,
    pub component_hashes: Vec<[u8; 32]>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub struct BackupRun {
    pub id: BackupRunId,
    pub deployment_id: DeploymentInstallationId,
    pub status: ProductMaintenanceOperationStatus,
    pub manifest_id: Option<BackupManifestId>,
    pub operation_id: OperationId,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

pub struct RestoreRun {
    pub id: RestoreRunId,
    pub target_deployment_id: DeploymentInstallationId,
    pub source_manifest_id: BackupManifestId,
    pub status: ProductMaintenanceOperationStatus,
    pub target_empty_verified: bool,
    pub operation_id: OperationId,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

pub struct UpgradeRun {
    pub id: UpgradeRunId,
    pub deployment_id: DeploymentInstallationId,
    pub from_release_manifest_id: ProductReleaseManifestId,
    pub to_release_manifest_id: ProductReleaseManifestId,
    pub preflight_report_artifact_revision_id: ArtifactRevisionId,
    pub backup_manifest_id: BackupManifestId,
    pub status: ProductMaintenanceOperationStatus,
    pub operation_id: OperationId,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}
```

Backup quiescence blocks new write commands, waits for active database transactions to finish and pauses new work leasing, but does not kill running external operations. If effectful operations remain Dispatching/Unknown after the configured maximum, backup fails safely rather than claiming a consistent snapshot. The secret master key is documented and backed up separately by the operator.

### Product readiness

```rust
pub enum ReadinessCheckKind {
    Database,
    Migrations,
    ArtifactStore,
    SecretBackend,
    ModelRuntime,
    ToolRuntime,
    SandboxManager,
    Scheduler,
    Workers,
    PublicSchemas,
    ReferencePackages,
    AuditIntegrity,
}

pub struct ProductReadinessReport {
    pub product_release_manifest_id: ProductReleaseManifestId,
    pub profile_revision_id: ProductProfileRevisionId,
    pub checks: Vec<ProductReadinessCheck>,
    pub ready: bool,
    pub generated_at: Timestamp,
}
```

`/health/live` reports process liveness only. `/health/ready` is successful only when mandatory profile checks pass. Optional provider/remote-agent unavailability appears as degraded detail unless the active profile/package requires it.

---

### Task 1: Add product, public-surface, transfer, webhook and maintenance contracts

**Files:** create H11 domain modules, modify IDs and exports, add unit/property tests.

**Consumes:** all authoritative IDs and contracts from v0.1/H1–H10/H9A.

**Produces:** every H11 domain/value contract defined above.

- [ ] Add all H11 IDs: `ProductProfileRevisionId`, `ProductReleaseManifestId`, `DeploymentInstallationId`, `PublicRequestId`, `PublicRequestBindingId`, `PublicSchemaBundleId`, `PublicSchemaBundleRevisionId`, `ArtifactUploadSessionId`, `ArtifactDownloadGrantId`, `WebhookSubscriptionId`, `WebhookSubscriptionRevisionId`, `WebhookDeliveryId`, `WebhookDeliveryAttemptId`, `ProductBootstrapRunId`, `BackupManifestId`, `BackupRunId`, `RestoreRunId`, `UpgradeRunId`.
- [ ] Write transition tests for upload, webhook, bootstrap and maintenance operation states, including terminal closure and explicit Unknown handling.
- [ ] Write canonical-hash tests for public request, schema bundle, package release set, webhook body/signature input, release manifest and backup manifest.
- [ ] Write tests proving product operations cannot be placed in `RunCheckpointV9` and do not increment RunVersion by themselves.
- [ ] Write validation tests for profile-safe defaults, release digests, idempotency keys, public error bounds, upload ranges, download-grant lifetime, webhook limits and empty-target restore.
- [ ] Implement the normative types and validation exactly as specified.
- [ ] Run and commit:

```bash
cargo test -p vestrace-domain product:: public_api:: transfer:: webhook:: backup::
git add crates/vestrace-domain
git commit -m "feat(product): add H11 surface and release contracts"
```

### Task 2: Define the shared product facade and surface-parity test harness

**Files:** create application surface/product modules and deterministic parity fixtures/tests.

**Interfaces:**

```rust
#[async_trait::async_trait]
pub trait PublicRequestBindingPort: Send + Sync {
    async fn begin(
        &self,
        context: &RequestContext,
        request: BeginPublicRequest,
    ) -> Result<PublicRequestDisposition, ApplicationError>;
    async fn complete(
        &self,
        context: &RequestContext,
        request: CompletePublicRequest,
    ) -> Result<PublicRequestBinding, ApplicationError>;
}

#[async_trait::async_trait]
pub trait PublicSchemaRegistryPort: Send + Sync {
    async fn current_bundle(
        &self,
        context: &RequestContext,
        api_major: u16,
    ) -> Result<PublicSchemaBundleRevision, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ArtifactTransferPort: Send + Sync {
    async fn create_upload(&self, context: &RequestContext, request: CreateArtifactUpload)
        -> Result<ArtifactUploadSession, ApplicationError>;
    async fn append_part(&self, context: &RequestContext, request: AppendArtifactUploadPart)
        -> Result<ArtifactUploadSession, ApplicationError>;
    async fn finalize_upload(&self, context: &RequestContext, request: FinalizeArtifactUpload)
        -> Result<ArtifactUploadSession, ApplicationError>;
    async fn create_download_grant(&self, context: &RequestContext, request: CreateArtifactDownloadGrant)
        -> Result<IssuedArtifactDownloadGrant, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ProductMaintenancePort: Send + Sync {
    async fn create_backup(&self, context: &RequestContext, request: CreateBackup)
        -> Result<BackupRun, ApplicationError>;
    async fn verify_backup(&self, context: &RequestContext, manifest_id: BackupManifestId)
        -> Result<BackupVerificationReport, ApplicationError>;
    async fn restore(&self, context: &RequestContext, request: CreateRestore)
        -> Result<RestoreRun, ApplicationError>;
    async fn upgrade(&self, context: &RequestContext, request: CreateUpgrade)
        -> Result<UpgradeRun, ApplicationError>;
}
```

- [ ] Implement `ProductCommandFacade` and `ProductQueryFacade` as thin dispatchers to existing application services; no repository access or duplicated policy logic.
- [ ] Define canonical public request hashing, receipt construction, error normalization and resource links in application-owned DTOs.
- [ ] Build a parity harness that submits the same normalized command through direct facade, HTTP mapping, CLI mapping and MCP mapping and compares canonical resource/event hashes.
- [ ] Include negative parity cases for denied capability, approval required, version conflict, budget exceeded, operation unknown and cross-workspace access.
- [ ] Add fixture adapters for upload bytes, webhook receiver, backup target and release manifests.
- [ ] Compile-test every port for object safety and Send/Sync.
- [ ] Run and commit:

```bash
cargo test -p vestrace-application surface:: product::
cargo test --test surface_parity
git add crates/vestrace-application tests/surface_parity.rs fixtures/product
git commit -m "feat(product): add shared surface facade and parity harness"
```

### Task 3: Persist public request bindings, schema bundles and release-surface metadata

**Files:** create migration `0081`, product repositories/services and persistence tests.

- [ ] `0081` creates `public_request_bindings`, schema-bundle identities/revisions/artifact links, release-surface capability rows and public-operation resource links.
- [ ] Enforce uniqueness by workspace, principal, surface, operation and idempotency-key hash.
- [ ] `begin` atomically returns `New`, `ExistingSameRequest` or `Conflict`; no command runs before `New` is durably claimed.
- [ ] `complete` stores exact response/resource hash and existing Operation/Run/resource reference.
- [ ] Schema bundle revisions are immutable, artifact-backed and bound to one API major.
- [ ] Persist no Authorization header, bearer token, request body, raw error, trace header or console state.
- [ ] Tests cover concurrent duplicate commands, crash before/after command dispatch, response reconstruction, changed-body conflict and schema-bundle immutability.
- [ ] Run and commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test public_api_idempotency --test schema_compatibility
git add migrations/0081_public_request_bindings_schema_bundles_and_release_surfaces.sql \
  crates/vestrace-infrastructure/src/postgres/product tests
git commit -m "feat(api): persist public request and schema bindings"
```

### Task 4: Complete the versioned HTTP API and durable SSE surface

**Files:** implement HTTP router/middleware/DTO/routes/SSE and contract tests.

- [ ] Mount `/v1` resources for workspaces, conversations/interactions, Runs/steps/events/approvals/artifacts, agents/skills/workflows/tools, triggers/proposals, connections, policies/models/extensions, remote agents, evaluations, audit and operations.
- [ ] Implement explicit commands: Create/Pause/Resume/Cancel Run, SubmitHumanResponse, GrantApproval, RevisePlan, RequestBudgetIncrease, FireTrigger, Accept/RejectRunProposal and CreateEvaluationRun.
- [ ] Return `202` receipts for async commands, `200/201` only when the command completed synchronously by contract, `409` for version/idempotency conflict, `428` when required `If-Match` is absent and `422` for semantic validation.
- [ ] Implement request ID, body/field limits, bearer/local authentication, workspace resolution, capability checks, idempotency and expected-version middleware in deterministic order.
- [ ] Build public error mapping with stable codes and no internal diagnostics.
- [ ] Implement bounded signed pagination tokens tied to workspace, principal, query hash and expiry.
- [ ] Implement SSE from H7 with cursor resume, heartbeat comments, slow-client disconnect, schema validation and exact viewer-policy filtering.
- [ ] Generate immutable ETags for revisioned resources and use weak ETags only for explicitly lagging projections.
- [ ] Test all published operations against OpenAPI examples and negative security cases.
- [ ] Run and commit:

```bash
cargo test -p vestrace-channel-http
cargo test --test public_api_contract --test public_api_concurrency \
           --test public_error_envelope --test public_event_stream
git add crates/vestrace-channel-http tests schemas/openapi schemas/events
git commit -m "feat(api): expose the complete v1 product surface"
```

### Task 5: Implement resumable Artifact upload and authorized range download

**Files:** create migration `0082`, transfer repositories/services and HTTP/CLI/SDK contract tests.

- [ ] `0082` creates upload sessions, immutable part rows, finalization records, download grants and grant-consumption events.
- [ ] Create upload session only after H2 `artifact.upload` authorization and H6 quota/retention checks.
- [ ] Stream each part into H6 staging while hashing; never buffer the whole upload in HTTP or SDK memory.
- [ ] Atomically persist part metadata after staging success. Ambiguous blob promotion uses H6 inspect/reconciliation.
- [ ] Finalize only when ranges are contiguous, expected size/hash match and no session expiry/cancel occurred.
- [ ] Bind finalization to one H6 ingestion session and return Quarantined status plus Artifact reference.
- [ ] Implement authenticated direct range download and one-use grant flow with ETag, `Range` and `If-Range` semantics.
- [ ] Reject active-content inline rendering; set attachment disposition unless a safe H6 preview representation is explicitly requested.
- [ ] Test resume after server restart, duplicate part, changed duplicate, expiration, size/hash mismatch, cross-workspace grant, range abuse and purge after grant issuance.
- [ ] Run and commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test artifact_upload_resume --test artifact_download_range \
             --test artifact_transfer_security
git add migrations/0082_artifact_upload_sessions_parts_and_download_grants.sql \
  crates/vestrace-application/src/transfer crates/vestrace-infrastructure/src/postgres/transfer \
  crates/vestrace-channel-http/src/upload.rs crates/vestrace-channel-http/src/download.rs tests
git commit -m "feat(artifact): add resumable transfer surface"
```

### Task 6: Implement signed outgoing webhooks with durable delivery

**Files:** create migration `0083`, webhook repositories/services/worker/routes and tests.

- [ ] `0083` creates subscription identities/revisions/lifecycle, filters, deliveries, event links, attempts and retry schedule indexes.
- [ ] Create/activate subscription only after exact H2 policy, destination SSRF validation and H8 service-binding compatibility.
- [ ] Build immutable canonical JSON body from authorized H7 public events after viewer/export policy filtering.
- [ ] At attempt time obtain a fresh H8 signing lease, compute the exact HMAC signature and send with `X-Vestrace-Delivery`, `X-Vestrace-Timestamp`, `X-Vestrace-Signature` and event schema bundle headers.
- [ ] Disable redirects, proxies, cookies, DNS rebinding and private/reserved targets unless an explicit local policy allows the exact destination.
- [ ] Store response status and bounded safe code only; never store response body or request signature.
- [ ] Retry the same immutable delivery according to the normative matrix. Reusing event IDs/delivery ID is required.
- [ ] Auto-disable after terminal policy revocation, destination incompatibility or configured consecutive terminal failures; lifecycle change is separately audited.
- [ ] Test signature golden vectors, replay/dedup headers, lost response, retry restart, `429 Retry-After`, SSRF, secret absence and subscription revision pinning.
- [ ] Run and commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test webhook_signature --test webhook_delivery_restart --test webhook_ssrf
git add migrations/0083_webhook_subscriptions_deliveries_and_attempts.sql \
  crates/vestrace-domain/src/webhook crates/vestrace-application/src/webhook \
  crates/vestrace-infrastructure/src/postgres/webhook crates/vestrace-channel-http/src/routes/webhooks.rs tests
git commit -m "feat(webhook): add signed durable event delivery"
```

### Task 7: Complete the MCP surface through the shared facade

**Files:** implement `vestrace-channel-mcp`, MCP schemas/docs and parity tests.

- [ ] Preserve all v0.1 memory/context/model/cognitive/execution tools and map Harness operations through the same application facades.
- [ ] Add agent-facing tools for create/get/follow Run, submit HumanResponse, list pending approvals, grant approval when capability permits, read Artifact metadata/preview, list/get Agent/Skill/Workflow and create EvaluationRun when explicitly allowed.
- [ ] Keep provider/secret/package-policy administration, hard purge, backup/restore and unrestricted export unavailable to ordinary MCP clients.
- [ ] Expose read-only URI resources for memory, Run, step, event, Artifact, Agent, Workflow and Operation with exact workspace scope.
- [ ] Support stdio local-trusted mode and Streamable HTTP bearer mode; transport identity never overrides authenticated principal/workspace.
- [ ] Convert application errors to stable MCP errors with operation/HumanRequest references but no internal fields.
- [ ] Validate tool/resource schemas against `schemas/mcp/v1.json` and prove canonical parity with HTTP/CLI for overlapping commands.
- [ ] Test approval binding, idempotency, version conflict, Unknown, cursor follow and cross-workspace denial.
- [ ] Run and commit:

```bash
cargo test -p vestrace-channel-mcp
cargo test --test mcp_contract --test mcp_policy_parity --test surface_parity
git add crates/vestrace-channel-mcp schemas/mcp docs/mcp.md tests
git commit -m "feat(mcp): expose the H11 agent-facing surface"
```

### Task 8: Build the Rust SDK ergonomic layer

**Files:** create `vestrace-sdk-rust`, docs/examples and SDK contract tests.

- [ ] Generate or include public DTOs from the exact OpenAPI/schema bundle without importing domain or SQL types.
- [ ] Implement `VestraceClient` with explicit base URL, bearer/local mode, timeouts, user agent and no ambient proxy by default.
- [ ] Add ergonomic modules for Runs, operations, SSE, Artifacts, approvals, agents, triggers, evaluations and webhooks.
- [ ] Implement a resumable upload stream from `AsyncRead`, range download stream and H7 cursor reconnect.
- [ ] Implement caller-provided or generated-once idempotency keys; preserve the same key/body across allowed retries.
- [ ] Expose typed `ApprovalRequired`, `VersionConflict`, `BudgetExceeded`, `OperationUnknown` and `RateLimited` errors.
- [ ] Never automatically retry a changed body, Unknown side effect, approval grant, cancel/commit-like command or expired download grant.
- [ ] Add contract tests against the loopback product server and compile examples for create/follow/respond/download.
- [ ] Run and commit:

```bash
cargo test -p vestrace-sdk-rust
cargo test --test rust_sdk_contract
bash scripts/verify-sdk-generation.sh
git add crates/vestrace-sdk-rust docs/sdk-rust.md scripts/verify-sdk-generation.sh
git commit -m "feat(sdk): add the Rust product client"
```

### Task 9: Build the TypeScript SDK for Node and browsers

**Files:** create `packages/sdk-typescript`, generated types, tests and docs.

- [ ] Generate TypeScript public DTOs from the exact OpenAPI/event/JSON schemas and preserve unknown compatible fields.
- [ ] Implement ESM-first client with Node 20+ and evergreen-browser targets; provide no implicit global singleton.
- [ ] Add helpers for Run creation/following, HumanResponse/approval, operation polling, Artifact upload/download, triggers and evaluations.
- [ ] Implement SSE reconnect using cursor and event-ID dedup; abort signals cancel local waiting but do not imply Run cancellation.
- [ ] Stream uploads with bounded chunks; do not read large files wholly into memory.
- [ ] Keep bearer token only in caller-supplied memory; never write browser storage or logs.
- [ ] Apply the same retry classes as the Rust SDK and surface Unknown explicitly.
- [ ] Run contract tests in Node and browser-compatible test environment against deterministic fixtures.
- [ ] Build a versioned npm tarball artifact for release tests; public npm publication is not required.
- [ ] Run and commit:

```bash
npm --prefix packages/sdk-typescript ci
npm --prefix packages/sdk-typescript test
npm --prefix packages/sdk-typescript run build
cargo test --test typescript_sdk_contract
bash scripts/verify-sdk-generation.sh
git add packages/sdk-typescript docs/sdk-typescript.md tests scripts
git commit -m "feat(sdk): add the TypeScript product client"
```

### Task 10: Publish schemas and integrate the three reference Agent Packages

**Files:** create migration `0084`, schemas, reference packages, bootstrap/release services and tests.

- [ ] `0084` creates ProductProfile revisions, bootstrap runs/events, release manifests, reference-package release entries and deployment-installation identity.
- [ ] Generate OpenAPI 3.1, event JSON Schemas, public JSON Schemas, MCP schema, extension schema, A2A reference and product config/release schemas deterministically.
- [ ] Store generated schema bundle as H6 Artifacts and persist one immutable `PublicSchemaBundleRevision`.
- [ ] Build portable packages `vestrace.universal-assistant`, `vestrace.research` and `vestrace.workspace-automation` using only package-local IDs and normal H9 manifests.
- [ ] Universal Assistant provides Direct/Guided coordination, memory-aware context, internal delegation, typed HumanRequest and verification, with default autonomy no greater than Prepare.
- [ ] Research provides read-only source discovery/fetch, source comparison, evidence requirements, internal research SubRuns and optional exact H9A remote-agent delegation.
- [ ] Workspace Automation provides Artifact transformations, sandboxed document rendering, preview, exact approval-bound export and optional schedule definitions that are inactive until explicitly enabled.
- [ ] Include package evaluation fixtures and require exact passing H10 gate evidence before adding each package to a release manifest.
- [ ] Implement Personal/Team/Embedded bootstrap semantics exactly as defined; repeated bootstrap resumes or returns the same result.
- [ ] Test package portability, permission diffs, lock reproducibility, gate expiry, bootstrap restart and no implicit Connection/Trigger/remote activation.
- [ ] Run and commit:

```bash
bash scripts/generate-public-schemas.sh
bash scripts/verify-public-schema-compatibility.sh
bash scripts/verify-reference-package-locks.sh
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test schema_compatibility --test reference_package_install \
             --test reference_package_gate --test product_bootstrap
git add migrations/0084_product_profiles_bootstrap_and_release_manifests.sql \
  schemas packages/reference crates/vestrace-application/src/product \
  crates/vestrace-infrastructure/src/postgres/product tests scripts
git commit -m "feat(product): add schemas and reference packages"
```

### Task 11: Build the minimal secure web console

**Files:** create `apps/console`, console integration, tests and docs.

- [ ] Use only `sdk-typescript`; no console code may call PostgreSQL, internal admin ports or adapter endpoints directly.
- [ ] Implement auth modes `LocalTrusted` and `BearerPrompt`. BearerPrompt token exists only in memory and is cleared on logout/tab close.
- [ ] Add Runs list/detail, plan/step view, event timeline, pending HumanRequests/approvals, Artifact previews/downloads, budget/verification/explanation, Agents/packages, Connections status/auth-start, remote agents, Triggers, evaluations/audit summary and settings/readiness.
- [ ] Require explicit confirmation and current expected version for cancellation, approval, trigger enable/fire, package activation, export and Connection changes.
- [ ] Display exact approval challenge, operation fingerprint summary, affected resource and expiry; never reduce approval to an unscoped yes/no control.
- [ ] Render only safe H6 representations. Untrusted text is escaped; active HTML/SVG is never placed in the DOM.
- [ ] Add CSP with no `unsafe-eval`, no inline scripts, restricted `connect-src`, frame denial and object denial. Referrer policy is `no-referrer`.
- [ ] Implement SSE reconnect and projection-lag indicators without treating the console cache as authority.
- [ ] Meet keyboard navigation, focus management, semantic labels and WCAG AA contrast for release-gate views.
- [ ] Test token non-persistence, XSS payloads, CSRF-inapplicability under bearer/local mode, stale-version confirmation, approval binding, safe preview, SSE reconnect and basic accessibility.
- [ ] Run and commit:

```bash
npm --prefix apps/console ci
npm --prefix apps/console test
npm --prefix apps/console run build
bash scripts/verify-console-security.sh
cargo test --test console_security
git add apps/console docs/console.md scripts/verify-console-security.sh tests/console_security.rs
git commit -m "feat(console): add the minimal secure product UI"
```

### Task 12: Package Personal, Team and Embedded deployments

**Files:** create product configuration, Dockerfiles, Compose profiles, readiness wiring and deployment tests/docs.

- [ ] Generate/validate `schemas/product/config.schema.json`; unknown keys fail unless explicitly namespaced for extensions.
- [ ] Build one Vestrace image with role commands and one static console image; record exact image digests in the release manifest.
- [ ] Personal Compose includes PostgreSQL/pgvector, Vestrace all-in-one or explicit local roles, local Artifact volume, encrypted secret volume and rootless-compatible sandbox manager. Public bind defaults to `127.0.0.1`.
- [ ] Team Compose separates server, scheduler, worker and sandbox manager, supports S3-compatible storage configuration, multiple principals/workspaces and external TLS termination. It refuses wildcard bind without trusted-proxy/TLS acknowledgment.
- [ ] Embedded Compose disables console/bootstrap, enables only configured HTTP/MCP/A2A surfaces and exposes stable health/readiness endpoints.
- [ ] Run containers as non-root, use read-only root filesystem where possible, declare writable paths, cap logs and set explicit health checks/restart policies.
- [ ] Prevent Docker socket from entering server/worker/console containers; only sandbox manager receives the dedicated engine endpoint.
- [ ] Implement startup preflight for schema head, release manifest, Artifact/secret backends, role compatibility, package/schema hashes and required workers.
- [ ] `/health/live` remains shallow; `/health/ready` returns the profile-aware readiness report without secrets.
- [ ] Test all profiles with generated safe config, missing dependencies, wrong migration head, non-loopback Team misconfiguration and degraded optional provider.
- [ ] Run and commit:

```bash
bash scripts/verify-image-boundary.sh
bash scripts/verify-compose-profiles.sh
docker compose -f deploy/compose.personal.yml config
docker compose -f deploy/compose.team.yml config
docker compose -f deploy/compose.embedded.yml config
cargo test --test deployment_profiles --test compose_smoke
git add config deploy docs/deployment-*.md schemas/product/config.schema.json \
  tests scripts .github/workflows/ci.yml
git commit -m "feat(deploy): add self-hosted product profiles"
```

### Task 13: Implement backup, restore, upgrade and operator diagnostics

**Files:** create migration `0085`, maintenance services/repositories/CLI/docs and tests.

- [ ] `0085` creates deployment-installation state, backup/restore/upgrade runs/events, immutable backup manifests, component links, maintenance locks and preflight reports.
- [ ] Implement `vestrace backup create`, `backup verify`, `restore`, `upgrade preflight`, `upgrade apply`, `doctor` and `release inspect` through `ProductMaintenancePort`.
- [ ] Acquire a deployment maintenance lock, reject concurrent restore/upgrade and make repeated idempotency keys return the original operation.
- [ ] Backup enters write quiescence, pauses new work leases, checks active effectful operations and fails safely on unresolved Dispatching/Unknown beyond the configured limit.
- [ ] Capture PostgreSQL snapshot/LSN, Artifact inventory/content hashes, encrypted secret-backend inventory and redacted configuration into H6-backed backup components.
- [ ] Verify every component/hash/reference before marking Ready. The secret master key is excluded and operator instructions name the separate recovery requirement.
- [ ] Restore verifies empty target, release/schema compatibility and all hashes before application; failed restore never marks the target ready.
- [ ] Upgrade preflight requires a verified backup, exact target release manifest, migration path, schema/package/API compatibility and H10 release gates.
- [ ] Apply forward migrations explicitly, restart roles, run doctor/readiness and record the final installed release. No automatic down migration.
- [ ] Test crash at each phase, repeated command, corrupted component, missing Artifact, wrong key inventory, non-empty target, incompatible binary/schema and post-upgrade readiness failure.
- [ ] Run and commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test backup_consistency --test restore_empty_target \
             --test upgrade_preflight --test release_manifest
bash scripts/verify-backup-manifest.sh
bash scripts/verify-release-manifest.sh
git add migrations/0085_backup_restore_and_upgrade_runs.sql \
  crates/vestrace-domain/src/backup crates/vestrace-application/src/backup \
  crates/vestrace-infrastructure/src/postgres/backup crates/vestrace-channel-cli \
  docs/backup-restore.md docs/upgrade.md docs/operations.md tests scripts
git commit -m "feat(ops): add backup restore and upgrade workflows"
```

### Task 14: Assemble and prove the universal vertical slice

**Files:** create deterministic product fixtures, end-to-end orchestration tests and release scenario script.

- [ ] Start the Personal profile with PostgreSQL, local CAS, secret backend, sandbox manager, deterministic OpenAI-compatible provider, read-only research Tool fixture, workspace export fixture, deterministic external A2A agent and webhook receiver.
- [ ] Bootstrap the default workspace/principal, install all three reference packages and explicitly activate the Universal Assistant plus required Research/Workspace Automation revisions after H10 gate checks.
- [ ] Submit the canonical task through the Rust SDK or HTTP:

```text
Изучи варианты решения задачи, сравни их,
подготовь документ и сохрани результат в рабочем пространстве.
```

- [ ] Prove the flow:

```text
Interaction
→ AgentRuntimeSnapshot
→ Guided ExecutionPlan validation
→ H6 memory-aware ContextSnapshot
→ internal Research SubRuns with narrowed grants/budgets
→ protected outbound H9A delegation
→ evidence and source comparison
→ H10 independent verification
→ H4 Docker document render
→ H6 quarantine/inspection/provenance
→ safe preview
→ exact H7/H2 export approval
→ H4 verified workspace export
→ H6 deliverable
→ memory candidates through v0.1 write policy
→ one-use VerifiedRunCompletion
→ terminal outcome and explanation
```

- [ ] Force a complete server/worker/scheduler/sandbox-manager restart after the remote A2A task is accepted and before Artifact finalization. Resume the same Run, SubRuns, remote task and upload/assembly without duplicate effects.
- [ ] Include one InputRequired continuation from the remote A2A agent and resume the same external Task after a typed HumanResponse and fresh H8 lease.
- [ ] Deliver final Run events through SSE and one signed webhook; prove duplicate webhook delivery has the same delivery/event IDs.
- [ ] Verify the exported document hash, Artifact provenance graph, evidence links, budget reconciliation, audit chain, H10 explanation and memory candidate status.
- [ ] Inject failure variants: model response loss, Tool Unknown, remote response loss, sandbox crash, Artifact inspection outage, approval expiry and export response loss. Each must resolve safely or remain explicit Partial/Unknown without false success.
- [ ] Run the same safe read/query/response operations through CLI and MCP and compare canonical state with HTTP.
- [ ] Run with `provider-openai-compatible` and no Rig feature; run the ADR-approved loop feature separately.
- [ ] Execute the scripted scenario and commit fixtures/tests only after all assertions pass.

```bash
bash scripts/run-h11-vertical-slice.sh
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h11_vertical_slice --test h11_restart_matrix
git add fixtures/product tests/h11_vertical_slice.rs tests/h11_restart_matrix.rs \
  scripts/run-h11-vertical-slice.sh
git commit -m "test(product): prove the universal vertical slice"
```

### Task 15: Prove inbound A2A, RLS, schema/release compatibility and final readiness

**Files:** create migration `0086`, final boundary scripts, CI/release workflow, docs and acceptance tests.

- [ ] `0086` forces RLS and same-workspace/principal consistency for H11 tables, append-only release/schema/backup manifests, one-use download grants and indexes for idempotency, upload resume, webhook queue, maintenance locks and release lookup.
- [ ] Publish one exact Universal Assistant `AgentRuntimeSnapshot` through H9A with no push callbacks and a deterministic external client.
- [ ] Prove inbound flow:

```text
external authenticated A2A client
→ Agent Card discovery
→ task/message send
→ durable H9A intake
→ one AgentRun
→ SSE Working
→ InputRequired
→ continuation of same task
→ validated Artifact/result
→ Completed projection
```

- [ ] Verify JSON-RPC and HTTP+JSON return equivalent Task hashes and no A2A Task becomes a Run authority.
- [ ] Add boundary scripts that reject direct repository access from surfaces/console, domain types in SDKs, token persistence, active-content rendering, schema drift, missing idempotency/version checks, unguarded download, webhook secret leakage, auto migration in Team/Embedded, backup master-key inclusion and product state in Run checkpoints.
- [ ] Generate a release manifest with schema/package/image/SBOM hashes and verify it against built artifacts.
- [ ] Generate CycloneDX/SPDX-compatible SBOMs, dependency audit, license report and container vulnerability scan outputs as release artifacts; configured severity policy fails the release.
- [ ] CI jobs:

```text
format/lint/unit
PostgreSQL migrations/RLS
public HTTP/SSE contract
MCP parity
Rust SDK
TypeScript SDK
console security/a11y
Artifact transfer
webhook delivery
reference packages/H10 gates
Personal/Team/Embedded compose
backup/restore/upgrade
outbound vertical slice/restart matrix
inbound A2A
schema/release compatibility
SBOM/dependency/container security
```

- [ ] Run final commands:

```bash
bash scripts/generate-public-schemas.sh
bash scripts/verify-public-schema-compatibility.sh
bash scripts/verify-surface-parity.sh
bash scripts/verify-sdk-generation.sh
bash scripts/verify-console-security.sh
bash scripts/verify-reference-package-locks.sh
bash scripts/verify-release-manifest.sh
bash scripts/verify-image-boundary.sh
bash scripts/verify-compose-profiles.sh
bash scripts/verify-backup-manifest.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features --features provider-openai-compatible
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test product_surface_rls --test public_api_contract \
             --test surface_parity --test h11_vertical_slice \
             --test h11_inbound_a2a --test h11_restart_matrix
npm --prefix packages/sdk-typescript ci
npm --prefix packages/sdk-typescript test
npm --prefix apps/console ci
npm --prefix apps/console test
npm --prefix apps/console run build

git add migrations/0086_product_surface_rls_indexes_and_bindings.sql \
  .github/workflows schemas crates packages apps deploy config docs tests scripts Cargo.toml Cargo.lock
git commit -m "test(release): add H11 product readiness gates"
```

---

## Migration ownership

```text
0081 Task 3  Public request bindings, schema bundles and release-surface metadata
0082 Task 5  Artifact upload sessions/parts and download grants
0083 Task 6  Webhook subscriptions, deliveries and attempts
0084 Task 10 Product profiles, bootstrap, reference release set and release manifests
0085 Task 13 Backup, restore and upgrade operations/manifests
0086 Task 15 RLS, indexes, append-only guards and cross-resource bindings
```

No later task edits an applied migration.

## Public surface matrix

```text
Operation / Query                HTTP  CLI  MCP  Rust SDK  TS SDK  Console  A2A
Create/observe Run                yes   yes  yes     yes      yes      yes    inbound mapping
Pause/resume/cancel Run           yes   yes  yes     yes      yes      yes    cancel mapping only
Submit HumanResponse              yes   yes  yes     yes      yes      yes    continuation mapping
Grant exact approval              yes   yes  gated   yes      yes      yes    no
Read Run events/explanation       yes   yes  yes     yes      yes      yes    Task projection subset
Upload/download Artifact          yes   yes  gated   yes      yes      yes    H9A part mapping
Manage Trigger                    yes   yes  gated   yes      yes      yes    no
Manage Connection                 yes   yes  no      yes      yes      yes    no
Evaluation/audit query            yes   yes  gated   yes      yes      yes    no
Backup/restore/upgrade            admin admin no     optional no       no     no
Remote-agent invocation           indirect through Run/H5/H9A; never a generic Tool command
```

`gated` means absent by default and exposed only when the authenticated MCP client has the exact capability and the operation is safe for the MCP surface.

## H11 completion definition

H11 is complete only when all fifteen tasks pass and the following product flows are proven:

```text
Product bootstrap:
exact release manifest
→ profile-safe configuration
→ migration/readiness checks
→ reference package install
→ explicit activation
→ usable HTTP/CLI/MCP/SDK/console surfaces

Outbound universal task:
interaction
→ durable verified AgentRun
→ internal SubRuns
→ external A2A delegation
→ Tool/Sandbox work
→ Artifact preview
→ exact approval/export
→ memory candidates
→ restart/resume

Inbound A2A:
published exact snapshot
→ authenticated intake
→ one AgentRun
→ stream/input continuation
→ verified Task completion projection

Release:
public schemas
+ reference packages/eval evidence
+ image/SBOM hashes
+ compose profiles
+ backup/restore proof
+ vertical-slice/restart proof
→ immutable ProductReleaseManifest
```

Required invariants:

1. H11 introduces no new execution authority or Run checkpoint schema.
2. HTTP/CLI/MCP use the same application/policy layer.
3. SDKs and console use only public contracts.
4. Every write is idempotent and version-safe where required.
5. Async commands return durable references, not long-held requests.
6. SSE reconnect is cursor-based and idempotent.
7. Public errors contain no internal or sensitive data.
8. Published schemas match runtime and remain backward-compatible within v1.
9. Artifact upload is streaming/resumable/hash-validated.
10. Upload completion enters H6 quarantine, never direct availability.
11. Downloads are exact-revision/range/audience authorized.
12. Webhooks are signed, request-scoped, SSRF-safe and at-least-once with stable IDs.
13. SDKs never retry Unknown by creating a new logical operation.
14. Browser tokens are never persisted by SDK/console.
15. Console renders only safe Artifact representations.
16. Reference packages are portable, permission-reviewed and H10-gated.
17. Bootstrap never creates hidden Connections, Triggers, credentials or remote trust.
18. Personal/Team/Embedded share the same domain contracts.
19. Team/Embedded do not auto-migrate or expose unsafe default binds.
20. Sandbox workloads never receive the Docker socket.
21. Backup manifests are complete, hash-verified and exclude the master key.
22. Restore refuses non-empty/incompatible targets.
23. Upgrade requires verified backup and exact release compatibility.
24. Binary rollback never implies database down-migration.
25. Release manifest pins schema, migration, package, image, lock and SBOM hashes.
26. The outbound vertical slice survives complete runtime restart without duplicate local or remote effects.
27. InputRequired resumes the same remote A2A Task.
28. Unknown model/Tool/remote/export outcomes never produce false success.
29. User deliverable has exact hash, provenance, inspections and verified export state.
30. `Succeeded` consumes exact H10 verification completion.
31. Memory candidates obey v0.1 write policy.
32. Inbound A2A creates one intake/Task/Run and preserves Task projection ownership.
33. HTTP+JSON and JSON-RPC A2A projections are equivalent where required.
34. Audit chain and product explanation remain valid after restart and content purge.
35. Release CI requires no public provider, A2A service, telemetry backend or permanent credential.

## Explicit non-goals

H11 does not implement managed SaaS, public signup, OIDC/browser SSO, billing, Kubernetes, Kafka/NATS, public marketplace, Python SDK, mobile application, unrestricted browser automation, arbitrary raw Artifact rendering, direct SQL dashboards, mandatory A2A gRPC/SLIMRPC/push callbacks, automatic package/Trigger/Connection activation, effectful canary traffic, database down-migrations, in-place destructive restore, cross-region backup orchestration, automatic public registry publication or a full polished commercial UI.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation-only phase, do not create `feat/h11-universal-product-surface`, add React/Vite/SDK/release dependencies, create migrations `0081`–`0086`, generate or publish schemas, build images, start Compose, install or activate packages, expose APIs, issue tokens/download grants/webhooks, create backup/restore/upgrade operations, modify CI or execute H11 tests.