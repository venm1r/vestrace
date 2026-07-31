# Vestrace H7 Conversations, Channels and Triggers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start channel or scheduler processes, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement durable conversations and interaction routing, typed human continuations, channel-independent public events and notifications, HTTP/SSE/CLI adapters, immutable trigger revisions, manual/API/schedule/run-continuation triggers, bounded autonomy, durable Run proposals and transactional storm protection.

**Architecture:** H7 separates communication, execution and activation. `Conversation` and append-only `InteractionEvent` records preserve communication history; `AgentRun` remains the sole executable authority; `TriggerDefinitionRevision` describes a governed source of observations, proposals, new Runs or exact continuations. Typed `HumanRequest` records are durable continuation points whose responses must satisfy exact schema, participant and H2 policy checks before they affect a Run, approval, remote invocation or Artifact review. All channels call one application core and consume one durable public-event stream; SSE, CLI and notifications are projections rather than sources of truth.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H6 Rust workspace; Rust Edition 2024; Tokio; Axum; Tower; Serde; Schemars; SQLx; PostgreSQL 17; Server-Sent Events; Clap; SHA-256 canonical hashing; JSON Schema validation; RFC 5545 RRULE subset with IANA time-zone data; deterministic clocks, schedulers and channel fixtures; proptest; tracing.

## Global Constraints

- Complete all five v0.1 plans and H1–H6 before implementing H7.
- Harness design sections `15. Triggers and autonomy` and `17. Conversations, interactions and channels` are normative.
- `Conversation` stores communication and `AgentRun` stores execution. They are connected only by explicit links.
- PostgreSQL is authoritative for conversations, participants, interactions, Run links, human requests/responses, continuation records, public events/cursors, notification intents/deliveries, trigger identities/revisions, schedule state, occurrences, evaluations, Run proposals, deduplication, counters and causal chains.
- Vestrace owns every domain type, application port, persisted schema, lifecycle transition, event kind and public DTO.
- Axum, Clap, SSE, WebSocket, MCP, A2A, provider, connector and notification SDK types may not appear in domain/application signatures or PostgreSQL schemas.
- Channel adapters authenticate a principal and normalize input; they never assign Run roles, approve actions, increase budgets, choose policies or mutate Run state directly.
- Conversation membership does not grant `run.cancel`, `approval.grant`, `artifact.export`, `budget.increase`, `trigger.manage` or any other capability.
- Run participation roles are explicit and checked together with base capability and H2 policy. `Approver` is eligibility to submit a decision, not an `ApprovalGrant`.
- Interaction content, channel metadata, external IDs, remote progress and trigger payloads are untrusted data.
- Interaction events are append-only. Corrections, retractions and redactions create linked events instead of changing history.
- Inline interaction content is bounded. Binary/large attachments use exact H6 `ArtifactRevisionId` references.
- Quarantined, rejected, deleted or purge-pending Artifacts may be shown as communication history but cannot enter Run context, tools or remote continuations.
- Human requests are typed. Free-form text cannot satisfy approval, authentication, review or manual-action requests without validating against the exact request.
- A continuation token is an opaque one-time correlation proof stored only as a hash. It does not authenticate or authorize the responder.
- Approval responses call H2 with the exact operation fingerprint, action, resource, constraints and approver identity. H7 never creates approval from words such as “yes”.
- Authentication responses contain only status and opaque authorization-flow references. Passwords, codes, tokens and credentials never enter H7 payloads.
- Remote continuation contracts are Vestrace-owned. No A2A Task, Agent Card or `a2a-rs` type appears in H7.
- Public event delivery is at-least-once. Consumers deduplicate by immutable event ID/cursor.
- Streaming, CLI rendering and notifications are projections. Their failure cannot alter Run, HumanRequest, Trigger or Proposal state.
- Public cursors are opaque, workspace-bound and monotonically ordered within one workspace.
- Notifications use canonical templates and bounded safe summaries; they exclude raw prompts, secrets, backend keys, hidden reasoning and unrestricted external content.
- Trigger identities have mutable lifecycle state; trigger revisions are immutable. Enabling, disabling, suspension and revocation use expected state revision.
- Required first-slice trigger kinds are `Manual`, `Api`, `Schedule` and `RunContinuation`.
- `ExternalEvent`, `StateChange` and `Condition` have stable contracts but remain inactive without a compatible H9 extension.
- Filters and objective mappings use a bounded declarative DSL. Arbitrary code, SQL, unbounded regex and executable templates are forbidden.
- Trigger execution cannot widen the selected `AgentRuntimeSnapshot`, H2 policy, data classifications, tool/delegation ceilings or budgets.
- Standard autonomy levels are `Observe`, `Suggest`, `Prepare` and `Execute`. `Commit` exists for forward compatibility but is rejected in standard v0.2.
- `Observe` can start only a read-only observation Run. `Suggest` creates a `RunProposal` and executes no proposed work. `Prepare` may reach safe preparation/preview but cannot commit. `Execute` permits only policy-approved bounded reversible/idempotent actions and cannot infer destructive, irreversible or external-commitment authority.
- Every trigger occurrence is idempotent. Ambiguous source delivery is reconciled by source occurrence ID/hash and never blindly creates another Run.
- Storm protection includes deduplication, cooldown, occurrence/Run windows, concurrent Run ceiling, H2 budget/quota checks, failure suspension and causal-loop prevention.
- `RunContinuation` never creates a second Run; it enqueues continuation of the exact existing Run after validating a durable cause.
- Schedule behavior is deterministic for the trigger revision, time-zone database revision and clock instant. DST ambiguity, nonexistent local time and catch-up behavior are explicit.
- Scheduler leases and heartbeats are operational state and do not increment `RunVersion` or trigger revision.
- Existing migrations `0014`–`0047` are never edited. H7 migrations are `0048`–`0053` and are created once.
- CI uses deterministic clocks, local HTTP/SSE fixtures and fake notification/remote-continuation ports. No public model, external channel, A2A server, OAuth provider or permanent credential is required.
- Future implementation branch: `feat/h7-conversations-channels-triggers`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  conversation/{mod,conversation,participant,interaction,content,run_link,routing}.rs
  human/{mod,request,response,continuation}.rs
  channel/{mod,event,cursor,notification,preference}.rs
  trigger/{mod,definition,source,filter,template,autonomy,occurrence,proposal,schedule,causality}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  conversation/{mod,ports,commands,service,routing}.rs
  human/{mod,ports,commands,request_service,response_service,approval,remote_continuation}.rs
  channel/{mod,ports,projection,event_stream,notification,rendering}.rs
  trigger/{mod,ports,commands,registry,evaluator,manual_api,scheduler,continuation,autonomy,proposal,storm}.rs

