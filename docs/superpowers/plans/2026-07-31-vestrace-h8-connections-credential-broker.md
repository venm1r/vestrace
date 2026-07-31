# Vestrace H8 Connections and Credential Broker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start authorization or proxy services, store credentials, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement immutable connector definitions, user/workspace-bound Connections, Authorization Code + PKCE and API-key onboarding, replaceable secret storage, operation-bound credential leases, delegated Connection access, request-scoped injection for providers/tools/remote agents, a sandbox credential proxy, rotation/revocation, secret-leak controls and content-free credential audit.

**Architecture:** H8 separates four authorities. PostgreSQL owns connector/Connection lifecycle, local permission ceilings, authorization-flow metadata, opaque secret references, grants, leases and audit. `SecretBackendPort` owns secret bytes and immutable secret versions but cannot authorize their use. H2 authorizes both the protected external operation and `secret.use`; the Credential Broker intersects those decisions with the exact Connection or service binding and issues a short-lived lease. `vestrace-credential-runtime` consumes the lease immediately before dispatch, resolves zeroizing ephemeral material and injects/signs the infrastructure request. Models, public DTOs, work items, durable events and sandbox workloads receive only sanitized logical handles.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H7 Rust workspace; Rust Edition 2024; Tokio; Axum/Tower for secure callback/submission routes; SQLx and PostgreSQL 17; SHA-256 and HMAC-SHA-256; OAuth Authorization Code with PKCE S256; non-serializable zeroizing secret wrappers; RustCrypto XChaCha20-Poly1305 for the initial local encrypted backend; deterministic loopback OAuth/resource/remote/proxy fixtures; proptest; tracing with mandatory redaction.

## Global Constraints

- Complete all five v0.1 plans and H1–H7 before implementing H8.
- Harness design section `20. Connector Registry and Credential Broker` is normative.
- PostgreSQL is authoritative for Connector definitions/revisions, Connections, permission ceilings, authorization flows, credential-set references, service bindings, rotations, validations, grants, remote profiles, leases, uses, proxy sessions, leak findings and audit metadata.
- Secret bytes, OAuth codes, PKCE verifiers, access/refresh tokens, API keys, client secrets, passwords, signing keys and private keys live only behind `SecretBackendPort` and are never PostgreSQL values.
- Vestrace owns every domain/application contract, ID, lifecycle, schema, operation fingerprint and public DTO. External OAuth/HTTP/keyring/cloud/A2A SDK types remain inside adapters.
- Connector, adapter-binding, secret-backend-binding, remote-profile, credential-generation and permission-ceiling revisions are immutable. Mutable lifecycle belongs to stable identities with optimistic state revisions.
- A Connection is a logical authorization relationship, not a secret. It is principal-bound or workspace-bound and has an explicit local ceiling.
- External scopes and remote security declarations are compatibility evidence, not local authority.
- Effective use is the intersection of operation requirements, Connection/service binding, sharing, delegated grant, current H2 policy, data classification, destination and the exact protected action.
- A `SecretReference`, `SecretVersionReference`, Connection ID, lease ID or content hash is not a bearer capability.
- Clear secret material types are non-Clone, non-Serialize and non-Debug, use constant-time-safe handling where applicable and zeroize on drop.
- Authorization Code + PKCE S256 is the initial user OAuth flow. Implicit and password-grant flows are rejected. State is one-time and persisted only as HMAC.
- OAuth codes and PKCE verifiers are short-retention secret-backend values. Raw token responses are never persisted.
- An ambiguous token exchange response becomes `AuthorizationFlowStatus::Unknown`; the code is not blindly exchanged again. The user is routed to reauthorization unless a connector-specific reconciliation proves the credential generation exists.
- API-key submission uses a dedicated sensitive route/secure CLI input and never becomes an H7 InteractionEvent, command-line argument, access-log field or error payload.
- Refresh tokens and client secrets are broker-internal components. They cannot be selected by a model or injected as an ordinary external-operation credential.
- Rotation creates a new immutable credential generation, validates it, atomically activates a new binding and revokes unconsumed leases for older generations.
- Connection or service-binding revocation blocks new leases immediately but cannot recall an already dispatched request.
- Credential leases default to one use and no more than 60 seconds. Longer/multi-use leases require explicit nonstandard policy and are excluded from the v0.2 reference profiles.
- Lease consumption occurs at the last practical enforcement point before secret resolution/injection. A consumed lease is not reused after crash, timeout or ambiguous completion.
- H3 provider, H4 tool, H5 remote-agent, H6 authenticated fetch/store and H9A A2A adapters use the same Vestrace lease and runtime-injection boundary.
- Models see only `LogicalConnectionHandle` and sanitized connector operation metadata. They never see backend refs, credential generations, lease IDs, proxy capabilities or secret fragments.
- Sandboxes receive no long-lived credential through environment, argv, mounted files or Artifacts. H8 supplies an application-level HTTP credential proxy over an H4-managed local channel.
- The proxy strips caller-supplied authentication, cookie and proxy-authentication headers before injection.
- Remote-agent authentication binds exact local remote-agent revision/profile, verified identity/origin, transport, operation, resource and classification. Agent Cards cannot widen the profile.
- SubRuns and remote invocations use explicit `ConnectionAccessGrant` records; no child inherits the parent Connection implicitly.
- Known-secret scans return only refs/hashes, rule IDs, ranges and irreversible fingerprints. Findings never contain recoverable secret values.
- Secret scanning is integrated before model transfer, ordinary event persistence, notification rendering, Artifact availability/export and connector diagnostics according to subsystem policy.
- Credential-use and credential-lifecycle audit records are append-only and content-free. H10 may add integrity chains without rewriting H8 rows.
- Replay never starts authorization, reads secret material, refreshes, rotates, issues/consumes leases, injects credentials, calls a connector or uses the proxy.
- Existing migrations `0014`–`0053` are never edited. H8 migrations are `0054`–`0060` and are created once.
- CI uses loopback fixtures, temporary encrypted stores and fake clocks. It requires no public identity provider, SaaS account, remote agent or permanent credential.
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

