# Vestrace H7 Conversations, Channels and Triggers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, start schedulers or channel servers, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement durable conversations and interaction routing, typed human continuations, channel-independent public events and notifications, HTTP/SSE/CLI channel adapters, immutable trigger definitions, manual/API/schedule/run-continuation triggers, bounded autonomy, durable Run proposals and transactional storm protection.

**Architecture:** H7 separates communication, execution and activation. `Conversation` and `InteractionEvent` preserve communication history; `AgentRun` remains the sole authority for executable work; `TriggerDefinitionRevision` describes a governed source of observations, proposals, new Runs or continuations. Human requests are typed durable continuation points whose responses are validated and authorized before they mutate a Run, approval, remote invocation or Artifact review. All channels call the same application services and consume one durable public-event stream; SSE and CLI are projections, never sources of truth.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H6 Rust workspace; Rust Edition 2024; Tokio; Axum; Tower; Serde; Schemars; SQLx; PostgreSQL 17; Server-Sent Events; Clap; SHA-256 canonical hashing; JSON Schema validation; RFC 5545 RRULE subset with IANA time-zone data; deterministic clock/scheduler/channel fixtures; proptest; tracing.

## Global Constraints

- Complete all five v0.1 plans and H1–H6 before implementing H7.
- Harness design sections `15. Triggers and autonomy` and `17. Conversations, interactions and channels` are normative.
- `Conversation` stores communication; `AgentRun` stores execution. A conversation may link to multiple Runs and a Run may have more than one authorized conversation link, but neither aggregate owns the other.
- PostgreSQL is authoritative for conversations, participants, interactions, Run links, human requests/responses, continuation records, public-event cursors, notification intents/deliveries, trigger definitions/revisions, occurrences, evaluations, schedule state, Run proposals, deduplication and causal-chain records.
- Vestrace owns every domain type, application port, persisted schema, lifecycle transition, event kind and public DTO.
- Axum, Clap, SSE, WebSocket, MCP, A2A, provider and connector SDK types may not appear in domain/application signatures or PostgreSQL schemas.
- Channel adapters authenticate a principal and normalize input; they never assign Run roles, approve operations, increase budgets, choose policies or bypass application services.
- Conversation membership does not grant `run.cancel`, `approval.grant`, `artifact.export`, `budget.increase`, `trigger.manage` or any other capability.
- Run participation roles are explicit, versioned and checked together with H2 policy. `Approver` is eligibility to request an approval decision, not an automatic `ApprovalGrant`.
- Interaction content, channel metadata, external IDs, filenames and remote progress text are untrusted data.
- Interaction events are append-only. Corrections, edits, retractions and redactions create new events linked to the original; they do not mutate history.
- Interaction content has bounded parts. Large/binary attachments are exact H6 `ArtifactRevision` references and are never embedded in ordinary interaction rows or work items.
- Quarantined/rejected/deleted Artifact revisions may be referenced as upload history but cannot enter Run context, tool input or remote continuation until H6 declares them eligible.
- Human requests are typed. A free-form message cannot satisfy an approval, authentication, review or manual-action request unless it validates against that exact request contract.
- A continuation token is an opaque one-time correlation proof stored only as a hash. It never authenticates a principal and never substitutes for H2 authorization.
- Approval responses call H2 approval services with the exact operation fingerprint, resource, constraints and approver identity. H7 never manufactures an `ApprovalGrant` from words such as “yes”.
- Authentication responses contain only status and a future H8 authorization-flow reference. Credentials, authorization codes, refresh tokens and passwords never enter H7 interaction or request payloads.
- Remote-agent input/authentication continuations use Vestrace-owned H5/H7 ports. No A2A Task, Agent Card or `a2a-rs` type appears in H7 contracts.
- Public event streaming is at-least-once. Consumers deduplicate by immutable event ID and cursor; reconnect never assumes exactly-once delivery.
- Streaming, notifications and CLI output are projections. Loss or duplication of a delivery never changes authoritative Run, HumanRequest, Trigger or Proposal state.
- Public-event cursors are workspace-bound, opaque externally and monotonically ordered within a workspace. A cursor from another workspace is invalid.
- Notification content is rendered from canonical templates and safe summaries. Raw secrets, backend identifiers, hidden reasoning, untrusted instructions and unrestricted Artifact content are excluded.
- Trigger revisions are immutable. Enabling, disabling, suspending and revoking are explicit lifecycle commands with expected revision.
- Mandatory first-slice trigger kinds are `Manual`, `Api`, `Schedule` and `RunContinuation`.
- `ExternalEvent`, `StateChange` and `Condition` are represented by stable source-port contracts but no production external connector is required in H7.
- Trigger filters and objective mappings use a bounded declarative DSL. Arbitrary code, SQL, regex with unbounded complexity and template evaluation are forbidden.
- Trigger execution cannot widen the target `AgentRuntimeSnapshot`, H2 policy, data classification, tool ceiling, delegation ceiling or budget ceiling.
- Standard autonomy levels are `Observe`, `Suggest`, `Prepare` and `Execute`. `Commit` exists in the schema for forward compatibility but activation is rejected unless a future explicit feature flag and policy permit it; standard v0.2 never enables it.
- `Observe` may automatically create only a read-only observation Run. `Suggest` creates a durable `RunProposal` and does not execute the proposed work. `Prepare` may create a Run that reaches safe preparation/preview but cannot commit. `Execute` may perform only policy-approved bounded reversible/idempotent actions and cannot infer permission for destructive, irreversible or external-commitment actions.
- Every trigger occurrence is idempotent. Unknown external source delivery is reconciled by source occurrence ID/hash and never blindly creates another Run.
- Trigger storm protection includes deduplication, cooldown, max occurrences/runs per window, concurrent-Run ceiling, H2 budget/quota checks, failure suspension and causal-loop prevention.
- A `RunContinuation` trigger never creates a second Run. It enqueues continuation of the exact existing Run after validating the durable cause.
- Schedule calculations are deterministic for one trigger revision, time-zone database revision and clock instant. DST ambiguity/nonexistent-time and catch-up behavior are explicit.
- Trigger scheduler leases and operational heartbeats do not increment `RunVersion` or trigger-definition revision.
- Existing migrations `0014`–`0047` are never edited. H7 migrations are `0048`–`0053` and are created once.
- CI uses deterministic clocks, local HTTP/SSE fixtures and fake notification/remote-continuation ports. It requires no public model, email/messenger service, A2A server, OAuth provider or permanent credential.
- Future implementation branch: `feat/h7-conversations-channels-triggers`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  conversation/{mod,conversation,participant,interaction,content,run_link}.rs
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