crates/vestrace-channel-http/src/{lib,routes,dto,errors,sse}.rs
crates/vestrace-channel-cli/src/{lib,commands,render,watch}.rs
crates/vestrace-channel-test-support/src/{lib,clock,events,notifications,remote,fixtures}.rs

crates/vestrace-infrastructure/src/postgres/
  conversation/{mod,repository,interaction_repository,participant_repository,run_link_repository}.rs
  human/{mod,request_repository,response_repository,continuation_repository}.rs
  channel/{mod,event_repository,notification_repository,preference_repository}.rs
  trigger/{mod,definition_repository,occurrence_repository,schedule_repository,proposal_repository,storm_repository}.rs

migrations/
  0048_conversations_participants_interactions_run_links.sql
  0049_human_requests_responses_and_continuations.sql
  0050_public_events_notifications_and_preferences.sql
  0051_trigger_definitions_revisions_and_schedules.sql
  0052_trigger_occurrences_proposals_dedup_and_causality.sql
  0053_conversation_trigger_rls_indexes_and_run_bindings.sql

tests/
  conversation_persistence.rs
  interaction_idempotency.rs
  conversation_run_routing.rs
  human_request_persistence.rs
  human_response_atomicity.rs
  approval_response_integration.rs
  remote_continuation_restart.rs
  public_event_cursor.rs
  sse_reconnect.rs
  notification_persistence.rs
  trigger_registry_persistence.rs
  trigger_manual_api.rs
  trigger_schedule_dst.rs
  trigger_run_continuation.rs
  trigger_storm_protection.rs
  run_proposal_persistence.rs
  conversation_trigger_rls.rs
  h7_acceptance.rs

scripts/
  verify-conversation-boundary.sh
  verify-human-continuation-boundary.sh
  verify-trigger-autonomy-boundary.sh
```

---

## Normative contracts

### Supporting opaque references and command values

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(transparent)]
pub struct OpaqueAuthorizationFlowReference(String);

#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(transparent)]
pub struct OpaqueConnectionReference(String);

pub enum TypedRunCommand {
    Pause { reason_code: String },
    Resume,
    Cancel { reason_code: String },
    AttachArtifact { revision_id: ArtifactRevisionId },
}

pub struct NotificationDeliveryObservation {
    pub delivery_id: NotificationDeliveryId,
    pub status: NotificationStatus,
    pub external_receipt_hash: Option<[u8; 32]>,
    pub completion_may_have_occurred: bool,
    pub observed_at: Timestamp,
}
```

Opaque references are bounded, redacted in Debug and reject credential-like payloads. Budget increases and approval grants are separate H2 commands and are not generic `TypedRunCommand` variants.

### ID ownership

Task 1 adds `ConversationId`, `ConversationRunLinkId`, `RunParticipantAssignmentId`, `InteractionEventId` and `InteractionRoutingDecisionId`. Task 2 adds `HumanRequestId`, `HumanResponseId` and `HumanContinuationRecordId`. Task 3 adds `TriggerDefinitionId`, `TriggerDefinitionRevisionId`, `TriggerOccurrenceId`, `TriggerEvaluationDecisionId`, `CausalChainId` and `RunProposalId`. Task 4 adds `PublicEventId`, `NotificationPreferenceId`, `NotificationPreferenceRevisionId`, `NotificationIntentId` and `NotificationDeliveryId`.

### Conversation, participants and Run links

```rust
pub enum ConversationStatus { Active, Archived, Closed }
pub enum ConversationVisibility { Private, Restricted, Workspace }

pub struct Conversation {
    pub id: ConversationId,
    pub workspace_id: WorkspaceId,
    pub title: Option<String>,
    pub status: ConversationStatus,
    pub visibility: ConversationVisibility,
    pub created_by: PrincipalId,
    pub revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub enum ConversationParticipantRole { Owner, Member, Guest, Agent, Observer }

pub struct ConversationParticipant {
    pub conversation_id: ConversationId,
    pub actor: InteractionActorRef,
    pub role: ConversationParticipantRole,
    pub joined_at: Timestamp,
    pub left_at: Option<Timestamp>,
}

pub enum RunParticipantRole { Owner, Requester, Approver, Contributor, Observer }

pub enum ConversationRunRelation { Origin, Continuation, Discussion, Notification }

pub struct ConversationRunLink {
    pub id: ConversationRunLinkId,
    pub conversation_id: ConversationId,
    pub run_id: AgentRunId,
    pub linked_by: PrincipalId,
    pub relation: ConversationRunRelation,
    pub created_at: Timestamp,
}

pub struct RunParticipantAssignment {
    pub id: RunParticipantAssignmentId,
    pub run_id: AgentRunId,
    pub principal_id: PrincipalId,
    pub role: RunParticipantRole,
    pub granted_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub valid_until: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
}
```

Conversation roles affect visibility/presentation only. Every privileged Run command still requires assignment eligibility, base capability and H2 policy.

### Interaction actors, content and routing

```rust
pub enum InteractionActorRef {
    Principal(PrincipalId),
    AgentSnapshot(AgentRuntimeSnapshotId),
    System,
    RemoteExternal {
        identity_hash: [u8; 32],
        remote_agent_revision_id: Option<RemoteAgentDefinitionRevisionId>,
    },
}

pub enum InteractionChannelKind {
    Http,
    Sse,
    Cli,
    Mcp,
    Extension { stable_id: String },
}

pub enum InteractionKind {
    UserMessage,
    AgentMessage,
    SystemMessage,
    Clarification,
    ApprovalResponse,
    CancellationRequest,
    RunCommand,
    AttachmentProvided,
    ExternalNotification,
    Feedback,
    Retraction,
    RedactionNotice,
}

pub enum InteractionContentPart {
    Text { text: String },
    Json { value: serde_json::Value },
    ArtifactRevision { revision_id: ArtifactRevisionId },
    HumanResponse { response_id: HumanResponseId },
    RunReference { run_id: AgentRunId },
}

pub struct InteractionExternalIdentifier {
    pub namespace: String,
    pub value_hmac: [u8; 32],
}

pub enum InteractionTrustClass {
    AuthenticatedUser,
    VestraceSystem,
    VestraceAgent,
    VerifiedExternal,
    UntrustedExternal,
}

pub struct InteractionEvent {
    pub id: InteractionEventId,
    pub workspace_id: WorkspaceId,
    pub conversation_id: ConversationId,
    pub actor: InteractionActorRef,
    pub channel: InteractionChannelKind,
    pub kind: InteractionKind,
    pub parts: Vec<InteractionContentPart>,
    pub reply_to: Option<InteractionEventId>,
    pub supersedes: Option<InteractionEventId>,
    pub external_identifiers: Vec<InteractionExternalIdentifier>,
    pub trust: InteractionTrustClass,
    pub idempotency_key: String,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

pub enum InteractionRoutingIntent {
    CreateRun,
    ContinueRun { run_id: AgentRunId },
    SubmitHumanResponse { request_id: HumanRequestId },
    ApplyRunCommand { run_id: AgentRunId, command: TypedRunCommand },
    AttachOnly,
    NoExecutableIntent,
}

pub enum InteractionRoutingDecisionKind { Accepted, ClarificationRequired, Rejected }

pub struct InteractionRoutingDecision {
    pub id: InteractionRoutingDecisionId,
    pub interaction_id: InteractionEventId,
    pub intent: InteractionRoutingIntent,
    pub kind: InteractionRoutingDecisionKind,
    pub target_run_id: Option<AgentRunId>,
    pub reason_code: String,
    pub policy_decision_id: Option<PolicyDecisionId>,
    pub created_at: Timestamp,
}
```

