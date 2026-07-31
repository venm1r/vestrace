# Vestrace H9 Agent Packages and Extension Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Do not create the implementation branch, modify dependencies, create migrations, install packages or extensions, start extension processes, import remote Agent Cards, run tests or write production code until the user explicitly ends the documentation-only phase.

**Goal:** Extend the existing v0.1 cognitive-asset registry into a reproducible Agent composition system with explicit narrowing overlays, immutable `AgentRuntimeSnapshot` records, portable content-addressed Agent Packages, deterministic dependency locks, an isolated Extension Registry with permission-diff activation, transport-independent remote-agent definitions and three reference packages.

**Architecture:** H9 does not create a second registry for agents, skills or workflows. Existing v0.1 cognitive identities and revisions remain authoritative; H9 adds companion runtime metadata, support profiles, overlays, compatibility reports and exact catalogue snapshots. Portable packages use package-local asset descriptors and content hashes; installation imports those definitions into the local authoritative registry and records exact local revision bindings. Installation, compatibility, activation and Run snapshot creation are separate operations. Extensions are registered behind category-owned ports and H4/H8 enforcement. Remote declarations remain distinct from local trust/activation. H9A later supplies A2A discovery and transport through these Vestrace-owned boundaries.

**Tech Stack:** Existing Vestrace v0.1 plus H1–H8 Rust workspace; Rust Edition 2024; Tokio; Serde/Schemars; SQLx and PostgreSQL 17; SHA-256 canonical hashes; JSON Schema; deterministic SemVer-compatible requirements and exact lockfiles; Ed25519 package signatures; H6 Artifact ingestion/archive representations; H4 sandbox/process execution; H8 service bindings and credential leases; Axum/Clap administration; deterministic extension/package/remote fixtures; proptest; tracing.

## Global Constraints

- Complete all five v0.1 plans and H1–H8 before implementing H9.
- Harness design sections `7. Универсальная модель агента`, `23. Extensions` and `24. Agent Profiles, Skills, Workflows и Packages` are normative.
- ADR-0003 is normative for remote-agent definitions and the future A2A adapter.
- Existing v0.1 `agents`, `agent_revisions`, `skills`, `skill_revisions`, `workflows` and `workflow_revisions` remain authoritative. H9 never creates parallel cognitive identities.
- Public H9 aliases `AgentProfileId` and `AgentProfileRevisionId` reuse existing `AgentId` and `AgentRevisionId`.
- PostgreSQL is authoritative for companion runtime revisions, overlays, support profiles, compatibility reports, catalogue snapshots, Agent runtime snapshots, publishers, package revisions and asset bindings, dependency locks, installations, activations, extension manifests/revisions/activations, conformance/health/circuit-breaker records, remote definitions/declarations/local activations and permission diffs.
- Package archives, received manifests, portable asset files, schemas, examples, evaluation fixtures, remote declarations and executable extension payloads are exact H6 Artifact revisions. PostgreSQL stores bounded normalized metadata, hashes and references.
- Vestrace owns every domain/application type, lifecycle, manifest schema, compatibility rule, permission dimension, activation decision, protocol envelope and public DTO.
- Package-manager, dynamic-library, subprocess, HTTP, MCP, gRPC, A2A, signature-library and archive-parser types remain inside adapters/runtime crates.
- Cognitive, profile, overlay, package, extension, remote declaration, activation revision, lock, catalogue and runtime snapshot revisions are immutable. Mutable lifecycle belongs to stable identity/install/activation records with optimistic state revisions or separate operational projections.
- `AgentRuntimeSnapshot` contains exact revisions/hashes only. It never stores secrets, H8 lease IDs, approvals, OAuth flows, mutable health, process handles or hidden chain-of-thought.
- Existing Runs retain their original snapshot after updates. Every enforcement point still rechecks current H2 policy, revocation and operational availability; reproducibility never means permanent authority.
- Skills, workflows, packages, extensions and remote declarations only declare requirements. Effective authority remains the intersection of principal capability, H2 policy, profile/overlay ceilings, Run/delegation/autonomy ceilings, exact Tool/Connection grants and current lifecycle.
- Overlays are explicit, ordered and shallow. They may preserve or narrow capabilities, tools, Skills, providers, classifications, budgets, delegation, autonomy and memory scope; they cannot widen.
- Package installation and activation are separate. Extension installation and activation are separate. Remote declaration import and local activation are separate.
- Package activation never activates H2 policy. Packaged policy templates remain inactive until an independent H2-managed operation.
- Package archives and extension payloads pass H6 quarantine, archive safety, media, malware and secret inspection before parsing or execution.
- Portable manifests contain package-local asset IDs and relative file paths, not local database UUIDs. Installation records the mapping to exact local revisions.
- Active dependency graphs are exact locks. Version ranges are installation inputs only and never appear unresolved in a Run snapshot.
- Packages contain no secret, credential reference, active Connection, approval, authorization ticket, Run/checkpoint state or mutable remote-task state.
- Package signatures prove that bytes match a publisher key revision. They do not grant trust, capabilities, data transfer or activation approval.
- Third-party `TrustedInProcess` extensions are forbidden. It is limited to deployment-compiled built-ins with deployment-known digest.
- Rust dynamic libraries are not a public plugin ABI.
- Initial extension protocols are built-in Rust registration, Vestrace JSON stdio, Vestrace HTTP and MCP bridge. gRPC remains reserved and inactive.
- Unverified third-party extensions default to `SandboxedProcess` or `RemoteService`. Unisolated `ExternalProcess` requires Verified/Approved trust and explicit deployment policy.
- Extension configuration contains no secret values. Secret requirements bind exact H8 service/Connection requirements, with operation-bound leases resolved only at invocation.
- Extension outputs, health text, package descriptions/examples/schemas and remote declarations are untrusted data.
- Activation requires a fresh compatibility report and complete permission diff. Any expansion requires exact H2 authorization and administrator approval. Narrowing/NoChange still requires an explicit protected activation command.
- Operational health and circuit-breaker state never mutate immutable Extension activation revisions or runtime snapshots.
- H3 owns model invocation; H4 owns Tools/sandboxes; H6 owns Artifact processing; H7 owns channels/triggers/notifications; H8 owns Connections/credentials. H9 registers exact adapters and eligibility only.
- External autonomous agents are not Tool Providers. They use H5 `RemoteAgentInvocation` and H9 remote definitions.
- Agent Cards and other declarations are untrusted compatibility input. They cannot grant trust, Skills, transports, data access or credentials.
- H9 contains no `a2a-rs` type. H9A implements discovery/invocation ports behind an anti-corruption adapter.
- Replay never installs/activates packages, starts extensions, runs conformance, probes health, imports/fetches remote declarations, changes trust or creates snapshots.
- Existing migrations `0014`–`0060` are never edited. H9 migrations are `0061`–`0067`, each owned once.
- CI uses local H6 Artifacts, deterministic signatures, local stdio/HTTP/MCP fixtures and transport-neutral remote declarations. No public registry, marketplace, A2A service, signing service or permanent credential is required.
- Future implementation branch: `feat/h9-agent-packages-extension-registry`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  cognitive/{mod,agent,skill,workflow,profile_extension,overlay,compatibility}.rs
  agent_runtime/{mod,profile,support_values,memory_profile,verification_profile,delegation_profile,communication_policy,catalogue,snapshot,permission_diff}.rs
  package/{mod,manifest,asset,publisher,dependency,lock,installation,activation,policy_template}.rs
  extension/{mod,manifest,definition,protocol,isolation,permission,activation,conformance,health,circuit_breaker,invocation}.rs
  remote_agent/{mod,definition,declaration,activation,trust,health,permission_diff}.rs
  run/{event,work,checkpoint,mod}.rs

