# Vestrace H8 Connections and Credential Broker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start authorization/proxy services, store credentials, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement immutable Connector definitions, user/workspace-bound Connections, Authorization Code + PKCE and API-key onboarding, replaceable secret storage, operation-bound credential leases, delegated Connection access, request-scoped injection for providers/tools/remote agents, a sandbox credential proxy, rotation/revocation, known-secret controls and content-free audit.

**Architecture:** PostgreSQL owns Connector/Connection/service-binding state, permission revisions, authorization-flow metadata, opaque secret references, grants, leases and audit. `SecretBackendPort` owns immutable secret bytes but cannot authorize their use. H2 separately authorizes the protected action and `secret.use`; the Broker verifies both tickets against one shared `credential_binding_hash`, then issues a short lease bound to the exact actor, Run/invocation, source generation, operation, resources and destination. `vestrace-credential-runtime` consumes the lease immediately before dispatch, resolves zeroizing material and injects/signs the infrastructure request. Models, public DTOs, work items, durable events and sandbox workloads receive only sanitized logical handles.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H7 Rust workspace; Rust Edition 2024; Tokio; Axum/Tower; SQLx and PostgreSQL 17; SHA-256 and HMAC-SHA-256; OAuth Authorization Code with PKCE S256; non-serializable zeroizing secret wrappers; RustCrypto XChaCha20-Poly1305 for the initial local encrypted backend; deterministic loopback OAuth/resource/remote/proxy fixtures; proptest; tracing with mandatory redaction.

## Global Constraints

- Complete all five v0.1 plans and H1–H7 before implementing H8.
- Harness design section `20. Connector Registry and Credential Broker` is normative.
- PostgreSQL is authoritative for Connector definitions/revisions, Connections, immutable permission-ceiling revisions, service credential bindings/revisions, authorization flows, credential-set references, rotations, validations, grants, remote profiles, leases/uses, proxy sessions, leak findings and audit metadata.
- Secret bytes, OAuth codes, PKCE verifiers, access/refresh tokens, API keys, client secrets, passwords and private/signing keys live only behind `SecretBackendPort` and are never PostgreSQL values.
- Vestrace owns every domain/application type, ID, lifecycle, schema, hash and public DTO. External OAuth/HTTP/keyring/cloud/A2A SDK types remain inside adapters.
- Connector, adapter-binding, backend-binding, Connection ceiling, service-binding, remote-profile and credential-generation revisions are immutable. Mutable lifecycle belongs to stable identity rows with optimistic state revisions.
- A Connection is a logical authorization relationship, not a secret. It is principal-bound or workspace-bound and points to exact Connector, permission and credential revisions.
- External scopes and remote security declarations are compatibility evidence, not local authority.
- Effective use is the intersection of Connector/service operation requirements, Connection/service-binding ceiling, sharing, delegated grant, current H2 policy, classification, destination and exact protected action.
- `SecretReference`, `SecretVersionReference`, Connection ID, lease ID and content hash are never bearer capabilities.
- Clear secret material is non-Clone, non-Serialize and non-Debug and zeroizes on drop. Secret refs use custom redacted Debug.
- Authorization Code + PKCE S256 is the initial user OAuth flow. Implicit/password-grant flows are rejected. Clear state is one-time; PostgreSQL stores only HMAC.
- OAuth codes and PKCE verifiers are short-retention exact secret versions. Raw token responses are never persisted.
- Ambiguous token exchange becomes `AuthorizationFlowStatus::Unknown`; the code is not blindly exchanged again. Reauthorization is required unless connector-specific reconciliation proves an already stored generation.
- API-key submission uses a dedicated sensitive HTTP/CLI path and never becomes an InteractionEvent, argv, access-log field or error payload.
- Refresh tokens/client secrets are broker-internal and cannot be selected by model/tool input or ordinary injection.
- Rotation creates one new immutable generation, validates it, atomically activates a new binding/revision and revokes unexhausted leases tied to old generations.
- Revocation blocks new leases immediately but cannot recall an already dispatched request.
- Credential leases default to one use and at most 60 seconds. Longer/multi-use leases are nonstandard and excluded from reference profiles.
- Lease consumption occurs at the last practical enforcement point. An exhausted lease is never reused after crash, timeout or ambiguous completion.
- H3 provider, H4 tool, H5 remote-agent, H6 authenticated fetch/store and H9A adapters use the same lease/runtime-injection boundary.
- Models see only `LogicalConnectionHandle` plus sanitized operation metadata. They never see backend refs, generations, lease IDs, proxy capabilities or secret fragments.
- Sandboxes receive no long-lived credential through env, argv, mounts or Artifacts. H8 uses an application-level HTTP credential proxy over an H4-managed local channel.
- Proxy requests strip caller authentication/cookie/proxy-auth headers before injection.
- Remote authentication binds exact local remote revision/profile, verified identity/origin, transport, operation, resources and classification. Agent Cards cannot widen it.
- SubRuns and remote invocations use explicit `ConnectionAccessGrant`; no child inherits a parent Connection.
- Known-secret scans return only ref hashes, rule IDs, ranges and irreversible fingerprints.
- Secret scanning is applied before model transfer, ordinary event persistence, notification rendering, Artifact availability/export and connector diagnostics according to subsystem policy.
- Usage and lifecycle audits are append-only and content-free. H10 may add integrity chains without rewriting H8 history.
- Replay never starts authorization, reads secret bytes, refreshes/rotates, issues/consumes leases, injects, calls a Connector or uses the proxy.
- Existing migrations `0014`–`0053` are never edited. H8 migrations are `0054`–`0060`, each owned once.
- CI uses loopback fixtures, temporary encrypted stores and fake clocks; no public provider/SaaS/remote agent/permanent credential is required.
- Future implementation branch: `feat/h8-connections-credential-broker`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  connector/{mod,definition,operation,resource,scope,binding}.rs
  connection/{mod,connection,owner,sharing,permission,status,validation}.rs
  credential/{mod,reference,set,authorization,rotation,grant,lease,audit,leak}.rs
  remote_connection/{mod,profile}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  connector/{mod,ports,commands,registry}.rs
  connection/{mod,ports,commands,service,validation,revocation}.rs
  credential/{mod,ports,commands,authorization,oauth_callback,api_key,broker,lease,refresh,rotation,injection,delegation,audit,leak,worker}.rs
  remote_connection/{mod,ports,service}.rs

