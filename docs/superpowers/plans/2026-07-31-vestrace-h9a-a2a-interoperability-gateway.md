# Vestrace H9A A2A Interoperability Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add A2A dependencies, create migrations, start protocol servers, contact remote agents, import or publish Agent Cards, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement a restart-safe bidirectional A2A v1 gateway that calls independent remote agents and exposes selected Vestrace agent snapshots through JSON-RPC, HTTP+JSON and SSE while preserving Vestrace ownership of Runs, policy, credentials, artifacts, continuations and reconciliation.

**Architecture:** H9A extends the transport-neutral H5 `RemoteAgentInvocation`, H7 continuation/event, H8 credential and H9 remote-definition/publication boundaries. `vestrace-remote-agent-runtime` owns normalized transport requests, observations, event fingerprints, intake/task bindings and reconciliation semantics; `vestrace-a2a-adapter` is an anti-corruption layer around exact-pinned `a2a-rs` crates. Outbound A2A Tasks remain remote-owned observations of a Vestrace invocation. Inbound A2A Tasks are protocol projections over authoritative Vestrace intake and `AgentRun` state. No A2A response can grant authority, bypass Artifact quarantine or declare a Vestrace Run successful.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H9; Rust Edition 2024; Tokio; Serde/Schemars; SQLx and PostgreSQL 17; Axum/Tower; Reqwest through a Vestrace-owned client factory; SSE; SHA-256 and HMAC-SHA-256; H2 ActionGuard; H6 Artifact ingestion/fetch; H7 public events and HumanRequest; H8 credential leases; H9 exact Agent/Remote-Agent revisions; exact-pinned `a2a-lf` 0.3.0, `a2a-client-lf` 0.2.1 and `a2a-server-lf` 0.4.1 from source commit `515f6eacf2b4b9b17bd3910e93ac47027afaaf90`; deterministic loopback A2A fixtures; proptest; tracing with mandatory redaction.

## Global Constraints