### Conversation, participants and Run links

```rust
pub enum ConversationStatus {
    Active,
    Archived,
    Closed,
}

pub enum ConversationVisibility {
    Private,
    Restricted,
    Workspace,
}

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

pub enum ConversationParticipantRole {
    Owner,
    Member,
    Guest,
    Agent,
    Observer,
}

pub struct ConversationParticipant {
    pub conversation_id: ConversationId,
    pub actor: InteractionActorRef,
    pub role: ConversationParticipantRole,
    pub joined_at: Timestamp,
    pub left_at: Option<Timestamp>,
}

pub enum RunParticipantRole {
    Owner,
    Requester,
    Approver,
    Contributor,
    Observer,
}

pub struct ConversationRunLink {
    pub id: ConversationRunLinkId,
    pub conversation_id: ConversationId,
    pub run_id: AgentRunId,
    pub linked_by: PrincipalId,
    pub relation: ConversationRunRelation,
    pub created_at: Timestamp,
}

pub enum ConversationRunRelation {
    Origin,
    Continuation,
    Discussion,
    Notification,
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

Conversation roles affect visibility and presentation only. Every Run command still checks `RunParticipantAssignment`, base capability and H2 policy.

### Interaction actors, kinds and content

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
    pub value_hash: [u8; 32],
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

pub enum InteractionTrustClass {
    AuthenticatedUser,
    VestraceSystem,
    VestraceAgent,
    VerifiedExternal,
    UntrustedExternal,
}
```

One event has 1–64 parts, each text part is at most 256 KiB and total serialized inline content is at most 1 MiB. External identifier values are never persisted in cleartext unless a future connector policy explicitly requires an encrypted mapping.

### Interaction-to-Run routing

