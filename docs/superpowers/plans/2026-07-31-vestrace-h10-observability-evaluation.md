# Vestrace H10 Observability and Evaluation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, add observability dependencies, create migrations, start exporters, capture production content, run evaluations, activate regression gates, execute replay or shadow traffic, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Implement privacy-aware operational telemetry, a tamper-evident security audit plane, durable evaluation datasets and runs, risk-oriented verification and bounded correction, component regression gates, side-effect-free replay/shadow execution, user-facing Run explanations and operational read models.

**Architecture:** H10 consumes canonical H1–H9A journals, references and outcomes without replacing them. Operational telemetry is a lossy projection and may never drive authoritative Run transitions. Security audit is append-only, source-linked and hash-chained with externally signed integrity checkpoints. Evaluation and verification are separate durable aggregates that use exact component/environment snapshots, H2 budgets, H3 model execution, H6 Artifact/evidence references and deterministic graders. Safe replay reconstructs or evaluates recorded work in an isolated evaluation scope where tools, remote agents, credentials, exports, notifications and memory activation are substituted or denied. Component owners remain responsible for activation and must require an exact passing regression-gate evidence record.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H9A; Rust Edition 2024; Tokio; Serde/Schemars; SQLx and PostgreSQL 17; tracing; OpenTelemetry-compatible traces, metrics and logs behind a Vestrace-owned port; optional OTLP exporter; SHA-256 and HMAC-SHA-256; deployment signing through H8 service credentials; H6 Artifact Store for approved captures/datasets/reports; deterministic fixed-point metrics; JSON Schema; local dashboard read models; deterministic evaluators and provider/tool/remote fixtures; proptest.

## Global Constraints

- Complete all five v0.1 plans and H1–H9A before implementing H10.
- Harness design section `26. Observability, audit и evaluations` and H10 roadmap exit gate are normative.
- H1 Run state, RunEvent journal and logical replay remain authoritative. H10 projections never become an alternate Run state store.
- H2 policy, approvals, budgets, tickets and accounting journals remain authoritative. H10 may evaluate and explain them but cannot issue or reinterpret authority.
- H3 owns model execution and usage; H4 owns Tool/Sandbox execution; H5 owns planning/delegation/handoff; H6 owns Artifact/evidence/context bytes and provenance; H7 owns public events/human interaction; H8 owns credentials; H9 owns component/package/extension activation; H9A owns A2A transport mappings.
- Vestrace owns every H10 domain/application/persistence/public contract. OpenTelemetry, exporter, dashboard, statistics and evaluator SDK types remain inside adapters.
- Operational telemetry is best-effort and bounded. Exporter outage, buffer overflow or sampling may drop telemetry but must never fail, pause, retry or mutate a Run.
- Security audit and evaluation records are durable PostgreSQL state. Their persistence failure follows the source subsystem’s explicit policy; mandatory security audit intents must be committed transactionally with the protected source operation or its durable outbox.
- Telemetry labels never contain user content, prompts, tool arguments, URLs, filenames, external task IDs, credentials, arbitrary workspace names or unbounded remote strings.
- Workspace/Run/step/invocation correlation IDs may appear as structured log/trace fields under capture policy but never as metric labels.
- Metric labels use bounded enums or deployment-approved stable identifiers only. High-cardinality IDs are prohibited.
- Inbound trace headers are untrusted. They may create a validated trace link but cannot choose the authoritative Vestrace trace ID, workspace or parent span.
- Capture modes are `Full`, `Redacted`, `StructuredOnly`, `MetadataOnly` and `Disabled`. No mode stores hidden chain-of-thought, provider reasoning tokens, secret material or unrestricted external content.
- `Full` means exact content only when current H2/H6/H8 policy explicitly allows capture. It does not override classification, retention, secret scanning or purge.
- `Disabled` disables optional content/diagnostic capture only. Minimum security audit, accounting, policy decision references, hashes, revisions, validation outcomes and operational health counters remain.
- Optional capture bytes are H6 Artifacts with exact provenance, quarantine, retention and purge behavior. PostgreSQL stores bounded manifests, hashes and Artifact revision references.
- Capture policy is evaluated at collection time and again before export. An exporter cannot receive a richer mode than its exact binding permits.
- Security audit records contain content-free facts and references. They never store raw prompts, bodies, headers, tokens, private keys, hidden reasoning, blob keys or unrestricted external errors.
- Each audit stream is append-only and hash-chained. Periodic checkpoints sign the exact chain head through an H8-backed signing port; signing keys never enter PostgreSQL or worker payloads.
- Audit verification detects sequence gaps, previous-hash mismatch, record-hash mismatch, invalid checkpoint signatures and source-reference conflicts.
- Hard purge of source content leaves content-free audit/evaluation tombstones and broken-content explanations, not reconstructed content.
- Evaluation datasets, component snapshots, environments, grading plans, metric definitions, gate definitions and baselines use immutable revisions and canonical hashes.
- Package evaluation fixtures from H9 remain H6 Artifact revisions until imported into an H10 `EvaluationDatasetRevision`.
- An `EvaluationRun` never mutates the tested component, active policy, production Run, memory or activation state.
- Evaluation budgets are allocated through H2. Evaluators cannot reserve outside the EvaluationRun allocation.
- Deterministic checks and external-state readback outrank model judgment. A model grader cannot override a deterministic failure, policy violation, unsupported evidence or unresolved `Unknown` operation.
- Independent-model grading uses H3, an exact grader snapshot/template and an independence requirement. Producer scratchpad and hidden reasoning are excluded.
- Evaluator output is untrusted structured data and must pass schema, bounds, evidence-reference and consistency checks before it becomes a grade.
- Scores use deterministic integer/fixed-point representations. Floating-point values are not persisted as authoritative grades or gate thresholds.
- Grader disagreement is preserved. Aggregation never silently discards a failing mandatory grader.
- Risk-oriented verification derives a minimum verification plan from deterministic risk inputs and the exact H9 VerificationProfile revision. A model may request stronger verification but cannot reduce it.
- A Run cannot transition to `Succeeded` until mandatory H10 verification reaches an allowed terminal disposition. `SucceededWithWarnings` and `Partial` preserve unresolved warnings and unmet criteria.
- Correction is bounded by attempts, time and H2 allocation. It creates explicit H5 plan revisions/new steps or new guarded operations; it never rewrites completed history.
- Unknown or ambiguous external effects must reconcile before correction. Verification cannot treat retry as proof.
- A correction loop cannot lower mandatory safety, policy, evidence or quality criteria.
- Regression-gate results are evidence, not activation. H3/H4/H7/H9/H9A component owners create activation revisions only after rechecking exact gate evidence, policy and current dependencies.
- Any policy-compliance regression, unauthorized side effect, secret leak, false success above the hard threshold or duplicate external effect is a hard gate failure regardless of aggregate quality score.
- Safe replay is distinct from H1 logical replay. H1 reconstructs authoritative state; H10 replay re-evaluates captured inputs in an isolated evaluation scope.
- Safe replay never invokes H4 Tool commit, H5 remote dispatch, H8 credential use, H6 export, H7 notification/trigger execution, H9 activation or memory activation.
- Shadow mode may invoke an explicitly selected H3 model under H2 evaluation policy/budget, but all tools, remote agents, human responses and external effects are recorded substitutions or deterministic fixtures.
- If capture is insufficient for a replay or grade, the result is `NotReplayable` or `Inconclusive`; H10 never fabricates missing inputs.
- Recorded `Unknown` outcomes remain Unknown in replay unless a separate stored reconciliation record resolves them.
- Run explanations use journaled decisions, exact references, safe summaries, evidence and omissions. They never expose hidden reasoning or claim causes not supported by records.
- Operational dashboards are projections and may lag. They cannot be used as the source of billing, policy, budget or Run completion truth.
- Replay never exports telemetry, signs audit checkpoints, creates production work, sends notifications or calls a remote service unless the operation is an explicitly authorized H3 model-only shadow call.
- Existing migrations `0014`–`0072` are never edited. H10 migrations are `0073`–`0080`, each created once by one task.
- CI uses in-memory/loopback exporters, deterministic signing keys, local H6 Artifacts, fake clocks and deterministic model/tool/remote fixtures. No public telemetry backend, model, A2A service or permanent credential is required.
- Future implementation branch: `feat/h10-observability-evaluation`.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  observability/{mod,correlation,capture,telemetry,projection}.rs
  audit/{mod,scope,record,source,chain,checkpoint,verification}.rs
  evaluation/{mod,dataset,case,component,environment,run,grader,grade,metric,report}.rs
  verification/{mod,risk,plan,attempt,finding,outcome,correction}.rs
  regression/{mod,baseline,gate,comparison,evidence}.rs
  replay/{mod,manifest,mode,substitution,checkpoint,outcome}.rs
  explanation/{mod,snapshot,section,warning}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  observability/{mod,ports,correlation,capture_policy,projection,export}.rs
  audit/{mod,ports,intent,writer,checkpoint,verify,query}.rs
  evaluation/{mod,ports,commands,dataset_service,runner,case_worker,grading,aggregation,report}.rs
  verification/{mod,ports,risk_service,planner,runner,completion_gate,correction}.rs
  regression/{mod,ports,baseline_service,gate_runner,activation_evidence}.rs
  replay/{mod,ports,manifest_builder,runner,substitutions,shadow,safety}.rs
  explanation/{mod,ports,builder,query}.rs
  operations/{mod,projection,dashboard}.rs
  composition/observability.rs

