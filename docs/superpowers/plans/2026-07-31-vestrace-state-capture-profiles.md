# Vestrace State Capture Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` while implementing each task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this document does not authorize a feature branch, Rust changes, SQL migrations, production capture, diagnostic overrides, exporter changes or additional retained content. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved ADR: `docs/superpowers/specs/2026-07-31-vestrace-state-capture-profiles-adr.md`;
- ADR merge commit: `24f02ed362b2af0361fcc9d499d275e21257c0e2`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- cross-plan normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- normalization merge commit: `788d47ec6c3ac5e0336f3cc8fb4af4df685da3a3`.

**Goal:** Implement operator-facing `Minimal`, `Operational`, `Reproducible` and `Forensic` capture profiles as versioned policy presets over the existing H10 `ObservabilityCaptureMode` lattice, while preserving mandatory audit, H6-governed capture bytes, H2 authorization, H8 secret boundaries, H11 transport parity and deterministic downgrade behavior.

**Architecture:** H10 remains the sole owner of capture modes, capture records, telemetry delivery and audit/evaluation semantics. This plan adds immutable profile definitions, scoped profile selections, bounded diagnostic overrides and a pure resolution service. A profile contributes only a baseline ceiling. Current H2/H6/H8/H10 policy, classification, secret findings, storage availability and exporter bindings may reduce disclosure. They cannot silently increase it. `Redacted` and `Full` content remains H6 Artifact content. Canonical H1–H9 events and mandatory security-audit facts remain independent of profile selection.

**Tech stack:** Existing Vestrace v0.1 plus H1–H11 plans; Rust Edition 2024; Tokio; Serde/Schemars; SQLx; PostgreSQL 17; H2 policy and authorization tickets; H6 Artifact lifecycle; H8 credential/secret boundaries; H10 observability/audit contracts; H11 HTTP/CLI/MCP/SDK surfaces; proptest; deterministic fake clocks and local test fixtures.

---

## Global constraints

- Complete the relevant v0.1, H2, H6, H8, H10 and H11 implementation plans before implementing this extension.
- H10 `ObservabilityCaptureMode` remains exactly:

```text
Disabled < MetadataOnly < StructuredOnly < Redacted < Full
```

- State capture profiles remain exactly:

```text
Minimal < Operational < Reproducible < Forensic
```

- Profile and mode are different types and are never serialized through one enum or database column.
- The normative baseline mapping is:

```text
Minimal      -> MetadataOnly
Operational  -> StructuredOnly
Reproducible -> Redacted
Forensic     -> Full
```

- A profile does not create, suppress, rename or rewrite a canonical domain event.
- A profile does not grant read, export, model-transfer, retention, legal-hold, secret-access or debugging authority.
- `Disabled` applies only to optional capture. It never disables mandatory content-free audit, policy references, accounting facts, validation outcomes or bounded health counters.
- Ordinary resolution may only keep or reduce the profile baseline.
- Raising the profile ceiling requires an exact, time-bounded diagnostic override authorized through H2 and still capped by classification, H6/H8 policy and exporter policy.
- `Forensic` is not a blanket “record everything” mode.
- Hidden chain-of-thought, provider reasoning-token content, private model scratchpads, credential bytes and unrestricted external bodies remain prohibited in every profile and mode.
- `Redacted` and `Full` bytes are H6 Artifact revisions or representations. H10 never stores those bytes directly in PostgreSQL or a second blob store.
- Redaction failure never falls back to `Full`.
- Capture storage failure never falls back to ordinary logs, temporary plaintext files or an ungoverned backend.
- Exported mode is always:

```text
min(resolved_capture_mode, exporter_maximum_capture_mode)
```

- A running Run pins its selected profile revision. Changing a deployment or workspace default does not rewrite historical capture records and does not silently raise capture for the Run.
- Current policy is re-evaluated for every new capture point and may reduce the pinned Run baseline.
- Profile revisions, selections, diagnostic overrides and resolution facts are workspace-isolated where applicable and use forced RLS.
- Existing applied migrations are never edited. This plan uses forward migrations after the H11 reserved range.
- Migration ownership for this concern is:

