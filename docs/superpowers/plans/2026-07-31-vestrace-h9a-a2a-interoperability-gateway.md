# Vestrace H9A A2A Interoperability Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add A2A dependencies, create migrations, start protocol servers, contact remote agents, import or publish Agent Cards, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement a restart-safe bidirectional A2A v1 gateway that calls independent remote agents and exposes selected Vestrace agent snapshots through JSON-RPC, HTTP+JSON and SSE while preserving Vestrace ownership of Runs, policy, credentials, artifacts, continuations and reconciliation.

**Architecture:** H9A extends the transport-neutral H5 `RemoteAgentInvocation`, H7 continuation/event, H8 credential and H9 remote-definition/publication boundaries. `vestrace-remote-agent-runtime` owns normalized transport requests, observations, event fingerprints, task bindings and reconciliation semantics; `vestrace-a2a-adapter` is an anti-corruption layer around exact-pinned `a2a-rs` crates. Outbound A2A Tasks remain remote-owned observations of a Vestrace invocation. Inbound A2A Tasks are protocol projections over authoritative Vestrace `AgentRun` state. No A2A response can grant authority, bypass Artifact quarantine or declare a Vestrace Run successful.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H9; Rust Edition 2024; Tokio; Serde/Schemars; SQLx and PostgreSQL 17; Axum/Tower; Reqwest with a Vestrace-owned client factory; SSE; SHA-256 and HMAC-SHA-256; H2 ActionGuard; H6 Artifact ingestion; H7 public events and HumanRequest; H8 credential leases; H9 exact Agent/Remote-Agent revisions; exact-pinned `a2a-lf` 0.3.0, `a2a-client-lf` 0.2.1 and `a2a-server-lf` 0.4.1 from source commit `515f6eacf2b4b9b17bd3910e93ac47027afaaf90`; deterministic loopback A2A fixtures; proptest; tracing with mandatory redaction.

## Global Constraints

- Complete all five v0.1 plans and H1–H9 before implementing H9A.
- ADR-0003 `A2A Interoperability Boundary` is normative.
- H5 `RemoteAgentInvocation`, delegation grants, handoff validation and `Unknown` semantics remain authoritative. H9A must not create a second remote-invocation aggregate.
- H7 owns typed `HumanRequest`, `HumanResponse`, public event cursors and channel-independent continuation.
- H8 owns outbound remote-agent Connections, exact remote profiles, dual-ticket credential leases and request-scoped secret injection.
- H9 owns immutable remote-agent declaration revisions, local activation/trust revisions, exact AgentRuntimeSnapshots and Extension activation.
- A2A Task is never an `AgentRun`, internal `SubRun`, Tool invocation, policy decision, budget account or Artifact.
- PostgreSQL is authoritative for A2A adapter bindings, route/publication revisions, discovery observations, dispatch attempts, normalized protocol events, local stream cursors, artifact assemblies, reconciliation records, inbound request/task bindings and protocol projections.
- Vestrace owns all domain/application/persistence/public contracts. `a2a-rs`, Reqwest and Axum types may appear only in `vestrace-a2a-adapter` and explicit A2A wire endpoints.
- Exact SDK baseline is locked to commit `515f6eacf2b4b9b17bd3910e93ac47027afaaf90`. Upgrades require a new adapter binding revision, conformance run and compatibility report; changing only Cargo version ranges is forbidden.
- Initial dependencies are only `a2a-lf`, `a2a-client-lf` and `a2a-server-lf`. H9A must not depend on `a2a-pb`, `a2a-grpc`, `a2a-slimrpc` or `a2acli` in production crates.
- `a2a-client-lf` and `a2a-server-lf` use `default-features = false` and `rustls-no-provider`. The composition root installs one deployment-approved Rustls crypto provider.
- Do not use `a2a_client::default_reqwest_client` or default transport factories. The SDK workspace enables `system-proxy`; H9A must inject a Vestrace-owned Reqwest client with ambient proxies disabled, redirects disabled, exact TLS roots, bounded timeouts, destination checks and redacted diagnostics.
- Initial product bindings are A2A v1 JSON-RPC over HTTP, HTTP+JSON/REST and SSE streaming/subscription.
- gRPC, protobuf, SLIMRPC, collaborative channels and push-notification callbacks are inactive and rejected with stable `unsupported_binding` or `unsupported_operation` errors.
- Push notification configuration methods are not registered or exposed in the first slice. A remote Agent Card advertising push does not activate it.
- Agent Card discovery and publication never create trust or authority. Signatures and security declarations are recorded as untrusted compatibility data.
- A remote declaration can only narrow route eligibility relative to the H9 local activation revision. It cannot add an origin, transport, Skill, data classification or credential scheme.
- Outbound standard delegation requires a task-based response with an external task ID. A message-only response is accepted only for an explicit `ImmediateMessageAllowed` read-only profile that requires no continuation, cancellation, artifact stream or external reconciliation.
- Reference packages and the mandatory acceptance path use `TaskRequired`.
- Every outbound dispatch, continuation, cancellation, subscription, poll, discovery requiring authentication and extended-card request is a separately authorized operation and uses a fresh H8 credential lease.
- One credential lease is never reused for reconnect, poll, continuation, cancellation or another transport request.
- Credential material is injected in adapter infrastructure after lease consumption and never enters A2A messages, metadata, Agent Cards, task records, protocol events or errors.
- Outbound `tenant` is `None` in the standard profile. Inbound `tenant`, task ID, context ID, message ID, headers and metadata are routing/consistency inputs only and never establish workspace, principal or authority.
- Standard outbound messages use a deterministic opaque message ID derived from invocation ID, continuation sequence and canonical content hash. The SDK random `Message::new` constructor is not used for protected dispatches.
- Optional correlation metadata uses an opaque HMAC-derived token under a Vestrace namespaced key. It contains no internal UUID and is not a bearer capability.
- Remote status text, messages, metadata, extensions, Agent Cards, errors and artifacts are untrusted external content.
- A2A `Completed` maps to H5 `CompletedPendingValidation`, never directly to `Succeeded`.
- A2A `Canceled` is an observation that remote cancellation was reported; it is not evidence of rollback or compensation.
- A2A `Unspecified`, invalid state regression, task/context mismatch or malformed field-presence union is a protocol violation and cannot silently advance an invocation.
- A remote dispatch with a response that may have been accepted becomes `Unknown` unless a durable external task binding and exact observation were persisted.
- `Unknown` never triggers automatic redispatch. Reconciliation uses known task/context identifiers, local correlation evidence, `GetTask`, `ListTasks` when policy permits, and transport-specific evidence; otherwise it remains `Unknown`.
- SSE has no assumed authoritative remote event cursor. H9A persists a local monotonic event sequence and canonical event fingerprints. Reconnect performs a fresh subscription or task poll and deduplicates replayed observations.
- A duplicate canonical event is idempotent. Same external identity with conflicting canonical content is a protocol violation.
- A terminal remote task cannot regress to a non-terminal state. Duplicate terminal snapshots are allowed only when canonically equivalent.
- Inbound and outbound A2A raw parts are bounded. A single decoded raw part is at most 8 MiB; aggregate raw content per message is at most 16 MiB; an assembled remote artifact is at most 64 MiB unless a stricter H6/workspace policy applies.
- Inline text is at most 256 KiB per part and 1 MiB per message. Metadata is at most 64 KiB canonical JSON. A message has 1–64 parts. History length is `0..=50`; task list page size is `1..=100`.
- Outbound Vestrace Artifact transfer in the first slice supports authorized text, JSON and raw bytes up to the inline limits. Outbound URL parts are disabled. Larger outbound Artifacts fail before dispatch with `remote_content_too_large`.
- Inbound URL parts are handled only through H6 secure external fetch with scheme, redirect, DNS, SSRF, size and credential policy. The A2A adapter never fetches a URL directly.
- Raw and incremental Artifact parts enter an H6 quarantine/ingestion session. No remote Artifact becomes Available directly.
- `append=true` requires an existing open artifact assembly. `last_chunk=true` closes the assembly. A stream ending with an incomplete required assembly prevents successful handoff.
- A full Task snapshot may reconcile incremental Artifact assembly only after content hashes and external Artifact IDs are consistent.
- H7 HumanRequest is the sole user-facing continuation for `InputRequired` and `AuthRequired`. Free text does not implicitly continue a remote task.
- H8 authentication completion produces an opaque continuation cause; the remote service never chooses the local Connection or credential scope.
- Inbound A2A authentication is resolved by Vestrace HTTP/authentication middleware through a dedicated port. A2A `tenant` or Agent Card security metadata is never accepted as identity.
- Inbound task creation and continuation are idempotent by authenticated external principal, publication revision, message ID and canonical request hash.
- An inbound task binding points to one exact AgentRun and publication revision. Reusing the same message ID with changed content is a conflict.
- Inbound protocol TaskStore data is a projection/cache only. Run status, events, checkpoints, approvals and artifacts remain authoritative in H1–H8 stores.
- Streaming and transport frames do not increment `RunVersion`. Only canonical logical Run transitions do.
- Replay never performs discovery, dispatch, subscribe, poll, continue, cancel, fetch, credential injection, inbound Run creation or protocol emission.
- Existing migrations `0014`–`0067` are never edited. H9A migrations are `0068`–`0072`, each owned once.
- CI uses deterministic loopback JSON-RPC/REST/SSE fixtures and generated local credentials. No public A2A service, public network, permanent credential or push callback is required.
- Future implementation branch: `feat/h9a-a2a-interoperability-gateway`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  remote_agent/
    mod.rs
    transport.rs
    protocol_event.rs
    artifact_assembly.rs
    inbound_binding.rs
    publication.rs
    reconciliation.rs
    error.rs
  run/
    event.rs
    work.rs
    checkpoint.rs
    mod.rs

