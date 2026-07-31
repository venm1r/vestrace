# Vestrace H9 Agent Packages and Extension Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, install packages or extensions, start extension processes, import remote Agent Cards, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Extend the existing v0.1 cognitive-asset registry into a reproducible Agent composition system with explicit overlays, immutable `AgentRuntimeSnapshot` records, content-addressed Agent Packages, deterministic dependency locks, an isolated Extension Registry with permission-diff activation, transport-independent remote-agent definitions and three reference packages.

**Architecture:** H9 does not create a second registry for agents, skills or workflows. Existing v0.1 `agents`, `agent_revisions`, `skills`, `skill_revisions`, `workflows` and `workflow_revisions` remain authoritative identities and immutable revisions; H9 adds runtime extension records, profiles, overlays, compatibility reports and exact catalogue snapshots around them. Package installation, package activation, extension installation, extension activation and Run snapshot creation are separate operations. A package or manifest may declare requirements but cannot grant capabilities, activate H2 policies, create H8 Connections, trust a remote agent or change a running `AgentRuntimeSnapshot`. Third-party execution remains behind Vestrace-owned extension ports and H4/H8 enforcement boundaries. H9A later supplies the A2A transport adapter without changing H9 domain contracts.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H8 Rust workspace; Rust Edition 2024; Tokio; Serde/Schemars; SQLx and PostgreSQL 17; SHA-256 canonical hashes; JSON Schema; deterministic SemVer-compatible version requirements and exact lockfiles; Ed25519 package signature verification; H6 Artifact ingestion and archive representations; H4 sandbox/process execution; H8 service bindings and credential leases; Axum/Clap administrative surfaces; deterministic extension/package/remote-agent fixtures; proptest; tracing.

## Global Constraints

- Complete all five v0.1 plans and H1–H8 before implementing H9.
- Harness design sections `7. Универсальная модель агента`, `23. Extensions` and `24. Agent Profiles, Skills, Workflows и Packages` are normative.
- ADR-0003 is normative for remote-agent definitions and the future A2A adapter.
- Existing v0.1 cognitive identities and revisions remain authoritative. H9 may add companion rows and canonical read models but never creates parallel `agent_profiles_v2`, `skills_v2` or `workflows_v2` identities.
- Public H9 names `AgentProfileId` and `AgentProfileRevisionId` are aliases for existing `AgentId` and `AgentRevisionId`; existing IDs and foreign keys remain valid.
- PostgreSQL is authoritative for runtime profile extensions, overlays, memory/verification/delegation/communication profiles, compatibility reports, catalogue snapshots, Agent runtime snapshots, publishers, package revisions, dependency locks, installations, activations, extension manifests/revisions/activations, conformance reports, health/circuit-breaker state, remote-agent definitions/revisions/local activation revisions and permission diffs.
- Package archives, manifests as received, schemas, examples, evaluation datasets, remote declarations and executable extension payloads are H6 Artifact revisions. PostgreSQL stores bounded normalized metadata, hashes and exact Artifact references.
- Vestrace owns every domain type, lifecycle, manifest schema, compatibility rule, permission dimension, activation decision, protocol envelope and public DTO.
- Package-manager, dynamic-library, subprocess, HTTP, MCP, gRPC, A2A, signature-library and archive-parser types may not appear in domain/application signatures or PostgreSQL schemas.
- Agent, Skill, Workflow, profile, overlay, package, extension, remote-agent and catalogue revisions are immutable. Mutable lifecycle belongs to stable identities or separate installation/activation records with optimistic revisions.
- `AgentRuntimeSnapshot` is immutable and contains only exact revisions/hashes. It never stores secret material, credential leases, approval grants, active OAuth flows, mutable health state or hidden chain-of-thought.
- An active Run continues to reference its original snapshot after package/profile/extension updates. Current policy, revocation and operational availability are still rechecked at every enforcement point; snapshot immutability is reproducibility, not permanent authority.
- Skills, workflows, packages, extensions and remote declarations never grant permissions. Effective authority remains the intersection of base principal capability, H2 policy, profile ceiling, overlay narrowing, Run/delegation/autonomy ceilings, exact tool/connection grants and current operational state.
- Overlays are explicit, ordered and shallow. An overlay may preserve or narrow capabilities, tools, data classifications, budgets, delegation, autonomy, model/provider eligibility and memory scope; it may not widen them.
- Package install and activation are separate. Extension install and activation are separate. Remote declaration import and local activation are separate.
- Package activation never activates H2 policy bundles or templates. Packaged policy templates are installed inactive and require an independent H2-managed activation flow.
- Package archives and extension payloads enter through H6 quarantine/inspection. Archive paths, symlinks, devices, executable files, content types, size and expansion limits are enforced before parsing or execution.
- Active package dependency graphs are exact lockfiles. Version ranges are allowed only as installation requirements; Run snapshots never contain unresolved ranges.
- A package contains no secret, active Connection, credential reference, approval, authorization ticket, Run state, checkpoint or mutable external task state.
- Package signatures establish publisher-key verification only. They do not create trust, capability, data-transfer permission or activation approval.
- Third-party `TrustedInProcess` extensions are forbidden. `TrustedInProcess` is limited to built-in code compiled into the deployment and identified by a deployment-known digest.
- Rust dynamic libraries are not a public extension ABI.
- Initial extension protocols are built-in Rust registration, Vestrace JSON over stdio for external processes, Vestrace HTTP protocol and MCP bridge. gRPC is reserved but not required by H9.
- Unverified third-party extensions default to `SandboxedProcess` or `RemoteService`; `ExternalProcess` outside H4 isolation requires explicit Verified/Approved trust and deployment policy.
- Extension configuration contains no secret values. Secret requirements resolve to H8 logical service/Connection requirements and operation-bound leases at invocation time.
- Extension outputs, health text, remote declarations, package descriptions, examples and schemas are untrusted data.
- Activation requires compatibility validation and a complete permission diff. Any expansion requires an exact H2 authorization and administrator approval. Narrowing still requires an explicit activation command, though policy may waive approval.
- Extension health and circuit-breaker state are operational projections and never mutate immutable extension revisions or package locks.
- Category-specific owners remain authoritative: H3 owns model execution, H4 tools/sandboxes, H6 artifacts/processors, H7 channels/triggers/notifications and H8 credentials/connectors. H9 registers exact adapters and eligibility; it does not bypass those runtimes.
- An external autonomous agent is never a Tool Provider. It uses `RemoteAgentDefinitionRevision` and H5 `RemoteAgentInvocation`.
- Agent Cards and other remote declarations are untrusted compatibility input. A card cannot grant trust, skills, data access, transport permission or credentials.
- H9 stores no `a2a-rs` type. H9A may implement H9 discovery/runtime ports through an anti-corruption adapter.
- Replay never installs/activates a package, starts an extension, invokes conformance tests, fetches a remote declaration, changes trust, creates a snapshot or dispatches an external operation.
- Existing migrations `0014`–`0060` are never edited. H9 migrations are `0061`–`0067`, each created once by one task.
- CI uses local H6 package Artifacts, deterministic signed/unsigned packages, local process/HTTP/MCP extension fixtures and transport-neutral remote declarations. No public registry, marketplace, A2A service, signing service or permanent credential is required.
- Future implementation branch: `feat/h9-agent-packages-extension-registry`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  cognitive/{mod,agent,skill,workflow,profile_extension,overlay,compatibility}.rs
  agent_runtime/{mod,profile,memory_profile,verification_profile,delegation_profile,communication_policy,catalogue,snapshot,permission_diff}.rs
  package/{mod,manifest,publisher,dependency,lock,installation,activation,policy_template}.rs
  extension/{mod,manifest,definition,protocol,isolation,permission,activation,conformance,health,circuit_breaker,invocation}.rs
  remote_agent/{mod,definition,declaration,activation,trust,health,permission_diff}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  cognitive/{mod,ports,commands,profile_service,skill_service,workflow_service,overlay_service,compatibility}.rs
  agent_runtime/{mod,ports,commands,composition,snapshot_service,catalogue_service,permission_diff}.rs
  package/{mod,ports,commands,ingestion,parser,signature,dependency_resolver,installer,activation,rollback,reference_packages,worker}.rs
  extension/{mod,ports,commands,registry,installer,activation,runtime_host,conformance,health,circuit_breaker,category_registration,worker}.rs
  remote_agent/{mod,ports,commands,registry,import,activation,permission_diff,health}.rs