One event has 1–64 parts, each text part is at most 256 KiB and total inline serialization is at most 1 MiB. Explicit Run/request references take precedence. Multiple active Runs without an explicit target produce `ClarificationRequired` rather than selecting one heuristically.

### Human request and response lifecycle

```rust
pub enum HumanRequestKind {
    Clarification,
    MissingData,
    Choice,
    Approval,
    Review,
    AuthenticationRequired,
    ManualAction,
    RemoteInputRequired,
}

pub enum HumanRequestStatus { Open, Answered, Expired, Cancelled, Superseded }

pub enum HumanRequestTarget {
    Run { run_id: AgentRunId },
    RunStep { run_id: AgentRunId, step_id: RunStepId },
    ToolInvocation { invocation_id: ToolInvocationId },
    RemoteAgentInvocation { invocation_id: RemoteAgentInvocationId },
    ArtifactReview { revision_id: ArtifactRevisionId },
}

pub enum HumanBlockingDisposition {
    WaitingForInput,
    WaitingForApproval,
    WaitingForDependency,
    NonBlocking,
}

pub struct HumanChoice {
    pub id: String,
    pub label: String,
    pub value: serde_json::Value,
}

pub struct ApprovalChallengeRef {
    pub policy_decision_id: PolicyDecisionId,
    pub operation_fingerprint: OperationFingerprint,
    pub action: ActionId,
    pub resource: ResourceRef,
    pub argument_constraint_hash: [u8; 32],
}

pub struct AuthenticationChallengeRef {
    pub connection_reference: Option<OpaqueConnectionReference>,
    pub authorization_flow_reference: Option<OpaqueAuthorizationFlowReference>,
    pub required_operation: String,
}

pub struct HumanRequest {
    pub id: HumanRequestId,
    pub workspace_id: WorkspaceId,
    pub target: HumanRequestTarget,
    pub kind: HumanRequestKind,
    pub question: String,
    pub choices: Vec<HumanChoice>,
    pub expected_response_schema: serde_json::Value,
    pub reason_code: String,
    pub blocking: HumanBlockingDisposition,
    pub approval_challenge: Option<ApprovalChallengeRef>,
    pub authentication_challenge: Option<AuthenticationChallengeRef>,
    pub supersedes_request_id: Option<HumanRequestId>,
    pub status: HumanRequestStatus,
    pub request_revision: u64,
    pub expires_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

pub enum HumanApprovalDecision { Approve, Deny }
pub enum HumanReviewDecision { Accept, AcceptWithWarnings, RequestRevision, Reject }
pub enum AuthenticationContinuationStatus { Completed, Cancelled, Failed }

pub enum HumanResponsePayload {
    Clarification { value: serde_json::Value },
    Choice { choice_id: String },
    Approval { decision: HumanApprovalDecision },
    Review {
        decision: HumanReviewDecision,
        comments: Option<String>,
        evidence: Vec<RunReference>,
    },
    Authentication {
        status: AuthenticationContinuationStatus,
        authorization_flow_reference: Option<OpaqueAuthorizationFlowReference>,
    },
    ManualAction { result: serde_json::Value, evidence: Vec<RunReference> },
    RemoteInput { value: serde_json::Value },
}

pub struct HumanResponse {
    pub id: HumanResponseId,
    pub request_id: HumanRequestId,
    pub responder_principal_id: PrincipalId,
    pub channel: InteractionChannelKind,
    pub payload: HumanResponsePayload,
    pub response_hash: [u8; 32],
    pub idempotency_key: String,
    pub submitted_at: Timestamp,
}
```

Payload variant must match request kind and exact JSON Schema. One request accepts at most one terminal response. A correction creates a new request using `supersedes_request_id`.

### Continuation records and disposition

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanContinuationToken(String);

pub enum HumanContinuationStatus { Issued, Consumed, Expired, Revoked }

pub struct HumanContinuationRecord {
    pub id: HumanContinuationRecordId,
    pub request_id: HumanRequestId,
    pub token_hash: [u8; 32],
    pub maximum_uses: u16,
    pub used_count: u16,
    pub expires_at: Timestamp,
    pub status: HumanContinuationStatus,
}

pub enum HumanResponseDisposition {
    ContinueRun { run_id: AgentRunId },
    ApplyApproval { challenge: ApprovalChallengeRef },
    ContinueRemoteInvocation { invocation_id: RemoteAgentInvocationId },
    MarkAuthenticationReady { invocation_id: Option<RemoteAgentInvocationId> },
    CompleteArtifactReview { revision_id: ArtifactRevisionId },
    RecordOnly,
}
```

The clear token is returned once through an authorized channel and never stored/logged. Token verification is a correlation check only; authenticated principal, participant assignment and H2 policy remain mandatory.

### Remote-agent continuation boundary

```rust
pub struct RemoteAgentInputContinuation {
    pub invocation_id: RemoteAgentInvocationId,
    pub request_id: HumanRequestId,
    pub normalized_input: serde_json::Value,
    pub response_references: Vec<RunReference>,
    pub idempotency_key: String,
}

pub struct RemoteAgentAuthenticationContinuation {
    pub invocation_id: RemoteAgentInvocationId,
    pub request_id: HumanRequestId,
    pub authorization_flow_reference: OpaqueAuthorizationFlowReference,
    pub idempotency_key: String,
}

#[async_trait::async_trait]
pub trait RemoteAgentContinuationPort: Send + Sync {
    async fn submit_input(
        &self,
        context: &RequestContext,
        continuation: RemoteAgentInputContinuation,
        guarded_action: GuardedAction,
    ) -> Result<RemoteAgentObservation, ApplicationError>;

