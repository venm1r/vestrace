# Vestrace Cross-Workspace Memory Sharing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Use `superpowers:test-driven-development` for every implementation task and `superpowers:verification-before-completion` before claiming a task or branch complete. Steps use checkbox (`- [ ]`) syntax for tracking.

**Documentation status:** Planning artifact only. Creating or merging this file does not authorize a feature branch, Rust changes, SQL migrations, cross-workspace access, real grants or mounts, target-side indexing, persistent copies, model use, import, export, revocation propagation or public endpoints. Implementation begins only after an explicit future instruction ending the documentation-only phase.

**Approval evidence:**

- approved design: `docs/superpowers/specs/2026-08-01-vestrace-cross-workspace-memory-sharing-design.md`;
- design merge commit: `5961d0d65c16bc5d067aeaa815d42f00becceaff`;
- base memory contract: `docs/superpowers/specs/2026-07-31-vestrace-v0.1-design.md`;
- Memory Core plan: `docs/superpowers/plans/2026-07-31-vestrace-memory-core.md`;
- State Engine boundary: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-boundary-amendment.md`;
- durable event compatibility ADR and implementation plan;
- signed Run export design and implementation plan;
- workspace envelope-encryption design and implementation plan;
- H2 authorization/policy plan;
- H6 context, Artifact and evidence plan;
- H8 connections, credentials and secret-backend plan;
- H10 observability/evaluation plan;
- H11 universal product-surface plan;
- normalization report: `docs/superpowers/specs/2026-07-31-vestrace-state-engine-normalization-report.md`;
- preceding implementation-plan merge: `25e6e916ed312352107889d9371560e7ce1cbd16`.

**Goal:** Implement explicit, disabled-by-default cross-workspace memory disclosure through source-owned immutable grant revisions and target-owned read-only mounts, with RLS-safe federated retrieval, independent source/target authorization, provenance-preserving import, immediate revocation fencing and no silent copy, indexing, authority elevation or transitive sharing.

**Architecture:** Vestrace keeps every authoritative `Memory`, `MemoryRevision`, source, derivation, conflict and lifecycle transition inside one workspace. A source workspace publishes a bounded `MemoryShareGrantRevision`; a target workspace separately accepts that exact revision into a `MemoryMount`. Activation uses a reconciled two-sided handshake rather than a cross-workspace SQL transaction. Live retrieval opens a separate source-workspace transaction under source RLS for each mount, returns only policy-bounded DTOs, and then applies target policy and origin-aware reranking. Persistent target derivatives use ordinary Memory Core writes, complete `SharedMemoryRef` provenance and fresh target encryption.

**Tech Stack:** Existing Vestrace v0.1 plus Memory Core, H1, H2, H6, H8, H10 and H11; Rust Edition 2024; Tokio; SQLx; PostgreSQL 17 with forced RLS; Serde/Schemars for safe contracts; repository-owned durable event schemas; existing outbox/job infrastructure; H6 Context Pack and cache lifecycle; H8 envelope encryption; FTS/pgvector adapters; deterministic policy/clock/provider fixtures; proptest; load and fault-injection test support.

## Global Constraints

- Workspace isolation remains the default and is never weakened by feature activation.
- Scope `global` continues to mean workspace-global only.
- `Memory`, `MemoryRevision`, `MemorySource`, `Derivation`, `MemoryConflict` and local relations remain owned by Memory Core.
- `MemoryShareGrant` is not a second Memory aggregate.
- `MemoryMount` is not an H6 Artifact mount.
- A mount is a target-owned authorization/read model, not a local Memory or source of truth.
- Source workspace controls disclosure; target workspace controls acceptance and use.
- Every grant binds exactly one source workspace and one distinct target workspace.
- Wildcard targets, organization-wide implicit grants and future-workspace grants are prohibited.
- Target acceptance is required for every exact grant revision.
- Grant revisions are immutable.
- Grant and mount lifecycle identities use optimistic concurrency.
- `expected_state_revision` belongs to commands and is never persisted as historical expectation data.
- Grant IDs, revision IDs, mount IDs, workspace IDs, hashes, memory IDs and shared references are references, not capabilities.
- `SharedMemoryRef` is namespaced and cannot convert to or masquerade as a local `MemoryId`.
- Target tables do not contain SQL foreign keys to source grant, source Memory or source revision tables.
- Source-side selection tables may reference source Memory rows only within the source workspace.
- No raw cross-workspace SQL join, privileged multi-workspace view or RLS bypass is introduced.
- Every source read runs in a distinct source-workspace transaction with forced RLS.
- A target request never changes the active PostgreSQL workspace context inside an already-open target transaction.
- Internal handshake and revocation messages are transport envelopes, not authority; receivers revalidate current state.
- Source and target policy checks are independent and intersect fail-closed.
- Target policy can reduce access immediately but cannot widen the source grant.
- A later broader target policy does not expand an existing grant.
- Source policy changes can reduce access immediately and can make a mount stale.
- Effective permission is the intersection of source grant, source current policy, target acceptance, target current policy, principal capability and exact Run/provider constraints.
- Effective content mode is the least-disclosing mode permitted by source revision, source current policy, target acceptance and target current policy.
- Read-only is the default.
- `DiscoverMetadata` does not imply `ReadContent`.
- `ReadContent` does not imply deterministic context, model context, derivation, import or export.
- Deterministic context use and model context use are separate permissions and both require `ReadContent`.
- Model/provider use is denied unless explicitly permitted by source grant and target policy.
- Tool arguments derived from mounted content still pass ordinary H2 authorization, validation and risk checks.
- Mounted content never authorizes an external commitment.
- Export is denied unless explicitly permitted by source grant and target policy.
- Local derivation and exact import are separate governed operations.
- Search, display, Context Pack inclusion, conflict detection and model invocation never create a local Memory automatically.
- Target-side FTS, pgvector, text or embedding indexes are disabled by default.
- `MetadataIndexOnly` requires `DiscoverMetadata`; content-bearing indexes require `ReadContent` and `GenerateDerivedIndex`.
- Persistent target cache/index/snapshot modes require explicit source permission, target acceptance, target encryption and cleanup obligations.
- Mounted content is foreign/untrusted data and never enters system/developer authority channels.
- Mounted content cannot grant capabilities, approvals, roles, budgets or execution authority.
- Mounted content cannot supersede local memory automatically.
- Source confidence and importance remain source facts and are never copied into local trust scores without a target assessment.
- Mount-of-mount is prohibited.
- A target cannot re-share a mounted reference to a third workspace.
- A locally imported Memory may be shared later only as a local Memory and only when accepted downstream obligations and provenance policy permit it.
- Relation traversal cannot escape the grant selection.
- A relation edge never grants access to its target node.
- Source wrapped DEKs, KEKs, backend paths and decrypt handles never enter target rows, logs, exports or DTOs.
- Persistent target representations use a fresh target DEK and target AEAD scope.
- Cross-workspace content hashes are not used for deduplication.
- Secret values are never shared through Memory; credential-like content is denied/quarantined at source.
- Source unavailable means live mount reads fail closed.
- Stale cached content is never returned as a live result.
- `StaleMetadataOnly` diagnostics contain no content and cannot be used in context.
- Unknown grant revision, schema version or source generation fails closed.
- Exact disclosed metadata and content always come from the same source revision.
- Revoked, expired, deleted or stale mounts do not disclose content.
- Source revoke is authoritative even when target acknowledgement is delayed or lost.
- Source hard purge or cryptographic erasure removes future queryability before asynchronous target cleanup completes.
- Cache/index/pinned snapshot cleanup begins with immediate non-queryable quarantine.
- Active Runs do not automatically reuse context assembled from a revoked mount.
- Historical invocations and audit records are not rewritten.
- Future retries require a rebuilt valid Context Pack.
- Downstream purge or erasure with ambiguous outcome remains `OutcomeUnknown`.
- Source does not directly delete target workspace data.
- Persistent imports/derivatives are tracked through downstream obligations accepted by target.
- `ReviewOnSourceChange` creates target review work and never rewrites a local derivative automatically.
- Exact import creates a new local Memory Candidate by default and never edits source memory.
- Import uses ordinary Memory Core idempotency, provenance, write-policy and activation rules.
- Imported Run bundles do not create grants, mounts or active foreign Memory.
- Feature capability is disabled by default at deployment and workspace levels.
- MCP/agent-facing surfaces cannot create, accept, revise, suspend or revoke grants by default.
- Admin mutations require H2 capabilities, idempotency and exact revisions.
- Audit stores content-free facts and bounded query categories, never unrestricted sensitive query text or shared content.
- Metrics use bounded-cardinality labels and do not include raw workspace/query/content identifiers.
- Existing applied migrations are never edited.
- This concern follows the envelope-encryption range and reserves:

```text
0105_memory_share_grants_revisions_collections_and_selection.sql
0106_memory_mounts_acceptance_handshake_and_lifecycle.sql
0107_memory_share_disclosures_federated_queries_budgets_and_receipts.sql
0108_memory_share_imports_indexes_snapshots_obligations_and_cleanup.sql
0109_memory_share_rls_outbox_reconciliation_events_and_worker_state.sql
```

- No other concern may reuse migration numbers `0105`–`0109`.
- No existing local-memory row is backfilled into a grant, mount or shared index.
- CI requires no public network, external model, cloud KMS, multi-tenant database or production workspace.
- Future implementation branch: `feat/cross-workspace-memory-sharing`.

---

## Compatibility with Memory Core and Owning Horizons

This extension leaves the v0.1 Memory Core invariants intact.

Implementation rules:

- `memories`, `memory_revisions`, `memory_sources`, `derivations`, `memory_conflicts`, scopes and local relations remain authoritative;
- existing cross-workspace-reference rejection remains active for ordinary Memory Core relations and foreign keys;
- `global` scope storage and query behavior are unchanged;
- source-side grant selection adapters load ordinary Memory Core revisions under source RLS;
- target-side mounted views never write to `memories`;
- persistent local import calls the existing Memory Core candidate lifecycle rather than inserting directly;
- `SourceRef` gains a typed namespaced shared-memory provenance variant only where Memory Core source contracts support it;
- the shared provenance variant stores identifiers/hashes but no cross-workspace foreign key;
- local relations may reference a local provenance proxy/typed shared reference but never a source-row foreign key;
- H2 authorizes exact grant, mount, read/use/import/export and revoke operations;
- H6 owns Context Pack inclusion, disclosed large-content handling, target cache/snapshot representations and cleanup;
- H8 owns source decrypt and fresh target re-encryption operations;
- H10 owns content-free source disclosure and target-use audit;
- H11 exposes adapters over the same application services;
- H1 receives context-dependency invalidation through an owning application port, never through direct table mutation;
- durable event kinds use the approved event-schema registry and readers-first rollout;
- existing outbox/job infrastructure is extended rather than duplicated;
- exactly one composition root registers memory-sharing workers;
- no second retrieval engine, policy engine, Memory store, event store or secret backend is introduced.

---

## Locked File Structure

```text
Cargo.toml
Cargo.lock
.github/workflows/ci.yml

