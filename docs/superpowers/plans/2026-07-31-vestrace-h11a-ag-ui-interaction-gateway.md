# Vestrace H11A AG-UI Interaction Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add AG-UI dependencies, create migrations, expose endpoints, modify the console, generate schemas, change release assets, update CI or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Add a durable, policy-governed AG-UI interaction gateway that lets the built-in console and independent AG-UI clients create, stream, interrupt, resume, reconnect and complete Vestrace Runs without making AG-UI state, messages, Tools or Run identifiers authoritative.

**Architecture:** H11A is an anti-corruption and projection layer over H1–H11. Inbound AG-UI envelopes are authenticated, bounded and materialized through H6/H7 before an existing application command creates or resumes an `AgentRun`; outbound AG-UI events are deterministic projections of H7 public events and current viewer-authorized state. PostgreSQL stores only Vestrace-owned endpoint revisions, external-key bindings, intake/projection state, interrupt bindings and conformance evidence. AG-UI SDK types remain inside one adapter crate and the TypeScript client/console integration. No `RunCheckpointV10` is introduced.

**Tech Stack:** Existing Vestrace H1–H11 and H9A; Rust Edition 2024; Tokio; Axum/Tower; SQLx and PostgreSQL 17; H7 durable public events and SSE cursors; H6 Artifact intake/quarantine; H2 policy/approval; H10 verification; H11 HTTP authentication, TypeScript SDK, console, schema bundle and release tooling; JSON Patch RFC 6902; exact AG-UI source revision `bb1c2afddb4880309879b9564cfb3a635a5da4eb`; TypeScript `@ag-ui/core` `0.0.57`; optional community Rust conformance crates `ag-ui-core`/`ag-ui-client` `0.1.0`; deterministic network-isolated AG-UI fixtures.

## Global Constraints

- ADR-0004 and the H11A roadmap amendment are normative.
- Complete H1–H11, H9A and the binding ADR-0002 outcome before implementing H11A.
- The implementation pin is exactly AG-UI source revision `bb1c2afddb4880309879b9564cfb3a635a5da4eb` until an explicit dependency-upgrade change passes schema diff, security review, adapter conformance and H10 regression evidence.
- `@ag-ui/core` is exactly `0.0.57` in the TypeScript lockfile for this plan.
- Community Rust AG-UI crates are optional conformance/test dependencies only; Vestrace production domain/application/persistence crates do not depend on them.
- AG-UI SDK/schema types are confined to `vestrace-ag-ui-adapter`, the TypeScript AG-UI integration layer, the console workspace and explicit wire/conformance tests.
- `AgentRun`, `RunStep`, `RunEvent`, `Conversation`, `InteractionEvent`, `HumanRequest`, Tool invocations, Artifacts, approvals, policy, budgets, credentials and terminal completion remain owned by H1–H11.
- AG-UI `threadId`, `runId` and `parentRunId` are untrusted external correlation keys, never Vestrace IDs.
- H7 `PublicEventRecord` and workspace cursor remain the durable event source.
- No `RunCheckpointV10` or AG-UI payload is added to a Run checkpoint.
- `RUN_FINISHED success` is emitted only for an authoritative terminal Run whose H10 one-use completion gate was consumed.
- `RUN_FINISHED interrupt` ends the current AG-UI stream but does not make the `AgentRun` terminal.
- A transport disconnect never changes Run state and is not projected as `RUN_ERROR`.
- Inbound state, transcript, context, Tools, metadata, forwarded properties, URLs and binary parts are untrusted.
- Inbound developer/system/assistant/Tool/reasoning messages never become trusted runtime instructions or canonical conversation history.
- Client Tool definitions may reference only exact server-published frontend-action definitions and cannot create H4 Tools or capabilities.
- AG-UI state snapshots and deltas are UI projections only. JSON Patch never applies to domain aggregates.
- Multimedia, document, binary and URL content pass H6 intake, quarantine and inspection before an Interaction can start or resume execution.
- Frontend effects use ordinary H11 commands with authentication, cross-surface idempotency, expected version and H2 authorization.
- Generic interrupt resolution, free-form “yes” or a boolean approval cannot create an H2 ApprovalGrant.
- `RAW`, `rawEvent`, all deprecated `THINKING_*`, all `REASONING_*`, `REASONING_ENCRYPTED_VALUE` and arbitrary `CUSTOM` events are prohibited.
- Only exact schema-pinned `vestrace.*` custom events are allowed.
- Secrets, credentials, policy tickets, internal paths, hidden reasoning, unsafe Tool arguments/results and active Artifact content never enter AG-UI payloads.
- SSE `id` is the H7 cursor. `Last-Event-ID` and an explicit cursor parameter may not disagree.
- Delivery is at-least-once; clients deduplicate by Vestrace event ID and projection key.
- Personal may enable the loopback AG-UI console surface by safe default. Team and Embedded require explicit configuration.
- Initial transport is authenticated HTTP POST returning SSE. WebSocket and AG-UI binary transport are deferred.
- H11 migrations `0081`–`0086` are never edited. H11A migrations are `0087`–`0091`, each created once by one task.
- Future implementation branch: `feat/harness-ag-ui-gateway`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock

crates/vestrace-domain/src/
  id.rs
  ag_ui/{mod,endpoint,binding,intake,projection,interrupt,frontend_action,release}.rs

crates/vestrace-application/src/
  ag_ui/{mod,ports,input_service,projection_service,resume_service,frontend_action_service,release_service}.rs
  composition/ag_ui.rs

crates/vestrace-ag-ui-adapter/
  Cargo.toml
  src/
    lib.rs
    pin.rs
    wire/{mod,input,event,error,extension}.rs
    decode.rs
    encode.rs
    input_mapper.rs
    event_mapper.rs
    state_patch.rs
    http_sse.rs
    bounds.rs
  tests/{golden_vectors,forbidden_events,cross_language}.rs

crates/vestrace-channel-http/src/
  routes/ag_ui.rs
  dto/ag_ui.rs
  sse/ag_ui.rs

crates/vestrace-infrastructure/src/postgres/ag_ui/
  mod.rs
  endpoint_repository.rs
  binding_repository.rs
  intake_repository.rs
  projection_repository.rs
  interrupt_repository.rs
  frontend_action_repository.rs
  conformance_repository.rs

