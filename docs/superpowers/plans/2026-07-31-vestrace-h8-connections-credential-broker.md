# Vestrace H8 Connections and Credential Broker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start authorization servers, store credentials, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement immutable connector definitions, user/workspace-bound connections, OAuth/API-key authorization, replaceable secret storage, operation-bound credential leases, delegated connection access, request-scoped injection for providers/tools/remote agents, a sandbox credential proxy, rotation/revocation, leak controls and complete credential-usage audit without exposing secret material to models or durable runtime state.

**Architecture:** H8 separates connector capability metadata, connection lifecycle, secret material and credential use. PostgreSQL stores connector/connection definitions, authorization-flow metadata, opaque secret references, grants, leases and audit; secret bytes live only behind `SecretBackendPort`. A worker first proves the protected external operation through H2/H3/H4/H5, then the Credential Broker issues a short-lived lease bound to the exact principal, Run/step/invocation, destination, operation, resources, credential generation and maximum use count. Infrastructure adapters consume the lease immediately before dispatch and resolve an ephemeral zeroizing credential value; models, sandboxes, public DTOs, work items and durable events receive only logical connection handles.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H7 Rust workspace; Rust Edition 2024; Tokio; Axum/Tower for secure callback and credential-submission routes; Serde/Schemars for non-secret contracts; SQLx and PostgreSQL 17; SHA-256 and HMAC-SHA-256; OAuth 2.1 Authorization Code with PKCE S256; zeroize/secrecy-style non-serializable memory wrappers; RustCrypto XChaCha20-Poly1305 for the initial local encrypted secret backend; deterministic OAuth/API-key/remote/sandbox fixtures; proptest; tracing with mandatory redaction.

## Global Constraints

- Complete all five v0.1 plans and H1–H7 before implementing H8.
- Harness design section `20. Connector Registry and Credential Broker` is normative.
- PostgreSQL is authoritative for connector definitions/revisions, connections, permission ceilings, authorization-flow state, credential bindings, rotations, validations, delegated grants, remote-agent profiles, leases, uses, proxy sessions, leak findings and audit metadata.
- Secret bytes, OAuth authorization codes, PKCE verifiers, access/refresh tokens, API keys, client secrets, passwords and private keys live only behind `SecretBackendPort` and are never PostgreSQL values.
- Vestrace owns every domain/application contract, identifier, lifecycle, persisted schema, operation fingerprint and public DTO.
- OAuth, HTTP, secret-backend, keyring, cloud SDK, A2A and connector-specific types may not appear in domain/application signatures or PostgreSQL schemas.
- `ConnectorDefinitionRevision`, `RemoteAgentConnectionProfileRevision`, credential-set generations and permission ceilings are immutable revisions. Mutable lifecycle belongs to stable identity records with optimistic state revisions.
- `Connection` is a logical authorization relationship, not a secret. It may be principal-bound or workspace-bound and always has an explicit local permission ceiling.
- External scopes are provider grants, not local authority. Effective use is the intersection of connector operation requirements, connection ceiling, delegated grant, current H2 policy and the exact protected action.
- A `SecretReference` or content hash is never an authorization capability. Reading secret material requires an unexpired consumed credential lease at an enforcement point.
- Secret material types are non-Clone, non-Serialize, non-Debug and zeroized on drop. Clear values are never placed in error messages, traces, panic payloads or test snapshots.
- Connector definitions and Agent Card security declarations cannot automatically create a Connection, credential, grant or lease.
- OAuth uses Authorization Code + PKCE S256. Implicit flow and resource-owner password flow are rejected. State is one-time and stored only as HMAC; authorization codes and PKCE verifiers are short-lived secret-backend objects.
- API-key submission uses a dedicated sensitive-input route/CLI path whose body is excluded from request logging and InteractionEvents. H7 records only an opaque completed/failed authorization-flow reference.
- OAuth token exchange stores returned token components directly in the secret backend before durable completion. Raw token responses are never persisted.
- Refresh tokens are broker-internal and can never be selected as an outbound connector credential component.
- Credential rotation creates a new immutable generation, validates it, atomically activates the binding and revokes unconsumed leases tied to older generations. Old material follows explicit retention/deletion policy.
- Connection revocation prevents new leases immediately. It cannot claim that an already dispatched external request was recalled.
- Every credential lease is bound to workspace, principal, acting agent, Run/step or invocation, protected-operation fingerprint, connector operation, resources, destination/origin, credential generation, injection scheme, expiry and maximum uses.
- Credential leases default to one use and at most 60 seconds. Longer or multi-use leases require explicit policy and are not used by the standard v0.2 profile.
- Lease consumption occurs immediately before credential resolution/injection. A consumed lease is not reused after timeout, crash or ambiguous external completion.
- H3 provider, H4 tool, H5 remote-agent, H6 authenticated fetch/store and H9A A2A adapters receive the same Vestrace-owned lease handle and infrastructure injection boundary.
- Models see only sanitized `LogicalConnectionHandle` values and connector operation metadata. They never see secret references, lease IDs, access tokens, API keys or proxy capabilities.
- Sandboxes receive no long-lived credential in environment variables, command-line arguments, mounted files or Artifact bytes. The initial sandbox integration uses an application-level HTTP credential proxy with destination and operation constraints.
- The sandbox proxy strips caller-supplied authorization/cookie/proxy-authentication headers before broker injection.
- Remote-agent authentication is bound to an exact local remote-agent revision, verified identity/origin, allowed transport, operation, resources and maximum data classification. Remote Agent Cards cannot widen this profile.
- Delegated SubRuns and remote invocations use separate `ConnectionAccessGrant` records. A child cannot use the parent Connection outside its exact grant.
- Known-secret leak detection returns only secret/version references, rule IDs, locations and irreversible fingerprints. Secret values never appear in findings.
- Secret detection is applied before model transfer, ordinary event persistence, notification rendering, Artifact availability/export and connector diagnostics according to the owning subsystem policy.
- Credential usage audit is append-only and content-free. H10 may add integrity chains and projections without rewriting H8 history.
- Replay never starts OAuth, reads secret material, refreshes tokens, issues/consumes leases, calls a connector, injects credentials or delivers a proxy request.
- Existing migrations `0014`–`0053` are never edited. H8 migrations are `0054`–`0060` and are created once.
- CI uses deterministic loopback OAuth/resource servers, in-memory and temporary encrypted secret backends, fake clocks and fake remote/sandbox clients. No public provider, SaaS account, A2A server or permanent credential is required.
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