crates/vestrace-credential-runtime/src/
  lib.rs
  secret_material.rs
  lease_consumer.rs
  http_injector.rs
  request_signer.rs
  remote_decorator.rs
  redaction.rs

crates/vestrace-secret-backend-local/src/{lib,backend,crypto,layout,staging,master_key,reconcile}.rs
crates/vestrace-credential-proxy/src/{lib,server,session,request_policy,injector,response}.rs
crates/vestrace-credential-test-support/src/{lib,secret_backend,oauth_server,resource_server,connector,remote,proxy,scanner,fixtures,conformance}.rs
crates/vestrace-channel-http/src/{connection_routes,authorization_routes,oauth_callback,sensitive_body}.rs
crates/vestrace-channel-cli/src/{connection_commands,secure_input}.rs

crates/vestrace-infrastructure/src/postgres/
  connector/{mod,definition_repository,binding_repository}.rs
  connection/{mod,repository,sharing_repository,validation_repository}.rs
  credential/{mod,authorization_repository,binding_repository,rotation_repository,grant_repository,lease_repository,audit_repository,leak_repository}.rs
  remote_connection/{mod,profile_repository}.rs

migrations/
  0054_connector_definitions_operations_and_backend_bindings.sql
  0055_connections_service_bindings_sharing_scopes_validation.sql
  0056_authorization_flows_credential_bindings_and_rotations.sql
  0057_connection_access_grants_and_remote_profiles.sql
  0058_credential_leases_uses_and_proxy_sessions.sql
  0059_secret_leak_findings_and_credential_audit.sql
  0060_connection_credential_rls_indexes_and_run_bindings.sql

tests/
  connector_definition_persistence.rs
  connection_persistence.rs
  connection_validation.rs
  secret_backend_local_conformance.rs
  oauth_pkce_flow.rs
  oauth_exchange_unknown.rs
  api_key_submission.rs
  credential_refresh_singleflight.rs
  credential_rotation_restart.rs
  connection_revocation.rs
  connection_access_grants.rs
  remote_connection_profiles.rs
  credential_lease_atomicity.rs
  credential_lease_restart.rs
  provider_credential_injection.rs
  tool_credential_injection.rs
  authenticated_artifact_fetch.rs
  sandbox_credential_proxy.rs
  remote_agent_credential_injection.rs
  authentication_continuation.rs
  secret_leak_detection.rs
  credential_audit_persistence.rs
  connection_credential_rls.rs
  h8_acceptance.rs

scripts/
  verify-secret-boundary.sh
  verify-credential-lease-boundary.sh
  verify-sandbox-credential-boundary.sh
  verify-credential-persistence.sh
```

---

## Normative contracts

### Shared supporting values

```rust
pub enum ConnectorDefinitionStatus { Draft, Active, Deprecated, Revoked }
pub enum ConnectorAdapterKind { Builtin, Http, Mcp, Extension }
pub enum SecretBackendKind { LocalEncrypted, External }
pub enum ServiceCredentialBindingStatus { Active, Disabled, Revoked, Expired }
pub enum ConnectionValidationStatus { Valid, Degraded, Invalid, ReauthorizationRequired }
pub enum ConnectionAccessGrantStatus { Issued, Exhausted, Expired, Revoked }
pub enum SandboxCredentialProxyStatus { Issued, Active, Exhausted, Expired, Revoked }
pub enum CredentialLeaseStatus { Issued, Exhausted, Expired, Revoked, Failed }
pub enum CredentialUseResult { MaterialResolved, Injected, RequestDispatched, RejectedBeforeDispatch, CompletionUnknown, Failed }

pub struct ConnectorDataHandlingDeclaration {
    pub allowed_classifications: std::collections::BTreeSet<DataClassification>,
    pub stores_external_content: bool,
    pub sends_content_to_third_parties: bool,
    pub declared_retention_days: Option<u32>,
}

pub struct ConnectorAdapterBindingRevision {
    pub id: ConnectorAdapterBindingRevisionId,
    pub revision: u32,
    pub kind: ConnectorAdapterKind,
    pub adapter_revision: String,
    pub configuration: serde_json::Value,
    pub content_hash: [u8; 32],
}