    async fn authentication_ready(
        &self,
        context: &RequestContext,
        continuation: RemoteAgentAuthenticationContinuation,
        guarded_action: GuardedAction,
    ) -> Result<RemoteAgentObservation, ApplicationError>;
}
```

H7 fixtures implement this port. H9A maps it to A2A later; H8 maps opaque authorization references to request-scoped credential state.

### Durable public-event stream

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceEventSequence(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableEventCursor(String);

pub enum PublicEventScope {
    Workspace,
    Conversation(ConversationId),
    Run(AgentRunId),
    HumanRequest(HumanRequestId),
    Trigger(TriggerDefinitionId),
}

pub enum PublicEventKind {
    ConversationCreated,
    InteractionRecorded,
    RunCreated,
    RunStatusChanged,
    PlanUpdated,
    ModelActivity,
    ToolActivity,
    DelegationActivity,
    ArtifactAvailable,
    ArtifactRejected,
    HumanRequestCreated,
    HumanRequestAnswered,
    ApprovalRequired,
    TriggerOccurrenceRecorded,
    RunProposalCreated,
    NotificationStateChanged,
    FinalOutcomeAvailable,
}

pub struct PublicEventRecord {
    pub id: PublicEventId,
    pub workspace_id: WorkspaceId,
    pub sequence: WorkspaceEventSequence,
    pub scope: PublicEventScope,
    pub kind: PublicEventKind,
    pub subject: RunReference,
    pub source_event_reference: RunReference,
    pub safe_payload: serde_json::Value,
    pub classification: DataClassification,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

pub struct PublicEventQuery {
    pub after: Option<DurableEventCursor>,
    pub conversation_id: Option<ConversationId>,
    pub run_id: Option<AgentRunId>,
    pub kinds: Vec<PublicEventKind>,
    pub limit: u32,
}
```

Cursor encoding binds workspace, sequence and format version with an integrity check or opaque server lookup. Payloads contain stable references and bounded safe summaries, not raw prompts, provider/tool frames or secrets.

### Notification preferences and delivery

```rust
pub enum NotificationDetailLevel { Minimal, Standard, Detailed }
pub enum NotificationUrgency { Passive, Normal, Important, Critical }

pub enum NotificationTarget {
    Principal(PrincipalId),
    Conversation(ConversationId),
    RunParticipants { run_id: AgentRunId, roles: Vec<RunParticipantRole> },
}

pub enum NotificationStatus {
    Prepared,
    Delivering,
    Delivered,
    Suppressed,
    Failed,
    Unknown,
    Cancelled,
}

pub struct NotificationPreferenceRevision {
    pub id: NotificationPreferenceRevisionId,
    pub preference_id: NotificationPreferenceId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
    pub revision: u32,
    pub detail_level: NotificationDetailLevel,
    pub enabled_event_kinds: Vec<PublicEventKind>,
    pub muted_conversation_ids: Vec<ConversationId>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub struct NotificationIntent {
    pub id: NotificationIntentId,
    pub source_event_id: PublicEventId,
    pub target: NotificationTarget,
    pub urgency: NotificationUrgency,
    pub detail_level: NotificationDetailLevel,
    pub template_revision: String,
    pub idempotency_key: String,
    pub status: NotificationStatus,
    pub created_at: Timestamp,
}

pub struct NotificationDeliveryRequest {
    pub intent_id: NotificationIntentId,
    pub recipient_principal_id: PrincipalId,
    pub channel: InteractionChannelKind,
    pub subject: String,
    pub body: String,
    pub references: Vec<RunReference>,
    pub idempotency_key: String,
}

#[async_trait::async_trait]
pub trait NotificationDeliveryPort: Send + Sync {
    async fn deliver(
        &self,
        context: &RequestContext,
        request: NotificationDeliveryRequest,
    ) -> Result<NotificationDeliveryObservation, ApplicationError>;

    async fn reconcile(
        &self,
        context: &RequestContext,
        delivery_id: NotificationDeliveryId,
    ) -> Result<NotificationDeliveryObservation, ApplicationError>;
}
```

H7 provides in-app/public-stream and CLI adapters. Email/messenger delivery remains an H9 extension using H8 connections.

### Trigger identities, revisions and sources

```rust
pub enum TriggerKind { Manual, Api, Schedule, ExternalEvent, StateChange, Condition, RunContinuation }
pub enum TriggerLifecycleStatus { Draft, Enabled, Disabled, Suspended, Revoked }
pub enum TriggerAutonomyLevel { Observe, Suggest, Prepare, Execute, Commit }
pub enum TriggerCatchUpPolicy { SkipMissed, FireOnce, Bounded { maximum_occurrences: u16 } }
pub enum AmbiguousLocalTimePolicy { Earliest, Latest }
pub enum NonexistentLocalTimePolicy { Skip, NextValid }

pub struct TriggerDefinition {
    pub id: TriggerDefinitionId,
    pub workspace_id: WorkspaceId,
    pub current_revision_id: TriggerDefinitionRevisionId,
    pub lifecycle: TriggerLifecycleStatus,
    pub state_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct TriggerScheduleSpec {
    pub dtstart_local: String,
    pub time_zone: String,
    pub rrule: String,
    pub catch_up: TriggerCatchUpPolicy,
    pub ambiguous_time: AmbiguousLocalTimePolicy,
    pub nonexistent_time: NonexistentLocalTimePolicy,
    pub time_zone_database_revision: String,
}

pub enum RunContinuationCause {
    HumanResponseAnswered,
    ApprovalGranted,
    ArtifactAvailable,
    RemoteInputSubmitted,
    AuthenticationReady,
    DependencyCompleted,
    ResumeAtTime,
}

pub enum TriggerSourceBinding {
    Manual,
    Api { route_key: String },
    Schedule { schedule: TriggerScheduleSpec },
    RunContinuation { causes: Vec<RunContinuationCause> },
    External { source_kind: String, binding_revision: String },
    StateChange { projection_kind: String },
    Condition { evaluator_revision: String, minimum_interval_seconds: u64 },
}
```

Unsupported source bindings cannot transition the Trigger identity to Enabled.

### Bounded filter and template DSL

```rust
pub enum TriggerFilterExpression {
    All(Vec<TriggerFilterExpression>),
    Any(Vec<TriggerFilterExpression>),
    Not(Box<TriggerFilterExpression>),
    Exists { pointer: String },
    Equals { pointer: String, value: serde_json::Value },
    InSet { pointer: String, values: Vec<serde_json::Value> },
    StringPrefix { pointer: String, prefix: String },
    NumberRange { pointer: String, minimum: Option<String>, maximum: Option<String> },
}

pub enum ObjectiveTemplateSegment {
    Literal(String),
    SourceValue {
        json_pointer: String,
        required: bool,
        maximum_characters: u32,
    },
}

pub struct ObjectiveTemplate {
    pub segments: Vec<ObjectiveTemplateSegment>,
    pub maximum_rendered_characters: u32,
}

pub struct InitialContextMappingRule {
    pub source_pointer: String,
    pub target_label: String,
    pub required: bool,
    pub maximum_bytes: u64,
    pub classification: DataClassification,
}
```