crates/vestrace-application/src/
  remote_agent_runtime/
    mod.rs
    ports.rs
    commands.rs
    routing.rs
    dispatch.rs
    stream.rs
    continuation.rs
    cancellation.rs
    reconciliation.rs
    artifact.rs
    inbound.rs
    projection.rs
    publication.rs
    worker.rs
  composition/
    a2a.rs

crates/vestrace-remote-agent-runtime/
  Cargo.toml
  src/
    lib.rs
    request.rs
    observation.rs
    event_fingerprint.rs
    state_mapping.rs
    content.rs
    cursor.rs
    errors.rs

crates/vestrace-a2a-adapter/
  Cargo.toml
  src/
    lib.rs
    dependency_baseline.rs
    client.rs
    client_factory.rs
    http_client.rs
    jsonrpc.rs
    rest.rs
    sse.rs
    interceptor.rs
    message_mapping.rs
    task_mapping.rs
    event_mapping.rs
    artifact_mapping.rs
    card_discovery.rs
    card_publication.rs
    server.rs
    request_handler.rs
    executor.rs
    task_projection_store.rs
    auth.rs
    error.rs

crates/vestrace-a2a-test-support/
  Cargo.toml
  src/
    lib.rs
    agent_card.rs
    remote_agent.rs
    client.rs
    server.rs
    jsonrpc_fixture.rs
    rest_fixture.rs
    sse_fixture.rs
    auth_fixture.rs
    artifact_fixture.rs
    faults.rs
    conformance.rs

crates/vestrace-channel-http/src/
  a2a_mount.rs
  a2a_auth.rs

crates/vestrace-channel-cli/src/
  a2a_commands.rs

crates/vestrace-infrastructure/src/postgres/
  a2a/
    mod.rs
    binding_repository.rs
    card_repository.rs
    outbound_repository.rs
    event_repository.rs
    artifact_assembly_repository.rs
    reconciliation_repository.rs
    inbound_binding_repository.rs
    projection_repository.rs

crates/vestrace-cli/src/composition/
  a2a.rs

migrations/
  0068_a2a_adapter_bindings_and_routes.sql
  0069_a2a_card_discovery_and_publication.sql
  0070_a2a_outbound_attempts_events_and_reconciliation.sql
  0071_a2a_inbound_task_bindings_and_projections.sql
  0072_a2a_rls_indexes_and_run_bindings.sql

tests/
  a2a_sdk_pin.rs
  a2a_type_boundary.rs
  a2a_binding_persistence.rs
  a2a_card_discovery.rs
  a2a_card_publication.rs
  a2a_jsonrpc_conformance.rs
  a2a_rest_conformance.rs
  a2a_transport_equivalence.rs
  a2a_sse_reconnect.rs
  a2a_event_deduplication.rs
  a2a_outbound_dispatch.rs
  a2a_outbound_unknown.rs
  a2a_outbound_reconciliation.rs
  a2a_input_continuation.rs
  a2a_auth_continuation.rs
  a2a_cancel_semantics.rs
  a2a_artifact_assembly.rs
  a2a_artifact_quarantine.rs
  a2a_inbound_authentication.rs
  a2a_inbound_task_binding.rs
  a2a_inbound_projection.rs
  a2a_inbound_sse.rs
  a2a_restart_recovery.rs
  a2a_rls.rs
  h9a_acceptance.rs

scripts/
  verify-a2a-sdk-pin.sh
  verify-a2a-type-boundary.sh
  verify-a2a-dependency-boundary.sh
  verify-a2a-network-boundary.sh
  verify-a2a-task-ownership.sh
  verify-a2a-secret-boundary.sh