crates/vestrace-secret-backend-local/src/
  lib.rs
  backend.rs
  crypto.rs
  layout.rs
  staging.rs
  master_key.rs
  reconcile.rs

crates/vestrace-credential-proxy/src/
  lib.rs
  server.rs
  session.rs
  request_policy.rs
  injector.rs
  response.rs

crates/vestrace-credential-test-support/src/
  lib.rs
  secret_backend.rs
  oauth_server.rs
  resource_server.rs
  connector.rs
  remote.rs
  proxy.rs
  scanner.rs
  fixtures.rs
  conformance.rs

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

### Supporting types

```rust
pub enum ConnectorDefinitionStatus { Draft, Active, Deprecated, Revoked }
pub enum ConnectorAdapterKind { Builtin, Http, Mcp, Extension }
pub enum SecretBackendKind { LocalEncrypted, External }

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

pub enum ConnectionValidationStatus { Valid, Degraded, Invalid, ReauthorizationRequired }
pub enum ConnectionAccessGrantStatus { Issued, Exhausted, Expired, Revoked }
pub enum SandboxCredentialProxyStatus { Issued, Active, Exhausted, Expired, Revoked }
pub enum CredentialUseResult { Injected, RequestDispatched, RejectedBeforeDispatch, CompletionUnknown, Failed }

pub enum SecretReadPurpose {
    ConsumedCredentialLease { lease_id: CredentialLeaseId },
    OAuthTokenExchange { flow_id: AuthorizationFlowId },
    OAuthRefresh { refresh_id: CredentialRefreshId },
    ConnectionValidation { validation_id: ConnectionValidationId },
    ConnectorRevocation { connection_id: ConnectionId },
    KnownSecretScan { scan_id: SecretLeakScanId },
}
```