crates/vestrace-channel-http/src/
  connection_routes.rs
  authorization_routes.rs
  oauth_callback.rs
  sensitive_body.rs

crates/vestrace-channel-cli/src/
  connection_commands.rs
  secure_input.rs

crates/vestrace-infrastructure/src/postgres/
  connector/{mod,definition_repository,binding_repository}.rs
  connection/{mod,repository,sharing_repository,validation_repository}.rs
  credential/{mod,authorization_repository,binding_repository,rotation_repository,grant_repository,lease_repository,audit_repository,leak_repository}.rs
  remote_connection/{mod,profile_repository}.rs

migrations/
  0054_connector_definitions_operations_and_backend_bindings.sql
  0055_connections_sharing_scopes_and_validation.sql
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

### Connector definitions and operations

```rust
pub enum ConnectorDefinitionStatus { Draft, Active, Deprecated, Revoked }

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
    pub required_external_scopes: Vec<String>,
    pub allowed_resource_patterns: Vec<ConnectorResourcePattern>,
    pub allowed_authentication_schemes: Vec<ConnectorAuthenticationScheme>,
    pub maximum_request_bytes: u64,
    pub maximum_response_bytes: u64,
    pub supports_idempotency_key: bool,
    pub supports_reconciliation: bool,
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

Connector operation/resource/scope names are normalized stable identifiers. Header names are validated against a fixed safe allowlist; `Authorization`, connector-specific `X-Api-Key`-style names and signed-request metadata are supported, while `Cookie`, `Proxy-Authorization`, `Host`, `Content-Length` and hop-by-hop headers are forbidden as connector-defined injection targets.

### Connections and local ceilings

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
```

`external_identity_display` is a bounded redacted label, not an authentication assertion. Sharing rows only select locally eligible principals/agent snapshots; every use still passes H2.