```text
0087_state_capture_profiles_and_selections.sql
0088_state_capture_overrides_resolution_and_rls.sql
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
  state_capture_export_cap.rs
  state_capture_failure_semantics.rs

crates/vestrace-infrastructure/src/postgres/
  observability/mod.rs
  observability/profile_repository.rs
  observability/policy_repository.rs
  observability/capture_repository.rs

crates/vestrace-channel-http/src/
  observability_routes.rs

crates/vestrace-channel-cli/src/
  observability_commands.rs

crates/vestrace-public-schema/src/
  observability.rs

migrations/
  0087_state_capture_profiles_and_selections.sql
  0088_state_capture_overrides_resolution_and_rls.sql

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

### IDs

Add strongly typed identifiers:

```rust
StateCaptureProfileDefinitionId
StateCaptureProfileRevisionId
StateCaptureSelectionId
DiagnosticCaptureOverrideId
CaptureResolutionDecisionId
```

All identifiers use the repository UUID strategy and reject nil values.

### Profile kind and immutable definition revision

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
- `Forensic` may set `diagnostic_override_allowed = true` only when deployment policy supports exact overrides;
- retention is a reference to an existing policy class, not a duration copied into the profile;
- revision is immutable;
- content hash covers every semantic field in canonical order.

The initial implementation seeds one revision for each approved profile. Changing mapping or default retention creates a new immutable revision and requires a separate approved policy change.

### Scoped selection

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum StateCaptureSelectionScope {
    Deployment,
    Workspace { workspace_id: WorkspaceId },
    Agent { workspace_id: WorkspaceId, agent_revision_id: AgentRevisionId },
    Workflow { workspace_id: WorkspaceId, workflow_revision_id: WorkflowRevisionId },
    Run { workspace_id: WorkspaceId, run_id: AgentRunId },
}

#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct StateCaptureSelection {
    pub id: StateCaptureSelectionId,
    pub scope: StateCaptureSelectionScope,
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub expected_scope_revision: u64,
    pub state_revision: u64,
    pub active: bool,
    pub selected_by: PrincipalId,
    pub policy_decision_id: PolicyDecisionId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Selection rules:

- one active selection exists per exact scope;
- selection is a mutable lifecycle identity with optimistic concurrency;
- changing the profile creates a new state revision and immutable selection-history fact;
- a Run selection pins the exact profile revision for the Run;
- workspace, agent and workflow references belong to the same workspace;
- a deployment selection cannot reference workspace-owned resources;
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

- `maximum_mode` is `Redacted` or `Full`; ordinary lower modes do not need an escalation object;
- duration is positive and bounded by deployment policy;
- workspace and optional Run match the H2 operation fingerprint;
- subject set is non-empty and bounded;
- exporter list is explicit; an empty list means no external exporter may receive escalated content;
- retention class exists before activation;
- revocation is immediate for new capture points;
- expiry and revocation never delete historical audit facts;
- extending expiry creates a new override, not an in-place extension;
- the override cannot authorize prohibited content.

### Resolution input and decision

```rust
#[derive(Clone, Debug)]
pub struct CaptureResolutionInput {
    pub workspace_id: WorkspaceId,
    pub run_id: Option<AgentRunId>,
    pub agent_revision_id: Option<AgentRevisionId>,
    pub workflow_revision_id: Option<WorkflowRevisionId>,
    pub subject: CaptureSubjectKind,
    pub classification: DataClassification,
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub capture_policy_revision_id: ObservabilityCapturePolicyRevisionId,
    pub exporter_binding_revision_id: Option<TelemetryExporterBindingRevisionId>,
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
    ExporterCap,
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
    pub exporter_mode: Option<ObservabilityCaptureMode>,
    pub reasons: Vec<CaptureResolutionReason>,
    pub replayability_effect: ReplayabilityEffect,
    pub retention_class: RetentionClassRef,
    pub content_hash: [u8; 32],
    pub resolved_at: Timestamp,
}
```

The decision stores no content and no secret finding value.

### Deterministic resolution algorithm

Resolution is a pure function over exact immutable revisions and current bounded facts:

```text
1. baseline = approved mode from pinned profile revision
2. authorized_ceiling = baseline
3. when an active exact-scope diagnostic override is valid:
     authorized_ceiling = max(baseline, override.maximum_mode)