```

---

## Normative contracts

### Exact SDK dependency baseline

The adapter Cargo manifest uses exact git revisions:

```toml
[dependencies]
a2a = { package = "a2a-lf", git = "https://github.com/a2aproject/a2a-rs", rev = "515f6eacf2b4b9b17bd3910e93ac47027afaaf90" }
a2a-client = { package = "a2a-client-lf", git = "https://github.com/a2aproject/a2a-rs", rev = "515f6eacf2b4b9b17bd3910e93ac47027afaaf90", default-features = false, features = ["rustls-no-provider"] }
a2a-server = { package = "a2a-server-lf", git = "https://github.com/a2aproject/a2a-rs", rev = "515f6eacf2b4b9b17bd3910e93ac47027afaaf90", default-features = false, features = ["rustls-no-provider"] }
```

The verified source workspace versions are:

```text
a2a-lf        0.3.0
a2a-client-lf 0.2.1
a2a-server-lf 0.4.1
Rust          1.85 minimum
Edition       2024
```

`Cargo.lock` must resolve all three crates to the exact commit. `cargo tree -i a2a-lf` must show only `vestrace-a2a-adapter` and its test-support crate as Vestrace dependants.

### Binding and route revisions

```rust
pub enum RemoteTransportProtocol {
    A2A,
}

pub enum A2AProtocolBindingKind {
    JsonRpcHttp,
    HttpJson,
}

pub enum A2AStreamingMode {
    Disabled,
    Sse,
}

pub enum RemoteCompletionMode {
    TaskRequired,
    ImmediateMessageAllowed,
}

pub struct A2AAdapterBindingRevision {
    pub id: A2AAdapterBindingRevisionId,
    pub binding_id: A2AAdapterBindingId,
    pub revision: u32,
    pub protocol_version: String,
    pub source_commit: String,
    pub core_crate_version: String,
    pub client_crate_version: String,
    pub server_crate_version: String,
    pub supported_bindings: std::collections::BTreeSet<A2AProtocolBindingKind>,
    pub streaming_mode: A2AStreamingMode,
    pub tls_profile_revision_id: TlsClientProfileRevisionId,
    pub maximum_request_bytes: u64,
    pub maximum_response_bytes: u64,
    pub content_hash: [u8; 32],
}

pub struct NormalizedA2AEndpoint {
    pub origin: NormalizedOrigin,
    pub path: String,
}

pub struct A2ARemoteRouteRevision {
    pub id: A2ARemoteRouteRevisionId,
    pub route_id: A2ARemoteRouteId,
    pub revision: u32,
    pub remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
    pub remote_activation_revision_id: RemoteAgentActivationRevisionId,
    pub adapter_binding_revision_id: A2AAdapterBindingRevisionId,
    pub protocol_binding: A2AProtocolBindingKind,
    pub endpoint: NormalizedA2AEndpoint,
    pub remote_connection_profile_revision_id: Option<RemoteAgentConnectionProfileRevisionId>,
    pub completion_mode: RemoteCompletionMode,
    pub accepted_output_modes: Vec<String>,
    pub history_length: u16,
    pub request_timeout_ms: u64,
    pub idle_stream_timeout_ms: u64,
    pub maximum_total_duration_ms: u64,
    pub content_hash: [u8; 32],
}
```

`NormalizedA2AEndpoint` rejects userinfo, query, fragment, non-HTTPS origins except explicit loopback policy, path traversal and an origin not allowed by the exact H9/H8 revisions. Route revisions are immutable and cannot choose a binding absent from both the remote declaration and adapter binding.

### External identifiers and correlation

```rust
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct RemoteTaskKey(String);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct RemoteContextKey(String);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct RemoteMessageKey(String);

pub struct RemoteCorrelationToken {
    pub hmac: [u8; 32],
}
```

External identifiers are UTF-8, 1–512 bytes, contain no control characters and use redacted bounded Debug. The clear correlation token is derived when constructing the request and is not stored as a bearer secret. It provides correlation only and grants no access.

### Dispatch attempts

```rust
pub enum RemoteDispatchAttemptStatus {
    Prepared,
    Dispatching,
    TaskBound,
    Streaming,
    Observed,
    FailedBeforeAcceptance,
    Unknown,
    Cancelled,
}

pub struct RemoteDispatchAttempt {
    pub id: RemoteDispatchAttemptId,
    pub workspace_id: WorkspaceId,
    pub invocation_id: RemoteAgentInvocationId,
    pub attempt_number: u16,
    pub route_revision_id: A2ARemoteRouteRevisionId,
    pub request_message_key: RemoteMessageKey,
    pub canonical_request_hash: [u8; 32],
    pub operation_fingerprint: OperationFingerprint,
    pub authorization_ticket_id: AuthorizationTicketId,
    pub budget_reservation_id: Option<BudgetReservationId>,
    pub credential_use_binding_hash: Option<[u8; 32]>,
    pub external_task_key: Option<RemoteTaskKey>,
    pub external_context_key: Option<RemoteContextKey>,
    pub status: RemoteDispatchAttemptStatus,
    pub completion_may_have_occurred: bool,
    pub started_at: Timestamp,
    pub observed_at: Option<Timestamp>,
    pub logical_revision: u64,
}
```

One H5 invocation normally owns one initial dispatch attempt. A later attempt is allowed only after H5 reconciliation returns `FailedSafeToRedispatch` and a new ActionGuard/budget/credential decision is issued. Attempt numbers are contiguous and unique.

### Protocol states and mapping

```rust
pub enum RemoteProtocolTaskState {
    Submitted,
    Working,
    CompletedCandidate,
    Failed,
    Cancelled,
    InputRequired,
    Rejected,
    AuthRequired,
    Unknown,
}
```

Normative mapping:

```text
A2A Submitted      → H5 Dispatching or Working
A2A Working        → H5 Working
A2A InputRequired  → H5 WaitingForRemoteInput + H7 HumanRequest
A2A AuthRequired   → H5 WaitingForAuthentication + H7 HumanRequest
A2A Completed      → H5 CompletedPendingValidation
A2A Failed         → H5 Failed
A2A Rejected       → H5 Rejected
A2A Canceled       → H5 Cancelled observation, no rollback claim
A2A Unspecified    → protocol violation / Unknown
lost response      → H5 Unknown
```

Only H5 handoff validation may move `CompletedPendingValidation` to `Succeeded`.

### Normalized protocol events

```rust
pub enum RemoteProtocolEventKind {
    StreamOpened,
    TaskSnapshot,
    StatusUpdate,
    Message,
    ArtifactUpdate,
    StreamClosed,
    TransportError,
    ReconciliationObservation,
}