```rust
pub enum InteractionRoutingIntent {
    CreateRun,
    ContinueRun { run_id: AgentRunId },
    SubmitHumanResponse { request_id: HumanRequestId },
    ApplyRunCommand { run_id: AgentRunId, command: TypedRunCommand },
    AttachOnly,
    NoExecutableIntent,
}

pub enum InteractionRoutingDecisionKind {
    Accepted,
    ClarificationRequired,
    Rejected,
}

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

Routing uses explicit run/request references first. An ambiguous conversation with multiple active Runs cannot silently choose one; it creates a typed clarification request or remains non-executable.

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

pub enum HumanRequestStatus {
    Open,
    Answered,
    Expired,
    Cancelled,
    Superseded,
}

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
    pub connection_id: Option<ConnectionId>,
    pub authorization_flow_reference: Option<String>,
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
    pub status: HumanRequestStatus,
    pub request_revision: u64,
    pub expires_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

pub enum HumanResponsePayload {
    Clarification { value: serde_json::Value },
    Choice { choice_id: String },
    Approval { decision: HumanApprovalDecision },
    Review { decision: HumanReviewDecision, comments: Option<String>, evidence: Vec<RunReference> },
    Authentication { status: AuthenticationContinuationStatus, authorization_flow_reference: Option<String> },
    ManualAction { result: serde_json::Value, evidence: Vec<RunReference> },
    RemoteInput { value: serde_json::Value },
}

pub enum HumanApprovalDecision { Approve, Deny }
pub enum HumanReviewDecision { Accept, AcceptWithWarnings, RequestRevision, Reject }
pub enum AuthenticationContinuationStatus { Completed, Cancelled, Failed }

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

Payload kind must exactly match request kind and JSON Schema. One request accepts at most one terminal response. A correction creates a new request linked through `supersedes`.

### Continuation proof and disposition

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanContinuationToken(String);

pub struct HumanContinuationRecord {
    pub id: HumanContinuationRecordId,
    pub request_id: HumanRequestId,
    pub token_hash: [u8; 32],
    pub maximum_uses: u16,
    pub used_count: u16,
    pub expires_at: Timestamp,
    pub status: HumanContinuationStatus,
}

pub enum HumanContinuationStatus {
    Issued,
    Consumed,
    Expired,
    Revoked,
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

The clear token is returned once through an authorized response channel and never logged or stored. Token verification is necessary for correlation where configured, but authenticated principal, participant eligibility and H2 policy remain mandatory.

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
    pub authorization_flow_reference: String,
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

H7 deterministic fixtures implement the port. H9A later maps it to protocol-specific continuation without changing these types. H8 supplies the authorization-flow reference and request-scoped credential state.

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

Cursor encoding binds workspace ID, sequence and version with an integrity MAC or server-side opaque lookup. Public payloads contain stable references and bounded safe summaries, not raw prompts, secrets, SQL or provider/tool frames.

### Notification intents and preferences

```rust
pub enum NotificationDetailLevel {
    Minimal,
    Standard,
    Detailed,
}

pub enum NotificationUrgency {
    Passive,
    Normal,
    Important,
    Critical,
}

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

H7 ships in-app/public-stream and CLI delivery adapters. Email/messenger adapters remain H9 extensions and H8 connection consumers.

### Trigger definitions and sources

```rust
pub enum TriggerKind {
    Manual,
    Api,
    Schedule,
    ExternalEvent,
    StateChange,
    Condition,
    RunContinuation,
}

pub enum TriggerLifecycleStatus {
    Draft,
    Enabled,
    Disabled,
    Suspended,
    Revoked,
}

pub enum TriggerAutonomyLevel {
    Observe,
    Suggest,
    Prepare,
    Execute,
    Commit,
}

pub enum TriggerCatchUpPolicy {
    SkipMissed,
    FireOnce,
    Bounded { maximum_occurrences: u16 },
}

pub enum AmbiguousLocalTimePolicy { Earliest, Latest }
pub enum NonexistentLocalTimePolicy { Skip, NextValid }

pub struct TriggerScheduleSpec {
    pub dtstart_local: String,
    pub time_zone: String,
    pub rrule: String,
    pub catch_up: TriggerCatchUpPolicy,
    pub ambiguous_time: AmbiguousLocalTimePolicy,
    pub nonexistent_time: NonexistentLocalTimePolicy,
    pub time_zone_database_revision: String,
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

pub enum RunContinuationCause {
    HumanResponseAnswered,
    ApprovalGranted,
    ArtifactAvailable,
    RemoteInputSubmitted,
    AuthenticationReady,
    DependencyCompleted,
    ResumeAtTime,
}
```