### Secret references, sets and backend

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
```

`RefreshToken`, `ClientSecret`, `Password` and `PrivateKey` are never eligible outbound components unless an internal protocol-specific exchange/signer explicitly consumes them. They cannot be selected by model/tool arguments.

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

`SensitiveSecretInput` and `EphemeralSecretMaterial` are Vestrace-owned non-serializable zeroizing wrappers defined in `vestrace-credential-runtime`. Only authorization/rotation services may stage material; only a consumed lease, token refresh/exchange, validation or leak scanner may read it with an enumerated `SecretReadPurpose`.

### Authorization flows

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
    pub verifier_secret_reference: Option<SecretReference>,
    pub authorization_code_secret_reference: Option<SecretReference>,
    pub expires_at: Timestamp,
    pub logical_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}
```

Clear OAuth state is returned once to the initiating user agent and only its HMAC is persisted. Callback codes are immediately staged as short-retention secrets before asynchronous exchange. A callback with wrong/used/expired state performs no exchange.

### Rotation, refresh and validation

```rust
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

Refresh is singleflight per connection/generation. `invalid_grant` moves the Connection to `ReauthorizationRequired`; transient refresh failure moves it to `Degraded` without exposing provider error bodies.

### Delegated access and remote-agent profiles

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
    pub allowed_origins: Vec<String>,
    pub allowed_transports: std::collections::BTreeSet<String>,
    pub allowed_operations: std::collections::BTreeSet<ConnectorOperationId>,
    pub allowed_resources: Vec<ConnectorResourcePattern>,
    pub maximum_classification: DataClassification,
    pub injection_scheme: CredentialInjectionScheme,
    pub content_hash: [u8; 32],
}
```

A remote profile is a local immutable allowlist. Remote declarations can narrow compatibility but cannot widen it.

### Credential leases and injection

```rust
pub enum CredentialConsumerRef {
    ModelAttempt { attempt_id: ModelExecutionAttemptId },
    ToolAttempt { attempt_id: ToolInvocationAttemptId },
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

pub struct CredentialLease {
    pub id: CredentialLeaseId,
    pub workspace_id: WorkspaceId,
    pub connection_id: Option<ConnectionId>,
    pub credential_generation_id: CredentialSetGenerationId,
    pub consumer: CredentialConsumerRef,
    pub principal_id: PrincipalId,
    pub acting_agent_snapshot_id: AgentRuntimeSnapshotId,
    pub run_id: AgentRunId,
    pub step_id: Option<RunStepId>,
    pub connector_operation_id: ConnectorOperationId,
    pub resource_hashes: Vec<[u8; 32]>,
    pub destination_origin: String,
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

The broker issues a lease only after both the protected operation and `secret.use` authorizations match the same normalized operation fingerprint. Consumption atomically checks current Connection/credential/grant status, increments use count, writes an audit-use record and only then resolves the exact secret component.

`EphemeralCredentialMaterial` is returned only inside `vestrace-credential-runtime` to an adapter injector and is destroyed after request construction/signing. Query-string injection and persistent cookie jars are not supported in the standard profile.

### Sandbox credential proxy

```rust
pub struct SandboxCredentialProxySession {
    pub id: SandboxCredentialProxySessionId,
    pub sandbox_session_id: SandboxSessionId,
    pub run_id: AgentRunId,
    pub step_id: RunStepId,
    pub connection_id: ConnectionId,
    pub allowed_origin: String,
    pub allowed_methods: std::collections::BTreeSet<String>,
    pub allowed_path_patterns: Vec<String>,
    pub maximum_requests: u16,
    pub maximum_request_bytes: u64,
    pub maximum_response_bytes: u64,
    pub expires_at: Timestamp,
    pub status: SandboxCredentialProxyStatus,
}
```

The workload receives only an ephemeral proxy capability over an H4-managed local socket/channel. The proxy validates sandbox/session/origin/method/path/body limits, strips supplied credential headers, consumes a fresh lease and injects the credential outside the workload namespace.

### Leak findings and audit

```rust
pub enum SecretLeakDisposition { Deny, Redact, Quarantine, Alert }

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
    pub connection_id: Option<ConnectionId>,
    pub credential_generation_id: CredentialSetGenerationId,
    pub lease_id: CredentialLeaseId,
    pub principal_id: PrincipalId,
    pub acting_agent_snapshot_id: AgentRuntimeSnapshotId,
    pub run_id: AgentRunId,
    pub step_id: Option<RunStepId>,
    pub consumer: CredentialConsumerRef,
    pub operation_fingerprint: OperationFingerprint,
    pub destination_origin_hash: [u8; 32],
    pub result: CredentialUseResult,
    pub occurred_at: Timestamp,
}
```

Audit records contain no header values, token fragments, URLs with query strings, request/response bodies or secret-backend locations.

---

### Task 1: Add connector definition and operation domain contracts

**Files:** create/modify `crates/vestrace-domain/src/id.rs`, `connector/{mod,definition,operation,resource,scope,binding}.rs`, `lib.rs`.

- [ ] Add IDs for connectors, revisions, adapter bindings, operations, Connections, credential sets/generations, flows, rotations, grants, leases, proxy sessions, leak findings and audit records.
- [ ] Write failing tests for duplicate operation IDs, unsafe injection header names, operation scope mismatch, invalid resource patterns and revision-zero/content-hash mismatch.
- [ ] Implement immutable revision hashing over adapter binding, auth schemes, scopes, operations, schemas and data-handling declaration.
- [ ] Require every write/destructive operation to declare idempotency/reconciliation support explicitly.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain connector
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(connector): add connector operation contracts"
```

