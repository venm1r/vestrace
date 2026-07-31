# Vestrace State Capture Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, production capture, diagnostic overrides, exporter changes or additional retained content. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved ADR: `docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md`;
- ADR merge commit: `24f02ed362b2af0361fcc9d499d275e21257c0e2`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- normalization merge commit: `788d47ec6c3ac5e0336f3cc8fb4af4df685da3a3`.

**Goal:** Implement operator-facing `Minimal`, `Operational`, `Reproducible` and `Forensic` capture profiles as versioned presets over H10 `ObservabilityCaptureMode`, preserving mandatory audit, H6-governed capture bytes, H2 authorization, H8 secret boundaries, deterministic downgrade and H11 transport parity.

**Architecture:** H10 remains the sole owner of capture modes, local capture records, telemetry delivery and audit/evaluation semantics. The extension adds immutable profile definitions, scoped profile selections, bounded diagnostic overrides, a pure local resolution service and a separate exporter-resolution service. A profile contributes a baseline ceiling only. H2/H6/H8/H10 policy, classification, secret findings, storage availability and exporter bindings can reduce disclosure. They cannot silently increase it. `Redacted` and `Full` bytes remain H6 Artifacts. Canonical H1–H9 events and mandatory security-audit facts remain independent of profile selection.

**Tech stack:** Existing Vestrace v0.1 plus H1–H11; Rust Edition 2024; Tokio; Serde/Schemars; SQLx; PostgreSQL 17; H2 policy and operation-bound tickets; H6 Artifact lifecycle; H8 credential/secret boundaries; H10 observability/audit contracts; H11 HTTP/CLI/MCP/SDK surfaces; proptest; deterministic fake clocks and local fixtures.

---

## Global constraints

- Complete the relevant v0.1, H2, H6, H8, H10 and H11 implementation plans before implementing this extension.
- H10 capture modes remain exactly:

```text
Disabled < MetadataOnly < StructuredOnly < Redacted < Full
```

- State capture profiles remain exactly:

```text
Minimal < Operational < Reproducible < Forensic
```

- Profile and mode are different types and never share one enum or persistence column.
- Approved baseline mapping:

```text
Minimal      -> MetadataOnly
Operational  -> StructuredOnly
Reproducible -> Redacted
Forensic     -> Full
```

- A profile cannot create, suppress, rename or rewrite a canonical domain event.
- A profile cannot grant read, export, model-transfer, retention, legal-hold, secret-access or debugging authority.
- `Disabled` applies only to optional capture. It never disables mandatory content-free audit, policy references, accounting facts, validation outcomes or bounded health counters.
- Ordinary resolution can only keep or reduce the profile baseline.
- Raising the profile ceiling requires an exact, time-bounded diagnostic override authorized through H2 and still capped by current classification, H6/H8 policy and exporter policy.
- `Forensic` is not a blanket “record everything” mode.
- Hidden chain-of-thought, provider reasoning-token content, private model scratchpads, credential bytes and unrestricted external bodies remain prohibited in every profile and mode.
- `Redacted` and `Full` bytes are H6 Artifact revisions or representations. H10 never creates a second blob store.
- Redaction failure never falls back to `Full`.
- Capture storage failure never falls back to ordinary logs, temporary plaintext files or an ungoverned backend.
- Local capture resolution and exporter resolution are separate decisions. Exported mode is:

```text
min(local_resolved_mode, exporter_maximum_capture_mode, current_export_policy_cap)
```

- One local capture may have several exporter decisions; exporter-specific state is not stored in the local capture resolution.
- A running Run pins its selected profile revision. Changing deployment or workspace defaults does not rewrite history and does not silently raise capture for that Run.
- Current policy is re-evaluated for every new capture point and may reduce the pinned baseline.
- Profile revisions, selections, diagnostic overrides, local resolution decisions and exporter decisions are workspace-isolated where applicable and use forced RLS.
- Existing applied migrations are never edited. This concern uses forward migrations after the H11 reserved range:

```text
0087_state_capture_profiles_and_selections.sql
0088_state_capture_overrides_resolutions_exports_and_rls.sql
```

- No other concern may reuse migration numbers `0087` or `0088`.
- CI requires no public model, telemetry backend, external secret store or internet access.
- Future implementation branch: `feat/state-capture-profiles`.

---

## Locked file structure