crates/vestrace-extension-runtime/src/
  lib.rs
  envelope.rs
  stdio_host.rs
  http_host.rs
  mcp_bridge.rs
  process.rs
  sandbox.rs
  registration.rs
  error.rs

crates/vestrace-package-runtime/src/
  lib.rs
  canonical_manifest.rs
  signature.rs
  lockfile.rs
  archive.rs
  error.rs

crates/vestrace-extension-test-support/src/
  lib.rs
  packages.rs
  signatures.rs
  stdio_extension.rs
  http_extension.rs
  mcp_extension.rs
  remote_agent.rs
  conformance.rs
  fixtures.rs
  faults.rs

crates/vestrace-channel-http/src/
  package_routes.rs
  extension_routes.rs
  agent_runtime_routes.rs
  remote_agent_routes.rs

crates/vestrace-channel-cli/src/
  package_commands.rs
  extension_commands.rs
  agent_commands.rs
  remote_agent_commands.rs

crates/vestrace-infrastructure/src/postgres/
  cognitive/{profile_extension_repository,overlay_repository,compatibility_repository}.rs
  agent_runtime/{profile_repository,catalogue_repository,snapshot_repository,permission_diff_repository}.rs
  package/{mod,publisher_repository,package_repository,lock_repository,installation_repository,activation_repository,policy_template_repository}.rs
  extension/{mod,definition_repository,activation_repository,conformance_repository,health_repository,circuit_breaker_repository}.rs
  remote_agent/{mod,definition_repository,activation_repository,health_repository}.rs

migrations/
  0061_agent_profile_extensions_overlays_and_skill_workflow_runtime.sql
  0062_memory_verification_delegation_and_communication_profiles.sql
  0063_agent_runtime_snapshots_and_catalogues.sql
  0064_agent_packages_publishers_lockfiles_and_activations.sql
  0065_extension_registry_activations_conformance_and_health.sql
  0066_remote_agent_definitions_revisions_and_permission_diffs.sql
  0067_package_extension_rls_indexes_and_run_bindings.sql

tests/
  cognitive_runtime_extensions.rs
  agent_overlay_narrowing.rs
  workflow_runtime_compatibility.rs
  profile_persistence.rs
  agent_runtime_snapshot.rs
  agent_runtime_snapshot_immutability.rs
  package_manifest_validation.rs
  package_signature_verification.rs
  package_dependency_resolution.rs
  package_installation_restart.rs
  package_activation_permission_diff.rs
  package_rollback.rs
  extension_registry_persistence.rs
  extension_activation_permission_diff.rs
  extension_stdio_conformance.rs
  extension_http_conformance.rs
  extension_mcp_conformance.rs
  extension_sandbox_isolation.rs
  extension_health_circuit_breaker.rs
  remote_agent_definition_persistence.rs
  remote_agent_permission_diff.rs
  remote_agent_card_change.rs
  runtime_snapshot_run_binding.rs
  package_extension_rls.rs
  h9_acceptance.rs

packages/reference/
  universal-assistant/vestrace-package.json
  universal-assistant/schemas/
  universal-assistant/evaluations/
  research/vestrace-package.json
  research/schemas/
  research/evaluations/
  workspace-automation/vestrace-package.json
  workspace-automation/schemas/
  workspace-automation/evaluations/

scripts/
  verify-cognitive-registry-boundary.sh
  verify-package-boundary.sh
  verify-extension-boundary.sh
  verify-extension-permissions.sh
  verify-remote-agent-boundary.sh
```

---

## Normative contracts

### Compatibility and exact revision references

```rust
pub struct PackageFormatVersion(pub u16);
pub struct VestraceCoreApiVersion(pub String);
pub struct ExtensionProtocolVersion(pub String);

pub struct VersionRequirement {
    pub normalized_expression: String,
}

pub struct ExactRevisionRef {
    pub kind: String,
    pub stable_id: String,
    pub revision_id: uuid::Uuid,
    pub content_hash: [u8; 32],
}

pub enum CompatibilityDisposition {
    Compatible,
    CompatibleWithWarnings,
    Incompatible,
}

pub struct CompatibilityFinding {
    pub code: String,
    pub severity: RiskLevel,
    pub subject: ExactRevisionRef,
    pub related: Option<ExactRevisionRef>,
    pub safe_message: String,
}

pub struct CompatibilityReport {
    pub id: CompatibilityReportId,
    pub workspace_id: WorkspaceId,
    pub disposition: CompatibilityDisposition,
    pub inputs_hash: [u8; 32],
    pub findings: Vec<CompatibilityFinding>,
    pub created_at: Timestamp,
}
```

Version requirements are parsed and normalized at write time. Active locks and snapshots contain only exact revision IDs and hashes. Compatibility reports are immutable evidence; a changed dependency, policy, catalogue or runtime capability requires a new report.

### Promotion of existing v0.1 cognitive assets

H9 uses the existing identities:

```rust
pub type AgentProfileId = AgentId;
pub type AgentProfileRevisionId = AgentRevisionId;
```

The existing v0.1 `AgentDefinition`, `AgentRevision`, `SkillDefinition`, `SkillRevision`, `WorkflowDefinition` and `WorkflowRevision` remain authoritative. H9 adds companion immutable rows:

```rust
pub struct AgentProfileRuntimeExtensionRevision {
    pub id: AgentProfileRuntimeExtensionRevisionId,
    pub agent_revision_id: AgentRevisionId,
    pub objective_classes: std::collections::BTreeSet<String>,
    pub routing_policy: RoutingPolicySnapshotRef,
    pub memory_profile_revision_id: MemoryProfileRevisionId,
    pub verification_profile_revision_id: VerificationProfileRevisionId,
    pub delegation_profile_revision_id: DelegationProfileRevisionId,
    pub communication_policy_revision_id: CommunicationPolicyRevisionId,
    pub context_policy_revision_id: ContextAssemblyPolicyRevisionId,
    pub default_workflow_revision_ids: Vec<WorkflowRevisionId>,
    pub permitted_workflow_families: std::collections::BTreeSet<String>,
    pub autonomy_ceiling: TriggerAutonomyLevel,
    pub capability_ceiling: std::collections::BTreeSet<Capability>,
    pub maximum_data_classification: DataClassification,
    pub runtime_budget_template: ResourceBudgetTemplate,
    pub content_hash: [u8; 32],
}