### Task 2: Add Connection lifecycle, sharing and permission ceilings

**Files:** create `connection/{mod,connection,owner,sharing,permission,status,validation}.rs`; modify domain exports.

- [ ] Test owner/workspace invariants, invalid lifecycle transitions, ceiling wider than connector revision, unsafe external-identity display and optimistic state revisions.
- [ ] Implement status transitions: `PendingAuthorization → Active|ReauthorizationRequired|Disabled`, `Active → Degraded|ReauthorizationRequired|Disabled|Revoked|Expired`, terminal `Revoked`, and policy-controlled re-enable from `Disabled`.
- [ ] Implement deterministic intersection for operations/resources/scopes/classification/risk.
- [ ] Define explicit principal/agent sharing entries; `WorkspaceAgents` still requires a matching locally registered agent snapshot and H2 decision.
- [ ] Run/commit.

### Task 3: Add secret contracts and local encrypted backend conformance

**Files:** create credential reference/set contracts, `vestrace-credential-runtime`, `vestrace-secret-backend-local`, test-support backend/conformance and `tests/secret_backend_local_conformance.rs`.

- [ ] Add compile/runtime tests proving sensitive material is not Clone/Serialize/Debug, redacted errors contain no value and buffers zeroize on drop.
- [ ] Define conformance for stage/promote/inspect/read-purpose/disable/delete, idempotent promotion, ambiguous reconciliation, generation isolation and wrong-master-key failure.
- [ ] Implement local backend using a deployment master key loaded from an owner-only file or inherited descriptor, XChaCha20-Poly1305 with random nonce and AAD `(deployment, backend binding, secret ID, version ID, generation)`, exclusive files, fsync and atomic rename.
- [ ] Never derive secret file paths from connector/user/external identity text.
- [ ] Run/commit.

### Task 4: Define H8 application ports, commands and deterministic fixtures

**Files:** create application connector/connection/credential/remote modules and `vestrace-credential-test-support` fixtures.

**Interfaces:** produce `ConnectorDefinitionRepositoryPort`, `ConnectionRepositoryPort`, `ConnectorAuthorizationPort`, `ConnectorValidationPort`, `CredentialRefreshPort`, `SecretBackendPort`, `CredentialBrokerPort`, `CredentialLeaseRepositoryPort`, `CredentialLeaseConsumerPort`, `ConnectionAccessGrantPort`, `RemoteAgentCredentialDecoratorPort`, `SandboxCredentialProxyPort`, `KnownSecretScannerPort` and deterministic clocks/faults.