4. resolved = min(
     authorized_ceiling,
     capture policy rule cap,
     classification cap,
     secret-scan cap,
     subject cap,
     source/storage/redaction availability cap
   )
5. exporter_mode = min(resolved, exporter maximum mode)
6. persist exact reasons and immutable revision references
```

Additional rules:

- no matching optional policy rule resolves to `Disabled`;
- policy evaluation failure resolves optional capture to `Disabled`;
- secret detection resolves content modes to `MetadataOnly` or `Disabled` according to exact policy;
- redaction failure cannot produce `Full`;
- missing H6 storage cannot produce a content mode;
- missing exporter configuration does not alter local resolved mode, only export disposition;
- output ordering of reasons is canonical and deterministic;
- the same input produces the same decision hash.

### Capture record extension

Extend H10 `ObservabilityCaptureRecord` with exact decision provenance:

```rust
pub struct ObservabilityCaptureRecord {
    // existing fields remain
    pub profile_revision_id: StateCaptureProfileRevisionId,
    pub resolution_decision_id: CaptureResolutionDecisionId,
    pub profile_baseline_mode: ObservabilityCaptureMode,
    pub resolved_mode: ObservabilityCaptureMode,
    pub exporter_mode: Option<ObservabilityCaptureMode>,
    pub resolution_reason_codes: Vec<CaptureResolutionReason>,
    pub replayability_effect: ReplayabilityEffect,
}
```

The existing `selected_mode` field is migrated to `resolved_mode` semantics through a forward-compatible read adapter. Historical H10 records created before profile support are represented as `LegacyCapturePolicyOnly` in application read models; rows are not rewritten with fabricated profile revisions.

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

Required constraints:

- stable definition uniqueness by `(deployment_id, profile_kind)`;
- revision uniqueness by `(definition_id, revision)`;
- exact approved baseline mapping check;
- positive revision numbers;
- one active selection per normalized scope key;
- composite workspace ownership checks for agent/workflow/Run scopes;
- profile revisions are append-only for the application role;
- selection history is append-only;
- all mutable selection updates require expected `state_revision`;
- forced RLS on workspace-owned selection/history rows;
- deployment rows require the existing deployment-admin database path.

Seed initial revision 1 definitions through deterministic migration data:

```text
Minimal      / MetadataOnly
Operational  / StructuredOnly
Reproducible / Redacted
Forensic     / Full
```

Seed hashes are generated from exact canonical migration constants and verified in an integration test.

### Migration `0088_state_capture_overrides_resolution_and_rls.sql`

Create:

```text
diagnostic_capture_overrides
capture_resolution_decisions
```

Alter the H10 capture record table to add nullable forward-compatible references:

```text
profile_revision_id
resolution_decision_id
profile_baseline_mode
resolved_mode
exporter_mode
resolution_reason_codes
replayability_effect
```

Required constraints:

- override `valid_until > valid_from`;
- override scope and Run belong to one workspace;
- override maximum mode is limited to approved escalation modes;
- terminal revocation is irreversible;
- one active non-overlapping override per exact workspace/Run/subject-set fingerprint unless policy explicitly permits multiple and resolution selects the lowest ceiling;
- resolution decisions are immutable and content-free;
- capture record decision references use the same workspace;
- forced RLS on every workspace-owned table;
- indexes cover active selection lookup, active override lookup, Run/profile history, capture record resolution and expiry sweeps;
- application role cannot update/delete immutable profile revisions, history rows or resolution decisions.

No trigger or migration rewrites historical event journals or capture payloads.

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
        override_value: DiagnosticCaptureOverride,
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

`commit_capture_resolution` must preserve the existing H6 staged/promoted/finalized Artifact protocol. It cannot expose a capture record pointing at unavailable bytes.

---

## Task 1: Add profile IDs, enums and immutable definition revisions

**Files:**

- Modify: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/observability/profile.rs`
- Modify: `crates/vestrace-domain/src/observability/mod.rs`
- Modify: `crates/vestrace-domain/src/observability/capture.rs`
- Test: inline unit and property tests

