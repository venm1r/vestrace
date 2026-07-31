# Vestrace H11A AG-UI Interaction Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add AG-UI dependencies, create migrations, expose endpoints, modify the console, generate schemas, change release assets, update CI or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Add a durable, policy-governed AG-UI interaction gateway that lets the built-in console and independent AG-UI clients create, stream, interrupt, resume, reconnect and complete Vestrace Runs without making AG-UI state, messages, Tools or Run identifiers authoritative.

**Architecture:** H11A is an anti-corruption and projection layer over H1–H11. Inbound AG-UI envelopes are authenticated, bounded and materialized through H6/H7 before existing application commands create or resume an `AgentRun`; outbound AG-UI events are deterministic projections of H7 public events and viewer-authorized state. PostgreSQL stores Vestrace-owned endpoint revisions, external-key bindings, intake state, projection state, interrupt bindings, frontend-action catalogues and release evidence. AG-UI SDK types remain inside one adapter crate and the TypeScript integration. No `RunCheckpointV10` is introduced.

**Tech Stack:** Existing H1–H11 and H9A; Rust Edition 2024; Tokio; Axum/Tower; SQLx/PostgreSQL 17; H6 Artifact intake; H7 public events/SSE cursors; H2 approvals; H10 verification; H11 HTTP authentication, TypeScript SDK, console, schema bundle and release tooling; JSON Patch RFC 6902; exact AG-UI source revision `bb1c2afddb4880309879b9564cfb3a635a5da4eb`; exact TypeScript `@ag-ui/core` `0.0.57`; optional community Rust conformance crates `ag-ui-core`/`ag-ui-client` `0.1.0`; deterministic offline fixtures.

## Global Constraints

- ADR-0004 and `2026-07-31-vestrace-h11a-ag-ui-roadmap-amendment.md` are normative.
- Complete H1–H11, H9A and the binding ADR-0002 outcome before implementation.
- The implementation pin is exactly AG-UI revision `bb1c2afddb4880309879b9564cfb3a635a5da4eb` until an explicit upgrade passes schema diff, security review, adapter conformance and H10 regression evidence.
- `@ag-ui/core` is exactly `0.0.57`; community Rust crates are optional conformance dependencies only.
- AG-UI SDK/schema types are confined to `vestrace-ag-ui-adapter`, the TypeScript integration, console workspace and wire tests.
- H1–H11 remain authoritative for Runs, conversations, Tools, Artifacts, approvals, policy, budgets, credentials and completion.
- AG-UI `threadId`, `runId` and `parentRunId` are untrusted external correlation keys, never Vestrace IDs.
- H7 `PublicEventRecord` and workspace cursor remain the only durable event-stream source.
- No AG-UI payload or state is stored in a Run checkpoint.
- `RUN_FINISHED success` requires authoritative terminal Run state after H10 one-use completion consumption.
- `RUN_FINISHED interrupt` ends one interaction stream but does not terminate the underlying Run.
- Disconnect/backpressure is not `RUN_ERROR` and never changes Run state.
- Inbound state, transcript, context, Tools, metadata, URLs and binary parts are untrusted.
- Inbound developer/system/assistant/Tool/reasoning messages never become trusted runtime instructions.
- Client Tool definitions may reference only exact server-published frontend actions and cannot create H4 Tools/capabilities.
- State snapshots/deltas are UI projections. JSON Patch never applies to domain aggregates.
- Rich input passes H6 intake, quarantine and inspection before execution starts or resumes.
- Frontend effects use ordinary H11 commands with authentication, idempotency, expected version and H2 authorization.
- Generic interrupt resolution, free-form “yes” or boolean approval cannot create an ApprovalGrant.
- `RAW`, `rawEvent`, `THINKING_*`, `REASONING_*`, `REASONING_ENCRYPTED_VALUE` and arbitrary `CUSTOM` events are prohibited.
- Only exact schema-pinned `vestrace.*` custom events are allowed.
- Secrets, tickets, internal paths, hidden reasoning, unsafe Tool arguments/results and active Artifact content never enter AG-UI payloads.
- SSE `id` is the H7 cursor; `Last-Event-ID` and explicit cursor may not disagree.
- Delivery is at-least-once; clients deduplicate by Vestrace event ID plus deterministic projection key.
- Initial transport is authenticated HTTP POST returning SSE. WebSocket/binary are deferred.
- Personal may enable loopback AG-UI for its console; Team/Embedded require explicit configuration.
- H11 migrations `0081`–`0086` are never edited. H11A owns `0087`–`0092`, one migration per task owner.
- Future implementation branch: `feat/harness-ag-ui-gateway`.