- [ ] Add object-safety compile tests for every port.
- [ ] Define one-shot faults after secret stage, secret promote, OAuth code stage, token exchange, credential binding commit, lease consume, proxy injection, rotation activation and audit append.
- [ ] Connector adapters receive ephemeral material only for exchange/validation/revoke and return normalized safe observations.
- [ ] Add a deterministic connector supporting OAuth PKCE, API-key header, identity validation, refresh and revocation.
- [ ] Run/commit.

### Task 5: Persist connector definitions and Connections

**Files:** create migrations `0054` and `0055`, PostgreSQL connector/connection repositories and persistence tests.

- [ ] `0054` creates stable connector definitions, immutable revisions, operations/scopes/resources, adapter-binding revisions and secret-backend-binding revisions.
- [ ] `0055` creates Connections, permission ceilings, sharing entries, granted external scopes and validation records.
- [ ] Enforce same-workspace ownership, exact current connector revision and append-only revisions/validations.
- [ ] Connector revision changes never mutate existing Connection semantics; rebinding requires an explicit validated Connection revision/update command.
- [ ] Run/commit.

### Task 6: Implement OAuth PKCE and secure API-key authorization

**Files:** create migration `0056`, authorization/binding repositories, authorization services, secure HTTP/CLI routes and tests `oauth_pkce_flow.rs`, `api_key_submission.rs`.

- [ ] OAuth test covers state HMAC, PKCE S256, callback replay rejection, wrong principal/workspace, expired flow, code stored only in secret backend, token exchange and Connection activation after validation.
- [ ] API-key test submits through `SensitiveRequestBody`, proves the key is absent from HTTP access logs, H7 interactions, PostgreSQL and errors, then validates/activates the Connection.
- [ ] `0056` creates authorization flows/events, credential sets/generations/components, Connection credential bindings, rotation/refresh state and temporary secret-reference bindings.
- [ ] Callback stages the authorization code as a short-retention secret before enqueueing exchange; restart resumes from that reference without storing clear code.
- [ ] Token exchange stages every component, promotes them, persists one generation and validates before activating the binding.
- [ ] H7 receives only opaque flow completion/failure status.
- [ ] Run/commit.

### Task 7: Implement validation, refresh, rotation, reauthorization and revocation

**Files:** create connection validation/revocation and credential refresh/rotation services/repositories/workers; tests for singleflight, restart and revocation.

- [ ] Validation uses a connector-declared safe identity operation and stores only identity/scope hashes and stable safe codes.
- [ ] Refresh obtains a per-connection generation lock; twenty concurrent expired-token requests cause one refresh and all successful callers use the same new access-token generation.
- [ ] Rotation flow stages new material, validates it, atomically activates the new binding, revokes old unconsumed leases and schedules old-version retention/deletion.
- [ ] Crash after backend promote but before binding commit reconciles the same version; it never creates another generation.
- [ ] Revocation marks Connection `Revoked`, revokes issued leases/grants, calls connector revoke when supported and preserves audit. In-flight dispatched work remains governed by its own `Unknown`/reconciliation lifecycle.
- [ ] `invalid_grant` creates/updates an H7 AuthenticationRequired continuation and status `ReauthorizationRequired`.
- [ ] Run/commit.

### Task 8: Persist delegated connection grants and remote-agent profiles

**Files:** create migration `0057`, grant/profile domain/services/repositories and tests.

- [ ] Compute grant ceiling as parent Connection ceiling ∩ H5 delegation scope ∩ request ∩ current H2 policy.
- [ ] Bind a grant to one exact SubRun, RemoteAgentInvocation or Agent snapshot; enforce expiry, maximum leases and one workspace.
- [ ] Child tests prove omitted operation/resource/classification and parent-only Connection are inaccessible.
- [ ] Remote profile tests prove Agent Card-declared schemes/origins/skills cannot widen local origin, transport, operation, resource or classification allowlists.
- [ ] `0057` creates grants, operation/resource rows, use counters and immutable remote-agent profile revisions.
- [ ] Run/commit.

### Task 9: Implement atomic credential leases and persistence

**Files:** create migration `0058`, broker/lease services/repository, credential-runtime lease consumer and atomicity/restart tests.