`LogicalConnectionHandle` contains an opaque logical alias selected by trusted application code. `NormalizedOrigin` is exactly `scheme://host:effective-port`, contains no userinfo/path/query/fragment and rejects non-HTTPS origins except deterministic loopback tests or explicit local policy.

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

Stable names use lowercase dotted segments. Connector-defined credential headers are validated against an allowlist. `Authorization` and explicitly approved `X-Api-Key`-style names are possible; `Cookie`, `Proxy-Authorization`, `Host`, `Content-Length` and hop-by-hop headers are forbidden.

### Connections and service credential bindings

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

pub struct ConnectionPermissionCeiling {
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
    pub current_credential_binding_id: Option<ConnectionCredentialBindingId>,
    pub external_identity_hash: Option<[u8; 32]>,
    pub external_identity_display: Option<String>,
    pub permission_ceiling: ConnectionPermissionCeiling,
    pub state_revision: u64,
    pub last_validation_id: Option<ConnectionValidationId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct ServiceCredentialBinding {
    pub id: ServiceCredentialBindingId,
    pub workspace_id: WorkspaceId,
    pub stable_name: String,
    pub purpose: String,
    pub credential_set_generation_id: CredentialSetGenerationId,
    pub allowed_operations: std::collections::BTreeSet<String>,
    pub allowed_origins: Vec<NormalizedOrigin>,
    pub maximum_classification: DataClassification,
    pub status: ServiceCredentialBindingStatus,
    pub state_revision: u64,
}
```

`ServiceCredentialBinding` covers infrastructure-owned provider, Artifact-store, processor or other non-Connector credentials. It does not appear in model-visible catalogues. Connection sharing and service binding use still require H2.

Allowed Connection transitions include:

```text
PendingAuthorization → Active | ReauthorizationRequired | Disabled | Revoked
Active → Degraded | ReauthorizationRequired | Disabled | Revoked | Expired
Degraded → Active | ReauthorizationRequired | Disabled | Revoked | Expired
ReauthorizationRequired → PendingAuthorization | Disabled | Revoked | Expired
Disabled → PendingAuthorization | Active | Revoked | Expired, only by protected command
Revoked and Expired are terminal
```

### Secret references, backend results and credential sets

```rust
#[derive(Clone, Debug, Eq, PartialEq,
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
    pub generation: u64,
    pub components: std::collections::BTreeMap<CredentialComponentKind, SecretVersionReference>,
    pub granted_external_scopes: std::collections::BTreeSet<String>,
    pub expires_at: Option<Timestamp>,
    pub content_hash: [u8; 32],
}