packages/sdk-typescript/src/ag-ui/
  index.ts
  client.ts
  types.ts
  cursor.ts
  reducer.ts
  interrupts.ts
  frontendActions.ts
  errors.ts

apps/console/src/
  ag-ui/{client,session,reducer,events,interrupts,frontendActions,generativeUi}.ts
  components/ag-ui/{AgentWorkspace,ConversationPane,ActivityTimeline,InterruptForm,ApprovalCard,ArtifactCard,ToolActivityCard,StateInspector}.tsx
  pages/AgentWorkspace.tsx

schemas/ag-ui/v1/
  source-pin.json
  run-agent-input.schema.json
  event.schema.json
  extension.schema.json
  state-projection.schema.json
  frontend-action.schema.json

fixtures/ag-ui/
  inputs/
  events/
  forbidden/
  multimedia/
  reconnect/
  console/

migrations/
  0087_ag_ui_endpoints_run_bindings_and_intakes.sql
  0088_ag_ui_projection_snapshots_events_and_cursors.sql
  0089_ag_ui_interrupt_bindings_and_frontend_actions.sql
  0090_ag_ui_schema_pins_conformance_and_release_evidence.sql
  0091_ag_ui_rls_indexes_and_cross_resource_guards.sql

tests/
  ag_ui_pin_boundary.rs
  ag_ui_input_contract.rs
  ag_ui_binding_idempotency.rs
  ag_ui_multimedia_intake.rs
  ag_ui_event_projection.rs
  ag_ui_state_patch.rs
  ag_ui_interrupt_resume.rs
  ag_ui_approval_boundary.rs
  ag_ui_frontend_actions.rs
  ag_ui_http_sse.rs
  ag_ui_reconnect.rs
  ag_ui_console_contract.rs
  ag_ui_release_manifest.rs
  ag_ui_rls.rs
  ag_ui_vertical_slice.rs
  ag_ui_restart_matrix.rs

scripts/
  generate-ag-ui-schemas.sh
  verify-ag-ui-pin.sh
  verify-ag-ui-type-boundary.sh
  verify-ag-ui-forbidden-events.sh
  verify-ag-ui-release-evidence.sh
  run-h11a-ag-ui-vertical-slice.sh

docs/
  ag-ui.md
  ag-ui-security.md
  ag-ui-console.md
```

---

## Normative contracts

### Exact protocol pin

```rust
pub struct AgUiProtocolPin {
    pub repository: String,
    pub source_revision: String,
    pub typescript_core_version: String,
    pub rust_core_version: Option<String>,
    pub rust_client_version: Option<String>,
    pub input_schema_sha256: [u8; 32],
    pub event_schema_sha256: [u8; 32],
    pub extension_schema_sha256: [u8; 32],
    pub content_hash: [u8; 32],
}
```

Required values:

```text
repository              ag-ui-protocol/ag-ui
source_revision         bb1c2afddb4880309879b9564cfb3a635a5da4eb
typescript_core_version 0.0.57
rust_core_version       0.1.0
rust_client_version     0.1.0
```

Schema hashes are generated from the exact pinned source and committed as deterministic release inputs.

### Endpoint revision

```rust
pub enum AgUiEndpointLifecycle { Draft, Active, Disabled, Revoked }

pub enum AgUiTransportBinding { HttpPostSse }

pub struct AgUiInputLimits {
    pub maximum_request_bytes: u64,
    pub maximum_messages: u16,
    pub maximum_message_bytes: u64,
    pub maximum_context_entries: u16,
    pub maximum_context_entry_bytes: u64,
    pub maximum_tools: u16,
    pub maximum_state_bytes: u64,
    pub maximum_forwarded_props_bytes: u64,
    pub maximum_inline_binary_bytes: u64,
    pub maximum_parts: u16,
}

pub struct AgUiEndpointRevision {
    pub id: AgUiEndpointRevisionId,
    pub endpoint_id: AgUiEndpointId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub lifecycle: AgUiEndpointLifecycle,
    pub transport: AgUiTransportBinding,
    pub agent_runtime_snapshot_id: AgentRuntimeSnapshotId,
    pub protocol_pin_hash: [u8; 32],
    pub input_schema_hash: [u8; 32],
    pub event_profile_hash: [u8; 32],
    pub state_projection_schema_hash: [u8; 32],
    pub frontend_action_catalogue_revision_id: AgUiFrontendActionCatalogueRevisionId,
    pub limits: AgUiInputLimits,
    pub maximum_classification: DataClassification,
    pub content_hash: [u8; 32],
}
```

Lifecycle changes create a new revision; active Runs remain bound to the exact revision selected at intake.

### External keys and binding

```rust
pub struct AgUiExternalThreadKeyHash(pub [u8; 32]);
pub struct AgUiExternalRunKeyHash(pub [u8; 32]);

pub enum AgUiRunBindingLifecycle {
    IntakePending,
    Bound,
    Interrupted,
    Streaming,
    Terminal,
    Rejected,
}