pub struct RoutingPolicySnapshotRef {
    pub routing_policy_id: RoutingPolicyId,
    pub content_hash: [u8; 32],
}
```

A companion revision is valid only for one exact existing `AgentRevision`. It does not overwrite role, instructions, skills or other v0.1 fields.

### Explicit profile overlays

```rust
pub struct AgentProfileOverlayRevision {
    pub id: AgentProfileOverlayRevisionId,
    pub overlay_id: AgentProfileOverlayId,
    pub revision: u32,
    pub base_agent_revision_id: AgentRevisionId,
    pub name: String,
    pub capability_limit: Option<std::collections::BTreeSet<Capability>>,
    pub allowed_tool_revision_ids: Option<std::collections::BTreeSet<ToolRevisionId>>,
    pub allowed_skill_revision_ids: Option<std::collections::BTreeSet<SkillRevisionId>>,
    pub maximum_data_classification: Option<DataClassification>,
    pub autonomy_ceiling: Option<TriggerAutonomyLevel>,
    pub budget_limit: Option<ResourceBudgetTemplate>,
    pub delegation_limit: Option<DelegationProfileRestriction>,
    pub provider_limit: Option<ModelProviderRestriction>,
    pub memory_limit: Option<MemoryProfileRestriction>,
    pub content_hash: [u8; 32],
}
```

Composition supports one base profile plus at most eight ordered overlays. Every overlay is independently proven to be monotonic narrowing relative to the accumulated result. Deep inheritance and overlay-to-overlay parent references are rejected.

### Memory, verification, delegation and communication profiles

```rust
pub struct MemoryProfileRevision {
    pub id: MemoryProfileRevisionId,
    pub profile_id: MemoryProfileId,
    pub revision: u32,
    pub readable_scope_kinds: std::collections::BTreeSet<MemoryScopeKind>,
    pub retrieval_policy: MemoryRetrievalPolicy,
    pub context_limit: MemoryContextLimit,
    pub candidate_write_policy: MemoryCandidateWritePolicy,
    pub consolidation_policy: RunConsolidationPolicy,
    pub maximum_classification: DataClassification,
    pub content_hash: [u8; 32],
}

pub struct VerificationProfileRevision {
    pub id: VerificationProfileRevisionId,
    pub profile_id: VerificationProfileId,
    pub revision: u32,
    pub required_verifiers: Vec<VerifierRequirement>,
    pub independence: ModelIndependenceRequirement,
    pub allow_producer_scratchpad: bool,
    pub maximum_correction_rounds: u16,
    pub failure_disposition: VerificationFailureDisposition,
    pub content_hash: [u8; 32],
}

pub struct DelegationProfileRevision {
    pub id: DelegationProfileRevisionId,
    pub profile_id: DelegationProfileId,
    pub revision: u32,
    pub maximum_depth: u16,
    pub maximum_parallel_subruns: u16,
    pub internal_target_requirements: Vec<AgentTargetRequirement>,
    pub remote_target_requirements: Vec<RemoteAgentRequirement>,
    pub require_handoff_verification: bool,
    pub maximum_remote_classification: DataClassification,
    pub content_hash: [u8; 32],
}

pub struct CommunicationPolicyRevision {
    pub id: CommunicationPolicyRevisionId,
    pub policy_id: CommunicationPolicyId,
    pub revision: u32,
    pub allowed_channel_kinds: std::collections::BTreeSet<InteractionChannelKind>,
    pub default_notification_detail: NotificationDetailLevel,
    pub may_request_approval: bool,
    pub may_request_authentication: bool,
    pub maximum_shared_classification: DataClassification,
    pub content_hash: [u8; 32],
}
```

These profiles describe agent behavior and requirements. They do not bypass H2, H6, H7 or H8 enforcement.

### Skill and Workflow runtime extensions

```rust
pub struct SkillRuntimeExtensionRevision {
    pub id: SkillRuntimeExtensionRevisionId,
    pub skill_revision_id: SkillRevisionId,
    pub activation_conditions: serde_json::Value,
    pub model_requirements: ModelInvocationRequirementsTemplate,
    pub required_tool_revision_ids: std::collections::BTreeSet<ToolRevisionId>,
    pub required_capabilities: std::collections::BTreeSet<Capability>,
    pub memory_pattern: MemoryUsagePattern,
    pub verification_profile_revision_id: VerificationProfileRevisionId,
    pub example_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub evaluation_dataset_revision_ids: Vec<EvaluationDatasetRevisionId>,
    pub tests: Vec<SkillConformanceCase>,
    pub content_hash: [u8; 32],
}

pub struct WorkflowRuntimeBindingRevision {
    pub id: WorkflowRuntimeBindingRevisionId,
    pub workflow_revision_id: WorkflowRevisionId,
    pub execution_mode: ExecutionMode,
    pub h5_step_bindings: Vec<WorkflowNodeRuntimeBinding>,
    pub budget_template: ResourceBudgetTemplate,
    pub policy_requirements: Vec<PolicyRequirement>,
    pub completion_contract: WorkflowCompletionContract,
    pub content_hash: [u8; 32],
}
```

A Skill declares requirements; it never grants them. A Workflow remains an immutable stored graph; H5 compiles its exact runtime binding into an `ExecutionPlanRevision`. Runtime state remains in `AgentRun` and `RunStep`.

### Permission diff

```rust
pub enum PermissionDiffDisposition {
    NoChange,
    Narrowing,
    Expansion,
    Incompatible,
}

pub enum PermissionDimension {
    Capability,
    Tool,
    ConnectorOperation,
    ConnectorResource,
    SecretRequirement,
    NetworkOrigin,
    ModelProvider,
    DataTransfer,
    DataClassification,
    ArtifactOperation,
    TriggerSource,
    Autonomy,
    Delegation,
    RemoteAgent,
    SandboxIsolation,
    Channel,
    Notification,
}

pub struct PermissionDiffEntry {
    pub dimension: PermissionDimension,
    pub key: String,
    pub before_hash: Option<[u8; 32]>,
    pub after_hash: Option<[u8; 32]>,
    pub disposition: PermissionDiffDisposition,
    pub explanation: String,
}

pub struct PermissionDiff {
    pub id: PermissionDiffId,
    pub workspace_id: WorkspaceId,
    pub subject_before: Option<ExactRevisionRef>,
    pub subject_after: ExactRevisionRef,
    pub disposition: PermissionDiffDisposition,
    pub entries: Vec<PermissionDiffEntry>,
    pub content_hash: [u8; 32],
    pub created_at: Timestamp,
}
```

Any single expansion makes the overall diff `Expansion`. Unknown or uncomparable permission semantics are `Incompatible`, not `NoChange`.

### Exact catalogues and AgentRuntimeSnapshot

```rust
pub struct ToolCatalogueSnapshot {
    pub id: ToolCatalogueSnapshotId,
    pub workspace_id: WorkspaceId,
    pub tool_revision_ids: Vec<ToolRevisionId>,
    pub visible_views_hash: [u8; 32],
    pub content_hash: [u8; 32],
}

pub struct ExtensionCatalogueEntry {
    pub extension_revision_id: ExtensionDefinitionRevisionId,
    pub activation_revision_id: ExtensionActivationRevisionId,
    pub categories: std::collections::BTreeSet<ExtensionCategory>,
}

pub struct ExtensionCatalogueSnapshot {
    pub id: ExtensionCatalogueSnapshotId,
    pub workspace_id: WorkspaceId,
    pub entries: Vec<ExtensionCatalogueEntry>,
    pub content_hash: [u8; 32],
}

pub struct RemoteAgentCatalogueEntry {
    pub remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
    pub local_activation_revision_id: RemoteAgentActivationRevisionId,
}

pub struct RemoteAgentCatalogueSnapshot {
    pub id: RemoteAgentCatalogueSnapshotId,
    pub workspace_id: WorkspaceId,
    pub entries: Vec<RemoteAgentCatalogueEntry>,
    pub content_hash: [u8; 32],
}