- Complete all five v0.1 plans and H1–H9 before implementing H9A.
- ADR-0003 `A2A Interoperability Boundary` is normative.
- H5 `RemoteAgentInvocation`, delegation grants, handoff validation and `Unknown` semantics remain authoritative. H9A never creates another remote-invocation aggregate.
- H7 owns typed `HumanRequest`, `HumanResponse`, public event cursors and channel-independent continuation.
- H8 owns outbound remote-agent Connections, exact remote profiles, dual-ticket credential leases and request-scoped secret injection.
- H9 owns immutable remote-agent declaration revisions, local activation/trust revisions, exact AgentRuntimeSnapshots and Extension activation.
- A2A Task is never an `AgentRun`, internal `SubRun`, Tool invocation, policy decision, budget account or Artifact.
- PostgreSQL is authoritative for A2A adapter/security bindings, route/publication revisions, discovery observations, outbound attempts/events/assemblies/reconciliation, inbound intake/request/task bindings and protocol projections.
- Vestrace owns all domain/application/persistence/public contracts. `a2a-rs`, Reqwest and Axum types may appear only in `vestrace-a2a-adapter` and explicit A2A wire endpoints.
- Exact SDK baseline is commit `515f6eacf2b4b9b17bd3910e93ac47027afaaf90`. Upgrades require a new adapter binding revision, conformance run and compatibility report.
- Initial dependencies are only `a2a-lf`, `a2a-client-lf` and `a2a-server-lf`. Production crates must not depend on `a2a-pb`, `a2a-grpc`, `a2a-slimrpc` or `a2acli`.
- Client/server crates use `default-features=false` and `rustls-no-provider`; the composition root installs one deployment-approved Rustls provider.
- Do not use `a2a_client::default_reqwest_client` or SDK default transport factories. The SDK workspace enables `system-proxy`; H9A injects a Vestrace-owned client with ambient proxies and redirects disabled.
- Initial bindings are A2A v1 JSON-RPC over HTTP, HTTP+JSON/REST and SSE send/subscription.
- gRPC/protobuf, SLIMRPC, collaborative channels and push callbacks are inactive and return stable unsupported errors.
- Push configuration methods are never registered in the first slice, even when a card advertises push support.
- Agent Card discovery/publication never grants trust or authority. Signatures and security declarations are compatibility evidence only.
- A remote declaration may only narrow route eligibility relative to the exact H9 activation. It cannot add an origin, transport, Skill, classification or credential scheme.
- Standard outbound delegation requires a Task response with an external task ID. Message-only completion is allowed only for an explicit read-only `ImmediateMessageAllowed` route requiring no continuation, cancellation, artifact stream or reconciliation.
- Reference packages and mandatory acceptance use `TaskRequired`.
- Dispatch, subscribe, poll, continue, cancel and authenticated card retrieval are separate protected operations with separate H2 decisions and fresh H8 leases.
- A lease is consumed once at the last practical enforcement point and never reused for reconnect or another request.
- Credentials never enter A2A messages, metadata, cards, tasks, protocol events, artifacts or errors.
- Outbound `tenant` is `None`. Inbound `tenant`, task/context/message IDs, headers and metadata are consistency inputs only and never identify workspace/principal.
- Protected outbound messages use deterministic opaque message IDs derived from invocation, sequence and canonical content hash; the random SDK `Message::new` helper is not used.
- Optional correlation metadata is an HMAC-derived opaque value with no internal UUID and no bearer authority.
- Remote cards/messages/status/metadata/extensions/errors/artifacts are untrusted input.
- A2A `Completed` maps to H5 `CompletedPendingValidation`, never directly to success.
- A2A `Canceled` is an observation, not evidence of rollback or compensation.
- `Unspecified`, malformed unions, task/context mismatch and terminal state regression are protocol violations.
- A response that may have been accepted but is not durably known produces `Unknown`.
- `Unknown` never redispatches automatically. Reconciliation uses known task/context IDs, correlation evidence, authorized Get/Subscribe/List and remains Unknown when evidence is insufficient.
- No authoritative remote SSE cursor is assumed. H9A stores a local sequence and canonical fingerprints; reconnect resubscribes or polls and deduplicates replay.
- Exact duplicate events are idempotent; same external identity with conflicting content is a protocol conflict.
- A terminal remote state cannot regress. Equivalent repeated terminal snapshots are allowed.
- One decoded raw part is at most 8 MiB, aggregate raw content per message 16 MiB and assembled remote artifact 64 MiB, subject to stricter H6/workspace policy.
- Text is at most 256 KiB per part and 1 MiB per message; metadata at most 64 KiB canonical JSON; 1–64 parts; history `0..=50`; list page `1..=100`.
- Outbound v0.2 supports authorized text, JSON and raw bytes within limits. Outbound URL parts and large-file hosting are disabled.
- Inbound URL parts go through H6 secure external fetch; the A2A adapter never downloads them directly.
- Raw/URL inbound content creates durable H6 intake work before it can affect Run context or execution.
- No remote Artifact becomes Available directly. Incremental parts use an H6 ingestion session and quarantine/inspection.
- `append=true` requires an open assembly; `last_chunk=true` closes it. Incomplete/conflicting required assemblies block success.
- Full Task snapshots reconcile incremental assemblies only with consistent artifact IDs and hashes.
- H7 HumanRequest is the only user-facing continuation for `InputRequired` and `AuthRequired`.
- H8 authentication completion supplies an opaque cause; the remote system cannot select local Connection or scopes.
- Inbound authentication is resolved by a Vestrace port. `tenant` and Agent Card security metadata are never accepted as identity.
- Inbound requests are idempotent by authenticated identity, publication revision, message ID and canonical request hash.
- Requests containing raw/URL parts first create a durable inbound intake and protocol Task in `Submitted`; CreateRun/ContinueRun occurs only after required parts are safely materialized.
- Text/JSON-only intake may be materialized and applied in the same transaction, but follows the same state machine.
- Reusing a message ID with changed content is an idempotency conflict.
- Inbound protocol TaskStore is projection/cache only. Run, policy, checkpoints, events, approvals and Artifacts remain authoritative elsewhere.
- Transport frames and projections do not increment `RunVersion`.
- Replay never discovers, dispatches, subscribes, polls, continues, cancels, fetches, injects credentials, applies inbound intake, creates Runs or emits protocol traffic.
- Existing migrations `0014`–`0067` are never edited. H9A migrations are `0068`–`0072`, each owned once.
- CI uses deterministic loopback JSON-RPC/REST/SSE fixtures and generated local credentials; no public A2A service/network/permanent secret is required.
- Future implementation branch: `feat/h9a-a2a-interoperability-gateway`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  remote_agent/{mod,transport,protocol_event,artifact_assembly,inbound_binding,publication,reconciliation,error}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  remote_agent_runtime/{mod,ports,commands,routing,dispatch,stream,continuation,cancellation,reconciliation,artifact,inbound,projection,publication,worker}.rs
  composition/a2a.rs

crates/vestrace-remote-agent-runtime/src/{lib,request,observation,event_fingerprint,state_mapping,content,cursor,errors}.rs
crates/vestrace-a2a-adapter/src/{lib,dependency_baseline,client,client_factory,http_client,jsonrpc,rest,sse,interceptor,message_mapping,task_mapping,event_mapping,artifact_mapping,card_discovery,card_publication,server,request_handler,executor,task_projection_store,auth,error}.rs
crates/vestrace-a2a-test-support/src/{lib,agent_card,remote_agent,client,server,jsonrpc_fixture,rest_fixture,sse_fixture,auth_fixture,artifact_fixture,faults,conformance}.rs

crates/vestrace-channel-http/src/{a2a_mount,a2a_auth}.rs
crates/vestrace-channel-cli/src/a2a_commands.rs
crates/vestrace-cli/src/composition/a2a.rs

crates/vestrace-infrastructure/src/postgres/a2a/
  mod.rs
  binding_repository.rs
  card_repository.rs
  outbound_repository.rs
  event_repository.rs
  artifact_assembly_repository.rs
  reconciliation_repository.rs
  inbound_binding_repository.rs
  intake_repository.rs
  projection_repository.rs

migrations/
  0068_a2a_adapter_bindings_and_routes.sql
  0069_a2a_card_discovery_and_publication.sql
  0070_a2a_outbound_attempts_events_and_reconciliation.sql
  0071_a2a_inbound_intake_task_bindings_and_projections.sql
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
  a2a_inbound_intake.rs
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