crates/vestrace-domain/src/
  id.rs
  memory/
    mod.rs
    shared_ref.rs
    share_grant.rs
    share_revision.rs
    share_selection.rs
    share_permissions.rs
    share_limits.rs
    share_obligations.rs
    share_retention.rs
    share_collection.rs
    mount.rs
    mounted_view.rs
    import_proposal.rs
    offline_snapshot.rs
    downstream_status.rs
    share_event.rs
    share_error.rs
  provenance.rs
  relation.rs

crates/vestrace-application/src/
  memory_sharing/
    mod.rs
    ports.rs
    commands.rs
    queries.rs
    capability.rs
    grant_service.rs
    grant_revision_service.rs
    collection_service.rs
    invitation_service.rs
    mount_service.rs
    handshake.rs
    source_disclosure.rs
    federated_retrieval.rs
    rerank.rs
    context_use.rs
    model_use.rs
    import_service.rs
    conflict_service.rs
    index_service.rs
    offline_snapshot_service.rs
    revocation.rs
    downstream_obligations.rs
    reconciliation.rs
    audit.rs
  composition/memory_sharing.rs

crates/vestrace-application/tests/
  memory_share_capability.rs
  memory_share_grant_lifecycle.rs
  memory_share_revision.rs
  memory_share_selection.rs
  memory_mount_handshake.rs
  memory_mount_lifecycle.rs
  memory_share_federated_retrieval.rs
  memory_share_query_privacy.rs
  memory_share_context_use.rs
  memory_share_model_use.rs
  memory_share_import.rs
  memory_share_indexing.rs
  memory_share_offline_snapshot.rs
  memory_share_revocation.rs
  memory_share_downstream_cleanup.rs
  memory_share_export.rs
  memory_share_failure_semantics.rs

crates/vestrace-memory-sharing-runtime/
  Cargo.toml
  src/
    lib.rs
    canonical.rs
    selection_compiler.rs
    bounded_filter.rs
    handshake_envelope.rs
    source_client.rs
    source_transaction.rs
    query_privacy.rs
    quota.rs
    result_budget.rs
    origin_rerank.rs
    invalidation.rs
    error.rs

crates/vestrace-memory-sharing-test-support/
  Cargo.toml
  src/
    lib.rs
    fixtures.rs
    clock.rs
    policy.rs
    source_client.rs
    handshake.rs
    retrieval.rs
    context.rs
    model.rs
    encryption.rs
    cleanup.rs
    faults.rs
    load.rs
    conformance.rs

crates/vestrace-infrastructure/src/postgres/memory_sharing/
  mod.rs
  grant_repository.rs
  grant_revision_repository.rs
  collection_repository.rs
  invitation_repository.rs
  mount_repository.rs
  handshake_repository.rs
  disclosure_repository.rs
  query_budget_repository.rs
  import_repository.rs
  index_repository.rs
  snapshot_repository.rs
  obligation_repository.rs
  cleanup_repository.rs
  inbox_repository.rs
  reconciliation_repository.rs
  worker_repository.rs

crates/vestrace-infrastructure/src/memory_sharing/
  mod.rs
  internal_source_client.rs
  source_workspace_executor.rs
  h2_adapter.rs
  h6_context_adapter.rs
  h8_encryption_adapter.rs
  h10_audit_adapter.rs
  h1_invalidation_adapter.rs

crates/vestrace-channel-http/src/
  routes/memory_share_grants.rs
  routes/memory_mounts.rs
  routes/memory_shared_search.rs
  routes/memory_import_proposals.rs
  dto/memory_sharing.rs

crates/vestrace-channel-cli/src/commands/memory_sharing.rs
crates/vestrace-channel-mcp/src/tools/memory_shared_search.rs
crates/vestrace-public-schema/src/memory_sharing.rs
crates/vestrace-sdk-rust/src/memory_sharing.rs
packages/sdk-typescript/src/memory-sharing.ts
packages/sdk-typescript/src/generated/memory-sharing.ts

schemas/memory-sharing/v1/grant.schema.json
schemas/memory-sharing/v1/grant-revision.schema.json
schemas/memory-sharing/v1/mount.schema.json
schemas/memory-sharing/v1/shared-reference.schema.json
schemas/memory-sharing/v1/disclosure-receipt.schema.json
schemas/memory-sharing/v1/import-proposal.schema.json
schemas/memory-sharing/v1/offline-snapshot.schema.json
schemas/memory-sharing/v1/downstream-obligation.schema.json
schemas/compatibility/memory-sharing-schema-baseline.json

migrations/0105_memory_share_grants_revisions_collections_and_selection.sql
migrations/0106_memory_mounts_acceptance_handshake_and_lifecycle.sql
migrations/0107_memory_share_disclosures_federated_queries_budgets_and_receipts.sql
migrations/0108_memory_share_imports_indexes_snapshots_obligations_and_cleanup.sql
migrations/0109_memory_share_rls_outbox_reconciliation_events_and_worker_state.sql

tests/memory_share_grant_persistence.rs
tests/memory_share_mount_persistence.rs
tests/memory_share_no_cross_workspace_fk.rs
tests/memory_share_rls.rs
tests/memory_share_global_scope.rs
tests/memory_share_handshake_restart.rs
tests/memory_share_source_unavailable.rs
tests/memory_share_uuid_guess.rs
tests/memory_share_revision_consistency.rs
tests/memory_share_read_only.rs
tests/memory_share_no_silent_copy.rs
tests/memory_share_no_default_index.rs
tests/memory_share_federated_query.rs
tests/memory_share_graph_boundary.rs
tests/memory_share_context_invalidation.rs
tests/memory_share_import_provenance.rs
tests/memory_share_target_encryption.rs
tests/memory_share_revocation_restart.rs
tests/memory_share_source_change_review.rs
tests/memory_share_purge_cascade.rs
tests/memory_share_outcome_unknown.rs
tests/memory_share_run_export.rs
tests/memory_share_h11_parity.rs
tests/memory_share_load.rs
tests/memory_share_acceptance.rs