crates/vestrace-observability-otel/
  Cargo.toml
  src/{lib,provider,span,metrics,logs,exporter,redaction,cardinality,error}.rs

crates/vestrace-audit-runtime/
  Cargo.toml
  src/{lib,canonical,chain,signing,verification,error}.rs

crates/vestrace-evaluation-runtime/
  Cargo.toml
  src/{lib,deterministic,reference,rubric,independent_model,aggregation,replay,error}.rs

crates/vestrace-evaluation-test-support/
  Cargo.toml
  src/{lib,datasets,components,environments,graders,model,tool,remote,clock,signing,exporter,faults,fixtures}.rs

crates/vestrace-channel-http/src/
  observability_routes.rs
  audit_routes.rs
  evaluation_routes.rs
  explanation_routes.rs

crates/vestrace-channel-cli/src/
  observability_commands.rs
  audit_commands.rs
  evaluation_commands.rs
  explanation_commands.rs

crates/vestrace-infrastructure/src/postgres/
  observability/{mod,policy_repository,capture_repository,projection_repository,exporter_repository}.rs
  audit/{mod,stream_repository,record_repository,checkpoint_repository}.rs
  evaluation/{mod,dataset_repository,component_repository,environment_repository,run_repository,grade_repository,metric_repository}.rs
  verification/{mod,plan_repository,attempt_repository,finding_repository,correction_repository}.rs
  regression/{mod,baseline_repository,gate_repository,evidence_repository}.rs
  replay/{mod,manifest_repository,run_repository,checkpoint_repository}.rs
  explanation/{mod,repository,dashboard_repository}.rs

migrations/
  0073_observability_capture_policies_exporters_and_projections.sql
  0074_security_audit_streams_records_and_integrity_checkpoints.sql
  0075_evaluation_datasets_cases_components_and_environments.sql
  0076_evaluation_runs_case_attempts_grades_metrics_and_reports.sql
  0077_verification_plans_attempts_findings_and_corrections.sql
  0078_regression_baselines_gates_activation_evidence_and_replay.sql
  0079_run_explanations_and_operational_read_models.sql
  0080_observability_evaluation_rls_indexes_and_run_bindings.sql

tests/
  telemetry_correlation.rs
  telemetry_cardinality.rs
  telemetry_export_failure.rs
  capture_policy.rs
  capture_artifact_purge.rs
  audit_persistence.rs
  audit_chain_integrity.rs
  audit_checkpoint_signing.rs
  audit_source_completeness.rs
  evaluation_dataset_persistence.rs
  evaluation_component_snapshot.rs
  evaluation_environment_snapshot.rs
  evaluation_runner_restart.rs
  deterministic_grader.rs
  reference_grader.rs
  rubric_grader.rs
  independent_model_grader.rs
  grade_aggregation.rs
  evaluation_budget_isolation.rs
  verification_risk_matrix.rs
  verification_completion_gate.rs
  verification_correction_bounds.rs
  regression_gate_persistence.rs
  regression_gate_activation.rs
  replay_projection.rs
  replay_side_effect_denial.rs
  shadow_model_only.rs
  replay_unknown_preservation.rs
  run_explanation.rs
  operational_projection.rs
  remote_agent_metrics.rs
  observability_evaluation_rls.rs
  h10_acceptance.rs

scripts/
  verify-telemetry-boundary.sh
  verify-telemetry-cardinality.sh
  verify-audit-append-only.sh
  verify-audit-integrity.sh
  verify-evaluation-boundary.sh
  verify-replay-side-effect-boundary.sh
  verify-no-hidden-reasoning-capture.sh
  verify-regression-gate-boundary.sh
```

---

## Normative contracts

### IDs and ownership

Task 1 adds:

```text
ObservabilityCapturePolicyId
ObservabilityCapturePolicyRevisionId
TelemetryExporterBindingId
TelemetryExporterBindingRevisionId
TelemetryProjectionCursorId
ObservabilityCaptureRecordId
```

Task 2 adds:

```text
SecurityAuditStreamId
SecurityAuditRecordId
AuditIntegrityCheckpointId
AuditIntegrityVerificationId
```

Task 3 adds:

```text
EvaluationDatasetId
EvaluationDatasetRevisionId
EvaluationCaseId
EvaluationComponentSnapshotId
EvaluationEnvironmentSnapshotId
EvaluationRunId
EvaluationCaseAttemptId
EvaluationGradingPlanRevisionId
EvaluationGradeId
EvaluationMetricDefinitionId
EvaluationMetricDefinitionRevisionId
EvaluationMetricObservationId
EvaluationReportId
```

Task 3 also adds:

```text
VerificationPlanId
VerificationPlanRevisionId
VerificationAttemptId
VerificationFindingId
CorrectionRequestId
RegressionBaselineId
RegressionBaselineRevisionId
RegressionGateDefinitionId
RegressionGateDefinitionRevisionId
RegressionGateRunId
RegressionGateEvidenceId
ReplayManifestId
ReplayRunId
ReplayCheckpointId
RunExplanationSnapshotId
OperationalProjectionRevisionId
```

### Correlation and trace ownership

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CorrelationContext {
    pub workspace_id: WorkspaceId,
    pub conversation_id: Option<ConversationId>,
    pub run_id: Option<AgentRunId>,
    pub subrun_id: Option<AgentRunId>,
    pub step_id: Option<RunStepId>,
    pub invocation: Option<InvocationCorrelationRef>,
    pub causation_id: CausationId,
    pub trace_id: VestraceTraceId,
    pub span_id: VestraceSpanId,
}

pub enum InvocationCorrelationRef {
    Model(ModelExecutionId),
    Tool(ToolInvocationId),
    Sandbox(SandboxSessionId),
    RemoteAgent(RemoteAgentInvocationId),
    Artifact(ArtifactIngestionSessionId),
    Evaluation(EvaluationRunId),
}

#[serde(transparent)] pub struct VestraceTraceId([u8; 16]);
#[serde(transparent)] pub struct VestraceSpanId([u8; 8]);
#[serde(transparent)] pub struct CausationId(uuid::Uuid);
```

Trace/span IDs are generated by Vestrace at trusted boundaries. Valid external trace context is stored as an optional link in adapter-local telemetry; it never replaces `workspace_id`, `trace_id` or parentage selected by Vestrace.

### Capture policy

```rust
pub enum ObservabilityCaptureMode {
    Full,
    Redacted,
    StructuredOnly,
    MetadataOnly,
    Disabled,
}

pub enum CaptureSubjectKind {
    ModelInput,
    ModelOutput,
    ToolInput,
    ToolOutput,
    RemoteMessage,
    RemoteArtifact,
    HumanInteraction,
    ContextSnapshot,
    EvaluationInput,
    EvaluationOutput,
    Diagnostic,
}

pub enum CaptureOmissionReason {
    PolicyDenied,
    SecretDetected,
    ClassificationExceeded,
    SubjectDisabled,
    Sampling,
    ContentUnavailable,
    Purged,
    UnsupportedRepresentation,
    SizeLimit,
}

pub struct CaptureRule {
    pub subject: CaptureSubjectKind,
    pub maximum_classification: DataClassification,
    pub mode: ObservabilityCaptureMode,
    pub sample_basis_points: u16,
    pub maximum_bytes: u64,
    pub retention_days: u32,
}

pub struct ObservabilityCapturePolicyRevision {
    pub id: ObservabilityCapturePolicyRevisionId,
    pub policy_id: ObservabilityCapturePolicyId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub rules: Vec<CaptureRule>,
    pub allow_external_export: bool,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub struct ObservabilityCaptureRecord {
    pub id: ObservabilityCaptureRecordId,
    pub workspace_id: WorkspaceId,
    pub policy_revision_id: ObservabilityCapturePolicyRevisionId,
    pub subject: CaptureSubjectKind,
    pub source_references: Vec<RunReference>,
    pub selected_mode: ObservabilityCaptureMode,
    pub source_content_hash: Option<[u8; 32]>,
    pub captured_artifact_revision_id: Option<ArtifactRevisionId>,
    pub structured_manifest: Option<serde_json::Value>,
    pub omission_reason: Option<CaptureOmissionReason>,
    pub byte_size: u64,
    pub created_at: Timestamp,
}
```

