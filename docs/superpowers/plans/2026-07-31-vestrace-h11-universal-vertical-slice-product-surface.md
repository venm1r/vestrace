# Vestrace H11 Universal Vertical Slice and Product Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add frontend/SDK/release dependencies, create migrations, start services, build images, install reference packages, publish schemas, run the vertical slice, create backups, modify CI or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Assemble H1–H10 and H9A into one production-shaped self-hosted Vestrace v0.2 surface with equivalent HTTP/CLI/MCP application behavior, Rust and TypeScript SDKs, resumable Artifact transfer, signed webhooks, a minimal secure web console, reference Agent Packages, Personal/Team/Embedded deployment profiles, upgrade/backup/recovery tooling and restart-safe end-to-end release acceptance.

**Architecture:** H11 is an integration and productization layer, not a new execution authority. Existing H1–H10 and H9A aggregates, journals, policies, budgets, credentials, Artifacts, evaluations and checkpoints remain authoritative. Public transports translate versioned DTOs into shared application commands and queries, then return durable operation/resource references. Rust/TypeScript SDKs and the console are clients of the HTTP/SSE contract, not distinct server-side idempotency domains. Deployment, transfer, webhook and maintenance records have independent durable state; `RunCheckpointV9` is not extended. Portable release manifests contain only stable hashes/assets, while a local installed-release binding maps them to deployment-specific schema/package revisions. Multipart upload uses temporary backend staging references and streams the assembled bytes into the existing H6 lifecycle. Backups use a separate encrypted backup store so they do not depend on the database/Artifact metadata they must restore.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H10 and H9A; Rust Edition 2024; Tokio; Axum/Tower; `rmcp`; Serde/Schemars; SQLx and PostgreSQL 17 with pgvector; H6 Local CAS and S3-compatible Artifact stores; H8 encrypted secret and cryptographic service bindings; Docker/Compose with a rootless-compatible sandbox manager; OpenAPI 3.1 and JSON Schema 2020-12; Server-Sent Events; HMAC-SHA-256 signed webhooks; Ed25519 release/backup manifest signatures; Rust SDK using Reqwest; TypeScript SDK and web console using TypeScript, React and Vite; deterministic local OpenAI-compatible, Tool, A2A, webhook, backup-store and object-store fixtures; cargo-nextest-compatible test layout; CycloneDX/SPDX-compatible SBOM output.

## Global Constraints

- Complete all five v0.1 plans, H1–H10, H9A and the binding ADR-0002 outcome before implementing H11.
- Harness design sections `27. Public API, event stream, MCP и SDK`, `28. Self-hosted product boundary`, `29. Reference Agent Packages`, `30. Обязательный vertical slice`, `31. Release acceptance criteria` and `32. Нормативные security invariants` are normative.
- H11 never creates a second Run, plan, Tool, model, memory, Artifact, approval, policy, budget, credential, remote-agent, evaluation or audit aggregate.
- H1 `AgentRun`, `RunStep`, `RunEvent` and `RunCheckpointV9` remain authoritative. H11 adds no `RunCheckpointV10`.
- Product-surface operations such as upload, webhook delivery, backup, restore and upgrade have independent durable state and do not increment `RunVersion` unless they invoke an explicit existing Run command.
- H7 `PublicEventRecord` and durable workspace cursor remain the only event-stream source. HTTP SSE, SDK streams, CLI follow mode and console timelines are projections over H7.
- H6 remains the only permanent Artifact byte/provenance lifecycle. Upload/download surfaces never create a second logical Artifact identity or bypass quarantine, inspection, export authorization, retention or purge.
- Temporary multipart upload objects are non-authoritative staging objects behind `ArtifactUploadPartStorePort`. They have no public/model identity and are deleted after finalize/expiry/cancel.
- H8 remains the only secret/credential authority. Webhook signing, pagination/download token signing, release signing, backup encryption/signing and external storage credentials use request-scoped cryptographic ports or leases; plaintext keys never cross into application/domain/persistence.
- H9 remains the package/profile/extension authority. Reference packages are installed, resolved, evaluated and activated through H9/H10; release bundling grants nothing.
- H9A remains the A2A gateway. H11 only mounts/configures it, publishes one selected exact snapshot and includes it in release acceptance.
- H10 verification, one-use `VerifiedRunCompletion`, regression evidence, audit and explanation remain mandatory. H11 cannot bypass them for demos, bootstrap or reference packages.
- Public DTOs, generated SDK types and console state are separate from domain, SQLx, provider, adapter and checkpoint types.
- `/v1` is the stable product API prefix. `/admin/v1` remains administrative and capability-gated. No unrestricted state `PATCH` endpoint is introduced.
- Every state-changing HTTP command requires `Idempotency-Key`. Commands changing a versioned/mutable resource require `If-Match` or explicit expected version.
- Idempotency scope is workspace + principal + operation + idempotency-key hash, not transport. The same key/body retried through HTTP, local CLI or MCP returns the original binding; changed canonical content conflicts.
- HTTP is the server-side surface for the Rust SDK, TypeScript SDK and console. SDK/client identity is recorded only as bounded user-agent telemetry and never changes idempotency scope or authority.
- Async commands return `202 Accepted` with an existing `OperationId` or resource ID, current state/version and canonical status/events links.
- Public errors use one bounded envelope and never expose SQL, stack traces, prompts, secrets, raw external errors, internal paths or adapter Debug output.
- SSE uses H7 cursor values. `Last-Event-ID` and `after_cursor` may not disagree. Reconnect is at-least-once and clients deduplicate by event ID.
- HTTP, CLI and MCP call the same application command/query services and produce equivalent canonical state for overlapping operations.
- A2A is not forced into HTTP/MCP parity where wire semantics differ. Equivalence is required at shared application outcomes and references.
- Artifact upload is streaming/resumable, bounded and hash-validated. Completed transfer becomes an H6 quarantined ingestion candidate, never directly Available.
- Upload parts may arrive out of order, but finalization requires the sorted part set to cover exactly `0..total_size` with no gap/overlap.
- Artifact download requires current H2 authorization and either a direct authenticated stream or a one-purpose short-lived grant bound to exact revision/range/audience/expiry.
- Active HTML, scripted SVG, executable content and untrusted rich documents are never rendered directly by the console. Only H6 Preview/RedactedCopy/PageImage or forced attachment download is allowed.
- Outgoing webhooks are at-least-once notifications, not commands. Retries reuse the same immutable body, event IDs and delivery ID.
- Webhook redirects, cookies and ambient proxies are disabled. Destination validation follows H6/H9A SSRF protections; private/reserved targets require an explicit exact local policy.
- Webhook HMAC is produced by `WebhookSigningPort` after consuming an H8-bound signing authorization. Secret bytes never enter the webhook service.
- Reference signature format: `v1=<lowercase hex HMAC-SHA-256>` over `timestamp + "." + delivery_id + "." + exact_body_bytes`.
- SDKs never automatically retry an ambiguous non-idempotent operation. They expose `operation_unknown` and status/reconciliation helpers.
- SDKs do not implement policy, approval matching, budgets, Tool execution or credential resolution locally.
- Browser SDK/console never store bearer tokens in localStorage, sessionStorage or IndexedDB. `BearerPrompt` keeps the token in memory; reload requires re-authentication.
- Initial authentication remains v0.1 local-trusted loopback/stdio and bearer-token HTTP. OIDC, signup and browser SSO remain out of scope.
- Personal console local-trusted writes require same-origin checks plus an ephemeral process-local `X-Vestrace-Console-Nonce`; the nonce is injected into the same-origin console bootstrap, never persisted/logged and rotates on restart.
- Bearer-mode console uses no auth cookies. Exact CORS origins are configured; wildcard origins are rejected.
- Personal deployment binds externally reachable services to loopback by default and refuses wildcard binding without an explicit unsafe-development override or configured TLS termination.
- Team deployment supports multiple principals/workspaces using existing bearer/capability contracts and does not claim OIDC support.
- Embedded deployment disables console and interactive bootstrap by default and exposes only explicitly configured HTTP/MCP/A2A surfaces.
- Configuration precedence remains `CLI → environment → configuration file → safe defaults`. Diagnostics redact bootstrap values and secret references.
- Docker images run non-root where roles permit, use read-only root filesystems where practical, expose explicit writable volumes and contain no build credentials.
- Untrusted sandbox containers never receive the Docker socket. Only the sandbox manager may receive a dedicated rootless-compatible engine endpoint under H4 policy.
- Forward-only migrations remain explicit. Team/Embedded startup never auto-migrates. Personal development auto-migrate requires an explicit setting and is off in release examples.
- Portable ProductReleaseManifest contains no deployment-local UUID. InstalledReleaseBinding maps its stable hashes to local schema/package revisions.
- ProductReleaseManifest is content-hashed and Ed25519-signed; the signing private key remains behind H8/build signing infrastructure.
- Reference release qualification reports are portable assets. Target activation still creates local H10 gate evidence for the exact installed candidate/dependency hash.
- Backups use an encrypted content-addressed `BackupStorePort`, not H6 Artifacts, so restore metadata is available independently of the source database.
- Backup bundles never include clear source secrets, source secret-backend master key or backup-encryption private key.
- Restore release scope is disaster recovery of the recorded deployment identity into an empty replacement target. Cross-deployment cloning is not supported.
- Restore requires the old deployment to be fenced/offline and enters recovery validation before outgoing webhooks, Triggers and external commits are enabled.
- Binary rollback is allowed only when the old binary declares compatibility with the current schema/release manifest. Database down-migrations are not generated.
- Release assets contain exact public-schema, migration, package, image, lock, console/SDK and SBOM hashes.
- Reference packages and release fixtures pass H10 gates before release assembly; passing evidence does not activate packages in existing workspaces.
- Mandatory vertical-slice CI uses deterministic local provider/tool/A2A/webhook/backup/object-store fixtures and requires no public internet, permanent credential or managed service.
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
  product/{mod,profile,release,bootstrap,readiness,error}.rs
  public_api/{mod,version,request,event,pagination,schema}.rs
  transfer/{mod,upload,download}.rs
  webhook/{mod,subscription,delivery,signature}.rs
  backup/{mod,target,object,manifest,run,restore,upgrade}.rs

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
  middleware/{request_id,idempotency,concurrency,error,limits,cors,csp,local_console_nonce}.rs
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
    auth/{mode,token,localNonce}.ts
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
  universal-assistant/{vestrace-package.json,profiles,skills,workflows,policies,schemas,evaluations,examples}/
  research/{vestrace-package.json,profiles,skills,workflows,policies,schemas,evaluations,examples}/
  workspace-automation/{vestrace-package.json,profiles,skills,workflows,policies,schemas,evaluations,examples}/