pub struct AgentRuntimeSnapshot {
    pub id: AgentRuntimeSnapshotId,
    pub workspace_id: WorkspaceId,
    pub agent_revision_id: AgentRevisionId,
    pub runtime_extension_revision_id: AgentProfileRuntimeExtensionRevisionId,
    pub overlay_revision_ids: Vec<AgentProfileOverlayRevisionId>,
    pub skill_revision_ids: Vec<SkillRevisionId>,
    pub skill_runtime_extension_revision_ids: Vec<SkillRuntimeExtensionRevisionId>,
    pub workflow_revision_id: Option<WorkflowRevisionId>,
    pub workflow_runtime_binding_revision_id: Option<WorkflowRuntimeBindingRevisionId>,
    pub routing_policy: RoutingPolicySnapshotRef,
    pub policy_bundle_revision_ids: Vec<PolicyBundleRevisionId>,
    pub memory_profile_revision_id: MemoryProfileRevisionId,
    pub verification_profile_revision_id: VerificationProfileRevisionId,
    pub delegation_profile_revision_id: DelegationProfileRevisionId,
    pub communication_policy_revision_id: CommunicationPolicyRevisionId,
    pub context_policy_revision_id: ContextAssemblyPolicyRevisionId,
    pub tool_catalogue_snapshot_id: ToolCatalogueSnapshotId,
    pub extension_catalogue_snapshot_id: ExtensionCatalogueSnapshotId,
    pub remote_agent_catalogue_snapshot_id: RemoteAgentCatalogueSnapshotId,
    pub package_lock_id: Option<AgentPackageLockId>,
    pub effective_capability_ceiling: std::collections::BTreeSet<Capability>,
    pub effective_autonomy_ceiling: TriggerAutonomyLevel,
    pub effective_maximum_classification: DataClassification,
    pub effective_budget_template: ResourceBudgetTemplate,
    pub compatibility_report_id: CompatibilityReportId,
    pub content_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}
```

All vectors are canonically sorted and duplicate-free before hashing. Snapshot construction is one transaction after validation. A snapshot contains no active Connection ID; connection eligibility is resolved by H8 per Run/step through logical requirements and grants.

### Agent Package manifest, publisher and lock

```rust
pub enum PublisherTrustLevel { Builtin, Verified, Approved, Unverified, Blocked }

pub struct PackagePublisherRevision {
    pub id: PackagePublisherRevisionId,
    pub publisher_id: PackagePublisherId,
    pub revision: u32,
    pub display_name: String,
    pub signing_public_keys: Vec<PackageSigningKey>,
    pub trust: PublisherTrustLevel,
    pub content_hash: [u8; 32],
}

pub struct PackageDependencyRequirement {
    pub package_stable_id: String,
    pub version_requirement: VersionRequirement,
    pub optional: bool,
}

pub struct AgentPackageManifest {
    pub format_version: PackageFormatVersion,
    pub stable_id: String,
    pub version: String,
    pub publisher_id: PackagePublisherId,
    pub minimum_core_api: VersionRequirement,
    pub dependencies: Vec<PackageDependencyRequirement>,
    pub profile_revisions: Vec<ExactRevisionRef>,
    pub skill_revisions: Vec<ExactRevisionRef>,
    pub workflow_revisions: Vec<ExactRevisionRef>,
    pub profile_support_revisions: Vec<ExactRevisionRef>,
    pub policy_template_revisions: Vec<ExactRevisionRef>,
    pub schema_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub evaluation_dataset_revision_ids: Vec<EvaluationDatasetRevisionId>,
    pub example_artifact_revision_ids: Vec<ArtifactRevisionId>,
    pub extension_requirements: Vec<ExtensionRequirement>,
    pub remote_agent_requirements: Vec<RemoteAgentRequirement>,
    pub requested_permissions: RequestedPermissionSet,
    pub files: Vec<PackageFileDigest>,
    pub manifest_digest: [u8; 32],
    pub signature: Option<PackageSignature>,
}

pub struct AgentPackageLock {
    pub id: AgentPackageLockId,
    pub root_package_revision_id: AgentPackageRevisionId,
    pub resolved_package_revision_ids: Vec<AgentPackageRevisionId>,
    pub resolved_extension_revision_ids: Vec<ExtensionDefinitionRevisionId>,
    pub resolved_remote_agent_revision_ids: Vec<RemoteAgentDefinitionRevisionId>,
    pub resolution_inputs_hash: [u8; 32],
    pub content_hash: [u8; 32],
}
```

The initial package format is canonical UTF-8 JSON named `vestrace-package.json`. Every listed file has a normalized relative path, media type, size and SHA-256. Absolute paths, `..`, duplicate normalized paths, links, devices and unlisted files are rejected. Signature verification signs the canonical manifest digest and file digest list; activation trust is still local policy.

### Package lifecycle

```rust
pub enum AgentPackageStatus { Draft, Installed, Deprecated, Revoked }
pub enum PackageInstallationStatus { Quarantined, Inspecting, Parsed, Compatible, Incompatible, Failed, Unknown }
pub enum PackageActivationStatus { Prepared, AwaitingApproval, Active, Superseded, Disabled, Revoked }

pub struct AgentPackageRevision {
    pub id: AgentPackageRevisionId,
    pub package_id: AgentPackageId,
    pub revision: u32,
    pub version: String,
    pub source_artifact_revision_id: ArtifactRevisionId,
    pub manifest_artifact_revision_id: ArtifactRevisionId,
    pub publisher_revision_id: PackagePublisherRevisionId,
    pub signature_status: PackageSignatureStatus,
    pub requested_permissions_hash: [u8; 32],
    pub content_hash: [u8; 32],
}

pub struct PackageActivationRevision {
    pub id: PackageActivationRevisionId,
    pub activation_id: PackageActivationId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub package_revision_id: AgentPackageRevisionId,
    pub lock_id: AgentPackageLockId,
    pub permission_diff_id: PermissionDiffId,
    pub compatibility_report_id: CompatibilityReportId,
    pub approval_grant_id: Option<ApprovalGrantId>,
    pub status: PackageActivationStatus,
    pub content_hash: [u8; 32],
}
```

Rollback creates a new activation revision pointing to a previously installed exact package/lock. It never deletes or rewrites the superseded activation.

### Extension manifest and lifecycle

```rust
pub enum ExtensionCategory {
    ToolProvider,
    Connector,
    ChannelAdapter,
    ModelProvider,
    ArtifactProcessor,
    SandboxProvider,
    Evaluator,
    TriggerSource,
    NotificationProvider,
    A2AAdapter,
}

pub enum ExtensionIsolationMode { TrustedInProcess, ExternalProcess, SandboxedProcess, RemoteService }

pub enum ExtensionRuntimeProtocol {
    BuiltinRust,
    VestraceJsonStdio { version: ExtensionProtocolVersion },
    VestraceHttp { version: ExtensionProtocolVersion },
    McpBridge { version: String },
    GrpcReserved { version: ExtensionProtocolVersion },
}

pub enum ExtensionTrustLevel { Builtin, Verified, Approved, Unverified, Blocked }
pub enum ExtensionStatus { Installed, Disabled, Quarantined, Active, Degraded, Revoked, Incompatible }