Filter depth is at most 8, nodes at most 128 and `InSet` at most 256 values. Number bounds use canonical decimal strings. JSON pointers must be compatible with the source schema. Template segments only substitute source values into the objective; they execute no expressions.

### Trigger revision, limits and autonomy envelope

```rust
pub struct TriggerWindowLimit {
    pub window_seconds: u64,
    pub maximum_occurrences: u32,
    pub maximum_new_runs: u32,
}

pub struct TriggerDeduplicationPolicy {
    pub source_key_pointers: Vec<String>,
    pub deduplication_window_seconds: u64,
    pub include_payload_hash: bool,
}

pub struct TriggerCausalLoopPolicy {
    pub reject_same_trigger_ancestry: bool,
    pub reject_same_run_ancestry: bool,
    pub maximum_causal_depth: u16,
    pub quiet_period_seconds: u64,
}

pub struct TriggerFailurePolicy {
    pub maximum_consecutive_failures: u16,
    pub suspension_seconds: Option<u64>,
    pub require_manual_reenable: bool,
}

pub struct TriggerDefinitionRevision {
    pub id: TriggerDefinitionRevisionId,
    pub trigger_id: TriggerDefinitionId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub kind: TriggerKind,
    pub source: TriggerSourceBinding,
    pub source_schema: serde_json::Value,
    pub filter: TriggerFilterExpression,
    pub target_agent_snapshot_id: AgentRuntimeSnapshotId,
    pub objective_template: ObjectiveTemplate,
    pub initial_context_mapping: Vec<InitialContextMappingRule>,
    pub autonomy_level: TriggerAutonomyLevel,
    pub requested_budget: ResourceBudgetRequest,
    pub cooldown_seconds: u64,
    pub window_limits: Vec<TriggerWindowLimit>,
    pub maximum_concurrent_runs: u16,
    pub deduplication: TriggerDeduplicationPolicy,
    pub causal_loop_policy: TriggerCausalLoopPolicy,
    pub failure_policy: TriggerFailurePolicy,
    pub content_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}

pub struct TriggerAutonomyEnvelope {
    pub trigger_revision_id: TriggerDefinitionRevisionId,
    pub level: TriggerAutonomyLevel,
    pub allowed_side_effects: Vec<ToolSideEffectClass>,
    pub maximum_tool_risk: RiskLevel,
    pub allow_external_commitment: bool,
    pub require_human_commit: bool,
    pub capability_ceiling_hash: [u8; 32],
    pub budget_allocation_id: BudgetAllocationId,
    pub content_hash: [u8; 32],
}
```

Activation compiles the immutable revision, then updates the Trigger identity lifecycle. The autonomy envelope is computed from revision + target snapshot + H2 policy + deployment flags and can only narrow permissions.

### Occurrence payload, causality and evaluation

```rust
pub enum TriggerOccurrencePayload {
    InlineJson { value: serde_json::Value },
    ArtifactReference {
        revision_id: ArtifactRevisionId,
        redacted_metadata: serde_json::Value,
    },
}

pub enum TriggerOccurrenceStatus {
    Received,
    Deduplicated,
    FilteredOut,
    CoolingDown,
    QuotaDenied,
    ProposalCreated,
    RunCreated,
    ContinuationEnqueued,
    Failed,
    Unknown,
}

pub struct TriggerCausation {
    pub causal_chain_id: CausalChainId,
    pub parent_occurrence_id: Option<TriggerOccurrenceId>,
    pub source_run_id: Option<AgentRunId>,
    pub source_trigger_revision_id: Option<TriggerDefinitionRevisionId>,
    pub depth: u16,
}

pub struct TriggerOccurrence {
    pub id: TriggerOccurrenceId,
    pub workspace_id: WorkspaceId,
    pub trigger_revision_id: TriggerDefinitionRevisionId,
    pub source_occurrence_id_hmac: [u8; 32],
    pub source_payload_hash: [u8; 32],
    pub payload: TriggerOccurrencePayload,
    pub causation: TriggerCausation,
    pub status: TriggerOccurrenceStatus,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

pub enum TriggerDecisionKind {
    FilteredOut,
    Deduplicated,
    SuppressedCooldown,
    SuppressedQuota,
    RejectedPolicy,
    CreateObservationRun,
    CreateProposal,
    CreatePreparationRun,
    CreateExecutionRun,
    ContinueExistingRun,
}

pub struct TriggerEvaluationDecision {
    pub id: TriggerEvaluationDecisionId,
    pub occurrence_id: TriggerOccurrenceId,
    pub kind: TriggerDecisionKind,
    pub rendered_objective_hash: Option<[u8; 32]>,
    pub autonomy_envelope_hash: Option<[u8; 32]>,
    pub policy_decision_id: Option<PolicyDecisionId>,
    pub budget_reservation_id: Option<BudgetReservationId>,
    pub reason_code: String,
    pub created_at: Timestamp,
}
```

Restricted or large source payloads are ingested through H6 and represented by exact Artifact revision plus redacted metadata.

### Durable Run proposals

```rust
pub enum RunProposalStatus { Draft, Ready, Accepted, Rejected, Expired, Superseded }

pub struct RunProposalRiskSummary {
    pub maximum_tool_risk: RiskLevel,
    pub possible_side_effects: Vec<ToolSideEffectClass>,
    pub data_classifications: Vec<DataClassification>,
    pub external_commitment_possible: bool,
}

pub struct PlanPreview {
    pub estimated_step_count: u32,
    pub step_kinds: Vec<PlanStepKind>,
    pub required_outputs: Vec<String>,
    pub warnings: Vec<String>,
    pub preview_hash: [u8; 32],
}

pub struct RunProposal {
    pub id: RunProposalId,
    pub workspace_id: WorkspaceId,
    pub trigger_occurrence_id: TriggerOccurrenceId,
    pub trigger_revision_id: TriggerDefinitionRevisionId,
    pub target_agent_snapshot_id: AgentRuntimeSnapshotId,
    pub objective: String,
    pub initial_context_manifest_hash: [u8; 32],
    pub plan_preview: Option<PlanPreview>,
    pub requested_tools: Vec<ToolRevisionId>,
    pub requested_budget: ResourceBudgetRequest,
    pub risk: RunProposalRiskSummary,
    pub required_approval_actions: Vec<ActionId>,
    pub status: RunProposalStatus,
    pub proposal_hash: [u8; 32],
    pub accepted_run_id: Option<AgentRunId>,
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}
```