pub struct SecretBackendBindingRevision {
    pub id: SecretBackendBindingRevisionId,
    pub revision: u32,
    pub kind: SecretBackendKind,
    pub configuration: serde_json::Value,
    pub content_hash: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct LogicalConnectionHandle(String);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedOrigin(String);

pub enum SecretReadPurpose {
    ConsumedCredentialLease { lease_id: CredentialLeaseId },
    OAuthTokenExchange { flow_id: AuthorizationFlowId },
    OAuthRefresh { refresh_id: CredentialRefreshId },
    ConnectionValidation { validation_id: ConnectionValidationId },
    ConnectorRevocation { connection_id: ConnectionId },
    KnownSecretScan { scan_id: SecretLeakScanId },
}
```

`NormalizedOrigin` is exactly `scheme://host:effective-port`, contains no userinfo/path/query/fragment and rejects non-HTTPS origins except explicit local loopback policy. All free identifiers/paths/origins are bounded and normalized.

### Connector definitions and operations

```rust
pub enum ConnectorAuthenticationScheme {
    OAuth2AuthorizationCodePkce,
    ApiKeyHeader { header_name: String },
    StaticBearer,
    Basic,
    ClientCertificate,
    RequestSigner { scheme: String },
}

pub struct ConnectorExternalScope {
    pub stable_name: String,
    pub description: String,
    pub sensitive: bool,
}

pub struct ConnectorResourcePattern {
    pub kind: String,
    pub pattern: String,
}

pub struct ConnectorOperationDefinition {
    pub id: ConnectorOperationId,
    pub name: String,
    pub side_effect_class: ToolSideEffectClass,
    pub risk: RiskLevel,
    pub required_external_scopes: std::collections::BTreeSet<String>,
    pub allowed_resource_patterns: Vec<ConnectorResourcePattern>,
    pub allowed_authentication_schemes: Vec<ConnectorAuthenticationScheme>,
    pub maximum_request_bytes: u64,
    pub maximum_response_bytes: u64,
    pub supports_idempotency_key: bool,
    pub supports_reconciliation: bool,
}

pub struct ConnectorDefinition {
    pub id: ConnectorDefinitionId,
    pub stable_name: String,
    pub workspace_id: Option<WorkspaceId>,
    pub status: ConnectorDefinitionStatus,
    pub current_revision_id: ConnectorDefinitionRevisionId,
    pub state_revision: u64,
}

pub struct ConnectorDefinitionRevision {
    pub id: ConnectorDefinitionRevisionId,
    pub connector_definition_id: ConnectorDefinitionId,
    pub revision: u32,
    pub display_name: String,
    pub adapter_binding_revision_id: ConnectorAdapterBindingRevisionId,
    pub authentication_schemes: Vec<ConnectorAuthenticationScheme>,
    pub external_scopes: Vec<ConnectorExternalScope>,
    pub operations: Vec<ConnectorOperationDefinition>,
    pub connection_configuration_schema: serde_json::Value,
    pub data_handling: ConnectorDataHandlingDeclaration,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Stable names use lowercase dotted segments. Credential header targets use a fixed allowlist: `Authorization` and approved `X-Api-Key`-style names are possible; `Cookie`, `Proxy-Authorization`, `Host`, `Content-Length` and hop-by-hop headers are forbidden.

### Connection and immutable permission revisions

```rust
pub enum ConnectionOwner {
    Principal { principal_id: PrincipalId },
    Workspace,
}

pub enum ConnectionStatus {
    PendingAuthorization,
    Active,
    Degraded,
    ReauthorizationRequired,
    Disabled,
    Revoked,
    Expired,
}

pub enum ConnectionSharingMode { OwnerOnly, ExplicitPrincipals, WorkspaceAgents }

pub struct ConnectionPermissionCeilingRevision {
    pub id: ConnectionPermissionCeilingRevisionId,
    pub connection_id: ConnectionId,
    pub revision: u32,
    pub allowed_operations: std::collections::BTreeSet<ConnectorOperationId>,
    pub allowed_resources: Vec<ConnectorResourcePattern>,
    pub allowed_external_scopes: std::collections::BTreeSet<String>,
    pub maximum_classification: DataClassification,
    pub maximum_risk: RiskLevel,
    pub content_hash: [u8; 32],
}

pub struct Connection {
    pub id: ConnectionId,
    pub workspace_id: WorkspaceId,
    pub connector_revision_id: ConnectorDefinitionRevisionId,
    pub owner: ConnectionOwner,
    pub status: ConnectionStatus,
    pub sharing_mode: ConnectionSharingMode,
    pub current_permission_revision_id: ConnectionPermissionCeilingRevisionId,
    pub current_credential_binding_id: Option<ConnectionCredentialBindingId>,
    pub external_identity_hash: Option<[u8; 32]>,
    pub external_identity_display: Option<String>,
    pub state_revision: u64,
    pub last_validation_id: Option<ConnectionValidationId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct ServiceCredentialBinding {
    pub id: ServiceCredentialBindingId,
    pub workspace_id: WorkspaceId,
    pub stable_name: String,
    pub status: ServiceCredentialBindingStatus,
    pub current_revision_id: ServiceCredentialBindingRevisionId,
    pub state_revision: u64,
}

pub struct ServiceCredentialBindingRevision {
    pub id: ServiceCredentialBindingRevisionId,
    pub binding_id: ServiceCredentialBindingId,
    pub revision: u32,
    pub purpose: String,
    pub credential_set_generation_id: CredentialSetGenerationId,
    pub allowed_operations: std::collections::BTreeSet<String>,
    pub allowed_origins: Vec<NormalizedOrigin>,
    pub maximum_classification: DataClassification,
    pub content_hash: [u8; 32],
}
```

Connection transitions:

```text
PendingAuthorization → Active | ReauthorizationRequired | Disabled | Revoked
Active → Degraded | ReauthorizationRequired | Disabled | Revoked | Expired
Degraded → Active | ReauthorizationRequired | Disabled | Revoked | Expired
ReauthorizationRequired → PendingAuthorization | Disabled | Revoked | Expired
Disabled → PendingAuthorization | Active | Revoked | Expired, only by protected command
Revoked and Expired are terminal
```

A new ceiling or service-binding revision never rewrites history. Connection sharing selects locally eligible principals/agents but every use still passes H2.

### Secret backend and credential generations

```rust
#[derive(Clone, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SecretReference {
    pub backend_binding_revision_id: SecretBackendBindingRevisionId,
    pub secret_id: SecretId,
}

pub struct SecretVersionReference {
    pub secret: SecretReference,
    pub version_id: SecretVersionId,
    pub generation: u64,
}

pub enum CredentialComponentKind {
    AccessToken,
    RefreshToken,
    ApiKey,
    BearerToken,
    Username,
    Password,
    ClientSecret,
    PrivateKey,
    ClientCertificate,
}

pub struct CredentialSetGeneration {
    pub id: CredentialSetGenerationId,
    pub credential_set_id: CredentialSetId,
    pub workspace_id: WorkspaceId,
    pub generation: u64,
    pub components: std::collections::BTreeMap<CredentialComponentKind, SecretVersionReference>,
    pub granted_external_scopes: std::collections::BTreeSet<String>,
    pub expires_at: Option<Timestamp>,
    pub content_hash: [u8; 32],
}

pub struct StageSecretVersionRequest {
    pub backend_binding_revision_id: SecretBackendBindingRevisionId,
    pub secret_id: SecretId,
    pub version_id: SecretVersionId,
    pub generation: u64,
    pub maximum_bytes: u64,
    pub expires_at: Option<Timestamp>,
}

pub struct StagedSecretVersion {
    pub reference: SecretVersionReference,
    pub opaque_staging_handle: OpaqueSecretStagingHandle,
    pub material_fingerprint: [u8; 32],
}

pub struct SecretVersionObservation {
    pub reference: SecretVersionReference,
    pub present: bool,
    pub enabled: bool,
    pub byte_size: u64,
}

pub enum SecretBackendErrorKind {
    InvalidRequest,
    SizeLimitExceeded,
    NotFound,
    Disabled,
    PermissionDenied,
    Unavailable,
    Timeout,
    Corrupt,
    Cancelled,
    UnknownCompletion,
}

pub struct SecretBackendError {
    pub kind: SecretBackendErrorKind,
    pub safe_message: String,
    pub completion_may_have_occurred: bool,
}

pub struct SecretDeletionObservation {
    pub reference: SecretVersionReference,
    pub absent: bool,
}
```

`SecretReference` implements a custom redacted Debug. `RefreshToken`, `ClientSecret`, `Password` and `PrivateKey` are not ordinary outbound components.

```rust
#[async_trait::async_trait]
pub trait SecretBackendPort: Send + Sync {
    async fn stage_version(
        &self,
        request: StageSecretVersionRequest,
        material: SensitiveSecretInput,
    ) -> Result<StagedSecretVersion, SecretBackendError>;
    async fn promote_version(&self, staged: &StagedSecretVersion)
        -> Result<SecretVersionObservation, SecretBackendError>;
    async fn inspect_version(&self, reference: &SecretVersionReference)
        -> Result<SecretVersionObservation, SecretBackendError>;
    async fn read_ephemeral(
        &self,
        reference: &SecretVersionReference,
        purpose: SecretReadPurpose,
    ) -> Result<EphemeralSecretMaterial, SecretBackendError>;
    async fn disable_version(&self, reference: &SecretVersionReference)
        -> Result<(), SecretBackendError>;
    async fn delete_version(&self, reference: &SecretVersionReference)
        -> Result<SecretDeletionObservation, SecretBackendError>;
}
```

`SensitiveSecretInput`/`EphemeralSecretMaterial` are Vestrace-owned, non-serializable and zeroizing. Authorization/rotation may stage; only an exhausted lease, exchange/refresh, validation/revocation signer or isolated scanner may read with exact `SecretReadPurpose`.

### Authorization, rotation and validation

```rust
pub enum AuthorizationFlowKind { OAuth2AuthorizationCodePkce, ApiKeySubmission }

pub enum AuthorizationFlowStatus {
    Created,
    AwaitingUser,
    CallbackReceived,
    Exchanging,
    MaterialStored,
    Validating,
    Completed,
    Failed,
    Cancelled,
    Expired,
    Unknown,
}

pub struct AuthorizationFlow {
    pub id: AuthorizationFlowId,
    pub workspace_id: WorkspaceId,
    pub connection_id: ConnectionId,
    pub kind: AuthorizationFlowKind,
    pub status: AuthorizationFlowStatus,
    pub requested_scopes: std::collections::BTreeSet<String>,
    pub state_hmac: Option<[u8; 32]>,
    pub pkce_challenge: Option<String>,
    pub verifier_secret_version: Option<SecretVersionReference>,
    pub authorization_code_secret_version: Option<SecretVersionReference>,
    pub expires_at: Timestamp,
    pub logical_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}

pub enum CredentialRotationStatus {
    Created,
    MaterialStaging,
    MaterialStored,
    Validating,
    Activating,
    RevokingOldLeases,
    Completed,
    Failed,
    Unknown,
}

pub struct ConnectionCredentialBinding {
    pub id: ConnectionCredentialBindingId,
    pub connection_id: ConnectionId,
    pub credential_set_generation_id: CredentialSetGenerationId,
    pub authentication_scheme: ConnectorAuthenticationScheme,
    pub activated_at: Timestamp,
    pub supersedes_binding_id: Option<ConnectionCredentialBindingId>,
    pub revoked_at: Option<Timestamp>,
}

pub struct ConnectionValidation {
    pub id: ConnectionValidationId,
    pub connection_id: ConnectionId,
    pub credential_generation_id: CredentialSetGenerationId,
    pub status: ConnectionValidationStatus,
    pub external_identity_hash: Option<[u8; 32]>,
    pub granted_scope_hash: [u8; 32],
    pub safe_code: String,
    pub validated_at: Timestamp,
}
```

Clear state is returned once and only HMAC is stored. Code/verifier use exact versions and are disabled/deleted after exchange. Refresh is singleflight per Connection/generation; `invalid_grant` produces `ReauthorizationRequired`.

### Delegated grants and remote profiles

```rust
pub enum ConnectionGrantConsumer {
    SubRun { child_run_id: AgentRunId, binding_id: SubRunBindingId },
    RemoteInvocation { invocation_id: RemoteAgentInvocationId },
}

pub struct ConnectionAccessGrant {
    pub id: ConnectionAccessGrantId,
    pub connection_id: ConnectionId,
    pub permission_revision_id: ConnectionPermissionCeilingRevisionId,
    pub parent_run_id: AgentRunId,
    pub consumer: ConnectionGrantConsumer,
    pub purpose: String,
    pub allowed_operations: std::collections::BTreeSet<ConnectorOperationId>,
    pub allowed_resources: Vec<ConnectorResourcePattern>,
    pub maximum_classification: DataClassification,
    pub maximum_leases: u32,
    pub lease_count: u32,
    pub expires_at: Timestamp,
    pub policy_decision_id: PolicyDecisionId,
    pub status: ConnectionAccessGrantStatus,
    pub created_at: Timestamp,
}

pub struct RemoteAgentConnectionProfileRevision {
    pub id: RemoteAgentConnectionProfileRevisionId,
    pub profile_id: RemoteAgentConnectionProfileId,
    pub revision: u32,
    pub remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
    pub connection_id: ConnectionId,
    pub permission_revision_id: ConnectionPermissionCeilingRevisionId,
    pub verified_external_identity_hash: Option<[u8; 32]>,
    pub allowed_origins: Vec<NormalizedOrigin>,
    pub allowed_transports: std::collections::BTreeSet<String>,
    pub allowed_operations: std::collections::BTreeSet<ConnectorOperationId>,
    pub allowed_resources: Vec<ConnectorResourcePattern>,
    pub maximum_classification: DataClassification,
    pub injection_scheme: CredentialInjectionScheme,
    pub content_hash: [u8; 32],
}
```

A remote profile is a local immutable allowlist. Remote declarations only narrow the intersection.

### Credential leases and dual-ticket binding

```rust
pub enum CredentialSourceRef {
    Connection { connection_id: ConnectionId, permission_revision_id: ConnectionPermissionCeilingRevisionId },
    ServiceBinding { binding_revision_id: ServiceCredentialBindingRevisionId },
}

pub enum CredentialOperationRef {
    Connector { operation_id: ConnectorOperationId },
    Service { namespace: String, operation: String },
}

pub enum CredentialConsumerRef {
    ModelAttempt { attempt_id: ModelExecutionAttemptId },
    ToolAttempt { attempt_id: ToolExecutionAttemptId },
    RemoteInvocation { invocation_id: RemoteAgentInvocationId },
    ArtifactFetch { ingestion_session_id: ArtifactIngestionSessionId },
    ArtifactStoreOperation { operation_id: String },
    SandboxProxy { proxy_session_id: SandboxCredentialProxySessionId },
    ConnectionValidation { validation_id: ConnectionValidationId },
}

pub enum CredentialInjectionScheme {
    OAuthBearerHeader,
    StaticBearerHeader,
    ApiKeyHeader { header_name: String },
    BasicHeader,
    ClientCertificate,
    RequestSigner { scheme: String },
}

pub struct CredentialResourceBinding {
    pub kind: String,
    pub canonical_identifier_hash: [u8; 32],
}

pub struct CredentialLease {
    pub id: CredentialLeaseId,
    pub workspace_id: WorkspaceId,
    pub source: CredentialSourceRef,
    pub credential_generation_id: CredentialSetGenerationId,
    pub consumer: CredentialConsumerRef,
    pub principal_id: PrincipalId,
    pub acting_agent_snapshot_id: Option<AgentRuntimeSnapshotId>,
    pub run_id: Option<AgentRunId>,
    pub step_id: Option<RunStepId>,
    pub operation: CredentialOperationRef,
    pub resources: Vec<CredentialResourceBinding>,
    pub destination_origin: NormalizedOrigin,
    pub credential_binding_hash: [u8; 32],
    pub protected_action_fingerprint: OperationFingerprint,
    pub secret_use_fingerprint: OperationFingerprint,
    pub protected_action_ticket_id: AuthorizationTicketId,
    pub secret_use_ticket_id: AuthorizationTicketId,
    pub access_grant_id: Option<ConnectionAccessGrantId>,
    pub injection_scheme: CredentialInjectionScheme,
    pub lease_idempotency_key: String,
    pub maximum_uses: u16,
    pub use_count: u16,
    pub expires_at: Timestamp,
    pub status: CredentialLeaseStatus,
    pub logical_revision: u64,
    pub created_at: Timestamp,
}
```

`protected_action_fingerprint` and `secret_use_fingerprint` are distinct because their H2 actions/resources differ. Both tickets must carry or obligation-bind the exact same `credential_binding_hash`, derived from workspace, principal/agent, Run/consumer, credential source revision, operation, resources, destination, classification and argument hash. This prevents a valid secret-use ticket from being attached to another protected action.

Consumption locks lease, source revision, generation and optional grant; validates tickets, binding hash, expiry/revocation/use count; increments counters and appends a usage intent; then resolves the exact component inside credential runtime. For a one-use lease, the transition is `Issued → Exhausted`. Query injection and persistent cookies are unsupported.

### Sandbox proxy

```rust
pub struct SandboxCredentialProxySession {
    pub id: SandboxCredentialProxySessionId,
    pub sandbox_session_id: SandboxSessionId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub connection_id: ConnectionId,
    pub permission_revision_id: ConnectionPermissionCeilingRevisionId,
    pub allowed_origin: NormalizedOrigin,
    pub allowed_methods: std::collections::BTreeSet<String>,
    pub allowed_path_patterns: Vec<String>,
    pub maximum_requests: u16,
    pub request_count: u16,
    pub maximum_request_bytes: u64,
    pub maximum_response_bytes: u64,
    pub expires_at: Timestamp,
    pub status: SandboxCredentialProxyStatus,
}
```

The workload receives only an ephemeral proxy capability through an H4-owned local channel. PostgreSQL stores its HMAC. The proxy validates session/origin/DNS pin/method/path/body, strips auth headers, consumes a fresh lease, injects outside the workload namespace and bounds/redacts the response.

### Secret leak and separate audit records

```rust
pub enum SecretLeakDisposition { Deny, Redact, Quarantine, Alert }

pub enum SecretLeakScanTarget {
    ModelTransfer { execution_id: ModelExecutionId },
    Interaction { interaction_id: InteractionEventId },
    Artifact { revision_id: ArtifactRevisionId },
    Notification { intent_id: NotificationIntentId },
    Diagnostic { diagnostic_id: String },
}

pub struct SecretLeakFinding {
    pub id: SecretLeakFindingId,
    pub workspace_id: WorkspaceId,
    pub target: SecretLeakScanTarget,
    pub secret_reference_hash: Option<[u8; 32]>,
    pub rule_id: String,
    pub byte_range: Option<ArtifactByteRange>,
    pub irreversible_fingerprint: [u8; 32],
    pub disposition: SecretLeakDisposition,
    pub created_at: Timestamp,
}

pub struct CredentialUsageAuditRecord {
    pub id: CredentialUsageAuditRecordId,
    pub workspace_id: WorkspaceId,
    pub lease_id: CredentialLeaseId,
    pub credential_generation_id: CredentialSetGenerationId,
    pub principal_id: PrincipalId,
    pub acting_agent_snapshot_id: Option<AgentRuntimeSnapshotId>,
    pub run_id: Option<AgentRunId>,
    pub step_id: Option<RunStepId>,
    pub consumer: CredentialConsumerRef,
    pub credential_binding_hash: [u8; 32],
    pub destination_origin_hash: [u8; 32],
    pub result: CredentialUseResult,
    pub occurred_at: Timestamp,
}

pub enum CredentialLifecycleAction {
    AuthorizationStarted,
    AuthorizationCompleted,
    AuthorizationUnknown,
    Validated,
    Refreshed,
    Rotated,
    ReauthorizationRequired,
    Disabled,
    Revoked,
    SecretLeakBlocked,
}

pub struct CredentialLifecycleAuditRecord {
    pub id: CredentialLifecycleAuditRecordId,
    pub workspace_id: WorkspaceId,
    pub connection_id: Option<ConnectionId>,
    pub service_binding_id: Option<ServiceCredentialBindingId>,
    pub credential_generation_id: Option<CredentialSetGenerationId>,
    pub principal_id: PrincipalId,
    pub action: CredentialLifecycleAction,
    pub safe_code: String,
    pub occurred_at: Timestamp,
}
```

Usage audit is lease-bound. Lifecycle audit covers authorization/refresh/rotation/revocation. Neither stores headers, token fragments, query strings, bodies or backend paths.

---

### Task 1: Add Connector definition and operation contracts

**Files:** modify `id.rs`; create `connector/{mod,definition,operation,resource,scope,binding}.rs`; export domain modules.

- [ ] Add all H8 IDs, including ceiling/service revisions, refresh/scan and both audit IDs.
- [ ] Test duplicate operations, unsafe headers, invalid origin/resource/scope, unsupported write semantics and hash/revision mismatch.
- [ ] Implement stable identifiers and immutable hashing across binding, auth, scopes, operations, schemas and data handling.
- [ ] Require every non-read-only operation to declare idempotency/reconciliation support.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain connector
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(connector): add connector operation contracts"
```

### Task 2: Add Connection and service-binding revisions

**Files:** create connection files and service-binding credential types.

- [ ] Test owner/workspace invariants, full transition table, ceiling wider than Connector, revision immutability, unsafe identity display and service-origin mismatch.
- [ ] Implement deterministic intersections for operation/resource/scope/classification/risk.
- [ ] Explicit sharing rows select principals/agents; H2 remains mandatory.
- [ ] Service bindings are non-model-visible and exact-origin/operation/classification bound.
- [ ] Run/commit.

### Task 3: Add secret contracts and local encrypted backend

**Files:** create credential reference/set types, credential runtime, local backend, test support/conformance.

- [ ] Prove material wrappers cannot Clone/Serialize/Debug; errors are redacted and buffers zeroize.
- [ ] Conformance covers stage/promote/inspect/purpose-read/disable/delete, idempotent promote, ambiguous reconciliation, generation isolation and wrong key.
- [ ] Local backend uses owner-only master-key file/inherited descriptor, XChaCha20-Poly1305, random nonce, exact AAD, exclusive staging, fsync and atomic rename.
- [ ] Paths derive from opaque IDs only.
- [ ] Run/commit.

### Task 4: Define H8 ports, commands and deterministic fixtures

**Files:** create application modules and credential test-support crate.

**Interfaces:** repository ports, Connector auth/validation/refresh, secret backend, Broker/lease consumer, grant, remote decorator, sandbox proxy, known-secret scanner and fault injection.

- [ ] Object-safety compile tests for every port.
- [ ] Faults after stage/promote, code stage, token response, generation persistence, binding activation, lease exhaust, proxy injection, rotation activation and audit append.
- [ ] Connector adapters receive ephemeral material only and return normalized safe observations.
- [ ] Deterministic Connector supports PKCE, API-key, validation, refresh, revoke and lost exchange response.
- [ ] Run/commit.

### Task 5: Persist Connector/Connection/service definitions

**Files:** create `0054`, `0055`, PostgreSQL repositories and persistence tests.

- [ ] `0054`: stable Connector definitions, immutable revisions, operations/scopes/resources, adapter/backend bindings.
- [ ] `0055`: Connections, immutable ceiling revisions, service bindings/revisions, sharing, external scopes and validations; add H3 provider/H6 backend relation tables to service-binding revisions without editing historical migrations.
- [ ] Enforce workspace, exact current revisions and append-only revision/validation rows.
- [ ] New Connector revision never silently changes an existing Connection.
- [ ] Run/commit.

### Task 6: Implement PKCE and secure API-key authorization

**Files:** create `0056`, authorization/binding repositories/services, secure HTTP/CLI routes and auth tests.

- [ ] PKCE test covers state HMAC, callback replay/wrong actor/expiry, exact code/verifier versions, exchange, validation and activation.
- [ ] Lost exchange response becomes Unknown, creates no guessed generation and does not replay code blindly.
- [ ] API-key value is absent from access logs, H7, PostgreSQL, traces and errors.
- [ ] `0056`: flows/events, credential sets/generations/components, Connection bindings, refresh/rotation and temporary secret bindings.
- [ ] Callback stages code; exchange promotes components, persists one generation, validates/activates, then disables/deletes code/verifier.
- [ ] H7 receives opaque status only.
- [ ] Run/commit.

### Task 7: Implement validation, refresh, rotation and revocation

**Files:** create services/repositories/workers and refresh/rotation/revocation tests.

- [ ] Validation uses safe identity operation and stores only hashes/codes.
- [ ] Twenty concurrent refreshes create one refresh and one new access-token generation.
- [ ] Rotation stages/validates/activates one generation and revokes old unexhausted leases.
- [ ] Crash after promote reconciles same version/generation.
- [ ] Revocation blocks new leases/grants, attempts provider revoke and preserves in-flight uncertainty/audit.
- [ ] `invalid_grant` sets ReauthorizationRequired and matching H7 auth request.
- [ ] Run/commit.

### Task 8: Persist delegated grants and remote profiles

**Files:** create `0057`, grant/profile services/repositories/tests.

- [ ] Grant = exact Connection ceiling revision ∩ H5 scope ∩ request ∩ H2.
- [ ] Bind to one SubRun or RemoteInvocation; enforce expiry/max leases/workspace.
- [ ] Child cannot use omitted or parent-only access.
- [ ] Agent Card declarations cannot widen profile.
- [ ] `0057`: grants, operation/resource rows, counters and immutable remote profile revisions.
- [ ] Run/commit.

### Task 9: Implement atomic dual-ticket leases

**Files:** create `0058`, Broker/lease repository/runtime consumer and atomicity/restart tests.

- [ ] Issue validates two distinct H2 tickets and a common `credential_binding_hash`, source revision/generation, operation/resource/origin, scheme and optional grant.
- [ ] Duplicate idempotency returns same lease; conflict fails.
- [ ] Consume locks lease/source/generation/grant, validates all fences, increments counters, appends usage intent and transitions one-use lease to Exhausted before secret read.
- [ ] Concurrent consume yields one success.
- [ ] Crash after exhaustion never reuses lease; owner must prove retry safety for a new lease.
- [ ] `0058`: leases, binding hashes/fingerprints/resources/origins, use records, proxy sessions/HMACs and revocation indexes.
- [ ] Run/commit.

### Task 10: Integrate H3, H4 and H6 credentials

**Files:** modify provider/tool/fetch/store adapters and add integration tests.

- [ ] H3 maps existing provider credential refs to exact ServiceCredentialBindingRevision and binds provider origin/attempt.
- [ ] H4 maps logical Connection handle to exact Connection/ceiling/operation/resources and H4 commit fingerprint.
- [ ] H6 authenticates only after SSRF/origin checks; cross-origin redirect requires new authorization and is denied by default.
- [ ] Prove no backend/generation/lease/material in prompts, results, Artifacts, work or events.
- [ ] Unknown completion exhausts lease and follows owning reconciliation; Broker never retries.
- [ ] Run/commit.

### Task 11: Implement sandbox credential proxy

**Files:** create proxy crate, H4 composition and proxy tests.

- [ ] Workload without env/mount secret reaches one allowed origin/path and upstream sees injected auth.
- [ ] Reject wrong origin/IP/DNS pin/method/path/body, caller auth/cookie headers, expired capability, wrong sandbox and overuse.
- [ ] Clear proxy capability uses H4 local socket/memfd-style channel; PostgreSQL stores HMAC only.
- [ ] Fresh lease per request; H4 egress and bounded/redacted response.
- [ ] No generic TCP/SOCKS/unrestricted proxy.
- [ ] Run/commit.

### Task 12: Integrate H7 authentication continuations

**Files:** modify H7 auth/continuation and HTTP/CLI commands; add auth-continuation tests.

- [ ] AuthenticationRequired references opaque H8 IDs and rejects credential content in HumanResponse.
- [ ] Start returns safe URL/instruction directly to authenticated requester; state/code never enter history/notifications.
- [ ] Completion enqueues exact `AuthenticationReady` cause.
- [ ] Resume rechecks Connection/operation/resource/grant/policy/current generation; unrelated flow cannot resume.
- [ ] CLI secure input disables echo and avoids argv/history.
- [ ] Run/commit.

### Task 13: Integrate remote-agent authentication

**Files:** create remote profile/decorator runtime, modify H5 fixtures and add remote tests.

- [ ] Validate exact remote profile/revision, identity/origin, transport, operation/resources/classification and H5 grant.
- [ ] Use distinct lease and infrastructure-only injection.
- [ ] Remote Card/message/task/metadata receives no refs/tickets/grants/leases/material.
- [ ] Lost response exhausts lease and sets H5 Unknown; reconciliation never reuses it.
- [ ] Revocation blocks later dispatch/continuation while preserving task/audit.
- [ ] H9A consumes boundary without A2A types in H8.
- [ ] Run/commit.

### Task 14: Add known-secret controls and separate audits

**Files:** create `0059`, scanner/audit/leak services/repositories, H3/H6/H7 integration and tests.

- [ ] Scanner loads eligible values into isolated zeroizing memory and returns only refs/rules/ranges/fingerprints.
- [ ] Test current/retained rotated secrets, patterns, false-positive policy and fragment-free serialization.
- [ ] Deny before model transfer, Redact eligible text, Quarantine Artifact, Alert diagnostics.
- [ ] `0059`: append-only scans/findings, usage audit, lifecycle audit and safe diagnostics.
- [ ] Usage records material-resolve/inject/dispatch/reject/unknown/failure; lifecycle records auth/validate/refresh/rotate/reauth/disable/revoke/leak block.
- [ ] Persistence script rejects secret-bearing SQL/schema/event/public fields.
- [ ] Run/commit.

### Task 15: Integrate workers, checkpoint V6, RLS and acceptance

**Files:** create `0060`, worker/Run/checkpoint changes, boundary scripts, CI and acceptance/RLS tests.

- [ ] Work kinds:

```text
ExchangeAuthorizationCode
ValidateConnection
RefreshConnectionCredential
RotateConnectionCredential
RevokeConnection
ReconcileSecretBackend
ReconcileAuthorizationFlow
ReconcileCredentialRotation
ExpireCredentialLeases
ExpireConnectionAccessGrants
ScanKnownSecrets
```

Payloads contain IDs/revisions/deadlines only—never material, code, verifier, clear state, proxy capability or ephemeral credential.

- [ ] Logical Run events only for auth waiting, available Connection binding or pause due to revocation; H8 operational events do not increment RunVersion.
- [ ] `RunCheckpointV6` extends V5 compatibly with required Connection IDs, active flow IDs and delegated grant IDs only; older payloads remain readable. No secret refs/generations/leases/capabilities.
- [ ] `0060` forces RLS, same-workspace/owner/consumer consistency, immutable/append-only guards and active flow/refresh/lease/grant/expiry/audit indexes.
- [ ] Boundary scripts reject serializable/debuggable material, secret-like work/event/public fields, backend reads outside credential runtime, raw credentials in H3–H7, sandbox env/mount credentials and A2A credential fields.
- [ ] Acceptance proves:
  1. immutable Connector/ceiling/service revisions;
  2. OAuth with no clear code/token in PostgreSQL/logs/events;
  3. exchange response-loss is Unknown without replay;
  4. API-key secure input creates no InteractionEvent copy;
  5. provider service binding and tool Connection use separate leases;
  6. dual tickets share exact binding hash but have distinct fingerprints;
  7. model sees logical handles only;
  8. sandbox proxy exposes no secret;
  9. SubRun grant is narrower than parent;
  10. remote dispatch uses identity/origin-bound lease;
  11. lost external response exhausts lease and does not duplicate work;
  12. refresh singleflight;
  13. rotation activates one generation/revision and revokes obsolete leases;
  14. revocation blocks future tool/remote leases;
  15. matching H7 auth resumes and unrelated flow does not;
  16. known secret is denied/redacted/quarantined;
  17. usage/lifecycle audit is complete and fragment-free;
  18. restart creates no duplicate generation/use/call;
  19. replay performs no credential-bearing action.
- [ ] Required CI jobs: domain, encrypted backend, authorization, refresh/rotation/revoke, grants/profiles, lease atomicity, provider/tool/fetch injection, proxy, remote auth, leak/audit, boundaries, H8 acceptance.
- [ ] Run/commit:

```bash
bash scripts/verify-secret-boundary.sh
bash scripts/verify-credential-lease-boundary.sh
bash scripts/verify-sandbox-credential-boundary.sh
bash scripts/verify-credential-persistence.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h8_acceptance --test connection_credential_rls \
             --test credential_lease_atomicity --test credential_rotation_restart
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

git add migrations/0060_connection_credential_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml crates scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(credentials): add H8 acceptance and secret safety gates"
```

---

## Migration ownership

```text
0054 Task 5  Connector definitions, operations and backend bindings
0055 Task 5  Connections, permission/service revisions, sharing, scopes and validation
0056 Task 6  Authorization flows, credential bindings and rotations
0057 Task 8  Delegated Connection grants and remote profiles
0058 Task 9  Credential leases, uses and proxy sessions
0059 Task 14 Secret-leak findings and usage/lifecycle audits
0060 Task 15 RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H8 completion definition

H8 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
ConnectorDefinitionRevision or ServiceCredentialBindingRevision
→ immutable local ceiling
→ authorization/secret generation
→ validation
→ protected action ticket + secret.use ticket
→ common credential_binding_hash
→ exact CredentialLease
→ atomic exhaustion
→ ephemeral injection/proxy decoration
→ external request
→ owning reconciliation
→ usage audit
→ refresh/rotation/revocation
```

Required invariants:

1. PostgreSQL stores no secret bytes; secret backend cannot authorize use.
2. Connector/remote declarations cannot grant authority.
3. External scopes/sharing cannot exceed immutable local revision and H2.
4. OAuth state/code/verifier/token lifecycle is one-time, bounded and restart-safe without clear persistence.
5. Ambiguous exchange is Unknown, not blind replay.
6. API-key input bypasses ordinary interactions/logging.
7. Material wrappers cannot clone/serialize/debug values and zeroize.
8. Protected-action and secret-use tickets are distinct but bound to one exact credential operation.
9. Every outbound use exhausts/consumes an exact lease immediately before injection.
10. Exhausted leases are never reused after crash/timeout/Unknown.
11. Refresh is singleflight; refresh tokens are not ordinary outbound credentials.
12. Rotation activates one immutable generation/revision and revokes obsolete leases.
13. Revocation blocks new leases while preserving in-flight uncertainty/audit.
14. SubRuns/remote invocations use explicit narrowed grants.
15. Remote auth binds local identity/origin/transport/operation/resource/classification.
16. Models see logical handles only.
17. Sandboxes receive no long-lived secret; proxy capability is exact-session/destination bound.
18. Provider/tool/fetch/remote adapters share one lease/runtime contract.
19. Known-secret detection persists no matched value.
20. Usage/lifecycle audits are append-only and content-free.
21. H7 authentication continuations contain opaque status only.
22. No A2A SDK type or Agent Card credential enters H8 core.
23. Restart does not duplicate generations, lease use, rotation or connector calls.
24. Replay performs no credential-bearing action.
25. H9 can register Connector/adapter revisions without changing H8 semantics.
26. H9A can consume remote profiles/leases without changing H8 types.
27. H10 can add integrity/metrics without rewriting H8 records.
28. Tests require no public provider/SaaS/remote agent/permanent secret.

## Explicit non-goals

H8 does not implement production Gmail/Slack/CRM connectors, organization OIDC/SCIM, a marketplace, Vault/cloud-secret-manager adapters, HSM signing, OAuth device flow, browser UI, generic TCP/SOCKS tunneling, unrestricted cookies, permanent sandbox env credentials, cross-organization credential delegation, Agent Card trust, A2A transport or any API returning secret material.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h8-connections-credential-broker`, add crypto/OAuth dependencies, create migrations `0054`–`0060`, create a master key/store, start callback/proxy servers, submit/rotate real credentials, modify production adapters, change CI or execute H8 tests.