fixtures/product/{provider,tools,a2a,webhook,object_store,backup_store,vertical_slice}/

config/{vestrace.example,personal,team,embedded}.toml

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
  backup/{mod,target_repository,run_repository,manifest_repository}.rs

migrations/
  0081_public_request_bindings_schema_bundles_and_release_surfaces.sql
  0082_artifact_upload_sessions_parts_and_download_grants.sql
  0083_webhook_subscriptions_deliveries_and_attempts.sql
  0084_product_profiles_bootstrap_and_installed_release_bindings.sql
  0085_backup_targets_restore_and_upgrade_runs.sql
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

### Product profiles and process roles

```rust
pub enum ProductDeploymentProfile { Personal, Team, Embedded }

pub enum ProductProcessRole {
    All,
    Server,
    Scheduler,
    Worker,
    SandboxManager,
    Mcp,
    Migrate,
    Doctor,
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

pub enum ProductAuthenticationMode { LocalTrusted, BearerToken }

pub struct ProductProfileRevision {
    pub id: ProductProfileRevisionId,
    pub profile: ProductDeploymentProfile,
    pub revision: u32,
    pub enabled_roles: std::collections::BTreeSet<ProductProcessRole>,
    pub enabled_surfaces: std::collections::BTreeSet<ProductSurface>,
    pub authentication_modes: std::collections::BTreeSet<ProductAuthenticationMode>,
    pub default_authentication_mode: ProductAuthenticationMode,
    pub artifact_backend_kind: String,
    pub secret_backend_kind: String,
    pub sandbox_required: bool,
    pub console_enabled: bool,
    pub safe_defaults_hash: [u8; 32],
    pub content_hash: [u8; 32],
}
```

Personal may combine LocalTrusted and BearerToken but LocalTrusted HTTP is loopback-only. Team uses BearerToken. Embedded defaults to BearerToken and may enable local-trusted stdio MCP separately.

### Portable release manifest and local installation binding

```rust
pub enum ReleaseAssetKind {
    Binary,
    ConsoleBundle,
    PublicSchemaBundle,
    ReferencePackage,
    QualificationReport,
    RustSdkArchive,
    TypeScriptSdkArchive,
    Sbom,
    LicenseReport,
}

pub struct ReleaseAssetDescriptor {
    pub kind: ReleaseAssetKind,
    pub logical_name: String,
    pub media_type: String,
    pub byte_size: u64,
    pub sha256: [u8; 32],
}

pub struct ReleaseReferencePackageAsset {
    pub stable_id: String,
    pub version: String,
    pub package_archive: ReleaseAssetDescriptor,
    pub package_manifest_digest: [u8; 32],
    pub qualification_report: ReleaseAssetDescriptor,
}

pub struct ProductReleaseSignature {
    pub algorithm: String,
    pub key_revision: String,
    pub signature: Vec<u8>,
}

pub struct ReleaseCompatibilityRange {
    pub minimum_schema_head: String,
    pub maximum_schema_head: String,
    pub minimum_public_api_major: u16,
    pub maximum_public_api_major: u16,
}

pub struct ProductReleaseManifest {
    pub id: ProductReleaseManifestId,
    pub product_version: String,
    pub source_revision: String,
    pub rust_toolchain: String,
    pub cargo_lock_hash: [u8; 32],
    pub migration_head: String,
    pub public_api_major: u16,
    pub public_schema_bundle_hash: [u8; 32],
    pub config_schema_hash: [u8; 32],
    pub reference_packages: Vec<ReleaseReferencePackageAsset>,
    pub assets: Vec<ReleaseAssetDescriptor>,
    pub container_image_digests: Vec<String>,
    pub compatibility: ReleaseCompatibilityRange,
    pub content_hash: [u8; 32],
    pub signature: ProductReleaseSignature,
    pub created_at: Timestamp,
}

pub struct InstalledReleaseBinding {
    pub id: InstalledReleaseBindingId,
    pub deployment_id: DeploymentInstallationId,
    pub release_manifest_id: ProductReleaseManifestId,
    pub public_schema_bundle_revision_id: PublicSchemaBundleRevisionId,
    pub installed_package_revision_ids: Vec<AgentPackageRevisionId>,
    pub installed_package_lock_ids: Vec<AgentPackageLockId>,
    pub installed_at: Timestamp,
}
```

The portable manifest has no local Artifact/package/schema UUID. `content_hash` excludes `signature`; signature is Ed25519 over a domain-separated canonical tuple containing the content hash and product version. Image digests use `sha256:<64 lowercase hex>`.

### Public request identity and cross-surface idempotency