Proposal acceptance re-evaluates current Trigger lifecycle/revision, target snapshot, H2 policy/budget and source availability. It creates exactly one Run or a stable rejection.

---

### Task 1: Add Conversation and Interaction domain contracts

**Files:** modify `id.rs`; create domain conversation files; modify domain `lib.rs`.

**Interfaces:** produces all Conversation/Interaction contracts and Task 1 IDs above.

- [ ] Write failing tests for bounded content, append-only correction, same-workspace Run links and conversation-role non-authority.
- [ ] Run `cargo test -p vestrace-domain conversation`; expect missing-module failures.
- [ ] Implement normalized titles, HMAC external identifiers, content accounting, lifecycle and deterministic interaction hash.
- [ ] Implement reply/supersede rules and explicit same-workspace links.
- [ ] Run tests and commit:

```bash
cargo test -p vestrace-domain conversation
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(conversation): add interaction contracts"
```

### Task 2: Add Human request, response and continuation contracts

**Files:** create domain human files; modify IDs/lib.

**Interfaces:** produces all Human contracts and Task 2 IDs above.

- [ ] Test request/payload mismatch, invalid schema, expiry, second response, token reuse and superseded request history.
- [ ] Test Approval without exact challenge fails and Authentication payload rejects code/token/password-shaped fields.
- [ ] Run `cargo test -p vestrace-domain human`; expect failures.
- [ ] Implement bounded values, deterministic response hash, one-way status transitions and CSPRNG token interface with constant-time hash verification.
- [ ] Run tests and commit:

```bash
cargo test -p vestrace-domain human
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(human): add typed continuation contracts"
```

### Task 3: Add Trigger, autonomy, schedule and proposal domain contracts

**Files:** create domain trigger files; modify IDs/lib.

**Interfaces:** produces Trigger/Proposal contracts and Task 3 IDs above.

- [ ] Test DSL depth/nodes, incompatible pointers, objective overflow, invalid schedule/catch-up, causal depth and deterministic dedup keys.
- [ ] Test Commit activation and autonomy-envelope widening fail.
- [ ] Run `cargo test -p vestrace-domain trigger`; expect failures.
- [ ] Implement immutable revision hashes, mutable identity lifecycle, schedule normalization, source-schema checks and proposal hashes.
- [ ] Implement fixed disposition mapping: Observe→observation Run, Suggest→proposal, Prepare→preparation Run, Execute→execution Run, Commit→`commit_autonomy_disabled`.
- [ ] Run tests and commit:

```bash
cargo test -p vestrace-domain trigger
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(trigger): add bounded trigger contracts"
```

### Task 4: Define H7 ports, commands and deterministic fixtures

**Files:** create application module roots/ports/commands and `vestrace-channel-test-support`.

**Interfaces:** produces stores/services for conversations, human continuations, public events, notifications, trigger registry/occurrences/schedules/proposals/storm state; `RemoteAgentContinuationPort`; deterministic `ClockPort`; Task 4 IDs.

- [ ] Add compile tests proving every port is object-safe.
- [ ] Define `ClockPort::now_utc()` plus time-zone database revision and deterministic advancement fixture.
- [ ] Define bounded canonical source occurrence input with inline JSON or Artifact reference only.
- [ ] Add one-shot fault points after interaction, response, approval, remote continuation, public event, notification, occurrence, proposal acceptance and Run creation.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application conversation human channel trigger
cargo test -p vestrace-channel-test-support
git add Cargo.toml Cargo.lock crates/vestrace-application crates/vestrace-channel-test-support
git commit -m "feat(channels): define H7 ports and fixtures"
```

### Task 5: Persist Conversations, interactions and Run links

**Files:** create migration `0048`, PostgreSQL conversation repositories and persistence tests.

- [ ] Migration creates conversations, participants, interaction events/parts/external HMACs, Run links, Run participant assignments and routing decisions.
- [ ] Test append-only rows, idempotency conflict, participant history, explicit Run roles and cross-workspace rejection.
- [ ] Persist one interaction plus optional routing/outbox work atomically; no model or Run execution occurs inside the transaction.
- [ ] Store only bounded inline data and exact Artifact revision IDs.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test conversation_persistence --test interaction_idempotency
git add migrations/0048_conversations_participants_interactions_run_links.sql crates tests
git commit -m "feat(conversation): persist interactions and run links"
```

### Task 6: Implement explicit Interaction-to-Run routing

**Files:** create conversation service/routing tests; integrate H1/H2/H6 ports.

- [ ] Test explicit Run/request references win and multiple active Runs yield clarification rather than heuristic continuation.
- [ ] Test Quarantined attachment records successfully but cannot enter executable context.
- [ ] Implement typed `CreateRun`, `Pause`, `Resume`, `Cancel`, `SubmitHumanResponse` and `AttachArtifact` routing with expected version and H2 action.
- [ ] Atomically create Run, origin link, participant assignments, Run event and work item.
- [ ] Treat model routing suggestions as advisory structured data requiring deterministic validation.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application --test conversation_run_routing
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test conversation_run_routing
git add crates tests/conversation_run_routing.rs
git commit -m "feat(conversation): route interactions explicitly"
```

### Task 7: Persist and apply typed Human requests/responses

**Files:** create migration `0049`, human repositories/services/tests.

- [ ] Migration creates requests, choices, target links, continuation hashes, responses, response references and lifecycle events.
- [ ] Request creation atomically applies exact H1 waiting status/event/checkpoint when blocking.
- [ ] Response service locks request/token, verifies principal/role/H2/schema/expiry, inserts one response, consumes token and enqueues one disposition work item.
- [ ] Restart after response commit reuses the same work item and rejects another response.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application --test human_response_atomicity
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test human_request_persistence --test human_response_atomicity
git add migrations/0049_human_requests_responses_and_continuations.sql crates tests
git commit -m "feat(human): persist typed requests and responses"
```

### Task 8: Integrate H2 approval and H5 remote continuation

**Files:** create approval/remote-continuation services/tests; modify H5 ports.