pub struct RemoteProtocolEventFingerprint {
    pub schema: u16,
    pub hash: [u8; 32],
}

pub struct RemoteProtocolEvent {
    pub id: RemoteProtocolEventId,
    pub workspace_id: WorkspaceId,
    pub invocation_id: Option<RemoteAgentInvocationId>,
    pub inbound_task_binding_id: Option<InboundA2ATaskBindingId>,
    pub attempt_id: Option<RemoteDispatchAttemptId>,
    pub local_sequence: u64,
    pub kind: RemoteProtocolEventKind,
    pub external_task_key: Option<RemoteTaskKey>,
    pub external_context_key: Option<RemoteContextKey>,
    pub external_message_key: Option<RemoteMessageKey>,
    pub external_artifact_key: Option<String>,
    pub event_fingerprint: RemoteProtocolEventFingerprint,
    pub normalized_state: Option<RemoteProtocolTaskState>,
    pub safe_summary: Option<String>,
    pub normalized_metadata: Option<serde_json::Value>,
    pub artifact_assembly_id: Option<RemoteArtifactAssemblyId>,
    pub source_timestamp: Option<Timestamp>,
    pub observed_at: Timestamp,
}
```

Fingerprint schema V1 hashes event kind, task/context/message/artifact IDs, normalized state, canonical message/metadata/content hashes, append/last-chunk flags and source timestamp. Local sequence is allocated transactionally per invocation or inbound task. Raw bodies, headers and secret material are not event fields.

### Content mapping

```rust
pub enum RemoteContentPart {
    Text {
        text: String,
        media_type: Option<String>,
        filename: Option<String>,
    },
    Json {
        value: serde_json::Value,
        media_type: Option<String>,
        filename: Option<String>,
    },
    RawCandidate {
        ingestion_session_id: ArtifactIngestionSessionId,
        decoded_size: u64,
        media_type: Option<String>,
        filename: Option<String>,
    },
    UrlCandidate {
        fetch_request_id: ExternalArtifactFetchRequestId,
        normalized_url_hash: [u8; 32],
        media_type: Option<String>,
        filename: Option<String>,
    },
}

pub struct RemoteMessageObservation {
    pub message_key: RemoteMessageKey,
    pub role: RemoteMessageRole,
    pub task_key: Option<RemoteTaskKey>,
    pub context_key: Option<RemoteContextKey>,
    pub parts: Vec<RemoteContentPart>,
    pub canonical_content_hash: [u8; 32],
}
```

Outbound mapping accepts only H6-authorized text/JSON/raw material. Inbound raw and URL parts create H6 requests before a normalized observation can reference them. Metadata and extension URIs are not treated as instructions.

### Artifact assembly

```rust
pub enum RemoteArtifactAssemblyStatus {
    Open,
    Complete,
    Quarantined,
    Available,
    Rejected,
    Incomplete,
    Conflict,
}