pub struct ExtensionManifest {
    pub stable_id: String,
    pub version: String,
    pub publisher_revision_id: PackagePublisherRevisionId,
    pub categories: std::collections::BTreeSet<ExtensionCategory>,
    pub protocol: ExtensionRuntimeProtocol,
    pub minimum_core_api: VersionRequirement,
    pub isolation: ExtensionIsolationMode,
    pub declared_capabilities: std::collections::BTreeSet<String>,
    pub requested_permissions: RequestedPermissionSet,
    pub operations: Vec<ExtensionOperationDefinition>,
    pub configuration_schema: serde_json::Value,
    pub secret_requirements: Vec<ExtensionSecretRequirement>,
    pub network_requirements: Vec<NormalizedOrigin>,
    pub data_handling: ConnectorDataHandlingDeclaration,
    pub health_contract: ExtensionHealthContract,
    pub executable_artifact_revision_id: Option<ArtifactRevisionId>,
    pub integrity_digest: [u8; 32],
}

pub struct ExtensionDefinitionRevision {
    pub id: ExtensionDefinitionRevisionId,
    pub extension_id: ExtensionDefinitionId,
    pub revision: u32,
    pub manifest: ExtensionManifest,
    pub trust: ExtensionTrustLevel,
    pub content_hash: [u8; 32],
}

pub struct ExtensionActivationRevision {
    pub id: ExtensionActivationRevisionId,
    pub activation_id: ExtensionActivationId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub extension_revision_id: ExtensionDefinitionRevisionId,
    pub permission_diff_id: PermissionDiffId,
    pub compatibility_report_id: CompatibilityReportId,
    pub conformance_report_id: ExtensionConformanceReportId,
    pub approval_grant_id: Option<ApprovalGrantId>,
    pub status: ExtensionStatus,
    pub configuration: serde_json::Value,
    pub service_binding_revision_ids: Vec<ServiceCredentialBindingRevisionId>,
    pub content_hash: [u8; 32],
}
```

Configuration is validated against the manifest schema and rejects fields marked or detected as secret values. Secret/service requirements are satisfied through H8 references selected by trusted administration, never embedded configuration.

### Extension invocation and runtime ownership

```rust
pub struct ExtensionInvocationEnvelope {
    pub invocation_id: ExtensionInvocationId,
    pub workspace_id: WorkspaceId,
    pub extension_revision_id: ExtensionDefinitionRevisionId,
    pub activation_revision_id: ExtensionActivationRevisionId,
    pub category: ExtensionCategory,
    pub operation: String,
    pub normalized_input: serde_json::Value,
    pub authorization_reference: AuthorizationTicketId,
    pub credential_lease_handles: Vec<CredentialLeaseHandle>,
    pub artifact_handles: Vec<ArtifactAccessHandle>,
    pub deadline: Timestamp,
    pub idempotency_key: String,
    pub output_contract: serde_json::Value,
}

pub struct ExtensionInvocationObservation {
    pub status: ExtensionInvocationStatus,
    pub normalized_output: Option<serde_json::Value>,
    pub external_operation_id: Option<String>,
    pub reconciliation_reference: Option<String>,
    pub completion_may_have_occurred: bool,
    pub safe_error: Option<ExtensionError>,
}
```

The category owner constructs the envelope after its own H2/H8/H4 checks. The extension host cannot reinterpret tickets or grant itself an operation. `CredentialLeaseHandle` and `ArtifactAccessHandle` are opaque one-purpose references consumed by infrastructure; they are not secrets or unrestricted storage references.

### Extension conformance, health and circuit breaker

```rust
pub enum ExtensionConformanceDisposition { Passed, PassedWithWarnings, Failed }

pub struct ExtensionConformanceReport {
    pub id: ExtensionConformanceReportId,
    pub extension_revision_id: ExtensionDefinitionRevisionId,
    pub protocol_version: String,
    pub category_results: Vec<ExtensionCategoryConformanceResult>,
    pub isolation_result: ExtensionIsolationResult,
    pub secret_boundary_result: BoundaryCheckResult,
    pub permission_boundary_result: BoundaryCheckResult,
    pub disposition: ExtensionConformanceDisposition,
    pub inputs_hash: [u8; 32],
    pub created_at: Timestamp,
}

pub struct ExtensionHealthObservation {
    pub id: ExtensionHealthObservationId,
    pub activation_revision_id: ExtensionActivationRevisionId,
    pub status: ExtensionHealthStatus,
    pub capability_hash: [u8; 32],
    pub safe_code: String,
    pub observed_at: Timestamp,
}

pub struct ExtensionCircuitBreakerState {
    pub activation_revision_id: ExtensionActivationRevisionId,
    pub state: CircuitBreakerState,
    pub consecutive_failures: u32,
    pub opened_until: Option<Timestamp>,
    pub state_revision: u64,
}
```

Health-reported capabilities must be a subset of the activated manifest. A changed or expanded report cannot silently alter registration and requires a new extension revision/activation.

### Remote-agent declarations and local activation

```rust
pub enum RemoteAgentTrustLevel { Verified, Approved, Unverified, Blocked }
pub enum RemoteAgentStatus { Candidate, Inactive, Active, Degraded, Revoked, Incompatible }

pub struct RemoteAgentDeclaredInterface {
    pub transport: String,
    pub endpoint_origin: NormalizedOrigin,
    pub supports_streaming: bool,
}

pub struct RemoteAgentDeclaredSkill {
    pub stable_id: String,
    pub description_hash: [u8; 32],
    pub input_schema_hash: Option<[u8; 32]>,
    pub output_schema_hash: Option<[u8; 32]>,
}

pub struct RemoteAgentDefinitionRevision {
    pub id: RemoteAgentDefinitionRevisionId,
    pub remote_agent_id: RemoteAgentDefinitionId,
    pub revision: u32,
    pub source_locator_hash: [u8; 32],
    pub declaration_artifact_revision_id: ArtifactRevisionId,
    pub declared_name: String,
    pub declared_version: Option<String>,
    pub declared_interfaces: Vec<RemoteAgentDeclaredInterface>,
    pub declared_skills: Vec<RemoteAgentDeclaredSkill>,
    pub declared_security_schemes_hash: [u8; 32],
    pub verified_external_identity_hash: Option<[u8; 32]>,
    pub declaration_content_hash: [u8; 32],
    pub retrieved_at: Timestamp,
}