- [ ] **Step 1: Write failing profile mapping tests**

Test all four exact mappings, profile/mode type separation, canonical ordering and rejection of mismatched stored mappings.

```rust
#[test]
fn approved_profiles_map_to_exact_h10_baselines() {
    assert_eq!(StateCaptureProfile::Minimal.baseline(), ObservabilityCaptureMode::MetadataOnly);
    assert_eq!(StateCaptureProfile::Operational.baseline(), ObservabilityCaptureMode::StructuredOnly);
    assert_eq!(StateCaptureProfile::Reproducible.baseline(), ObservabilityCaptureMode::Redacted);
    assert_eq!(StateCaptureProfile::Forensic.baseline(), ObservabilityCaptureMode::Full);
}

#[test]
fn profile_revision_rejects_unapproved_mapping() {
    let result = profile_revision_fixture(
        StateCaptureProfile::Minimal,
        ObservabilityCaptureMode::Full,
    );
    assert!(result.is_err());
}
```

Run:

```bash
cargo test -p vestrace-domain observability::profile
```

Expected before implementation: compilation failure because profile types do not exist.

- [ ] **Step 2: Add IDs and validated profile types**

Implement the normative contracts above. Use explicit constructors and private fields where invalid combinations would otherwise be representable.

- [ ] **Step 3: Add deterministic canonical hashing**

Hash definition revisions with the repository canonicalization utility. Include a domain separator and every semantic field.

- [ ] **Step 4: Add property tests**

Generate profile revisions and prove:

- only approved mappings construct;
- revision zero is rejected;
- the same semantic revision hashes identically;
- changing any semantic field changes the hash;
- serialization round trips preserve distinct profile and mode types.

- [ ] **Step 5: Run focused verification**

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

## Task 2: Implement the pure capture-profile resolver

**Files:**

- Modify: `crates/vestrace-application/src/observability/capture_policy.rs`
- Create: `crates/vestrace-application/src/observability/profile_service.rs`
- Modify: `crates/vestrace-application/src/observability/mod.rs`
- Create: `crates/vestrace-application/tests/state_capture_profile_resolution.rs`
- Create: `crates/vestrace-application/tests/state_capture_failure_semantics.rs`

- [ ] **Step 1: Write the failing resolution matrix**

Cover at minimum:

1. Minimal plus optional subject disabled resolves to `Disabled` while mandatory audit remains required.
2. Operational never resolves arbitrary prompt or Tool body above `StructuredOnly`.
3. Reproducible resolves to `Redacted` only when redaction and H6 storage are available.
4. Forensic without a valid override is reduced by ordinary policy and cannot self-authorize `Full`.
5. A valid exact-scope override may raise the profile ceiling but not classification or exporter ceilings.
6. Expired, revoked or wrong-Run overrides are ignored and produce explicit reasons.
7. Redaction failure reduces mode and never exposes original bytes.
8. Secret detection reduces content mode without recording the secret.
9. Policy lookup failure resolves optional capture to `Disabled`.
10. Same input produces same decision hash and reason ordering.

Run:

```bash
cargo test -p vestrace-application --test state_capture_profile_resolution
cargo test -p vestrace-application --test state_capture_failure_semantics
```

- [ ] **Step 2: Implement a side-effect-free resolver**

The resolver receives fully loaded immutable revisions and bounded facts. It performs no SQL, network, clock, H6, H8 or H2 calls.

- [ ] **Step 3: Implement canonical reason ordering**

Use a stable closed enum order. Do not preserve incidental repository or policy-rule iteration order.

- [ ] **Step 4: Implement replayability effects**

Map insufficient captures to exact outcomes such as `Replayable`, `ReplayableWithOmissions`, `NotReplayable` or `InconclusiveInputCapture`. Never fabricate missing content.