scripts/verify-memory-share-boundary.sh
scripts/verify-memory-share-no-cross-workspace-sql.sh
scripts/verify-memory-share-no-silent-copy.sh
scripts/verify-memory-share-no-source-key-material.sh
scripts/verify-memory-share-global-remains-local.sh
scripts/verify-memory-share-revocation-fail-closed.sh
scripts/verify-memory-share-migration-ownership.sh
```

---

## Normative Domain Contracts

### Identifiers

```text
MemoryShareGrantId
MemoryShareGrantRevisionId
MemoryShareCollectionId
MemoryShareCollectionRevisionId
MemoryShareInvitationId
MemoryMountId
MemoryShareHandshakeId
MemoryDisclosureReceiptId
MemoryShareQueryBudgetId
MemoryShareImportProposalId
MemoryShareIndexId
MemoryShareIndexGenerationId
PinnedOfflineSnapshotId
DownstreamObligationJobId
DownstreamObligationItemId
MemoryShareReconciliationId
MemoryShareWorkerLeaseId
```

All IDs follow the repository UUID strategy and reject nil values.

### Feature capability

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemorySharingCapabilityState {
    Disabled,
    MetadataOnly,
    LiveRead,
    LiveReadAndPersistentUse,
}

pub struct WorkspaceMemorySharingCapabilitySnapshot {
    pub workspace_id: WorkspaceId,
    pub deployment_state: MemorySharingCapabilityState,
    pub workspace_state: MemorySharingCapabilityState,
    pub policy_bundle_revision_id: PolicyBundleRevisionId,
}

pub struct EffectiveMemorySharingCapability {
    pub source: WorkspaceMemorySharingCapabilitySnapshot,
    pub target: WorkspaceMemorySharingCapabilitySnapshot,
    pub effective_state: MemorySharingCapabilityState,
}
```

`effective_state` is the strict intersection of deployment, source workspace, target workspace and current H2 policy. `Disabled` is the default for both workspaces.

### Grant lifecycle identity

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryShareGrantState {
    Draft,
    PendingAcceptance,
    Active,
    Suspended,
    Revoked,
    Expired,
    Deleted,
}