crates/vestrace-application/src/
  cognitive/{mod,ports,commands,profile_service,skill_service,workflow_service,overlay_service,compatibility}.rs
  agent_runtime/{mod,ports,commands,composition,snapshot_service,catalogue_service,permission_diff}.rs
  package/{mod,ports,commands,ingestion,parser,asset_import,signature,dependency_resolver,installer,activation,rollback,reference_packages,worker}.rs
  extension/{mod,ports,commands,registry,installer,activation,runtime_host,conformance,health,circuit_breaker,category_registration,worker}.rs
  remote_agent/{mod,ports,commands,registry,import,activation,permission_diff,health}.rs

crates/vestrace-extension-runtime/src/{lib,envelope,stdio_host,http_host,mcp_bridge,process,sandbox,registration,error}.rs
crates/vestrace-package-runtime/src/{lib,canonical_manifest,signature,lockfile,archive,error}.rs
crates/vestrace-extension-test-support/src/{lib,packages,signatures,stdio_extension,http_extension,mcp_extension,remote_agent,conformance,fixtures,faults}.rs

crates/vestrace-channel-http/src/{package_routes,extension_routes,agent_runtime_routes,remote_agent_routes}.rs
crates/vestrace-channel-cli/src/{package_commands,extension_commands,agent_commands,remote_agent_commands}.rs

crates/vestrace-infrastructure/src/postgres/
  cognitive/{profile_extension_repository,overlay_repository,compatibility_repository}.rs
  agent_runtime/{profile_repository,catalogue_repository,snapshot_repository,permission_diff_repository}.rs
  package/{mod,publisher_repository,package_repository,asset_binding_repository,lock_repository,installation_repository,activation_repository,policy_template_repository}.rs
  extension/{mod,definition_repository,activation_repository,conformance_repository,health_repository,circuit_breaker_repository}.rs
  remote_agent/{mod,definition_repository,activation_repository,health_repository}.rs

migrations/
  0061_agent_profile_extensions_overlays_and_skill_workflow_runtime.sql
  0062_memory_verification_delegation_and_communication_profiles.sql
  0063_agent_runtime_snapshots_and_catalogues.sql
  0064_agent_packages_publishers_assets_lockfiles_and_activations.sql
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
  package_portability.rs
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
  universal-assistant/assets/
  research/vestrace-package.json
  research/assets/
  workspace-automation/vestrace-package.json
  workspace-automation/assets/

scripts/
  verify-cognitive-registry-boundary.sh
  verify-package-boundary.sh
  verify-extension-boundary.sh
  verify-extension-permissions.sh
  verify-remote-agent-boundary.sh