- [ ] **Step 5: Add property tests for monotonic disclosure**

Prove:

- adding a restrictive cap cannot increase resolved mode;
- removing a valid override cannot increase resolved mode;
- lowering exporter maximum cannot increase exporter mode;
- storage/redaction failure cannot increase mode;
- ordinary policy cannot exceed profile baseline.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_profile_resolution
cargo test -p vestrace-application --test state_capture_failure_semantics
cargo clippy -p vestrace-application --all-targets -- -D warnings
git add crates/vestrace-application
git commit -m "feat(observability): resolve State capture profiles deterministically"
```

---

## Task 3: Add forward-only persistence and repositories

**Files:**

- Create: `migrations/0087_state_capture_profiles_and_selections.sql`
- Create: `migrations/0088_state_capture_overrides_resolution_and_rls.sql`
- Create: `crates/vestrace-infrastructure/src/postgres/observability/profile_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/policy_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/capture_repository.rs`
- Create: `tests/state_capture_profile_persistence.rs`
- Create: `tests/state_capture_profile_rls.rs`
- Create: `tests/state_capture_profile_history.rs`
- Create: `scripts/verify-state-capture-migration-ownership.sh`

- [ ] **Step 1: Write failing database invariant tests**

Assert:

- seed revisions exist with exact mappings and hashes;
- profile revisions cannot be updated or deleted by application role;
- one active selection per exact scope;
- stale expected revision loses deterministically;
- cross-workspace scope references fail;
- override with invalid interval fails;
- immutable resolution decision cannot be changed;
- capture record cannot reference another workspace decision;
- forced RLS blocks foreign workspace reads and writes;
- migrations do not edit `0073` or any earlier migration.

- [ ] **Step 2: Implement migration 0087**

Create profile definition/revision/selection/history tables, constraints, indexes, RLS and deterministic seeds.

- [ ] **Step 3: Implement migration 0088**

Create override and resolution tables; extend capture records through nullable forward-compatible columns; add constraints, indexes and forced RLS.

- [ ] **Step 4: Implement repositories**

All repository methods require a scoped `RequestContext`. Profile selection replacement and history insertion occur in one transaction.

- [ ] **Step 5: Add migration ownership script**

The script fails when:

- `0087` or `0088` appears in another plan or migration;
- an earlier migration is modified in the implementation branch;
- the migration filenames differ from this plan.

- [ ] **Step 6: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_persistence
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_rls
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_history
bash scripts/verify-state-capture-migration-ownership.sh
git add migrations crates/vestrace-infrastructure tests scripts
git commit -m "feat(storage): persist State capture profiles and resolutions"
```

---

## Task 4: Implement scoped selection and Run pinning

**Files:**

- Modify: `crates/vestrace-application/src/observability/ports.rs`
- Modify: `crates/vestrace-application/src/observability/profile_service.rs`
- Modify: `crates/vestrace-application/src/composition/observability.rs`
- Create: `crates/vestrace-application/tests/state_capture_profile_selection.rs`
- Create: `tests/state_capture_profile_restart.rs`

- [ ] **Step 1: Write failing precedence and pinning tests**

The precedence test covers:

```text
Run selection
> Workflow selection
> Agent selection
> Workspace selection
> Deployment selection
```

The selected profile is then constrained by current policy; precedence does not grant authority.

Also prove:

- a Run pins the exact profile revision at creation or first governed capture initialization;
- later workspace default changes do not alter the pinned Run revision;
- a new Run observes the new default;
- restart reloads the same pinned profile and state revision;
- selection replacement emits immutable history and uses optimistic concurrency.

- [ ] **Step 2: Implement selection commands and queries**

Commands require H2 authorization for exact action and scope:

```text
observability.capture_profile.select
observability.capture_profile.clear
```

- [ ] **Step 3: Integrate Run pinning without changing H1 authority**

Store a profile revision reference through the approved H10/H1 extension point. Do not introduce a second Run aggregate or modify Run state outside H1 commands.

- [ ] **Step 4: Add restart test**