### Exact SDK baseline

```toml
[dependencies]
a2a = { package = "a2a-lf", git = "https://github.com/a2aproject/a2a-rs", rev = "515f6eacf2b4b9b17bd3910e93ac47027afaaf90" }
a2a-client = { package = "a2a-client-lf", git = "https://github.com/a2aproject/a2a-rs", rev = "515f6eacf2b4b9b17bd3910e93ac47027afaaf90", default-features = false, features = ["rustls-no-provider"] }
a2a-server = { package = "a2a-server-lf", git = "https://github.com/a2aproject/a2a-rs", rev = "515f6eacf2b4b9b17bd3910e93ac47027afaaf90", default-features = false, features = ["rustls-no-provider"] }
```

```text
a2a-lf        0.3.0
a2a-client-lf 0.2.1
a2a-server-lf 0.4.1
Rust floor    1.85
Edition       2024
```

`Cargo.lock` resolves all three to the exact commit. Only the adapter and test-support crates may depend on them.

### Binding, security and route revisions

```rust
pub enum A2AProtocolBindingKind { JsonRpcHttp, HttpJson }
pub enum A2AStreamingMode { Disabled, Sse }
pub enum RemoteCompletionMode { TaskRequired, ImmediateMessageAllowed }

pub struct A2AClientSecurityProfileRevision {
    pub id: A2AClientSecurityProfileRevisionId,
    pub profile_id: A2AClientSecurityProfileId,
    pub revision: u32,
    pub minimum_tls_version: String,
    pub extra_root_bundle_artifact_revision_id: Option<ArtifactRevisionId>,
    pub allow_loopback_http: bool,
    pub allow_ambient_proxy: bool,
    pub allow_redirects: bool,
    pub content_hash: [u8; 32],
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
    pub client_security_profile_revision_id: A2AClientSecurityProfileRevisionId,
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

Active standard security profiles require `allow_ambient_proxy=false` and `allow_redirects=false`. Loopback HTTP is test/local-development policy only. Endpoints reject userinfo/query/fragment/path traversal and origins outside exact H9/H8 allowlists.

### External identifiers and roles

```rust
#[serde(transparent)] pub struct RemoteTaskKey(String);
#[serde(transparent)] pub struct RemoteContextKey(String);
#[serde(transparent)] pub struct RemoteMessageKey(String);
#[serde(transparent)] pub struct RemoteArtifactKey(String);

pub enum RemoteMessageRole { User, Agent }

pub struct RemoteCorrelationToken {
    pub hmac: [u8; 32],
}
```

External keys are UTF-8, 1–512 bytes, contain no control characters and have redacted bounded Debug. Correlation grants no authority.

### Transport request and observation contracts

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(transparent)]
pub struct RemoteCredentialUseHandle(String);

pub struct RemoteTransportMessage {
    pub message_key: RemoteMessageKey,
    pub task_key: Option<RemoteTaskKey>,
    pub context_key: Option<RemoteContextKey>,
    pub role: RemoteMessageRole,
    pub parts: Vec<OutboundRemoteContentPart>,
    pub canonical_content_hash: [u8; 32],
    pub correlation_hmac: [u8; 32],
}

pub enum OutboundRemoteContentPart {
    Text { text: String, media_type: Option<String>, filename: Option<String> },
    Json { value: serde_json::Value, media_type: Option<String>, filename: Option<String> },
    Raw { revision_id: ArtifactRevisionId, byte_size: u64, media_type: String, filename: Option<String> },
}

pub struct RemoteTransportDispatchRequest {
    pub invocation_id: RemoteAgentInvocationId,
    pub attempt_id: RemoteDispatchAttemptId,
    pub route: A2ARemoteRouteRevision,
    pub message: RemoteTransportMessage,
    pub completion_mode: RemoteCompletionMode,
    pub credential_handle: Option<RemoteCredentialUseHandle>,
    pub deadline: Timestamp,
    pub idempotency_key: String,
}

pub struct RemoteTransportSubscribeRequest {
    pub invocation_id: RemoteAgentInvocationId,
    pub attempt_id: RemoteDispatchAttemptId,
    pub route: A2ARemoteRouteRevision,
    pub task_key: RemoteTaskKey,
    pub credential_handle: Option<RemoteCredentialUseHandle>,
    pub deadline: Timestamp,
}

pub struct RemoteTransportGetTaskRequest {
    pub invocation_id: RemoteAgentInvocationId,
    pub route: A2ARemoteRouteRevision,
    pub task_key: RemoteTaskKey,
    pub history_length: u16,
    pub credential_handle: Option<RemoteCredentialUseHandle>,
    pub deadline: Timestamp,
}

pub struct RemoteTransportListTasksRequest {
    pub invocation_id: RemoteAgentInvocationId,
    pub route: A2ARemoteRouteRevision,
    pub correlation_hmac: [u8; 32],
    pub page_size: u16,
    pub opaque_page_token: Option<String>,
    pub credential_handle: Option<RemoteCredentialUseHandle>,
    pub deadline: Timestamp,
}

pub struct RemoteTransportContinuationRequest {
    pub invocation_id: RemoteAgentInvocationId,
    pub route: A2ARemoteRouteRevision,
    pub task_key: RemoteTaskKey,
    pub context_key: RemoteContextKey,
    pub message: RemoteTransportMessage,
    pub credential_handle: Option<RemoteCredentialUseHandle>,
    pub deadline: Timestamp,
    pub idempotency_key: String,
}

pub struct RemoteTransportCancellationRequest {
    pub invocation_id: RemoteAgentInvocationId,
    pub route: A2ARemoteRouteRevision,
    pub task_key: RemoteTaskKey,
    pub credential_handle: Option<RemoteCredentialUseHandle>,
    pub deadline: Timestamp,
    pub idempotency_key: String,
}

pub struct RemoteTaskObservation {
    pub task_key: RemoteTaskKey,
    pub context_key: RemoteContextKey,
    pub state: RemoteProtocolTaskState,
    pub status_message: Option<RemoteMessageObservation>,
    pub artifacts: Vec<RemoteArtifactObservation>,
    pub canonical_hash: [u8; 32],
    pub source_timestamp: Option<Timestamp>,
}

pub struct RemoteTransportDispatchObservation {
    pub task: Option<RemoteTaskObservation>,
    pub immediate_message: Option<RemoteMessageObservation>,
    pub completion_may_have_occurred: bool,
}

pub struct RemoteTaskListObservation {
    pub tasks: Vec<RemoteTaskObservation>,
    pub opaque_next_page_token: Option<String>,
}

pub enum RemoteProtocolWireObservation {
    Task(RemoteTaskObservation),
    Message(RemoteMessageObservation),
    Status(RemoteStatusObservation),
    Artifact(RemoteArtifactObservation),
}

pub type RemoteProtocolEventStream = std::pin::Pin<Box<
    dyn futures_core::Stream<Item = Result<RemoteProtocolWireObservation, RemoteTransportError>> + Send
>>;
```