`External`, `StateChange` and `Condition` revisions remain disabled unless a compatible H9 extension is active.

### Bounded filter and objective mapping DSL

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

Filter depth is at most 8, total nodes at most 128 and `InSet` at most 256 values. JSON pointers are allowlisted against the source schema. Templates perform substitution only; they cannot execute expressions, access environment variables or emit instructions outside the bounded objective field.

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
    pub lifecycle: TriggerLifecycleStatus,
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

Activation computes the autonomy envelope from the trigger revision, target snapshot, H2 policy and deployment feature flags. The envelope can only narrow permissions.

### Trigger occurrences and decisions

```rust
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

pub struct TriggerOccurrence {
    pub id: TriggerOccurrenceId,
    pub workspace_id: WorkspaceId,
    pub trigger_revision_id: TriggerDefinitionRevisionId,
    pub source_occurrence_id_hash: [u8; 32],
    pub source_payload_hash: [u8; 32],
    pub source_payload: serde_json::Value,
    pub causation: TriggerCausation,
    pub status: TriggerOccurrenceStatus,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
}

pub struct TriggerCausation {
    pub causal_chain_id: CausalChainId,
    pub parent_occurrence_id: Option<TriggerOccurrenceId>,
    pub source_run_id: Option<AgentRunId>,
    pub source_trigger_revision_id: Option<TriggerDefinitionRevisionId>,
    pub depth: u16,
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

Source payload is bounded by its schema and classification policy. Restricted source data may instead be stored as an H6 Artifact reference and a redacted metadata payload.

### Durable Run proposals

```rust
pub enum RunProposalStatus {
    Draft,
    Ready,
    Accepted,
    Rejected,
    Expired,
    Superseded,
}