Create a Run, resolve profile, restart application composition, change workspace default, and prove the existing Run remains pinned while new captures can still be downgraded by current policy.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_profile_selection
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_restart
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

- [ ] **Step 1: Write failing authorization and lifecycle tests**

Cover:

- missing base capability is final deny;
- explicit policy deny cannot be overridden by approval;
- authorization ticket fingerprint binds workspace, optional Run, subject set, maximum mode, exporter set, retention and expiry;
- consumed ticket cannot create a second override;
- approval grant cannot be reused outside its bound operation;
- expired/revoked override does not affect new capture points;
- old override history remains content-free and immutable;
- ambiguous external approval transport never creates an override without a committed H2 grant.

- [ ] **Step 2: Define exact actions and resources**

```text
observability.diagnostic_capture.create
observability.diagnostic_capture.revoke
observability.diagnostic_capture.read
```

The resource is the exact workspace and optional Run scope. `Full` is classified as high or critical risk according to deployment policy.

- [ ] **Step 3: Implement create/revoke services**

Creation or revocation commits:

- override lifecycle mutation;
- operation-bound H2 ticket/grant consumption;
- mandatory H10 audit intent;
- selection state revision where required.

No capture bytes are created by override activation itself.

- [ ] **Step 4: Implement expiry sweep as operational work**

Expiry marks overrides unavailable for new resolutions. Sweep retries do not change RunVersion. Resolution always checks timestamps directly, so delayed sweep cannot keep an expired override effective.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_diagnostic_override
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_h2_authorization
git add crates/vestrace-application tests
git commit -m "feat(observability): govern diagnostic capture overrides"
```

---

## Task 6: Integrate resolved profiles with H10 capture and H6 Artifact lifecycle

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

Cover:

- Minimal optional capture stores metadata only and still commits mandatory audit intent;
- Operational stores allowlisted structured fields but no arbitrary prompt/tool body;
- Reproducible stores a redacted H6 capture Artifact with exact source and generator revisions;
- Forensic stores `Full` only under a valid override and passing H6/H8 checks;
- redaction failure downgrades and never persists original bytes;
- H6 storage outage creates `CaptureUnavailable`, preserves audit and does not write bytes to logs;
- purge removes capture bytes, chunks, embeddings, excerpts and replay fixtures while preserving content-free resolution/audit facts;
- hidden reasoning input is rejected before H6 staging.

- [ ] **Step 2: Extend capture Artifact provenance**

The H10-owned source variant remains authoritative:

```rust
ArtifactRevisionSource::ObservabilityCapture {
    capture_record_id: ObservabilityCaptureRecordId,
    resolution_decision_id: CaptureResolutionDecisionId,
}
```

- [ ] **Step 3: Implement transactional orchestration**

Required sequence:

```text
load pinned profile + current policies
→ resolve exact mode
→ prepare optional structured/content representation
→ when content mode: stage H6 bytes and inspect/redact
→ commit immutable resolution + capture record + audit intent
→ finalize H6 Artifact metadata
→ expose capture only when H6 lifecycle permits
```

Use existing H6 reconciliation when blob promotion and metadata finalization diverge. Do not invent a second blob transaction protocol.

- [ ] **Step 4: Implement content-free failure records**

External error messages, raw bodies and secret findings are reduced to bounded enums and irreversible fingerprints before persistence.

- [ ] **Step 5: Run boundary scripts and tests**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_h6_integration
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test capture_artifact_purge
bash scripts/verify-no-hidden-reasoning-capture.sh
```

- [ ] **Step 6: Commit**

```bash
git add crates tests scripts
git commit -m "feat(observability): capture profile content through H6 lifecycle"
```

---

## Task 7: Enforce exporter caps and delivery downgrade

**Files:**

- Modify: `crates/vestrace-application/src/observability/export.rs`
- Modify: `crates/vestrace-application/src/observability/delivery.rs`
- Modify: `crates/vestrace-domain/src/observability/capture.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/observability/delivery_repository.rs`
- Create: `crates/vestrace-application/tests/state_capture_export_cap.rs`