pub struct RemoteArtifactAssembly {
    pub id: RemoteArtifactAssemblyId,
    pub workspace_id: WorkspaceId,
    pub invocation_id: RemoteAgentInvocationId,
    pub external_artifact_key: String,
    pub generation: u32,
    pub status: RemoteArtifactAssemblyStatus,
    pub ingestion_session_id: ArtifactIngestionSessionId,
    pub observed_parts: u32,
    pub decoded_bytes: u64,
    pub rolling_content_hash: [u8; 32],
    pub final_artifact_revision_id: Option<ArtifactRevisionId>,
    pub last_event_sequence: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

An event with `append=false` or absent starts a new generation unless it is an exact duplicate. `append=true` requires one open matching generation. `last_chunk=true` closes the generation and asks H6 to finalize quarantine. Conflicting replay marks `Conflict` and prevents handoff acceptance.

### Reconciliation

```rust
pub enum RemoteReconciliationDisposition {
    SucceededCandidate,
    FailedTerminal,
    CancelledObserved,
    StillWorking,
    WaitingForInput,
    WaitingForAuthentication,
    FailedSafeToRedispatch,
    StillUnknown,
    ProtocolConflict,
}

pub struct RemoteReconciliationRecord {
    pub id: RemoteReconciliationId,
    pub invocation_id: RemoteAgentInvocationId,
    pub attempt_id: RemoteDispatchAttemptId,
    pub route_revision_id: A2ARemoteRouteRevisionId,
    pub external_task_key: Option<RemoteTaskKey>,
    pub correlation_hmac: [u8; 32],
    pub evidence_event_ids: Vec<RemoteProtocolEventId>,
    pub disposition: RemoteReconciliationDisposition,
    pub safe_code: String,
    pub created_at: Timestamp,
}
```

`FailedSafeToRedispatch` requires positive evidence that no task/effect was accepted, not merely a timeout or `TaskNotFound` after a previously observed task.

### Agent Card discovery and publication

```rust
pub enum A2ACardObservationKind {
    PublicWellKnown,
    ExtendedAuthenticated,
}

pub struct A2ACardDiscoveryObservation {
    pub id: A2ACardDiscoveryObservationId,
    pub workspace_id: WorkspaceId,
    pub source_endpoint: NormalizedA2AEndpoint,
    pub kind: A2ACardObservationKind,
    pub card_artifact_revision_id: ArtifactRevisionId,
    pub card_content_hash: [u8; 32],
    pub normalized_candidate_hash: [u8; 32],
    pub resulting_remote_agent_revision_id: Option<RemoteAgentDefinitionRevisionId>,
    pub credential_use_audit_id: Option<CredentialUsageAuditRecordId>,
    pub observed_at: Timestamp,
}

pub struct A2APublishedAgentRevision {
    pub id: A2APublishedAgentRevisionId,
    pub publication_id: A2APublishedAgentId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub agent_runtime_snapshot_id: AgentRuntimeSnapshotId,
    pub display_name: String,
    pub description: String,
    pub public_version: String,
    pub supported_bindings: Vec<A2APublishedInterface>,
    pub published_skills: Vec<A2APublishedSkill>,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub security_declaration_revision_id: A2ASecurityDeclarationRevisionId,
    pub maximum_input_classification: DataClassification,
    pub card_artifact_revision_id: ArtifactRevisionId,
    pub card_content_hash: [u8; 32],
    pub status: A2APublicationStatus,
    pub content_hash: [u8; 32],
}
```

Publication is bound to one exact AgentRuntimeSnapshot. Updating a package/profile requires a new publication revision. Security schemes are generated from deployment authentication configuration; they contain no credential. The first slice publishes no push-notification capability and no trust-bearing signature.

### Inbound bindings and projections

```rust
pub enum InboundA2ATaskStatus {
    Bound,
    Running,
    WaitingForInput,
    WaitingForAuthentication,
    Completed,
    Failed,
    Cancelled,
    Rejected,
}

pub struct InboundA2ARequestIdentity {
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub external_identity_hash: [u8; 32],
    pub authentication_method_revision_id: AuthenticationMethodRevisionId,
}

pub struct InboundA2ATaskBinding {
    pub id: InboundA2ATaskBindingId,
    pub workspace_id: WorkspaceId,
    pub publication_revision_id: A2APublishedAgentRevisionId,
    pub protocol_task_key: RemoteTaskKey,
    pub protocol_context_key: RemoteContextKey,
    pub run_id: AgentRunId,
    pub requester_principal_id: PrincipalId,
    pub external_identity_hash: [u8; 32],
    pub initial_message_key: RemoteMessageKey,
    pub initial_request_hash: [u8; 32],
    pub status: InboundA2ATaskStatus,
    pub last_public_event_cursor: Option<PublicEventCursor>,
    pub state_revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct InboundA2ARequestBinding {
    pub id: InboundA2ARequestBindingId,
    pub task_binding_id: InboundA2ATaskBindingId,
    pub message_key: RemoteMessageKey,
    pub canonical_request_hash: [u8; 32],
    pub interaction_event_id: InteractionEventId,
    pub routing_decision_id: InteractionRoutingDecisionId,
    pub created_at: Timestamp,
}
```

The same authenticated identity/publication/message key/hash returns the original result. Changed content with the same message key returns `idempotency_conflict`. Task/context mismatch, foreign workspace or unrelated identity is denied.

### Inbound projection ownership

```rust
pub struct A2ATaskProjection {
    pub binding_id: InboundA2ATaskBindingId,
    pub protocol_task_key: RemoteTaskKey,
    pub protocol_context_key: RemoteContextKey,
    pub normalized_state: RemoteProtocolTaskState,
    pub status_message_event_id: Option<InteractionEventId>,
    pub artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub history_event_ids: Vec<InteractionEventId>,
    pub projection_hash: [u8; 32],
    pub projected_at: Timestamp,
}
```

Projection is derived from the binding, H1 Run, H7 interactions/public events and H6 Artifacts. SDK `TaskStore::create/update` may persist or refresh this projection only after the authoritative application transaction; it cannot initiate a Run transition.

### Normalized error model

```rust
pub enum RemoteTransportErrorKind {
    InvalidDeclaration,
    UnsupportedBinding,
    UnsupportedOperation,
    InvalidRequest,
    AuthenticationRequired,
    AuthenticationFailed,
    AuthorizationDenied,
    RateLimited,
    Unavailable,
    TimeoutBeforeDispatch,
    StreamInterrupted,
    TaskNotFound,
    TaskNotCancellable,
    ContentRejected,
    ProtocolViolation,
    Cancelled,
    UnknownCompletion,
}

pub struct RemoteTransportError {
    pub kind: RemoteTransportErrorKind,
    pub stable_code: String,
    pub safe_message: String,
    pub retry_after_ms: Option<u64>,
    pub completion_may_have_occurred: bool,
}
```

Errors contain no raw URL query, header, body, token fragment, SDK Debug output or remote unbounded text. A transport failure after request body dispatch normally sets `completion_may_have_occurred=true`.

---

### Task 1: Pin the A2A SDK and enforce the anti-corruption boundary

**Files:** modify workspace `Cargo.toml`, `Cargo.lock`; create `vestrace-a2a-adapter`, `vestrace-a2a-test-support`, dependency baseline module and SDK/dependency/type-boundary tests/scripts.

- [ ] Add the three exact git dependencies and no other `a2a-rs` production crate.
- [ ] Use `default-features=false` and `rustls-no-provider` for client/server.
- [ ] Add `dependency_baseline.rs` constants for source commit, crate versions and protocol version; tests compare constants to SDK exports/package metadata.
- [ ] `verify-a2a-sdk-pin.sh` asserts Cargo.lock git source ends in `#515f6eacf2b4b9b17bd3910e93ac47027afaaf90`.
- [ ] `verify-a2a-dependency-boundary.sh` fails if any domain/application/infrastructure/public crate depends on an `a2a-*` crate or if grpc/pb/slimrpc appears.
- [ ] Add a compile fixture proving all non-adapter crates build with feature `a2a` disabled.
- [ ] Run and commit:

```bash
bash scripts/verify-a2a-sdk-pin.sh
bash scripts/verify-a2a-dependency-boundary.sh
cargo test --test a2a_sdk_pin --test a2a_type_boundary
cargo tree -i a2a-lf
cargo clippy -p vestrace-a2a-adapter --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/vestrace-a2a-adapter \
  crates/vestrace-a2a-test-support scripts tests
git commit -m "build(a2a): pin SDK behind adapter boundary"
```

### Task 2: Add transport-neutral remote protocol domain contracts

**Files:** modify `id.rs`; create remote-agent transport/event/artifact/inbound/publication/reconciliation/error files; extend H5 remote status with `CompletedPendingValidation` if not already present.

- [ ] Add every H9A ID and value type defined above.
- [ ] Unit-test external identifier bounds/control-character rejection, route origin/path validation and immutable content hashes.
- [ ] Property-test canonical event fingerprints against map insertion order and duplicate metadata ordering.
- [ ] Test the complete A2A state mapping, terminal regression rejection and `Completed → CompletedPendingValidation`.
- [ ] Test artifact assembly transition table, exact duplicate replay and conflicting replay.
- [ ] Test `ImmediateMessageAllowed` rejects write/external-commitment profiles and any profile requiring continuation or artifacts.
- [ ] Run and commit.

### Task 3: Define H9A application ports and deterministic fixtures

**Files:** create application remote-agent-runtime modules, `vestrace-remote-agent-runtime` and test-support fixtures.

**Interfaces:**

```rust
#[async_trait::async_trait]
pub trait RemoteTransportPort: Send + Sync {
    async fn dispatch(
        &self,
        request: RemoteTransportDispatchRequest,
    ) -> Result<RemoteTransportDispatchObservation, RemoteTransportError>;

    async fn subscribe(
        &self,
        request: RemoteTransportSubscribeRequest,
    ) -> Result<RemoteProtocolEventStream, RemoteTransportError>;

    async fn get_task(
        &self,
        request: RemoteTransportGetTaskRequest,
    ) -> Result<RemoteTaskObservation, RemoteTransportError>;

    async fn list_tasks(
        &self,
        request: RemoteTransportListTasksRequest,
    ) -> Result<RemoteTaskListObservation, RemoteTransportError>;

    async fn continue_task(
        &self,
        request: RemoteTransportContinuationRequest,
    ) -> Result<RemoteTransportDispatchObservation, RemoteTransportError>;

    async fn cancel_task(
        &self,
        request: RemoteTransportCancellationRequest,
    ) -> Result<RemoteTaskObservation, RemoteTransportError>;
}
```

- [ ] Add `InboundExternalAgentPort`, `A2ACardDiscoveryPort`, `A2ACardPublicationPort`, `RemoteArtifactPartPort`, `RemoteProtocolEventRepositoryPort` and `InboundTaskProjectionPort`.
- [ ] Object-safety compile-test every port.
- [ ] Deterministic fixtures support JSON-RPC/REST equivalence, SSE replay, duplicate/conflicting events, `InputRequired`, `AuthRequired`, chunked artifacts, response loss after acceptance, terminal regression and task-not-found reconciliation.
- [ ] Fault points exist after request write, task ID receipt, event persistence, artifact chunk write, HumanRequest creation, continuation dispatch, inbound Run creation and projection persistence.
- [ ] Run and commit.

### Task 4: Persist adapter bindings and outbound route revisions

**Files:** create migration `0068`, binding repository/services and persistence tests.

- [ ] `0068` creates adapter identities/revisions, supported binding rows, TLS profile links, remote route identities/revisions, accepted output modes and route lifecycle/current pointers.
- [ ] Enforce exact source commit/version fields and immutable revisions.
- [ ] Route activation verifies exact H9 declaration/activation, H8 remote profile, origin, binding and protocol version intersection.
- [ ] Reject route changes that widen H9 local activation or select unsupported push/grpc/slimrpc.
- [ ] New Agent Card or local activation never silently edits an existing route; create a candidate route revision and permission diff.
- [ ] Run and commit.

### Task 5: Implement secure Agent Card discovery and exact publication

**Files:** create migration `0069`, card discovery/publication services, adapter mapping and tests.

- [ ] `0069` creates discovery observations, card Artifact links, publication identities/revisions, interfaces, Skills, modes, security declaration revisions and publication lifecycle/current pointers.
- [ ] Public discovery fetch uses H6 secure URL handling with no ambient proxy/redirect. Authenticated extended-card retrieval uses one exact H8 lease.
- [ ] Persist received canonical card bytes as an H6 quarantined/inspected Artifact before H9 import.
- [ ] Normalize interfaces, skills, security declarations and capabilities into an H9 candidate without treating claims/signatures as trust.
- [ ] Publication builds deterministic card bytes from one exact AgentRuntimeSnapshot and explicit publication revision.
- [ ] Public and extended card routes return the same revision/hash where policy permits; extended card may add bounded private compatibility fields but no secrets.
- [ ] Publish `streaming=true` only when inbound SSE is active; publish no push-notification capability.
- [ ] Card refresh with changed content creates a new H9 candidate; unchanged hash is idempotent.
- [ ] Run and commit.

### Task 6: Implement hardened JSON-RPC and HTTP+JSON clients

**Files:** create adapter HTTP client, factory, JSON-RPC, REST, interceptor and error modules; add conformance/network tests.

- [ ] Build Reqwest through `VestraceA2AHttpClientFactory`: `no_proxy`, redirects disabled, exact TLS roots, connect/request/idle timeouts, response-size limit and redacted tracing.
- [ ] Register custom JSON-RPC and REST transport factories with `A2AClientFactory::builder().no_defaults()`; never use SDK default factories.
- [ ] A one-shot interceptor consumes one H8 credential handle and refuses a second `before` call.
- [ ] Construct deterministic outbound A2A Message IDs without `Message::new`; set `tenant=None`; include only allowlisted metadata.
- [ ] Map normalized request/response/event/error values without exposing SDK types.
- [ ] Test JSON-RPC and REST canonical equivalence for send/get/list/cancel/subscribe/extended-card operations.
- [ ] Set `HTTP_PROXY` and `HTTPS_PROXY` to a trap fixture and prove zero requests reach it.
- [ ] Prove redirects to same or foreign origin are rejected rather than followed.
- [ ] Run and commit.

### Task 7: Persist outbound attempts, protocol events, artifact assemblies and reconciliation

**Files:** create migration `0070`, outbound/event/assembly/reconciliation repositories and tests.

- [ ] `0070` creates dispatch attempts, external task/context bindings, normalized event sequences/fingerprints, stream sessions, artifact assemblies/chunk links and reconciliation records.
- [ ] Unique constraints enforce `(invocation, attempt_number)`, one active initial attempt, event fingerprint idempotency and external task/context consistency.
- [ ] Allocate local event sequence transactionally and return existing row for an identical fingerprint.
- [ ] Same identity/fingerprint slot with changed canonical content records a protocol-conflict event and blocks the invocation.
- [ ] Persist `Dispatching` before network I/O. Persist task/context keys and first observation atomically after response.
- [ ] Crash after task binding/event persistence resumes the same attempt without redispatch.
- [ ] Run and commit.

### Task 8: Implement outbound dispatch, streaming and state projection

**Files:** create dispatch/stream/routing services and outbound/SSE/dedup tests.

- [ ] Resolve the exact H9 route and H5 delegation scope, build H6-authorized content and consume H2/H8 guards immediately before transport dispatch.
- [ ] Standard `TaskRequired` rejects a message-only response with `task_required`; `ImmediateMessageAllowed` persists a completed candidate only under its restrictive profile.
- [ ] Map the initial Task and every SSE response to normalized events before updating H5 invocation status.
- [ ] Subscription uses a fresh credential lease and persists `StreamOpened` before accepting events.
- [ ] On disconnect, persist `StreamClosed/TransportError`, then reconnect with a new subscription lease or fall back to authorized `GetTask`.
- [ ] Do not assume remote SSE event IDs. Deduplicate replay through Vestrace event fingerprints.
- [ ] Enforce state monotonicity, task/context consistency and idle/total deadlines.
- [ ] Publish safe H7 remote-progress events only after canonical event persistence; raw remote text is not a public status label.
- [ ] Run and commit.

### Task 9: Implement remote input/auth continuation, cancellation and reconciliation

**Files:** create continuation/cancellation/reconciliation services and related tests.

- [ ] `InputRequired` creates one typed H7 `RemoteInputRequired` HumanRequest bound to invocation/task/context/status event and expected response schema.
- [ ] A validated HumanResponse creates a deterministic continuation message ID and a new `remote_agent.continue` ActionGuard plus fresh H8 lease.
- [ ] `AuthRequired` creates H7 AuthenticationRequired using only opaque H8 references; remote text cannot choose a Connection/scope.
- [ ] After H8 completion, resume rechecks exact remote revision/route/profile/current policy and sends a new continuation request with a fresh lease.
- [ ] Lost continuation response marks the invocation `Unknown`; it never resends automatically.
- [ ] Cancellation uses its own ActionGuard/lease, stores the request before dispatch and treats returned `Canceled` as an observation only.
- [ ] Reconciliation order is: known task `GetTask` → optional `SubscribeToTask` → policy-permitted bounded `ListTasks` correlation → disposition.
- [ ] `TaskNotFound` after a previously bound task is `StillUnknown` or terminal policy decision, never automatically safe to redispatch.
- [ ] Only positive no-acceptance evidence produces `FailedSafeToRedispatch`.
- [ ] Run and commit.

### Task 10: Map remote messages and artifacts through H6 and H5 handoff validation

**Files:** create content/artifact services, adapter mapping and Artifact/handoff tests.

- [ ] Outbound content builder accepts only exact authorized H6 revisions/excerpts and enforces text/JSON/raw limits before network dispatch.
- [ ] Outbound URL parts are rejected in H9A v0.2.
- [ ] Inbound Text/Data create bounded H6 ingestion candidates with remote provenance; Raw streams into the exact ingestion session; URL creates an H6 external fetch request.
- [ ] Artifact update ordering follows the assembly contract. Duplicate chunks are idempotent; conflicting append sequence becomes `Conflict`.
- [ ] Full Task snapshots reconcile only matching external artifact IDs and content hashes.
- [ ] H6 quarantine/inspection/secret scan completes before assembly becomes Available.
- [ ] A2A Completed waits for all required assemblies, output schema, evidence and H5 HandoffArtifact checks.
- [ ] Rejected/quarantined/incomplete artifact creates a failed or review-required handoff, never success.
- [ ] Provenance links include remote agent revision, route, invocation, task/context IDs, protocol event IDs and source hashes.
- [ ] Run and commit.

### Task 11: Implement authenticated inbound A2A request handling

**Files:** create adapter server/auth/request-handler/executor modules, HTTP mount and inbound auth/task tests.

- [ ] Mount explicit well-known card, JSON-RPC, REST and SSE routes under a configured A2A base path.
- [ ] Resolve authentication through `InboundA2AAuthenticationPort`; strip/redact auth headers before protocol metadata persistence.
- [ ] Ignore `tenant` for identity and workspace selection. Optionally store only an HMAC as diagnostic routing metadata.
- [ ] Validate protocol version, publication route, request body/part/metadata limits, role, task/context consistency and message ID.
- [ ] New message without task binding creates one H7 InteractionEvent, routing decision, H1 AgentRun and inbound task binding atomically.
- [ ] Message with task ID continues only the exact bound Run after identity, participant, publication and H2 checks.
- [ ] Default response is an A2A Task projection. `returnImmediately` affects waiting behavior but not state authority.
- [ ] Implement a custom Vestrace-backed `RequestHandler`; SDK handler/executor/task-store helpers may be used only behind the projection boundary and cannot mutate Run state independently.
- [ ] Run and commit.

### Task 12: Persist inbound task bindings, request idempotency and projections

**Files:** create migration `0071`, inbound binding/projection repositories and tests.

- [ ] `0071` creates publication-route bindings, inbound task/request bindings, external identity hashes, protocol projection snapshots/events and list pagination indexes.
- [ ] Atomically create Run, H7 interaction/routing records and inbound task binding through one application transaction/outbox boundary.
- [ ] Same identity/publication/message/hash returns the existing task. Same message ID with changed hash returns conflict.
- [ ] `get` and `list` enforce workspace, authenticated identity and publication visibility.
- [ ] Projection rows reference authoritative Run/public-event/interaction/Artifact IDs and store no prompt, secret or checkpoint.
- [ ] SDK TaskStore create/update methods can only be invoked by the projection service after an authoritative change.
- [ ] Restart after Run creation but before response returns the same task binding rather than creating a second Run.
- [ ] Run and commit.

### Task 13: Implement inbound Task projections, SSE subscription and transport equivalence

**Files:** create projection/SSE services, task projection store and inbound conformance tests.

- [ ] Map H1/H7/H6 state to A2A Task/Status/Message/Artifact deterministically.
- [ ] `WaitingForInput` emits `InputRequired`; `WaitingForAuthentication` emits `AuthRequired`; terminal Run states map without exposing internal errors.
- [ ] `Completed` is emitted only after Vestrace success criteria and Artifact availability pass.
- [ ] `GetTask` returns bounded history length and only artifacts authorized for the authenticated external requester.
- [ ] `ListTasks` uses opaque signed page tokens bound to identity/workspace/publication/filter and page size.
- [ ] `SubscribeToTask` starts from the persisted H7 cursor, projects subsequent canonical events and deduplicates reconnects.
- [ ] A slow client is disconnected with a resumable protocol state; it does not block Run workers.
- [ ] JSON-RPC and HTTP+JSON calls produce equivalent Task hashes and stable error codes.
- [ ] Cancellation maps to protected Run cancellation and does not claim compensation.
- [ ] Push config methods return `unsupported_operation`.
- [ ] Run and commit.

### Task 14: Integrate workers, Checkpoint V8 and restart recovery

**Files:** modify Run work/event/checkpoint and composition; add worker/restart tests.

- [ ] Add work kinds:

```text
DiscoverRemoteAgentCard
PublishA2AAgentCard
DispatchRemoteAgent
SubscribeRemoteTask
PollRemoteTask
ContinueRemoteTask
CancelRemoteTask
ReconcileRemoteTask
IngestRemoteArtifactPart
FinalizeRemoteArtifactAssembly
ProjectInboundA2ATask
```

- [ ] Work payloads contain IDs, exact revisions, hashes, local cursors and deadlines only—no clear URL query, headers, credential handle, raw part, Agent Card bytes or A2A SDK value.
- [ ] `RunCheckpointV8` extends V7 with active remote attempt IDs, local protocol event cursors, open artifact assembly IDs, pending remote HumanRequest IDs and inbound task binding IDs.
- [ ] Checkpoint contains no external message body, raw metadata, credentials, H8 lease, protocol frame or Task history.
- [ ] V7 and older checkpoints remain readable.
- [ ] Resume rules:
  - bound external task → fresh authorized Get/Subscribe;
  - Dispatching without durable task binding after possible send → Unknown and reconciliation;
  - open assembly → resume H6 ingestion/reconcile full Task snapshot;
  - inbound binding → rebuild projection from Run/public events, never re-execute request.
- [ ] Logical Run events are limited to remote delegation start/wait/unknown/completed-candidate/validated outcome and inbound Run state. Transport events remain H9A-local.
- [ ] Run and commit.

### Task 15: Add RLS, boundary gates, full conformance and H9A acceptance

**Files:** create migration `0072`, boundary scripts, CI jobs, RLS and acceptance tests.

- [ ] `0072` forces RLS, same-workspace/publication/invocation consistency, immutable revision/append-only event guards and indexes for task binding, event fingerprint, open stream/assembly, Unknown reconciliation and inbound list pagination.
- [ ] Boundary scripts reject:
  - any `a2a-rs` type/import outside adapter/test-support;
  - non-exact SDK git source;
  - grpc/pb/slimrpc dependencies;
  - SDK default client/factory use;
  - ambient proxy or redirect enablement;
  - credentials/headers in durable/public fields;
  - A2A Task as Run/SubRun/Tool authority;
  - direct remote Artifact availability;
  - automatic retry from Unknown;
  - push callback activation.
- [ ] Outbound conformance proves JSON-RPC/REST equivalence, task-required behavior, fresh lease per call, SSE replay dedup, status mapping, input/auth continuation, cancellation and artifact quarantine.
- [ ] Inbound conformance proves authentication, CreateRun/ContinueRun/CancelRun mapping, get/list/subscribe, TaskStore projection-only behavior and transport equivalence.
- [ ] Mandatory acceptance scenario:

```text
parent AgentRun
→ protected RemoteAgentInvocation
→ JSON-RPC or REST A2A dispatch
→ external Task binding
→ SSE Working
→ InputRequired
→ durable H7 pause
→ user response
→ fresh H2/H8 continuation
→ same external Task
→ streamed chunked Artifact
→ H6 quarantine and validation
→ H5 verified HandoffArtifact
→ parent continuation after worker restart
```

- [ ] Failure scenario drops the initial response after remote acceptance. Vestrace stores Unknown, reconciles through task/correlation evidence or remains Unknown, and the fixture asserts exactly one remote task.
- [ ] Inbound scenario exposes one exact Vestrace AgentRuntimeSnapshot, creates one Run through an authenticated A2A message, reconnects SSE, retrieves the same task through JSON-RPC and REST and proves the Task projection follows Run state without becoming authority.
- [ ] Acceptance additionally proves no credential in messages/state, Agent Card claims cannot widen activation, URL Artifact follows H6 SSRF checks, terminal regression is rejected, old snapshot/publication remains unchanged after package/card update and replay performs no network action.
- [ ] Required CI jobs: SDK pin/boundary, card discovery/publication, JSON-RPC, REST, transport equivalence, outbound lifecycle, SSE/dedup, continuation/auth, artifacts, inbound gateway, restart/reconciliation, RLS and H9A acceptance.
- [ ] Run and commit:

```bash
bash scripts/verify-a2a-sdk-pin.sh
bash scripts/verify-a2a-type-boundary.sh
bash scripts/verify-a2a-dependency-boundary.sh
bash scripts/verify-a2a-network-boundary.sh
bash scripts/verify-a2a-task-ownership.sh
bash scripts/verify-a2a-secret-boundary.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h9a_acceptance --test a2a_rls \
             --test a2a_transport_equivalence --test a2a_restart_recovery
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

git add migrations/0072_a2a_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml crates scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(a2a): add H9A interoperability and safety gates"
```

---

## Migration ownership

```text
0068 Task 4  A2A adapter bindings and exact outbound routes
0069 Task 5  Agent Card discovery observations and published Agent revisions
0070 Task 7  Outbound attempts, events, streams, Artifact assemblies and reconciliation
0071 Task 12 Inbound task/request bindings and protocol projections
0072 Task 15 RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H9A completion definition

H9A is complete only when all fifteen tasks pass and evidence demonstrates both directions:

```text
Outbound:
H5 RemoteAgentInvocation
→ exact H9 remote revision/activation
→ exact H9A route
→ H2 protected operation
→ H8 one-request credential lease
→ A2A JSON-RPC or HTTP+JSON
→ external Task
→ SSE/local dedup
→ H7 continuation
→ H6 Artifact quarantine
→ H5 verified handoff
→ parent Run continuation

Inbound:
authenticated A2A request
→ exact published AgentRuntimeSnapshot
→ idempotent Interaction/CreateRun or ContinueRun
→ authoritative AgentRun
→ H7 public event projection
→ A2A Task/Message/Artifact/SSE projection
```

Required invariants:

1. `a2a-rs` is exact-pinned and isolated to one adapter boundary.
2. JSON-RPC and HTTP+JSON map to the same canonical Vestrace contracts.
3. SDK default clients/factories, ambient proxies and redirects are not used.
4. A2A Task never becomes Run/SubRun/Tool authority.
5. H5 RemoteAgentInvocation remains the outbound durable aggregate.
6. H9 remote declaration and local activation remain separate from protocol transport.
7. Agent Card claims/signatures never grant trust or permission.
8. Outbound standard delegation requires a durable external Task ID.
9. Every network action has a distinct H2 authorization and fresh H8 lease.
10. Credentials never enter messages, cards, protocol events, artifacts or durable errors.
11. Message/task/context identifiers are bounded correlation values, not authority.
12. `tenant` never selects workspace or principal.
13. SSE replay is idempotent through local canonical fingerprints.
14. Terminal task state cannot regress.
15. Completed remains pending until H6/H5 validation succeeds.
16. Raw/URL Artifacts cannot bypass H6 quarantine/fetch security.
17. Incomplete/conflicting chunk assembly blocks success.
18. Input/Auth continuation uses H7 typed requests and the same external Task.
19. Cancellation is not rollback or compensation.
20. Unknown never redispatches automatically.
21. Reconciliation records positive evidence and preserves StillUnknown when uncertain.
22. Inbound message idempotency cannot create duplicate Runs.
23. Inbound authentication, not tenant/task metadata, resolves identity.
24. Inbound TaskStore is projection/cache only.
25. Get/List/Subscribe enforce identity/workspace/publication boundaries.
26. Existing Runs and publications remain pinned after package/card updates.
27. Restart does not duplicate a remote task, continuation, inbound Run, event or Artifact chunk.
28. Replay performs no external or protocol side effect.
29. H10 can add cross-run remote metrics/evaluation without rewriting H9A history.
30. Tests require no public A2A service, public network or permanent credential.

## Explicit non-goals

H9A does not implement required gRPC/protobuf, SLIMRPC, collaborative multi-party channels, push-notification callbacks, automatic Agent Card trust, trust federation, cross-organization credential delegation, remote policy negotiation, remote budget enforcement guarantees, full memory sharing, unrestricted Artifact URLs, outbound large-file URL hosting, generic webhooks, remote agents as Tools, A2A as the internal worker protocol, public marketplace discovery or automatic remote-agent fallback after ambiguous completion.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h9a-a2a-interoperability-gateway`, add or fetch production A2A dependencies, create migrations `0068`–`0072`, start A2A listeners, access a remote Agent Card, issue credentials, publish an Agent Card, send an A2A request, modify CI or execute H9A tests.