- [ ] Prove free-form “yes” creates no grant; typed Approve by eligible principal against matching challenge calls H2, while Deny records explicit denial.
- [ ] Coordinate approval creation, HumanRequest answer and Run wake-up idempotently; notification delivery never consumes a grant.
- [ ] Map H5 InputRequired to `RemoteInputRequired`; dispatch typed response through `RemoteAgentContinuationPort` after exact H2 guard consumption.
- [ ] Authentication continuation contains only opaque H8 reference; without H8 fixture it remains waiting.
- [ ] Lost remote continuation response becomes H5 `Unknown`/reconciliation and is never blindly resent.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application --test approval_response_integration
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test remote_continuation_restart
git add crates tests
git commit -m "feat(human): integrate approvals and remote continuations"
```

### Task 9: Persist public events, cursors and notifications

**Files:** create migration `0050`, public-event/notification repositories and tests.

- [ ] Migration creates workspace sequences, public events/source bindings, projection checkpoints, preference identities/revisions, notification intents/deliveries.
- [ ] Allocate workspace sequence and insert event atomically; source reprocessing returns original event.
- [ ] Encode/decode workspace-bound cursor; reject malformed/future/cross-workspace values.
- [ ] Query applies scope/participant/H2/classification filters before returning safe payload.
- [ ] Test reconnect from sequence 50 returns 51..N and duplicate delivery creates no authoritative mutation.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test public_event_cursor --test notification_persistence
git add migrations/0050_public_events_notifications_and_preferences.sql crates tests
git commit -m "feat(channel): persist public events and notifications"
```

### Task 10: Add HTTP/SSE and CLI adapters

**Files:** create HTTP/CLI crates, adapter tests and composition wiring.

- [ ] Implement endpoints:

```text
POST /v1/conversations
POST /v1/conversations/{id}/interactions
POST /v1/human-requests/{id}:respond
GET  /v1/events
GET  /v1/events/stream
POST /v1/triggers/{id}:fire
POST /v1/run-proposals/{id}:accept
POST /v1/run-proposals/{id}:reject
```

Writes require `Idempotency-Key`; versioned commands require expected revision/`If-Match`.

- [ ] SSE emits immutable `id`, typed `event`, JSON `data`, heartbeat comments and supports `Last-Event-ID`/`after`. Slow clients are closed with resumable cursor instead of unbounded buffering.
- [ ] CLI supports conversation send, request list/respond, event watch, trigger fire and proposal accept/reject.
- [ ] HTTP and CLI contract tests assert equivalent application records/events.
- [ ] Run/commit:

```bash
cargo test -p vestrace-channel-http -p vestrace-channel-cli
cargo clippy -p vestrace-channel-http -p vestrace-channel-cli --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates
git commit -m "feat(channel): add HTTP SSE and CLI adapters"
```

### Task 11: Implement safe notification rendering/delivery

**Files:** create notification/rendering services and tests using `0050`.

- [ ] Test Minimal/Standard/Detailed renderings increase safe detail but never include prompts, secrets, raw external text, blob keys or hidden reasoning.
- [ ] Generate intents only for enabled visible targets; muting/policy denial creates `Suppressed` with reason.
- [ ] Deliver with stable intent/recipient/channel idempotency key; built-in in-app/stream/CLI adapters return receipts.
- [ ] Unknown future external delivery uses reconcile and is not resent solely because acknowledgement was lost.
- [ ] Delivery failure never changes source state.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application channel::notification
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test notification_persistence
git add crates tests/notification_persistence.rs
git commit -m "feat(channel): deliver safe notifications"
```

### Task 12: Persist Trigger registry and immutable revisions

**Files:** create migration `0051`, registry/schedule repositories and tests.

- [ ] Migration creates Trigger identities, immutable revisions, source/filter/template/context/limit rows, lifecycle events, schedules and scheduler state.
- [ ] Activation recompiles schema/filter/template, resolves exact target snapshot, evaluates H2 `trigger.enable`, computes narrowed autonomy envelope and rejects Commit/unsupported sources.
- [ ] Disable/suspend blocks new occurrences but preserves history and does not cancel existing Runs without an explicit Run command.
- [ ] Revision change creates a new immutable row and scheduler state; prior occurrences remain bound to old revision.
- [ ] Run/commit:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test trigger_registry_persistence
git add migrations/0051_trigger_definitions_revisions_and_schedules.sql crates tests
git commit -m "feat(trigger): persist registry and schedules"
```

### Task 13: Implement Trigger evaluation, Manual/API firing, proposals and storm protection

**Files:** create migration `0052`, evaluator/manual API/autonomy/proposal/storm services/repositories/tests.

**Migration ownership:** Task 13 creates `0052` once. Task 14 consumes it and never edits it.

- [ ] Migration creates occurrences/payload references, evaluation decisions, Run proposals/events, dedup keys, cooldown/window/concurrency/failure counters, causal chains and Trigger-to-Run bindings.
- [ ] Manual/API fire authenticates principal, checks `trigger.fire`, validates source schema and inserts one occurrence + evaluation work atomically.
- [ ] Evaluation order is exact:

```text
load enabled identity + exact revision
→ source/schema validation
→ causal-loop check
→ dedup
→ filter
→ cooldown/window/concurrency
→ H2 policy and budget/quota reservation
→ objective/context manifest
→ autonomy envelope
→ proposal or new Run
```

- [ ] Counters/dedup/reservation/decision and proposal-or-Run creation commit atomically. Duplicate API/source occurrence returns original occurrence.
- [ ] Suggest creates Ready proposal only. Acceptance re-evaluates current state and creates one Run; repeated acceptance returns `accepted_run_id`.
- [ ] Observe/Prepare/Execute Runs persist exact revision, occurrence, causal chain and autonomy envelope; H2 checks the envelope on every action.
- [ ] Test A→A and A→B→A causal loops, limits, concurrent Run ceiling and failure suspension.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application --test trigger_manual_api
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test trigger_manual_api --test trigger_storm_protection \
             --test run_proposal_persistence