- [ ] Issue requires matching protected-action and `secret.use` tickets, current Connection/credential generation, exact operation/resource/destination, eligible injection scheme and optional valid delegated grant.
- [ ] Duplicate lease idempotency returns the same lease; conflicting payload fails.
- [ ] Consume locks lease, Connection, binding/generation and grant in deterministic order, verifies expiry/revocation/use count/fingerprint, increments counters and appends audit-use intent before secret read.
- [ ] Concurrent one-use consumption yields exactly one success.
- [ ] Crash after consume before dispatch leaves the lease consumed; retry obtains a new lease only after the owning H3/H4/H5 operation proves retry safety.
- [ ] `0058` creates leases, resource/destination bindings, use records, proxy sessions/capability hashes and revocation indexes.
- [ ] Run/commit.

### Task 10: Integrate request-scoped credentials with H3, H4 and H6

**Files:** modify H3 provider adapter binding/invocation, H4 HTTP/MCP/native adapter execution and H6 authenticated fetch/store integration; add provider/tool/fetch tests.

- [ ] H3 provider attempt requests a lease bound to provider origin/model attempt and injects only inside the provider adapter immediately before send.
- [ ] H4 tool invocation binds connector operation/resources to the same operation fingerprint used by prepare/commit; model-visible tool arguments contain only `LogicalConnectionHandle`.
- [ ] H6 authenticated external fetch uses a Connection/lease decorator only after URL/origin SSRF checks; redirect to another origin requires a separately eligible lease and is denied by default.
- [ ] Adapter errors and request captures prove no secret reference/lease/material appears in provider messages, tool results, Artifact metadata, work items or durable events.
- [ ] Ambiguous external completion consumes the lease and follows the owning operation reconciliation; Broker never silently retries.
- [ ] Run/commit.

### Task 11: Implement the sandbox credential proxy

**Files:** create `vestrace-credential-proxy`, H4 composition integration and `tests/sandbox_credential_proxy.rs`.

- [ ] Test a sandbox with no environment/mount secret can call one allowed HTTPS origin/path through the proxy; the upstream receives the injected header while workload capture does not.
- [ ] Reject different origin, IP, method, path, oversized body, caller Authorization/Cookie/Proxy-Authorization headers, expired capability, wrong sandbox session and second use above limit.
- [ ] Proxy capability is generated after H4 sandbox creation, stored only as an HMAC in PostgreSQL and passed through an H4-managed local socket/memfd-style channel, never Artifact or command line.
- [ ] Proxy consumes a fresh credential lease per outbound request, enforces H4 egress/DNS pinning and bounds/redacts the response.
- [ ] Generic TCP/SOCKS tunneling and unrestricted forward proxying are not implemented.
- [ ] Run/commit.

### Task 12: Integrate H7 authentication continuations and secure channel surfaces

**Files:** modify H7 human authentication service, H7 trigger/run continuation, HTTP/CLI connection commands and add `tests/authentication_continuation.rs`.

- [ ] `HumanRequest::AuthenticationRequired` references an opaque H8 flow/Connection ID and never accepts credentials as `HumanResponse` content.
- [ ] Starting authorization returns a safe URL/CLI instruction plus opaque flow reference; completion records only `Completed|Cancelled|Failed` and enqueues the exact H7 `AuthenticationReady` RunContinuation cause.
- [ ] Run resume rechecks Connection status, exact required operation/resource, delegated grant, current policy and credential generation; completion of an unrelated flow cannot resume it.
- [ ] Secure API-key CLI input disables echo and writes directly to the H8 sensitive endpoint/service, not shell arguments/history.
- [ ] Notification rendering contains no authorization codes, state values, token fragments or secret references.
- [ ] Run/commit.

### Task 13: Integrate remote-agent request-scoped authentication

**Files:** create remote profile/service/decorator runtime, modify H5 remote dispatch boundary test fixtures and add `tests/remote_agent_credential_injection.rs`.