`sample_basis_points` is `0..=10_000`. `Full`/`Redacted` captures are H6 Artifact revisions that re-enter quarantine and secret inspection. `StructuredOnly` contains allowlisted typed fields and references. `MetadataOnly` contains hashes, revisions, usage, validation and policy IDs only.

### Telemetry exporter bindings

```rust
pub enum TelemetrySignal { Traces, Metrics, Logs }
pub enum TelemetryExporterKind { InMemory, StdoutJson, Otlp }

pub struct TelemetryExporterBindingRevision {
    pub id: TelemetryExporterBindingRevisionId,
    pub binding_id: TelemetryExporterBindingId,
    pub workspace_id: Option<WorkspaceId>,
    pub revision: u32,
    pub kind: TelemetryExporterKind,
    pub signals: std::collections::BTreeSet<TelemetrySignal>,
    pub endpoint_origin: Option<NormalizedOrigin>,
    pub service_binding_revision_id: Option<ServiceCredentialBindingRevisionId>,
    pub maximum_capture_mode: ObservabilityCaptureMode,
    pub queue_capacity: u32,
    pub batch_size: u32,
    pub flush_interval_ms: u64,
    pub content_hash: [u8; 32],
}
```

Exporter configuration contains no secret. OTLP authentication uses H8 service binding and request-scoped lease. Exporter retry is bounded and independent of Run/work retries.

### Metric cardinality contract

```rust
pub enum MetricLabelKey {
    DeploymentRole,
    WorkKind,
    OutcomeClass,
    RiskClass,
    ProviderStableId,
    ToolStableId,
    ExtensionStableId,
    TransportKind,
    ArtifactDisposition,
    EvaluationKind,
}

pub struct MetricPoint {
    pub name: String,
    pub value: MetricNumber,
    pub labels: std::collections::BTreeMap<MetricLabelKey, String>,
    pub observed_at: Timestamp,
}

pub enum MetricNumber {
    Counter(u64),
    Gauge(i64),
    DurationMillis(u64),
    Bytes(u64),
    BasisPoints(u16),
    Microunits(u64),
}
```

Metric validation rejects arbitrary label keys, UUID values, URLs, user strings and values longer than 128 bytes. Workspace, Run, step, invocation, task and message IDs are trace/log fields only.

### Security audit chain

```rust
pub enum SecurityAuditScope {
    Deployment,
    Workspace { workspace_id: WorkspaceId },
}

pub enum SecurityAuditCategory {
    Capability,
    Policy,
    Approval,
    Budget,
    Credential,
    Connection,
    Commitment,
    ArtifactExport,
    MemoryPurge,
    ExtensionActivation,
    PackageActivation,
    RemoteAgentActivation,
    Autonomy,
    Trigger,
    EvaluationGate,
}

pub enum SecurityAuditResult {
    Permitted,
    Denied,
    Completed,
    Failed,
    Cancelled,
    Unknown,
    Revoked,
}

pub struct SecurityAuditSourceRef {
    pub source_kind: String,
    pub source_id: String,
    pub source_hash: [u8; 32],
}

pub struct SecurityAuditRecordBody {
    pub category: SecurityAuditCategory,
    pub principal_id: Option<PrincipalId>,
    pub acting_agent_snapshot_id: Option<AgentRuntimeSnapshotId>,
    pub run_id: Option<AgentRunId>,
    pub step_id: Option<RunStepId>,
    pub operation_fingerprint: Option<OperationFingerprint>,
    pub action: String,
    pub resource_kind: String,
    pub resource_id_hash: [u8; 32],
    pub decision_id: Option<PolicyDecisionId>,
    pub obligations_hash: Option<[u8; 32]>,
    pub external_identity_hash: Option<[u8; 32]>,
    pub result: SecurityAuditResult,
    pub safe_code: String,
    pub source: SecurityAuditSourceRef,
    pub occurred_at: Timestamp,
}

pub struct SecurityAuditRecord {
    pub id: SecurityAuditRecordId,
    pub stream_id: SecurityAuditStreamId,
    pub sequence: u64,
    pub previous_record_hash: [u8; 32],
    pub body: SecurityAuditRecordBody,
    pub record_hash: [u8; 32],
    pub recorded_at: Timestamp,
}

pub struct AuditIntegrityCheckpoint {
    pub id: AuditIntegrityCheckpointId,
    pub stream_id: SecurityAuditStreamId,
    pub through_sequence: u64,
    pub chain_head_hash: [u8; 32],
    pub signing_key_revision: String,
    pub signature_algorithm: String,
    pub signature: Vec<u8>,
    pub created_at: Timestamp,
}
```

Record hash V1 is SHA-256 over domain separator, stream ID, sequence, previous hash and canonical body. Sequence starts at 1. Checkpoint signature is over stream ID, sequence, chain head and checkpoint schema version.

### Evaluation dataset and cases

```rust
pub enum EvaluationCaseInput {
    Interaction { parts: Vec<InteractionContentPart> },
    Objective { objective: String, artifact_revision_ids: Vec<ArtifactRevisionId> },
    RecordedRun { run_id: AgentRunId, capture_manifest_id: ReplayManifestId },
}

pub struct ExpectedProperty {
    pub stable_id: String,
    pub description: String,
    pub mandatory: bool,
    pub evidence_requirement: Option<EvidenceRequirement>,
}

pub struct EvaluationCase {
    pub id: EvaluationCaseId,
    pub dataset_revision_id: EvaluationDatasetRevisionId,
    pub stable_key: String,
    pub input: EvaluationCaseInput,
    pub expected_properties: Vec<ExpectedProperty>,
    pub prohibited_outcomes: Vec<ProhibitedOutcome>,
    pub grading_plan_revision_id: EvaluationGradingPlanRevisionId,
    pub maximum_budget: ResourceBudgetRequest,
    pub case_hash: [u8; 32],
}

pub struct EvaluationDatasetRevision {
    pub id: EvaluationDatasetRevisionId,
    pub dataset_id: EvaluationDatasetId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub stable_name: String,
    pub description: String,
    pub source_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub case_ids: Vec<EvaluationCaseId>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Dataset import verifies Artifact availability, package asset binding, schemas, duplicate stable keys, exact case hashes and prohibited content. Dataset cases cannot contain secrets, live credential handles or mutable external task references.

### Component and environment snapshots

```rust
pub enum EvaluatedComponentRef {
    AgentRuntimeSnapshot(AgentRuntimeSnapshotId),
    Model(ModelDefinitionRevisionId),
    Tool(ToolRevisionId),
    Workflow(WorkflowRevisionId),
    PolicySnapshot(PolicySnapshotId),
    PackageActivation(PackageActivationRevisionId),
    ExtensionActivation(ExtensionActivationRevisionId),
    RemoteAgentActivation(RemoteAgentActivationRevisionId),
    A2ARoute(A2ARemoteRouteRevisionId),
}

pub struct EvaluationComponentSnapshot {
    pub id: EvaluationComponentSnapshotId,
    pub workspace_id: WorkspaceId,
    pub root_component: EvaluatedComponentRef,
    pub dependency_revision_refs: Vec<ExactRevisionRef>,
    pub catalogue_hashes: Vec<[u8; 32]>,
    pub policy_snapshot_id: PolicySnapshotId,
    pub content_hash: [u8; 32],
}

pub enum EvaluationNetworkMode {
    DenyAll,
    ModelProvidersOnly,
    LoopbackFixturesOnly,
}

pub struct EvaluationEnvironmentSnapshot {
    pub id: EvaluationEnvironmentSnapshotId,
    pub core_source_revision: String,
    pub migration_head: String,
    pub cargo_lock_hash: [u8; 32],
    pub enabled_feature_hash: [u8; 32],
    pub deployment_profile_hash: [u8; 32],
    pub clock_revision: String,
    pub deterministic_seed: u64,
    pub network_mode: EvaluationNetworkMode,
    pub provider_fixture_revision_ids: Vec<String>,
    pub tool_fixture_revision_ids: Vec<String>,
    pub remote_fixture_revision_ids: Vec<String>,
    pub capture_policy_revision_id: ObservabilityCapturePolicyRevisionId,
    pub content_hash: [u8; 32],
}
```

Snapshots contain no secret, host path, raw environment variable, credential reference or mutable health state.

### Evaluation run

```rust
pub enum EvaluationRunStatus {
    Created,
    Validating,
    Scheduled,
    Running,
    Grading,
    Completed,
    CompletedWithWarnings,
    Partial,
    Failed,
    Cancelled,
}