```rust
pub enum PublicSurfaceKind { Http, Cli, Mcp, A2A }

pub struct PublicRequestBinding {
    pub id: PublicRequestBindingId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub first_surface: PublicSurfaceKind,
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
    Bootstrap(ProductBootstrapRunId),
    Backup(BackupRunId),
    Restore(RestoreRunId),
    Upgrade(UpgradeRunId),
    Connection(ConnectionId),
    PackageActivation(PackageActivationRevisionId),
}

pub enum PublicCommandStatus {
    Accepted,
    Completed,
    WaitingForInput,
    WaitingForApproval,
    Rejected,
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
```

Uniqueness is `(workspace_id, principal_id, operation_name, idempotency_key_hash)`; `first_surface` is audit metadata only. Idempotency keys are 8–200 visible ASCII bytes. Canonical request hash includes API major, operation, authenticated scope, normalized body and expected version; it excludes authorization/trace/user-agent/transport metadata.

### Public errors and events

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

pub struct PublicErrorDetail {
    pub path: Option<String>,
    pub code: String,
    pub safe_message: String,
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

pub struct PublicErrorEnvelope {
    pub request_id: PublicRequestId,
    pub error: PublicErrorBody,
}

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

Messages are bounded to 8 KiB, details to 32 × 2 KiB. Event `data` validates against the exact event-type schema. SSE `id` is cursor, `event` is event type and heartbeat comments do not advance the cursor.

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
    pub portable_bundle_hash: [u8; 32],
    pub compatibility_baseline_hash: [u8; 32],
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Local schema artifacts are imported into the deployment system workspace and bound to the portable bundle hash from the release manifest. Within `/v1`, removing required fields, changing type/meaning, narrowing accepted input or changing operation IDs is breaking.

### Application command/query DTOs and facade

```rust
pub struct BeginPublicRequest {
    pub surface: PublicSurfaceKind,
    pub operation_name: String,
    pub idempotency_key: String,
    pub canonical_request_hash: [u8; 32],
}

pub enum PublicRequestDisposition {
    New { binding_id: PublicRequestBindingId },
    ExistingSameRequest { binding: PublicRequestBinding },
    Conflict,
}

pub struct CompletePublicRequest {
    pub binding_id: PublicRequestBindingId,
    pub operation_id: Option<OperationId>,
    pub resource: Option<PublicResourceRef>,
    pub response_hash: [u8; 32],
}

pub enum ProductCommand {
    CreateRun(CreateRunCommand),
    PauseRun(PauseRunCommand),
    ResumeRun(ResumeRunCommand),
    CancelRun(CancelRunCommand),
    SubmitHumanResponse(SubmitHumanResponseCommand),
    GrantApproval(GrantApprovalCommand),
    RevisePlan(RevisePlanCommand),
    RequestBudgetIncrease(RequestBudgetIncreaseCommand),
    FireTrigger(FireTriggerCommand),
    AcceptRunProposal(AcceptRunProposalCommand),
    RejectRunProposal(RejectRunProposalCommand),
    CreateEvaluationRun(CreateEvaluationRunCommand),
    VerifyAuditIntegrity(VerifyAuditIntegrityCommand),
    CreateWebhookSubscription(CreateWebhookSubscriptionCommand),
    DisableWebhookSubscription(DisableWebhookSubscriptionCommand),
}

pub enum ProductQuery {
    GetRun { run_id: AgentRunId },
    ListRuns { filter: RunListFilter, page: PublicPageRequest },
    GetRunEvents { run_id: AgentRunId, after: Option<PublicEventCursor> },
    GetOperation { operation_id: OperationId },
    GetArtifact { revision_id: ArtifactRevisionId },
    GetAgent { agent_revision_id: AgentRevisionId },
    GetTrigger { trigger_id: TriggerDefinitionId },
    GetConnection { connection_id: ConnectionId },
    GetEvaluation { evaluation_run_id: EvaluationRunId },
    GetRunExplanation { run_id: AgentRunId },
    GetReadiness,
}

pub struct ProductQueryResult {
    pub schema_id: String,
    pub canonical_value: serde_json::Value,
    pub content_hash: [u8; 32],
    pub resource_version: Option<u64>,
}

#[async_trait::async_trait]
pub trait ProductCommandFacade: Send + Sync {
    async fn execute(&self, context: &RequestContext, command: ProductCommand)
        -> Result<PublicCommandReceipt, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ProductQueryFacade: Send + Sync {
    async fn query(&self, context: &RequestContext, query: ProductQuery)
        -> Result<ProductQueryResult, ApplicationError>;
}
```

Command payload types through `VerifyAuditIntegrityCommand` are owned by prior plans. H11 defines the two webhook commands, public paging/filter values and query projection registry. `canonical_value` is produced by a closed schema-specific projector, not arbitrary SQL/JSON.

### Multipart upload staging and finalization

```rust
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpaqueUploadPartStorageRef(String);

impl std::fmt::Debug for OpaqueUploadPartStorageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpaqueUploadPartStorageRef([REDACTED])")
    }
}

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
    pub storage_ref: OpaqueUploadPartStorageRef,
    pub received_at: Timestamp,
}

pub struct StagedUploadPart {
    pub storage_ref: OpaqueUploadPartStorageRef,
    pub content_hash: ArtifactContentHash,
    pub byte_size: u64,
}

#[async_trait::async_trait]
pub trait ArtifactUploadPartStorePort: Send + Sync {
    async fn stage(&self, request: StageUploadPartRequest)
        -> Result<StagedUploadPart, ArtifactTransferError>;
    async fn open(&self, reference: &OpaqueUploadPartStorageRef)
        -> Result<ArtifactByteStream, ArtifactTransferError>;
    async fn delete(&self, reference: &OpaqueUploadPartStorageRef)
        -> Result<(), ArtifactTransferError>;
}
```

Arrival order is arbitrary. Finalization sorts by ordinal/range, rejects gaps/overlaps, opens parts sequentially and streams one combined body into H6 `StageArtifactBlobRequest`. Default part size is 8 MiB; range is 1–64 MiB.

Application transfer DTOs:

```rust
pub struct CreateArtifactUpload {
    pub declared_media_type: Option<String>,
    pub original_filename: Option<String>,
    pub expected_size: Option<u64>,
    pub expected_hash: Option<ArtifactContentHash>,
    pub requested_part_size: Option<u64>,
}

pub struct AppendArtifactUploadPart {
    pub session_id: ArtifactUploadSessionId,
    pub ordinal: u32,
    pub byte_start: u64,
    pub body: ArtifactByteStream,
}

pub struct FinalizeArtifactUpload {
    pub session_id: ArtifactUploadSessionId,
    pub expected_logical_revision: u64,
}

pub struct IssuedArtifactDownloadGrant {
    pub grant: ArtifactDownloadGrant,
    pub clear_token: String,
}
```

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

Default uses is one, lifetime five minutes. Clear token is returned once and never persisted. ETag is exact content hash; `If-Range` follows HTTP semantics.

### Webhook support types and cryptographic boundary

```rust
pub struct NormalizedWebhookDestination {
    pub origin: NormalizedOrigin,
    pub path: String,
}

pub struct WebhookResourceFilter {
    pub resource_kind: String,
    pub stable_id_hash: Option<[u8; 32]>,
}

pub struct WebhookRetryPolicy {
    pub maximum_attempts: u16,
    pub maximum_age_seconds: u64,
    pub base_delay_seconds: u64,
    pub maximum_delay_seconds: u64,
}

pub enum WebhookAttemptOutcome {
    Delivered,
    RetryableFailure,
    TerminalFailure,
    UnknownResponse,
    Cancelled,
}

pub enum WebhookSubscriptionLifecycle { Draft, Active, Disabled, Revoked }
pub enum WebhookDeliveryStatus { Pending, Delivering, Delivered, RetryScheduled, Failed, Disabled }

pub struct WebhookSubscriptionRevision {
    pub id: WebhookSubscriptionRevisionId,
    pub subscription_id: WebhookSubscriptionId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub destination: NormalizedWebhookDestination,
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

pub struct SignWebhookRequest {
    pub service_binding_revision_id: ServiceCredentialBindingRevisionId,
    pub timestamp: Timestamp,
    pub delivery_id: WebhookDeliveryId,
    pub exact_body: bytes::Bytes,
}

pub struct WebhookSignature { pub header_value: String }

#[async_trait::async_trait]
pub trait WebhookSigningPort: Send + Sync {
    async fn sign(&self, request: SignWebhookRequest)
        -> Result<WebhookSignature, ApplicationError>;
}
```

The signing adapter consumes H8 authorization/lease at the enforcement point and returns only the bounded signature. One delivery contains 1–100 events, ≤1 MiB. Attempts cap at 12/24h. `2xx` succeeds; `408/425/429/5xx` retry; other `4xx` fail. Lost response retries the same immutable delivery.

Webhook commands:

```rust
pub struct CreateWebhookSubscriptionCommand {
    pub destination: NormalizedWebhookDestination,
    pub event_type_filters: std::collections::BTreeSet<String>,
    pub resource_filters: Vec<WebhookResourceFilter>,
    pub signing_service_binding_revision_id: ServiceCredentialBindingRevisionId,
    pub maximum_classification: DataClassification,
}

pub struct DisableWebhookSubscriptionCommand {
    pub subscription_id: WebhookSubscriptionId,
    pub expected_revision: u64,
}
```

### SDK retry contract

```rust
pub enum SdkRetryClass {
    Never,
    SafeRead,
    SameIdempotentCommand,
    StatusOrReconciliationOnly,
}
```

Reads/range/SSE use SafeRead. SameIdempotentCommand preserves exact endpoint/body/key. Unknown effect/finalization uses status/reconciliation only. A new retry key is forbidden.

### Reference package release and local qualification

```rust
pub struct ReferencePackageInstallationBinding {
    pub release_manifest_id: ProductReleaseManifestId,
    pub stable_id: String,
    pub package_revision_id: AgentPackageRevisionId,
    pub package_lock_id: AgentPackageLockId,
    pub local_regression_gate_evidence_id: RegressionGateEvidenceId,
    pub content_hash: [u8; 32],
}
```

Required stable IDs are `vestrace.universal-assistant`, `vestrace.research`, `vestrace.workspace-automation`. Portable archive/qualification hashes live in the release manifest; local H9/H10 IDs live only in installation bindings.

### Product bootstrap

```rust
pub enum ProductBootstrapStatus {
    Created,
    ImportingReleaseAssets,
    CreatingWorkspace,
    CreatingPrincipal,
    InstallingReferencePackages,
    RunningLocalQualification,
    ApplyingSafePolicies,
    Ready,
    Failed,
}

pub struct ProductBootstrapRun {
    pub id: ProductBootstrapRunId,
    pub deployment_profile_revision_id: ProductProfileRevisionId,
    pub installed_release_binding_id: InstalledReleaseBindingId,
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

Personal bootstrap creates default workspace/principal, installs packages, runs local exact qualification and may activate only Universal Assistant under Prepare autonomy. Team creates administrative scope but activates no workspace package. Embedded creates no interactive principal/activation.

### Encrypted backup store and maintenance contracts

```rust
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpaqueBackupObjectRef(String);

pub enum BackupObjectKind {
    DatabaseSnapshot,
    ArtifactBlob,
    ArtifactInventory,
    SecretBackendCiphertext,
    RedactedConfiguration,
    AuditVerificationReport,
}

pub struct BackupObjectDescriptor {
    pub kind: BackupObjectKind,
    pub object_ref: OpaqueBackupObjectRef,
    pub plaintext_hash: [u8; 32],
    pub ciphertext_hash: [u8; 32],
    pub byte_size: u64,
    pub encryption_profile_revision: String,
}

pub struct BackupTargetRevision {
    pub id: BackupTargetRevisionId,
    pub target_id: BackupTargetId,
    pub revision: u32,
    pub backend_kind: String,
    pub safe_destination_ref: String,
    pub service_binding_revision_id: Option<ServiceCredentialBindingRevisionId>,
    pub encryption_service_binding_revision_id: ServiceCredentialBindingRevisionId,
    pub signing_service_binding_revision_id: ServiceCredentialBindingRevisionId,
    pub content_hash: [u8; 32],
}

pub struct BackupManifestSeal {
    pub algorithm: String,
    pub key_revision: String,
    pub signature: Vec<u8>,
}

pub struct BackupManifest {
    pub id: BackupManifestId,
    pub source_deployment_id: DeploymentInstallationId,
    pub product_release_manifest_hash: [u8; 32],
    pub database_schema_head: String,
    pub database_snapshot_lsn: String,
    pub artifact_generation: u64,
    pub objects: Vec<BackupObjectDescriptor>,
    pub content_hash: [u8; 32],
    pub seal: BackupManifestSeal,
    pub created_at: Timestamp,
}

pub enum ProductMaintenanceOperationStatus {
    Created,
    Preflight,
    Quiescing,
    Capturing,
    Verifying,
    Ready,
    Applying,
    RecoveryValidation,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

pub struct BackupRun {
    pub id: BackupRunId,
    pub deployment_id: DeploymentInstallationId,
    pub target_revision_id: BackupTargetRevisionId,
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
    pub source_deployment_fenced: bool,
    pub operation_id: OperationId,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

pub struct UpgradeRun {
    pub id: UpgradeRunId,
    pub deployment_id: DeploymentInstallationId,
    pub from_release_manifest_id: ProductReleaseManifestId,
    pub to_release_manifest_id: ProductReleaseManifestId,
    pub preflight_report_hash: [u8; 32],
    pub backup_manifest_id: BackupManifestId,
    pub status: ProductMaintenanceOperationStatus,
    pub operation_id: OperationId,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

#[async_trait::async_trait]
pub trait BackupStorePort: Send + Sync {
    async fn put(&self, request: PutBackupObject)
        -> Result<BackupObjectDescriptor, BackupStoreError>;
    async fn inspect(&self, reference: &OpaqueBackupObjectRef)
        -> Result<BackupObjectObservation, BackupStoreError>;
    async fn open(&self, reference: &OpaqueBackupObjectRef)
        -> Result<BackupByteStream, BackupStoreError>;
    async fn delete(&self, reference: &OpaqueBackupObjectRef)
        -> Result<(), BackupStoreError>;
}

#[async_trait::async_trait]
pub trait BackupCryptographyPort: Send + Sync {
    async fn encrypt(&self, request: EncryptBackupStream)
        -> Result<EncryptedBackupStream, ApplicationError>;
    async fn decrypt(&self, request: DecryptBackupStream)
        -> Result<BackupByteStream, ApplicationError>;
    async fn seal_manifest(&self, content_hash: [u8; 32], binding: ServiceCredentialBindingRevisionId)
        -> Result<BackupManifestSeal, ApplicationError>;
}
```

Application maintenance DTOs:

```rust
pub struct CreateBackup {
    pub deployment_id: DeploymentInstallationId,
    pub target_revision_id: BackupTargetRevisionId,
    pub maximum_quiesce_seconds: u64,
}

pub struct BackupVerificationReport {
    pub manifest_id: BackupManifestId,
    pub valid_signature: bool,
    pub all_objects_present: bool,
    pub all_hashes_match: bool,
    pub compatible_release: bool,
    pub content_hash: [u8; 32],
}

pub struct CreateRestore {
    pub target_deployment_id: DeploymentInstallationId,
    pub source_manifest_id: BackupManifestId,
    pub source_deployment_fenced: bool,
}

pub struct CreateUpgrade {
    pub deployment_id: DeploymentInstallationId,
    pub target_release_manifest_id: ProductReleaseManifestId,
    pub backup_target_revision_id: BackupTargetRevisionId,
}
```

Backups are encrypted before `BackupStorePort`. Manifest seal is Ed25519. Restore is disaster recovery only; it verifies empty target, source identity/fencing, signature, object hashes and release compatibility. RecoveryValidation keeps Triggers, webhooks and external commits disabled until doctor checks pass.

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

pub enum ReadinessCheckStatus { Healthy, Degraded, Failed, Unknown }

pub struct ProductReadinessCheck {
    pub kind: ReadinessCheckKind,
    pub required: bool,
    pub status: ReadinessCheckStatus,
    pub safe_code: String,
    pub observed_revision: Option<String>,
}

pub struct ProductReadinessReport {
    pub product_release_manifest_id: ProductReleaseManifestId,
    pub profile_revision_id: ProductProfileRevisionId,
    pub checks: Vec<ProductReadinessCheck>,
    pub ready: bool,
    pub generated_at: Timestamp,
}
```

`/health/live` is process liveness only. `/health/ready` requires all profile-required checks; optional providers/remotes may be degraded unless active package/profile requires them.

---

### Task 1: Add product, public-surface, transfer, webhook and maintenance contracts

**Files:** create H11 domain modules, modify IDs/exports and add unit/property tests.

**Produces:** every H11 value/aggregate above.

- [ ] Add IDs: `ProductProfileRevisionId`, `ProductReleaseManifestId`, `InstalledReleaseBindingId`, `DeploymentInstallationId`, `PublicRequestId`, `PublicRequestBindingId`, `PublicSchemaBundleId`, `PublicSchemaBundleRevisionId`, `ArtifactUploadSessionId`, `ArtifactDownloadGrantId`, `WebhookSubscriptionId`, `WebhookSubscriptionRevisionId`, `WebhookDeliveryId`, `WebhookDeliveryAttemptId`, `ProductBootstrapRunId`, `BackupTargetId`, `BackupTargetRevisionId`, `BackupManifestId`, `BackupRunId`, `RestoreRunId`, `UpgradeRunId`.
- [ ] Test transition closure for upload/webhook/bootstrap/maintenance including Unknown and RecoveryValidation.
- [ ] Golden-test canonical hashes/signatures for public request, schema bundle, release manifest, webhook signature input and backup manifest.
- [ ] Prove portable release/backup manifests reject deployment-local schema/package/Artifact UUIDs except their own local persistence ID.
- [ ] Prove H11 state cannot serialize into RunCheckpointV9 or increment RunVersion alone.
- [ ] Validate profile defaults, release assets/digests, idempotency bounds, errors, upload ranges, grant lifetime, webhook limits, backup target and empty/fenced restore.
- [ ] Implement all normative types.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain product:: public_api:: transfer:: webhook:: backup::
git add crates/vestrace-domain
git commit -m "feat(product): add H11 surface and release contracts"
```

### Task 2: Define shared facades, cryptographic/staging ports and parity fixtures

**Files:** create application product/surface/transfer/webhook/backup modules and deterministic test support.

- [ ] Implement all application DTOs/ports defined above plus `PublicRequestBindingPort`, `PublicSchemaRegistryPort`, `ArtifactTransferPort`, `ProductMaintenancePort`.
- [ ] `ProductCommandFacade` delegates only to existing command services; query facade uses closed schema projectors, never repositories/raw SQL.
- [ ] Add `ArtifactUploadPartStorePort`, `WebhookSigningPort`, `BackupStorePort`, `BackupCryptographyPort`, release-signing and pagination/download-token signing ports.
- [ ] Build parity fixtures that submit identical command/key/body through direct facade, HTTP mapper, local CLI mapper and MCP mapper and compare canonical resource/event hashes.
- [ ] Negative parity: denied capability, approval required, version conflict, budget exceeded, Unknown and cross-workspace access.
- [ ] Add faults after idempotency claim, existing command commit, upload-part stage, webhook body persist, backup object put and manifest seal.
- [ ] Compile-test object safety/Send/Sync and redacted Debug for opaque refs.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application surface:: product:: transfer:: webhook:: backup::
cargo test --test surface_parity
git add crates/vestrace-application tests/surface_parity.rs fixtures/product
git commit -m "feat(product): add shared facades and boundary ports"
```

### Task 3: Persist cross-surface idempotency, schema bundles and release bindings

**Files:** create `0081`, repositories/services/tests.

- [ ] `0081` creates public request bindings, schema bundle identities/revisions/artifact links, release manifest rows/signatures, installed-release bindings and public-operation resource links.
- [ ] Uniqueness excludes surface and is `(workspace, principal, operation, idempotency hash)`.
- [ ] `begin` atomically returns New/ExistingSameRequest/Conflict before command execution.
- [ ] `complete` stores exact response/resource hash and existing Operation/Run/resource reference.
- [ ] Portable manifest bytes/signature are verified before persistence; installed binding validates local schema/package hashes.
- [ ] Schema bundle revisions are immutable, deployment-system-workspace Artifact-backed and bound to one portable hash/API major.
- [ ] Persist no auth header/token/body/raw error/trace/user-agent/console state.
- [ ] Test concurrent cross-surface duplicate, crash before/after command commit, changed body, manifest signature and local binding mismatch.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test public_api_idempotency --test schema_compatibility --test release_manifest
git add migrations/0081_public_request_bindings_schema_bundles_and_release_surfaces.sql \
  crates/vestrace-infrastructure/src/postgres/product tests
git commit -m "feat(api): persist request schema and release bindings"
```

### Task 4: Complete `/v1` HTTP, admin and durable SSE surfaces

**Files:** implement HTTP router/middleware/DTO/routes/SSE/tests.

- [ ] Mount `/v1` resources for workspaces, conversations/interactions, Runs/steps/events/approvals/artifacts, agents/skills/workflows/tools, triggers/proposals, connections, policies/models/extensions/remotes, evaluations/audit/operations/webhooks/schemas.
- [ ] Keep provider/secret/hard-purge/backup/upgrade endpoints under capability-gated `/admin/v1`.
- [ ] Implement explicit Run/HumanResponse/Approval/Plan/Budget/Trigger/Proposal/Evaluation/Webhook commands; no unrestricted PATCH.
- [ ] Return `202` async receipts; `409` conflict, `428` missing precondition, `422` semantic validation and bounded error envelopes.
- [ ] Middleware order: request ID → transport/auth validation → scope → body limits → idempotency/precondition → application facade → error/receipt mapping.
- [ ] LocalTrusted browser writes require exact Origin/Host/Sec-Fetch-Site and process-local console nonce. Bearer requests use exact CORS allowlist and no credentials cookies.
- [ ] Signed pagination tokens use H8 signing port and bind workspace/principal/query/expiry.
- [ ] SSE uses H7 cursor, heartbeat comments, slow-client disconnect, viewer filtering and event-schema validation.
- [ ] ETags are immutable revision/content hashes; weak ETags only for lagging projections.
- [ ] Contract-test every published operation/example and security negative case.
- [ ] Run/commit:

```bash
cargo test -p vestrace-channel-http
cargo test --test public_api_contract --test public_api_concurrency \
           --test public_error_envelope --test public_event_stream
git add crates/vestrace-channel-http tests schemas/openapi schemas/events
git commit -m "feat(api): expose the complete v1 product surface"
```

### Task 5: Implement resumable upload and exact authorized download

**Files:** create `0082`, staging/repositories/services/HTTP/CLI/SDK tests.

- [ ] `0082` creates upload sessions, immutable part rows with opaque storage refs, finalization records, download grants and consumption events.
- [ ] Create session after H2 `artifact.upload` and H6 quota/retention checks.
- [ ] Stage each bounded part through `ArtifactUploadPartStorePort` while hashing; HTTP/SDK never buffer whole files.
- [ ] Persist part after stage success. Same ordinal/range/hash is idempotent; changed duplicate conflicts.
- [ ] On finalize, sort parts and require exact contiguous coverage; stream open parts into one H6 stage request and verify expected size/hash.
- [ ] Ambiguous final H6 promote uses inspect/reconciliation; status becomes Quarantined only after durable H6 binding.
- [ ] Delete temporary parts after verified finalize or expiry/cancel; cleanup is idempotent.
- [ ] Implement authenticated range download and one-use grant with ETag/Range/If-Range.
- [ ] Force attachment unless exact safe preview representation requested.
- [ ] Test out-of-order resume/restart, duplicate/conflict, missing range, expiry, hash/size mismatch, cross-workspace, purge and ambiguous promote.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test artifact_upload_resume --test artifact_download_range \
             --test artifact_transfer_security
git add migrations/0082_artifact_upload_sessions_parts_and_download_grants.sql \
  crates/vestrace-application/src/transfer crates/vestrace-infrastructure/src/postgres/transfer \
  crates/vestrace-channel-http/src/upload.rs crates/vestrace-channel-http/src/download.rs tests
git commit -m "feat(artifact): add resumable transfer surface"
```

### Task 6: Implement signed durable outgoing webhooks

**Files:** create `0083`, webhook repositories/services/worker/routes/tests.

- [ ] `0083` creates subscription identities/revisions/lifecycle, filters, deliveries, event links, attempts/retry indexes.
- [ ] Activate after H2 policy, destination SSRF validation and H8 signing binding compatibility.
- [ ] Build immutable canonical JSON from authorized H7 public events after classification/export filtering.
- [ ] Immediately before send, call `WebhookSigningPort`; send delivery/timestamp/signature/schema headers.
- [ ] Disable redirects/proxies/cookies, re-resolve/pin DNS and reject private/reserved destinations unless exact local policy.
- [ ] Store response status/safe code only; no body/signature/header.
- [ ] Retry same immutable delivery under matrix; lost response is UnknownResponse and schedules same delivery.
- [ ] Auto-disable only by explicit configured terminal conditions; audit lifecycle change.
- [ ] Test golden signature, receiver dedup, restart, 429, DNS rebinding/SSRF, secret absence and revision pinning.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test webhook_signature --test webhook_delivery_restart --test webhook_ssrf
git add migrations/0083_webhook_subscriptions_deliveries_and_attempts.sql \
  crates/vestrace-domain/src/webhook crates/vestrace-application/src/webhook \
  crates/vestrace-infrastructure/src/postgres/webhook crates/vestrace-channel-http/src/routes/webhooks.rs tests
git commit -m "feat(webhook): add signed durable event delivery"
```

### Task 7: Complete MCP through the shared facade

**Files:** implement channel, schemas/docs/parity tests.

- [ ] Preserve v0.1 memory/context/model/cognitive/execution tools.
- [ ] Add create/get/follow Run, HumanResponse, pending approvals, exact grant when capable, Artifact metadata/preview, Agent/Skill/Workflow and Evaluation tools.
- [ ] Exclude provider/secret/package-policy admin, hard purge, backup/restore and unrestricted export from ordinary MCP.
- [ ] Expose read-only memory/Run/step/event/Artifact/Agent/Workflow/Operation resources.
- [ ] Support stdio LocalTrusted and Streamable HTTP BearerToken; transport metadata grants nothing.
- [ ] Map application errors safely and validate `schemas/mcp/v1.json`.
- [ ] Prove parity for overlapping commands including cross-surface idempotency.
- [ ] Test approvals, versions, Unknown, cursor follow and RLS.
- [ ] Run/commit:

```bash
cargo test -p vestrace-channel-mcp
cargo test --test mcp_contract --test mcp_policy_parity --test surface_parity
git add crates/vestrace-channel-mcp schemas/mcp docs/mcp.md tests
git commit -m "feat(mcp): expose the H11 agent-facing surface"
```

### Task 8: Build the Rust SDK

**Files:** create SDK/docs/examples/tests.

- [ ] Generate/include public types from exact schemas; no domain/SQL types.
- [ ] Implement explicit-base-URL `VestraceClient`, bearer/local config, timeouts, no ambient proxy.
- [ ] Modules: Runs, operations, SSE, Artifacts, approvals, agents, triggers, evaluations, webhooks.
- [ ] Stream upload from `AsyncRead`, range download and cursor reconnect.
- [ ] Generate idempotency key once per command object or accept caller key; preserve exact key/body on allowed retry.
- [ ] Typed ApprovalRequired/VersionConflict/BudgetExceeded/OperationUnknown/RateLimited.
- [ ] Never auto-retry changed body, Unknown, approval grant, cancel/commit-like command or expired grant.
- [ ] Loopback contract tests/examples.
- [ ] Run/commit:

```bash
cargo test -p vestrace-sdk-rust
cargo test --test rust_sdk_contract
bash scripts/verify-sdk-generation.sh
git add crates/vestrace-sdk-rust docs/sdk-rust.md scripts/verify-sdk-generation.sh
git commit -m "feat(sdk): add the Rust product client"
```

### Task 9: Build the TypeScript SDK

**Files:** create package/generated types/tests/docs.

- [ ] Generate types from OpenAPI/events/JSON schemas and preserve compatible unknown fields.
- [ ] ESM-first Node 20+/evergreen browser client; no singleton.
- [ ] Helpers for Runs, responses/approvals, operations, Artifacts, triggers/evaluations.
- [ ] Implement bearer-capable SSE with `fetch` streaming/parser rather than native EventSource; reconnect by cursor/event dedup.
- [ ] Browser upload uses bounded `Blob.slice`; Node uses streams; neither buffers full file.
- [ ] Caller-supplied token remains memory-only; no storage/logging.
- [ ] Same retry classes and explicit Unknown.
- [ ] Node/browser-compatible contract tests and versioned npm tarball release artifact.
- [ ] Run/commit:

```bash
npm --prefix packages/sdk-typescript ci
npm --prefix packages/sdk-typescript test
npm --prefix packages/sdk-typescript run build
cargo test --test typescript_sdk_contract
bash scripts/verify-sdk-generation.sh
git add packages/sdk-typescript docs/sdk-typescript.md tests scripts
git commit -m "feat(sdk): add the TypeScript product client"
```

### Task 10: Generate schemas, reference packages, profiles and bootstrap

**Files:** create `0084`, schemas/packages/bootstrap/install bindings/tests.

- [ ] `0084` creates ProductProfile revisions, bootstrap runs/events, deployment installation and local reference-package installation bindings.
- [ ] Generate OpenAPI 3.1, event/JSON/MCP/extension/A2A/config/release schemas deterministically and import the exact bundle into the system workspace.
- [ ] Build portable packages `vestrace.universal-assistant`, `vestrace.research`, `vestrace.workspace-automation` using package-local IDs.
- [ ] Universal: Direct/Guided, memory context, internal delegation, HumanRequest, verification, autonomy ≤ Prepare.
- [ ] Research: read-only source tools, comparison/evidence, internal SubRuns and optional exact H9A delegation.
- [ ] Workspace Automation: Artifact transforms, Docker render, preview, exact approval/export; schedules inactive.
- [ ] Produce portable qualification report assets during release evaluation, but target install runs local exact H10 gates before activation.
- [ ] Implement profile bootstrap semantics; repeated bootstrap resumes/returns same result.
- [ ] Personal may activate only locally qualified Universal; Team/Embedded activate none by default.
- [ ] Test package portability, permission diff, locks, local gate expiry, restart and no hidden Connection/Trigger/credential/remote trust.
- [ ] Run/commit:

```bash
bash scripts/generate-public-schemas.sh
bash scripts/verify-public-schema-compatibility.sh
bash scripts/verify-reference-package-locks.sh
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test schema_compatibility --test reference_package_install \
             --test reference_package_gate --test product_bootstrap
git add migrations/0084_product_profiles_bootstrap_and_installed_release_bindings.sql \
  schemas packages/reference crates/vestrace-application/src/product \
  crates/vestrace-infrastructure/src/postgres/product tests scripts
git commit -m "feat(product): add schemas reference packages and bootstrap"
```

### Task 11: Build the minimal secure console

**Files:** create React/Vite app/tests/docs.

- [ ] Use only TypeScript SDK; no DB/internal repository/adapter calls.
- [ ] Auth modes LocalTrusted and BearerPrompt. Bearer stays memory-only.
- [ ] LocalTrusted obtains same-origin process nonce from bootstrap and sends it on writes; exact Origin/Host/Sec-Fetch enforcement.
- [ ] Pages: Runs/detail/plan/events/HumanRequests/approvals, safe Artifact previews, budget/verification/explanation, Agents/packages, Connections status/auth start, remotes, Triggers, evaluations/audit summary, readiness/settings.
- [ ] Explicit confirmation + expected version for cancel/approval/trigger/package/export/Connection changes.
- [ ] Show exact approval challenge/fingerprint/resource/expiry; no unscoped yes/no.
- [ ] Escape text; active HTML/SVG never enters DOM; only safe H6 representations.
- [ ] CSP no unsafe-eval/inline script, restricted connect-src, frame/object deny, no-referrer.
- [ ] SSE reconnect and projection-lag indicators; cache is not authority.
- [ ] Keyboard/focus/labels/WCAG AA for release views.
- [ ] Test token/nonce non-persistence, origin/nonce CSRF defense, XSS, stale version, approval binding, preview, reconnect, accessibility.
- [ ] Run/commit:

```bash
npm --prefix apps/console ci
npm --prefix apps/console test
npm --prefix apps/console run build
bash scripts/verify-console-security.sh
cargo test --test console_security
git add apps/console docs/console.md scripts/verify-console-security.sh tests/console_security.rs
git commit -m "feat(console): add the minimal secure product UI"
```

### Task 12: Package Personal, Team and Embedded release candidates

**Files:** config, Dockerfiles, Compose, readiness, release assembly/tests/docs.

- [ ] Validate config schema; unknown keys fail except extension namespaces.
- [ ] Build one role-capable Vestrace image and static console image; non-root/read-only boundaries.
- [ ] Personal: PostgreSQL, local Artifact/secret volumes, rootless-compatible sandbox manager, loopback bind.
- [ ] Team: split server/scheduler/worker/sandbox, S3 config, multiple principals/workspaces, external TLS termination; reject unsafe wildcard.
- [ ] Embedded: console/bootstrap disabled and only configured surfaces.
- [ ] Docker socket only at sandbox manager dedicated endpoint.
- [ ] Startup preflight for schema, release binding, Artifact/secret, role, package/schema hashes and workers.
- [ ] Generate SDK archives, console bundle, CycloneDX/SPDX SBOMs, license report and image digests.
- [ ] Assemble/sign immutable ProductReleaseManifest from portable assets; persist local InstalledReleaseBinding only after signature/hash validation.
- [ ] `/health/live` shallow; `/health/ready` profile-aware and secret-free.
- [ ] Test profiles, missing dependencies, wrong schema, unsafe bind, degraded optional provider and manifest mismatch.
- [ ] Run/commit:

```bash
bash scripts/verify-image-boundary.sh
bash scripts/verify-compose-profiles.sh
bash scripts/verify-release-manifest.sh
docker compose -f deploy/compose.personal.yml config
docker compose -f deploy/compose.team.yml config
docker compose -f deploy/compose.embedded.yml config
cargo test --test deployment_profiles --test compose_smoke --test release_manifest
git add config deploy docs/deployment-*.md schemas/product \
  tests scripts .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "feat(deploy): assemble self-hosted release profiles"
```

### Task 13: Implement encrypted backup, disaster recovery, upgrade and doctor

**Files:** create `0085`, backup target/store/crypto services/repositories/CLI/docs/tests.

- [ ] `0085` creates backup target revisions, backup/restore/upgrade runs/events, immutable manifest metadata, object descriptors, maintenance locks and preflight reports.
- [ ] Commands: backup create/verify, restore, upgrade preflight/apply, doctor, release inspect.
- [ ] Acquire deployment maintenance lock; idempotent repeats return same operation.
- [ ] Quiesce writes/new leasing, wait DB transactions and fail safely if effectful Dispatching/Unknown remains beyond limit.
- [ ] Capture PostgreSQL snapshot/LSN and all permanent Artifact blobs referenced by snapshot plus encrypted secret-backend ciphertext inventory/redacted config/audit verification.
- [ ] Encrypt every backup object through BackupCryptographyPort before BackupStorePort; seal manifest.
- [ ] Verify signature, presence, plaintext/ciphertext hashes and release compatibility before Ready.
- [ ] Restore only to empty replacement with source deployment fenced; apply DB/blobs/ciphertexts and enter RecoveryValidation.
- [ ] During RecoveryValidation disable Triggers, outgoing webhooks and external commits until doctor validates Artifact/secret/audit/release/workers and operator confirms fencing.
- [ ] Upgrade requires verified backup, signed target manifest, migration path, API/package/schema compatibility and H10 gates; no down migration.
- [ ] Test phase crashes, duplicate command, corruption, missing object, wrong encryption binding, non-empty/unfenced target, incompatible schema and failed recovery validation.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test backup_consistency --test restore_empty_target \
             --test upgrade_preflight --test release_manifest
bash scripts/verify-backup-manifest.sh
bash scripts/verify-release-manifest.sh
git add migrations/0085_backup_targets_restore_and_upgrade_runs.sql \
  crates/vestrace-domain/src/backup crates/vestrace-application/src/backup \
  crates/vestrace-infrastructure/src/postgres/backup crates/vestrace-channel-cli \
  docs/backup-restore.md docs/upgrade.md docs/operations.md tests scripts
git commit -m "feat(ops): add encrypted backup recovery and upgrade"
```

### Task 14: Assemble and prove the outbound universal vertical slice

**Files:** deterministic product fixtures/E2E tests/script.

- [ ] Start Personal release candidate with DB/local CAS/secret/sandbox, deterministic provider, read-only research tool, export fixture, external A2A agent and webhook receiver.
- [ ] Bootstrap and locally qualify/activate Universal plus required Research/Workspace Automation revisions.
- [ ] Submit canonical task:

```text
Изучи варианты решения задачи, сравни их,
подготовь документ и сохрани результат в рабочем пространстве.
```

- [ ] Prove:

```text
Interaction
→ AgentRuntimeSnapshot
→ Guided validated plan
→ memory-aware ContextSnapshot
→ narrowed internal Research SubRuns
→ protected outbound A2A delegation
→ evidence/source comparison
→ H10 independent verification
→ Docker document render
→ H6 quarantine/inspection/provenance
→ safe preview
→ exact export approval
→ verified workspace export
→ deliverable + memory candidates
→ one-use VerifiedRunCompletion
→ terminal outcome/explanation
```

- [ ] Restart server/worker/scheduler/sandbox manager after remote Task acceptance and before Artifact finalize; resume same Run/SubRuns/remote Task/assemblies with no duplicates.
- [ ] Include remote InputRequired and resume same Task after typed response/fresh H8 lease.
- [ ] Deliver final events via SSE and signed webhook; duplicate notification retains IDs/body.
- [ ] Verify deliverable hash/provenance/evidence/budget/audit/explanation/memory candidate.
- [ ] Failure variants: model loss, Tool Unknown, remote loss, sandbox crash, inspection outage, approval expiry, export response loss. Resolve safely or remain Partial/Unknown.
- [ ] Compare safe query/response operations through HTTP/CLI/MCP.
- [ ] Run Rig-free provider-openai-compatible path and ADR-approved loop path separately.
- [ ] Run/commit:

```bash
bash scripts/run-h11-vertical-slice.sh
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h11_vertical_slice --test h11_restart_matrix
git add fixtures/product tests/h11_vertical_slice.rs tests/h11_restart_matrix.rs \
  scripts/run-h11-vertical-slice.sh
git commit -m "test(product): prove the universal vertical slice"
```

### Task 15: Prove inbound A2A, RLS, compatibility and final release readiness

**Files:** create `0086`, scripts, CI/release workflow, docs/tests.

- [ ] `0086` forces RLS/scope consistency, append-only release/schema/backup manifests, one-use grants and indexes for idempotency/upload/webhook/maintenance/release.
- [ ] Publish exact Universal snapshot via H9A without push; deterministic external client proves Agent Card → intake → one Run → SSE → InputRequired → same-task continuation → validated completion.
- [ ] Prove JSON-RPC/HTTP+JSON equivalent Task hashes and projection-only Task ownership.
- [ ] Boundary scripts reject repository access from surfaces/console, domain types in SDK, token/nonce persistence, active rendering, schema drift, missing idempotency/preconditions, unguarded download, webhook secret leakage, unsafe auto-migrate, backup key inclusion, H6-backed backup circularity and H11 state in Run checkpoints.
- [ ] Reproduce release manifest from assets and verify signature/schema/package/image/SDK/console/SBOM/license hashes.
- [ ] Dependency/license/container scans produce release artifacts and fail configured severity/policy.
- [ ] CI jobs: format/lint/unit; migrations/RLS; HTTP/SSE; MCP parity; Rust/TS SDK; console; transfer; webhook; package/H10 gates; Compose; backup/restore/upgrade; outbound/restart; inbound A2A; schema/release; supply-chain scans.
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
0081 Task 3  Public request bindings, schema bundles, release manifests and local installation bindings
0082 Task 5  Multipart upload sessions/parts and download grants
0083 Task 6  Webhook subscriptions, deliveries and attempts
0084 Task 10 Product profiles, bootstrap and reference-package installation bindings
0085 Task 13 Backup targets, objects, restore and upgrade operations
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
Read events/explanation           yes   yes  yes     yes      yes      yes    Task projection subset
Upload/download Artifact          yes   yes  gated   yes      yes      yes    H9A part mapping
Manage Trigger                    yes   yes  gated   yes      yes      yes    no
Manage Connection                 yes   yes  no      yes      yes      yes    no
Evaluation/audit query            yes   yes  gated   yes      yes      yes    no
Backup/restore/upgrade            admin admin no     optional no       no     no
Remote-agent invocation           indirect through Run/H5/H9A; never a generic Tool command
```

SDK and console columns are HTTP clients; they do not create separate server authority/idempotency scope. `gated` means absent by default and requires exact MCP capability/safety eligibility.

## H11 completion definition

```text
Product bootstrap:
signed portable release manifest
→ local installed-release binding
→ profile-safe config/readiness
→ package import + local qualification
→ explicit activation
→ HTTP/CLI/MCP/SDK/console

Outbound task:
interaction
→ durable verified Run
→ internal SubRuns + external A2A
→ Tool/Sandbox
→ preview + exact approval/export
→ memory candidates
→ restart/resume

Inbound A2A:
published exact snapshot
→ authenticated intake
→ one Run
→ stream/input continuation
→ verified Task projection

Release/recovery:
portable schemas/packages/SDK/console/images/SBOM
→ signed manifest
→ Compose profiles
→ encrypted backup/restore proof
→ vertical-slice/restart proof
```

Required invariants:

1. No new execution authority or Run checkpoint schema.
2. HTTP/CLI/MCP share application/policy services.
3. SDK/console use public HTTP/SSE contracts only.
4. Idempotency is cross-surface for the same principal/workspace/operation.
5. Writes are version-safe where required.
6. Async commands return durable references.
7. SSE reconnect is cursor/idempotent.
8. Errors contain no internals/secrets.
9. Schemas match runtime and remain v1-compatible.
10. Upload is streaming/resumable/hash-validated with restart-accessible opaque staging refs.
11. Final upload enters H6 quarantine; temporary parts are cleaned.
12. Downloads are exact revision/range/audience authorized.
13. Webhook signing secret never reaches application; delivery is SSRF-safe/stable-ID at-least-once.
14. SDKs never retry Unknown as a new operation.
15. Browser bearer token/local nonce are not persisted.
16. LocalTrusted browser writes have origin + nonce CSRF defense.
17. Console renders only safe H6 representations.
18. Portable release manifest has no deployment-local UUID and is signed.
19. Local installed binding exactly matches portable hashes.
20. Reference packages are portable, permission-reviewed, locally H10-qualified.
21. Bootstrap creates no hidden Connection/Trigger/credential/remote trust.
22. Personal/Team/Embedded share domain contracts and safe bind/migration defaults.
23. Sandbox workloads never receive Docker socket.
24. Backup is independent of H6/source DB, encrypted, hash-verified and excludes master keys.
25. Restore is empty-target disaster recovery with source fencing/recovery validation.
26. Upgrade requires verified backup and signed compatible release; no down migration.
27. Release manifest pins schemas/migrations/packages/images/SDK/console/SBOM/license assets.
28. Outbound slice survives full restart without duplicate local/remote effects.
29. InputRequired resumes same remote Task.
30. Unknown model/Tool/remote/export never yields false success.
31. Deliverable has hash/provenance/inspections/verified export.
32. Succeeded consumes exact H10 completion.
33. Memory candidates obey v0.1 write policy.
34. Inbound A2A creates one intake/Task/Run and projection ownership.
35. Audit/explanation survive restart/purge.
36. CI needs no public provider/A2A/telemetry service/permanent credential.

## Explicit non-goals

H11 does not implement managed SaaS, public signup, OIDC/browser SSO, billing, Kubernetes, Kafka/NATS, public marketplace, Python SDK, mobile app, unrestricted browser automation, raw active Artifact rendering, direct SQL dashboards, mandatory A2A gRPC/SLIMRPC/push, automatic package/Trigger/Connection activation, effectful canary traffic, database down-migrations, in-place destructive restore, cross-deployment cloning, cross-region backup orchestration, public registry publication or polished commercial UI.

## Documentation-only boundary

Creating this document does not authorize implementation. During documentation-only work, do not create `feat/h11-universal-product-surface`, add React/Vite/SDK/release dependencies, create migrations `0081`–`0086`, generate/publish schemas, build images, start Compose, install/activate packages, expose APIs, issue tokens/download grants/webhooks, create backup/restore/upgrade operations, modify CI or execute H11 tests.