pub struct RemoteAgentActivationRevision {
    pub id: RemoteAgentActivationRevisionId,
    pub activation_id: RemoteAgentActivationId,
    pub workspace_id: WorkspaceId,
    pub revision: u32,
    pub remote_agent_revision_id: RemoteAgentDefinitionRevisionId,
    pub trust: RemoteAgentTrustLevel,
    pub allowed_transports: std::collections::BTreeSet<String>,
    pub allowed_skills: std::collections::BTreeSet<String>,
    pub maximum_classification: DataClassification,
    pub required_verified_identity_hash: Option<[u8; 32]>,
    pub remote_connection_profile_revision_id: Option<RemoteAgentConnectionProfileRevisionId>,
    pub permission_diff_id: PermissionDiffId,
    pub approval_grant_id: Option<ApprovalGrantId>,
    pub status: RemoteAgentStatus,
    pub content_hash: [u8; 32],
}
```

The declaration revision records observed remote claims; the activation revision records local trust and allowlists. Changing either creates a new immutable revision. H9 does not fetch A2A itself; a `RemoteAgentDiscoveryPort` supplies a bounded H6 Artifact plus normalized candidate. H9A later implements that port.

---

### Task 1: Promote existing cognitive assets and add compatibility primitives

**Files:** modify `id.rs`, existing cognitive modules and create `cognitive/compatibility.rs`, `profile_extension.rs`.

**Interfaces:** adds all H9 IDs; aliases `AgentProfileId/RevisionId` to existing agent IDs; produces compatibility values and `AgentProfileRuntimeExtensionRevision`.

- [ ] Write failing tests proving an H9 profile points to an existing exact `AgentRevision`, duplicate objective/workflow references fail and a changed source content hash invalidates an old compatibility report.
- [ ] Add normalized version requirements with deterministic equality and reject empty, unbounded-wildcard-only or malformed requirements.
- [ ] Implement exact revision references and canonical compatibility hashing.
- [ ] Add a compile test proving H9 does not define a second stable Agent/Skill/Workflow identity type.
- [ ] Run and commit:

```bash
cargo test -p vestrace-domain cognitive compatibility
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(agent-runtime): extend existing cognitive assets"
```

### Task 2: Add support profiles and monotonic overlays

**Files:** create `agent_runtime/{profile,memory_profile,verification_profile,delegation_profile,communication_policy}.rs`, `cognitive/overlay.rs` and domain tests.

- [ ] Write property tests generating base ceilings and overlays; every accepted overlay result must be a subset or stricter bound in every permission dimension.
- [ ] Reject overlay chains longer than eight, overlays for another base revision, deep parent references and attempts to add a capability/tool/provider/origin/delegation target.
- [ ] Implement immutable Memory, Verification, Delegation and Communication profile revisions with bounded fields and exact content hashes.
- [ ] Enforce `maximum_depth = 1` for standard delegation profiles unless an explicit higher-level H2 policy requirement is present; H9 reference profiles use one.
- [ ] Run and commit.

### Task 3: Extend Skill and Workflow revisions for executable Harness use

**Files:** create `cognitive/skill.rs`/`workflow.rs` companion runtime types and tests `workflow_runtime_compatibility.rs`, `cognitive_runtime_extensions.rs`.

- [ ] Test that a Skill requirement unavailable in the selected Tool catalogue is incompatible rather than silently omitted.
- [ ] Test workflow nodes map exactly to H5 runtime step kinds, transitions reference existing nodes, parallel branches have bounded join behavior and completion criteria produce at least one controlled terminal path.
- [ ] Implement Skill activation conditions, model/tool/capability requirements, examples, evaluation datasets, verification references and conformance cases.
- [ ] Implement `WorkflowRuntimeBindingRevision` without copying mutable Run state into Workflow rows.
- [ ] Run and commit.

### Task 4: Define H9 application ports, commands and deterministic fixtures

**Files:** create application cognitive/agent_runtime/package/extension/remote_agent ports and `vestrace-extension-test-support`.

**Interfaces:** produces repository ports, `AgentCompositionPort`, `AgentRuntimeSnapshotPort`, `PackageArtifactPort`, `PackageSignatureVerifierPort`, `PackageDependencyResolverPort`, `ExtensionRuntimeHostPort`, `ExtensionConformancePort`, `ExtensionCategoryRegistrationPort`, `RemoteAgentDiscoveryPort`, `RemoteAgentHealthPort` and deterministic fixtures/faults.

- [ ] Add object-safety compile tests for every port.
- [ ] Define one-shot faults after package parse, signature verify, lock persistence, package activation, extension process spawn, conformance persistence, extension registration, remote candidate import and snapshot commit.
- [ ] `PackageArtifactPort` accepts exact Available H6 Artifact revisions only and returns bounded streams/manifest representations; no host path enters application code.
- [ ] `ExtensionRuntimeHostPort` accepts only normalized envelopes and returns normalized observations; no SDK/process/HTTP/MCP types cross the port.
- [ ] Build deterministic signed/unsigned package, stdio/HTTP/MCP extension and remote-declaration fixtures.
- [ ] Run and commit.

### Task 5: Persist cognitive runtime extensions and support profiles

**Files:** create migrations `0061`, `0062`, PostgreSQL repositories and tests `profile_persistence.rs`, `agent_overlay_narrowing.rs`.

- [ ] `0061` creates Agent profile companion revisions, overlays, Skill runtime extensions, Workflow runtime bindings and compatibility reports linked to existing v0.1 revisions.
- [ ] `0062` creates Memory, Verification, Delegation and Communication profile identities/revisions plus normalized child rows.
- [ ] Force immutable revision rows and same-workspace or deployment-builtin references; current pointers use optimistic revision checks.
- [ ] Prove a workspace cannot attach an extension row to another workspace's Agent/Skill/Workflow revision.
- [ ] Prove migration code does not duplicate existing `agents`, `skills` or `workflows` tables.
- [ ] Run and commit.

### Task 6: Implement deterministic composition, compatibility and permission diff

**Files:** create `agent_runtime/composition.rs`, `compatibility.rs`, `permission_diff.rs` and tests.

- [ ] Compose in fixed order: base Agent revision → runtime extension → ordered overlays → selected Skills → optional Workflow → support profiles → policy/tool/extension/remote catalogues.
- [ ] Collect all incompatibilities instead of stopping at the first: missing revision, lifecycle, tool/capability, model requirement, policy, memory scope, context policy, delegation, extension protocol, remote eligibility and budget conflicts.
- [ ] Implement permission diff across every normative `PermissionDimension`; unknown semantics become `Incompatible`.
- [ ] Test one apparently harmless Extension update that adds a new network origin and assert overall `Expansion`.
- [ ] Test narrowing tool and classification ceilings produces `Narrowing` and no omitted expansion.
- [ ] Run and commit.

### Task 7: Build immutable catalogues and AgentRuntimeSnapshot

**Files:** create migration `0063`, catalogue/snapshot services/repositories and tests `agent_runtime_snapshot.rs`, `agent_runtime_snapshot_immutability.rs`.

- [ ] `0063` creates Tool, Extension and Remote-Agent catalogue snapshots, Agent runtime snapshots, exact revision relation rows and snapshot compatibility bindings.
- [ ] Build catalogues from exact active revisions after current lifecycle and H2 read/eligibility checks; canonically sort entries before hashing.
- [ ] In one transaction persist catalogues, compatibility report reference and `AgentRuntimeSnapshot` or persist nothing.
- [ ] Same composition idempotency key and identical inputs return the same snapshot; changed payload conflicts or creates a different snapshot under a new key.
- [ ] After revising a Skill, package or Extension, prove the old snapshot still resolves all exact historical revisions and its hash remains unchanged.
- [ ] Before every future execution, category owners recheck current policy/revocation; prove revoking an Extension blocks new invocation without rewriting the snapshot.
- [ ] Run and commit.

### Task 8: Add Agent Package, publisher, signature and lock contracts

**Files:** create package domain modules and `vestrace-package-runtime` with package tests.

- [ ] Validate canonical `vestrace-package.json`, normalized file list, SHA-256, maximum files/bytes/dependency depth and forbidden archive entries.
- [ ] Verify Ed25519 signatures against the exact publisher revision and canonical manifest/file digest list; changed publisher trust does not rewrite signature history.
- [ ] Implement deterministic dependency resolution with same-workspace/deployment-builtin eligibility, cycle rejection, maximum depth 16 and stable tie-breaking by highest compatible version then exact package ID.
- [ ] Lockfiles store exact revisions only and hash every resolved package/extension/remote requirement.
- [ ] Reject package content containing secret-like fields, active Connection IDs, approval/ticket/lease IDs, Run/checkpoint state or executable paths outside declared Extension payloads.
- [ ] Run and commit.

### Task 9: Implement restart-safe package ingestion and local registry

**Files:** create migration `0064`, package ingestion/parser/signature/resolver/installer services, repositories and installation tests.

- [ ] `0064` creates publishers/revisions/keys, package identities/revisions, installations/events, file digests, dependencies, locks/entries, policy templates and activation identities/revisions.
- [ ] Installation flow:

```text
Available H6 package Artifact
→ inspect archive representation
→ parse canonical manifest
→ verify every listed digest
→ verify optional signature
→ normalize requested permissions
→ resolve exact dependency lock
→ compatibility report
→ Installed/Compatible or Incompatible
```

- [ ] Crash after parsing/signature/lock persistence resumes the same installation and cannot create another logical package revision.
- [ ] Store packaged policy templates inactive; activating the package does not create or activate an H2 PolicyBundleRevision.
- [ ] Unsigned packages remain `Unverified`; Blocked publisher packages cannot become Compatible or Active.
- [ ] Run and commit.

### Task 10: Implement package activation, upgrade, rollback and reference packages

**Files:** create package activation/rollback/reference services, HTTP/CLI package routes and `packages/reference/*`; add activation/rollback tests.

- [ ] Prepare activation by comparing the exact currently active package lock with the candidate lock and persisting compatibility plus permission diff.
- [ ] `Expansion` requires exact H2 `package.activate` authorization and operation-bound administrator ApprovalGrant. `Narrowing/NoChange` still uses an explicit protected activation command.
- [ ] Activation atomically supersedes the prior activation revision and never changes existing `AgentRuntimeSnapshot` rows.
- [ ] Rollback creates a new activation revision pointing to an older compatible lock and runs the same diff/approval process relative to current active state.
- [ ] Add deterministic reference packages:
  - `vestrace.universal-assistant`: Direct/Guided, no Commit, candidate-memory writes, delegation depth one;
  - `vestrace.research`: evidence/artifact-heavy workflow, independent verification, no external commitment by default;
  - `vestrace.workspace-automation`: H8 logical Connection requirements, Prepare/Execute ceilings, exact approval for destructive/external commitments.
- [ ] Reference packages contain schemas/evaluation fixtures and no active tools, Connections or secrets.
- [ ] Run and commit.

### Task 11: Persist Extension Registry, activation, conformance and health

**Files:** create migration `0065`, extension domain/services/repositories and persistence/activation tests.

- [ ] `0065` creates Extension identities/revisions/manifests/categories/operations/permissions, activation identities/revisions, service-binding relations, conformance reports/results, health observations, circuit-breaker state and registration records.
- [ ] Reject `TrustedInProcess` unless trust is Builtin and integrity digest matches a deployment-registered compiled adapter.
- [ ] Require executable Artifact revision for process modes and exact normalized origin for RemoteService.
- [ ] Install as Disabled/Quarantined; run compatibility, isolation, secret-boundary and category conformance before activation.
- [ ] Activation with permission expansion requires H2 + administrator approval; configuration and H8 service bindings are exact revisions.
- [ ] Extension revision updates never mutate an existing activation or runtime snapshot.
- [ ] Run and commit.

### Task 12: Implement extension protocol hosts, category conformance and circuit breakers

**Files:** create `vestrace-extension-runtime`, host/conformance/health services and stdio/HTTP/MCP/isolation/health tests.

- [ ] `VestraceJsonStdio` uses length-bounded canonical JSON messages, handshake/version negotiation, request IDs, deadline/cancel and no inherited ambient environment except an allowlist.
- [ ] `VestraceHttp` uses exact origin, TLS policy, H8 service binding, request/response limits and no redirect by default.
- [ ] MCP bridge maps only activated declared tools/resources and treats discovery annotations/output as untrusted; it cannot register a remote agent as a Tool Provider.
- [ ] `SandboxedProcess` launches through an H4 digest-pinned profile with read-only payload, declared output paths, network policy and H8 proxy where needed.
- [ ] One category-specific conformance suite proves normalized inputs/outputs, idempotency, timeout, cancellation, Unknown completion, reconciliation reference, secret boundary and permission ceiling.
- [ ] Health reports may only narrow declared capabilities. Circuit breaker opens after configured failures, supports bounded half-open probes and does not alter immutable activation data.
- [ ] Category registration is withdrawn while breaker is open/activation revoked; H3/H4/H6/H7/H8 receive `adapter_unavailable`, not a fallback to undeclared behavior.
- [ ] Run and commit.

### Task 13: Implement transport-neutral remote-agent registry and local activation

**Files:** create migration `0066`, remote-agent services/repositories and remote tests.

- [ ] `0066` creates stable definitions, immutable declaration revisions/interfaces/skills/security declarations, local activation identities/revisions, permission diffs, import candidates and health observations.
- [ ] Import accepts only an H6 Available declaration Artifact plus normalized `RemoteAgentDeclarationCandidate` from `RemoteAgentDiscoveryPort`.
- [ ] Treat every declared field as untrusted; verified identity is supplied by a separate authenticated transport/administrator evidence reference.
- [ ] Card/declaration changes always create a new candidate revision. Compute diff for transports, origins, skills, security requirements and schema hashes.
- [ ] New origin/skill/security requirement or broader classification is `Expansion` and cannot silently replace an active revision.
- [ ] Local activation fixes allowed transports/skills/classification, required identity and optional exact H8 remote Connection profile revision.
- [ ] Blocked/Unverified remote agents cannot enter standard reference package catalogues without explicit administrator policy.
- [ ] H9A later implements discovery and invocation; prove no A2A SDK type enters H9 domain/application/persistence.
- [ ] Run and commit.

### Task 14: Integrate snapshots and registries with H1–H8 and management surfaces

**Files:** modify H1 Run creation, H5 internal/remote delegation, H7 Trigger targets, H8 remote profiles, worker registration/composition, HTTP/CLI routes and schemas; add `runtime_snapshot_run_binding.rs`.

- [ ] Run creation accepts one exact `AgentRuntimeSnapshotId`; application rejects a mutable profile/package name as executable identity.
- [ ] H5 internal delegation resolves a child exact snapshot under parent delegation ceiling. H5 remote delegation selects exact remote declaration + local activation revisions from the parent remote catalogue.
- [ ] H7 Trigger revisions continue to target exact snapshots. Updating an active package does not redirect an existing Trigger revision; trigger management must create a new revision.
- [ ] H8 remote Connection profiles bind exact H9 remote declaration revision; a card update does not silently reuse identity/origin grants.
- [ ] Worker registration reports supported extension protocols, active built-in digests and isolation capabilities; scheduler dispatches only to compatible workers.
- [ ] Add minimal administrative HTTP/CLI commands for inspect/install/activate/disable/list, permission diff, compatibility report and snapshot build. No endpoint returns executable bytes, secrets, lease handles or raw untrusted diagnostics.
- [ ] Regenerate HTTP/event/extension schemas from Vestrace-owned DTOs.
- [ ] Run and commit.

### Task 15: Add checkpoint V7, RLS, boundary gates and H9 acceptance

**Files:** create migration `0067`, Run work/event/checkpoint changes, scripts, CI and `h9_acceptance.rs`/RLS tests.

- [ ] Add work kinds:

```text
ValidateAgentComposition
BuildAgentRuntimeSnapshot
InspectAgentPackage
VerifyPackageSignature
ResolvePackageLock
ValidatePackageCompatibility
RunExtensionConformance
PollExtensionHealth
EvaluateExtensionCircuitBreaker
ImportRemoteAgentCandidate
ValidateRemoteAgentRevision
```

Work payloads contain stable IDs/revisions/hashes/deadlines only; no archive bytes, executable paths, raw remote URL, secret, Connection material, approval payload or process handle.

- [ ] Add logical Run events only for binding/rebinding an exact runtime snapshot or pausing because a required activated revision is revoked/incompatible. Install, health, conformance and catalogue progress remain H9-local and do not increment `RunVersion`.
- [ ] `RunCheckpointV7` extends V6 with coordinator/child snapshot IDs, package lock IDs, extension catalogue snapshot IDs and remote-agent activation revision IDs. Older payloads remain readable. Mutable health/circuit-breaker state is never checkpoint authority.
- [ ] `0067` forces RLS, deployment-builtin read rules, same-workspace reference consistency, append-only revision/report/lock/snapshot guards and active installation/activation/health indexes.
- [ ] Boundary scripts reject duplicate cognitive identities, unresolved ranges in snapshots, overlays that widen, package secrets/runtime state, third-party in-process plugins, SDK types in core, direct extension credential reads, remote agents registered as tools and A2A types outside H9A.
- [ ] Mandatory acceptance scenario proves:
  1. existing v0.1 Agent/Skill/Workflow IDs are reused;
  2. base profile plus two overlays deterministically narrows capabilities/tools/classification;
  3. incompatible Skill/Workflow requirement yields a complete report and no snapshot;
  4. signed reference package installs from an H6 Artifact and resolves an exact lock;
  5. unsigned package remains Unverified and inactive;
  6. package permission expansion requires exact administrator approval;
  7. policy templates remain inactive after package activation;
  8. snapshot pins exact profile/skills/workflow/policies/catalogues/package lock;
  9. updating a package/Skill/Extension leaves the old snapshot unchanged;
  10. stdio, HTTP and MCP extension fixtures pass one normalized conformance contract;
  11. sandboxed extension cannot read undeclared Artifact, network or credential;
  12. extension capability expansion creates a new revision/diff and cannot self-activate;
  13. circuit breaker removes an unhealthy adapter without mutating snapshots;
  14. remote declaration import is untrusted, card change creates a new candidate and new origin is an Expansion;
  15. Agent Card cannot create trust, Connection or skill authority;
  16. H5 internal SubRun receives a narrowed exact child snapshot;
  17. H7 Trigger retains its pinned snapshot after package activation changes;
  18. revoking a required Extension blocks future dispatch while preserving prior Run history;
  19. restart at package/lock/activation/conformance/registration/import/snapshot fault points creates no duplicate revision, lock, activation or process;
  20. replay performs no installation, activation, process start, network discovery, health probe or snapshot creation.
- [ ] Required CI jobs: cognitive extensions/profiles, package parsing/signatures/locks, package activation/reference packages, extension registry/protocols/isolation, remote-agent registry, snapshot integration, boundaries, RLS and H9 acceptance.
- [ ] Run and commit:

```bash
bash scripts/verify-cognitive-registry-boundary.sh
bash scripts/verify-package-boundary.sh
bash scripts/verify-extension-boundary.sh
bash scripts/verify-extension-permissions.sh
bash scripts/verify-remote-agent-boundary.sh
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test h9_acceptance --test package_extension_rls \
             --test agent_runtime_snapshot --test extension_activation_permission_diff
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

git add migrations/0067_package_extension_rls_indexes_and_run_bindings.sql \
  .github/workflows/ci.yml crates packages scripts tests schemas Cargo.toml Cargo.lock
git commit -m "test(extensions): add H9 acceptance and registry gates"
```

---

## Migration ownership

```text
0061 Task 5  Agent profile extensions, overlays and Skill/Workflow runtime bindings
0062 Task 5  Memory, verification, delegation and communication profiles
0063 Task 7  Agent runtime snapshots and exact catalogues
0064 Task 9  Publishers, Agent Packages, lockfiles, installations and activations
0065 Task 11 Extension registry, activations, conformance, health and circuit breakers
0066 Task 13 Remote-agent definitions, local activation revisions and permission diffs
0067 Task 15 RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H9 completion definition

H9 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
existing Agent/Skill/Workflow revisions
+ support profile revisions
+ explicit narrowing overlays
+ exact active Tool/Extension/Remote catalogues
+ exact H2 policy revisions
+ optional exact Package lock
→ CompatibilityReport
→ immutable AgentRuntimeSnapshot
→ AgentRun / SubRun / Trigger binding
```

Package/Extension lifecycle must demonstrate:

```text
H6 quarantined Artifact
→ package/extension inspection
→ canonical manifest + integrity/signature
→ exact dependency lock
→ compatibility report
→ permission diff
→ explicit H2-governed activation
→ exact catalogue entry
→ immutable future Run snapshot
```

Required invariants:

1. Existing v0.1 cognitive identities remain authoritative and are not duplicated.
2. Every executable Run/child Run/Trigger target uses an exact immutable `AgentRuntimeSnapshot`.
3. Overlay composition is shallow, deterministic and narrowing-only.
4. Skills and Workflows declare requirements but grant no authority.
5. Workflow runtime state remains in H1/H5, not Workflow definitions.
6. Snapshot hashes include every exact revision/catalogue/policy/lock and contain no unresolved range.
7. Snapshot immutability does not bypass current policy, revocation or availability checks.
8. Package bytes and extension payloads enter through H6 quarantine and inspection.
9. Package install, compatibility and activation are separate durable operations.
10. Package signatures prove key verification only; local trust and activation remain explicit.
11. Package policy templates never auto-activate H2 policy.
12. Package locks are deterministic, exact, cycle-free and workspace-safe.
13. Existing Runs and Trigger revisions do not silently follow package updates.
14. Third-party in-process Rust plugins are forbidden.
15. Extension SDK/protocol types remain behind the H9 runtime adapter boundary.
16. Extension configuration contains no secrets; H8 leases are resolved only at category enforcement points.
17. Extension permission expansion requires new revision, complete diff and approval.
18. Health/circuit-breaker state cannot expand manifest capability or rewrite snapshots.
19. Category owners H3/H4/H6/H7/H8 remain authoritative for actual operations.
20. Remote agents are distinct from tools and use H5 delegation semantics.
21. Remote declarations/Agent Cards are untrusted and separated from local activation/trust.
22. Remote declaration changes cannot silently broaden origins, skills, security or classification.
23. H9 contains no A2A SDK type; H9A can implement discovery/transport through ports.
24. Reference Universal Assistant, Research and Workspace Automation packages contain no secret or active Connection.
25. Restart does not duplicate package/extension/remote revisions, locks, activations, process registration or snapshots.
26. Replay performs no package, extension or remote-agent side effect.
27. H10 can evaluate packages/extensions/snapshots without rewriting their history.
28. H11 can build product surfaces on stable H9 APIs without changing authority boundaries.
29. Tests require no public package registry, marketplace, remote agent or permanent credential.

## Explicit non-goals

H9 does not implement a public marketplace, automatic model-driven package/Skill installation, package self-update, automatic policy activation, automatic Connection creation, arbitrary Rust dynamic libraries, WASM runtime, production gRPC extension protocol, browser extensions, Kubernetes operators, cross-workspace package sharing with mutable authority, cross-organization trust federation, A2A transport/client/server behavior, automatic Agent Card trust, public signing service, remote executable download at invocation time or live mutation of an active Run snapshot.

## Documentation-only boundary

Creating this document does not authorize implementation. During the current documentation phase, do not create `feat/h9-agent-packages-extension-registry`, add package/signature/process dependencies, create migrations `0061`–`0067`, ingest or install a real package, create publisher trust keys, start extension processes, activate adapters, import remote Agent Cards, modify production Run composition, change CI or execute H9 tests.