```

---

## Normative contracts

### Supporting values

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

pub struct ResourceBudgetTemplate {
    pub limits: Vec<ResourceBudgetTemplateLimit>,
    pub exhaustion_reaction: BudgetExhaustionReaction,
}

pub struct DelegationProfileRestriction {
    pub maximum_depth: u16,
    pub maximum_parallel_subruns: u16,
    pub allowed_internal_profile_ids: std::collections::BTreeSet<AgentId>,
    pub allowed_remote_stable_ids: std::collections::BTreeSet<String>,
}

pub struct ModelProviderRestriction {
    pub allowed_provider_revision_ids: std::collections::BTreeSet<ProviderRevisionId>,
    pub maximum_remote_classification: DataClassification,
}

pub struct MemoryProfileRestriction {
    pub allowed_scope_kinds: std::collections::BTreeSet<MemoryScopeKind>,
    pub maximum_context_items: u32,
    pub allow_candidate_write: bool,
}

pub struct MemoryRetrievalPolicy {
    pub strategy: String,
    pub maximum_candidates: u32,
    pub conflict_mode: String,
}

pub struct MemoryContextLimit {
    pub maximum_items: u32,
    pub maximum_tokens: u64,
}

pub struct MemoryCandidateWritePolicy {
    pub allowed_kinds: std::collections::BTreeSet<MemoryKind>,
    pub require_verified_sources: bool,
}

pub struct RunConsolidationPolicy {
    pub enabled: bool,
    pub maximum_candidates: u32,
    pub require_terminal_evaluation: bool,
}

pub struct VerifierRequirement {
    pub kind: String,
    pub minimum_count: u16,
    pub deterministic_first: bool,
}

pub enum VerificationFailureDisposition { Fail, Partial, RequestCorrection, RequestHumanReview }

pub struct AgentTargetRequirement {
    pub profile_stable_id: String,
    pub required_capabilities: std::collections::BTreeSet<Capability>,
}

pub struct RemoteAgentRequirement {
    pub stable_id: String,
    pub version_requirement: Option<VersionRequirement>,
    pub required_skills: std::collections::BTreeSet<String>,
    pub required_transports: std::collections::BTreeSet<String>,
    pub maximum_classification: DataClassification,
    pub optional: bool,
}

pub struct ModelInvocationRequirementsTemplate {
    pub task_type: String,
    pub required_capabilities: std::collections::BTreeSet<ModelCapability>,
    pub minimum_quality_micros: u32,
    pub independence: ModelIndependenceRequirement,
}

pub struct MemoryUsagePattern {
    pub required_scope_kinds: std::collections::BTreeSet<MemoryScopeKind>,
    pub may_write_candidate: bool,
}

pub struct SkillConformanceCase {
    pub stable_id: String,
    pub input_artifact_revision_id: ArtifactRevisionId,
    pub expected_output_schema_hash: [u8; 32],
}

pub struct WorkflowNodeRuntimeBinding {
    pub workflow_node_id: WorkflowNodeId,
    pub step_kind: PlanStepKind,
    pub input_schema_hash: [u8; 32],
    pub output_schema_hash: [u8; 32],
}

pub struct PolicyRequirement {
    pub action: ActionId,
    pub resource_kind: String,
}

pub struct WorkflowCompletionContract {
    pub required_output_schema_hash: [u8; 32],
    pub success_criteria_hash: [u8; 32],
}
```

Every free-form stable identifier is bounded and normalized. Strings such as strategies or kinds use registered lowercase dotted identifiers; unknown identifiers are preserved for import diagnostics but cannot become active until registered.

### Compatibility

```rust
pub enum CompatibilityDisposition { Compatible, CompatibleWithWarnings, Incompatible }

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

Version requirements are normalized at write time. Active locks and snapshots contain exact revisions only. A changed dependency, runtime capability, policy, catalogue or trust revision requires a new report.

### Existing cognitive identities and runtime companion revisions

```rust
pub type AgentProfileId = AgentId;
pub type AgentProfileRevisionId = AgentRevisionId;

pub struct RoutingPolicySnapshotRef {
    pub routing_policy_id: RoutingPolicyId,
    pub revision: u32,
    pub content_hash: [u8; 32],
}

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
```

The companion revision is valid only for one exact existing `AgentRevision` and never overwrites v0.1 fields.

### Explicit overlays

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

Composition supports one base profile and at most eight ordered overlays. Each overlay is proven as monotonic narrowing against the accumulated result. Parent overlays and deep inheritance are rejected.

### Support profiles

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

These profiles describe requirements and ceilings; H2/H6/H7/H8 remain enforcement authorities.

### Skill and Workflow runtime companions

```rust
pub struct EvaluationFixtureRef {
    pub artifact_revision_id: ArtifactRevisionId,
    pub task_type: String,
    pub schema_hash: [u8; 32],
}

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
    pub evaluation_fixtures: Vec<EvaluationFixtureRef>,
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

H10 may import evaluation fixtures into richer datasets later without changing H9 package or Skill history.

### Permission diff