git add migrations/0052_trigger_occurrences_proposals_dedup_and_causality.sql crates tests
git commit -m "feat(trigger): evaluate bounded trigger occurrences"
```

### Task 14: Implement durable Schedule and RunContinuation sources

**Files:** create scheduler/continuation services and tests using `0051`/`0052` without modifying migrations.

- [ ] Test UTC schedule, spring-forward nonexistent time, fall-back ambiguity, downtime and every catch-up policy.
- [ ] Scheduler leases due rows with generation fencing and atomically records occurrence + next fire + evaluation work.
- [ ] Schedule edit uses a new Trigger revision; old occurrences remain unchanged.
- [ ] RunContinuation accepts exact durable causes and creates one occurrence/decision/`ContinueRun` work item; it never creates a Run.
- [ ] Duplicate HumanResponse/ArtifactAvailable/DependencyCompleted projection returns original continuation occurrence and no second wake-up.
- [ ] Run/commit:

```bash
cargo test -p vestrace-application trigger::scheduler trigger::continuation
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test trigger_schedule_dst --test trigger_run_continuation
git add crates tests/trigger_schedule_dst.rs tests/trigger_run_continuation.rs
git commit -m "feat(trigger): add schedules and run continuations"
```

### Task 15: Integrate workers, checkpoint V5, RLS, boundaries and acceptance

**Files:** create migration `0053`, workers, Run event/work/checkpoint changes, boundary scripts, CI and acceptance tests.

- [ ] Add work kinds:

```text
RouteInteraction
ApplyHumanResponse
ContinueRemoteInvocation
ProjectPublicEvent
DeliverNotification
EvaluateTriggerOccurrence
FireScheduledTrigger
ContinueRunFromTrigger
ExpireHumanRequest
ExpireRunProposal
SuspendTrigger
```

Payloads contain stable IDs/expected revisions/deadlines, never interaction bodies, credentials, clear continuation tokens or unredacted trigger data.

- [ ] Add logical Run events:

```rust
ConversationLinked { conversation_id: ConversationId },
HumanRequestCreated { request_id: HumanRequestId },
HumanResponseApplied { request_id: HumanRequestId, response_id: HumanResponseId },
TriggerRunBound { trigger_revision_id: TriggerDefinitionRevisionId, occurrence_id: TriggerOccurrenceId },
RunProposalAccepted { proposal_id: RunProposalId },
```

Projection/delivery/scheduler progress remains H7-local and does not increment `RunVersion`.

- [ ] `RunCheckpointV5` stores active requests, linked conversations, source Trigger occurrence/autonomy envelope and last public-event projection reference. Older payloads remain readable.
- [ ] Migration `0053` forces RLS, same-workspace checks, append-only guards and idempotency/expiry/due/window/causal indexes; it prevents cross-workspace role/token/cursor use.
- [ ] Boundary scripts reject channel SDKs in core, generic approval booleans without challenge refs, clear tokens in storage/logs, credentials in auth responses, A2A types, direct channel Run mutation, Commit activation and Trigger actions without H2/autonomy envelope.
- [ ] Mandatory acceptance scenario:

```text
authenticated HTTP interaction
→ Conversation + Origin Run link
→ durable AgentRun
→ HumanRequest Clarification
→ WaitingForInput + checkpoint
→ disconnect
→ CLI typed response
→ one continuation
→ Run resumes
→ SSE reconnect from prior cursor
→ missed events in order
→ deterministic remote InputRequired
→ same HumanRequest boundary
→ continuation after worker restart
→ scheduled occurrence
→ bounded Observe Run
→ duplicate occurrence suppressed
→ causal loop suppressed
→ trigger ceiling enforced
```

- [ ] Negative acceptance proves membership cannot approve/cancel/export; “yes” creates no approval; quarantined attachment cannot enter context; notification failure does not change Run; Suggest creates proposal only; Prepare cannot commit; Execute cannot make external commitment; Commit activation fails; replay sends no response/notification/Trigger/remote continuation.
- [ ] Run all gates and commit:

```bash
bash scripts/verify-conversation-boundary.sh
bash scripts/verify-human-continuation-boundary.sh
bash scripts/verify-trigger-autonomy-boundary.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h7_acceptance --test conversation_trigger_rls \
             --test sse_reconnect --test trigger_schedule_dst \
             --test trigger_storm_protection
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

git add migrations/0053_conversation_trigger_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml crates scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(channels): add H7 acceptance and autonomy gates"
```

---

## Migration ownership

```text
0048 Task 5   Conversations, participants, interactions and Run links
0049 Task 7   Human requests, responses and continuations
0050 Task 9   Public events, notifications and preferences
0051 Task 12  Trigger identities, revisions, schedules and scheduler state
0052 Task 13  Occurrences, proposals, deduplication, counters and causality
0053 Task 15  RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H7 completion definition

H7 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
channel input
→ authenticated principal
→ append-only InteractionEvent
→ explicit routing decision
→ AgentRun or HumanRequest command
→ durable waiting/response/continuation
→ canonical PublicEvent
→ reconnectable SSE/CLI projection
→ optional safe notification

trigger source
→ enabled Trigger identity + immutable revision
→ occurrence + causality
→ dedup/cooldown/quota/policy
→ autonomy envelope
→ RunProposal, bounded new Run or exact Run continuation
→ restart-safe execution
```

Required invariants:

1. Conversations and Runs are separate aggregates connected explicitly.
2. Multiple Runs in one conversation never cause ambiguous automatic continuation.
3. Interaction history is append-only/bounded and binary content uses H6 references.
4. Conversation membership grants no privileged Run operation.
5. Human requests are typed, exact-target and restart-safe.
6. Continuation tokens are hashed/one-time and never authenticate alone.
7. Approval responses cannot bypass H2 operation-bound rules.
8. Authentication responses contain no credential material.
9. Remote continuations are transport-neutral and duplicate-safe.
10. Public events are durable, workspace-ordered, resumable and at-least-once.
11. Channel adapters are equivalent projections and never mutate Runs directly.
12. Notification failure cannot affect authoritative state.
13. Trigger identities/revisions and lifecycle ownership are unambiguous.
14. Manual/API/Schedule/RunContinuation work without external services.
15. External/StateChange/Condition remain disabled without extension.
16. Filters/templates are bounded declarative data.
17. Trigger autonomy only narrows authority.
18. Suggest creates proposal; Prepare cannot commit; Execute cannot infer external commitment; Commit is disabled.
19. Every occurrence is idempotent and ambiguous source delivery cannot duplicate a Run.
20. Cooldown/windows/concurrency/H2 budget/quota/failure suspension are transactional.
21. Direct and multi-Trigger causal loops are suppressed.
22. DST/downtime behavior is explicit and deterministic.
23. RunContinuation resumes the exact existing Run.
24. Restart duplicates no routing, response, approval, remote continuation, notification, occurrence, proposal acceptance or Run creation.
25. Replay performs no channel delivery, continuation or Trigger fire.
26. H8 can bind credential flows without changing H7 persistence.
27. H9 can provide external channel/Trigger/notification extensions without changing H7 core.
28. H9A can map A2A progress/InputRequired/AuthRequired without SDK leakage.
29. H10 can project telemetry/evaluations without changing history.
30. H11 can expose web/MCP/SDK surfaces over the same ports.
31. H7 tests require no public provider, channel, A2A service or permanent credential.

## Explicit non-goals

H7 does not implement email/messenger adapters, OAuth/API-key acquisition, secret storage, A2A wire protocol, Agent Card discovery, arbitrary webhooks, executable condition code, public web UI, permanent push tokens, automatic Commit autonomy, model-controlled policy changes or exactly-once network delivery.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h7-conversations-channels-triggers`, change dependencies, create migrations `0048`–`0053`, start HTTP/SSE/scheduler services, send notifications, fire Triggers, mutate Runs, change CI or execute H7 tests.