- [ ] Before dispatch, validate exact remote-agent revision/profile, verified identity/origin, transport, operation, resources, classification and H5 ConnectionAccessGrant.
- [ ] Issue/consume a distinct lease from any tool/provider lease; decorate only the transport request inside infrastructure.
- [ ] Remote fixture proves Agent Card/message/task/metadata never receives Vestrace secret reference, grant, ticket, lease or credential material.
- [ ] Lost dispatch response consumes the lease and sets H5 invocation `Unknown`; reconciliation uses external task identifiers and never reuses the credential lease.
- [ ] Profile/Connection revocation prevents new dispatch/continuation while preserving the remote task and audit history.
- [ ] H9A can implement this decorator behind `RemoteAgentPort` without changing H8 contracts or exposing `a2a-rs` types.
- [ ] Run/commit.

### Task 14: Add known-secret detection, redaction and usage audit

**Files:** create migration `0059`, scanner/audit/leak services/repositories, integrate H3/H6/H7 redaction points and tests.

- [ ] Known-secret scanner loads eligible secret values only into zeroizing isolated memory, scans bounded text/bytes and returns findings without values or recoverable fragments.
- [ ] Test exact current/retained secret detection, rotated-secret detection during retention, common credential patterns, false-positive handling and no secret in finding serialization.
- [ ] Policies apply `Deny` before model transfer, `Redact` for eligible ordinary text, `Quarantine` for Artifact content and `Alert` for diagnostic/audit targets.
- [ ] `0059` creates append-only leak scans/findings, credential usage audit, Connection lifecycle audit and safe connector diagnostic records.
- [ ] Audit result distinguishes injected, request-dispatched, rejected-before-dispatch, completion-unknown, refresh, rotate, revoke and leak-blocked without storing request/response content.
- [ ] Add repository script scanning SQL/schema/event/public DTOs for forbidden secret-bearing columns/fields.
- [ ] Run/commit.

### Task 15: Integrate H8 workers, checkpoint V6, RLS and acceptance

**Files:** create migration `0060`, H8 worker/run event/work/checkpoint changes, boundary scripts, CI and acceptance/RLS tests.

- [ ] Add work kinds:

```text
ExchangeAuthorizationCode
ValidateConnection
RefreshConnectionCredential
RotateConnectionCredential
RevokeConnection
ReconcileSecretBackend
ReconcileCredentialRotation
ExpireCredentialLeases
ExpireConnectionAccessGrants
ScanKnownSecrets
```

Work payloads contain stable IDs/revisions/deadlines only, never secret material, OAuth code, PKCE verifier, clear state, proxy capability or lease material.

- [ ] Add logical Run events only when a Run enters authentication waiting, binds an available Connection or is paused by revocation. Lease/refresh/use/audit progress remains H8-local and does not increment `RunVersion`.
- [ ] `RunCheckpointV6` adds required Connection IDs, active authorization-flow IDs and delegated ConnectionAccessGrant IDs; it never stores credential generations, lease IDs/tokens or secret references needed to resume external dispatch.
- [ ] `0060` forces RLS, same-workspace/owner/Run/consumer consistency, immutable/append-only guards and active flow/refresh/lease/grant/expiry/audit indexes.
- [ ] Boundary scripts reject serializable/debuggable secret material, secret-like work/event/public fields, direct backend reads outside credential runtime, raw credentials in H3/H4/H5/H6/H7 types, sandbox env/mount credentials and A2A credential fields.
- [ ] Mandatory acceptance scenario proves:
  1. immutable connector revision with OAuth and API-key schemes;
  2. user-bound OAuth PKCE Connection with no clear code/token in PostgreSQL/logs/events;
  3. API-key submission through secure input with no InteractionEvent copy;
  4. tool call uses one short lease and model sees only logical Connection handle;
  5. sandbox call uses proxy and workload receives no secret;
  6. internal SubRun grant is narrower than parent and cannot use omitted operation;
  7. remote-agent dispatch uses a separate identity/origin-bound lease;
  8. lost external response consumes the lease and does not duplicate dispatch;
  9. refresh is singleflight;
  10. rotation activates one new generation and revokes old unconsumed leases;
  11. revocation prevents every later tool/remote lease;
  12. H7 AuthenticationRequired resumes only after matching Connection becomes Active;
  13. known secret in interaction/model input is denied/redacted and in Artifact is quarantined;
  14. usage audit contains complete references/results and no secret fragments;
  15. restart at every auth/store/rotation/lease/proxy fault point creates no duplicate generation, Connection, lease use or external call;
  16. replay performs no secret backend read, authorization, refresh, injection, proxy request or connector call.