pub struct RunProposalRiskSummary {
    pub maximum_tool_risk: RiskLevel,
    pub possible_side_effects: Vec<ToolSideEffectClass>,
    pub data_classifications: Vec<DataClassification>,
    pub external_commitment_possible: bool,
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
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

pub struct PlanPreview {
    pub estimated_step_count: u32,
    pub step_kinds: Vec<PlanStepKind>,
    pub required_outputs: Vec<String>,
    pub warnings: Vec<String>,
    pub preview_hash: [u8; 32],
}
```

Proposal acceptance re-evaluates current policy, budget, trigger lifecycle, target snapshot and source references. It creates one Run or returns a stable rejection; it never trusts the earlier estimate as current authority.

---

### Task 1: Add Conversation and Interaction domain contracts

**Files:** create/modify domain conversation files and IDs listed above.

**Interfaces:** produces `Conversation`, participant/run-role types, `InteractionEvent`, content parts, trust values and run-link contracts.

- [ ] Write failing tests for append-only interaction correction, bounded parts, path-independent Artifact references, participant non-authority and same-workspace run links.
- [ ] Implement normalized titles, channel/namespace stable IDs, content-size accounting and canonical interaction hash.
- [ ] Implement conversation revision transitions `Active → Archived|Closed`, with `Archived → Active` allowed by policy and `Closed` terminal.
- [ ] Implement immutable interaction linkage: `reply_to` may reference prior visible event; `supersedes` is permitted only for Retraction/RedactionNotice or typed correction.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain conversation
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(conversation): add interaction contracts"
```

### Task 2: Add Human request, response and continuation contracts

**Files:** create domain `human/{mod,request,response,continuation}.rs`; modify IDs/lib.

- [ ] Test request-kind/payload mismatch, invalid response schema, expired request, second terminal response and token reuse.
- [ ] Test that an Approval response without exact `ApprovalChallengeRef` is invalid and Authentication payload rejects credential/code/token fields.
- [ ] Implement bounded questions/choices/comments, deterministic response hash and one-way status transitions.
- [ ] Implement token generation interface using CSPRNG, persistent hash only, constant-time verification and redacted debug/serialization.
- [ ] Implement disposition derivation without executing side effects.
- [ ] Run/commit.

### Task 3: Add Trigger, schedule, autonomy and proposal domain contracts

**Files:** create domain trigger files and IDs listed above.

- [ ] Test bounded filter/template depth, invalid JSON pointer, objective overflow, duplicate schedule ambiguity policy and invalid RRULE/catch-up bounds.
- [ ] Test lifecycle activation rejects `Commit`, unsupported external source and autonomy envelope wider than target/H2 ceiling.
- [ ] Test causal depth, same-trigger ancestry and dedup-key determinism.
- [ ] Implement immutable revision/content hashes, source-schema validation, schedule normalization and proposal hashes.
- [ ] Implement stable mapping `Observe → observation Run`, `Suggest → proposal`, `Prepare → preparation Run`, `Execute → execution Run`; `Commit` returns `commit_autonomy_disabled`.
- [ ] Run/commit.

### Task 4: Define H7 application ports and deterministic test support

**Files:** create application conversation/human/channel/trigger module roots/ports/commands and `vestrace-channel-test-support` crate.

**Interfaces:** produces conversation/interaction repositories, Run-routing port, HumanRequest/Response/Continuation stores, H2 approval adapter, H5 remote continuation port, public-event store/projector, notification delivery port, trigger registry/occurrence/schedule/proposal/storm stores, deterministic clock and fixtures.

- [ ] Add object-safety compile tests for every port.
- [ ] Define `ClockPort` returning UTC plus time-zone database revision; tests can advance deterministically.
- [ ] Define source occurrence ingestion as bounded canonical JSON plus source occurrence ID hash; no SDK payload type.
- [ ] Add one-shot fault points after interaction insert, request answer, approval grant, remote continuation dispatch, public-event insert, notification dispatch, trigger occurrence insert, proposal acceptance and Run creation.
- [ ] Run/commit.

### Task 5: Persist Conversations, participants, interactions and Run links

**Files:** create migration `0048`, PostgreSQL conversation repositories and persistence tests.

- [ ] Migration creates `conversations`, `conversation_participants`, `conversation_events`, `interaction_content_parts`, `interaction_external_identifiers`, `conversation_run_links`, `run_participant_assignments` and routing decisions.
- [ ] Test append-only interaction rows, idempotency conflict, participant leave/rejoin history, explicit Run roles and cross-workspace rejection.
- [ ] Store JSON/text inline only under bounds; Artifact parts store exact H6 revision IDs.
- [ ] Atomically record interaction plus optional routing decision/outbox work, but never perform model/Run work inside the transaction.
- [ ] Run/commit.

### Task 6: Implement explicit Interaction-to-Run routing and typed Run commands

**Files:** create conversation service/routing/commands tests; integrate H1 Run coordinator and H6 attachment eligibility ports.

- [ ] Test explicit request/run references win over conversation heuristics; multiple active Runs without reference yield `ClarificationRequired`.
- [ ] Test an AttachmentProvided event may be recorded while Quarantined but cannot create/continue model work until H6 says Available.
- [ ] Implement typed commands `CreateRun`, `PauseRun`, `ResumeRun`, `CancelRun`, `SubmitHumanResponse`, `AttachArtifact` with expected version and H2 action.
- [ ] Create Run, interaction, origin link, participant assignments, one Run event and work item atomically.
- [ ] Ensure model-proposed routing is advisory and must pass deterministic target/reference checks.
- [ ] Run/commit.

### Task 7: Persist and execute typed Human requests/responses

**Files:** create migration `0049`, human repositories/services/tests.

- [ ] Migration creates requests, choices, target links, continuation hashes, responses, response references and lifecycle events.
- [ ] Request creation atomically applies the exact H1 waiting state/event and checkpoint reference when blocking.
- [ ] Response service locks request/token, validates principal/role/H2 policy/schema/expiry, inserts one response, consumes token and enqueues disposition work atomically.
- [ ] Clarification/choice/manual/review responses resume only after all target-specific checks pass.
- [ ] Restart after response commit but before continuation worker executes reuses the same disposition/work item and never accepts a second response.
- [ ] Run/commit.

### Task 8: Integrate exact approval and remote-agent continuation

**Files:** create human approval/remote-continuation services and integration tests; modify H5 delegation ports with `RemoteAgentContinuationPort`.

- [ ] Approval test proves text “yes” does not grant approval; only typed Approve by eligible principal against matching fingerprint calls H2, while Deny records a durable denial.
- [ ] H2 approval grant creation, HumanRequest answer, continuation disposition and Run wake-up are coordinated idempotently; no grant is consumed merely by notification delivery.
- [ ] Remote InputRequired creates a linked `RemoteInputRequired` request; typed response dispatches through `RemoteAgentContinuationPort` after `remote_agent.continue` guard consumption.
- [ ] AuthenticationRequired response contains only H8 flow reference/status; without H8 fixture it remains waiting.
- [ ] Lost remote continuation response moves invocation to H5 `Unknown`/reconciliation and restart never sends a duplicate continuation blindly.
- [ ] Run/commit.

### Task 9: Persist canonical public events, cursors and reconnect projections

**Files:** create migration `0050`, public-event projection/store and cursor tests.

- [ ] Migration creates workspace stream sequences, public events, source bindings, notification preferences/intents/deliveries and projection checkpoints.
- [ ] Allocate one workspace sequence and insert event atomically with source projection checkpoint. Reprocessing source returns the existing event.
- [ ] Encode/decode opaque cursor with workspace/version/integrity binding; reject malformed, future and cross-workspace cursors.
- [ ] Query applies event scope, participant visibility, H2 read policy and classification redaction before returning payload.
- [ ] Test reconnect after event 50 returns 51..N in sequence, and duplicate SSE delivery does not create duplicate state.
- [ ] Run/commit.

### Task 10: Add HTTP/SSE and CLI adapters over the same application core

**Files:** create `vestrace-channel-http`, `vestrace-channel-cli`, adapter tests and CLI composition wiring.

- [ ] HTTP endpoints:

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

All writes require `Idempotency-Key`; versioned commands require expected revision/`If-Match`.

- [ ] SSE uses `id`, typed `event`, JSON `data`, heartbeat comments and `Last-Event-ID`/`after` cursor. Backpressure closes slow clients with a resumable last cursor rather than buffering without bound.
- [ ] CLI commands cover conversation send, request list/respond, event watch, trigger fire and proposal accept/reject; CLI prints references and never bypasses authorization.
- [ ] Contract tests issue equivalent HTTP and CLI commands and assert identical application records/events.
- [ ] MCP adapter remains a thin future/public-surface consumer of these application ports; no H7 domain changes are reserved for it.
- [ ] Run/commit.

### Task 11: Implement notification preferences, safe rendering and delivery

**Files:** create channel notification/rendering services, repositories/tests using migration `0050`.

- [ ] Test Minimal/Standard/Detailed output contains progressively more safe references but never prompts, credentials, raw external text, blob keys or hidden reasoning.
- [ ] Generate intents only for enabled kinds/visible targets; muted conversation and policy-denied classification become `Suppressed` with reason.
- [ ] Delivery uses stable recipient/channel/intent idempotency key. Built-in in-app/stream/CLI adapters return deterministic receipts.
- [ ] Unknown future external delivery remains `Unknown` and uses reconcile; never send a second notification solely because acknowledgement was lost.
- [ ] Notification failure never rolls back or changes source Run/HumanRequest/Trigger state.
- [ ] Run/commit.

### Task 12: Persist Trigger definitions/revisions and implement Manual/API firing

**Files:** create migration `0051`, trigger registry/manual API services/repositories/tests.

- [ ] Migration creates trigger identities/revisions, source/filter/template/context/limit rows, lifecycle events, schedule definitions and scheduler state.
- [ ] Activation recompiles source schema/filter/template, resolves exact target snapshot, evaluates H2 `trigger.enable`, computes narrowed autonomy envelope and rejects Commit/unsupported source.
- [ ] Manual/API firing authenticates principal, checks `trigger.fire`, validates payload/schema, creates one occurrence and enqueues evaluation atomically.
- [ ] Same API idempotency/source occurrence returns original occurrence; conflicting payload returns idempotency conflict.
- [ ] Disable/suspend prevents new occurrences but preserves history and does not cancel existing Runs unless an explicit Run command is authorized.
- [ ] Run/commit.

### Task 13: Implement durable schedule and Run-continuation triggers

**Files:** create trigger scheduler/continuation services, schedule repository/tests using migrations `0051`/`0052`.

- [ ] Test UTC schedules, spring-forward nonexistent local time, fall-back ambiguous time, process downtime and every catch-up policy.
- [ ] Scheduler leases due rows with generation fencing, computes next occurrence from immutable revision/time-zone DB revision and records occurrence + next fire + work atomically.
- [ ] Schedule edit creates a new revision and scheduler state; already recorded occurrence remains bound to the old revision.
- [ ] RunContinuation accepts only exact durable causes. It inserts an occurrence/decision and one `ContinueRun` work item; it never creates a Run.
- [ ] Duplicate HumanResponse/ArtifactAvailable/DependencyCompleted projection yields the original continuation occurrence and no second wake-up.
- [ ] Run/commit.

### Task 14: Implement Trigger evaluation, Run proposals, autonomy and storm protection

**Files:** create migration `0052`, trigger evaluator/autonomy/proposal/storm services/repositories/tests.

- [ ] Migration creates occurrences, payload references, evaluation decisions, Run proposals, proposal events, dedup keys, cooldown state, window counters, causal chains, failure counters and trigger-to-Run bindings.
- [ ] Evaluation order:

```text
load exact enabled revision
→ source/schema validation
→ causal-loop check
→ deduplication
→ filter
→ cooldown/window/concurrency counters
→ H2 policy and budget/quota reservation
→ render objective/context manifest
→ compute autonomy envelope
→ create proposal, new Run or existing-Run continuation
```

- [ ] All counters/dedup/reservation/decision and proposal-or-Run creation commit in one scoped transaction; rejected paths release prepared reservations as specified by H2.
- [ ] `Suggest` creates a Ready proposal only. Acceptance re-evaluates current state and atomically creates one Run. Repeated acceptance returns the same Run.
- [ ] `Observe`, `Prepare` and `Execute` Runs store exact trigger revision, occurrence, causal chain and autonomy envelope; Policy Engine uses the envelope on every action.
- [ ] Simulate trigger A → Run → event → trigger A and A → B → A. Same ancestry/depth policy suppresses loops with durable reason.
- [ ] Consecutive failure threshold suspends trigger and emits notification; scheduler/worker retry does not count the same occurrence twice.
- [ ] Run/commit.

### Task 15: Integrate H7 workers, checkpoint V5, RLS, boundaries and acceptance

**Files:** create migration `0053`, H7 workers, Run events/work/checkpoint changes, boundary scripts, CI and acceptance tests.

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

Work payloads contain stable IDs/expected revisions/deadlines, never interaction bodies, credentials, clear continuation tokens or unredacted trigger payloads.

- [ ] Add logical Run events:

```rust
ConversationLinked { conversation_id: ConversationId },
HumanRequestCreated { request_id: HumanRequestId },
HumanResponseApplied { request_id: HumanRequestId, response_id: HumanResponseId },
TriggerRunBound { trigger_revision_id: TriggerDefinitionRevisionId, occurrence_id: TriggerOccurrenceId },
RunProposalAccepted { proposal_id: RunProposalId },
```

Delivery/projection/trigger scheduler progress remains H7-local and does not increment `RunVersion`.

- [ ] `RunCheckpointV5` adds active human requests, linked conversations, source trigger occurrence/autonomy envelope and last public-event projection reference. Older payloads remain readable.
- [ ] Migration `0053` forces RLS on every H7 workspace table, adds same-workspace target checks, append-only guards, idempotency/expiry/due-schedule/window/causal-chain indexes and prevents role/token/cursor cross-workspace use.
- [ ] Boundary scripts reject channel SDK types in core, generic approval booleans without challenge refs, clear continuation tokens in persistence/logs, credentials in auth responses, A2A types in remote continuation, direct channel mutation of Runs, Commit activation and trigger actions without H2/autonomy envelope.
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
→ missed events replay in order
→ deterministic remote invocation InputRequired
→ same HumanRequest/response boundary
→ continuation after worker restart
→ schedule occurrence
→ bounded Observe Run
→ duplicate schedule/source delivery suppressed
→ causal loop suppressed
→ trigger ceiling enforced
```

- [ ] Acceptance also proves: conversation membership alone cannot approve/cancel/export; “yes” does not create approval; quarantined attachment cannot enter context; notification failure does not change Run; Suggest creates proposal only; proposal acceptance is idempotent; Prepare cannot commit; Execute cannot perform external commitment; Commit activation fails; replay sends no response/notification/trigger/remote continuation.
- [ ] Required CI jobs:

```text
conversation-domain-and-postgres
human-request-continuation
approval-and-remote-continuation
public-events-and-sse
channel-http-cli-contracts
trigger-registry-manual-api
trigger-schedule-dst
trigger-storm-autonomy
conversation-trigger-boundaries
h7-acceptance
```

- [ ] Run/commit:

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
0049 Task 7   Human requests, responses and continuation records
0050 Task 9   Public events, notification intents/deliveries and preferences
0051 Task 12  Trigger definitions, revisions, schedules and scheduler state
0052 Task 14  Occurrences, proposals, deduplication, counters and causality
0053 Task 15  RLS, indexes and Run bindings
```

No later task edits a migration after its owner task commits it.

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
→ immutable Trigger revision
→ occurrence + causality
→ dedup/cooldown/quota/policy
→ autonomy envelope
→ RunProposal, bounded Run or exact Run continuation
→ durable events and restart recovery
```

Required invariants:

1. Conversations and Runs remain separate aggregates connected by explicit links.
2. One conversation can discuss multiple Runs without ambiguous automatic continuation.
3. Interaction history is append-only and bounded; binary content uses H6 references.
4. Conversation membership never grants privileged Run operations.
5. Human requests are typed, exact-target and restart-safe.
6. Continuation tokens are hashed, one-time and never authenticate by themselves.
7. Approval responses cannot bypass H2 operation-bound approval rules.
8. Authentication responses contain no credential material.
9. Remote input/auth continuations remain transport-neutral and duplicate-safe.
10. Public-event streams are durable, workspace-ordered, resumable and at-least-once.
11. Channel adapters produce equivalent application state and cannot mutate Runs directly.
12. Notifications are safe projections and delivery failure cannot affect source state.
13. Trigger definitions/revisions are immutable and activation is explicit.
14. Manual/API/Schedule/RunContinuation are fully implemented without external services.
15. External/StateChange/Condition sources remain disabled without compatible extension.
16. Filters/templates are bounded declarative data, never arbitrary code.
17. Trigger autonomy only narrows target/policy authority.
18. Suggest creates a proposal rather than executing proposed work.
19. Prepare cannot commit; Execute cannot infer irreversible/external-commitment authority.
20. Commit remains disabled in standard v0.2.
21. Every occurrence is idempotent and source-loss ambiguity cannot duplicate a Run.
22. Cooldown, windows, concurrency, H2 budgets/quotas and failure suspension are transactional.
23. Causal-loop policies suppress direct and multi-trigger feedback loops.
24. Schedule behavior across DST/downtime is explicit and deterministic.
25. RunContinuation resumes the exact existing Run and never creates another.
26. Restart does not duplicate interaction routing, response application, approval, remote continuation, notification, trigger occurrence, proposal acceptance or Run creation.
27. Replay performs no channel delivery, remote continuation or trigger firing.
28. H8 can attach connection/credential authorization flows without changing HumanRequest/trigger persistence.
29. H9 can supply external trigger/channel/notification extensions and profile resolution without changing H7 core.
30. H9A can map A2A InputRequired/AuthRequired/progress to H7 contracts without SDK-type leakage.
31. H10 can add trace/metrics/evaluation projections without changing event history.
32. H11 can expose web/MCP/SDK surfaces over the same application ports.
33. H7 tests require no public provider, external channel, A2A service or permanent credential.

## Explicit non-goals

H7 does not implement email or messenger adapters, OAuth/API-key acquisition, secret storage, A2A wire protocol, Agent Card discovery, arbitrary webhook sources, browser notifications, unrestricted condition code, production WebSocket as a second mandatory stream, public web UI, permanent push tokens, automatic Commit autonomy, model-controlled policy changes or exactly-once network delivery.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h7-conversations-channels-triggers`, change dependencies, create migrations `0048`–`0053`, start HTTP/SSE/scheduler services, send notifications, fire triggers, mutate Runs, modify production channel/trigger code, change CI or execute H7 tests.