pub struct MemoryShareGrant {
    pub id: MemoryShareGrantId,
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub state: MemoryShareGrantState,
    pub active_revision_id: Option<MemoryShareGrantRevisionId>,
    pub activation_epoch: u64,
    pub state_revision: u64,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Grant invariants:

- source and target workspace differ;
- target is exact and never wildcarded;
- `activation_epoch` is monotonic and changes on submission, activation, suspension, revision replacement and revocation;
- `Revoked`, `Expired` and `Deleted` are terminal;
- `Active` requires accepted exact revision and completed handshake;
- lifecycle transitions require expected state revision in commands;
- grant identity contains no content or secret material.

### Grant commands

```rust
pub struct SubmitGrantForAcceptance {
    pub grant_id: MemoryShareGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub expected_state_revision: u64,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}

pub struct ChangeMemoryShareGrantState {
    pub grant_id: MemoryShareGrantId,
    pub target_state: MemoryShareGrantState,
    pub expected_state_revision: u64,
    pub idempotency_key: String,
    pub requested_by: PrincipalId,
}
```

A different target requires a new grant identity.

### Grant revision

```rust
pub struct MemoryShareGrantRevision {
    pub id: MemoryShareGrantRevisionId,
    pub grant_id: MemoryShareGrantId,
    pub revision: u32,
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub purpose: BoundedText,
    pub selection: MemoryShareSelection,
    pub allowed_kinds: BTreeSet<MemoryKind>,
    pub allowed_statuses: BTreeSet<MemoryStatus>,
    pub classification_ceiling: DataClassification,
    pub content_mode: MemoryShareContentMode,
    pub operation_permissions: BTreeSet<MemoryShareOperation>,
    pub query_disclosure: QueryDisclosureMode,
    pub provider_policy: SharedProviderUsePolicy,
    pub indexing_policy: TargetIndexingPolicy,
    pub downstream_obligations: BTreeSet<DownstreamObligation>,
    pub limits: MemoryShareLimits,
    pub valid_from: Timestamp,
    pub valid_until: Option<Timestamp>,
    pub source_policy_decision_id: PolicyDecisionId,
    pub approval_grant_ids: Vec<ApprovalGrantId>,
    pub canonical_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}
```

Revision invariants:

- revision is positive and immutable;
- source/target match grant identity;
- when present, `valid_until` is later than `valid_from`;
- `valid_until` is mandatory for Restricted content, support/evaluation access, provider/model use, export, exact import and run-scoped sharing;
- permissions are internally consistent;
- deterministic/model context permissions require `ReadContent`;
- derivation/import/export permissions do not imply one another;
- `MetadataIndexOnly` requires `DiscoverMetadata`;
- content-bearing indexing requires `ReadContent`, `GenerateDerivedIndex` and persistent-use capability;
- `NoRetention` rejects all persistent target modes;
- canonical hash includes every semantic field in stable order;
- any semantic change creates a new revision.

### Selection contract

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MemoryShareSelection {
    ExplicitRevisionSet { revisions: Vec<ExactSharedRevisionSelector> },
    ExplicitMemorySetCurrentRevision { memory_ids: Vec<MemoryId> },
    NamedSourceCollection { collection_revision_id: MemoryShareCollectionRevisionId },
    BoundedTypedFilter { filter: BoundedMemoryShareFilter },
    RunScopedReferenceSet {
        run_id: AgentRunId,
        revisions: Vec<ExactSharedRevisionSelector>,
    },
}

pub struct ExactSharedRevisionSelector {
    pub memory_id: MemoryId,
    pub memory_revision_id: MemoryRevisionId,
    pub source_generation: u64,
    pub revision_content_hash: [u8; 32],
}
```

Selection limits:

- explicit sets are nonempty and deployment-bounded;
- explicit revisions belong to source workspace and pass source policy at revision creation;
- current-revision sets revalidate kind/status/classification/policy on every read;
- run-scoped selections bind exact Run and mandatory expiry;
- source collection revisions are immutable;
- filters compile only approved typed predicates;
- future unknown Memory kinds are not included automatically.

### Bounded filter

```rust
pub struct BoundedMemoryShareFilter {
    pub kinds: BTreeSet<MemoryKind>,
    pub statuses: BTreeSet<MemoryStatus>,
    pub controlled_tags: BTreeSet<ControlledTagId>,
    pub source_scope_kinds: BTreeSet<MemoryScopeKind>,
    pub created_after: Option<Timestamp>,
    pub created_before: Option<Timestamp>,
    pub valid_at: Option<Timestamp>,
    pub minimum_confidence: Option<Confidence>,
    pub maximum_confidence: Option<Confidence>,
    pub minimum_importance: Option<Importance>,
    pub maximum_importance: Option<Importance>,
    pub provenance_classes: BTreeSet<ProvenanceSourceClass>,
    pub classification_ceiling: DataClassification,
}
```

The compiler emits parameterized repository predicates and rejects raw SQL, JSONPath, regex, executable expressions and target-controlled source widening.

### Source collection

```rust
pub struct MemoryShareCollectionRevision {
    pub id: MemoryShareCollectionRevisionId,
    pub collection_id: MemoryShareCollectionId,
    pub source_workspace_id: WorkspaceId,
    pub revision: u32,
    pub members: Vec<ExactSharedRevisionSelector>,
    pub canonical_hash: [u8; 32],
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
}
```

Collection revisions are source-owned and immutable.

### Permissions and modes

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryShareOperation {
    DiscoverMetadata,
    ReadContent,
    UseInDeterministicContext,
    UseInModelContext,
    CreateLocalDerivationProposal,
    ImportExactContentProposal,
    IncludeInRunExport,
    InspectProvenance,
    InspectRelations,
    GenerateDerivedIndex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryShareContentMode {
    MetadataOnly,
    StructuredOnly,
    Redacted,
    FullAuthorized,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryDisclosureMode {
    ExactQueryAllowed,
    RedactedQueryTerms,
    StructuredFilterOnly,
    PrecomputedCollectionOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetIndexingPolicy {
    Disabled,
    MetadataIndexOnly,
    EncryptedEmbeddingIndex,
    EncryptedRedactedTextIndex,
    PinnedRevisionIndex,
}
```

### Downstream obligations

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownstreamObligation {
    NoRetention,
    NoModelUse,
    NoExternalProvider,
    NoDerivation,
    NoExactImport,
    NoExport,
    ReviewOnSourceChange,
    ReviewOnRevocation,
    PurgeImportedContentOnRevocation,
    PurgeDerivedIndexesOnRevocation,
    AttributionRequired,
    PurposeBound,
    TimeBound,
}
```

Target acceptance stores the exact obligation set and cannot remove source obligations.

### Retention classes

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryShareRetentionClass {
    GrantLifecycleMetadata,
    DisclosureAudit,
    TargetUseAudit,
    TransientQueryData,
    TargetDerivedIndex,
    ImportedMemoryContent,
    DownstreamPurgeEvidence,
}
```

Raw query and disclosed content are not retained solely for diagnostics.

### Limits

```rust
pub struct MemoryShareLimits {
    pub max_results_per_request: u32,
    pub max_total_content_bytes: u64,
    pub max_structured_payload_bytes: u64,
    pub max_provenance_nodes: u32,
    pub max_relation_depth: u16,
    pub max_requests_per_window: u32,
    pub request_window: Duration,
    pub max_context_tokens: u32,
    pub max_query_duration: Duration,
    pub max_active_runs: u32,
    pub max_mounts_per_query: u16,
}
```

Values are positive, deployment-bounded and intersected with stricter target limits.

### Mount lifecycle identity

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMountState {
    Pending,
    PendingActivation,
    Active,
    Suspended,
    Stale,
    Revoked,
    Expired,
    Deleted,
}

pub struct MemoryMount {
    pub id: MemoryMountId,
    pub target_workspace_id: WorkspaceId,
    pub source_workspace_id: WorkspaceId,
    pub source_grant_id: MemoryShareGrantId,
    pub accepted_grant_revision_id: MemoryShareGrantRevisionId,
    pub accepted_grant_revision_hash: [u8; 32],
    pub accepted_activation_epoch: u64,
    pub display_name: BoundedText,
    pub state: MemoryMountState,
    pub target_policy_revision_id: PolicyBundleRevisionId,
    pub accepted_obligations: BTreeSet<DownstreamObligation>,
    pub accepted_by: PrincipalId,
    pub state_revision: u64,
    pub mount_generation: u64,
    pub last_source_check_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Mount persistence stores namespaced source identifiers without source-table foreign keys.

### Two-sided handshake

```rust
pub struct GrantInvitationSnapshot {
    pub invitation_id: MemoryShareInvitationId,
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub grant_id: MemoryShareGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub grant_revision_hash: [u8; 32],
    pub source_activation_epoch: u64,
    pub purpose_summary: BoundedText,
    pub content_mode: MemoryShareContentMode,
    pub operation_permissions: BTreeSet<MemoryShareOperation>,
    pub obligations: BTreeSet<DownstreamObligation>,
    pub offer_expires_at: Timestamp,
    pub envelope_hash: [u8; 32],
}

pub struct TargetMountAcceptance {
    pub handshake_id: MemoryShareHandshakeId,
    pub invitation_id: MemoryShareInvitationId,
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub grant_id: MemoryShareGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub source_activation_epoch: u64,
    pub mount_id: MemoryMountId,
    pub grant_revision_hash: [u8; 32],
    pub target_policy_revision_id: PolicyBundleRevisionId,
    pub accepted_obligations_hash: [u8; 32],
    pub accepted_by: PrincipalId,
    pub accepted_at: Timestamp,
    pub canonical_hash: [u8; 32],
}

pub struct SourceGrantActivationAck {
    pub handshake_id: MemoryShareHandshakeId,
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub grant_id: MemoryShareGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub grant_revision_hash: [u8; 32],
    pub target_acceptance_hash: [u8; 32],
    pub activation_epoch: u64,
    pub activated_at: Timestamp,
    pub canonical_hash: [u8; 32],
}
```

Envelopes are not capabilities. Source and target independently authorize every transition. A mount becomes `Active` only after applying an acknowledgement that binds the exact target acceptance hash.

### Shared reference

```rust
#[derive(Clone, Debug, Eq, Hash, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SharedMemoryRef {
    pub source_workspace_id: WorkspaceId,
    pub memory_id: MemoryId,
    pub memory_revision_id: MemoryRevisionId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub source_generation: u64,
}
```

Rules:

- no conversion to local `MemoryId`;
- checked at every content access;
- may remain as content-free tombstone provenance;
- never used as a source-row foreign key;
- mount-of-mount constructors reject already-mounted sources.

### Mounted view

```rust
pub struct MountedMemoryView {
    pub shared_ref: SharedMemoryRef,
    pub origin: MemoryResultOrigin,
    pub source_kind: MemoryKind,
    pub source_status: MemoryStatus,
    pub source_confidence: Confidence,
    pub source_importance: Importance,
    pub source_validity: MemoryValidity,
    pub source_provenance_summary: BoundedProvenanceSummary,
    pub source_classification: DataClassification,
    pub disclosed_content: DisclosedMemoryContent,
    pub disclosure_mode: MemoryShareContentMode,
    pub source_policy_revision_id: PolicyBundleRevisionId,
    pub target_policy_revision_id: PolicyBundleRevisionId,
    pub source_disclosure_decision_id: PolicyDecisionId,
    pub target_use_decision_id: PolicyDecisionId,
    pub retrieved_at: Timestamp,
    pub use_constraints: EffectiveMemoryUseConstraints,
}
```

This is transient/read-only and cannot enter Memory write repositories.

### Import proposal

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryImportOperation {
    DeriveLocalMemory,
    ImportExactContent,
    CreateSummary,
    CreateProcedureVariant,
    CreateConflictRecord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryImportProposalState {
    Candidate,
    Approved,
    Rejected,
    Expired,
    Committed,
    Cancelled,
    OutcomeUnknown,
}

pub struct MemoryShareImportProposal {
    pub id: MemoryShareImportProposalId,
    pub target_workspace_id: WorkspaceId,
    pub mount_id: MemoryMountId,
    pub source_ref: SharedMemoryRef,
    pub source_grant_revision_id: MemoryShareGrantRevisionId,
    pub operation: MemoryImportOperation,
    pub proposed_kind: MemoryKind,
    pub proposed_content_ref: H6ByteRepresentationRevisionRef,
    pub proposed_classification: DataClassification,
    pub provenance_plan: SharedMemoryProvenancePlan,
    pub accepted_obligations: BTreeSet<DownstreamObligation>,
    pub source_authorization_decision_id: PolicyDecisionId,
    pub target_policy_decision_id: PolicyDecisionId,
    pub state: MemoryImportProposalState,
    pub state_revision: u64,
    pub idempotency_key: String,
    pub created_by: PrincipalId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Proposal content uses target H6 encryption when persistence is required. Commit calls ordinary Memory Core services.

### Pinned offline snapshot

```rust
pub struct PinnedOfflineMemorySnapshot {
    pub id: PinnedOfflineSnapshotId,
    pub target_workspace_id: WorkspaceId,
    pub mount_id: MemoryMountId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub exact_revisions: Vec<SharedMemoryRef>,
    pub manifest_hash: [u8; 32],
    pub target_representation_ref: H6ByteRepresentationRevisionRef,
    pub source_disclosure_decision_id: PolicyDecisionId,
    pub target_retention_decision_id: PolicyDecisionId,
    pub valid_until: Timestamp,
    pub revocation_check_policy: RevocationCheckPolicy,
    pub state: OfflineSnapshotState,
    pub created_at: Timestamp,
}
```

Offline snapshots are disabled by default, exact-revision-only, target-encrypted, non-authoritative and non-transitive.

### Downstream statuses

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownstreamObligationStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Blocked,
    OutcomeUnknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceErasureCascadeStatus {
    NotRequired,
    SourceErasedDownstreamPending,
    Completed,
    Blocked,
    OutcomeUnknown,
}
```

`SourceErasedDownstreamPending` is not reported as completed erasure.

---

## Application Ports

### Source grant repository

```rust
#[async_trait::async_trait]
pub trait MemoryShareGrantRepository: Send {
    async fn insert_grant(&mut self, grant: &MemoryShareGrant) -> Result<(), ApplicationError>;
    async fn insert_revision(&mut self, revision: &MemoryShareGrantRevision) -> Result<(), ApplicationError>;
    async fn load_grant_for_update(&mut self, grant_id: MemoryShareGrantId) -> Result<MemoryShareGrant, ApplicationError>;
    async fn load_revision(&mut self, revision_id: MemoryShareGrantRevisionId) -> Result<MemoryShareGrantRevision, ApplicationError>;
    async fn save_grant(&mut self, grant: &MemoryShareGrant, expected_state_revision: u64) -> Result<(), ApplicationError>;
}
```

### Target mount repository

```rust
#[async_trait::async_trait]
pub trait MemoryMountRepository: Send {
    async fn insert_pending_mount(&mut self, mount: &MemoryMount) -> Result<(), ApplicationError>;
    async fn load_mount_for_update(&mut self, mount_id: MemoryMountId) -> Result<MemoryMount, ApplicationError>;
    async fn save_mount(&mut self, mount: &MemoryMount, expected_state_revision: u64) -> Result<(), ApplicationError>;
    async fn list_active_mounts(
        &mut self,
        target_workspace_id: WorkspaceId,
        requested_mount_ids: &[MemoryMountId],
    ) -> Result<Vec<MemoryMount>, ApplicationError>;
}
```

### Source disclosure boundary

```rust
#[async_trait::async_trait]
pub trait SourceMemoryDisclosurePort: Send + Sync {
    async fn inspect_invitation(&self, request: InspectGrantInvitation)
        -> Result<GrantInvitationInspection, MemoryShareError>;
    async fn confirm_acceptance(&self, acceptance: TargetMountAcceptance)
        -> Result<SourceGrantActivationAck, MemoryShareError>;
    async fn disclose(&self, request: SourceDisclosureRequest)
        -> Result<SourceDisclosureBatch, MemoryShareError>;
    async fn check_lifecycle(&self, request: SourceLifecycleCheck)
        -> Result<SourceLifecycleSnapshot, MemoryShareError>;
}
```

The infrastructure adapter opens a fresh source transaction, sets source workspace context, relies on forced RLS, performs source H2/H6 checks and returns bounded safe DTOs only.

### Disclosure request

```rust
pub struct SourceDisclosureRequest {
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub grant_id: MemoryShareGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub expected_revision_hash: [u8; 32],
    pub expected_activation_epoch: u64,
    pub operation: MemoryShareOperation,
    pub query: SharedQueryEnvelope,
    pub requested_content_mode: MemoryShareContentMode,
    pub result_limits: MemoryShareLimits,
    pub target_policy_revision_id: PolicyBundleRevisionId,
    pub target_decision_id: PolicyDecisionId,
    pub target_principal_id: PrincipalId,
    pub target_run_id: Option<AgentRunId>,
    pub correlation_id: CorrelationId,
}
```

Source ignores attempts to widen selection, permission, content mode or limits.

### Policy ports

```rust
#[async_trait::async_trait]
pub trait SourceMemorySharePolicyPort: Send + Sync {
    async fn authorize_grant_revision(&self, request: SourceGrantPolicyRequest)
        -> Result<SourceGrantPolicyDecision, PolicyError>;
    async fn authorize_disclosure(&self, request: SourceDisclosurePolicyRequest)
        -> Result<SourceDisclosurePolicyDecision, PolicyError>;
}

#[async_trait::async_trait]
pub trait TargetMemoryMountPolicyPort: Send + Sync {
    async fn authorize_mount_acceptance(&self, request: TargetMountPolicyRequest)
        -> Result<TargetMountPolicyDecision, PolicyError>;
    async fn authorize_use(&self, request: TargetUsePolicyRequest)
        -> Result<TargetUsePolicyDecision, PolicyError>;
}
```

### Context and invalidation ports

```rust
#[async_trait::async_trait]
pub trait MountedContextPort: Send + Sync {
    async fn add_mounted_entries(&self, request: MountedContextAssemblyRequest)
        -> Result<Vec<MountedContextEntry>, ContextError>;
    async fn invalidate_mount_generation(&self, request: MountContextInvalidation)
        -> Result<InvalidationDisposition, ContextError>;
}

#[async_trait::async_trait]
pub trait RunContextDependencyInvalidationPort: Send + Sync {
    async fn invalidate_dependency(&self, request: RunContextDependencyInvalidation)
        -> Result<RunInvalidationDisposition, ApplicationError>;
}
```

The H1 adapter uses existing Run commands/events and never updates Run tables directly.

### Target representation port

```rust
#[async_trait::async_trait]
pub trait TargetSharedRepresentationPort: Send + Sync {
    async fn persist_encrypted_representation(&self, request: PersistTargetSharedRepresentation)
        -> Result<TargetSharedRepresentation, ApplicationError>;
    async fn quarantine_representation(&self, request: QuarantineTargetSharedRepresentation)
        -> Result<QuarantineDisposition, ApplicationError>;
    async fn purge_representation(&self, request: PurgeTargetSharedRepresentation)
        -> Result<PurgeDisposition, ApplicationError>;
}
```

The adapter allocates fresh target DEKs through the approved envelope-encryption boundary.

---

## Two-Sided Activation Handshake

The handshake avoids cross-workspace database transactions and cross-workspace foreign keys.

### Source submission transaction

Source atomically writes:

1. immutable grant revision;
2. grant transition to `PendingAcceptance`;
3. monotonic activation epoch;
4. durable event;
5. existing outbox invitation intent;
6. idempotency result.

### Target acceptance transaction

Target:

1. inspects exact current invitation through source port;
2. checks target capability/policy;
3. validates exact revision hash, epoch, offer expiry and obligations;
4. creates mount in `PendingActivation`;
5. writes target acceptance and its canonical hash;
6. writes event/existing outbox intent;
7. stores idempotency result.

### Source confirmation transaction

Source reconciler:

1. loads grant/revision under source RLS;
2. verifies source/target IDs, revision hash, epoch, acceptance hash, grant validity and source policy;
3. rejects changed, suspended, expired or revoked source state;
4. transitions grant to `Active` idempotently;
5. writes acknowledgement binding target acceptance hash;
6. writes event/outbox.

### Target activation transaction

Target reconciler:

1. loads pending mount under target RLS;
2. verifies acknowledgement, target acceptance hash and exact IDs;
3. rechecks target policy;
4. transitions mount to `Active`;
5. increments mount generation;
6. writes event/outbox.

No read occurs while mount is `Pending` or `PendingActivation`.

Race rules:

- source revision change before confirmation conflicts;
- source revoke wins over acceptance;
- duplicate messages return original result;
- lost acknowledgement replays idempotently;
- unknown message schema fails closed;
- possessing IDs/hashes never activates access.

---

## Federated Retrieval

```rust
pub struct SearchMemoriesWithMounts {
    pub target_workspace_id: WorkspaceId,
    pub local_query: MemorySearchQuery,
    pub mount_ids: Vec<MemoryMountId>,
    pub requested_operation: MemoryShareOperation,
    pub requested_content_mode: MemoryShareContentMode,
    pub context: MemoryUseContext,
    pub idempotency_key: Option<String>,
    pub requested_by: PrincipalId,
}
```

Pipeline:

```text
target request
→ target authentication/capability
→ load exact active mounts under target RLS
→ target policy and budgets
→ local Memory retrieval
→ bounded per-mount source disclosure call
→ source transaction + source RLS
→ current grant/revision/epoch/policy validation
→ source-side least-disclosure resolution
→ bounded source response
→ target policy/classification/provider validation
→ target-side least-disclosure resolution
→ origin-aware reranking
→ combined result/content/token limits
→ content-free source/target receipts
```

Source transaction rules:

- one transaction has exactly one source workspace context;
- source repository receives compiled typed selection only;
- no source query joins target tables;
- target query is transformed according to query-disclosure mode;
- exact query text is omitted from source audit unless explicitly permitted;
- rows use a consistent snapshot;
- metadata/content come from the same revision;
- generation/hash accompany every result;
- limits are enforced before serialization.

```rust
pub struct FederatedMemoryCandidate {
    pub origin: MemoryResultOrigin,
    pub local_memory_id: Option<MemoryId>,
    pub shared_ref: Option<SharedMemoryRef>,
    pub source_confidence: Confidence,
    pub source_importance: Importance,
    pub mount_trust_ceiling: TrustClass,
    pub source_score: f32,
    pub target_interpretation_score: f32,
    pub effective_score: f32,
}
```

Ranking rules:

- mounted origin stays visible;
- foreign score cannot exceed target trust ceiling;
- foreign constraints never override H2/local policy;
- local conflict/supersession is not inferred automatically;
- deterministic ties include origin and namespaced reference;
- combined limits apply across local and mounted results.

Query privacy:

- `ExactQueryAllowed` sends bounded exact terms;
- `RedactedQueryTerms` applies target-approved redaction;
- `StructuredFilterOnly` sends typed categories only;
- `PrecomputedCollectionOnly` sends no target query;
- source audit stores hash/safe categories;
- sensitive target purpose is not disclosed beyond policy.

Budgets are durable and keyed by grant revision, mount generation, window and optional Run. Hard exhaustion denies before content read.

---

## Context Pack and Active Run Integration

```rust
pub struct MountedContextEntry {
    pub shared_ref: SharedMemoryRef,
    pub mount_id: MemoryMountId,
    pub mount_generation: u64,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub source_disclosure_decision_id: PolicyDecisionId,
    pub target_use_decision_id: PolicyDecisionId,
    pub disclosure_mode: MemoryShareContentMode,
    pub content_hash_or_tombstone: ContentHashOrTombstone,
    pub included_token_count: u32,
    pub provider_use_allowed: bool,
    pub assembled_at: Timestamp,
}
```

Inclusion requires active exact mount, current active grant/epoch, `ReadContent`, exact deterministic/model permission, source/target policy, provider locality, Run purpose/budget, effective content mode, valid approvals and disclosure receipt.

Revocation, suspension, expiry, purge, erasure, policy reduction or source generation change:

1. makes mount generation stale/revoked;
2. quarantines target persistent representations;
3. invalidates H6 Context Pack caches;
4. emits durable H1 dependency invalidation through the port;
5. causes future invocation/retry to pause, rebuild or fail closed;
6. preserves historical invocation evidence;
7. never reconstructs missing content from a hash.

Replay without lawful retained bytes is `NotReplayable` or `Inconclusive`.

---

## Target Indexing and Pinned Offline Mode

Default live mounts create no target index or persistent content representation.

Metadata index requires `DiscoverMetadata`. Content-bearing index requires `ReadContent`, `GenerateDerivedIndex`, compatible obligations, source/target provider approvals, H8 target encryption, exact mount/grant/source generations and downstream cleanup tracking.

Quarantine-first invalidation:

1. target marks generation `Quarantined`;
2. query adapters exclude it immediately;
3. cleanup removes embedding/text/metadata representation;
4. target writes receipt;
5. source downstream status reconciles;
6. ambiguity remains `OutcomeUnknown`.

Pinned offline mode:

- disabled by default;
- exact-revision or run-scoped exact selections only;
- explicit retention permission;
- target encryption with fresh DEK;
- exact hashes/snapshot time;
- never shown as current source content;
- expiry fail-closed;
- periodic revocation checks where configured;
- no re-share or import without separate permission.

---

## Local Derivation, Exact Import and Conflict Handling

Derivation proposal stores source provenance and proposed target content but writes no local Memory before governed commit. Target confidence/importance are independently assessed.

Exact import:

```text
active mount + exact permission
→ source disclosure authorization
→ exact source revision under source RLS
→ target classification/redaction
→ fresh target DEK representation
→ target proposal approval
→ ordinary Memory Core candidate creation
→ first local revision
→ SharedMemoryRef provenance
→ optional Derivation
→ target activation/write-policy review
→ source/target audit receipts
```

Source keys/wrapped DEKs/blob paths are never copied.

Idempotency scope includes target workspace, mount, shared ref, operation and canonical proposed-content hash. Ambiguous completion loads local Memory/provenance before retry; blind duplicate import is prohibited.

A local-vs-foreign conflict proposal stores namespaced `SharedMemoryRef` without source FK. Resolution may create a local derived revision or review outcome; source memory is never changed.

A later share of an imported local Memory must evaluate retained downstream obligations and preserve source provenance.

---

## Revocation, Source Change, Purge, Erasure and Obligations

Source revoke atomically writes grant `Revoked`, incremented epoch, authoritative event, existing outbox intent, obligation intents and audit. Revocation is irreversible for that grant identity.

Target receipt marks mount revoked/stale, increments generation, quarantines persistent representations, invalidates contexts/Runs, creates cleanup/review items and emits receipt.

A missing target receipt does not weaken source revoke. Live read always checks source lifecycle.

Source revision/status/classification change immediately affects eligibility. `ReviewOnSourceChange` creates target review work for tracked derivatives/imports without modifying them.

Hard purge/cryptographic erasure removes live queryability, returns content-free tombstones, increments source generation, emits invalidation and schedules target cleanup where accepted obligations require it.

```rust
pub struct DownstreamObligationJob {
    pub id: DownstreamObligationJobId,
    pub source_workspace_id: WorkspaceId,
    pub target_workspace_id: WorkspaceId,
    pub grant_id: MemoryShareGrantId,
    pub grant_revision_id: MemoryShareGrantRevisionId,
    pub obligation: DownstreamObligation,
    pub scope_hash: [u8; 32],
    pub status: DownstreamObligationStatus,
    pub source_erasure_cascade_status: SourceErasureCascadeStatus,
    pub total_items: u64,
    pub completed_items: u64,
    pub failed_items: u64,
    pub outcome_unknown_items: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Source cannot directly delete target content. Target performs governed cleanup and reports content-free receipts. `SourceErasedDownstreamPending` and `OutcomeUnknown` remain non-complete.

---

## Durable Event Contracts

Repository-owned v1 descriptors:

```text
memory_share.grant_created
memory_share.grant_revision_created
memory_share.grant_pending_acceptance
memory_share.grant_activated
memory_share.grant_suspended
memory_share.grant_revoked
memory_share.grant_expired
memory_share.mount_accepted
memory_share.mount_activated
memory_share.mount_stale
memory_share.mount_suspended
memory_share.mount_revoked
memory_share.read_disclosed
memory_share.read_denied
memory_share.context_use_recorded
memory_share.model_use_recorded
memory_share.derivation_proposed
memory_share.import_proposed
memory_share.import_committed
memory_share.cache_quarantined
memory_share.index_purge_requested
memory_share.index_purged
memory_share.downstream_purge_unknown
```

Payloads contain safe IDs, hashes, counts, modes, reason categories and policy references only. Readers/upcasters ship before producer activation.

---

## Persistence Model

### Migration `0105`

Create:

```text
memory_share_capability_settings
memory_share_grants
memory_share_grant_revisions
memory_share_collections
memory_share_collection_revisions
memory_share_collection_members
memory_share_selection_revision_members
memory_share_grant_state_history
```

Constraints:

- source and target differ;
- target non-null/exact;
- unique revision per grant;
- immutable revision rows;
- optional expiry with conditional mandatory-expiry checks for Restricted/support/model/export/import/run-scoped grants;
- bounded canonical typed contracts;
- collection members same source workspace;
- no arbitrary query fields;
- one active revision binding;
- terminal states cannot reopen.

### Migration `0106`

Create:

```text
memory_share_invitations
memory_mounts
memory_mount_acceptances
memory_share_handshakes
memory_share_activation_acks
memory_mount_state_history
```

Constraints:

- namespaced source IDs/hashes without source FKs;
- unique active mount per target/grant identity;
- revision/hash/epoch and acceptance hash immutable per generation;
- pending mounts non-queryable;
- terminal states cannot reopen;
- duplicate envelope hashes idempotent;
- offer expiry enforced.

### Migration `0107`

Create:

```text
memory_share_disclosure_receipts
memory_share_target_use_receipts
memory_share_query_budgets
memory_share_rate_windows
memory_share_source_lifecycle_checks
memory_share_run_dependencies
memory_share_query_category_audit
```

No content/raw-query columns. Receipt uniqueness binds correlation, mount generation, shared ref and operation. Counters are bounded. Run dependency rows store hashes/tombstones only.

### Migration `0108`

Create:

```text
memory_share_import_proposals
memory_share_import_source_refs
memory_share_indexes
memory_share_index_generations
memory_share_index_members
pinned_offline_memory_snapshots
memory_share_downstream_obligation_jobs
memory_share_downstream_obligation_items
memory_share_cleanup_receipts
memory_share_foreign_conflict_refs
```

Constraints:

- no source Memory FK;
- local imported Memory FK only after local commit and within target;
- target representation belongs to target workspace;
- no source key/backend columns;
- exact mount/grant/source generations;
- quarantined representations non-queryable;
- unknown cleanup cannot become succeeded;
- expected revisions remain command-only.

### Migration `0109`

Create/extend exactly:

```text
memory_share_inbox_receipts
memory_share_reconciliation_records
memory_share_worker_leases
```

And:

- reuse existing outbox tables with typed memory-sharing event/message kinds;
- force RLS on all new workspace-owned tables;
- add separate source/target policies;
- add same-workspace composite FKs where ownership is local;
- assert absence of cross-workspace Memory/grant FKs;
- add event-schema binding rows;
- add lifecycle/worker indexes and terminal protections;
- add feature-state constraints and migration-ownership comments;
- restrict application roles from multi-workspace reads.

Database tests prove no supported role can execute a raw target-to-source Memory join.

---

## H11 Surfaces

```text
POST /admin/v1/memory-share-grants
POST /admin/v1/memory-share-grants/{id}/revisions
POST /admin/v1/memory-share-grants/{id}/submit
POST /admin/v1/memory-share-grants/{id}/suspend
POST /admin/v1/memory-share-grants/{id}/revoke
GET  /admin/v1/memory-share-grants/{id}
GET  /admin/v1/memory-share-invitations
POST /admin/v1/memory-mounts/{grant-id}/accept
POST /admin/v1/memory-mounts/{id}/suspend
DELETE /admin/v1/memory-mounts/{id}
GET  /admin/v1/memory-mounts/{id}
POST /v1/memories/search-with-mounts
POST /v1/memory-import-proposals
POST /v1/memory-import-proposals/{id}/commit
```

Rules:

- mutations use idempotency;
- lifecycle mutations use expected revisions;
- acceptance shows exact source/target/hash/expiry/operations/provider/indexing/obligations;
- revoke is not a reversible toggle;
- DTOs expose safe IDs/status/counts/reasons only;
- results always label origin;
- no source key/path or unrestricted content in management surfaces;
- HTTP/CLI/SDK share application services.

CLI:

```text
vestrace memory-share grant create
vestrace memory-share grant revise
vestrace memory-share grant submit
vestrace memory-share grant show
vestrace memory-share grant revoke
vestrace memory-share invitation list
vestrace memory-share mount accept
vestrace memory-share mount show
vestrace memory-share mount suspend
vestrace memory-share search
vestrace memory-share import propose
vestrace memory-share import commit
vestrace memory-share doctor
```

MCP default is search-only through active mounts; no grant management or exact import.

---

## Failure Semantics

- **Source unavailable:** live read is `SourceUnavailableFailClosed`; no stale content/local copy.
- **Unknown/changed revision:** mount stale/conflict; no last-known fallback.
- **Source policy reduced:** immediate reduction/deny; persistent modes quarantine.
- **Target policy reduced:** immediate target deny/reduction; source grant unchanged.
- **Source revision changes during read:** one exact snapshot or conflict; no metadata/content mixing.
- **Revoke races with read:** disclosure must prove active epoch; otherwise no content.
- **Acknowledgement lost:** target remains pending; replay exact ack idempotently.
- **Cleanup partial:** representation stays non-queryable; source cascade not complete.
- **Import unknown:** reconcile idempotency/local provenance before retry.
- **H8 unavailable:** transient live read only when policy allows; persistence fails closed; no plaintext fallback.
- **Provider mismatch:** remote model/embedding denied; local deterministic use may remain.
- **Graph escape:** inaccessible node omitted/denied; edge grants no access.

---

## Implementation Sequence

### Task 1: Add failing boundary and migration-ownership tests

- [ ] Add compile-fail tests: no `SharedMemoryRef -> MemoryId`; no mounted view into write repositories.
- [ ] Add database tests that `global` remains local.
- [ ] Add static checks rejecting raw cross-workspace SQL/RLS bypass.
- [ ] Add default-disabled and no-default-index tests.
- [ ] Add `0105`–`0109` ownership assertions.
- [ ] Run and verify failures.
- [ ] Commit tests only.

### Task 2: Implement IDs, capability and safe value types

- [ ] Implement IDs/nil rejection.
- [ ] Implement source/target capability snapshots and intersection.
- [ ] Implement lifecycle/operation/content/query/index/obligation/retention/status enums.
- [ ] Implement bounded limits and canonical hashes.
- [ ] Add invalid combination property tests.
- [ ] Run and commit.

### Task 3: Implement grant, revision, collection and selection domain

- [ ] Write lifecycle/revision/selection failures.
- [ ] Implement grant transition table and epochs.
- [ ] Implement immutable revisions with conditional expiry.
- [ ] Implement explicit selections/typed filter input.
- [ ] Implement source collection revisions.
- [ ] Reject wildcard/empty/unknown/arbitrary expressions.
- [ ] Run and commit.

### Task 4: Create `0105` and source repositories

- [ ] Add source tables, RLS and constraints.
- [ ] Implement SQLx repositories.
- [ ] Add immutable/terminal/idempotency tests.
- [ ] Verify source writes require no target query/table.
- [ ] Run and commit.

### Task 5: Implement source grant services and invitation outbox

- [ ] Add create/revise/submit/suspend/revoke commands.
- [ ] Bind exact H2/H6 decisions/approvals.
- [ ] Create canonical time-bounded invitation offers.
- [ ] Write event and existing outbox atomically.
- [ ] Add races/idempotency.
- [ ] Register readers/descriptors before producers.
- [ ] Run and commit.

### Task 6: Implement mount and handshake contracts

- [ ] Write acceptance/lifecycle failures.
- [ ] Implement mount transition table.
- [ ] Implement acceptance/ack contracts binding exact hashes/epoch.
- [ ] Ensure envelopes are not capabilities.
- [ ] Add stale/revoke races.
- [ ] Run and commit.

### Task 7: Create `0106` and target handshake repositories

- [ ] Add invitation/mount/acceptance/handshake/ack tables.
- [ ] Store source IDs without source FKs.
- [ ] Add target RLS/inbox idempotency.
- [ ] Add restart tests at each boundary.
- [ ] Prove pending mounts non-queryable.
- [ ] Run and commit.

### Task 8: Implement reconciled activation handshake

- [ ] Inspect source invitation in source transaction.
- [ ] Execute target acceptance transaction.
- [ ] Execute source confirmation/epoch transition.
- [ ] Apply target activation ack bound to acceptance hash.
- [ ] Test duplicate/lost/out-of-order envelopes.
- [ ] Test changed/revoked/expired conflicts.
- [ ] Run and commit.

### Task 9: Implement RLS-safe source disclosure runtime

- [ ] Add exact source workspace executor.
- [ ] Implement typed selection compiler/parameterized predicates.
- [ ] Validate current grant/revision/epoch/policy.
- [ ] Resolve least-disclosing source mode.
- [ ] Enforce query privacy/bounds before serialization.
- [ ] Return revision-consistent DTOs.
- [ ] Prove target transaction context is never reused.
- [ ] Run and commit.

### Task 10: Create `0107` and disclosure accounting

- [ ] Add content-free receipts/budgets/windows/lifecycle checks/Run dependencies.
- [ ] Add uniqueness/idempotency and bounded counters.
- [ ] Implement quota transactions.
- [ ] Verify no content/raw-query columns.
- [ ] Run and commit.

### Task 11: Implement federated retrieval and reranking

- [ ] Load active target mounts.
- [ ] Retrieve local candidates.
- [ ] Dispatch bounded source calls with concurrency limits.
- [ ] Apply target policy/provider and least-disclosure resolution.
- [ ] Rerank with origin/trust ceilings.
- [ ] Apply combined limits.
- [ ] Test UUID guess/source unavailable/multi-mount/load.
- [ ] Run and commit.

### Task 12: Integrate Context Pack, model/tool use and Run invalidation

- [ ] Add mounted context manifests.
- [ ] Separate deterministic/model permissions.
- [ ] Enforce provider locality/NoExternalProvider.
- [ ] Keep tool commitments behind ordinary H2.
- [ ] Add H1 invalidation port.
- [ ] Invalidate cache/context on generations/lifecycle.
- [ ] Test no stale retry/historical audit.
- [ ] Run and commit.

### Task 13: Create `0108` and persistent-use state

- [ ] Add import/source-ref/index/snapshot/obligation/cleanup/conflict tables.
- [ ] Enforce target representation ownership.
- [ ] Prove no source keys/paths/FKs.
- [ ] Add quarantine/unknown-state constraints.
- [ ] Run and commit.

### Task 14: Implement target indexing and pinned snapshots

- [ ] Keep disabled by default.
- [ ] Distinguish metadata-only vs content-bearing requirements.
- [ ] Require exact permissions/obligations/H2/H6/H8.
- [ ] Use fresh target DEKs.
- [ ] Include exact generations/manifests.
- [ ] Quarantine before cleanup.
- [ ] Test expiry/revoke/provider/indexing.
- [ ] Run and commit.

### Task 15: Implement derivation, exact import and conflicts

- [ ] Create governed proposals.
- [ ] Persist target-encrypted proposal content.
- [ ] Extend provenance with `SharedMemoryRef` without source FK.
- [ ] Commit through Memory Core candidate/write policy.
- [ ] Assess local confidence independently.
- [ ] Reconcile idempotency/OutcomeUnknown.
- [ ] Handle local-vs-foreign conflicts.
- [ ] Preserve obligations for later local sharing.
- [ ] Run and commit.

### Task 16: Implement source-change review, revoke, purge/erasure and cleanup

- [ ] Implement authoritative source revoke.
- [ ] Implement `ReviewOnSourceChange` target work.
- [ ] Implement mount invalidation/quarantine.
- [ ] Implement active Run/context invalidation.
- [ ] Implement obligation jobs/receipts/cascade statuses.
- [ ] Integrate hard purge/erasure signals.
- [ ] Keep source/target cleanup authority separate.
- [ ] Test lost receipt/restart/unknown outcomes.
- [ ] Run and commit.

### Task 17: Create `0109`, complete RLS/inbox/reconciliation/events/workers

- [ ] Create exact inbox/reconciliation/worker tables.
- [ ] Reuse existing outbox with typed messages.
- [ ] Force RLS/add constraints/indexes.
- [ ] Register event schema bindings/readers.
- [ ] Add raw multi-workspace denial tests.
- [ ] Run and commit.

### Task 18: Add H11 HTTP/CLI/MCP/SDK/schema surfaces

- [ ] Add admin grant/mount review routes.
- [ ] Add shared search/import routes.
- [ ] Add CLI and safe doctor.
- [ ] Add MCP active-mount search only.
- [ ] Generate SDKs/schemas.
- [ ] Add parity/unknown-schema/redaction tests.
- [ ] Run and commit.

### Task 19: Security, load and operational gates

- [ ] Run boundary/static scripts.
- [ ] Run unit/integration/property/migration/RLS suites.
- [ ] Run crash-at-every-handshake/revoke/import tests.
- [ ] Run bounded multi-mount load tests.
- [ ] Measure revocation-to-nonqueryable latency in approved local profile.
- [ ] Verify no source keys/content/raw queries in logs/metrics/schemas.
- [ ] Verify no silent copy/index/model/export/transitive behavior.
- [ ] Use verification-before-completion.
- [ ] Commit verification/docs.

---

## Rollout Order

1. ship domain/readers/schemas disabled;
2. apply `0105`–`0109`;
3. verify RLS/role denial;
4. enable metadata invitation/inspection in test workspaces;
5. exercise handshake/restart;
6. enable live read for explicit revisions only;
7. verify source-unavailable/revoke fail-closed;
8. enable typed filters/collections;
9. enable deterministic Context Pack use;
10. enable model use after provider/locality tests;
11. enable derivation/import after encryption/provenance tests;
12. enable indexes/snapshots for explicit cohorts;
13. exercise source-change/revoke/purge/erasure cleanup;
14. publish H11 surfaces after security/parity review;
15. permanently reject wildcard/transitive/mount-of-mount behavior.

Rollback:

- capability disable blocks future operations without rewriting history;
- active mounts suspend/stale by policy;
- persistent representations quarantine before cleanup;
- no down migrations;
- local imports remain under local lifecycle/obligations;
- rollback never re-enables revoked access or erased content.

---

## Acceptance Scenarios

1. Default disabled: no grant/mount rows.
2. `global` remains local.
3. Explicit structured-only procedures read creates no local copy/index.
4. Guessed source UUID denied without existence leak.
5. Wildcard target rejected.
6. Stale revision acceptance conflicts.
7. Lost activation ack replays once.
8. Duplicate acceptance idempotent; changed input conflicts.
9. Source unavailable fails closed.
10. Structured-only query privacy sends no free text.
11. Budget exhaustion denies before content.
12. Revision change during read returns exact snapshot/conflict.
13. Source policy reduction immediately reduces/denies.
14. Target policy reduction immediately denies model use.
15. Broader target policy does not widen source grant.
16. `NoExternalProvider` blocks remote model but may allow local display.
17. Prompt injection remains untrusted data.
18. Foreign conflict proposal does not supersede local Memory.
19. Search/context creates no local Memory/index.
20. Embedding without permission creates no job/representation.
21. Explicit embedding uses fresh target DEK/no source key.
22. Revoke quarantines index before cleanup.
23. Pinned offline denied by default.
24. Valid pinned snapshot is exact/time-bound/non-current.
25. Mount-of-mount denied.
26. Transitive sharing denied.
27. Graph traversal cannot escape selection.
28. Deterministic context records exact decisions/generations/origin.
29. Model context without permission denied.
30. Tool commitment still requires ordinary H2 approval.
31. Revoke during active Run invalidates future invocation only.
32. Revoke/read race returns no content.
33. Source hard purge removes live result immediately.
34. Source erasure with downstream cleanup is pending until receipts.
35. Lost cleanup response remains `OutcomeUnknown`.
36. Source revision change creates review work without rewriting derivative.
37. Exact import creates fresh target-encrypted local Candidate with provenance.
38. Foreign confidence is independently assessed.
39. Duplicate import retry creates no duplicate.
40. Ambiguous import remains `OutcomeUnknown`.
41. Target/public/log scan contains no source key/path.
42. Application role cannot raw-join target mounts to source Memory.
43. Run export without permission omits/reference/tombstone only.
44. Run export with permission transfers provenance, not authority.
45. Bundle import creates no mount or Memory.
46. Restricted content expiry denies despite cache.
47. Secret-like content denied/quarantined without raw leak.
48. Multi-mount load remains bounded despite slow source.
49. Source and target RLS isolation both pass.
50. Unknown event/schema blocks semantic handling.
51. Source/target audit correlation stores no content/raw query.
52. Disabling feature after activation blocks future use and quarantines persistence.
53. Metadata-only indexing works without content read.
54. Content-bearing indexing fails without `ReadContent`/`GenerateDerivedIndex`.
55. Non-expiring low-risk metadata grant is allowed when policy permits.
56. Model/export/import/run-scoped grant without expiry is rejected.
57. Acceptance ack mismatch cannot activate mount.
58. Imported local Memory re-share is blocked when obligations prohibit it.
59. `SourceErasedDownstreamPending` is never displayed as completed.
60. Existing outbox is reused; no parallel outbox/event runtime exists.

---

## Definition of Done

1. `global` remains workspace-local.
2. Every grant binds one exact source/target.
3. Target acceptance and source ack are both required and hash-bound.
4. Grant revisions immutable; mutable transitions optimistic.
5. No cross-workspace Memory/grant FK in target persistence.
6. No raw cross-workspace SQL/RLS bypass.
7. Source reads execute under source RLS.
8. UUID/hash/reference alone grants nothing.
9. Live reads create no local Memory/index by default.
10. Indexing/offline disabled by default.
11. Source/target policies and content modes intersect fail-closed.
12. Source unavailable/stale/revoked/expired disclose no content.
13. Metadata/content revision-consistent.
14. Mounted origin/trust/provenance explicit.
15. Mounted content cannot supersede/elevate authority.
16. Mount-of-mount/transitive sharing rejected.
17. Model/import/derivation/index/export are separate permissions.
18. Persistent target representations use fresh target keys.
19. No source key/backend material enters target/public/export.
20. Source change/revoke/purge invalidates dependent system state.
21. Ambiguous cleanup remains `OutcomeUnknown`.
22. Source erasure cascade status is truthful.
23. Import uses Memory Core lifecycle/provenance/write policy.
24. Run export/import never recreates grant/mount authority.
25. Event readers ship before producers.
26. Existing outbox is reused.
27. `0105`–`0109` are forward-only/uniquely owned.
28. RLS/restart/idempotency/security/load and 60 scenarios pass.
29. Implementation starts only after explicit authorization ending documentation-only phase.

---

## Documentation-Only Boundary

Merging this plan authorizes only implementation sequence/contracts. It does not authorize:

- creating `feat/cross-workspace-memory-sharing`;
- changing Rust/dependencies;
- applying `0105`–`0109`;
- enabling sharing capability;
- creating real grants/invitations/mounts;
- opening source workspace reads;
- building indexes/offline snapshots;
- persisting imported/derived shared content;
- sending mounted content to models;
- publishing admin/MCP/SDK surfaces;
- propagating revoke/purge against real data.

A separate explicit instruction is required before implementation.