- [ ] Required CI jobs: connector/connection domain, encrypted secret backend, OAuth/API-key flows, refresh/rotation/revocation, delegated grants, lease atomicity, provider/tool/fetch injection, sandbox proxy, remote authentication, leak/audit, boundaries, H8 acceptance.
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
0055 Task 5  Connections, sharing, scopes and validation
0056 Task 6  Authorization flows, credential bindings and rotations
0057 Task 8  Delegated connection grants and remote-agent profiles
0058 Task 9  Credential leases, uses and sandbox proxy sessions
0059 Task 14 Secret leak findings and credential audit
0060 Task 15 RLS, indexes and Run bindings
```

No later task edits a migration after its owner task commits it.

## H8 completion definition

H8 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
ConnectorDefinitionRevision
→ Connection + local permission ceiling
→ OAuth/API-key authorization
→ secret-backend CredentialSetGeneration
→ validation and Active Connection
→ protected external operation
→ exact CredentialLease
→ enforcement-point consumption
→ ephemeral injection/proxy decoration
→ external request
→ owning operation reconciliation
→ credential audit
→ refresh/rotation/revocation
```

Required invariants:

1. PostgreSQL never stores secret material; the secret backend never becomes the authority for Connection/policy state.
2. Connector and remote security declarations cannot grant local authority.
3. External scopes and Connection sharing cannot exceed the local permission ceiling and H2 policy.
4. OAuth state/code/verifier/token lifecycles are one-time, bounded and restart-safe without clear persistence.
5. API-key input bypasses ordinary interaction/logging surfaces.
6. Secret material wrappers cannot serialize, clone or debug-print values and zeroize on drop.
7. Every outbound credential use consumes an exact operation-bound lease immediately before injection.
8. A consumed lease is not reused after crash, timeout or ambiguous completion.
9. Refresh is singleflight and refresh tokens are never outbound connector credentials.
10. Rotation activates one immutable generation and revokes obsolete unconsumed leases.
11. Revocation blocks new leases while preserving in-flight uncertainty and audit.
12. SubRuns and remote invocations receive only explicit ConnectionAccessGrants.
13. Remote-agent credentials are bound to local identity/origin/transport/operation/resource/classification policy.
14. Models receive logical Connection handles only.
15. Sandboxes receive no long-lived secret; proxy capability is not a credential and cannot escape its exact session/destination.
16. Provider, tool, Artifact fetch and remote adapters share one lease/injection contract.
17. Known-secret detection never persists the matched secret.
18. Credential audit is complete, append-only and content-free.
19. H7 authentication continuations contain only opaque flow/Connection status.
20. No A2A SDK type or Agent Card credential enters H8 domain/application/persistence.
21. Restart does not duplicate credential generations, lease consumption, rotation or connector calls.
22. Replay performs no credential-bearing action.
23. H9 can register connector/adapter revisions without changing H8 Connection semantics.
24. H9A can consume remote profiles/leases without changing H8 types.
25. H10 can add audit integrity/metrics without rewriting H8 records.
26. H8 tests require no public identity provider, SaaS account, remote agent or permanent secret.

## Explicit non-goals

H8 does not implement production Gmail/Slack/CRM connectors, organization OIDC/SCIM, a public connector marketplace, HashiCorp Vault/cloud-secret-manager adapters, HSM-backed signing, OAuth device flow, browser UI, generic TCP/SOCKS credential tunneling, unrestricted cookie jars, permanent sandbox environment credentials, cross-organization credential delegation, Agent Card trust, A2A transport or public APIs that return secret material.

## Documentation-only boundary

Creating this document does not authorize implementation. During the current documentation phase, do not create `feat/h8-connections-credential-broker`, add crypto/OAuth dependencies, create migrations `0054`–`0060`, create a master key or secret store, start callback/proxy servers, submit or rotate real credentials, modify production adapters, change CI or execute H8 tests.