```rust
pub enum PermissionDiffDisposition { NoChange, Narrowing, Expansion, Incompatible }

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

One expansion makes the overall result `Expansion`. Unknown semantics are `Incompatible`.

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

Vectors are sorted and duplicate-free before hashing. Snapshot creation is atomic. It contains no active Connection ID; H8 resolves exact runtime access per Run/step.

### Portable Agent Package manifest and local asset bindings

```rust
pub enum PublisherTrustLevel { Builtin, Verified, Approved, Unverified, Blocked }
pub enum PackageAssetKind {
    AgentProfile,
    Skill,
    Workflow,
    AgentProfileRuntimeExtension,
    MemoryProfile,
    VerificationProfile,
    DelegationProfile,
    CommunicationPolicy,
    ContextPolicyTemplate,
    H2PolicyTemplate,
    Schema,
    EvaluationFixture,
    Example,
}

pub struct PackageSigningKey {
    pub key_id: String,
    pub algorithm: String,
    pub public_key_bytes: Vec<u8>,
    pub revoked_at: Option<Timestamp>,
}

pub struct PackageSignature {
    pub algorithm: String,
    pub key_id: String,
    pub signature_bytes: Vec<u8>,
}

pub enum PackageSignatureStatus { Builtin, Verified, Invalid, Missing, UnknownKey, RevokedKey }

pub struct PackageFileDigest {
    pub path: String,
    pub media_type: String,
    pub byte_size: u64,
    pub sha256: [u8; 32],
}

pub struct PackageAssetDescriptor {
    pub local_id: String,
    pub kind: PackageAssetKind,
    pub path: String,
    pub content_hash: [u8; 32],
    pub stable_name: String,
}

pub struct PackageAssetBinding {
    pub package_revision_id: AgentPackageRevisionId,
    pub package_local_id: String,
    pub asset_kind: PackageAssetKind,
    pub local_revision: ExactRevisionRef,
}