pub struct EvaluationRun {
    pub id: EvaluationRunId,
    pub workspace_id: WorkspaceId,
    pub dataset_revision_id: EvaluationDatasetRevisionId,
    pub component_snapshot_id: EvaluationComponentSnapshotId,
    pub environment_snapshot_id: EvaluationEnvironmentSnapshotId,
    pub repetitions: u16,
    pub status: EvaluationRunStatus,
    pub budget_allocation_id: BudgetAllocationId,
    pub capture_policy_revision_id: ObservabilityCapturePolicyRevisionId,
    pub requested_by: PrincipalId,
    pub logical_revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub enum EvaluationCaseAttemptStatus {
    Pending,
    Running,
    WaitingForGrade,
    Completed,
    Failed,
    Inconclusive,
    Cancelled,
}

pub struct EvaluationCaseAttempt {
    pub id: EvaluationCaseAttemptId,
    pub evaluation_run_id: EvaluationRunId,
    pub case_id: EvaluationCaseId,
    pub repetition: u16,
    pub deterministic_seed: u64,
    pub produced_run_id: Option<AgentRunId>,
    pub replay_run_id: Option<ReplayRunId>,
    pub status: EvaluationCaseAttemptStatus,
    pub output_references: Vec<RunReference>,
    pub usage_snapshot_id: Option<ResourceUsageSnapshotId>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}
```

Repetitions are `1..=100`. Seeds are derived deterministically from EvaluationRun ID, case hash, repetition and environment seed.

### Grading plans and results

```rust
pub enum EvaluationGraderKind {
    Deterministic,
    Reference,
    Rubric,
    IndependentModel,
    PolicyCompliance,
}

pub struct GraderRequirement {
    pub stable_key: String,
    pub kind: EvaluationGraderKind,
    pub mandatory: bool,
    pub weight_basis_points: u16,
    pub grader_revision: String,
    pub configuration: serde_json::Value,
}

pub struct EvaluationGradingPlanRevision {
    pub id: EvaluationGradingPlanRevisionId,
    pub revision: u32,
    pub graders: Vec<GraderRequirement>,
    pub pass_threshold_basis_points: u16,
    pub disagreement_policy: GraderDisagreementPolicy,
    pub content_hash: [u8; 32],
}

pub enum GraderDisagreementPolicy {
    PreserveAndFailMandatory,
    RequireAdditionalIndependentGrade,
    RequireHumanReview,
}

pub enum GradeDisposition {
    Passed,
    Failed,
    Inconclusive,
    Error,
}

pub struct EvaluationGrade {
    pub id: EvaluationGradeId,
    pub attempt_id: EvaluationCaseAttemptId,
    pub grader_key: String,
    pub grader_kind: EvaluationGraderKind,
    pub grader_revision: String,
    pub disposition: GradeDisposition,
    pub score_basis_points: Option<u16>,
    pub evidence_references: Vec<RunReference>,
    pub failed_property_keys: Vec<String>,
    pub safe_rationale: Option<String>,
    pub output_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Weights total 10,000 for weighted graders. Mandatory deterministic/policy graders fail the case regardless of weighted score. `safe_rationale` is a concise result summary, not hidden reasoning.

### Metrics and reports

```rust
pub enum EvaluationMetricDirection { HigherIsBetter, LowerIsBetter, Exact }
pub enum EvaluationMetricAggregation { Sum, Mean, Median, Minimum, Maximum, RateBasisPoints }

pub enum EvaluationMetricUnit {
    Count,
    BasisPoints,
    Milliseconds,
    Bytes,
    Tokens,
    Microunits,
}

pub struct EvaluationMetricDefinitionRevision {
    pub id: EvaluationMetricDefinitionRevisionId,
    pub metric_id: EvaluationMetricDefinitionId,
    pub revision: u32,
    pub stable_name: String,
    pub direction: EvaluationMetricDirection,
    pub unit: EvaluationMetricUnit,
    pub aggregation: EvaluationMetricAggregation,
    pub content_hash: [u8; 32],
}

pub struct EvaluationMetricObservation {
    pub id: EvaluationMetricObservationId,
    pub evaluation_run_id: EvaluationRunId,
    pub case_attempt_id: Option<EvaluationCaseAttemptId>,
    pub metric_revision_id: EvaluationMetricDefinitionRevisionId,
    pub integer_value: i128,
    pub scale: u32,
    pub sample_count: u64,
    pub evidence_references: Vec<RunReference>,
    pub observed_at: Timestamp,
}
```

Required standard metrics:

```text
task_success_rate
criteria_pass_rate
factual_support_rate
tool_correctness_rate
policy_compliance_rate
false_success_rate
cost_per_successful_run
human_intervention_rate
memory_contamination_rate
verifier_disagreement_rate
remote_dispatch_unknown_rate
remote_reconciliation_success_rate
remote_input_wait_millis
remote_artifact_rejection_rate
remote_duplicate_prevention_count
inbound_a2a_intake_millis
```

### Risk-oriented verification

```rust
pub enum VerificationRiskClass { Low, Medium, High, Critical }

pub struct VerificationRiskInputs {
    pub maximum_action_risk: RiskLevel,
    pub side_effect_classes: std::collections::BTreeSet<ToolSideEffectClass>,
    pub maximum_classification: DataClassification,
    pub external_commitment_present: bool,
    pub unresolved_unknown_present: bool,
    pub evidence_trust_floor: ContextTrustClass,
    pub source_count: u32,
    pub novelty_basis_points: u16,
    pub factual_criticality_basis_points: u16,
    pub user_impact_basis_points: u16,
}

pub enum VerificationCheckKind {
    OutputSchema,
    DeterministicAssertions,
    EvidenceCoverage,
    ArtifactInspection,
    ExternalStateReadback,
    IndependentModel,
    CrossCheck,
    HumanReview,
    PolicyCompliance,
    BudgetReconciliation,
}

pub struct VerificationCheckRequirement {
    pub stable_key: String,
    pub kind: VerificationCheckKind,
    pub mandatory: bool,
    pub independence_requirement: Option<ModelIndependenceRequirement>,
    pub configuration_hash: [u8; 32],
}

pub struct VerificationPlanRevision {
    pub id: VerificationPlanRevisionId,
    pub plan_id: VerificationPlanId,
    pub run_id: AgentRunId,
    pub revision: u32,
    pub profile_revision_id: VerificationProfileRevisionId,
    pub risk_class: VerificationRiskClass,
    pub risk_inputs_hash: [u8; 32],
    pub checks: Vec<VerificationCheckRequirement>,
    pub maximum_correction_attempts: u16,
    pub correction_budget_allocation_id: BudgetAllocationId,
    pub content_hash: [u8; 32],
}

pub enum VerificationDisposition {
    Verified,
    VerifiedWithWarnings,
    NeedsCorrection,
    HumanReviewRequired,
    Failed,
    Inconclusive,
}

pub struct VerificationAttempt {
    pub id: VerificationAttemptId,
    pub plan_revision_id: VerificationPlanRevisionId,
    pub attempt_number: u16,
    pub disposition: VerificationDisposition,
    pub finding_ids: Vec<VerificationFindingId>,
    pub checked_references: Vec<RunReference>,
    pub unresolved_warning_codes: Vec<String>,
    pub created_at: Timestamp,
}
```

Minimum matrix:

```text
Low      → output schema + deterministic assertions + policy/budget checks
Medium   → Low + evidence coverage + Artifact inspection
High     → Medium + external state readback or independent model/cross-check
Critical → High + human review when exact profile/policy requires it
```

An unresolved `Unknown`, policy failure, required scanner outage or missing mandatory evidence yields `Inconclusive`/`Failed`, never Verified.

### Findings and correction

```rust
pub enum VerificationFindingSeverity { Info, Warning, Error, Critical }

pub struct VerificationFinding {
    pub id: VerificationFindingId,
    pub attempt_id: VerificationAttemptId,
    pub check_key: String,
    pub severity: VerificationFindingSeverity,
    pub stable_code: String,
    pub safe_message: String,
    pub evidence_references: Vec<RunReference>,
    pub correctable: bool,
    pub finding_hash: [u8; 32],
}

pub enum CorrectionActionKind {
    RevisePlan,
    RerunModelStep,
    RebuildArtifactRepresentation,
    RequestMissingEvidence,
    RequestHumanDecision,
    ReconcileUnknownOperation,
}

pub struct CorrectionRequest {
    pub id: CorrectionRequestId,
    pub run_id: AgentRunId,
    pub verification_attempt_id: VerificationAttemptId,
    pub action: CorrectionActionKind,
    pub finding_ids: Vec<VerificationFindingId>,
    pub maximum_additional_steps: u16,
    pub budget_allocation_id: BudgetAllocationId,
    pub status: CorrectionRequestStatus,
    pub created_at: Timestamp,
}

pub enum CorrectionRequestStatus {
    Prepared,
    Authorized,
    Scheduled,
    Completed,
    Failed,
    Cancelled,
}
```

Correction cannot directly repeat a Tool/remote write. It requests H5 replanning or H4/H9A reconciliation and uses fresh authorization for any new protected action.

### Regression gates

```rust
pub enum RegressionComparisonKind {
    MinimumAbsolute,
    MaximumAbsolute,
    MaximumRegressionBasisPoints,
    ExactMatch,
    HardZero,
}

pub struct RegressionMetricRule {
    pub metric_revision_id: EvaluationMetricDefinitionRevisionId,
    pub comparison: RegressionComparisonKind,
    pub threshold_integer: i128,
    pub threshold_scale: u32,
    pub minimum_samples: u64,
    pub mandatory: bool,
}

pub struct RegressionGateDefinitionRevision {
    pub id: RegressionGateDefinitionRevisionId,
    pub gate_id: RegressionGateDefinitionId,
    pub revision: u32,
    pub applicable_component_kinds: Vec<String>,
    pub dataset_revision_ids: Vec<EvaluationDatasetRevisionId>,
    pub metric_rules: Vec<RegressionMetricRule>,
    pub required_grader_keys: Vec<String>,
    pub maximum_baseline_age_days: u32,
    pub content_hash: [u8; 32],
}

pub struct RegressionBaselineRevision {
    pub id: RegressionBaselineRevisionId,
    pub baseline_id: RegressionBaselineId,
    pub revision: u32,
    pub component_snapshot_id: EvaluationComponentSnapshotId,
    pub evaluation_run_ids: Vec<EvaluationRunId>,
    pub aggregate_metric_hash: [u8; 32],
    pub content_hash: [u8; 32],
}

pub enum RegressionGateDisposition { Passed, PassedWithWarnings, Failed, Inconclusive }

pub struct RegressionGateEvidence {
    pub id: RegressionGateEvidenceId,
    pub gate_run_id: RegressionGateRunId,
    pub gate_revision_id: RegressionGateDefinitionRevisionId,
    pub candidate_component_snapshot_id: EvaluationComponentSnapshotId,
    pub baseline_revision_id: RegressionBaselineRevisionId,
    pub evaluation_run_ids: Vec<EvaluationRunId>,
    pub disposition: RegressionGateDisposition,
    pub failed_rule_keys: Vec<String>,
    pub dependency_hash: [u8; 32],
    pub valid_until: Timestamp,
    pub content_hash: [u8; 32],
}
```

Activation requires evidence for the exact candidate component/dependency hash. A passed gate cannot be reused after component, policy, dependency, dataset, grader or environment change.

### Replay and shadow

```rust
pub enum ReplayMode {
    ProjectionOnly,
    DeterministicEvaluation,
    ShadowModel,
}

pub enum ReplaySubstitutionKind {
    RecordedModelOutput,
    CandidateModel,
    RecordedToolObservation,
    RecordedRemoteObservation,
    RecordedHumanResponse,
    DeterministicClock,
    DeterministicRandom,
}

pub struct ReplaySubstitution {
    pub source_reference: RunReference,
    pub kind: ReplaySubstitutionKind,
    pub replacement_reference: Option<RunReference>,
    pub content_hash: [u8; 32],
}

pub struct ReplayManifest {
    pub id: ReplayManifestId,
    pub workspace_id: WorkspaceId,
    pub source_run_id: AgentRunId,
    pub source_event_from: u64,
    pub source_event_through: u64,
    pub context_snapshot_ids: Vec<ContextSnapshotId>,
    pub capture_record_ids: Vec<ObservabilityCaptureRecordId>,
    pub substitutions: Vec<ReplaySubstitution>,
    pub unresolved_unknown_references: Vec<RunReference>,
    pub manifest_hash: [u8; 32],
}

pub enum ReplayRunStatus {
    Created,
    Validating,
    Running,
    Completed,
    Failed,
    Inconclusive,
    NotReplayable,
    Cancelled,
}

pub struct ReplayRun {
    pub id: ReplayRunId,
    pub evaluation_run_id: Option<EvaluationRunId>,
    pub manifest_id: ReplayManifestId,
    pub mode: ReplayMode,
    pub environment_snapshot_id: EvaluationEnvironmentSnapshotId,
    pub status: ReplayRunStatus,
    pub budget_allocation_id: Option<BudgetAllocationId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Replay safety policy denies Tool execution, remote dispatch, credentials, exports, notifications, triggers, package/extension activation and memory writes. `ShadowModel` permits only H3 model calls through an evaluation-only policy/budget and feeds recorded observations for all effectful boundaries.

### User-facing Run explanation

```rust
pub enum RunExplanationSectionKind {
    Objective,
    Plan,
    Models,
    Tools,
    Delegations,
    Approvals,
    Sources,
    Artifacts,
    Verification,
    Budget,
    Warnings,
    MissingInformation,
}

pub struct RunExplanationSection {
    pub kind: RunExplanationSectionKind,
    pub safe_summary: String,
    pub references: Vec<RunReference>,
    pub omitted_count: u32,
    pub omission_codes: Vec<String>,
}

pub struct RunExplanationSnapshot {
    pub id: RunExplanationSnapshotId,
    pub workspace_id: WorkspaceId,
    pub run_id: AgentRunId,
    pub run_version: RunVersion,
    pub viewer_policy_hash: [u8; 32],
    pub sections: Vec<RunExplanationSection>,
    pub unresolved_warning_codes: Vec<String>,
    pub verification_attempt_id: Option<VerificationAttemptId>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Explanation is generated for an exact Run version and viewer policy. It reports decisions and evidence, not private deliberation. A purged source becomes a tombstone/omission rather than reconstructed text.

---

### Task 1: Add observability, capture and correlation domain contracts

**Files:** create observability domain modules, modify IDs and domain exports; add unit/property tests.

**Consumes:** H1 Run/step/event IDs; H3/H4/H5/H6/H9A invocation IDs; H8 service bindings.

**Produces:** `CorrelationContext`, capture modes/rules/policy revisions, exporter bindings, bounded metric points and projection cursor values.

- [ ] Write failing tests proving external trace context cannot replace the Vestrace trace ID or workspace.
- [ ] Write tests rejecting UUIDs, URLs, user strings and unbounded values as metric labels.
- [ ] Property-test capture-policy canonical hashing independent of rule insertion order after normalized sorting.
- [ ] Implement `ObservabilityCaptureMode`, `CaptureRule`, `ObservabilityCapturePolicyRevision`, `ObservabilityCaptureRecord`, `TelemetryExporterBindingRevision` and `MetricPoint` exactly as defined above.
- [ ] Implement validation: sample basis points `0..=10_000`, positive queue/batch/time limits, exporter maximum mode cannot exceed policy, standard OTLP requires HTTPS except explicit loopback fixture.
- [ ] Run and commit:

```bash
cargo test -p vestrace-domain observability::
cargo test --test telemetry_correlation --test telemetry_cardinality
git add crates/vestrace-domain tests/telemetry_correlation.rs tests/telemetry_cardinality.rs
git commit -m "feat(observability): add correlation and capture contracts"
```

### Task 2: Add audit-chain domain contracts and canonical hashing

**Files:** create audit domain modules and `vestrace-audit-runtime`; add chain/signature tests.

**Consumes:** H2 operation fingerprints/decisions; H8 external identity hashes; source journal references.

**Produces:** audit stream/record/checkpoint types, canonical record hash V1 and verification results.

- [ ] Write a failing golden-vector test for record hash V1 with a fixed canonical body.
- [ ] Write tests for first record zero previous hash, contiguous sequence, previous-hash mismatch, altered body and invalid checkpoint signature.
- [ ] Implement canonical body encoding with explicit domain separators and deterministic enum/string encoding.
- [ ] Implement `AuditChainBuilder::append(previous, body)` and `AuditIntegrityVerifier::verify(records, checkpoints, public_keys)`.
- [ ] Implement `AuditSigningPort` whose input is exact checkpoint bytes and whose output contains algorithm, key revision and signature; no secret types cross the port.
- [ ] Run and commit:

```bash
cargo test -p vestrace-audit-runtime
cargo test --test audit_chain_integrity --test audit_checkpoint_signing
git add crates/vestrace-domain/src/audit crates/vestrace-audit-runtime tests
git commit -m "feat(audit): add tamper-evident chain contracts"
```

### Task 3: Add evaluation, verification, regression, replay and explanation contracts

**Files:** create domain modules for evaluation/verification/regression/replay/explanation; modify IDs and exports; add transition/hash tests.

**Produces:** every immutable revision and aggregate type defined in the normative contracts.

- [ ] Write transition tests for EvaluationRun, case attempts, correction requests, ReplayRun and gate evidence expiry.
- [ ] Write tests proving mandatory deterministic/policy failure cannot be overridden by weighted score.
- [ ] Write tests for deterministic repetition seed derivation and component/environment snapshot hashes.
- [ ] Write tests that a replay manifest containing an effectful live substitution is rejected.
- [ ] Implement fixed-point metric values and checked aggregation inputs; reject scale over 18 and overflow.
- [ ] Implement all dataset, component, environment, grading, verification, correction, regression, replay and explanation types above.
- [ ] Run and commit:

```bash
cargo test -p vestrace-domain evaluation:: verification:: regression:: replay:: explanation::
cargo test --test evaluation_component_snapshot --test evaluation_environment_snapshot \
           --test verification_risk_matrix --test replay_side_effect_denial
git add crates/vestrace-domain tests
git commit -m "feat(evaluation): add durable evaluation and verification contracts"
```

### Task 4: Define application ports and deterministic test support

**Files:** create H10 application module skeletons and `vestrace-evaluation-test-support`.

**Interfaces:**

```rust
#[async_trait::async_trait]
pub trait TelemetryPort: Send + Sync {
    async fn emit_span(&self, span: NormalizedSpan) -> Result<(), TelemetryError>;
    async fn emit_metric(&self, point: MetricPoint) -> Result<(), TelemetryError>;
    async fn emit_log(&self, record: NormalizedLogRecord) -> Result<(), TelemetryError>;
}

#[async_trait::async_trait]
pub trait SecurityAuditPort: Send + Sync {
    async fn append_intent(
        &self,
        context: &RequestContext,
        intent: SecurityAuditIntent,
    ) -> Result<SecurityAuditRecord, ApplicationError>;
}

#[async_trait::async_trait]
pub trait EvaluationExecutorPort: Send + Sync {
    async fn execute_case(
        &self,
        context: &RequestContext,
        request: ExecuteEvaluationCase,
    ) -> Result<EvaluationCaseExecutionObservation, ApplicationError>;
}

#[async_trait::async_trait]
pub trait EvaluationGraderPort: Send + Sync {
    async fn grade(
        &self,
        context: &RequestContext,
        request: GradeEvaluationAttempt,
    ) -> Result<EvaluationGradeDraft, ApplicationError>;
}

#[async_trait::async_trait]
pub trait RunVerificationPort: Send + Sync {
    async fn verify(
        &self,
        context: &RequestContext,
        request: VerifyRunOutcome,
    ) -> Result<VerificationAttempt, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ComponentActivationGatePort: Send + Sync {
    async fn require_passing_evidence(
        &self,
        context: &RequestContext,
        request: RequireRegressionGateEvidence,
    ) -> Result<RegressionGateEvidence, ApplicationError>;
}

#[async_trait::async_trait]
pub trait ReplayExecutionPort: Send + Sync {
    async fn execute(
        &self,
        context: &RequestContext,
        request: ExecuteReplay,
    ) -> Result<ReplayExecutionObservation, ApplicationError>;
}
```

- [ ] Define normalized span/log/error DTOs with bounded fields and no exporter types.
- [ ] Define repositories for every H10 aggregate and append-only record.
- [ ] Compile-test object safety and Send/Sync for all ports.
- [ ] Add deterministic fixtures for exporter failure, signer failure, model grader disagreement, policy violation, false success, tool/remote substitutions, Unknown outcome, purge and worker crash.
- [ ] Add fault points after case claim, produced Run, each grade, metric aggregation, verification finding, audit record and replay checkpoint.
- [ ] Run and commit.

### Task 5: Implement the OpenTelemetry-compatible adapter and cardinality guard

**Files:** create `vestrace-observability-otel`, composition wiring and exporter tests; modify workspace features.

- [ ] Add optional features `telemetry-stdout` and `telemetry-otlp`; the default self-hosted test path uses in-memory/stdout and needs no collector.
- [ ] Implement a Vestrace-owned `TelemetryPort` adapter; OTel types do not enter domain/application signatures.
- [ ] Map `CorrelationContext` to trace/log fields. Metric labels pass through the strict `MetricLabelKey` validator.
- [ ] Use constant span names such as `run.step`, `model.invoke`, `tool.invoke`, `remote.dispatch`, `evaluation.case`; never use user content as span names.
- [ ] Add a bounded non-blocking queue. On overflow/export failure, increment local drop counters and return a swallowed operational error to the caller.
- [ ] Implement redaction before batching and again before exporter serialization.
- [ ] OTLP authentication uses one H8 service lease per export request; no token is retained in exporter configuration.
- [ ] Test collector outage, queue overflow, invalid label, proxy/redirect policy and exporter retry independence from Run work.
- [ ] Run and commit:

```bash
cargo test -p vestrace-observability-otel
cargo test --test telemetry_export_failure --test telemetry_cardinality
bash scripts/verify-telemetry-boundary.sh
bash scripts/verify-telemetry-cardinality.sh
git add Cargo.toml Cargo.lock crates/vestrace-observability-otel \
  crates/vestrace-application/src/composition tests scripts
git commit -m "feat(observability): add bounded telemetry adapter"
```

### Task 6: Persist capture policies, optional captures, exporters and operational projections

**Files:** create migration `0073`, observability repositories/services and capture/projection tests.

- [ ] `0073` creates capture-policy identities/revisions/rules, exporter identities/revisions/signals, capture records, capture-source links, projection cursors and bounded operational read-model tables.
- [ ] Enforce immutable policy/exporter revisions, one current revision pointer per identity and canonical hash uniqueness.
- [ ] Implement capture decision order: subject/classification → current H2 policy → known-secret scan → sampling → mode/size → H6 Artifact or structured metadata.
- [ ] Full/Redacted capture creates an H6 Artifact candidate with exact source references and retention; capture row becomes final only after Artifact quarantine result is known.
- [ ] A failed/rejected capture never blocks the source Run. It persists omission/failure metadata only when policy permits.
- [ ] Export service rechecks exporter maximum mode, destination, classification and current policy immediately before enqueue.
- [ ] Operational projection consumes durable source outboxes with a cursor and is idempotent by source ID/hash.
- [ ] Tests cover policy changes, sampling determinism, capture purge, exporter downgrade, projection restart and no RunVersion change.
- [ ] Run and commit.

### Task 7: Persist security audit records and signed integrity checkpoints

**Files:** create migration `0074`, audit writer/checkpoint/query/verification services and tests.

- [ ] `0074` creates audit streams, source-intent inbox, records, record-source links, integrity checkpoints, signing-key public metadata and verification reports.
- [ ] Protected subsystem transactions enqueue a `SecurityAuditIntent` with exact source ID/hash. The audit writer claims intents idempotently.
- [ ] Allocate sequence and append record under one stream row lock. Duplicate source intent returns the existing record.
- [ ] Force append-only triggers: no update/delete of records/checkpoints; mutable verification reports are separate.
- [ ] Create checkpoints every configured record count or maximum interval. Sign through H8-backed `AuditSigningPort`; only public verification metadata persists.
- [ ] Signer outage leaves records valid but checkpoint pending and emits an operational alert; it never fabricates a signature.
- [ ] Implement CLI/API integrity verification over a bounded range and optional source completeness check.
- [ ] Test concurrent append order, writer restart, source duplication, altered record detection, missing sequence, invalid signature and purged-source references.
- [ ] Run and commit.

### Task 8: Persist evaluation datasets, cases and exact snapshots

**Files:** create migration `0075`, dataset/component/environment services and persistence tests.

- [ ] `0075` creates dataset identities/revisions, cases, expected properties, prohibited outcomes, grading-plan revisions/graders, metric identities/revisions, component snapshots/dependency refs and environment snapshots/fixture refs.
- [ ] Import H9 package `EvaluationFixture` assets only from Available H6 Artifact revisions and exact package asset bindings.
- [ ] Validate schemas, stable keys, case budgets, grading weights, mandatory graders and duplicate case hashes before dataset revision finalization.
- [ ] Build component snapshots by traversing exact immutable revisions only; mutable health and current pointers are excluded.
- [ ] Build environment snapshots from source revision, migration head, Cargo.lock hash, feature hash, deployment profile, fixture revisions, clock/seed and capture policy.
- [ ] Prevent activation/runtime secrets, host paths and environment-variable values from entering snapshots.
- [ ] Tests prove identical normalized inputs produce identical hashes and any dependency/policy/fixture change produces a new snapshot.
- [ ] Run and commit.

### Task 9: Implement durable EvaluationRun execution and non-model graders

**Files:** create migration `0076`, evaluation runner/case worker/deterministic/reference/rubric graders, aggregation and report services.

- [ ] `0076` creates EvaluationRuns, case attempts, work leases/checkpoints, grades, metric observations, aggregate reports, exclusions and budget links.
- [ ] Create EvaluationRun only after exact dataset/component/environment validation and H2 budget allocation.
- [ ] Materialize one case attempt per `(case, repetition)` with deterministic seed and idempotent work item.
- [ ] Execute cases in isolated evaluation Runs or ReplayRuns according to case input. Production Runs are never mutated.
- [ ] Deterministic grader evaluates H5 success criteria, schemas, prohibited outcomes, policy/audit references and recorded operation status.
- [ ] Reference grader compares normalized exact/reference values and approved Artifact hashes/representations.
- [ ] Rubric grader uses a versioned deterministic rubric engine for typed criteria; arbitrary code or SQL is forbidden.
- [ ] Persist each grade before aggregation. Worker restart resumes missing graders only.
- [ ] Aggregate with fixed-point arithmetic. Mandatory fail/inconclusive rules take precedence over weighted threshold.
- [ ] Produce standard metrics and a bounded EvaluationReport Artifact with provenance.
- [ ] Test budget isolation, repetitions, crash after produced Run/grade, duplicate work, overflow, incomplete capture and partial report.
- [ ] Run and commit.

### Task 10: Implement independent-model grading and verifier disagreement

**Files:** create independent-model grader, H3 integration and tests; no new migration.

- [ ] Define an exact structured grading schema containing disposition, score, property results, evidence references and concise rationale.
- [ ] Build grader context from approved outputs/evidence only. Exclude producer scratchpad, hidden reasoning and unneeded conversation history.
- [ ] Route through H3 with exact minimum-quality and independence requirements from the grading-plan revision.
- [ ] Reserve/reconcile evaluation budget separately from the producer Run.
- [ ] Validate structured output, evidence references, score bounds and contradiction with deterministic facts.
- [ ] A model grade cannot change a deterministic/policy grade; disagreement is stored and processed by `GraderDisagreementPolicy`.
- [ ] Safety refusal, invalid output, budget exhaustion and provider Unknown produce Inconclusive/Error, not a guessed score.
- [ ] Tests cover different model/provider selection, producer scratchpad exclusion, unsupported independence, disagreement escalation and restart after model response.
- [ ] Run and commit.

### Task 11: Implement risk-oriented Run verification and bounded correction

**Files:** create migration `0077`, risk/planner/runner/completion-gate/correction services and tests.

- [ ] `0077` creates verification plan identities/revisions/checks, attempts, findings, checked-reference links, correction requests and correction-budget links.
- [ ] Compute risk inputs from actual plan/actions/classifications/commitments/Unknown states/evidence trust, not from model self-report alone.
- [ ] Resolve the exact H9 VerificationProfile revision and create the minimum check matrix. Model suggestions may append checks only.
- [ ] Execute deterministic checks first, then Artifact/evidence/readback, then independent model/cross-check/human review as required.
- [ ] External-state readback uses H4/H9A read/reconciliation operations under fresh authorization; it does not repeat the original write.
- [ ] Persist all findings and checked references before disposition.
- [ ] Integrate `RunCompletionGateService` with the authoritative H1 completion command: only allowed verification dispositions can create terminal success.
- [ ] `NeedsCorrection` creates one bounded `CorrectionRequest`; H5 replanning/new steps perform correction under fresh policy/budget.
- [ ] Enforce attempt count, additional steps, wall-clock and allocation ceilings. Mandatory criteria remain unchanged across corrections.
- [ ] Tests cover false-success prevention, unresolved Unknown, scanner outage, critical human review, correction exhaustion and restart.
- [ ] Run and commit.

### Task 12: Implement regression baselines, gates and activation evidence

**Files:** create migration `0078`, baseline/gate/evidence services, component-owner integrations and tests.

- [ ] `0078` creates baseline identities/revisions, gate identities/revisions/rules, gate runs, comparisons, activation evidence, replay manifests/runs/checkpoints and substitution rows.
- [ ] Baselines reference exact completed EvaluationRuns with compatible datasets/metrics/environments and minimum sample counts.
- [ ] Gate runner validates freshness and launches required candidate EvaluationRuns with exact seeds/repetitions.
- [ ] Compare integer/fixed-point metrics by explicit rules. Store numerator/denominator/sample data used for every comparison.
- [ ] Hard-fail policy compliance, unauthorized action, secret leak, duplicate side effect and false-success rules regardless of weighted quality.
- [ ] Produce immutable `RegressionGateEvidence` bound to exact candidate/dependency hash, gate revision, baseline and expiry.
- [ ] Add integration ports for H3 model binding, H4 Tool, H7 Trigger, H9 package/extension/remote activation and H9A route activation. Owners must call `require_passing_evidence` before creating an active revision.
- [ ] Evidence does not activate automatically and is rejected after any candidate/dependency/policy/dataset/grader/environment change.
- [ ] Tests cover stale baseline, insufficient samples, hard blocker, warning pass, evidence expiry, dependency change and concurrent activation attempts.
- [ ] Run and commit.

### Task 13: Implement safe replay and model-only shadow mode

**Files:** create replay manifest/runner/substitution/shadow/safety services and replay tests; use tables from `0078`.

- [ ] Build replay manifests from exact H1 event ranges, H3 context snapshots, H10 capture records and H4/H9A observations.
- [ ] Validate completeness and classify missing content as NotReplayable/Inconclusive before execution.
- [ ] `ProjectionOnly` reuses H1 logical replay and compares resulting state hashes without executing application effects.
- [ ] `DeterministicEvaluation` re-runs pure validators/graders with recorded model/tool/remote/human observations.
- [ ] `ShadowModel` invokes only the candidate H3 model under evaluation policy/budget and supplies recorded Tool/remote/human outputs.
- [ ] Install a deny-all replay implementation for Tool commit, remote dispatch, secret use, export, notification, trigger, activation and memory write ports; tests assert every call is denied before I/O.
- [ ] Preserve recorded Unknown and conflicting outcomes. No substitution may convert them to success without stored reconciliation evidence.
- [ ] Persist replay checkpoints after each event/step/grade so restart resumes without repeated model call or duplicated result.
- [ ] Compare canonical outputs/events/metrics against source and record differences without mutating source Run.
- [ ] Tests cover crash recovery, model-only allowance, attempted Tool/remote call, missing capture, purge, Unknown and no external network in deterministic modes.
- [ ] Run and commit.

### Task 14: Build Run explanations, operational dashboards and management surfaces

**Files:** create migration `0079`, explanation/dashboard repositories/services, HTTP/CLI routes and tests.

- [ ] `0079` creates Run explanation snapshots/sections/reference links, operational projection revisions, daily/hourly aggregates and projection cursors.
- [ ] Explanation builder reads exact H1 plan/events/checkpoint, H2 decisions/budgets/approvals, H3 routing/usage, H4 tools, H5 delegations, H6 evidence/Artifacts, H7 human waits, H8 connection-use audit, H9 snapshot and H9A remote records.
- [ ] Generate only supported statements with exact references and safe summaries. Mark missing, purged, policy-hidden and unresolved information explicitly.
- [ ] Viewer policy determines section visibility. A new viewer policy or RunVersion creates a new explanation snapshot.
- [ ] Exclude hidden reasoning, raw secrets, credential metadata, backend keys, unrestricted external text and sensitive policy internals.
- [ ] Build operational projections for Run throughput/latency/outcomes, queues, model/tool/sandbox usage, human waits, budgets, Artifact dispositions and remote-agent metrics.
- [ ] Metrics projection uses bounded dimensions and never stores raw content or high-cardinality labels.
- [ ] Add endpoints:

```text
GET  /v1/runs/{id}/explanation
GET  /v1/evaluation-datasets
POST /v1/evaluation-runs
GET  /v1/evaluation-runs/{id}
GET  /v1/evaluation-runs/{id}/report
GET  /v1/regression-gates/{id}/evidence
GET  /v1/audit/records
POST /v1/audit:verify-integrity
GET  /v1/operations/summary
```

- [ ] Add matching CLI commands with role/policy checks and bounded pagination.
- [ ] Tests cover explanation support, viewer redaction, purged evidence, unresolved warnings, projection lag/rebuild and remote metric cardinality.
- [ ] Run and commit.

### Task 15: Add Checkpoint V9, RLS, boundary scripts, CI and H10 acceptance

**Files:** create migration `0080`, modify Run work/event/checkpoint/composition, CI/scripts and acceptance/RLS tests.

- [ ] Add work kinds:

```text
CaptureObservabilitySubject
ExportTelemetryBatch
AppendSecurityAuditRecord
SealAuditIntegrityCheckpoint
RunEvaluationCase
GradeEvaluationCase
AggregateEvaluationRun
VerifyRunOutcome
ExecuteCorrectionRequest
RunRegressionGate
ExecuteReplay
BuildRunExplanation
RefreshOperationalProjection
```

- [ ] Work payloads contain IDs, exact revisions, hashes, cursors, deadlines and bounded safe codes only—no raw content, secret, credential handle, exporter token, prompt, remote frame or hidden reasoning.
- [ ] Extend H9A `RunCheckpointV8` to `RunCheckpointV9` with active verification plan/attempt IDs, pending correction request ID and latest explanation snapshot ID. Telemetry queues, audit chain state and evaluation case state are not embedded in Run checkpoints.
- [ ] Add a separate `EvaluationCheckpoint` containing case/repetition cursor, completed grader keys, aggregate cursor, replay checkpoint ID and budget snapshot ID.
- [ ] V8 and older Run checkpoints remain readable. Evaluation checkpoint schema is versioned independently.
- [ ] `0080` forces RLS, append-only/immutability triggers and indexes for capture source lookup, audit chain append/verify, evaluation queues, verification completion gate, gate evidence freshness, replay restart and explanation query.
- [ ] Boundary scripts reject:
  - OTel/exporter types outside adapter crates;
  - user content or IDs as metric labels;
  - optional capture without policy decision;
  - hidden reasoning/provider reasoning fields;
  - update/delete of audit records/checkpoints;
  - unsigned fabricated audit checkpoints;
  - model grade overriding deterministic/policy failure;
  - Run success without mandatory verification;
  - correction lowering mandatory criteria;
  - activation without exact gate evidence;
  - replay Tool/remote/credential/export/notification/activation/memory calls;
  - dashboards used as authoritative budget/Run state.
- [ ] Mandatory acceptance scenario:

```text
production-like AgentRun
→ correlated traces/metrics without content labels
→ protected model/tool/internal and A2A work
→ durable security audit source records
→ high-risk verification plan
→ deterministic checks + evidence + independent verifier
→ bounded correction
→ verified terminal outcome
→ user-facing explanation
→ capture-policy-aware evaluation replay
→ component regression comparison
→ activation denied after injected quality/policy regression
→ audit chain verification after worker restart
```

- [ ] Failure scenario injects telemetry exporter outage; Run completes normally and drop counters increase.
- [ ] Audit scenario alters a copied record/checkpoint and verifier detects the exact sequence/signature failure.
- [ ] Replay scenario proves recorded write/remote dispatch occurs exactly zero times while projection and grading complete.
- [ ] Remote scenario records dispatch, Unknown, reconciliation, input wait, Artifact rejection and duplicate prevention without credentials or external IDs as metric labels.
- [ ] Acceptance proves no hidden reasoning capture, no false success, no gate reuse after dependency change, no model grader override, content purge behavior and side-effect-free replay.
- [ ] Required CI commands:

```bash
bash scripts/verify-telemetry-boundary.sh
bash scripts/verify-telemetry-cardinality.sh
bash scripts/verify-audit-append-only.sh
bash scripts/verify-audit-integrity.sh
bash scripts/verify-evaluation-boundary.sh
bash scripts/verify-replay-side-effect-boundary.sh
bash scripts/verify-no-hidden-reasoning-capture.sh
bash scripts/verify-regression-gate-boundary.sh
cargo test --workspace --all-features
cargo test --workspace --no-default-features --features provider-openai-compatible
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h10_acceptance --test observability_evaluation_rls \
             --test audit_chain_integrity --test verification_completion_gate \
             --test replay_side_effect_denial --test regression_gate_activation
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

git add migrations/0080_observability_evaluation_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml crates scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(evaluation): add H10 integrity and regression gates"
```

---

## Migration ownership

```text
0073 Task 6  Capture policies, exporter bindings, capture records and operational projections
0074 Task 7  Security audit streams, source intents, records and integrity checkpoints
0075 Task 8  Evaluation datasets, cases, grading plans, metrics, component/environment snapshots
0076 Task 9  Evaluation runs, attempts, grades, metric observations and reports
0077 Task 11 Verification plans, attempts, findings and correction requests
0078 Task 12 Regression baselines/gates/evidence and replay manifests/runs/checkpoints
0079 Task 14 Run explanations and operational read models
0080 Task 15 RLS, indexes, append-only guards and Run/component bindings
```

No later task edits an applied migration.

## H10 completion definition

H10 is complete only when all fifteen tasks pass and the following flows are proven:

```text
Operational:
canonical subsystem records
→ bounded correlation
→ privacy/cardinality policy
→ best-effort trace/metric/log export
→ non-authoritative operational projection

Audit:
protected source transaction
→ durable audit intent
→ ordered append-only audit record
→ hash chain
→ H8-backed signed checkpoint
→ independent integrity verification

Evaluation:
immutable dataset revision
+ exact component snapshot
+ exact environment snapshot
+ H2 evaluation budget
→ durable repetitions
→ deterministic/reference/rubric/independent grading
→ fixed-point metrics
→ immutable report

Verification:
completed candidate outcome
→ deterministic risk classification
→ exact verification plan
→ ordered checks/readback/independent verification
→ bounded correction or verified disposition
→ authoritative H1 terminal command

Regression:
exact candidate snapshot
+ fresh baseline/gate
→ evaluation evidence
→ hard safety/policy blockers
→ immutable gate evidence
→ owning subsystem activation check

Replay:
source Run/events/captures
→ completeness validation
→ recorded substitutions
→ no effectful ports
→ optional model-only shadow
→ restart-safe comparison/report
```

Required invariants:

1. H1–H9A journals remain authoritative; H10 does not create a parallel execution truth.
2. Telemetry failure never changes Run behavior.
3. Metric labels have bounded cardinality and no content/IDs.
4. Optional content capture always has an exact policy decision and H6 lifecycle.
5. No capture mode stores hidden chain-of-thought or secret material.
6. Disabled capture still preserves mandatory audit/accounting references.
7. Audit intents are source-linked and idempotent.
8. Audit records/checkpoints are append-only and tamper-evident.
9. Checkpoint signing keys remain behind H8.
10. Evaluation datasets/components/environments are immutable and reproducible.
11. Evaluation Runs have isolated H2 budgets and cannot mutate tested components.
12. Deterministic/policy failures outrank model judgment.
13. Independent graders exclude producer scratchpad and satisfy exact independence policy.
14. Grader disagreement remains visible.
15. Fixed-point metrics avoid authoritative floating-point drift.
16. Run success requires mandatory verification.
17. Risk class cannot be reduced by a model.
18. Unknown effects reconcile before correction or verification success.
19. Correction is bounded and creates explicit new work/history.
20. Mandatory criteria cannot be weakened by correction.
21. Gate evidence is bound to exact candidate/dependencies and expires.
22. Gate evidence never activates a component automatically.
23. Policy/secret/duplicate/false-success regressions hard-fail gates.
24. Safe replay is distinct from logical replay.
25. Replay cannot execute Tools, remote agents, credentials, exports, notifications, activations or memory writes.
26. Shadow mode permits only explicitly authorized model calls.
27. Insufficient capture produces NotReplayable/Inconclusive, not invented data.
28. Run explanations cite exact records and expose no hidden reasoning.
29. Dashboards are lagging projections, not authority.
30. H9A remote metrics preserve correlation without credentials or high-cardinality external IDs.
31. Purge removes capture bytes while retaining content-free integrity/history references.
32. Worker restart does not duplicate audit records, case attempts, grades, verification attempts, gate evidence or replay model calls.
33. Replay and diagnostics perform no production side effects.
34. CI requires no public telemetry collector, model, remote agent or permanent credential.

## Explicit non-goals

H10 does not implement public multi-tenant analytics SaaS, unrestricted raw production tracing, hidden chain-of-thought capture, arbitrary SQL dashboards, automatic online prompt/router learning, self-modifying evaluation datasets, automatic activation after a passing gate, effectful canary execution, replay of external writes, remote-agent load testing against public services, probabilistic billing authority, permanent telemetry credentials, managed incident response, public benchmark publication or H11 product-console polish.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation-only phase, do not create `feat/h10-observability-evaluation`, add OpenTelemetry/exporter dependencies, create migrations `0073`–`0080`, start a collector, capture production content, sign checkpoints, run EvaluationRuns, invoke a model grader, execute replay/shadow mode, change component activation behavior, modify CI or run H10 tests.