`RemoteCredentialUseHandle` is one-purpose, short-lived, non-Debug in clear form and never persisted. The H8 adapter creates it only after lease issuance; the A2A adapter consumes it once.

### Transport port

```rust
#[async_trait::async_trait]
pub trait RemoteTransportPort: Send + Sync {
    async fn dispatch(&self, request: RemoteTransportDispatchRequest)
        -> Result<RemoteTransportDispatchObservation, RemoteTransportError>;
    async fn subscribe(&self, request: RemoteTransportSubscribeRequest)
        -> Result<RemoteProtocolEventStream, RemoteTransportError>;
    async fn get_task(&self, request: RemoteTransportGetTaskRequest)
        -> Result<RemoteTaskObservation, RemoteTransportError>;
    async fn list_tasks(&self, request: RemoteTransportListTasksRequest)
        -> Result<RemoteTaskListObservation, RemoteTransportError>;
    async fn continue_task(&self, request: RemoteTransportContinuationRequest)
        -> Result<RemoteTransportDispatchObservation, RemoteTransportError>;
    async fn cancel_task(&self, request: RemoteTransportCancellationRequest)
        -> Result<RemoteTaskObservation, RemoteTransportError>;
}
```

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
    pub credential_binding_hash: Option<[u8; 32]>,
    pub external_task_key: Option<RemoteTaskKey>,
    pub external_context_key: Option<RemoteContextKey>,
    pub status: RemoteDispatchAttemptStatus,
    pub completion_may_have_occurred: bool,
    pub started_at: Timestamp,
    pub observed_at: Option<Timestamp>,
    pub logical_revision: u64,
}
```

A later attempt is permitted only after H5 reconciliation returns positive `FailedSafeToRedispatch` evidence and new guards/reservations/lease are created.

### Protocol states

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

```text
Submitted     → H5 Dispatching/Working
Working       → H5 Working
InputRequired → H5 WaitingForRemoteInput + H7 HumanRequest
AuthRequired  → H5 WaitingForAuthentication + H7 HumanRequest
Completed     → H5 CompletedPendingValidation
Failed        → H5 Failed
Rejected      → H5 Rejected
Canceled      → H5 Cancelled observation
Unspecified   → protocol violation / Unknown
lost response → H5 Unknown
```

### Normalized events

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

pub struct RemoteProtocolEventFingerprint { pub schema: u16, pub hash: [u8; 32] }

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
    pub external_artifact_key: Option<RemoteArtifactKey>,
    pub event_fingerprint: RemoteProtocolEventFingerprint,
    pub normalized_state: Option<RemoteProtocolTaskState>,
    pub safe_summary: Option<String>,
    pub normalized_metadata: Option<serde_json::Value>,
    pub artifact_assembly_id: Option<RemoteArtifactAssemblyId>,
    pub source_timestamp: Option<Timestamp>,
    pub observed_at: Timestamp,
}
```

Fingerprint V1 covers event kind, external IDs, normalized state, canonical content/metadata hashes, append/final flags and source timestamp. Local sequence is transactional per invocation or inbound task.

### Inbound content and H6 integration