```text
crates/vestrace-domain/src/
  id.rs
  observability/mod.rs
  observability/capture.rs
  observability/profile.rs

crates/vestrace-application/src/
  observability/mod.rs
  observability/ports.rs
  observability/capture_policy.rs
  observability/profile_service.rs
  observability/diagnostic_override.rs
  observability/export.rs
  composition/observability.rs

crates/vestrace-application/tests/
  state_capture_profile_resolution.rs
  state_capture_profile_selection.rs
  state_capture_diagnostic_override.rs
  state_capture_export_resolution.rs
  state_capture_failure_semantics.rs

crates/vestrace-infrastructure/src/postgres/
  observability/mod.rs
  observability/profile_repository.rs
  observability/policy_repository.rs
  observability/capture_repository.rs
  observability/exporter_repository.rs

crates/vestrace-channel-http/src/
  observability_routes.rs

crates/vestrace-channel-cli/src/
  observability_commands.rs

crates/vestrace-public-schema/src/
  observability.rs

migrations/
  0087_state_capture_profiles_and_selections.sql
  0088_state_capture_overrides_resolutions_exports_and_rls.sql

tests/
  state_capture_profile_persistence.rs
  state_capture_profile_rls.rs
  state_capture_profile_restart.rs
  state_capture_profile_history.rs
  state_capture_profile_h6_integration.rs
  state_capture_profile_h2_authorization.rs
  state_capture_profile_h11_parity.rs
  state_capture_profile_acceptance.rs

scripts/
  verify-state-capture-profile-boundary.sh
  verify-no-hidden-reasoning-capture.sh
  verify-state-capture-migration-ownership.sh
```

---

## Normative domain contracts

### Identifiers

Add strongly typed IDs:

```text
StateCaptureProfileDefinitionId
StateCaptureProfileRevisionId
StateCaptureSelectionId
DiagnosticCaptureOverrideId
CaptureResolutionDecisionId
CaptureExportDecisionId
```

All IDs use the repository UUID strategy and reject nil values.