---

## Locked file structure

```text
crates/vestrace-domain/src/ag_ui/
  mod.rs endpoint.rs binding.rs intake.rs projection.rs interrupt.rs frontend_action.rs release.rs
crates/vestrace-application/src/ag_ui/
  mod.rs ports.rs input_service.rs projection_service.rs resume_service.rs
  frontend_action_service.rs release_service.rs
crates/vestrace-ag-ui-adapter/src/
  lib.rs pin.rs decode.rs encode.rs input_mapper.rs event_mapper.rs state_patch.rs bounds.rs
  wire/{mod,input,event,error,extension}.rs
crates/vestrace-channel-http/src/routes/ag_ui.rs
crates/vestrace-channel-http/src/dto/ag_ui.rs
crates/vestrace-channel-http/src/sse/ag_ui.rs
crates/vestrace-infrastructure/src/postgres/ag_ui/
  endpoint_repository.rs binding_repository.rs intake_repository.rs projection_repository.rs
  interrupt_repository.rs frontend_action_repository.rs conformance_repository.rs
packages/sdk-typescript/src/ag-ui/
apps/console/src/ag-ui/
apps/console/src/components/ag-ui/
schemas/ag-ui/v1/
fixtures/ag-ui/

migrations/
  0087_ag_ui_endpoints_run_bindings_and_intakes.sql
  0088_ag_ui_projection_snapshots_events_and_cursors.sql
  0089_ag_ui_interrupt_bindings_and_resume_idempotency.sql
  0090_ag_ui_frontend_action_catalogues.sql
  0091_ag_ui_schema_pins_conformance_and_release_evidence.sql
  0092_ag_ui_rls_indexes_and_cross_resource_guards.sql

tests/
  ag_ui_pin_boundary.rs ag_ui_input_contract.rs ag_ui_binding_idempotency.rs
  ag_ui_multimedia_intake.rs ag_ui_event_projection.rs ag_ui_state_patch.rs
  ag_ui_interrupt_resume.rs ag_ui_approval_boundary.rs ag_ui_frontend_actions.rs
  ag_ui_http_sse.rs ag_ui_reconnect.rs ag_ui_console_contract.rs
  ag_ui_release_manifest.rs ag_ui_rls.rs ag_ui_vertical_slice.rs ag_ui_restart_matrix.rs

scripts/
  generate-ag-ui-schemas.sh verify-ag-ui-pin.sh verify-ag-ui-type-boundary.sh
  verify-ag-ui-forbidden-events.sh verify-ag-ui-release-evidence.sh
  run-h11a-ag-ui-vertical-slice.sh
```

## Normative contracts

### Protocol pin

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

### Endpoint and binding

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

pub enum AgUiRunBindingLifecycle { IntakePending, Bound, Interrupted, Streaming, Terminal, Rejected }