```rust
pub struct RemoteUrlIngestionRequestId(uuid::Uuid);

pub enum RemoteContentPart {
    Text { text: String, media_type: Option<String>, filename: Option<String> },
    Json { value: serde_json::Value, media_type: Option<String>, filename: Option<String> },
    RawCandidate {
        ingestion_session_id: ArtifactIngestionSessionId,
        decoded_size: u64,
        media_type: Option<String>,
        filename: Option<String>,
    },
    UrlCandidate {
        ingestion_request_id: RemoteUrlIngestionRequestId,
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

pub struct RemoteArtifactObservation {
    pub artifact_key: RemoteArtifactKey,
    pub parts: Vec<RemoteContentPart>,
    pub append: bool,
    pub last_chunk: bool,
    pub canonical_content_hash: [u8; 32],
}

pub struct RemoteStatusObservation {
    pub task_key: RemoteTaskKey,
    pub context_key: RemoteContextKey,
    pub state: RemoteProtocolTaskState,
    pub message: Option<RemoteMessageObservation>,
    pub source_timestamp: Option<Timestamp>,
}
```

`RemoteUrlIngestionRequestId` is H9A-owned correlation. `RemoteArtifactPartPort` converts it to the existing H6 secure external-ingestion command and returns H6 IDs/references; H9A does not invent a second URL fetcher.

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
    pub external_artifact_key: RemoteArtifactKey,
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

`append=false`/absent starts a generation unless exact duplicate; `append=true` needs an open generation; `last_chunk=true` asks H6 to finalize quarantine. Conflict blocks handoff.

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

Timeout or TaskNotFound alone is not positive no-acceptance evidence.

### Agent Card discovery and publication

```rust
pub enum A2ACardObservationKind { PublicWellKnown, ExtendedAuthenticated }
pub enum A2APublicationLifecycle { Draft, Active, Disabled, Revoked }

pub struct A2APublishedAgent {
    pub id: A2APublishedAgentId,
    pub workspace_id: WorkspaceId,
    pub current_revision_id: Option<A2APublishedAgentRevisionId>,
    pub lifecycle: A2APublicationLifecycle,
    pub state_revision: u64,
}

pub struct A2APublishedInterface {
    pub binding: A2AProtocolBindingKind,
    pub endpoint: NormalizedA2AEndpoint,
    pub protocol_version: String,
}

pub struct A2APublishedSkill {
    pub stable_id: String,
    pub display_name: String,
    pub description: String,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
    pub input_schema_hash: Option<[u8; 32]>,
    pub output_schema_hash: Option<[u8; 32]>,
}

pub struct A2ASecuritySchemeDeclaration {
    pub stable_name: String,
    pub scheme_kind: String,
    pub public_configuration: serde_json::Value,
}

pub struct A2ASecurityDeclarationRevision {
    pub id: A2ASecurityDeclarationRevisionId,
    pub revision: u32,
    pub schemes: Vec<A2ASecuritySchemeDeclaration>,
    pub requirements_hash: [u8; 32],
    pub content_hash: [u8; 32],
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
    pub content_hash: [u8; 32],
}
```

Mutable lifecycle belongs to `A2APublishedAgent`; revisions are immutable. Publication declares no push capability or trust-bearing signature in the first slice.

### Inbound intake, bindings and projection