- [ ] **Step 1: Write failing exporter tests**

Prove:

- exporter receives resolved mode, never profile name as authority;
- lower exporter cap deterministically downgrades output;
- exporter cannot request richer content than local resolution;
- missing current export authorization blocks delivery without altering local capture record;
- exporter authentication uses H8 and never enters capture payload;
- retry reuses immutable bounded payload and cannot mutate Run state;
- export after override expiry still rechecks current authorization and Artifact lifecycle;
- purged content exports a tombstone/omission fact rather than reconstructed bytes.

- [ ] **Step 2: Implement export projection builder**

The builder receives exact capture record, resolution decision, exporter revision and current authorization. It creates a bounded immutable delivery payload.

- [ ] **Step 3: Preserve independent delivery state**

Delivery attempts are operational/export state. They do not rewrite the capture record, resolution decision or canonical source event.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-application --test state_capture_export_cap
git add crates/vestrace-application crates/vestrace-domain crates/vestrace-infrastructure
git commit -m "feat(observability): cap State capture profile exports"
```

---

## Task 8: Add H11 HTTP, CLI and public-schema surfaces

**Files:**

- Modify: `crates/vestrace-public-schema/src/observability.rs`
- Modify: `crates/vestrace-channel-http/src/observability_routes.rs`
- Modify: `crates/vestrace-channel-cli/src/observability_commands.rs`
- Modify generated OpenAPI/JSON Schema inputs through the repository generator only
- Create: `tests/state_capture_profile_h11_parity.rs`

- [ ] **Step 1: Write failing public-contract tests**

Public DTOs expose separately:

- profile kind;
- profile revision;
- selection scope and state revision;
- resolved mode for a capture record;
- bounded downgrade reasons;
- override lifecycle, exact scope and expiry;
- replayability status;
- retention/purge state.

Public DTOs never expose:

- raw policy rules when restricted;
- secret finding details;
- credential/key references;
- hidden reasoning;
- backend blob paths;
- arbitrary SQL fields;
- authorization ticket material.

- [ ] **Step 2: Define commands and queries**

HTTP examples:

```text
GET  /admin/v1/observability/capture-profiles
GET  /admin/v1/observability/capture-selections
PUT  /admin/v1/observability/capture-selections/{scope}
POST /admin/v1/observability/diagnostic-overrides
POST /admin/v1/observability/diagnostic-overrides/{id}/revoke
GET  /v1/observability/captures/{id}
```

All state-changing commands require `Idempotency-Key` and expected version where applicable.

CLI examples:

```text
vestrace observability profile list
vestrace observability profile select
vestrace observability override create
vestrace observability override revoke
vestrace observability capture inspect
```

MCP exposes only bounded read/query surfaces unless the existing H11 capability profile explicitly permits an administrative command. MCP never receives Full capture bytes by default.

- [ ] **Step 3: Enforce transport parity**

HTTP and CLI call the same application commands/queries. Transport does not alter idempotency or authority.

- [ ] **Step 4: Generate schemas and verify compatibility**

Run the repository schema generator. Ensure profile and mode remain separate schema definitions.

- [ ] **Step 5: Run and commit**

```bash
cargo test --test state_capture_profile_h11_parity
cargo test -p vestrace-public-schema
bash scripts/verify-public-schema-drift.sh
git add crates tests generated
git commit -m "feat(api): expose State capture profile management"
```

Use the repository’s actual generated-output directory in the implementation branch; do not hand-edit generated files.

---

## Task 9: Complete restart, security and acceptance verification

**Files:**

- Create: `tests/state_capture_profile_acceptance.rs`
- Create: `scripts/verify-state-capture-profile-boundary.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/operations/observability.md` when that operations document exists in the implemented repository

- [ ] **Step 1: Encode the ADR acceptance matrix**

The acceptance test must prove all of the following:

1. Minimal preserves mandatory audit while optional capture is Disabled.
2. Operational never stores arbitrary prompt or Tool payload.
3. Reproducible produces redacted H6 content and exact revision/reference manifests.
4. Forensic without exact authorization cannot produce Full capture.
5. Valid Forensic override is bounded by workspace, Run, subjects, exporters, retention and expiry.
6. Policy, classification, secret scanning and exporter caps only reduce disclosure.
7. Changing defaults does not rewrite history or change a pinned Run profile.
8. Current policy can downgrade later capture points inside an older Run.
9. Redaction failure never falls back to Full.
10. Storage failure never writes payload to ordinary logs.
11. Purge removes optional content and leaves content-free proof.
12. Hidden reasoning and secret bytes are never accepted.
13. Capture resolution and exporter payloads remain deterministic across restart.
14. Foreign workspace profile/override/capture access fails under RLS.
15. Canonical H1–H9 event counts are identical across profiles for the same logical operations.

- [ ] **Step 2: Add static boundary checks**

`verify-state-capture-profile-boundary.sh` fails when it finds:

- profile-based filtering in canonical Run/event persistence paths;
- a profile enum used where `ObservabilityCaptureMode` is required;
- capture content written to tracing/log macros;
- direct SQL in channel/model/agent code;
- H10-owned capture bytes outside H6 Artifact ports;
- hidden-reasoning fields in capture DTOs;
- profile names treated as export authorization;
- migration-number reuse.

- [ ] **Step 3: Add CI jobs**

CI runs focused domain/application/PostgreSQL tests and boundary scripts using deterministic local fixtures.

- [ ] **Step 4: Run the full verification set**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test \
  cargo test --test state_capture_profile_acceptance
bash scripts/verify-state-capture-profile-boundary.sh
bash scripts/verify-no-hidden-reasoning-capture.sh
bash scripts/verify-state-capture-migration-ownership.sh
git diff --check
```