pub struct AgUiRunBinding {
    pub id: AgUiRunBindingId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub endpoint_revision_id: AgUiEndpointRevisionId,
    pub external_thread_key_hash: [u8; 32],
    pub external_run_key_hash: [u8; 32],
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

Uniqueness is `(workspace, principal, endpoint_revision, external_run_key_hash)`. Same key/input returns the existing binding; changed canonical input conflicts.

### Durable intake

```rust
pub enum AgUiIntakeStatus { Received, Validating, Materializing, Ready, Applied, Rejected, Expired }
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

`Ready → Applied` binds the result of an existing H7/H1 command atomically. Required uninspected parts prevent Run work from being scheduled.

### Protocol-neutral input

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

Standard mode accepts only user messages. Compatibility imports are explicitly untrusted and cannot become developer/system/assistant/Tool/reasoning authority.

### Projection

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
    RunStarted, RunFinishedSuccess, RunFinishedInterrupt, RunError,
    StepStarted, StepFinished,
    TextMessageStart, TextMessageContent, TextMessageEnd,
    ToolCallStart, ToolCallArgs, ToolCallEnd, ToolCallResult,
    StateSnapshot, StateDelta, MessagesSnapshot,
    ActivitySnapshot, ActivityDelta, Custom,
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

Uniqueness is `(binding_id, public_event_id, projection_key)`. One H7 event may yield multiple ordered events with deterministic keys.

### State projection and patch

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

Allowed pointer prefixes: `/run/status`, `/run/version`, `/plan`, `/activity`, `/humanRequests`, `/artifacts`, `/budget`, `/verification`. Maximum 128 operations and 256 KiB. Invalid/stale patches fall back to a full snapshot.

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

Each entry binds to one open HumanRequest. Identical replay returns the existing response; changed payload conflicts. Approval uses the exact H7/H2 path.

### Frontend actions

```rust
pub enum AgUiFrontendActionAuthority { ClientOnly, ServerCommand }
pub enum AgUiFrontendActionRisk { Navigation, ReadOnly, UserInput, ProtectedMutation }

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

Initial client-only stable IDs:

```text
vestrace.ui.navigate.run
vestrace.ui.navigate.artifact
vestrace.ui.focus.human-request
vestrace.ui.copy.reference
vestrace.ui.expand.activity
```

Initial server-command stable IDs:

```text
vestrace.command.submit-human-response
vestrace.command.grant-approval
vestrace.command.cancel-run
vestrace.command.resume-run
vestrace.command.create-download-grant
vestrace.command.export-artifact
```

An emitted Tool-call is presentation only; server effects require a separate authenticated H11 command.

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

`ProductSurface::AgUiHttpSse` may be advertised only with valid exact evidence.

---

### Task 1: Add protocol-neutral domain contracts

**Files:** domain `ag_ui` modules and ID exports.

**Produces:** all H11A domain values above.

- [ ] Write transition tests for endpoint, binding and intake lifecycles.
- [ ] Write golden canonical-hash tests for endpoint, binding identity, intake, projection, state, interrupt, action and conformance report.
- [ ] Write negative serialization tests proving no clear external key, SDK type, secret, raw event or checkpoint field exists.
- [ ] Implement IDs and contracts exactly as specified.
- [ ] Run `cargo test -p vestrace-domain ag_ui::`.
- [ ] Commit `feat(ag-ui): add interaction gateway contracts`.

### Task 2: Pin AG-UI and isolate wire types

**Files:** `vestrace-ag-ui-adapter`, pin/schema scripts and golden fixtures.

**Produces:** `decode_run_agent_input`, `encode_projected_event`, exact pin metadata and cross-language vectors.

- [ ] Add no AG-UI dependency outside the adapter/TypeScript integration.
- [ ] Record exact source/package versions and deterministic schema hashes.
- [ ] Implement pinned wire DTOs for input, messages, parts, Tools, interrupts/resume and enabled events.
- [ ] Decoder rejects prohibited roles, unknown frontend actions, non-`vestrace.*` forwarded properties and bound violations.
- [ ] Encoder has no constructors for RAW/reasoning and rejects arbitrary custom names.
- [ ] Validate vectors with pinned TypeScript and optional community Rust crates.
- [ ] Run adapter tests plus pin/type-boundary scripts.
- [ ] Commit `feat(ag-ui): pin protocol and isolate wire types`.

### Task 3: Add application ports and intake orchestration

**Files:** application `ag_ui` modules and deterministic fixtures.

**Produces:** repository ports, H6 materialization port, H7/H11 command port and `AgUiInputService`.

- [ ] Define endpoint/binding/intake/projection/interrupt/action/conformance repository ports.
- [ ] Define `AgUiInputMaterializationPort` and `AgUiInteractionCommandPort` using Vestrace DTOs only.
- [ ] Implement begin, part recording, reject and `apply_ready` flows.
- [ ] Hash external keys immediately; return New/ExistingSameInput/Conflict.
- [ ] Ensure `apply_ready` creates/resumes H7/H1 state once and binds returned IDs.
- [ ] Add crash fixtures before/after command commit.
- [ ] Run `cargo test -p vestrace-application ag_ui::`.
- [ ] Commit `feat(ag-ui): add intake and projection application ports`.

### Task 4: Persist endpoints, bindings and intake

**Files:** migration `0087`, repositories and tests.

- [ ] Create endpoint identities/revisions, external-key bindings, intake records, message/context hashes, part references and H6/H7/H1 links.
- [ ] Persist keyed hashes only; never raw request or clear external keys.
- [ ] Enforce scoped uniqueness and transactional New/Same/Conflict behavior.
- [ ] Recover Received/Validating/Materializing/Ready records without duplicate Runs.
- [ ] Test concurrent duplicate, changed input, expiry and both crash boundaries.
- [ ] Run `ag_ui_binding_idempotency` and `ag_ui_input_contract` integration tests.
- [ ] Commit `feat(ag-ui): persist endpoints bindings and intake`.

### Task 5: Route rich input through H6

**Files:** input materializer, multimedia fixtures and tests.

- [ ] Stream inline media/documents/binary into H6 without whole-body buffering.
- [ ] Route URL parts through H6 secure fetch with SSRF/DNS/redirect/MIME/size/time controls.
- [ ] Keep intake Materializing until required inspections pass.
- [ ] Reuse same source hash binding without duplicate ingestion.
- [ ] Test document success, malware, oversized data, private IP, DNS rebinding, lost response and restart.
- [ ] Run `cargo test --test ag_ui_multimedia_intake`.
- [ ] Commit `feat(ag-ui): route rich input through artifact intake`.

### Task 6: Persist deterministic projections and cursors

**Files:** migration `0088`, projection repository and tests.

- [ ] Create state snapshots, projected events, H7 links, projection keys, cursor checkpoints and stream observations.
- [ ] Enforce `(binding, public_event, projection_key)` uniqueness.
- [ ] Advance cursor only after all projections for one H7 event persist.
- [ ] Treat changed output for the same pin/input as conformance conflict.
- [ ] Test restart mid-batch, duplicate H7 delivery, cursor disagreement and isolation.
- [ ] Run `ag_ui_event_projection` and `ag_ui_reconnect` tests.
- [ ] Commit `feat(ag-ui): persist durable interaction projections`.

### Task 7: Project lifecycle, messages and steps

**Files:** projection service and adapter mapper.

- [ ] Emit one RUN_STARTED after durable binding.
- [ ] Map assistant streaming into stable text start/content/end keys.
- [ ] Map viewer-visible step start/finish without private plan context.
- [ ] Gate success on terminal Run plus H10 completion reference.
- [ ] Emit interrupt only for open HumanRequest and leave Run non-terminal.
- [ ] Never map disconnect/backpressure/projection lag to RUN_ERROR.
- [ ] Test chunk restart/dedup, terminal gate, interrupt semantics and redaction.
- [ ] Commit `feat(ag-ui): project run lifecycle messages and steps`.

### Task 8: Project state, activity and allowed custom events

**Files:** state patcher, schemas and forbidden-event tests.

- [ ] Build closed viewer-authorized state projection.
- [ ] Emit patch only for exact base version/hash, allowed paths, ≤128 ops, ≤256 KiB and valid result; otherwise snapshot.
- [ ] Add planning/research/wait/artifact/sandbox/verification/reconciliation/budget activities.
- [ ] Add exact schemas for approved `vestrace.*` events.
- [ ] Make RAW/reasoning/arbitrary custom generation impossible at compile and runtime boundaries.
- [ ] Test pointer abuse, stale base, oversized patch and viewer filtering.
- [ ] Run state tests and forbidden-event script.
- [ ] Commit `feat(ag-ui): add safe state and activity projection`.

### Task 9: Persist interrupts and idempotent resume

**Files:** migration `0089`, resume service/repository and approval tests.

- [ ] Create interrupt bindings, external interrupt hashes, schema/challenge links, consumed response links and resume idempotency rows.
- [ ] Validate participant, request, schema, status and expiry before H7 response.
- [ ] Return existing response for identical replay; conflict on changed payload/status.
- [ ] Route approval through exact H7/H2 challenge/fingerprint path.
- [ ] Reconcile lost response after H7 commit before retry.
- [ ] Test clarification, choice, review, approval, expiry, cancellation, cross-Run replay and restart.
- [ ] Commit `feat(ag-ui): add durable interrupt and resume mapping`.

### Task 10: Project safe Tool, Artifact and remote activity

**Files:** projection mapper, schemas and security fixtures.

- [ ] Classify Tool views as publishable-args, summary-only or hidden.
- [ ] Exclude credentials, tickets, headers, environment, internal paths and unsafe results.
- [ ] Use activity/custom summary for sensitive Tools.
- [ ] Emit exact Artifact revision plus safe preview/download-command references only.
- [ ] Never embed active HTML/SVG/document content.
- [ ] Expose safe remote status without A2A frames/credentials.
- [ ] Test secret fixtures, destructive Tool, shell-like args, quarantined/purged Artifact, SVG and remote Unknown.
- [ ] Commit `feat(ag-ui): project safe tool and artifact activity`.

### Task 11: Persist governed frontend actions

**Files:** migration `0090`, action service/repository/schema/tests.

- [ ] Create immutable action definitions/catalogues and endpoint bindings; migration seeds no active authority.
- [ ] Import exact definitions through application commands.
- [ ] Validate stable ID and revision hash against endpoint catalogue.
- [ ] Client-only actions call no server service.
- [ ] Server-command actions call exact H11 operations with normal idempotency/version/auth/H2 checks.
- [ ] Require a separate authenticated action request; emitted Tool events remain presentation-only.
- [ ] Test all initial actions, stale schema/version, hidden capability and cross-workspace target.
- [ ] Commit `feat(ag-ui): add governed frontend actions`.

### Task 12: Expose authenticated POST + durable SSE

**Files:** H11 HTTP route/DTO/SSE integration and contract tests.

- [ ] Authenticate before decoding keys/content; reuse H11 bearer/local nonce/CORS rules.
- [ ] Decode to neutral envelope, begin intake, materialize parts, apply Ready and stream projections.
- [ ] Include Vestrace extension on every event.
- [ ] Use H7 cursor as SSE ID; reject cursor disagreement.
- [ ] Heartbeats do not advance cursor; slow-client disconnect returns resumable cursor without Run mutation.
- [ ] Same key/input reconnects; changed input conflicts.
- [ ] Return bounded errors without stack/SQL/raw body/adapter Debug.
- [ ] Test auth, limits, duplicate, interrupt stream, reconnect, slow client and disconnect.
- [ ] Commit `feat(ag-ui): expose authenticated HTTP SSE gateway`.

### Task 13: Add TypeScript client and console workspace

**Files:** TypeScript integration, console components, docs and tests.

- [ ] Pin `@ag-ui/core@0.0.57` exactly.
- [ ] Preserve external keys, H7 cursor and event IDs; bearer remains memory-only.
- [ ] Reducer validates projection version/hash and requests snapshot on mismatch.
- [ ] Render schema-driven interrupt forms and exact approval cards.
- [ ] Render text, activities, safe Tool summaries, Artifact cards and allowlisted generative UI.
- [ ] Never render raw HTML/SVG or arbitrary component names.
- [ ] Client-only actions remain local; server commands use H11 SDK.
- [ ] Persist cursor only under bounded session policy; never persist bearer or console nonce.
- [ ] Test independent client, console security and accessibility.
- [ ] Commit `feat(console): add AG-UI interactive workspace`.

### Task 14: Bind schemas, pin and release evidence

**Files:** migration `0091`, H11 schema/profile/release integration and scripts.

- [ ] Create immutable protocol pins, schema bindings, conformance reports and release evidence.
- [ ] Add AG-UI schemas to the H11 public schema bundle.
- [ ] Add source pin, schema digest, adapter revision, TS version and report to release evidence.
- [ ] Add `AgUiHttpSse` product surface; Personal loopback may enable, Team/Embedded opt in.
- [ ] Fail readiness for enabled surface with missing/mismatched evidence.
- [ ] Require schema/security/H10 evidence for upgrades.
- [ ] Test stale pin, hash/signature mismatch, console mismatch and profile defaults.
- [ ] Commit `feat(release): bind AG-UI pin and conformance evidence`.

### Task 15: Add RLS and final restart-safe acceptance

**Files:** migration `0092`, final tests/scripts/docs and future CI job definition.

- [ ] Force RLS on all H11A tables; add indexes and append-only/immutability guards.
- [ ] Prove external hashes cannot resolve across workspace/principal/endpoint.
- [ ] Boundary scripts reject SDK-type escape, forbidden events, raw request persistence, checkpoint fields and console repository access.
- [ ] Run the mandatory flow:

```text
authenticated AG-UI client
→ user message + inspected document
→ one Conversation + one AgentRun
→ lifecycle/text/step/activity/state stream
→ typed interrupt
→ exact resume of same Run
→ safe Tool/Artifact projections
→ full runtime restart
→ H7 cursor reconnect
→ H10 verified completion
→ RUN_FINISHED success
```

- [ ] Restart after binding claim, during intake, after Run creation before binding acknowledgement, mid-message, after interrupt, after HumanResponse, during Tool projection and before terminal persistence.
- [ ] Prove all 15 negative gates from the roadmap amendment.
- [ ] Run the built-in console and independent pinned client against the same fixture.
- [ ] Generate conformance/release evidence offline.
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

- [ ] Commit `test(release): add H11A AG-UI readiness gates`.

---

## Migration ownership

```text
0087 Task 4   Endpoints, scoped external-key bindings and durable intake
0088 Task 6   Projection snapshots, projected events and durable cursors
0089 Task 9   Interrupt bindings and resume idempotency
0090 Task 11  Frontend-action definitions, catalogues and endpoint bindings
0091 Task 14  Protocol pins, schema bindings, conformance and release evidence
0092 Task 15  RLS, indexes, append-only and cross-resource guards
```

Each migration has exactly one owner and is never edited by a later task.

## Authoritative flow

```text
POST /v1/ag-ui/{endpoint}:run
→ authenticate workspace/principal
→ decode and bound pinned input
→ hash external keys
→ begin/reuse binding + intake
→ H6 materialization/inspection
→ Ready → Applied
→ H7 Interaction or HumanResponse command
→ H1 AgentRun
→ H7 PublicEventRecord
→ deterministic persisted projection
→ SSE
```

Resume:

```text
HumanRequest
→ interrupt binding + RUN_FINISHED interrupt
→ authenticated resume
→ exact response/approval validation
→ H7/H2 command
→ same binding and Run
→ new stream from durable cursor
```

## Completion definition

1. Exact source/package/schema pin is reproducible and release-bound.
2. AG-UI SDK types do not enter domain/application/persistence/checkpoints.
3. External keys are hashed, scoped and idempotent.
4. Rich input cannot execute before H6 inspection.
5. H7 remains the sole durable stream source.
6. Projections are deterministic and deduplicated.
7. Interrupt stream completion is non-terminal for the Run.
8. Resume is exact, idempotent and approval-safe.
9. Client state/Tools create no authority.
10. Frontend effects use H11/H2 commands.
11. Reasoning, RAW and arbitrary CUSTOM are impossible.
12. Sensitive Tool/Artifact/remote information is not disclosed.
13. Disconnect/backpressure does not mutate Run state.
14. Console and independent client pass create/stream/interrupt/resume/reconnect/complete.
15. Full restart creates no duplicate Run, response, message, Tool projection or terminal event.
16. Success follows H10 verified completion only.
17. Profiles/releases advertise AG-UI only with passing exact evidence.
18. CI needs no public AG-UI service or permanent credential.

## Explicit non-goals

H11A does not replace H7/H11, persist AG-UI objects as domain state, accept client-defined backend Tools, expose hidden reasoning, allow arbitrary custom events, add WebSocket/binary transport, implement unrestricted generative UI, create another conversation store, add OIDC, render active Artifact content, make AG-UI mandatory for Embedded or modify the Run checkpoint.

## Documentation-only boundary

Creating this plan does not authorize implementation. Do not create `feat/harness-ag-ui-gateway`, add AG-UI packages, create migrations `0087`–`0092`, expose `/v1/ag-ui`, generate schemas, modify the console, enable the product surface, update release assets/CI or execute H11A tests during the documentation-only phase.