```rust
pub enum InboundA2AIntakeStatus {
    Received,
    Materializing,
    Ready,
    Applied,
    Rejected,
    Failed,
}

pub enum InboundA2ATaskStatus {
    Submitted,
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
    pub authentication_scheme: String,
    pub authentication_evidence_hash: [u8; 32],
}

pub struct InboundA2AIntake {
    pub id: InboundA2AIntakeId,
    pub workspace_id: WorkspaceId,
    pub publication_revision_id: A2APublishedAgentRevisionId,
    pub protocol_task_key: RemoteTaskKey,
    pub protocol_context_key: RemoteContextKey,
    pub message_key: RemoteMessageKey,
    pub canonical_request_hash: [u8; 32],
    pub status: InboundA2AIntakeStatus,
    pub required_ingestion_session_ids: Vec<ArtifactIngestionSessionId>,
    pub required_url_ingestion_request_ids: Vec<RemoteUrlIngestionRequestId>,
    pub applied_interaction_event_id: Option<InteractionEventId>,
    pub applied_routing_decision_id: Option<InteractionRoutingDecisionId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct InboundA2ATaskBinding {
    pub id: InboundA2ATaskBindingId,
    pub workspace_id: WorkspaceId,
    pub publication_revision_id: A2APublishedAgentRevisionId,
    pub protocol_task_key: RemoteTaskKey,
    pub protocol_context_key: RemoteContextKey,
    pub run_id: Option<AgentRunId>,
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
    pub intake_id: InboundA2AIntakeId,
    pub message_key: RemoteMessageKey,
    pub canonical_request_hash: [u8; 32],
    pub created_at: Timestamp,
}

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

A submitted binding may temporarily have no Run while required content is materialized. `Ready → Applied` atomically records H7 interaction/routing, creates or continues the exact Run and sets `run_id`. Projection remains protocol-facing only.

### Error model

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

No raw URL query, header, body, token fragment, SDK Debug output or unbounded remote text is included.

---

### Task 1: Pin the SDK and enforce the anti-corruption boundary

**Files:** modify workspace manifests/lock; create adapter/test-support baselines, tests and scripts.

- [ ] Add exactly the three git dependencies and `rustls-no-provider` feature setup.
- [ ] Add constants/tests for source commit, crate versions and `a2a::VERSION`.
- [ ] Verify Cargo.lock exact commit and fail on grpc/pb/slimrpc/a2acli.
- [ ] Prove non-adapter crates build with A2A disabled and contain no SDK import/type.
- [ ] Run/commit:

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

### Task 2: Add protocol-neutral domain contracts

**Files:** modify IDs; create remote-agent transport/event/assembly/intake/publication/reconciliation/error modules.

- [ ] Add every H9A ID and all normative values above.
- [ ] Test security profile, endpoint, external key, lifecycle and hash invariants.
- [ ] Property-test canonical event and message hashing independent of map insertion order.
- [ ] Test complete state mapping, terminal regression and CompletedPendingValidation.
- [ ] Test assembly duplicate/conflict/finalization and inbound intake transitions.
- [ ] Reject ImmediateMessageAllowed for any non-read-only or continuation/artifact requirement.
- [ ] Run and commit.

### Task 3: Define application ports and deterministic fixtures

**Files:** create application/runtime/test-support modules.

- [ ] Implement the exact RemoteTransportPort and request/observation DTOs above.
- [ ] Add `InboundExternalAgentPort`, `InboundA2AAuthenticationPort`, `A2ACardDiscoveryPort`, `A2ACardPublicationPort`, `RemoteArtifactPartPort`, event/intake/projection repository ports.
- [ ] Object-safety compile-test every port.
- [ ] Fixtures support JSON-RPC/REST equivalence, SSE replay, duplicate/conflicting events, Input/AuthRequired, chunking, response loss, terminal regression and TaskNotFound.
- [ ] Faults after request write, task bind, event/chunk/intake persistence, HumanRequest, continuation, Run creation and projection.
- [ ] Run and commit.

### Task 4: Persist adapter/security bindings and outbound routes

**Files:** create `0068`, services/repositories/tests.

- [ ] Create immutable adapter and client-security revisions, exact bindings, route identities/revisions and accepted modes.
- [ ] Standard active security rejects ambient proxy/redirect and requires TLS except explicit loopback policy.
- [ ] Route activation intersects exact H9 declaration/activation, H8 profile, origin, protocol and SDK capability.
- [ ] Reject push/grpc/slimrpc and any local-authority widening.
- [ ] Card/activation change creates a candidate route revision, never silent mutation.
- [ ] Run and commit.

### Task 5: Implement secure Agent Card discovery and publication

**Files:** create `0069`, services/mapping/tests.

- [ ] Persist discovery observations and H6 card Artifact links; create publication identities/revisions/interfaces/skills/security declarations.
- [ ] Public discovery uses H6 secure fetch; extended-card retrieval uses one H8 lease.
- [ ] Normalize into H9 candidate without trusting claims/signatures.
- [ ] Generate deterministic card bytes from one exact AgentRuntimeSnapshot/publication revision.
- [ ] Publish streaming only when inbound SSE is active; no push capability/signature in reference slice.
- [ ] Changed hash creates a candidate revision; unchanged is idempotent.
- [ ] Run and commit.

### Task 6: Implement hardened JSON-RPC and HTTP+JSON clients

**Files:** create HTTP/factory/transport/interceptor/mapping/error modules and conformance tests.

- [ ] Build Reqwest with no proxy, no redirect, exact TLS roots, bounded connect/request/idle/body and redacted tracing.
- [ ] Use `A2AClientFactory::builder().no_defaults()` and custom transport factories with the injected client.
- [ ] One-shot credential handle refuses second use.
- [ ] Build deterministic Message IDs, `tenant=None`, allowlisted metadata only.
- [ ] Normalize all SDK values/errors behind ports.
- [ ] Prove JSON-RPC/REST equivalence for send/get/list/cancel/subscribe/extended-card.
- [ ] Proxy trap receives zero traffic; redirects are rejected.
- [ ] Run and commit.

### Task 7: Persist outbound attempts, events, assemblies and reconciliation

**Files:** create `0070`, repositories/tests.

- [ ] Create attempts, task/context bindings, event sequences/fingerprints, streams, assemblies/chunks and reconciliation records.
- [ ] Enforce contiguous attempt numbers, one active initial attempt and event idempotency.
- [ ] Conflicting duplicate identity creates protocol conflict and blocks the invocation.
- [ ] Persist Dispatching before I/O; persist task binding and first observation atomically.
- [ ] Crash after task/event persistence resumes without redispatch.
- [ ] Run and commit.

### Task 8: Implement outbound dispatch, SSE and state projection

**Files:** create dispatch/stream/routing services and tests.

- [ ] Resolve exact route/delegation/context; consume H2/H8 immediately before transport.
- [ ] Enforce TaskRequired or restrictive immediate-message mode.
- [ ] Persist initial Task and each stream observation before H5 status projection.
- [ ] Every subscribe/reconnect/poll uses a fresh lease.
- [ ] Reconnect resubscribes or polls and deduplicates locally; no remote cursor assumption.
- [ ] Enforce state monotonicity, task/context consistency and deadlines.
- [ ] Publish only safe H7 progress after persistence.
- [ ] Run and commit.

### Task 9: Implement continuation, cancellation and reconciliation

**Files:** create services/tests.

- [ ] InputRequired creates one schema-bound H7 request; response creates deterministic continuation message and fresh guards/lease.
- [ ] AuthRequired uses opaque H8 references only; after auth, recheck exact route/profile/policy.
- [ ] Lost continuation response becomes Unknown without resend.
- [ ] Cancellation is separately guarded/persisted and returned Canceled is no rollback claim.
- [ ] Reconcile by known Get → optional Subscribe → policy-bounded List correlation.
- [ ] TaskNotFound after binding is not safe redispatch.
- [ ] Only positive no-acceptance evidence yields FailedSafeToRedispatch.
- [ ] Run and commit.

### Task 10: Map content/artifacts through H6 and H5 handoff

**Files:** create content/artifact services and tests.

- [ ] Outbound builder reads exact authorized H6 revisions and enforces limits; URL output is rejected.
- [ ] Inbound Text/Data become bounded H6 candidates, Raw streams to ingestion, URL calls H6 secure external ingestion via RemoteArtifactPartPort.
- [ ] Assembly ordering, duplicates, conflicts and full-snapshot reconciliation follow normative rules.
- [ ] H6 quarantine/inspection/secret scan completes before Available.
- [ ] Completed waits for required assemblies, schema, evidence and H5 handoff verification.
- [ ] Rejected/incomplete/conflicting Artifact cannot produce success.
- [ ] Preserve full remote provenance.
- [ ] Run and commit.

### Task 11: Implement authenticated inbound request handling and durable intake

**Files:** create server/auth/request-handler/executor/HTTP mount and intake/auth tests.

- [ ] Mount well-known card, JSON-RPC, REST and SSE under configured base path.
- [ ] Authenticate through Vestrace port and redact headers before any metadata persistence.
- [ ] Ignore tenant for identity; validate protocol/publication/body/parts/role/IDs.
- [ ] Idempotently create task binding, request binding and intake before materialization.
- [ ] Raw/URL parts enqueue H6 work; Task projects Submitted while intake is pending.
- [ ] Text/JSON-only intake may become Ready immediately but still passes the same service.
- [ ] `Ready → Applied` atomically records H7 interaction/routing, creates or continues the exact Run and sets run_id.
- [ ] Use a custom Vestrace RequestHandler; SDK helpers cannot independently mutate Run.
- [ ] Run and commit.

### Task 12: Persist inbound intake, task/request bindings and projections

**Files:** create `0071`, repositories/tests.

- [ ] Create intake rows/part links, task/request bindings, identity hashes, projection snapshots/events and pagination indexes.
- [ ] Same identity/publication/message/hash returns existing intake/task; changed hash conflicts.
- [ ] Apply Ready intake exactly once and atomically bind the Run.
- [ ] Get/List enforce identity/workspace/publication.
- [ ] Projection stores only authoritative IDs/hashes, no prompt/secret/checkpoint.
- [ ] SDK TaskStore writes are projection-service-only.
- [ ] Crash before/after intake apply returns the same Task/Run and does not duplicate either.
- [ ] Run and commit.

### Task 13: Implement inbound Task projection, SSE and transport equivalence

**Files:** create projection/SSE/store and conformance tests.

- [ ] Map pending intake to Submitted and H1/H7/H6 state to deterministic Task/Status/Message/Artifact.
- [ ] WaitingForInput/Auth map to protocol states; Completed only after Vestrace criteria/Artifact availability.
- [ ] GetTask bounds history and artifact authorization.
- [ ] ListTasks uses opaque signed identity/workspace/publication-bound page tokens.
- [ ] Subscribe starts from persisted H7 cursor and deduplicates reconnect.
- [ ] Slow client disconnects without blocking workers.
- [ ] JSON-RPC/HTTP+JSON produce equivalent Task hashes/errors.
- [ ] Cancel maps to protected Run cancellation; push methods unsupported.
- [ ] Run and commit.

### Task 14: Integrate workers, Checkpoint V8 and restart recovery

**Files:** modify Run work/event/checkpoint/composition and restart tests.

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
MaterializeInboundA2AIntake
ApplyInboundA2AIntake
ProjectInboundA2ATask
```