- [ ] **Step 5: Review generated and migration diffs**

Confirm:

- only `0087` and `0088` are added for this concern;
- no earlier migration changed;
- no H1 Run-state schema is duplicated;
- H10 remains capture owner;
- H6 remains byte owner;
- H2 remains final authorization owner;
- H8 remains credential/secret owner;
- H11 surfaces are adapters only.

- [ ] **Step 6: Commit**

```bash
git add .github tests scripts docs
git commit -m "test(observability): verify State capture profile boundaries"
```

---

## Required security review checklist

Before requesting implementation review, verify each item with code/tests rather than prose alone:

- profile selection cannot create missing base capability;
- approval cannot override explicit deny or hard policy ceiling;
- Full capture requires exact unexpired override when ordinary policy does not already permit it;
- override ID, profile revision ID, hash or Artifact ID is not a bearer capability;
- optional capture failure does not suppress mandatory audit;
- mandatory compliance capture follows explicit pause/fail/intervention policy rather than insecure fallback;
- secret values never appear in database rows, logs, errors, metrics or public DTOs;
- H6 quarantine and inspection run before captured bytes become readable/exportable;
- exporter destination and credential binding are exact immutable revisions;
- export retries cannot create richer payloads;
- purge propagates to all H6-derived forms and replay fixtures;
- historical capture decisions remain immutable after profile/policy change;
- RLS is forced and tested for every new workspace-owned table;
- resolution monotonicity is property-tested;
- replayability degrades honestly when content is missing;
- hidden reasoning remains prohibited in every path.

---

## Completion gate

This implementation plan is complete only when a future implementation branch demonstrates:

1. exact profile-to-mode mappings with distinct types;
2. immutable profile revisions and auditable scoped selections;
3. deterministic monotonic resolution;
4. H2-authorized, bounded and revocable diagnostic overrides;
5. H6-governed Redacted/Full content lifecycle;
6. exporter downgrade and current authorization checks;
7. H11 transport parity without new authority;
8. restart-safe pinning and history;
9. forced-RLS isolation;
10. all ADR acceptance scenarios and boundary scripts passing;
11. no canonical event suppression, hidden reasoning capture, plaintext fallback or direct SQL;
12. no migration changes outside `0087` and `0088`.

Merging this plan authorizes none of those implementation actions. The branch must remain documentation-only until the user explicitly ends the documentation-only phase.