pub struct AgUiRunBinding {
    pub id: AgUiRunBindingId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub endpoint_revision_id: AgUiEndpointRevisionId,
    pub external_thread_key_hash: AgUiExternalThreadKeyHash,
    pub external_run_key_hash: AgUiExternalRunKeyHash,
    pub canonical_input_hash: [u8; 32],
    pub conversation_id: Option<ConversationId>,
    pub run_id: Option<AgentRunId>,
    pub lifecycle: AgUiRunBindingLifecycle,
    pub projection_version: u64,
    pub last_public_event_cursor: Option<PublicEventCursor>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Uniqueness is `(workspace_id, principal_id, endpoint_revision_id, external_run_key_hash)`. Same hash and same canonical input returns the existing binding. Same key with changed canonical input conflicts.

### Intake

```rust
pub enum AgUiIntakeStatus {
    Received,
    Validating,
    Materializing,
    Ready,
    Applied,
    Rejected,
    Expired,
}

pub enum AgUiInputPartKind { Text, Image, Audio, Video, Document, Binary, Url }

pub struct AgUiInputPartReference {
    pub ordinal: u16,
    pub kind: AgUiInputPartKind,
    pub media_type: Option<String>,
    pub artifact_revision_id: Option<ArtifactRevisionId>,
    pub bounded_text_hash: Option<[u8; 32]>,
    pub source_hash: [u8; 32],
}

pub struct AgUiIntake {
    pub id: AgUiIntakeId,
    pub run_binding_id: AgUiRunBindingId,
    pub status: AgUiIntakeStatus,
    pub canonical_input_hash: [u8; 32],
    pub user_message_hashes: Vec<[u8; 32]>,
    pub context_candidate_hashes: Vec<[u8; 32]>,
    pub input_parts: Vec<AgUiInputPartReference>,
    pub expected_projection_revision: Option<u64>,
    pub resume_entries_hash: Option<[u8; 32]>,
    pub rejection_code: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

`Ready → Applied` is one transaction that creates or resumes the H7/H1 authoritative state and binds IDs. No Run work is leased while required input parts remain uninspected.

### Protocol-neutral inbound DTO

```rust
pub enum AgUiInboundMessageRole { User, UntrustedImportedHistory }

pub struct AgUiInboundMessage {
    pub external_message_key_hash: [u8; 32],
    pub role: AgUiInboundMessageRole,
    pub text: Option<String>,
    pub part_ordinals: Vec<u16>,
}

pub struct AgUiContextCandidate {
    pub description: String,
    pub value: String,
    pub content_hash: [u8; 32],
}

pub struct AgUiFrontendActionDeclarationRef {
    pub stable_id: String,
    pub revision_hash: [u8; 32],
}

pub struct AgUiInboundEnvelope {
    pub external_thread_key: String,
    pub external_run_key: String,
    pub parent_external_run_key: Option<String>,
    pub messages: Vec<AgUiInboundMessage>,
    pub context_candidates: Vec<AgUiContextCandidate>,
    pub state_projection: Option<serde_json::Value>,
    pub expected_projection_revision: Option<u64>,
    pub frontend_actions: Vec<AgUiFrontendActionDeclarationRef>,
    pub resume_entries: Vec<AgUiResumeEntry>,
    pub forwarded_properties: std::collections::BTreeMap<String, serde_json::Value>,
    pub canonical_input_hash: [u8; 32],
}
```

Only user messages are accepted in the standard profile. Compatibility import maps other allowed transcript entries to `UntrustedImportedHistory`; it never maps developer/system/assistant/Tool/reasoning content to authoritative roles.

### Projection and extension envelope

```rust
pub struct AgUiVestraceExtension {
    pub schema_version: u16,
    pub event_id: PublicEventId,
    pub cursor: PublicEventCursor,
    pub run_version: Option<u64>,
    pub projection_version: u64,
    pub causation_id: CausationId,
}

pub enum AgUiProjectedEventKind {
    RunStarted,
    RunFinishedSuccess,
    RunFinishedInterrupt,
    RunError,
    StepStarted,
    StepFinished,
    TextMessageStart,
    TextMessageContent,
    TextMessageEnd,
    ToolCallStart,
    ToolCallArgs,
    ToolCallEnd,
    ToolCallResult,
    StateSnapshot,
    StateDelta,
    MessagesSnapshot,
    ActivitySnapshot,
    ActivityDelta,
    Custom,
}

pub struct AgUiProjectedEvent {
    pub binding_id: AgUiRunBindingId,
    pub projection_key: String,
    pub kind: AgUiProjectedEventKind,
    pub extension: AgUiVestraceExtension,
    pub safe_payload: serde_json::Value,
    pub payload_hash: [u8; 32],
}
```

Projection uniqueness is `(binding_id, event_id, projection_key)`. The same public event may produce multiple ordered AG-UI events; `projection_key` is deterministic, such as `text:start:<message-id>`.

### State projection

```rust
pub struct AgUiStateProjection {
    pub schema_version: u16,
    pub projection_version: u64,
    pub run: AgUiRunStateView,
    pub plan: Option<AgUiPlanStateView>,
    pub activity: Vec<AgUiActivityView>,
    pub human_requests: Vec<AgUiHumanRequestView>,
    pub artifacts: Vec<AgUiArtifactView>,
    pub budget: Option<AgUiBudgetView>,
    pub verification: Option<AgUiVerificationView>,
    pub content_hash: [u8; 32],
}

pub struct AgUiStatePatch {
    pub base_projection_version: u64,
    pub target_projection_version: u64,
    pub operations: Vec<serde_json::Value>,
    pub resulting_content_hash: [u8; 32],
}
```

Allowed JSON Pointer prefixes:

```text
/run/status
/run/version
/plan
/activity
/humanRequests
/artifacts
/budget
/verification
```

Maximum 128 operations and 256 KiB encoded patch. On base mismatch, invalid path or invalid resulting schema, emit a full snapshot instead of applying the patch.

### Interrupt and resume

```rust
pub struct AgUiInterruptBinding {
    pub id: AgUiInterruptBindingId,
    pub run_binding_id: AgUiRunBindingId,
    pub human_request_id: HumanRequestId,
    pub external_interrupt_id_hash: [u8; 32],
    pub response_schema_hash: [u8; 32],
    pub approval_challenge_id: Option<ApprovalChallengeId>,
    pub expires_at: Option<Timestamp>,
    pub consumed_response_id: Option<HumanResponseId>,
}

pub enum AgUiResumeStatus { Resolved, Cancelled }

pub struct AgUiResumeEntry {
    pub external_interrupt_id: String,
    pub status: AgUiResumeStatus,
    pub payload: Option<serde_json::Value>,
}
```

A resume entry binds to exactly one open HumanRequest. Repeated identical resume returns the existing response. Changed payload conflicts. Approval requests call the H7/H2 approval path; no generic resolved state can create an ApprovalGrant.

### Frontend actions

```rust
pub enum AgUiFrontendActionAuthority {
    ClientOnly,
    ServerCommand,
}

pub enum AgUiFrontendActionRisk {
    Navigation,
    ReadOnly,
    UserInput,
    ProtectedMutation,
}

pub struct AgUiFrontendActionDefinitionRevision {
    pub id: AgUiFrontendActionDefinitionRevisionId,
    pub stable_id: String,
    pub revision: u32,
    pub authority: AgUiFrontendActionAuthority,
    pub risk: AgUiFrontendActionRisk,
    pub input_schema_hash: [u8; 32],
    pub output_schema_hash: Option<[u8; 32]>,
    pub mapped_product_operation: Option<String>,
    pub required_capabilities: CapabilitySet,
    pub content_hash: [u8; 32],
}

pub struct AgUiFrontendActionCatalogueRevision {
    pub id: AgUiFrontendActionCatalogueRevisionId,
    pub revision: u32,
    pub definitions: Vec<AgUiFrontendActionDefinitionRevisionId>,
    pub content_hash: [u8; 32],
}
```

Initial client-only actions:

```text
vestrace.ui.navigate.run
vestrace.ui.navigate.artifact
vestrace.ui.focus.human-request
vestrace.ui.copy.reference
vestrace.ui.expand.activity
```

Initial server-command actions:

```text
vestrace.command.submit-human-response
vestrace.command.grant-approval
vestrace.command.cancel-run
vestrace.command.resume-run
vestrace.command.create-download-grant
vestrace.command.export-artifact
```

Server-command actions invoke the ordinary H11 command facade. An AG-UI Tool-call event alone never invokes them.

### Event profile

Enabled:

```text
RUN_STARTED
RUN_FINISHED
RUN_ERROR
STEP_STARTED
STEP_FINISHED
TEXT_MESSAGE_START
TEXT_MESSAGE_CONTENT
TEXT_MESSAGE_END
TOOL_CALL_START
TOOL_CALL_ARGS when safe
TOOL_CALL_END
TOOL_CALL_RESULT when safe
STATE_SNAPSHOT
STATE_DELTA
MESSAGES_SNAPSHOT
ACTIVITY_SNAPSHOT
ACTIVITY_DELTA
schema-pinned vestrace.* CUSTOM
```

Forbidden:

```text
RAW
rawEvent
THINKING_*
REASONING_START
REASONING_MESSAGE_*
REASONING_END
REASONING_ENCRYPTED_VALUE
arbitrary CUSTOM
```

### Conformance and release evidence

```rust
pub struct AgUiConformanceReport {
    pub id: AgUiConformanceReportId,
    pub protocol_pin_hash: [u8; 32],
    pub adapter_revision: String,
    pub input_vector_hash: [u8; 32],
    pub event_vector_hash: [u8; 32],
    pub forbidden_event_report_hash: [u8; 32],
    pub restart_report_hash: [u8; 32],
    pub console_report_hash: [u8; 32],
    pub passed: bool,
    pub content_hash: [u8; 32],
}

pub struct AgUiReleaseEvidence {
    pub release_manifest_id: ProductReleaseManifestId,
    pub protocol_pin_hash: [u8; 32],
    pub schema_bundle_hash: [u8; 32],
    pub conformance_report_id: AgUiConformanceReportId,
    pub console_bundle_hash: [u8; 32],
    pub content_hash: [u8; 32],
}
```

A release cannot advertise `ProductSurface::AgUiHttpSse` without passing exact evidence.

---

### Task 1: Add protocol-neutral AG-UI domain contracts

**Files:** create domain modules, modify ID exports and add unit/property tests.

**Consumes:** H1 Run IDs/state, H6 Artifact revision IDs/classification, H7 Conversation/HumanRequest/public cursor, H9 runtime snapshot, H10 completion, H11 release IDs.

**Produces:** every H11A domain type in the normative contracts.

- [ ] Add IDs: `AgUiEndpointId`, `AgUiEndpointRevisionId`, `AgUiRunBindingId`, `AgUiIntakeId`, `AgUiInterruptBindingId`, `AgUiFrontendActionDefinitionRevisionId`, `AgUiFrontendActionCatalogueRevisionId`, `AgUiConformanceReportId`.
- [ ] Write transition tests for endpoint lifecycle, binding lifecycle and intake lifecycle. Reject terminal-to-active, Applied-to-Materializing and consumed interrupt reuse.
- [ ] Write canonical hash golden tests for endpoint, binding request identity, intake, projection, state projection, frontend action and conformance report.
- [ ] Write serialization-negative tests proving no AG-UI SDK type, raw event, secret, checkpoint or clear external key is represented.
- [ ] Implement the domain contracts and validation bounds exactly as above.
- [ ] Run:

```bash
cargo test -p vestrace-domain ag_ui::
```

- [ ] Commit:

```bash
git add crates/vestrace-domain/src/ag_ui crates/vestrace-domain/src/id.rs
git commit -m "feat(ag-ui): add interaction gateway contracts"
```

### Task 2: Pin AG-UI and create the anti-corruption wire crate

**Files:** create `vestrace-ag-ui-adapter`, pin/schema scripts, fixtures and boundary tests.

**Consumes:** Task 1 domain-neutral DTOs.

**Produces:** `decode_run_agent_input`, `encode_projected_event`, exact pin metadata and golden vectors.

- [ ] Add the adapter crate without adding AG-UI dependencies to domain/application/infrastructure crates.
- [ ] Record the exact source commit and package versions in `pin.rs` and `schemas/ag-ui/v1/source-pin.json`.
- [ ] Generate deterministic schemas from pinned TypeScript source and store hashes in `AgUiProtocolPin` fixture.
- [ ] Implement wire DTOs only inside the adapter for pinned `RunAgentInput`, messages, content parts, Tools, interrupts/resume and enabled events.
- [ ] Decoder rejects prohibited message roles in standard mode, unknown frontend Tool definitions, non-`vestrace.*` forwarded properties and all input limits.
- [ ] Encoder has no constructors for RAW or reasoning events and rejects arbitrary custom names.
- [ ] Add cross-language vectors validated by pinned TypeScript `@ag-ui/core` and optional Rust community crates.
- [ ] Boundary script fails if `ag_ui_core`, `@ag-ui/*` names or wire DTO imports appear outside allowed paths.
- [ ] Run:

```bash
cargo test -p vestrace-ag-ui-adapter
bash scripts/generate-ag-ui-schemas.sh
bash scripts/verify-ag-ui-pin.sh
bash scripts/verify-ag-ui-type-boundary.sh
```

- [ ] Commit:

```bash
git add crates/vestrace-ag-ui-adapter schemas/ag-ui fixtures/ag-ui scripts Cargo.toml Cargo.lock
git commit -m "feat(ag-ui): pin protocol and isolate wire types"
```

### Task 3: Define application ports, intake orchestration and deterministic fixtures

**Files:** create application AG-UI modules and fixture adapters.

**Consumes:** Task 1 contracts and Task 2 protocol-neutral decoder output.

**Produces:** services and ports consumed by persistence/HTTP/projection tasks.

- [ ] Define `AgUiEndpointRepositoryPort`, `AgUiBindingRepositoryPort`, `AgUiIntakeRepositoryPort`, `AgUiProjectionRepositoryPort`, `AgUiInterruptRepositoryPort`, `AgUiFrontendActionRepositoryPort`, `AgUiConformanceRepositoryPort`.
- [ ] Define `AgUiInputMaterializationPort` over H6 ingestion/fetch and `AgUiInteractionCommandPort` over H7/H11 commands.
- [ ] Implement `AgUiInputService::begin`, `record_materialized_part`, `reject`, `apply_ready`.
- [ ] `begin` hashes clear external keys immediately and returns ExistingSameInput/Conflict/New.
- [ ] `apply_ready` atomically creates or resumes H7 state and binds Conversation/Run; no model work is scheduled before Ready.
- [ ] Build fixtures for text-only, inspected document, rejected URL, duplicate key, changed key, crash before/after H7 command commit.
- [ ] Test no application port accepts AG-UI wire types.
- [ ] Run:

```bash
cargo test -p vestrace-application ag_ui::
```

- [ ] Commit:

```bash
git add crates/vestrace-application/src/ag_ui crates/vestrace-application/src/composition/ag_ui.rs fixtures/ag-ui
git commit -m "feat(ag-ui): add intake and projection application ports"
```

### Task 4: Persist endpoints, run bindings and durable intake

**Files:** create migration `0087`, repositories and integration tests.

**Consumes:** Task 3 repository ports.

**Produces:** restart-safe endpoint/binding/intake storage.

- [ ] `0087` creates endpoint identities/revisions/lifecycle, external-key bindings, intake records, message/context hashes, part references and H6/H7/H1 foreign-key bindings.
- [ ] Store only keyed hashes of external thread/run/message/interrupt keys; never clear values or raw request JSON.
- [ ] Unique constraint enforces workspace/principal/endpoint/external-run key.
- [ ] `begin_binding_and_intake` is one transaction and returns same/conflict/new deterministically.
- [ ] `apply_ready` locks binding/intake and atomically records Conversation/Run IDs after the existing H7/H1 command result.
- [ ] Work recovery finds Received/Validating/Materializing/Ready records without creating duplicate Runs.
- [ ] Test RLS preliminarily, concurrent duplicate, changed body, expired intake, crash before command, crash after command before binding acknowledgement.
- [ ] Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test ag_ui_binding_idempotency --test ag_ui_input_contract
```

- [ ] Commit:

```bash
git add migrations/0087_ag_ui_endpoints_run_bindings_and_intakes.sql \
  crates/vestrace-infrastructure/src/postgres/ag_ui tests/ag_ui_binding_idempotency.rs tests/ag_ui_input_contract.rs
git commit -m "feat(ag-ui): persist endpoints bindings and intake"
```

### Task 5: Implement H6 multimedia, document and URL intake

**Files:** extend input service/materialization adapter and add fixtures/tests.

**Consumes:** H6 ingestion, secure URL fetch, inspections and Task 4 intake persistence.

**Produces:** inspected `AgUiInputPartReference` values and Ready intake.

- [ ] Text parts are bounded and hashed; inline image/audio/video/document/binary data streams into H6 without whole-body buffering.
- [ ] URL parts call H6 secure external ingestion with SSRF, DNS, redirect, MIME, size and timeout policies.
- [ ] Required parts keep intake Materializing until exact Artifact revisions are available and pass required inspections.
- [ ] Rejected/quarantined-for-review part rejects or pauses intake according to endpoint policy; it never starts Run execution.
- [ ] Same source hash reuses the same intake part binding without duplicating H6 ingestion.
- [ ] Test document success, malware rejection, oversized base64/data, URL private IP, DNS rebinding, response loss and restart.
- [ ] Run:

```bash
cargo test --test ag_ui_multimedia_intake
```

- [ ] Commit:

```bash
git add crates/vestrace-application/src/ag_ui/input_service.rs fixtures/ag-ui/multimedia tests/ag_ui_multimedia_intake.rs
git commit -m "feat(ag-ui): route rich input through artifact intake"
```

### Task 6: Persist projection snapshots, projected events and durable cursors

**Files:** create migration `0088`, repository and tests.

**Consumes:** Task 4 bindings and H7 public events.

**Produces:** deduplicated projection records and state snapshots.

- [ ] `0088` creates projection snapshots, projected event records, public-event links, projection keys, cursor checkpoints and stream-session observations.
- [ ] Unique `(binding_id, public_event_id, projection_key)` prevents duplicate logical events.
- [ ] Projection snapshots store safe JSON, schema/hash/version and source cursor; they are rebuildable projections, not authority.
- [ ] Cursor advancement occurs only after all deterministic projections for a public event are persisted.
- [ ] Rebuilding the same cursor range is idempotent; changed mapping output for the same pin is a conformance conflict.
- [ ] Test multi-event projection, restart during batch, duplicate H7 delivery, cursor disagreement and cross-workspace isolation.
- [ ] Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test ag_ui_event_projection --test ag_ui_reconnect
```

- [ ] Commit:

```bash
git add migrations/0088_ag_ui_projection_snapshots_events_and_cursors.sql \
  crates/vestrace-infrastructure/src/postgres/ag_ui/projection_repository.rs \
  tests/ag_ui_event_projection.rs tests/ag_ui_reconnect.rs
git commit -m "feat(ag-ui): persist durable interaction projections"
```

### Task 7: Project lifecycle, messages and steps

**Files:** implement projection service and adapter event mapping.

**Consumes:** H7 public events, H1 Run/step query views, Task 6 repository.

**Produces:** lifecycle/text/step AG-UI events.

- [ ] Map visible Run start to one `RUN_STARTED` after binding is durable.
- [ ] Map assistant message stream to stable `TEXT_MESSAGE_START/CONTENT/END` projection keys and safe bounded deltas.
- [ ] Map visible steps to `STEP_STARTED/STEP_FINISHED` without exposing private plan nodes or hidden SubRun context.
- [ ] Emit `RUN_FINISHED success` only when the authoritative Run is terminal and H10 completion consumption reference is visible.
- [ ] Emit interrupt outcome only for an open typed HumanRequest and keep binding lifecycle Interrupted, not Terminal.
- [ ] Map safe application errors to `RUN_ERROR`; do not map disconnect, backpressure or projection lag to Run error.
- [ ] Viewer-policy change causes omission/full snapshot, never retroactive disclosure.
- [ ] Test message chunk restart/dedup, terminal gate, interrupt non-terminal and projection redaction.
- [ ] Run:

```bash
cargo test --test ag_ui_event_projection --test ag_ui_restart_matrix
```

- [ ] Commit:

```bash
git add crates/vestrace-application/src/ag_ui/projection_service.rs \
  crates/vestrace-ag-ui-adapter/src/event_mapper.rs fixtures/ag-ui/events tests
git commit -m "feat(ag-ui): project run lifecycle messages and steps"
```

### Task 8: Add state, activity and allowlisted custom projections

**Files:** implement state builder/patcher/activity/custom schemas and tests.

**Consumes:** H1/H5/H6/H7/H10 viewer-authorized query views.

**Produces:** `STATE_SNAPSHOT`, safe `STATE_DELTA`, activity and custom events.

- [ ] Build closed `AgUiStateProjection` from Run status/version, plan summary, activities, HumanRequests, safe Artifacts, budget and verification.
- [ ] Produce RFC 6902 patch only when base version/hash matches, operations ≤128, bytes ≤256 KiB, paths are allowed and resulting state validates.
- [ ] Otherwise persist and emit a full snapshot.
- [ ] Activities cover planning, research, waits, Artifact processing, sandbox render, verification, reconciliation and budget warnings.
- [ ] Implement exact schemas for the seven allowed `vestrace.*` custom event names.
- [ ] Reject RAW, rawEvent, THINKING, REASONING and arbitrary custom events at compile/mapping/runtime boundaries.
- [ ] Test pointer escaping, prototype-like keys, stale base, oversized patch, forbidden event generation and viewer filtering.
- [ ] Run:

```bash
cargo test --test ag_ui_state_patch
bash scripts/verify-ag-ui-forbidden-events.sh
```

- [ ] Commit:

```bash
git add crates/vestrace-ag-ui-adapter/src/state_patch.rs \
  crates/vestrace-application/src/ag_ui/projection_service.rs schemas/ag-ui fixtures/ag-ui/forbidden \
  tests/ag_ui_state_patch.rs scripts/verify-ag-ui-forbidden-events.sh
git commit -m "feat(ag-ui): add safe state and activity projection"
```

### Task 9: Bind HumanRequests, interrupts, resume and exact approvals

**Files:** create relevant `0089` tables, resume service and tests.

**Consumes:** H7 HumanRequest/response/continuation and H2 approval path.

**Produces:** durable interrupt binding and idempotent resume.

- [ ] `0089` creates interrupt bindings, external interrupt hashes, response schema hashes, optional approval challenge link, consumed response link and resume idempotency rows.
- [ ] Project open HumanRequest to an interrupt with exact schema, reason, expiry and safe metadata.
- [ ] Resume authenticates participant, resolves binding/request, validates schema/status/expiry and calls H7 response command.
- [ ] Repeated identical resume returns existing HumanResponse; changed payload/status conflicts.
- [ ] Approval interrupt calls exact H2 approval command with ApprovalChallenge and operation fingerprint; generic resolved payload is rejected.
- [ ] Cancelled resume follows HumanRequest kind policy and never grants approval.
- [ ] Lost response after H7 commit is reconciled by response binding before retry.
- [ ] Test clarification, choice, review, approval, expired, cancelled, cross-Run replay and restart.
- [ ] Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test ag_ui_interrupt_resume --test ag_ui_approval_boundary
```

- [ ] Commit:

```bash
git add migrations/0089_ag_ui_interrupt_bindings_and_frontend_actions.sql \
  crates/vestrace-application/src/ag_ui/resume_service.rs \
  crates/vestrace-infrastructure/src/postgres/ag_ui/interrupt_repository.rs \
  tests/ag_ui_interrupt_resume.rs tests/ag_ui_approval_boundary.rs
git commit -m "feat(ag-ui): add durable interrupt and resume mapping"
```

### Task 10: Project safe Tool and Artifact activity

**Files:** extend projection mapper, schemas and security tests.

**Consumes:** H4 Tool public views, H6 Artifact safe representations/export policy and H5/H9A remote progress.

**Produces:** safe Tool call/result, Artifact cards and remote activities.

- [ ] Classify each Tool projection as publishable arguments, summary-only or hidden.
- [ ] Never emit credentials, tickets, headers, raw command/environment, undelegated paths or unsafe result bodies.
- [ ] Sensitive Tools use Activity/Custom summary rather than argument/result events.
- [ ] Artifact event contains exact revision reference, classification-safe title/media/status and safe preview/download-command references only.
- [ ] Active HTML/SVG/document content is never embedded; use H6 Preview/RedactedCopy/PageImage.
- [ ] Remote-agent activity exposes safe status and wait/reconciliation state, not A2A frames or remote credentials.
- [ ] Test secret fixtures, destructive Tool, shell-like args, quarantined Artifact, purged Artifact, unsafe SVG and remote Unknown.
- [ ] Run:

```bash
cargo test --test ag_ui_event_projection --test ag_ui_frontend_actions
```

- [ ] Commit:

```bash
git add crates/vestrace-application/src/ag_ui/projection_service.rs \
  crates/vestrace-ag-ui-adapter/src/event_mapper.rs schemas/ag-ui fixtures/ag-ui/events tests
git commit -m "feat(ag-ui): project safe tool and artifact activity"
```

### Task 11: Implement frontend-action catalogue and command boundary

**Files:** complete `0089` action tables, service/repository/schemas/tests.

**Consumes:** H11 ProductCommandFacade and H2 command authorization.

**Produces:** exact client-only and server-command action catalogue.

- [ ] Add frontend action definitions/catalogue revisions and endpoint binding tables to `0089`; no later task edits the migration.
- [ ] Seed no global active actions in migration; bootstrap imports exact definitions through application commands.
- [ ] Validate client declarations by stable ID and revision hash against endpoint catalogue.
- [ ] Client-only actions return typed UI instructions and call no server command.
- [ ] Server-command actions map to the exact H11 operation, require ordinary idempotency/expected version/authentication and pass H2.
- [ ] An emitted AG-UI Tool call is presentation only; execution requires a separate authenticated action request.
- [ ] Reject unknown action, changed schema, hidden capability, stale version and cross-workspace target.
- [ ] Test all initial actions and prove approval/export/cancel cannot bypass application commands.
- [ ] Run:

```bash
cargo test --test ag_ui_frontend_actions
```

- [ ] Commit:

```bash
git add migrations/0089_ag_ui_interrupt_bindings_and_frontend_actions.sql \
  crates/vestrace-domain/src/ag_ui/frontend_action.rs \
  crates/vestrace-application/src/ag_ui/frontend_action_service.rs \
  crates/vestrace-infrastructure/src/postgres/ag_ui/frontend_action_repository.rs \
  schemas/ag-ui/v1/frontend-action.schema.json tests/ag_ui_frontend_actions.rs
git commit -m "feat(ag-ui): add governed frontend actions"
```

### Task 12: Expose authenticated HTTP POST plus durable SSE

**Files:** add H11 route/DTO/SSE integration and contract tests.

**Consumes:** Tasks 3–11 services and H11 authentication/error/idempotency middleware.

**Produces:** `/v1/ag-ui/{endpoint_id}:run` and reconnect stream.

- [ ] POST authenticates before decoding external keys/content and applies endpoint/body/concurrency limits.
- [ ] Reuse H11 LocalTrusted origin/nonce and BearerToken rules; AG-UI grants no new authentication mode.
- [ ] Decode into protocol-neutral envelope, begin intake, materialize required parts, apply Ready and stream projections.
- [ ] Response content type follows pinned AG-UI SSE contract and every event carries the Vestrace extension.
- [ ] SSE `id` is H7 cursor; validate `Last-Event-ID` against explicit cursor.
- [ ] Heartbeats do not advance cursor; slow client disconnect returns last resumable cursor without changing Run.
- [ ] Same run key/input reconnects existing binding; changed input returns idempotency conflict.
- [ ] Map bounded public errors; no stack, SQL, raw body or adapter Debug output.
- [ ] Test HTTP auth, nonce/CORS, limits, duplicate, conflict, interrupt stream, cursor reconnect, slow client and disconnect.
- [ ] Run:

```bash
cargo test -p vestrace-channel-http ag_ui::
cargo test --test ag_ui_http_sse --test ag_ui_reconnect
```

- [ ] Commit:

```bash
git add crates/vestrace-channel-http/src/routes/ag_ui.rs crates/vestrace-channel-http/src/dto/ag_ui.rs \
  crates/vestrace-channel-http/src/sse/ag_ui.rs tests/ag_ui_http_sse.rs tests/ag_ui_reconnect.rs
git commit -m "feat(ag-ui): expose authenticated HTTP SSE gateway"
```

### Task 13: Add TypeScript AG-UI client integration and console workspace

**Files:** create TypeScript integration, console workspace/components/tests/docs.

**Consumes:** Task 12 endpoint and schemas; H11 TypeScript SDK/auth/security.

**Produces:** built-in interactive Agent Workspace and reusable client integration.

- [ ] Pin `@ag-ui/core@0.0.57` exactly and verify package integrity in lockfile.
- [ ] Implement an integration client that preserves external thread/run keys, H7 cursor and event IDs; bearer remains memory-only.
- [ ] Reducer treats snapshots/deltas as projections, validates projection version/hash and requests full snapshot on mismatch.
- [ ] Interrupt form renders only server schema; approval card shows exact challenge/resource/fingerprint/expiry and submits through governed action command.
- [ ] Render text, activities, steps, safe Tool summaries, Artifact cards and allowlisted generative UI components.
- [ ] Never render raw HTML/SVG or arbitrary custom component names.
- [ ] Client-only frontend actions are local; server-command actions call H11 SDK with idempotency/expected version.
- [ ] Persist reconnect cursor only in bounded application session storage policy; never persist bearer token or local console nonce.
- [ ] Add independent deterministic client test and console accessibility/security tests.
- [ ] Run:

```bash
npm --prefix packages/sdk-typescript ci
npm --prefix packages/sdk-typescript test
npm --prefix apps/console ci
npm --prefix apps/console test
cargo test --test ag_ui_console_contract
```

- [ ] Commit:

```bash
git add packages/sdk-typescript apps/console docs/ag-ui*.md tests/ag_ui_console_contract.rs
git commit -m "feat(console): add AG-UI interactive workspace"
```

### Task 14: Persist pin/conformance/release evidence and integrate product profiles

**Files:** create migration `0090`, release/profile/schema integration, scripts/tests.

**Consumes:** H11 schema bundle, release manifest, ProductProfile and Tasks 2/13 evidence.

**Produces:** release-advertised `ProductSurface::AgUiHttpSse` only with valid evidence.

- [ ] `0090` creates protocol pin records, schema bundle bindings, conformance reports and release-evidence rows; all immutable and content-hashed.
- [ ] Add AG-UI input/event/extension/state/action schemas to H11 public schema bundle.
- [ ] Add protocol source revision, schema digest, adapter revision, TypeScript package version and conformance report to release manifest assets/evidence.
- [ ] Add `AgUiHttpSse` to ProductSurface. Personal safe profile may enable loopback console endpoint; Team/Embedded require explicit config.
- [ ] Startup readiness fails when an enabled surface lacks exact pin/schema/evidence or console bundle mismatch.
- [ ] Dependency upgrade script compares schema/event profile and requires security/H10 evidence before creating a new pin revision.
- [ ] Test invalid signature/hash, stale pin, wrong console version, disabled surface and profile-safe defaults.
- [ ] Run:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test ag_ui_pin_boundary --test ag_ui_release_manifest
bash scripts/verify-ag-ui-release-evidence.sh
```

- [ ] Commit:

```bash
git add migrations/0090_ag_ui_schema_pins_conformance_and_release_evidence.sql \
  crates/vestrace-application/src/ag_ui/release_service.rs \
  crates/vestrace-infrastructure/src/postgres/ag_ui/conformance_repository.rs \
  schemas/ag-ui config deploy scripts/verify-ag-ui-release-evidence.sh \
  tests/ag_ui_pin_boundary.rs tests/ag_ui_release_manifest.rs
git commit -m "feat(release): bind AG-UI pin and conformance evidence"
```

### Task 15: Add RLS, final boundary gates and restart-safe vertical acceptance

**Files:** create `0091`, final tests/scripts/CI plan/docs.

**Consumes:** all H11A tasks.

**Produces:** release-ready H11A exit gate.

- [ ] `0091` forces RLS on endpoint/binding/intake/projection/interrupt/action/conformance tables and adds indexes/append-only/immutability guards.
- [ ] Prove external key hashes cannot resolve across workspace/principal/endpoint revisions.
- [ ] Boundary scripts reject AG-UI SDK types outside allowed directories, forbidden events, raw request persistence, Run checkpoint fields and direct repository use by console.
- [ ] Mandatory vertical scenario:

```text
authenticated AG-UI client
→ user message plus document
→ H6 quarantine/inspection
→ one Conversation and one AgentRun
→ RUN_STARTED
→ text/step/activity/state stream
→ typed HumanRequest interrupt
→ exact resume of same Run
→ safe Tool and Artifact projections
→ full server/worker restart
→ reconnect from H7 cursor
→ H10 verified terminal completion
→ RUN_FINISHED success
```

- [ ] Restart points: after binding claim, during document intake, after Run creation before binding acknowledgement, mid-message, after interrupt emission, after HumanResponse commit, during Tool projection, before terminal event persistence.
- [ ] Negative matrix proves all 15 roadmap gates: no duplicate Run, changed-input conflict, no domain mutation from state, no client Tool authority, no generic approval, no instruction override, no H6 bypass, no sensitive Tool disclosure, no reasoning/RAW, no duplicate reconnect events, no disconnect failure, no SDK type escape, no frontend effect bypass, no premature success, no terminal Run on interrupt.
- [ ] Run independent pinned TypeScript client and built-in console against the same deterministic fixture.
- [ ] Generate and verify conformance report/release evidence entirely offline.
- [ ] Add the eventual CI job plan without changing CI during documentation-only work.
- [ ] Final implementation commands:

```bash
bash scripts/generate-ag-ui-schemas.sh
bash scripts/verify-ag-ui-pin.sh
bash scripts/verify-ag-ui-type-boundary.sh
bash scripts/verify-ag-ui-forbidden-events.sh
bash scripts/verify-ag-ui-release-evidence.sh
bash scripts/run-h11a-ag-ui-vertical-slice.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test ag_ui_rls --test ag_ui_vertical_slice --test ag_ui_restart_matrix
npm --prefix packages/sdk-typescript ci
npm --prefix packages/sdk-typescript test
npm --prefix apps/console ci
npm --prefix apps/console test
```

- [ ] Commit:

```bash
git add migrations/0091_ag_ui_rls_indexes_and_cross_resource_guards.sql \
  tests scripts docs schemas crates packages apps config deploy Cargo.toml Cargo.lock
git commit -m "test(release): add H11A AG-UI readiness gates"
```

---

## Migration ownership

```text
0087 Task 4   Endpoints, external-key Run bindings and durable intake
0088 Task 6   Projection snapshots, projected events and durable cursors
0089 Task 9/11 Interrupt bindings, resume idempotency and frontend-action catalogues
0090 Task 14  Protocol pins, schema bindings, conformance and release evidence
0091 Task 15  RLS, indexes, append-only and cross-resource guards
```

`0089` is created in Task 9 and completed in Task 11 before application; the implementation branch must keep it un-applied until Task 11 review. After Task 11 no task edits it. All other migrations have one task owner and are never edited later.

## H11A public interaction flow

```text
POST /v1/ag-ui/{endpoint}:run
→ authenticate workspace/principal
→ decode and bound pinned RunAgentInput
→ hash external thread/run keys
→ begin/reuse AGUIRunBinding + Intake
→ materialize H6 parts
→ Ready → Applied
→ H7 Interaction / HumanResponse command
→ H1 AgentRun
→ H7 PublicEventRecord
→ deterministic AG-UI projection
→ persisted projection event + cursor
→ SSE client
```

Resume:

```text
AG-UI interrupt
→ durable HumanRequest + InterruptBinding
→ stream closes with interrupt outcome
→ authenticated resume entry
→ exact response/approval validation
→ H7/H2 command
→ same AGUIRunBinding and AgentRun
→ new SSE stream from durable cursor
```

## H11A completion definition

1. Exact AG-UI source/package/schema pin is reproducible and release-bound.
2. No AG-UI SDK type enters domain, application, persistence or checkpoints.
3. External run/thread keys are hashed, scoped and idempotent.
4. Rich input cannot start/resume execution before H6 inspection.
5. H7 cursor/public events remain the sole durable stream source.
6. Lifecycle/message/step/state/activity projections are deterministic and deduplicated.
7. Interrupt stream completion is non-terminal for the Run.
8. Resume is exact, idempotent and approval-safe.
9. Client state and Tool definitions create no authority.
10. Frontend effects use ordinary H11/H2 commands.
11. Reasoning, RAW and arbitrary CUSTOM events are impossible in the standard profile.
12. Sensitive Tool/Artifact/remote information is not disclosed.
13. Disconnect/backpressure does not mutate Run state.
14. Console and independent client both pass create/stream/interrupt/resume/reconnect/complete.
15. Full restart creates no duplicate Run, response, Tool projection, message or terminal event.
16. `RUN_FINISHED success` follows H10 verified completion only.
17. Product profiles and release manifest advertise AG-UI only with exact passing evidence.
18. CI acceptance requires no public AG-UI service or permanent credential.

## Explicit non-goals

H11A does not replace H7 or H11, persist AG-UI objects as domain state, accept client-defined backend Tools, expose hidden reasoning, support arbitrary custom events, add WebSocket/binary transport, implement public CopilotKit-specific server behavior, enable unrestricted generative UI, create a second conversation store, add browser SSO/OIDC, permit direct Artifact HTML rendering, make AG-UI mandatory for Embedded deployments, or modify the authoritative Run checkpoint.

## Documentation-only boundary

Creating this plan does not authorize implementation. During documentation-only work, do not create `feat/harness-ag-ui-gateway`, add `@ag-ui/core` or Rust AG-UI crates, create migrations `0087`–`0091`, expose `/v1/ag-ui`, generate schemas, modify the console, enable the product surface, update release manifests, alter CI or execute H11A tests.