### Profile kind and immutable revision

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StateCaptureProfile {
    Minimal,
    Operational,
    Reproducible,
    Forensic,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct StateCaptureProfileRevision {
    pub id: StateCaptureProfileRevisionId,
    pub definition_id: StateCaptureProfileDefinitionId,
    pub deployment_id: DeploymentId,
    pub profile: StateCaptureProfile,
    pub revision: u32,
    pub baseline_mode: ObservabilityCaptureMode,
    pub default_retention_class: RetentionClassRef,
    pub diagnostic_override_allowed: bool,
    pub content_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}
```

Constructor invariants:

- revision is positive;
- profile-to-baseline mapping matches the approved ADR;
- retention is a reference to an existing H6/H10 policy class, not a copied duration;
- the revision is immutable;
- content hash covers every semantic field in canonical order.

The initial migration seeds one revision for every approved profile. A future mapping change requires a new approved ADR and a new immutable revision.

### Scoped selection

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum StateCaptureSelectionScope {
    Deployment,
    Workspace { workspace_id: WorkspaceId },
    Agent {
        workspace_id: WorkspaceId,
        agent_snapshot_id: AgentRuntimeSnapshotId,
    },
    Workflow {
        workspace_id: WorkspaceId,
        workflow_revision_id: WorkflowRevisionId,
    },
    Run {
        workspace_id: WorkspaceId,
        run_id: AgentRunId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct StateCaptureSelection {
    pub id: StateCaptureSelectionId,
    pub scope: StateCaptureSelectionScope,
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub state_revision: u64,
    pub active: bool,
    pub selected_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

pub struct ReplaceStateCaptureSelection {
    pub scope: StateCaptureSelectionScope,
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub expected_state_revision: Option<u64>,
    pub idempotency_key: String,
}
```

Selection rules:

- one active selection exists per exact scope;
- changing selection uses optimistic concurrency and writes immutable history;
- `expected_state_revision` belongs to the command and is never persisted as domain state;
- a Run selection pins the exact profile revision for that Run;
- agent/workflow/Run references belong to the same workspace;
- a profile revision ID is not permission to capture content.

### Diagnostic override

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DiagnosticCaptureOverride {
    pub id: DiagnosticCaptureOverrideId,
    pub workspace_id: WorkspaceId,
    pub run_id: Option<AgentRunId>,
    pub subject_kinds: std::collections::BTreeSet<CaptureSubjectKind>,
    pub maximum_mode: ObservabilityCaptureMode,
    pub exporter_binding_revision_ids: Vec<TelemetryExporterBindingRevisionId>,
    pub retention_class: RetentionClassRef,
    pub purpose: BoundedText,
    pub authorized_principal_id: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub authorization_ticket_id: AuthorizationTicketId,
    pub approval_grant_ids: Vec<ApprovalGrantId>,
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
    pub revoked_at: Option<Timestamp>,
    pub state_revision: u64,
    pub created_at: Timestamp,
}
```

Override invariants:

- `maximum_mode` is `StructuredOnly`, `Redacted` or `Full`;
- activation rejects an override that does not raise the selected profile ceiling for at least one requested subject;
- duration is positive and bounded by deployment policy;
- workspace and optional Run match the H2 operation fingerprint;
- subject set is non-empty and bounded;
- exporter list is explicit; empty means escalated content cannot be externally exported;
- retention class exists before activation;
- revocation is immediate for new capture points;
- expiry/revocation preserve immutable content-free history;
- extending expiry creates a new override;
- prohibited content remains prohibited.

### Local resolution

```rust
#[derive(Clone, Debug)]
pub struct CaptureResolutionInput {
    pub workspace_id: WorkspaceId,
    pub run_id: Option<AgentRunId>,
    pub agent_snapshot_id: Option<AgentRuntimeSnapshotId>,
    pub workflow_revision_id: Option<WorkflowRevisionId>,
    pub subject: CaptureSubjectKind,
    pub classification: DataClassification,
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub capture_policy_revision_id: ObservabilityCapturePolicyRevisionId,
    pub active_override_id: Option<DiagnosticCaptureOverrideId>,
    pub secret_scan_disposition: SecretScanDisposition,
    pub source_availability: CaptureSourceAvailability,
    pub storage_availability: CaptureStorageAvailability,
    pub requested_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureResolutionReason {
    ProfileBaseline,
    PolicyRuleCap,
    ClassificationCap,
    SecretDetected,
    SubjectDisabled,
    OverrideAuthorized,
    OverrideExpired,
    OverrideRevoked,
    OverrideScopeMismatch,
    RedactionUnavailable,
    SourceUnavailable,
    StorageUnavailable,
    RetentionUnavailable,
    QuarantineRequired,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureResolutionDecision {
    pub id: CaptureResolutionDecisionId,
    pub workspace_id: WorkspaceId,
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub capture_policy_revision_id: ObservabilityCapturePolicyRevisionId,
    pub diagnostic_override_id: Option<DiagnosticCaptureOverrideId>,
    pub subject: CaptureSubjectKind,
    pub profile_baseline_mode: ObservabilityCaptureMode,
    pub authorized_ceiling_mode: ObservabilityCaptureMode,
    pub resolved_mode: ObservabilityCaptureMode,
    pub reasons: Vec<CaptureResolutionReason>,
    pub replayability_effect: ReplayabilityEffect,
    pub retention_class: RetentionClassRef,
    pub content_hash: [u8; 32],
    pub resolved_at: Timestamp,
}
```

Resolution decision stores no captured content and no secret finding value.

### Exporter resolution

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureExportReason {
    LocalResolvedMode,
    ExporterMaximumMode,
    CurrentPolicyCap,
    ClassificationCap,
    DestinationDenied,
    ArtifactUnavailable,
    SecretDetected,
    Purged,
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureExportDecision {
    pub id: CaptureExportDecisionId,
    pub workspace_id: WorkspaceId,
    pub capture_record_id: ObservabilityCaptureRecordId,
    pub resolution_decision_id: CaptureResolutionDecisionId,
    pub exporter_binding_revision_id: TelemetryExporterBindingRevisionId,
    pub local_resolved_mode: ObservabilityCaptureMode,
    pub exporter_maximum_mode: ObservabilityCaptureMode,
    pub exported_mode: ObservabilityCaptureMode,
    pub current_policy_decision_id: PolicyDecisionId,
    pub reasons: Vec<CaptureExportReason>,
    pub payload_hash: [u8; 32],
    pub decided_at: Timestamp,
}
```

One capture can have zero or many exporter decisions. An exporter decision is immutable and does not alter the local capture resolution.

### Deterministic algorithms

Local resolution:

```text
1. baseline = mode from pinned profile revision
2. authorized_ceiling = baseline
3. valid exact-scope override may set:
     authorized_ceiling = max(baseline, override.maximum_mode)
4. resolved = min(
     authorized_ceiling,
     capture policy rule cap,
     classification cap,
     secret-scan cap,
     subject cap,
     source/storage/redaction availability cap
   )
5. persist canonical reasons and exact revision references
```

Exporter resolution:

```text
exported = min(
  local resolved mode,
  exporter maximum mode,
  current export policy cap,
  current classification/destination cap
)
```

Rules:

- no matching optional capture rule resolves local capture to `Disabled`;
- policy lookup failure resolves optional local capture to `Disabled`;
- secret detection resolves content modes to `MetadataOnly` or `Disabled` according to exact policy;
- redaction failure cannot produce `Full`;
- missing H6 storage cannot produce a content mode;
- missing exporter configuration does not alter local resolved mode;
- reason ordering is stable and deterministic;
- identical inputs produce identical decision hashes.

### Capture record extension

Extend H10 `ObservabilityCaptureRecord` with local decision provenance only:

```rust
pub struct ObservabilityCaptureRecord {
    // existing H10 fields remain
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub resolution_decision_id: CaptureResolutionDecisionId,
    pub profile_baseline_mode: ObservabilityCaptureMode,
    pub resolved_mode: ObservabilityCaptureMode,
    pub resolution_reason_codes: Vec<CaptureResolutionReason>,
    pub replayability_effect: ReplayabilityEffect,
}
```

The existing H10 `selected_mode` value is interpreted as local resolved mode by a forward-compatible read adapter. Historical rows are exposed as `LegacyCapturePolicyOnly`; they are not rewritten with fabricated profile revisions.

---

## Persistence model

### Migration `0087_state_capture_profiles_and_selections.sql`

Create:

```text
state_capture_profile_definitions
state_capture_profile_revisions
state_capture_selections
state_capture_selection_history
```

Constraints:

- stable definition uniqueness by `(deployment_id, profile_kind)`;
- revision uniqueness by `(definition_id, revision)`;
- exact approved baseline mapping check;
- positive revision numbers;
- one active selection per normalized scope key;
- composite workspace checks for agent/workflow/Run scopes;
- profile revisions and selection history are append-only for application role;
- mutable selection replacement requires expected `state_revision`;
- forced RLS on workspace-owned selection/history rows;
- deployment rows use the existing deployment-admin path.

Seed deterministic revision 1 definitions:

```text
Minimal      / MetadataOnly
Operational  / StructuredOnly
Reproducible / Redacted
Forensic     / Full
```

Integration tests verify exact canonical seed hashes.

### Migration `0088_state_capture_overrides_resolutions_exports_and_rls.sql`

Create:

```text
diagnostic_capture_overrides
capture_resolution_decisions
capture_export_decisions
```

Alter the H10 capture-record table with nullable forward-compatible columns:

```text
profile_revision_id
resolution_decision_id
profile_baseline_mode
resolved_mode
resolution_reason_codes
replayability_effect
```

Constraints:

- override `valid_until > valid_from`;
- override scope/Run belongs to one workspace;
- override maximum mode uses the approved escalation set;
- revocation is irreversible;
- resolution and exporter decisions are immutable and content-free;
- capture/decision/export references remain in one workspace;
- forced RLS on every workspace-owned table;
- indexes cover active selection lookup, active override lookup, expiry, Run/profile history, capture resolution and exporter delivery;
- application role cannot update/delete immutable revisions, history or decisions.

No migration rewrites historical event journals or content payloads.

---

## Application ports

```rust
#[async_trait::async_trait]
pub trait StateCaptureProfileRepositoryPort: Send + Sync {
    async fn get_profile_revision(
        &self,
        context: &RequestContext,
        id: StateCaptureProfileRevisionId,
    ) -> Result<StateCaptureProfileRevision, ApplicationError>;

    async fn resolve_active_selection(
        &self,
        context: &RequestContext,
        query: StateCaptureSelectionQuery,
    ) -> Result<ResolvedStateCaptureSelection, ApplicationError>;

    async fn replace_selection(
        &self,
        context: &RequestContext,
        command: ReplaceStateCaptureSelection,
    ) -> Result<StateCaptureSelection, ApplicationError>;
}

#[async_trait::async_trait]
pub trait DiagnosticCaptureOverrideRepositoryPort: Send + Sync {
    async fn find_active_override(
        &self,
        context: &RequestContext,
        query: ActiveDiagnosticOverrideQuery,
    ) -> Result<Option<DiagnosticCaptureOverride>, ApplicationError>;

    async fn create_override(
        &self,
        context: &RequestContext,
        value: DiagnosticCaptureOverride,
        audit_intent: SecurityAuditIntent,
    ) -> Result<DiagnosticCaptureOverride, ApplicationError>;

    async fn revoke_override(
        &self,
        context: &RequestContext,
        command: RevokeDiagnosticCaptureOverride,
        audit_intent: SecurityAuditIntent,
    ) -> Result<DiagnosticCaptureOverride, ApplicationError>;
}

pub trait CaptureProfileResolverPort: Send + Sync {
    fn resolve(
        &self,
        input: CaptureResolutionInput,
    ) -> Result<CaptureResolutionDecision, ApplicationError>;
}

pub trait CaptureExporterResolverPort: Send + Sync {
    fn resolve_export(
        &self,
        input: CaptureExportResolutionInput,
    ) -> Result<CaptureExportDecision, ApplicationError>;
}

#[async_trait::async_trait]
pub trait CaptureResolutionStorePort: Send + Sync {
    async fn commit_capture_resolution(
        &self,
        context: &RequestContext,
        decision: CaptureResolutionDecision,
        record: ObservabilityCaptureRecord,
        optional_capture_artifact: Option<PreparedCaptureArtifact>,
        mandatory_audit_intent: SecurityAuditIntent,
    ) -> Result<CommittedCaptureRecord, ApplicationError>;
}
```

`commit_capture_resolution` uses the existing H6 staged/promoted/finalized Artifact protocol and cannot expose a capture record pointing at unavailable bytes.

---

## Task 1: Add profile IDs, enums and immutable revisions

**Files:**

- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/observability/profile.rs`
- Modify: `crates/vestrace-domain/src/observability/mod.rs`
- Modify: `crates/vestrace-domain/src/observability/capture.rs`
- Test: inline unit/property tests

- [ ] **Step 1: Write failing mapping and type-separation tests**

Test all four exact mappings, separate profile/mode serialization and rejection of mismatched stored mappings.

```rust
#[test]
fn approved_profiles_map_to_exact_h10_baselines() {
    assert_eq!(StateCaptureProfile::Minimal.baseline(), ObservabilityCaptureMode::MetadataOnly);
    assert_eq!(StateCaptureProfile::Operational.baseline(), ObservabilityCaptureMode::StructuredOnly);
    assert_eq!(StateCaptureProfile::Reproducible.baseline(), ObservabilityCaptureMode::Redacted);
    assert_eq!(StateCaptureProfile::Forensic.baseline(), ObservabilityCaptureMode::Full);
}
```

Run:

```bash
cargo test -p vestrace-domain observability::profile
```

Expected before implementation: compilation failure because profile types do not exist.

- [ ] **Step 2: Implement IDs and validated constructors**
- [ ] **Step 3: Implement canonical revision hashing**
- [ ] **Step 4: Add properties proving invalid mappings cannot construct and semantic changes alter hashes**
- [ ] **Step 5: Run**

```bash
cargo test -p vestrace-domain observability::profile
cargo clippy -p vestrace-domain --all-targets -- -D warnings
```

- [ ] **Step 6: Commit**

```bash
git add crates/vestrace-domain
git commit -m "feat(observability): add State capture profile domain types"
```

---

## Task 2: Implement pure local and exporter resolvers

**Files:**

- Modify: `crates/vestrace-application/src/observability/capture_policy.rs`
- Create: `crates/vestrace-application/src/observability/profile_service.rs`
- Modify: `crates/vestrace-application/src/observability/export.rs`
- Modify: `crates/vestrace-application/src/observability/mod.rs`
- Create: `crates/vestrace-application/tests/state_capture_profile_resolution.rs`
- Create: `crates/vestrace-application/tests/state_capture_export_resolution.rs`
- Create: `crates/vestrace-application/tests/state_capture_failure_semantics.rs`

- [ ] **Step 1: Write failing local-resolution matrix**

Cover:

1. Minimal plus optional subject disabled resolves `Disabled` while mandatory audit remains required.
2. Operational never resolves arbitrary prompt/Tool bodies above `StructuredOnly`.
3. Reproducible reaches `Redacted` only when redaction and H6 storage are available.
4. Forensic does not self-authorize Full content.
5. Valid exact-scope override may raise profile ceiling but not policy/classification ceilings.
6. Expired, revoked or wrong-Run overrides are ignored with explicit reasons.
7. Redaction failure reduces mode and never exposes original bytes.
8. Secret detection reduces content mode without storing the secret.
9. Policy lookup failure resolves optional capture to `Disabled`.
10. Same input produces same decision hash/reason order.

- [ ] **Step 2: Write failing exporter matrix**

Cover one local capture exported to several bindings with different maximum modes. Prove exporter resolution never changes the local capture decision.

- [ ] **Step 3: Implement side-effect-free resolvers**

Resolvers receive fully loaded immutable revisions and bounded facts. They perform no SQL, network, clock, H6, H8 or H2 calls.

- [ ] **Step 4: Add monotonicity property tests**

Prove restrictive caps, override removal, storage failure, redaction failure and lower exporter ceilings cannot increase disclosure.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_profile_resolution
cargo test -p vestrace-application --test state_capture_export_resolution
cargo test -p vestrace-application --test state_capture_failure_semantics
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application
git commit -m "feat(observability): resolve State capture profiles deterministically"
```

---

## Task 3: Add forward-only persistence and repositories

**Files:**

- Create: `migrations/0087_state_capture_profiles_and_selections.sql`
- Create: `migrations/0088_state_capture_overrides_resolutions_exports_and_rls.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/observability/profile_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/policy_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/capture_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/exporter_repository.rs`
- Create: `tests/state_capture_profile_persistence.rs`
- Create: `tests/state_capture_profile_rls.rs`
- Create: `tests/state_capture_profile_history.rs`
- Create: `scripts/verify-state-capture-migration-ownership.sh`

- [ ] **Step 1: Write failing DB invariant tests**

Assert exact seeds/hashes, immutable revisions/decisions, one active selection per scope, optimistic conflict behavior, invalid override intervals, cross-workspace rejection, forced RLS and no earlier migration edits.

- [ ] **Step 2: Implement migration 0087**
- [ ] **Step 3: Implement migration 0088**
- [ ] **Step 4: Implement scoped repositories and atomic selection-history replacement**
- [ ] **Step 5: Add migration-ownership script**
- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_persistence
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_rls
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_history
bash scripts/verify-state-capture-migration-ownership.sh
git add migrations crates/vestrace-infrastructure tests scripts
git commit -m "feat(storage): persist State capture profiles and decisions"
```

---

## Task 4: Implement selection precedence and Run pinning

**Files:**

- Modify: `crates/vestrace-application/src/observability/ports.rs`
- Modify: `crates/vestrace-application/src/observability/profile_service.rs`
- Modify: `crates/vestrace-application/src/composition/observability.rs`
- Create: `crates/vestrace-application/tests/state_capture_profile_selection.rs`
- Create: `tests/state_capture_profile_restart.rs`

- [ ] **Step 1: Write failing precedence/pinning tests**

Precedence:

```text
Run > Workflow > Agent > Workspace > Deployment
```

Precedence selects a baseline preset; it does not grant authority.

Prove existing Runs remain pinned after default changes, new Runs observe new defaults, restart reloads the same pin and selection replacement writes immutable history.

- [ ] **Step 2: Implement H2-governed selection commands**

```text
observability.capture_profile.select
observability.capture_profile.clear
```

- [ ] **Step 3: Integrate Run pinning through the approved H10/H1 extension point without creating a second Run aggregate**
- [ ] **Step 4: Add restart test**
- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_profile_selection
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_restart
git add crates/vestrace-application tests
git commit -m "feat(observability): select and pin State capture profiles"
```

---

## Task 5: Add H2-authorized diagnostic override lifecycle

**Files:**

- Create: `crates/vestrace-application/src/observability/diagnostic_override.rs`
- Modify: `crates/vestrace-application/src/observability/ports.rs`
- Modify: `crates/vestrace-application/src/composition/observability.rs`
- Create: `crates/vestrace-application/tests/state_capture_diagnostic_override.rs`
- Create: `tests/state_capture_profile_h2_authorization.rs`

- [ ] **Step 1: Write failing authorization/lifecycle tests**

Cover missing base capability, explicit deny, exact operation fingerprint, one-use tickets/grants, expiry/revocation, content-free history and ambiguous approval transport.

- [ ] **Step 2: Define exact actions**

```text
observability.diagnostic_capture.create
observability.diagnostic_capture.revoke
observability.diagnostic_capture.read
```

- [ ] **Step 3: Implement create/revoke transactions with mandatory audit intent**
- [ ] **Step 4: Implement operational expiry sweep; direct timestamp checks remain authoritative**
- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_diagnostic_override
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_h2_authorization
git add crates/vestrace-application tests
git commit -m "feat(observability): govern diagnostic capture overrides"
```

---

## Task 6: Integrate local resolution with H10 and H6

**Files:**

- Modify: `crates/vestrace-application/src/observability/capture_policy.rs`
- Modify: `crates/vestrace-application/src/observability/ports.rs`
- Modify: `crates/vestrace-application/src/composition/observability.rs`
- Modify: `crates/vestrace-domain/src/observability/capture.rs`
- Modify: `crates/vestrace-domain/src/artifact/revision.rs`
- Create: `tests/state_capture_profile_h6_integration.rs`
- Modify: `tests/capture_artifact_purge.rs`
- Modify: `scripts/verify-no-hidden-reasoning-capture.sh`

- [ ] **Step 1: Write failing end-to-end capture tests**

Cover Minimal metadata + audit, Operational allowlisted structure, Reproducible redacted H6 Artifact, authorized Forensic Full, redaction downgrade, storage outage, purge propagation and hidden-reasoning rejection.

- [ ] **Step 2: Extend H6 provenance**

```rust
ArtifactRevisionSource::ObservabilityCapture {
    capture_record_id: ObservabilityCaptureRecordId,
    resolution_decision_id: CaptureResolutionDecisionId,
}
```

- [ ] **Step 3: Implement orchestration**

```text
load pinned profile + current policies
→ resolve local mode
→ prepare bounded representation
→ for content mode: stage/inspect/redact through H6
→ commit resolution + capture record + mandatory audit intent
→ finalize H6 metadata
→ expose only when H6 lifecycle permits
```

- [ ] **Step 4: Store only bounded failure categories/fingerprints**
- [ ] **Step 5: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_h6_integration
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test capture_artifact_purge
bash scripts/verify-no-hidden-reasoning-capture.sh
git add crates tests scripts
git commit -m "feat(observability): capture profile content through H6 lifecycle"
```

---

## Task 7: Implement per-exporter decisions and delivery caps

**Files:**

- Modify: `crates/vestrace-application/src/observability/export.rs`
- Modify: `crates/vestrace-application/src/observability/delivery.rs`
- Modify: `crates/vestrace-domain/src/observability/capture.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/exporter_repository.rs`
- Create: `crates/vestrace-application/tests/state_capture_export_resolution.rs`

- [ ] **Step 1: Write failing exporter tests**

Prove exporter receives local resolved mode rather than profile authority; several exporters can receive different bounded modes; current authorization is rechecked; H8 credentials never enter payloads; retry reuses immutable payload; purged content exports an omission/tombstone fact.

- [ ] **Step 2: Implement immutable export-decision builder**
- [ ] **Step 3: Preserve independent delivery state without mutating source capture or Run**
- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_export_resolution
git add crates/vestrace-application crates/vestrace-domain crates/vestrace-infrastructure
git commit -m "feat(observability): cap State capture profile exports"
```

---

## Task 8: Add H11 HTTP, CLI and public schemas

**Files:**

- Modify: `crates/vestrace-public-schema/src/observability.rs`
- Modify: `crates/vestrace-channel-http/src/observability_routes.rs`
- Modify: `crates/vestrace-channel-cli/src/observability_commands.rs`
- Regenerate OpenAPI/JSON Schema through the repository generator
- Create: `tests/state_capture_profile_h11_parity.rs`

- [ ] **Step 1: Write failing public-contract tests**

Expose profile kind/revision, selection scope/state revision, local resolved mode, bounded downgrade reasons, exporter decision, override scope/expiry, replayability and retention/purge state.

Never expose raw restricted policy, secret findings, key references, hidden reasoning, backend paths, arbitrary SQL fields or authorization-ticket material.

- [ ] **Step 2: Add shared commands/queries**

```text
GET  /admin/v1/observability/capture-profiles
GET  /admin/v1/observability/capture-selections
PUT  /admin/v1/observability/capture-selections/{scope}
POST /admin/v1/observability/diagnostic-overrides
POST /admin/v1/observability/diagnostic-overrides/{id}/revoke
GET  /v1/observability/captures/{id}
GET  /v1/observability/captures/{id}/exports
```

State changes require `Idempotency-Key` and expected version where applicable.

- [ ] **Step 3: Enforce HTTP/CLI parity and bounded MCP read surfaces**
- [ ] **Step 4: Generate schemas; profile and mode remain separate definitions**
- [ ] **Step 5: Run and commit**

```bash
cargo test --test state_capture_profile_h11_parity
cargo test -p vestrace-public-schema
bash scripts/verify-public-schema-drift.sh
git add crates tests generated
git commit -m "feat(api): expose State capture profile management"
```

Use the repository’s generated-output directory discovered in the implementation checkout; do not hand-edit generated files.

---

## Task 9: Complete restart, security and acceptance verification

**Files:**

- Create: `tests/state_capture_profile_acceptance.rs`
- Create: `scripts/verify-state-capture-profile-boundary.sh`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Encode ADR acceptance scenarios**

Prove:

1. Minimal preserves mandatory audit while optional capture is Disabled.
2. Operational never stores arbitrary prompt/Tool payload.
3. Reproducible creates redacted H6 content and exact revision manifests.
4. Forensic without exact authorization cannot create Full capture.
5. Valid override is bounded by workspace, Run, subjects, exporters, retention and expiry.
6. Policy/classification/secret/exporter caps only reduce disclosure.
7. Default changes do not rewrite history or alter pinned Runs.
8. Current policy can downgrade later capture points in an older Run.
9. Redaction failure never falls back to Full.
10. Storage failure never writes payload to ordinary logs.
11. Purge removes optional content and keeps content-free proof.
12. Hidden reasoning and secret bytes are rejected.
13. Local/export decisions remain deterministic across restart.
14. Foreign workspace access fails under RLS.
15. Canonical H1–H9 event counts are identical across profiles for identical logical operations.

- [ ] **Step 2: Add static boundary checks**

Fail on profile-based canonical event filtering, profile/mode type conflation, capture content in logs, direct agent/channel SQL, H10 bytes outside H6 ports, hidden-reasoning DTO fields, profile-as-export-authority and migration reuse.

- [ ] **Step 3: Add focused CI jobs**
- [ ] **Step 4: Run full verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test state_capture_profile_acceptance
bash scripts/verify-state-capture-profile-boundary.sh
bash scripts/verify-no-hidden-reasoning-capture.sh
bash scripts/verify-state-capture-migration-ownership.sh
git diff --check
```

- [ ] **Step 5: Review diff ownership**

Confirm only migrations `0087`/`0088` are added, H1 Run state is not duplicated, H10 remains capture owner, H6 remains byte owner, H2 remains final authorization owner, H8 remains secret owner and H11 remains adapter-only.

- [ ] **Step 6: Commit**

```bash
git add .github tests scripts
git commit -m "test(observability): verify State capture profile boundaries"
```

---

## Required security review

Verify with code/tests:

- profile selection cannot create missing capability;
- approval cannot override explicit deny or hard ceiling;
- Full capture requires exact unexpired authority when ordinary policy does not permit it;
- override/profile/hash/Artifact IDs are not bearer capabilities;
- optional capture failure does not suppress mandatory audit;
- secret values never enter DB rows, logs, errors, metrics or DTOs;
- H6 quarantine/inspection precede content availability/export;
- exporter destination and credential binding are exact immutable revisions;
- export retries cannot increase disclosure;
- purge reaches derived representations and replay fixtures;
- historical decisions remain immutable after profile/policy change;
- RLS is forced/tested for every workspace table;
- resolution monotonicity is property-tested;
- replayability degrades honestly;
- hidden reasoning is prohibited everywhere.

---

## Completion gate

A future implementation branch is complete only when it demonstrates:

1. exact mappings with distinct profile/mode types;
2. immutable profile revisions and auditable scoped selections;
3. deterministic monotonic local resolution;
4. separate deterministic exporter decisions;
5. H2-authorized, bounded, revocable overrides;
6. H6-governed Redacted/Full lifecycle;
7. H11 transport parity without new authority;
8. restart-safe pinning/history;
9. forced-RLS isolation;
10. all ADR acceptance scenarios and boundary scripts passing;
11. no canonical event suppression, hidden reasoning capture, plaintext fallback or direct SQL;
12. no migration changes outside `0087` and `0088`.

Merging this plan authorizes none of those actions. The repository remains documentation-only until the user explicitly ends that phase.