- [ ] Work payloads contain IDs/revisions/hashes/cursors/deadlines only.
- [ ] Checkpoint V8 adds active attempts, local cursors, open assemblies, pending remote HumanRequests, inbound intake/task IDs; no content/credentials/frames/history.
- [ ] V7 and older remain readable.
- [ ] Resume: bound task gets fresh Get/Subscribe; ambiguous unbound Dispatching becomes Unknown; open assembly resumes H6; pending intake resumes materialization; applied inbound binding rebuilds projection only.
- [ ] Logical Run events are remote start/wait/unknown/completed-candidate/validated outcome and inbound Run state; transport/intake telemetry stays local.
- [ ] Run and commit.

### Task 15: Add RLS, boundary gates, conformance and acceptance

**Files:** create `0072`, scripts, CI, RLS/acceptance tests.

- [ ] Force RLS, same-workspace/publication/invocation consistency, append-only revisions/events and indexes for task binding, intake, fingerprints, streams, assemblies and Unknown reconciliation.
- [ ] Scripts reject SDK leakage, non-exact pin, grpc/pb/slimrpc, default clients/factories, proxy/redirect enablement, secret/header fields, Task authority, direct Artifact availability, Unknown retry and push activation.
- [ ] Outbound conformance covers transport equivalence, task-required mode, fresh leases, SSE dedup, states, continuations, cancel, Artifact quarantine.
- [ ] Inbound conformance covers authentication, intake, Create/Continue/Cancel, Get/List/Subscribe and projection-only TaskStore.
- [ ] Mandatory outbound scenario:

```text
parent AgentRun
→ protected RemoteAgentInvocation
→ JSON-RPC or REST dispatch
→ external Task
→ SSE Working
→ InputRequired
→ durable pause
→ user response
→ fresh continuation authorization/lease
→ same Task
→ streamed Artifact
→ H6 quarantine/validation
→ H5 verified handoff
→ parent continuation after restart
```

- [ ] Lost-response scenario asserts one remote task and Unknown/reconciliation without duplicate dispatch.
- [ ] Inbound scenario publishes one exact snapshot, creates one Task/intake/Run, reconnects SSE and retrieves equivalent JSON-RPC/REST projection.
- [ ] Acceptance also proves card claims cannot widen, URL follows H6 SSRF policy, terminal regression rejection, pinning after package/card update and side-effect-free replay.
- [ ] Required CI jobs: SDK/boundary, cards, JSON-RPC, REST/equivalence, outbound, SSE, continuation/auth, artifacts, inbound intake/gateway, restart/reconciliation, RLS, acceptance.
- [ ] Run/commit:

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
0068 Task 4  Adapter/security bindings and exact outbound routes
0069 Task 5  Agent Card discovery and published Agent revisions
0070 Task 7  Outbound attempts, events, streams, assemblies and reconciliation
0071 Task 12 Inbound intake, request/task bindings and projections
0072 Task 15 RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H9A completion definition

```text
Outbound:
H5 RemoteAgentInvocation
→ exact H9 remote revision/activation
→ exact H9A route
→ H2 protected operation
→ H8 one-request lease
→ A2A JSON-RPC/HTTP+JSON
→ external Task
→ SSE/local dedup
→ H7 continuation
→ H6 Artifact quarantine
→ H5 verified handoff
→ parent continuation

Inbound:
authenticated A2A request
→ exact published AgentRuntimeSnapshot
→ durable intake and Task binding
→ safe content materialization
→ idempotent Interaction/CreateRun or ContinueRun
→ authoritative AgentRun
→ H7/H6 projection
→ A2A Task/Message/Artifact/SSE
```

Required invariants:

1. Exact-pinned SDK is isolated to the adapter.
2. JSON-RPC and HTTP+JSON use identical Vestrace contracts.
3. Default clients/factories, ambient proxies and redirects are absent.
4. A2A Task never owns Run/SubRun/Tool state.
5. H5 invocation remains the outbound aggregate.
6. H9 declaration/local activation remain separate from transport.
7. Agent Card claims/signatures grant nothing.
8. Standard outbound requires a durable task ID.
9. Every network request has distinct authorization and a fresh lease.
10. Credentials never enter protocol content/state.
11. IDs/tenant/metadata are correlation, not authority.
12. SSE replay is locally idempotent.
13. Terminal states cannot regress.
14. Completed waits for H6/H5 validation.
15. Raw/URL content cannot bypass H6.
16. Incomplete/conflicting assemblies block success.
17. Input/Auth use typed H7 continuation and same external Task.
18. Cancellation is no rollback claim.
19. Unknown never redispatches automatically.
20. Safe redispatch requires positive evidence.
21. Inbound raw/URL content cannot create/continue a Run before intake readiness.
22. Inbound message idempotency cannot duplicate intake, Task or Run.
23. Authentication, not tenant/task metadata, resolves identity.
24. Inbound TaskStore is projection only.
25. Get/List/Subscribe enforce identity/workspace/publication.
26. Existing Runs/publications remain pinned after updates.
27. Restart does not duplicate task, continuation, intake, Run, event or chunk.
28. Replay performs no protocol/network side effect.
29. H10 can add metrics/evaluation without rewriting H9A history.
30. Tests require no public A2A service/network/permanent credential.

## Explicit non-goals

H9A does not implement required gRPC/protobuf, SLIMRPC, collaborative channels, push callbacks, automatic Agent Card trust, trust federation, cross-organization credential delegation, remote policy negotiation, remote budget guarantees, full memory sharing, unrestricted Artifact URLs, outbound large-file hosting, generic webhooks, remote agents as Tools, A2A as internal worker protocol, marketplace discovery or automatic fallback after ambiguous completion.

## Documentation-only boundary

Creating this document does not authorize implementation. During documentation-only work, do not create `feat/h9a-a2a-interoperability-gateway`, add/fetch production A2A dependencies, create migrations `0068`–`0072`, start listeners, access a remote card, issue credentials, publish a card, send A2A traffic, change CI or execute H9A tests.