pub struct StageSecretVersionRequest {
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

`RefreshToken`, `ClientSecret`, `Password` and `PrivateKey` are never ordinary outbound components. Their use is restricted to enumerated refresh/exchange/revoke/signer purposes.

```rust
#[async_trait::async_trait]
pub trait SecretBackendPort: Send + Sync {
    async fn stage_version(
        &self,
        request: StageSecretVersionRequest,
        material: SensitiveSecretInput,
    ) -> Result<StagedSecretVersion, SecretBackendError>;
    async fn promote_version(
        &self,
        staged: &StagedSecretVersion,
    ) -> Result<SecretVersionObservation, SecretBackendError>;
    async fn inspect_version(
        &self,
        reference: &SecretVersionReference,
    ) -> Result<SecretVersionObservation, SecretBackendError>;
    async fn read_ephemeral(
        &self,
        reference: &SecretVersionReference,
        purpose: SecretReadPurpose,
    ) -> Result<EphemeralSecretMaterial, SecretBackendError>;
    async fn disable_version(
        &self,
        reference: &SecretVersionReference,
    ) -> Result<(), SecretBackendError>;
    async fn delete_version(
        &self,
        reference: &SecretVersionReference,
    ) -> Result<SecretDeletionObservation, SecretBackendError>;
}
```

`SensitiveSecretInput` and `EphemeralSecretMaterial` are Vestrace-owned non-serializable zeroizing wrappers in `vestrace-credential-runtime`. Authorization/rotation may stage; only a consumed lease, exchange/refresh, validation/revocation signer or isolated known-secret scanner may read with an exact purpose.

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

Clear OAuth state is returned once and only its HMAC is persisted. Callback code and PKCE verifier use exact secret versions. After successful exchange the code/verifier versions are disabled and deleted under short-retention cleanup. Refresh is singleflight per Connection/generation; `invalid_grant` moves the Connection to `ReauthorizationRequired`.

### Delegated grants and remote profiles

```rust
pub enum ConnectionGrantConsumer {
    SubRun { child_run_id: AgentRunId, binding_id: SubRunBindingId },
    RemoteInvocation { invocation_id: RemoteAgentInvocationId },
    AgentSnapshot { agent_snapshot_id: AgentRuntimeSnapshotId },
}

pub struct ConnectionAccessGrant {
    pub id: ConnectionAccessGrantId,
    pub connection_id: ConnectionId,
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

A remote profile is a local immutable allowlist. Agent Card declarations may only reduce the compatible intersection.

### Credential leases and injection

```rust
pub enum CredentialSourceRef {
    Connection { connection_id: ConnectionId },
    ServiceBinding { binding_id: ServiceCredentialBindingId },
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

pub enum CredentialLeaseStatus { Issued, Consumed, Expired, Revoked, Failed }

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
    pub operation_fingerprint: OperationFingerprint,
    pub protected_action_ticket_id: AuthorizationTicketId,
    pub secret_use_ticket_id: AuthorizationTicketId,
    pub access_grant_id: Option<ConnectionAccessGrantId>,
    pub injection_scheme: CredentialInjectionScheme,
    pub maximum_uses: u16,
    pub use_count: u16,
    pub expires_at: Timestamp,
    pub status: CredentialLeaseStatus,
    pub created_at: Timestamp,
}
```

Interactive setup/validation uses an authenticated principal with no Run; Run work includes agent/Run/step. System maintenance uses a dedicated service principal. Both protected-action and `secret.use` tickets bind the same normalized operation fingerprint.

Consumption atomically verifies current Connection/service binding, credential generation, delegated grant, destination, expiry, revocation and use count; increments counters; appends a usage intent; then resolves the exact component inside `vestrace-credential-runtime`. Query injection and persistent cookie jars are not supported by the standard profile.

### Sandbox credential proxy

```rust
pub struct SandboxCredentialProxySession {
    pub id: SandboxCredentialProxySessionId,
    pub sandbox_session_id: SandboxSessionId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub connection_id: ConnectionId,
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

The workload receives an ephemeral proxy capability over an H4-owned local channel. The proxy validates session, origin, DNS pin, method/path/body limits; strips credential headers; consumes a fresh lease; injects outside the workload namespace; and returns a bounded/redacted response.

### Secret leak records and separate audits

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
    pub operation_fingerprint: OperationFingerprint,
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

Usage audit describes lease-bound request handling. Lifecycle audit describes authorization/refresh/rotation/revocation/leak decisions. Neither contains headers, token fragments, URL query strings, bodies or backend paths.

---

### Task 1: Add Connector definition and operation contracts

**Files:** modify `id.rs`; create `connector/{mod,definition,operation,resource,scope,binding}.rs`; export from domain.

- [ ] Add all H8 IDs, including `ServiceCredentialBindingId`, `CredentialRefreshId`, `SecretLeakScanId` and `CredentialLifecycleAuditRecordId`.
- [ ] Write failing tests for duplicate operations, unsafe header names, invalid origins/resources/scopes, unsupported write semantics and revision/content-hash mismatch.
- [ ] Implement stable identifiers and immutable hashing across binding, auth schemes, scopes, operations, schemas and data handling.
- [ ] Require each non-read-only operation to declare idempotency and reconciliation support explicitly.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain connector
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(connector): add connector operation contracts"
```

### Task 2: Add Connection, service binding and permission-ceiling contracts

**Files:** create `connection/{mod,connection,owner,sharing,permission,status,validation}.rs`; create credential service-binding types.

- [ ] Test owner/workspace invariants, complete lifecycle table, ceiling wider than Connector, unsafe identity display, service origin mismatch and optimistic revisions.
- [ ] Implement deterministic intersection for operation/resource/scope/classification/risk.
- [ ] Define explicit principal/agent sharing rows; `WorkspaceAgents` still requires eligible Agent snapshot and H2.
- [ ] Keep service bindings non-model-visible and limited to exact origins/operations/classification.
- [ ] Run/commit.

### Task 3: Add secret contracts and local encrypted backend

**Files:** create credential reference/set types, `vestrace-credential-runtime`, `vestrace-secret-backend-local`, test support/conformance and `secret_backend_local_conformance.rs`.

- [ ] Compile/runtime tests prove material wrappers are not Clone/Serialize/Debug, errors are redacted and buffers zeroize.
- [ ] One backend conformance suite covers stage/promote/inspect/purpose-read/disable/delete, idempotent promotion, ambiguous reconciliation, generation isolation and wrong-master-key failure.
- [ ] Local backend uses an owner-only master-key file or inherited descriptor, XChaCha20-Poly1305, random nonce, AAD `(deployment, backend binding, secret ID, version ID, generation)`, exclusive staging, fsync and atomic rename.
- [ ] Paths derive only from opaque IDs, never connector/user/identity text.
- [ ] Run/commit.

### Task 4: Define H8 ports, commands and deterministic fixtures

**Files:** create application connector/connection/credential/remote modules and `vestrace-credential-test-support`.

**Interfaces:** produce repository ports, `ConnectorAuthorizationPort`, `ConnectorValidationPort`, `CredentialRefreshPort`, `SecretBackendPort`, `CredentialBrokerPort`, `CredentialLeaseConsumerPort`, `ConnectionAccessGrantPort`, `RemoteAgentCredentialDecoratorPort`, `SandboxCredentialProxyPort`, `KnownSecretScannerPort` and fault injection.

- [ ] Add object-safety compile tests for every port.
- [ ] Fault points: after secret stage/promote, OAuth code stage, token response, credential-generation persistence, binding activation, lease consume, proxy injection, rotation activation and audit append.
- [ ] Connector adapters receive ephemeral material only for exchange/refresh/validation/revoke and return normalized safe observations.
- [ ] Deterministic connector supports PKCE, API-key header, identity validation, refresh, revoke and an exchange response-loss script.
- [ ] Run/commit.

### Task 5: Persist Connector definitions, Connections and service bindings

**Files:** create migrations `0054`, `0055`, PostgreSQL repositories and connector/Connection persistence tests.

- [ ] `0054` creates stable definitions, immutable revisions, operations/scopes/resources, adapter bindings and secret-backend bindings.
- [ ] `0055` creates Connections, service credential bindings, permission ceilings, sharing, granted external scopes and validation records.
- [ ] Enforce same workspace, exact current revisions, append-only revisions/validations and one active binding pointer.
- [ ] New Connector revisions never silently alter existing Connection semantics; rebinding is explicit and validated.
- [ ] Run/commit.

### Task 6: Implement PKCE and secure API-key authorization

**Files:** create migration `0056`, authorization/binding repositories/services, secure HTTP/CLI routes and OAuth/API-key/unknown tests.

- [ ] PKCE test covers state HMAC, callback replay, wrong principal/workspace, expiry, exact verifier/code secret versions, exchange, validation and activation.
- [ ] Lost token-exchange response produces `Unknown`, stores no guessed generation and requires connector reconciliation or reauthorization; no blind code replay.
- [ ] API-key test proves value absent from HTTP logs, H7 interactions, PostgreSQL, traces and errors.
- [ ] `0056` creates authorization flows/events, credential sets/generations/components, Connection bindings, refresh/rotation rows and temporary secret bindings.
- [ ] Callback immediately stages code; exchange stages/promotes token components, persists one generation, validates, activates binding, then disables/deletes code/verifier versions.
- [ ] H7 sees opaque flow status only.
- [ ] Run/commit.

### Task 7: Implement validation, refresh, rotation, reauthorization and revocation

**Files:** create validation/revocation/refresh/rotation services, repositories/workers and tests.

- [ ] Validation calls a Connector-declared safe identity operation and stores only identity/scope hashes and stable codes.
- [ ] Twenty concurrent refresh requests use one singleflight operation and one new access-token generation.
- [ ] Rotation stages, validates, atomically activates, revokes old unconsumed leases and schedules old-version retention/deletion.
- [ ] Crash after backend promote reconciles the same version/generation.
- [ ] Revocation marks stable identity Revoked, revokes issued leases/grants, attempts connector revoke when supported and preserves in-flight uncertainty.
- [ ] `invalid_grant` sets `ReauthorizationRequired` and opens/updates a matching H7 AuthenticationRequired request.
- [ ] Run/commit.

### Task 8: Persist delegated Connection grants and remote profiles

**Files:** create migration `0057`, grant/profile services/repositories/tests.

- [ ] Effective grant = Connection ceiling ∩ H5 delegation scope ∩ request ∩ H2 policy.
- [ ] Bind to one exact SubRun, RemoteAgentInvocation or Agent snapshot; enforce expiry, maximum leases and one workspace.
- [ ] Child cannot use omitted operation/resource/classification or parent-only Connection.
- [ ] Agent Card-declared schemes/origins/skills cannot widen local remote profile.
- [ ] `0057` creates grants, operation/resource rows, counters and immutable profile revisions.
- [ ] Run/commit.

### Task 9: Implement atomic credential leases

**Files:** create migration `0058`, Broker/lease services/repository/runtime consumer and atomicity/restart tests.

- [ ] Issue requires matching protected-action and `secret.use` tickets, current source/generation, exact operation/resource/origin, allowed injection scheme and valid optional grant.
- [ ] Duplicate idempotency returns identical lease; conflicting payload fails.
- [ ] Consume locks lease, source, generation and grant in deterministic order; verifies all fences; increments use/grant counters and appends usage intent before secret read.
- [ ] Concurrent one-use consumption yields exactly one success.
- [ ] Crash after consume leaves the lease consumed; the owning operation must prove retry safety before requesting a new lease.
- [ ] `0058` creates leases, resource/origin rows, uses, proxy sessions/capability HMACs and revocation indexes.
- [ ] Run/commit.

### Task 10: Integrate leases with H3, H4 and H6

**Files:** modify H3 provider adapters, H4 HTTP/MCP/native adapters and H6 authenticated fetch/store decorators; add provider/tool/fetch tests.

- [ ] H3 uses `ServiceCredentialBinding` bound to exact provider origin/attempt and injects only inside adapter.
- [ ] H4 uses Connection + Connector operation/resources matching the H4 commit fingerprint; tool arguments expose only `LogicalConnectionHandle`.
- [ ] H6 authenticates only after SSRF/origin checks; cross-origin redirect requires new authorization/lease and is denied by default.
- [ ] Prove no backend ref/generation/lease/material appears in prompts, tool results, Artifact metadata, work items or events.
- [ ] Ambiguous completion consumes the lease and follows owner reconciliation; Broker does not retry.
- [ ] Run/commit.

### Task 11: Implement sandbox credential proxy

**Files:** create `vestrace-credential-proxy`, H4 composition and `sandbox_credential_proxy.rs`.

- [ ] Workload with no credential env/mount reaches one allowed HTTPS origin/path; upstream sees injected auth while workload capture does not.
- [ ] Reject wrong origin/IP/DNS pin/method/path/body, caller auth/cookie headers, expired capability, wrong sandbox and overuse.
- [ ] Capability clear value is passed only through H4 local socket/memfd-style channel; PostgreSQL stores HMAC only.
- [ ] Consume a fresh lease per request; enforce H4 egress limits and bounded/redacted response.
- [ ] No generic TCP/SOCKS/unrestricted proxy.
- [ ] Run/commit.

### Task 12: Integrate H7 authentication continuations

**Files:** modify H7 authentication/continuation and HTTP/CLI commands; add `authentication_continuation.rs`.

- [ ] AuthenticationRequired references opaque H8 Connection/flow IDs and rejects credentials in HumanResponse.
- [ ] Starting authorization returns safe instruction/URL directly to authenticated requester; state/code never enter notification or interaction history.
- [ ] Completion enqueues exact H7 `AuthenticationReady` cause.
- [ ] Resume rechecks Connection, required operation/resource, grant, current policy and active generation; unrelated flow cannot resume.
- [ ] CLI API-key input disables echo and avoids shell args/history.
- [ ] Run/commit.

### Task 13: Integrate remote-agent request-scoped authentication

**Files:** create remote profile/service/decorator runtime, modify H5 fixtures and add remote-agent credential tests.

- [ ] Validate exact remote revision/profile, verified identity/origin, transport, operation, resources, classification and H5 grant.
- [ ] Use a distinct lease and inject only in infrastructure transport.
- [ ] Remote Card/message/task/metadata receives no Vestrace refs, tickets, grants, leases or material.
- [ ] Lost response consumes lease and sets H5 `Unknown`; reconciliation never reuses it.
- [ ] Profile/Connection revocation blocks later dispatch/continuation while preserving remote task/audit.
- [ ] H9A can consume this boundary without A2A types in H8.
- [ ] Run/commit.

### Task 14: Add known-secret detection and separate audits

**Files:** create migration `0059`, scanner/audit/leak services/repositories, H3/H6/H7 integration and tests.

- [ ] Scanner loads eligible values into isolated zeroizing memory, returns only refs/rules/ranges/fingerprints.
- [ ] Test current and retained rotated secrets, credential patterns, false-positive policy and finding serialization without fragments.
- [ ] Apply Deny before model transfer, Redact for permitted ordinary text, Quarantine for Artifact and Alert for diagnostics.
- [ ] `0059` creates append-only scan/findings, usage audit, lifecycle audit and safe diagnostics.
- [ ] Usage records distinguish injection/dispatch/rejection/unknown/failure; lifecycle records distinguish authorization/validation/refresh/rotation/reauth/disable/revoke/leak block.
- [ ] Persistence script rejects secret-bearing SQL/schema/event/public fields.
- [ ] Run/commit.

### Task 15: Integrate H8 workers, checkpoint V6, RLS and acceptance

**Files:** create migration `0060`, worker/Run/checkpoint changes, boundary scripts, CI and acceptance/RLS tests.

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

Payloads contain stable IDs/revisions/deadlines only—never material, code, verifier, clear state, proxy capability or ephemeral credential.

- [ ] Logical Run events only when entering auth wait, binding an available Connection or pausing due to revocation. Lease/use/refresh/audit progress remains H8-local.
- [ ] `RunCheckpointV6` stores required Connection IDs, active flow IDs and delegated grant IDs only; it stores no secret refs, generations, leases or capabilities.
- [ ] `0060` forces RLS, same-workspace/owner/consumer consistency, immutable/append-only guards and active flow/refresh/lease/grant/expiry/audit indexes.
- [ ] Boundary scripts reject serializable/debuggable material, secret-like work/event/public fields, backend reads outside credential runtime, raw credentials in H3–H7, sandbox env/mount credentials and A2A credential fields.
- [ ] Acceptance proves:
  1. immutable Connector revision with PKCE/API-key schemes;
  2. OAuth Connection with no clear code/token in PostgreSQL/logs/events;
  3. exchange response-loss becomes Unknown without blind replay;
  4. API-key secure input creates no InteractionEvent copy;
  5. provider service binding and tool Connection use separate short leases;
  6. model sees only logical handles;
  7. sandbox proxy exposes no secret to workload;
  8. SubRun grant is narrower than parent;
  9. remote dispatch has identity/origin-bound lease;
  10. lost external response consumes lease and does not duplicate work;
  11. refresh is singleflight;
  12. rotation activates one generation and revokes obsolete leases;
  13. revocation blocks future tool/remote leases;
  14. matching H7 auth continuation resumes, unrelated one does not;
  15. known secret is denied/redacted/quarantined by target policy;
  16. usage/lifecycle audit is complete and fragment-free;
  17. restart at every store/auth/rotation/lease/proxy fault creates no duplicate generation, use or call;
  18. replay performs no secret-bearing action.
- [ ] Required CI jobs: Connector/Connection domain, encrypted backend, authorization, refresh/rotation/revoke, grants/profiles, lease atomicity, provider/tool/fetch injection, proxy, remote auth, leak/audit, boundaries, H8 acceptance.
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
0055 Task 5  Connections, service bindings, sharing, scopes and validation
0056 Task 6  Authorization flows, credential bindings and rotations
0057 Task 8  Delegated Connection grants and remote profiles
0058 Task 9  Credential leases, uses and proxy sessions
0059 Task 14 Secret-leak findings and credential audits
0060 Task 15 RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H8 completion definition

H8 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
ConnectorDefinitionRevision or ServiceCredentialBinding
→ local permission ceiling
→ authorization/secret generation
→ validation
→ protected external operation + secret.use
→ exact CredentialLease
→ atomic consumption
→ ephemeral injection/proxy decoration
→ external request
→ owning operation reconciliation
→ usage audit
→ refresh/rotation/revocation
```

Required invariants:

1. PostgreSQL stores no secret material; secret backend does not authorize use.
2. Connector/remote declarations cannot grant local authority.
3. External scopes/sharing cannot exceed local ceiling and H2.
4. OAuth state/code/verifier/token lifecycles are one-time, bounded and restart-safe without clear persistence.
5. Ambiguous exchange is Unknown, not blind replay.
6. API-key input bypasses ordinary interactions/logging.
7. Material wrappers cannot clone/serialize/debug values and zeroize on drop.
8. Every outbound use consumes an exact lease immediately before injection.
9. Consumed leases are never reused after crash/timeout/unknown completion.
10. Refresh is singleflight and refresh tokens are never ordinary outbound credentials.
11. Rotation activates one immutable generation and revokes obsolete leases.
12. Revocation blocks new leases while preserving in-flight uncertainty/audit.
13. SubRuns/remote invocations use explicit narrowed grants.
14. Remote auth binds local identity/origin/transport/operation/resource/classification.
15. Models see logical Connection handles only.
16. Sandboxes receive no long-lived secret; proxy capability cannot escape exact session/destination.
17. Provider/tool/fetch/remote adapters share one lease/runtime contract.
18. Known-secret detection persists no matched value.
19. Usage and lifecycle audits are append-only and content-free.
20. H7 authentication continuations contain opaque status only.
21. No A2A SDK type or Agent Card credential enters H8 core.
22. Restart does not duplicate generations, lease consumption, rotation or connector calls.
23. Replay performs no credential-bearing action.
24. H9 can register Connector/adapter revisions without changing H8 semantics.
25. H9A can consume remote profiles/leases without changing H8 types.
26. H10 can add integrity/metrics without rewriting H8 records.
27. H8 tests require no public provider, SaaS account, remote agent or permanent secret.

## Explicit non-goals

H8 does not implement production Gmail/Slack/CRM connectors, organization OIDC/SCIM, a marketplace, Vault/cloud-secret-manager adapters, HSM-backed signing, OAuth device flow, browser UI, generic TCP/SOCKS tunneling, unrestricted cookie jars, permanent sandbox env credentials, cross-organization credential delegation, Agent Card trust, A2A transport or any API returning secret material.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h8-connections-credential-broker`, add crypto/OAuth dependencies, create migrations `0054`–`0060`, create a master key/store, start callback/proxy servers, submit/rotate real credentials, modify production adapters, change CI or execute H8 tests.