pub struct PackagePublisherRevision {
    pub id: PackagePublisherRevisionId,
    pub publisher_id: PackagePublisherId,
    pub revision: u32,
    pub stable_id: String,
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

pub struct ToolRequirement {
    pub stable_name: String,
    pub version_requirement: Option<VersionRequirement>,
    pub optional: bool,
}

pub struct ExtensionRequirement {
    pub stable_id: String,
    pub version_requirement: VersionRequirement,
    pub required_categories: std::collections::BTreeSet<ExtensionCategory>,
    pub optional: bool,
}

pub struct RequestedPermissionSet {
    pub capabilities: std::collections::BTreeSet<String>,
    pub tool_requirements: Vec<ToolRequirement>,
    pub connector_operations: std::collections::BTreeSet<String>,
    pub network_origins: Vec<NormalizedOrigin>,
    pub secret_requirements: std::collections::BTreeSet<String>,
    pub maximum_classification: DataClassification,
    pub autonomy_ceiling: TriggerAutonomyLevel,
    pub delegation_depth: u16,
    pub content_hash: [u8; 32],
}

pub struct AgentPackageManifest {
    pub format_version: PackageFormatVersion,
    pub stable_id: String,
    pub version: String,
    pub publisher_stable_id: String,
    pub minimum_core_api: VersionRequirement,
    pub dependencies: Vec<PackageDependencyRequirement>,
    pub assets: Vec<PackageAssetDescriptor>,
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
    pub asset_bindings_hash: [u8; 32],
    pub registry_snapshot_hash: [u8; 32],
    pub resolution_inputs_hash: [u8; 32],
    pub content_hash: [u8; 32],
}
```

`vestrace-package.json` is canonical UTF-8 JSON. Paths are normalized relative paths. Absolute/parent paths, duplicate normalized paths, links/devices, unlisted files and hash/size mismatch fail. Package-local IDs make the archive portable. Installation imports definitions under a package namespace and records exact local bindings; it never trusts UUIDs supplied by the archive.

### Package lifecycle

```rust
pub enum AgentPackageLifecycle { Installed, Deprecated, Revoked }
pub enum PackageInstallationStatus { Quarantined, Inspecting, Parsed, ImportingAssets, Resolving, Compatible, Incompatible, Failed, Unknown }
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
    pub asset_bindings_hash: [u8; 32],
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

Rollback creates a new activation revision pointing to an older installed exact package/lock.

### Extension values, manifest and lifecycle

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
pub enum ExtensionDefinitionLifecycle { Installed, Quarantined, Incompatible, Revoked }
pub enum ExtensionActivationStatus { Prepared, AwaitingApproval, Active, Disabled, Superseded, Revoked }
pub enum ExtensionOperationalHealth { Healthy, Degraded, Unavailable, Unknown }
pub enum CircuitBreakerState { Closed, Open, HalfOpen }

pub struct ExtensionSecretRequirement {
    pub stable_name: String,
    pub allowed_injection_schemes: std::collections::BTreeSet<String>,
    pub required: bool,
}

pub struct ExtensionOperationDefinition {
    pub stable_name: String,
    pub side_effect: ToolSideEffectClass,
    pub risk: RiskLevel,
    pub input_schema_hash: [u8; 32],
    pub output_schema_hash: [u8; 32],
    pub supports_idempotency: bool,
    pub supports_reconciliation: bool,
}

pub struct ExtensionHealthContract {
    pub probe_operation: String,
    pub interval_seconds: u64,
    pub timeout_ms: u64,
    pub failure_threshold: u32,
    pub half_open_success_threshold: u32,
}

pub struct ExtensionManifest {
    pub stable_id: String,
    pub version: String,
    pub publisher_stable_id: String,
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
    pub publisher_revision_id: PackagePublisherRevisionId,
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
    pub status: ExtensionActivationStatus,
    pub configuration: serde_json::Value,
    pub service_binding_revision_ids: Vec<ServiceCredentialBindingRevisionId>,
    pub content_hash: [u8; 32],
}
```

Operational degradation is stored in health/circuit-breaker records, not by mutating activation status.

### Extension invocation boundary

```rust
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct OpaqueExtensionAuthorizationRef(String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CredentialLeaseHandle(String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ArtifactAccessHandle(String);

pub enum ExtensionInvocationStatus { Completed, Failed, Cancelled, Unknown }

pub struct ExtensionError {
    pub stable_code: String,
    pub safe_message: String,
    pub retryable: bool,
}

pub struct ExtensionInvocationEnvelope {
    pub invocation_id: ExtensionInvocationId,
    pub workspace_id: WorkspaceId,
    pub extension_revision_id: ExtensionDefinitionRevisionId,
    pub activation_revision_id: ExtensionActivationRevisionId,
    pub category: ExtensionCategory,
    pub operation: String,
    pub normalized_input: serde_json::Value,
    pub authorization_reference: OpaqueExtensionAuthorizationRef,
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

Handles are HMAC-backed, one-purpose, bounded and redacted. They are not raw H2 ticket IDs, H8 lease IDs, secrets or storage keys. Category owners issue and validate them at enforcement points.

### Extension conformance and health

```rust
pub enum ExtensionConformanceDisposition { Passed, PassedWithWarnings, Failed }
pub enum BoundaryCheckResult { Passed, Failed }

pub struct ExtensionCategoryConformanceResult {
    pub category: ExtensionCategory,
    pub disposition: ExtensionConformanceDisposition,
    pub case_hashes: Vec<[u8; 32]>,
}

pub struct ExtensionIsolationResult {
    pub mode: ExtensionIsolationMode,
    pub boundary: BoundaryCheckResult,
    pub safe_code: String,
}

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
    pub status: ExtensionOperationalHealth,
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

Reported capabilities must be a subset of the activated manifest. Expansion requires a new definition/activation.

### Remote declarations, local activation and health

```rust
pub enum RemoteAgentTrustLevel { Verified, Approved, Unverified, Blocked }
pub enum RemoteAgentActivationStatus { Prepared, AwaitingApproval, Active, Inactive, Superseded, Revoked, Incompatible }
pub enum RemoteAgentOperationalHealth { Healthy, Degraded, Unavailable, Unknown }

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
    pub status: RemoteAgentActivationStatus,
    pub content_hash: [u8; 32],
}

pub struct RemoteAgentHealthObservation {
    pub id: RemoteAgentHealthObservationId,
    pub activation_revision_id: RemoteAgentActivationRevisionId,
    pub status: RemoteAgentOperationalHealth,
    pub safe_code: String,
    pub observed_at: Timestamp,
}
```

Declaration claims and local trust/allowlists are separate immutable revisions. Health is operational and does not rewrite either.

---

### Task 1: Promote existing cognitive assets and add compatibility primitives

**Files:** modify `id.rs`, existing cognitive modules; create `cognitive/{compatibility,profile_extension}.rs`.

- [ ] Add all H9 IDs and aliases without defining a second Agent/Skill/Workflow stable ID.
- [ ] Test exact existing `AgentRevision` binding, duplicate objective/workflow refs and stale compatibility input hashes.
- [ ] Normalize version requirements; reject malformed, empty and unrestricted-wildcard-only requirements.
- [ ] Implement canonical exact revision and report hashing.
- [ ] Run/commit:

```bash
cargo test -p vestrace-domain cognitive compatibility
cargo clippy -p vestrace-domain --all-targets -- -D warnings
git add crates/vestrace-domain
git commit -m "feat(agent-runtime): extend existing cognitive assets"
```

### Task 2: Add support profiles and monotonic overlays

**Files:** create agent runtime support/profile files, `cognitive/overlay.rs` and domain tests.

- [ ] Property-test that every accepted overlay result is a subset or stricter limit in every permission dimension.
- [ ] Reject more than eight overlays, another base revision, parent overlays and additions of capability/tool/provider/origin/delegation target.
- [ ] Implement immutable Memory, Verification, Delegation and Communication profile revisions.
- [ ] Standard profiles enforce delegation depth one; a higher value is incompatible without an explicit nonstandard H2 requirement.
- [ ] Run/commit.

### Task 3: Extend Skill and Workflow revisions for Harness execution

**Files:** add companion Skill/Workflow runtime types and tests.

- [ ] Missing required Tool/capability/model/profile makes a Skill incompatible, never silently reduced.
- [ ] Map Workflow nodes exactly to H5 `PlanStepKind`; validate transitions, bounded parallel/join behavior and controlled terminal path.
- [ ] Add activation conditions, examples, H6 evaluation fixtures, verification and conformance cases.
- [ ] Store no mutable Run state in Workflow records.
- [ ] Run/commit.

### Task 4: Define H9 ports, commands and deterministic fixtures

**Files:** create application ports/modules and `vestrace-extension-test-support`.

**Interfaces:** repository ports, Agent composition/snapshot, package Artifact/signature/import/resolution, extension runtime/conformance/category registration and remote discovery/health.

- [ ] Object-safety compile tests for every port.
- [ ] One-shot faults after package parse, asset import, signature, lock, activation, extension spawn/conformance/registration, remote import and snapshot commit.
- [ ] Package input is an exact Available H6 Artifact; no host path crosses the port.
- [ ] Extension host accepts/returns normalized Vestrace envelopes only.
- [ ] Build deterministic portable packages, signatures, stdio/HTTP/MCP extensions and remote declarations.
- [ ] Run/commit.

### Task 5: Persist runtime companions and support profiles

**Files:** create `0061`, `0062`, repositories and profile/overlay tests.

- [ ] `0061`: Agent companion revisions, overlays, Skill extensions, Workflow bindings, compatibility reports linked to existing v0.1 revisions.
- [ ] `0062`: Memory, Verification, Delegation and Communication profile identities/revisions and child rows.
- [ ] Enforce immutable revisions, optimistic current pointers and same-workspace/deployment-builtin references.
- [ ] Prove cross-workspace companion attachment fails.
- [ ] Prove no duplicate `agents`, `skills` or `workflows` table is created.
- [ ] Run/commit.

### Task 6: Implement deterministic composition, compatibility and permission diff

**Files:** create composition/compatibility/permission-diff services and tests.

- [ ] Fixed order: base Agent → companion → overlays → Skills → optional Workflow → support profiles → policy/Tool/Extension/Remote catalogues.
- [ ] Return all missing/lifecycle/tool/capability/model/policy/memory/context/delegation/protocol/remote/budget findings.
- [ ] Compare every Permission dimension; unknown semantics are Incompatible.
- [ ] New Extension origin produces Expansion; reduced Tool/classification ceilings produce Narrowing.
- [ ] Run/commit.

### Task 7: Build immutable catalogues and AgentRuntimeSnapshot

**Files:** create `0063`, catalogue/snapshot services/repositories and snapshot tests.

- [ ] `0063`: Tool/Extension/Remote catalogue snapshots, Agent runtime snapshots, exact relation rows and compatibility bindings.
- [ ] Build only from exact active revisions after lifecycle and H2 eligibility checks; canonical sort before hashing.
- [ ] Persist catalogues/report/snapshot atomically.
- [ ] Same idempotency key + same inputs returns same snapshot; changed payload conflicts.
- [ ] Later Skill/package/Extension revision leaves old snapshot resolvable and unchanged.
- [ ] Current revocation blocks future use without rewriting snapshot.
- [ ] Run/commit.

### Task 8: Add portable Package, publisher, signature, asset and lock contracts

**Files:** create package domain/runtime modules and package tests.

- [ ] Validate canonical manifest, package-local IDs, file list, hashes, maximum files/bytes/depth and forbidden entries.
- [ ] Verify Ed25519 signature against exact local publisher revision and canonical manifest/file digest list.
- [ ] Resolve dependencies deterministically from a recorded registry snapshot; reject cycles and depth above 16.
- [ ] Lock exact package/Extension/Remote revisions and imported asset-binding hash.
- [ ] Reject local UUIDs in portable asset descriptors and secret/Connection/approval/ticket/Run/checkpoint fields.
- [ ] Run/commit.

### Task 9: Implement restart-safe package ingestion, asset import and local registry

**Files:** create `0064`, installer/importer/parser/signature/resolver services/repositories and portability/restart tests.

- [ ] `0064`: publishers/revisions/keys, package identities/revisions, installations/events, files/dependencies, asset bindings, locks/entries, inactive policy templates and activation identities/revisions.
- [ ] Flow:

```text
Available H6 package Artifact
→ archive/secret inspection
→ canonical manifest and file hashes
→ publisher/signature result
→ import package-local assets into authoritative registries
→ persist exact PackageAssetBindings
→ resolve exact dependency lock
→ compatibility report
→ Compatible or Incompatible
```

- [ ] Package stable IDs namespace imported assets. Identical installed content is reused; conflicting same namespace/stable name from another source fails and never overwrites manually authored assets.
- [ ] Crash after asset import/signature/lock resumes one installation and creates no duplicate local revision or package revision.
- [ ] Policy templates remain inactive H9 records and are not H2 bundles.
- [ ] Unsigned is Unverified; Blocked publisher cannot become Compatible/Active.
- [ ] Run/commit.

### Task 10: Implement package activation, upgrade, rollback and reference packages

**Files:** activation/rollback/reference services, HTTP/CLI package routes, reference package assets and tests.

- [ ] Compare exact current lock to candidate and persist fresh compatibility + permission diff.
- [ ] Expansion requires H2 `package.activate` and exact administrator ApprovalGrant; Narrowing/NoChange is still explicit/protected.
- [ ] Activation atomically supersedes prior activation and never mutates existing snapshots.
- [ ] Rollback creates a new activation revision and runs the same checks.
- [ ] Add portable deterministic packages:
  - `vestrace.universal-assistant`: Direct/Guided, no Commit, candidate-memory writes, depth one;
  - `vestrace.research`: evidence/Artifact workflow, independent verification, no external commitment by default;
  - `vestrace.workspace-automation`: logical H8 Connection requirements, Prepare/Execute ceilings, approval for destructive/external commitment.
- [ ] Reference assets contain no local UUID, active Tool/Connection, secret or approval.
- [ ] Run/commit.

### Task 11: Persist Extension Registry, activations, conformance and health

**Files:** create `0065`, Extension services/repositories and registry/activation tests.

- [ ] `0065`: identities/revisions/manifests/categories/operations/permissions, activation identities/revisions, service-binding refs, conformance, health, circuit breakers and registrations.
- [ ] Reject third-party TrustedInProcess; require deployment-compiled Builtin digest.
- [ ] Require Available executable Artifact for process modes and exact origin for RemoteService.
- [ ] Install Quarantined/Disabled; require compatibility/isolation/secret/category conformance before activation.
- [ ] Expansion requires H2 + administrator approval; configuration and H8 bindings are exact revisions.
- [ ] Definition update never mutates activation/snapshot.
- [ ] Run/commit.

### Task 12: Implement extension hosts, conformance and circuit breakers

**Files:** create extension runtime and stdio/HTTP/MCP/isolation/health tests.

- [ ] JSON stdio: bounded canonical messages, handshake/version, request IDs, deadline/cancel and allowlisted environment only.
- [ ] HTTP: exact origin/TLS/H8 binding, bounded request/response and no redirect by default.
- [ ] MCP: activated declared tools/resources only; untrusted annotations; remote agents cannot register as Tool Providers.
- [ ] SandboxedProcess: H4 digest-pinned profile, read-only payload, declared outputs/network and H8 proxy when needed.
- [ ] Category conformance covers normalized I/O, idempotency, timeout, cancel, Unknown, reconciliation, secret and permission boundaries.
- [ ] Health may narrow only. Circuit breaker supports Closed/Open/HalfOpen and never changes immutable activation.
- [ ] Remove registration while unavailable/revoked; category owner gets `adapter_unavailable`.
- [ ] Run/commit.

### Task 13: Implement transport-neutral remote-agent registry and local activation

**Files:** create `0066`, remote services/repositories and tests.

- [ ] `0066`: stable definitions, immutable declarations/interfaces/skills/security, local activation identities/revisions, diffs, import candidates and health observations.
- [ ] Import only an Available H6 declaration Artifact plus normalized discovery candidate.
- [ ] Treat claims as untrusted; verified identity requires separate authenticated/admin evidence.
- [ ] Every card/declaration change creates a candidate revision and diff for transports/origins/skills/security/schema hashes.
- [ ] New origin/skill/security requirement or broader classification is Expansion.
- [ ] Activation fixes local trust, allowed transports/skills/classification, identity and optional exact H8 remote profile.
- [ ] Operational Degraded is health state, not activation mutation.
- [ ] Prove no A2A SDK type enters H9.
- [ ] Run/commit.

### Task 14: Integrate snapshots/registries with H1–H8 and management surfaces

**Files:** modify Run creation, H5 delegation, H7 Trigger target, H8 remote profile, worker registration, HTTP/CLI routes and schemas; add Run-binding test.

- [ ] Run creation requires exact snapshot ID and rejects mutable package/profile names as executable identity.
- [ ] Internal delegation resolves exact narrowed child snapshot; remote delegation selects exact declaration + local activation from parent catalogue.
- [ ] Existing Trigger revision remains pinned after package changes; update requires new Trigger revision.
- [ ] H8 remote profile remains bound to exact declaration revision after card changes.
- [ ] Worker registration reports supported protocols, built-in digests and isolation; scheduler selects compatible workers only.
- [ ] Add bounded admin inspect/install/activate/disable/list/diff/report/snapshot commands. Return no executable bytes, secrets, handles or raw diagnostics.
- [ ] Regenerate Vestrace-owned schemas.
- [ ] Run/commit.

### Task 15: Add checkpoint V7, RLS, boundary gates and acceptance

**Files:** create `0067`, Run work/event/checkpoint changes, scripts, CI and H9 acceptance/RLS tests.

- [ ] Work kinds:

```text
ValidateAgentComposition
BuildAgentRuntimeSnapshot
InspectAgentPackage
ImportPackageAssets
VerifyPackageSignature
ResolvePackageLock
ValidatePackageCompatibility
RunExtensionConformance
PollExtensionHealth
EvaluateExtensionCircuitBreaker
ImportRemoteAgentCandidate
ValidateRemoteAgentRevision
```

Payloads contain IDs/revisions/hashes/deadlines only—no bytes, paths, raw URLs, secret/Connection material, approval payload or process handle.

- [ ] Logical Run events only for exact snapshot binding or pause caused by revoked/incompatible required revision. Registry progress remains H9-local.
- [ ] `RunCheckpointV7` extends V6 with coordinator/child snapshots, package locks, Extension catalogue snapshots and remote activation revisions. Older payloads remain readable; mutable health is not authority.
- [ ] `0067` forces RLS, deployment-builtin read policy, same-workspace consistency, append-only revisions/reports/locks/snapshots and active registry indexes.
- [ ] Boundary scripts reject duplicate cognitive IDs, local UUIDs in portable manifests, unresolved ranges in snapshots, widening overlays, package secrets/runtime state, third-party in-process code, SDK types in core, direct credential reads, remote agents as tools and A2A types outside H9A.
- [ ] Acceptance proves:
  1. v0.1 cognitive IDs are reused;
  2. two overlays narrow deterministically;
  3. incompatible Skill/Workflow gives complete report and no snapshot;
  4. portable signed reference package installs in two workspaces with different local UUIDs but identical package content/asset hashes;
  5. exact local asset bindings are recorded and package-supplied UUIDs are rejected;
  6. unsigned package is Unverified/inactive;
  7. package expansion requires approval and policy templates remain inactive;
  8. snapshot pins exact revisions/catalogues/policies/lock;
  9. later updates leave old snapshot unchanged;
  10. stdio/HTTP/MCP fixtures pass normalized conformance;
  11. sandboxed extension cannot access undeclared Artifact/network/credential;
  12. Extension expansion cannot self-activate;
  13. circuit breaker removes unhealthy adapter without snapshot mutation;
  14. remote declaration is untrusted; card change/new origin creates candidate Expansion;
  15. Agent Card cannot create trust, Connection or Skill authority;
  16. SubRun receives narrowed exact child snapshot;
  17. Trigger remains pinned;
  18. revocation blocks future dispatch and preserves history;
  19. restart creates no duplicate imported asset, package/Extension/remote revision, lock, activation, process or snapshot;
  20. replay performs no install/activation/process/discovery/health/snapshot side effect.
- [ ] Required CI jobs: cognitive profiles, package portability/signatures/locks/activation/reference packages, Extension protocols/isolation, remote registry, snapshot integration, boundaries, RLS and H9 acceptance.
- [ ] Run/commit:

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
0061 Task 5  Agent companions, overlays and Skill/Workflow runtime bindings
0062 Task 5  Memory, verification, delegation and communication profiles
0063 Task 7  Agent runtime snapshots and exact catalogues
0064 Task 9  Publishers, portable packages, asset bindings, locks, installations and activations
0065 Task 11 Extension registry, activations, conformance, health and circuit breakers
0066 Task 13 Remote declarations, local activations, health and permission diffs
0067 Task 15 RLS, indexes and Run bindings
```

No later task edits an applied migration.

## H9 completion definition

H9 is complete only when all fifteen tasks pass and evidence demonstrates:

```text
existing Agent/Skill/Workflow revisions
+ support profiles
+ explicit narrowing overlays
+ exact Tool/Extension/Remote catalogues
+ exact H2 policy revisions
+ optional exact package lock and local asset bindings
→ CompatibilityReport
→ immutable AgentRuntimeSnapshot
→ AgentRun / SubRun / Trigger binding
```

Portable package lifecycle:

```text
H6 inspected package Artifact
→ canonical portable manifest
→ signature/integrity
→ import package-local assets into authoritative registries
→ exact local asset bindings
→ exact dependency lock
→ compatibility + permission diff
→ explicit H2-governed activation
→ future immutable snapshot
```

Required invariants:

1. Existing cognitive identities are reused, not duplicated.
2. Portable manifests contain package-local IDs, never trusted local UUIDs.
3. Every executable Run/SubRun/Trigger target uses an exact snapshot.
4. Overlays are shallow, deterministic and narrowing-only.
5. Skills/Workflows declare requirements and grant no authority.
6. Workflow runtime state remains in H1/H5.
7. Snapshot hashes include exact revisions/catalogues/policies/lock and no unresolved range.
8. Snapshot immutability does not bypass current policy/revocation/availability.
9. Package/Extension bytes pass H6 quarantine/inspection.
10. Package install, asset import, compatibility and activation are separate durable stages.
11. Signature verification does not imply local trust/activation.
12. Policy templates never auto-activate H2 policy.
13. Locks are deterministic, exact, cycle-free and workspace-safe.
14. Existing Runs/Triggers do not follow updates silently.
15. Third-party in-process Rust plugins are forbidden.
16. Extension protocol types remain behind runtime boundaries.
17. Extension configuration contains no secrets; H8 handles are exact and operation-bound.
18. Extension expansion requires new revision, diff and approval.
19. Operational health never mutates activation or snapshot.
20. H3/H4/H6/H7/H8 remain authoritative category owners.
21. Remote agents remain distinct from Tools.
22. Remote declarations are separate from local trust/activation/health.
23. Card changes cannot silently broaden origin/skill/security/classification.
24. H9 contains no A2A SDK type; H9A can implement ports.
25. Reference packages contain no local UUID, secret or active Connection.
26. Restart creates no duplicate imported asset, revision, lock, activation, process or snapshot.
27. Replay performs no package/Extension/remote side effect.
28. H10 can add evaluation without rewriting H9 history.
29. H11 can build product surfaces on stable H9 APIs.
30. Tests require no public registry, marketplace, A2A service or permanent credential.

## Explicit non-goals

H9 does not implement a public marketplace, model-driven installation, self-update, automatic policy activation, automatic Connection creation, arbitrary Rust dynamic libraries, WASM, production gRPC extension protocol, browser extensions, Kubernetes operators, mutable cross-workspace authority sharing, trust federation, A2A client/server transport, automatic Agent Card trust, public signing service, remote executable download at invocation time or live mutation of an active Run snapshot.

## Documentation-only boundary

Creating this document does not authorize implementation. During the documentation phase, do not create `feat/h9-agent-packages-extension-registry`, add package/signature/process dependencies, create migrations `0061`–`0067`, ingest/install a real package, create publisher trust keys, start extensions, activate adapters, import Agent Cards, modify production Run composition, change CI or execute H9 tests.