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

**Architecture:** Vestrace keeps every authoritative `Memory`, `MemoryRevision`, source, derivation, conflict and lifecycle transition inside one workspace. A source workspace publishes a bounded `MemoryShareGrantRevision`; a target workspace separately accepts that exact revision into a `MemoryMount`. Activation uses a reconciled two-sided handshake rather than a cross-workspace SQL transaction. Live retrieval opens a separate source-workspace transaction under source RLS for each mount, returns only policy-bounded DTOs, and then applies target policy and reranking. Persistent target derivatives use ordinary Memory Core writes, complete `SharedMemoryRef` provenance and fresh target encryption.

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
- Read-only is the default.
- `DiscoverMetadata` does not imply `ReadContent`.
- `ReadContent` does not imply deterministic context, model context, derivation, import or export.
- Deterministic context use and model context use are separate permissions.
- Model/provider use is denied unless explicitly permitted by source grant and target policy.
- Export is denied unless explicitly permitted by source grant and target policy.
- Local derivation and exact import are separate governed operations.
- Search, display, Context Pack inclusion, conflict detection and model invocation never create a local Memory automatically.
- Target-side FTS, pgvector, text or embedding indexes are disabled by default.
- Persistent target cache/index/snapshot modes require explicit source permission, target acceptance, target encryption and cleanup obligations.
- Mounted content is foreign/untrusted data and never enters system/developer authority channels.
- Mounted content cannot grant capabilities, approvals, roles, budgets or execution authority.
- Mounted content cannot supersede local memory automatically.
- Source confidence and importance remain source facts and are never copied into local trust scores without a target assessment.
- Mount-of-mount is prohibited.
- A target cannot re-share a mounted reference to a third workspace.
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

## Compatibility with Memory Core and owning Horizons

This extension leaves the v0.1 Memory Core invariants intact.

Implementation rules:

- `memories`, `memory_revisions`, `memory_sources`, `derivations`, `memory_conflicts`, scopes and local relations remain authoritative;
- existing cross-workspace-reference rejection remains active for ordinary Memory Core relations and foreign keys;
- `global` scope storage and query behavior are unchanged;
- source-side grant selection adapters load ordinary Memory Core revisions under source RLS;
- target-side mounted views never write to `memories`;
- persistent local import calls the existing Memory Core `RememberMemory`/candidate lifecycle rather than inserting directly;
- `SourceRef` gains a typed namespaced shared-memory provenance variant only where Memory Core source contracts support it;
- the shared provenance variant stores identifiers/hashes but no cross-workspace foreign key;
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
    share_collection.rs
    mount.rs
    mounted_view.rs
    import_proposal.rs
    offline_snapshot.rs
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

Add strongly typed IDs:

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

pub struct MemorySharingCapabilitySnapshot {
    pub deployment_state: MemorySharingCapabilityState,
    pub workspace_state: MemorySharingCapabilityState,
    pub policy_bundle_revision_id: PolicyBundleRevisionId,
}
```

Effective capability is the strict intersection of deployment state, source workspace state, target workspace state and current H2 policy.

`Disabled` is the default for both source and target workspaces.

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
- `activation_epoch` is monotonic and changes on activation, suspension, revision replacement and revocation;
- `Revoked`, `Expired` and `Deleted` are terminal;
- `Active` requires an accepted exact revision and completed handshake;
- lifecycle transitions require expected state revision in commands;
- grant identity contains no content or secret material.

### Grant lifecycle commands

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

Commands do not accept target changes for an existing grant. A different target requires a new grant identity.

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
    pub valid_until: Timestamp,
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
- `valid_until` is later than `valid_from`;
- restricted, support/evaluation, model-use, export and exact-import revisions always expire;
- permissions are internally consistent;
- `UseInModelContext` requires `ReadContent`;
- derivation/import/export permissions do not imply one another;
- target indexing permission requires `ReadContent` and a persistent-use capability ceiling;
- `NoRetention` rejects all persistent target modes;
- canonical hash includes every semantic field in stable order;
- any semantic change creates a new revision.

### Selection contract

```rust
#[derive(Clone, Debug, Eq, PartialEq,
         serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum MemoryShareSelection {
    ExplicitRevisionSet {
        revisions: Vec<ExactSharedRevisionSelector>,
    },
    ExplicitMemorySetCurrentRevision {
        memory_ids: Vec<MemoryId>,
    },
    NamedSourceCollection {
        collection_revision_id: MemoryShareCollectionRevisionId,
    },
    BoundedTypedFilter {
        filter: BoundedMemoryShareFilter,
    },
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

- all explicit sets are nonempty and bounded;
- explicit revisions must belong to source workspace and pass source policy at revision creation;
- current-revision sets revalidate kind, status, classification and policy on every read;
- run-scoped selections bind exact Run and expiry;
- source collection revisions are immutable;
- filters compile only from approved typed predicates;
- future unknown Memory kinds are not automatically included.

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

The compiler produces parameterized repository predicates. It rejects raw SQL, JSONPath, regex, executable expressions and target-controlled source widening.

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

Collection revisions are source-owned and immutable. Membership changes create a new revision.

### Operation permissions

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
```

Permissions are checked per operation, principal, Run, provider and source revision.

### Content, query and indexing modes

```rust
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

`Disabled` is the default. Persistent modes require explicit permissions, compatible obligations and H8 target encryption availability.

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

All values are positive, deployment-bounded and intersected with stricter target limits.

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

Mount persistence stores source identifiers as namespaced values without a SQL foreign key to source tables.

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
    pub valid_until: Timestamp,
    pub envelope_hash: [u8; 32],
}

pub struct TargetMountAcceptance {
    pub handshake_id: MemoryShareHandshakeId,
    pub invitation_id: MemoryShareInvitationId,
    pub target_workspace_id: WorkspaceId,
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
    pub activation_epoch: u64,
    pub activated_at: Timestamp,
    pub canonical_hash: [u8; 32],
}
```

Handshake envelopes are not capabilities. Source and target independently authorize every transition and re-read current state. A target mount becomes `Active` only after the exact activation acknowledgement is durably applied.

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

- no `From<SharedMemoryRef> for MemoryId` implementation;
- reference is checked at every content access;
- a tombstoned reference may remain in provenance/audit;
- target repositories never use it as a foreign key to source memory;
- mount-of-mount constructors reject any source that is already mounted.

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

This is transient/read-only and cannot be passed to Memory Core write repositories.

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

Proposal content is stored through H6 target-side encrypted representation when persistence is required. Commit calls ordinary Memory Core services.

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

Offline snapshots are disabled by default, exact-revision-only, encrypted under target keys, non-authoritative and non-transitive.

---

## Application Ports

### Source grant repository

```rust
#[async_trait::async_trait]
pub trait MemoryShareGrantRepository: Send {
    async fn insert_grant(&mut self, grant: &MemoryShareGrant) -> Result<(), ApplicationError>;
    async fn insert_revision(&mut self, revision: &MemoryShareGrantRevision) -> Result<(), ApplicationError>;
    async fn load_grant_for_update(
        &mut self,
        grant_id: MemoryShareGrantId,
    ) -> Result<MemoryShareGrant, ApplicationError>;
    async fn load_revision(
        &mut self,
        revision_id: MemoryShareGrantRevisionId,
    ) -> Result<MemoryShareGrantRevision, ApplicationError>;
    async fn save_grant(
        &mut self,
        grant: &MemoryShareGrant,
        expected_state_revision: u64,
    ) -> Result<(), ApplicationError>;
}
```

### Target mount repository

```rust
#[async_trait::async_trait]
pub trait MemoryMountRepository: Send {
    async fn insert_pending_mount(&mut self, mount: &MemoryMount) -> Result<(), ApplicationError>;
    async fn load_mount_for_update(&mut self, mount_id: MemoryMountId) -> Result<MemoryMount, ApplicationError>;
    async fn save_mount(
        &mut self,
        mount: &MemoryMount,
        expected_state_revision: u64,
    ) -> Result<(), ApplicationError>;
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
    async fn inspect_invitation(
        &self,
        request: InspectGrantInvitation,
    ) -> Result<GrantInvitationInspection, MemoryShareError>;

    async fn confirm_acceptance(
        &self,
        acceptance: TargetMountAcceptance,
    ) -> Result<SourceGrantActivationAck, MemoryShareError>;

    async fn disclose(
        &self,
        request: SourceDisclosureRequest,
    ) -> Result<SourceDisclosureBatch, MemoryShareError>;

    async fn check_lifecycle(
        &self,
        request: SourceLifecycleCheck,
    ) -> Result<SourceLifecycleSnapshot, MemoryShareError>;
}
```

The infrastructure implementation opens a fresh transaction, sets source workspace context, relies on forced RLS, performs source H2/H6 checks and returns bounded safe DTOs only.

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

Source ignores any target request to widen selection, mode or limits.

### Policy ports

```rust
#[async_trait::async_trait]
pub trait SourceMemorySharePolicyPort: Send + Sync {
    async fn authorize_grant_revision(
        &self,
        request: SourceGrantPolicyRequest,
    ) -> Result<SourceGrantPolicyDecision, PolicyError>;

    async fn authorize_disclosure(
        &self,
        request: SourceDisclosurePolicyRequest,
    ) -> Result<SourceDisclosurePolicyDecision, PolicyError>;
}

#[async_trait::async_trait]
pub trait TargetMemoryMountPolicyPort: Send + Sync {
    async fn authorize_mount_acceptance(
        &self,
        request: TargetMountPolicyRequest,
    ) -> Result<TargetMountPolicyDecision, PolicyError>;

    async fn authorize_use(
        &self,
        request: TargetUsePolicyRequest,
    ) -> Result<TargetUsePolicyDecision, PolicyError>;
}
```

### Context and invalidation ports

```rust
#[async_trait::async_trait]
pub trait MountedContextPort: Send + Sync {
    async fn add_mounted_entries(
        &self,
        request: MountedContextAssemblyRequest,
    ) -> Result<Vec<MountedContextEntry>, ContextError>;

    async fn invalidate_mount_generation(
        &self,
        request: MountContextInvalidation,
    ) -> Result<InvalidationDisposition, ContextError>;
}

#[async_trait::async_trait]
pub trait RunContextDependencyInvalidationPort: Send + Sync {
    async fn invalidate_dependency(
        &self,
        request: RunContextDependencyInvalidation,
    ) -> Result<RunInvalidationDisposition, ApplicationError>;
}
```

The H1 adapter uses existing Run commands/events and never updates Run tables directly.

### Target persistence and encryption ports

```rust
#[async_trait::async_trait]
pub trait TargetSharedRepresentationPort: Send + Sync {
    async fn persist_encrypted_representation(
        &self,
        request: PersistTargetSharedRepresentation,
    ) -> Result<TargetSharedRepresentation, ApplicationError>;

    async fn quarantine_representation(
        &self,
        request: QuarantineTargetSharedRepresentation,
    ) -> Result<QuarantineDisposition, ApplicationError>;

    async fn purge_representation(
        &self,
        request: PurgeTargetSharedRepresentation,
    ) -> Result<PurgeDisposition, ApplicationError>;
}
```

The adapter allocates fresh target DEKs through the approved envelope-encryption boundary.

---

## Two-Sided Activation Handshake

The handshake avoids cross-workspace database transactions and cross-workspace foreign keys.

### Source submission transaction

Source transaction atomically writes:

1. immutable grant revision;
2. grant transition to `PendingAcceptance`;
3. monotonic activation epoch;
4. durable event;
5. outbox invitation intent;
6. idempotency result.

### Target acceptance transaction

Target transaction:

1. inspects exact current invitation through source disclosure port;
2. checks target capability and policy;
3. validates exact revision hash, expiry and obligations;
4. creates `MemoryMount` in `PendingActivation`;
5. writes target acceptance record;
6. writes event/outbox;
7. stores idempotency result.

### Source confirmation transaction

Source reconciler:

1. loads grant/revision under source RLS;
2. verifies target, revision hash, epoch, expiry and source policy;
3. rejects if source changed, suspended or revoked;
4. transitions grant to `Active` idempotently;
5. writes activation acknowledgement and event/outbox.

### Target activation transaction

Target reconciler:

1. loads pending mount under target RLS;
2. verifies acknowledgement hash and exact IDs;
3. rechecks target policy;
4. transitions mount to `Active`;
5. increments mount generation;
6. writes event/outbox.

No read occurs while mount is `Pending` or `PendingActivation`.

### Race handling

- source revision change before source confirmation returns conflict;
- source revoke always wins over acceptance;
- duplicate messages return original durable result;
- lost acknowledgement is safely replayed;
- unknown message schema fails closed;
- activation does not rely on target possessing an ID or hash.

---

## Federated Retrieval

### Target request

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

### Pipeline

```text
target request
→ target authentication/capability
→ load exact active mounts under target RLS
→ target policy and budgets
→ local Memory retrieval
→ per-mount source disclosure call
→ source transaction + source RLS
→ current grant/revision/epoch/policy validation
→ bounded source candidate response
→ target policy/classification/provider validation
→ origin-aware reranking
→ final result/content/token limits
→ durable content-free disclosure/use receipts
```

### Source transaction rules

- one transaction has exactly one source workspace context;
- source repository receives compiled typed selection only;
- no source query includes target table joins;
- target query is transformed according to query-disclosure mode before transmission;
- exact query text is omitted from source audit unless policy explicitly permits it;
- rows are read at a consistent snapshot;
- metadata and content are returned from the same revision;
- source generation and revision hash are included in every result;
- result limit enforcement happens before response serialization.

### Combined ranking

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

Rules:

- mounted origin remains visible;
- foreign score cannot exceed target trust ceiling;
- foreign constraints never override H2 or local policy;
- local conflict/supersession is not automatically inferred;
- deterministic tie-breaking includes origin and namespaced reference;
- final limits apply across local and mounted results.

### Query privacy

- `ExactQueryAllowed` sends bounded exact terms;
- `RedactedQueryTerms` applies target-approved redaction before source call;
- `StructuredFilterOnly` sends typed categories and no free text;
- `PrecomputedCollectionOnly` sends no target query;
- source audit stores query hash and safe categories;
- sensitive target purpose is not disclosed beyond policy.

### Quotas and budgets

Source and target maintain durable bounded counters keyed by grant revision, mount generation, time window and optional Run.

A disclosure is denied before content is read when a hard budget is exhausted.

---

## Context Pack and Active Run Integration

### Mounted context entry

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

### Assembly rules

Context inclusion requires:

- active exact mount;
- current active grant revision and activation epoch;
- `ReadContent`;
- deterministic or model-context permission as appropriate;
- source/target current policy;
- provider/model locality compatibility;
- Run purpose/capability/budget;
- current content mode;
- unexpired approvals and grant;
- successful source disclosure receipt.

### Revocation after assembly

Revocation, suspension, expiry, purge, erasure, policy reduction or source generation change:

1. marks mount generation stale/revoked;
2. quarantines target caches/indexes immediately;
3. invalidates H6 Context Pack cache entries;
4. emits a durable context dependency invalidation through the H1 port;
5. causes future invocation/retry to pause, rebuild or fail closed under Run policy;
6. preserves historical invocation evidence;
7. never reconstructs missing content from a hash.

Replay without retained lawful bytes reports `NotReplayable` or `Inconclusive`.

---

## Target Indexing and Pinned Offline Mode

### Default

No target-side index or persistent content representation is created for live mounts.

### Explicit index creation

An index job requires:

- `GenerateDerivedIndex` permission;
- compatible target indexing policy;
- no `NoRetention`/`NoModelUse` conflict;
- source and target provider-use approvals;
- H8 target encryption availability;
- exact mount and grant generation;
- downstream purge obligations;
- bounded exact source revision manifest.

Index rows include source workspace, grant revision, mount ID/generation, exact shared ref, origin and expiry.

### Quarantine-first invalidation

On revoke/purge/expiry:

1. index generation state becomes `Quarantined` in target transaction;
2. query adapters exclude it immediately;
3. cleanup job deletes embeddings/text/metadata representations;
4. target writes cleanup receipt;
5. source downstream status is reconciled;
6. ambiguous cleanup remains `OutcomeUnknown`.

### Pinned offline snapshot

Pinned offline mode:

- is disabled by default;
- supports only `ExplicitRevisionSet` or run-scoped exact revisions;
- requires explicit retention permission;
- records exact source hashes and snapshot timestamp;
- encrypts bytes under fresh target DEK;
- cannot be presented as current source content;
- expires fail-closed;
- respects periodic revocation checks where configured;
- cannot be re-shared or imported without separate permission.

---

## Local Derivation, Exact Import and Conflict Handling

### Derivation proposal

A derivation proposal stores source provenance and target proposed content but does not write local Memory until approved/committed.

Target confidence and importance are independently assessed.

### Exact import flow

```text
active mount and exact permission
→ source disclosure authorization
→ exact revision read under source RLS
→ target classification/redaction
→ fresh target DEK encrypted representation
→ target import proposal approval
→ ordinary Memory Core candidate creation
→ first local revision
→ SharedMemoryRef source provenance
→ optional Derivation record
→ target activation/write-policy review
→ source and target audit receipts
```

Source key references, wrapped DEKs and blob paths are never copied.

### Import idempotency

The idempotency scope includes target workspace, mount, shared ref, operation and canonical proposed content hash.

An ambiguous completion loads target Memory/source provenance before any retry. Blind duplicate import is prohibited.

### Conflict handling

A target conflict proposal may compare local revision with `SharedMemoryRef`.

Persistence stores the foreign reference as namespaced fields without a foreign key.

Resolving the conflict can create a local derived revision or human-review outcome; source memory is never changed.

---

## Revocation, Purge, Erasure and Downstream Obligations

### Source revocation transaction

Source transaction atomically writes:

- grant state `Revoked`;
- incremented activation epoch;
- authoritative event;
- target revocation outbox intent;
- downstream obligation intents;
- audit fact.

Revocation is irreversible for the grant identity.

### Target receipt

Target transaction:

- marks mount `Revoked` or `Stale`;
- increments mount generation;
- quarantines persistent representations;
- invalidates contexts/Runs;
- creates cleanup/obligation items;
- emits target receipt.

A missing target receipt does not make source revoke ineffective. Live reads always perform source lifecycle checks.

### Source purge and erasure

Source Memory status/classification/generation changes update eligibility immediately.

Hard purge or cryptographic erasure:

- removes live queryability;
- returns content-free tombstones;
- increments source generation;
- emits invalidation messages;
- schedules target cleanup where obligations require it;
- never reconstructs content from hashes.

### Downstream obligation job

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
    pub total_items: u64,
    pub completed_items: u64,
    pub failed_items: u64,
    pub outcome_unknown_items: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

Source cannot directly delete target content. Target performs governed cleanup and reports content-free receipts.

`OutcomeUnknown` remains nonterminal until reconciled.

---

## Durable Event Contracts

Repository-owned descriptors are added for version 1 of:

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

Event payloads contain safe IDs, hashes, counts, modes, reason categories and policy references only. They never contain unrestricted Memory content, queries, secrets or target plaintext.

Readers/upcasters ship before producer activation according to the event compatibility plan.

---

## Persistence Model

### Migration `0105`

Create source-owned tables:

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

- source and target workspace differ;
- target workspace is non-null and exact;
- unique grant revision number per grant;
- immutable revision rows;
- exact positive expiry interval;
- bounded canonical JSON for typed selection/permissions/limits;
- collection members belong to source workspace;
- no target-controlled arbitrary query fields;
- one active revision binding per grant;
- terminal grant states cannot reopen.

### Migration `0106`

Create target-owned handshake tables:

```text
memory_share_invitations
memory_mounts
memory_mount_acceptances
memory_share_handshakes
memory_share_activation_acks
memory_mount_state_history
```

Constraints:

- target tables store namespaced source IDs/hashes without cross-workspace foreign keys;
- unique active mount per target/grant identity unless a new grant is issued;
- accepted revision/hash/epoch are immutable for a mount generation;
- pending mounts are non-queryable;
- terminal mount states cannot reopen;
- duplicate handshake envelope hashes are idempotent;
- invitation expiry is enforced.

### Migration `0107`

Create disclosure/accounting tables:

```text
memory_share_disclosure_receipts
memory_share_target_use_receipts
memory_share_query_budgets
memory_share_rate_windows
memory_share_source_lifecycle_checks
memory_share_run_dependencies
memory_share_query_category_audit
```

Constraints:

- content/query text columns do not exist;
- receipt uniqueness binds correlation, mount generation, shared ref and operation;
- counters are nonnegative and bounded;
- lifecycle checks record exact epoch/hash;
- Run dependency rows store content hashes/tombstones, not unrestricted content;
- expired budget windows are retained/purged by policy.

### Migration `0108`

Create persistent-use tables:

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

- no SQL FK from shared refs to source Memory rows;
- local imported Memory FK exists only after local commit and within target workspace;
- target representation refs belong to target workspace;
- source key/backend reference columns do not exist;
- persistent items bind exact mount/grant/source generations;
- quarantined indexes/snapshots are not queryable;
- unknown cleanup outcomes cannot be marked succeeded;
- proposal expected revisions remain command-only.

### Migration `0109`

Add:

- forced RLS to all workspace-owned tables;
- separate source and target policies;
- composite same-workspace foreign keys where ownership is local;
- explicit absence tests for cross-workspace Memory/grant foreign keys;
- outbox/inbox message tables or typed extensions to existing infrastructure;
- reconciliation records;
- worker leases and indexes;
- event-schema binding rows;
- lifecycle partial indexes;
- feature-state constraints;
- migration-ownership comments;
- application-role restrictions preventing multi-workspace reads.

Database tests prove that no supported role can execute a raw target-to-source Memory join.

---

## H11 Surfaces

### Administrative HTTP

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

- all mutations use idempotency keys;
- lifecycle mutations use expected revisions;
- grant acceptance displays exact source/target, revision hash, expiry, operations, provider use, indexing and obligations;
- revoke is not displayed as a reversible toggle;
- public DTOs expose safe IDs/status/counts/reason codes only;
- search results always include local/mounted origin;
- no source key material, storage path or unrestricted content appears in management endpoints;
- HTTP, CLI and SDK call the same application services.

### CLI

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

### MCP

Default MCP tools may search through already-active mounts when capability permits. They cannot create/accept/revise/revoke grants or exact-import content.

### Public schemas and SDKs

Schemas provide stable enums, lifecycle summaries, shared references, result origins and safe error variants. Unknown future kinds remain opaque/fail-closed according to the event/schema compatibility plan.

---

## Failure Semantics

### Source unavailable

- live read returns `SourceUnavailableFailClosed`;
- target stale cache is not returned;
- no local Memory/index is created;
- metadata-only health state may be shown without content.

### Unknown or changed grant revision

- mount becomes stale or request conflicts;
- no fallback to last-known revision;
- target must inspect and accept a new exact revision.

### Source policy reduced

- effective permission reduces immediately;
- persistent modes quarantine when no longer permitted;
- target widening never compensates for source reduction.

### Target policy reduced

- target denies/reduces immediately;
- source grant remains unchanged;
- contexts and indexes invalidated as required.

### Source revision changes during read

- repository returns one exact revision snapshot or conflict;
- metadata/content mixing across revisions is prohibited;
- safe retry reauthorizes from the start.

### Revoke races with read

- disclosure commit proves current active epoch;
- if proof cannot be committed, no content is returned;
- revoke boundary wins fail-closed.

### Handshake acknowledgement lost

- target remains `PendingActivation` and cannot read;
- reconciliation replays exact acknowledgement idempotently.

### Target cleanup partially fails

- representation remains non-queryable quarantine;
- obligation stays pending/failed/unknown;
- source does not claim successful cascade.

### Import completion unknown

- target checks idempotency, local Memory and `SharedMemoryRef` provenance;
- blind second import is prohibited;
- unresolved state is `OutcomeUnknown`.

### H8 unavailable for persistent target representation

- live read may continue only when no retention is required and policy allows transient use;
- persistent index/snapshot/import fails closed;
- no plaintext fallback is written.

### Provider locality mismatch

- deterministic/local display may remain allowed;
- remote model or embedding use is denied;
- no implicit provider downgrade/redirect occurs.

### Graph expansion outside selection

- inaccessible node is omitted/denied;
- edge does not reveal hidden content or existence beyond bounded policy response.

---

## Implementation Sequence

### Task 1: Add failing boundary, compile-fail and migration-ownership tests

**Files:**
- Create the boundary scripts and initial tests listed in the locked structure.
- Modify CI only to run deterministic local checks.

- [ ] Add compile-fail tests proving `SharedMemoryRef` cannot convert to `MemoryId` and `MountedMemoryView` cannot enter Memory write repositories.
- [ ] Add database tests proving `global` remains workspace-local.
- [ ] Add static checks rejecting raw cross-workspace SQL joins and RLS bypass helpers.
- [ ] Add tests proving default capability and target indexing are disabled.
- [ ] Add migration-number ownership assertions for `0105`–`0109`.
- [ ] Run tests and verify they fail for missing implementation.
- [ ] Commit tests only.

### Task 2: Implement domain identifiers, capability and safe value types

**Files:**
- Add IDs and memory-sharing domain modules.

- [ ] Implement typed IDs and nil rejection.
- [ ] Implement capability, lifecycle, operation, content, query, indexing and obligation enums.
- [ ] Implement bounded limits and constructor validation.
- [ ] Implement canonical hashing for safe semantic contracts.
- [ ] Add property tests for invalid permission/obligation combinations.
- [ ] Run domain tests and commit.

### Task 3: Implement grant, revision, collection and selection contracts

- [ ] Write failing lifecycle/revision/selection tests.
- [ ] Implement `MemoryShareGrant` transition table.
- [ ] Implement immutable `MemoryShareGrantRevision`.
- [ ] Implement explicit selections and bounded filter compiler input.
- [ ] Implement immutable source collection revisions.
- [ ] Reject wildcard targets, empty sets, unknown kinds and arbitrary expressions.
- [ ] Run tests and commit.

### Task 4: Create migration `0105` and source repositories

- [ ] Add source grant/revision/collection tables.
- [ ] Add forced source-workspace RLS and same-workspace member constraints.
- [ ] Add immutable-revision and terminal-state constraints.
- [ ] Implement SQLx source repositories.
- [ ] Add transaction/idempotency tests.
- [ ] Verify no target table/query is needed for source grant writes.
- [ ] Run tests and commit.

### Task 5: Implement source grant services and invitation outbox

- [ ] Add create/revise/submit/suspend/revoke commands.
- [ ] Bind exact H2/H6 decisions and approvals.
- [ ] Create canonical invitation snapshots.
- [ ] Write source event and outbox in the same transaction.
- [ ] Add race and idempotency tests.
- [ ] Register event readers/descriptors before producer activation.
- [ ] Run tests and commit.

### Task 6: Implement mount and handshake domain/application contracts

- [ ] Write failing target acceptance and lifecycle tests.
- [ ] Implement mount identity and transition table.
- [ ] Implement target acceptance and source activation acknowledgement contracts.
- [ ] Ensure envelopes are references, not capabilities.
- [ ] Add stale revision and source-revoke race tests.
- [ ] Run tests and commit.

### Task 7: Create migration `0106` and target handshake repositories

- [ ] Add invitations, mounts, acceptances, handshake and acknowledgement tables.
- [ ] Store source identifiers/hashes without source-table foreign keys.
- [ ] Add forced target RLS.
- [ ] Implement inbox/idempotency handling.
- [ ] Add restart tests for every handshake boundary.
- [ ] Prove pending mounts are non-queryable.
- [ ] Run tests and commit.

### Task 8: Implement reconciled activation handshake

- [ ] Implement invitation inspection through a source-workspace transaction.
- [ ] Implement target policy/acceptance transaction.
- [ ] Implement source confirmation and activation epoch update.
- [ ] Implement target acknowledgement application.
- [ ] Add duplicate/lost/out-of-order message tests.
- [ ] Add conflict tests for changed/revoked/expired source state.
- [ ] Run tests and commit.

### Task 9: Implement RLS-safe source disclosure runtime

- [ ] Add source workspace executor that opens one exact source transaction.
- [ ] Implement typed selection compiler and parameterized predicates.
- [ ] Implement current grant/revision/epoch/policy validation.
- [ ] Enforce query privacy and result/content bounds before serialization.
- [ ] Return revision-consistent DTOs with generation/hash.
- [ ] Add tests proving no target transaction context is reused.
- [ ] Run tests and commit.

### Task 10: Create migration `0107` and disclosure accounting

- [ ] Add content-free source disclosure and target-use receipts.
- [ ] Add query budgets/rate windows and lifecycle-check records.
- [ ] Add Run context dependency records.
- [ ] Add uniqueness/idempotency constraints.
- [ ] Implement repositories and quota transactions.
- [ ] Verify no content/raw query columns exist.
- [ ] Run tests and commit.

### Task 11: Implement federated retrieval and origin-aware ranking

- [ ] Load exact active target mounts.
- [ ] Retrieve local candidates normally.
- [ ] Dispatch bounded per-mount source calls with concurrency limits.
- [ ] Apply target policy/classification/provider checks.
- [ ] Rerank with explicit Local/Mounted origins and trust ceilings.
- [ ] Apply combined result/token/content limits.
- [ ] Add UUID-guess, source-unavailable, multi-mount and load tests.
- [ ] Run tests and commit.

### Task 12: Integrate Context Pack, model use and active Run invalidation

- [ ] Add `MountedContextEntry` manifest support.
- [ ] Separate deterministic and model-context authorization.
- [ ] Enforce provider locality and NoExternalProvider.
- [ ] Add H1 dependency invalidation application port.
- [ ] Invalidate cache/context on mount/grant generation changes.
- [ ] Add no-stale-retry and historical-audit tests.
- [ ] Run tests and commit.

### Task 13: Create migration `0108` and persistent-use state

- [ ] Add import proposal/source-ref tables.
- [ ] Add index/snapshot generation tables.
- [ ] Add downstream obligation and cleanup tables.
- [ ] Add foreign-conflict reference sidecars.
- [ ] Enforce target-workspace representation ownership.
- [ ] Prove no source key/path or cross-workspace FK columns exist.
- [ ] Run tests and commit.

### Task 14: Implement target indexing and pinned offline snapshots

- [ ] Keep feature disabled by default.
- [ ] Require exact permissions/obligations and target H2/H6/H8 decisions.
- [ ] Encrypt persistent representations with fresh target DEKs.
- [ ] Include mount/grant/source generations in keys and manifests.
- [ ] Quarantine before asynchronous cleanup.
- [ ] Add expiry/revocation/provider/index tests.
- [ ] Run tests and commit.

### Task 15: Implement derivation, exact import and conflict proposals

- [ ] Create governed import proposals.
- [ ] Persist proposed bytes through target H6 encryption.
- [ ] Extend provenance with namespaced `SharedMemoryRef`.
- [ ] Commit through ordinary Memory Core candidate/write policy.
- [ ] Assess local confidence independently.
- [ ] Add idempotency and `OutcomeUnknown` reconciliation.
- [ ] Add local-vs-foreign conflict proposal handling.
- [ ] Run tests and commit.

### Task 16: Implement revocation, purge/erasure propagation and cleanup

- [ ] Implement source authoritative revoke transaction.
- [ ] Implement target mount invalidation and quarantine.
- [ ] Implement active Run/context invalidation.
- [ ] Implement downstream obligation jobs/receipts.
- [ ] Integrate source hard purge and cryptographic erasure signals.
- [ ] Keep source and target cleanup authorities separate.
- [ ] Add lost-receipt/restart/unknown-outcome tests.
- [ ] Run tests and commit.

### Task 17: Create migration `0109`, complete RLS/outbox/events/workers

- [ ] Force RLS on every new table.
- [ ] Add source/target composite constraints and indexes.
- [ ] Add/extend outbox/inbox and reconciliation records.
- [ ] Add worker leases and terminal-state protections.
- [ ] Register durable event schema bindings/readers.
- [ ] Add database-role tests for raw multi-workspace access denial.
- [ ] Run migration/RLS/restart tests and commit.

### Task 18: Add H11 HTTP/CLI/MCP/SDK/schema surfaces

- [ ] Add admin grant/mount routes with exact review DTOs.
- [ ] Add shared search and import-proposal routes.
- [ ] Add CLI commands and safe doctor output.
- [ ] Add MCP search-only surface for active mounts.
- [ ] Generate Rust/TypeScript SDKs and schemas.
- [ ] Add parity, unknown-schema and secret-redaction tests.
- [ ] Run tests and commit.

### Task 19: Add security, load and operational acceptance gates

- [ ] Run all boundary/static scripts.
- [ ] Run unit/integration/property/migration/RLS suites.
- [ ] Run crash-at-every-handshake/revoke/import boundary tests.
- [ ] Run bounded multi-mount load tests.
- [ ] Measure revocation-to-nonqueryable latency under the approved local test profile.
- [ ] Verify no source keys/content/raw queries in logs, metrics or public schemas.
- [ ] Verify no silent copy/index/model/export/transitive behavior.
- [ ] Use `superpowers:verification-before-completion` before reporting success.
- [ ] Commit final verification/documentation updates.

---

## Rollout Order

1. ship domain/readers/schemas with deployment and workspace capability disabled;
2. apply migrations `0105`–`0109`;
3. verify forced RLS and database-role denial tests;
4. enable metadata-only invitation/inspection in isolated test workspaces;
5. exercise reconciled handshake and restart paths;
6. enable live read for explicit revision sets only;
7. verify source-unavailable and revoke fail-closed behavior;
8. add bounded typed filters and source collections;
9. enable deterministic Context Pack use for selected workspaces;
10. enable model use only after provider/locality tests;
11. enable derivation/import proposals only after envelope encryption and provenance tests;
12. enable target indexing/pinned snapshots only for explicit cohorts;
13. exercise purge/erasure/downstream cleanup before broad rollout;
14. publish H11 admin/SDK surfaces after parity/security review;
15. keep wildcard/transitive/mount-of-mount behavior permanently unsupported.

Rollback rules:

- disabling capability blocks new grant/mount/read/use operations but does not rewrite history;
- existing active mounts become suspended/stale according to policy;
- persistent representations quarantine before cleanup;
- no database down migrations are generated;
- local Memories/imports remain governed by their own lifecycle and accepted obligations;
- a rollback never re-enables revoked access or restores erased content.

---

## Acceptance Scenarios

### Scenario 1: Default disabled

Source and target have no enabled capability. Grant creation and mount acceptance are denied without any sharing rows.

### Scenario 2: `global` remains local

A workspace-global Memory is not visible in another workspace without an exact active grant/mount.

### Scenario 3: Exact read-only procedures library

Source grants structured-only deterministic use of explicit procedure revisions. Target reads them through source RLS. No local Memory, embedding or index is created.

### Scenario 4: Guessed source UUID

Target requests a source Memory outside selection. Response is denied/not-found without revealing existence.

### Scenario 5: Wildcard target attempt

Grant constructor rejects missing or wildcard target workspace.

### Scenario 6: Stale acceptance

Source replaces revision while target accepts the old hash. Source confirmation conflicts and target remains non-active.

### Scenario 7: Lost activation acknowledgement

Target remains pending. Reconciliation replays exact acknowledgement and activates once.

### Scenario 8: Duplicate acceptance

Same idempotency key/canonical input returns the original mount; changed input conflicts.

### Scenario 9: Source unavailable

Live read fails closed and stale cached content is not returned.

### Scenario 10: Query privacy structured-only

Target free-text query is not sent; source receives only typed filter categories.

### Scenario 11: Result budget exhausted

Content is denied before source bytes are disclosed; content-free receipt records budget denial.

### Scenario 12: Source revision changes during read

Result contains one exact revision or conflict; metadata/content never mix.

### Scenario 13: Source policy becomes stricter

Read is immediately reduced/denied and mount becomes stale where required.

### Scenario 14: Target policy becomes stricter

Target denies model use while source grant remains unchanged.

### Scenario 15: Target policy becomes broader

Existing source grant remains the ceiling; no access expansion occurs.

### Scenario 16: Remote model prohibited

Search/display may succeed, but remote model context is denied by `NoExternalProvider`.

### Scenario 17: Prompt injection content

Mounted procedure is marked foreign/untrusted and cannot change capabilities or policy.

### Scenario 18: Foreign fact conflicts locally

Target creates conflict proposal; foreign fact does not supersede local Memory.

### Scenario 19: Read creates no local copy

Row counts in target `memories`, revisions and indexes remain unchanged after search/context use.

### Scenario 20: Target embedding without permission

No index job or H8 target representation is created.

### Scenario 21: Explicit target embedding

Fresh target DEK and exact provenance are used; source key material is absent.

### Scenario 22: Index revoke

Index becomes non-queryable quarantine before physical cleanup finishes.

### Scenario 23: Pinned offline denied by default

Live grant cannot create retained snapshot without exact revision/retention permission.

### Scenario 24: Valid pinned snapshot

Exact revisions are target-encrypted, time-bound and clearly non-current.

### Scenario 25: Mount-of-mount

Target attempts to grant a mounted reference. Constructor/service denies it.

### Scenario 26: Transitive sharing

Workspace B cannot re-share A content to C; A must issue a direct grant.

### Scenario 27: Graph escape

Relation traversal stops at nodes outside selection and reveals no hidden content.

### Scenario 28: Deterministic context use

Mounted entry includes exact decisions, hash, grant/mount generation and origin.

### Scenario 29: Model context without permission

Read succeeds but model Context Pack inclusion is denied.

### Scenario 30: Revoke during active Run

Context dependency invalidates; future invocation pauses/rebuilds; historical invocation remains audited.

### Scenario 31: Revoke races with read

Disclosure cannot commit active epoch proof and returns no content.

### Scenario 32: Source hard purge

Live result disappears immediately; target caches/indexes quarantine; tombstone remains.

### Scenario 33: Source cryptographic erasure

No source bytes/keys are recoverable through mount; target obligations run truthfully.

### Scenario 34: Downstream cleanup response lost

Status remains `OutcomeUnknown`; source does not claim completed cascade.

### Scenario 35: Exact import

Target creates fresh encrypted representation and local Memory Candidate with complete shared provenance.

### Scenario 36: Foreign confidence import

Target local confidence is independently assessed and not copied automatically.

### Scenario 37: Import duplicate retry

Existing local Memory/source provenance is detected; no duplicate is created.

### Scenario 38: Import completion ambiguous

Proposal remains `OutcomeUnknown` until reconciliation.

### Scenario 39: Source key material scan

Target rows/logs/exports contain no source KEK, wrapped DEK, lease or backend path.

### Scenario 40: Cross-workspace SQL attempt

Application role cannot join target mounts to source Memory rows under supported interfaces.

### Scenario 41: Run export without permission

Mounted content is omitted/reference/tombstone according to export profile; no grant authority is transferred.

### Scenario 42: Run export with permission

Exact source/target decisions and provenance are included; importing bundle creates no mount or Memory.

### Scenario 43: Restricted content expiry

Expired grant denies content even if target cache exists.

### Scenario 44: Secret-like content

Source denies/quarantines it and target receives bounded tombstone without raw value.

### Scenario 45: Multi-mount bounded load

Concurrency, result and token limits hold; one slow/unavailable source cannot cause unbounded resource use.

### Scenario 46: RLS target isolation

One target workspace cannot see another target's mounts, acceptances or receipts.

### Scenario 47: RLS source isolation

One source workspace cannot manage another source's grants or collections.

### Scenario 48: Unknown event/schema version

Handshake/read/import blocks semantic processing and preserves safe opaque evidence only.

### Scenario 49: Audit parity

Source disclosure and target use receipts share correlation without storing content or raw sensitive query.

### Scenario 50: Feature disabled after activation

Future reads/uses stop fail-closed; history is retained and persistent representations quarantine according to policy.

---

## Definition of Done

Implementation is complete only when:

1. `global` remains workspace-local;
2. every grant binds one exact source and target;
3. target acceptance and source activation acknowledgement are both required;
4. grant revisions are immutable and lifecycle transitions use optimistic concurrency;
5. no cross-workspace Memory/grant SQL foreign key exists in target persistence;
6. no raw cross-workspace SQL/RLS bypass path exists;
7. source reads execute under source workspace RLS;
8. UUID/hash/reference alone never grants access;
9. live reads create no local Memory/index by default;
10. target indexing and offline snapshots remain disabled by default;
11. source and target policies intersect fail-closed;
12. source unavailable/stale/revoked/expired states disclose no content;
13. metadata/content are revision-consistent;
14. mounted origin/trust/provenance remain explicit;
15. mounted content cannot supersede local memory or elevate authority;
16. mount-of-mount and transitive sharing are rejected;
17. model, import, derivation, index and export require separate permissions;
18. persistent target representations use fresh target encryption material;
19. no source key/backend material enters target/public/export contracts;
20. revocation/purge invalidates caches, indexes, contexts and active Run dependencies;
21. ambiguous downstream cleanup remains `OutcomeUnknown`;
22. import uses ordinary Memory Core candidate/provenance/write-policy lifecycle;
23. Run export/import never recreates grant/mount authority;
24. durable event schemas/readers are registered before producers;
25. migrations `0105`–`0109` are forward-only and uniquely owned;
26. RLS, restart, idempotency, security, load and 50 acceptance scenarios pass;
27. implementation begins only after explicit authorization ending the documentation-only phase.

---

## Documentation-Only Boundary

Merging this plan authorizes only the implementation sequence and contracts. It does not authorize:

- creating `feat/cross-workspace-memory-sharing`;
- changing Rust code or dependencies;
- applying migrations `0105`–`0109`;
- enabling deployment/workspace sharing capability;
- creating real grants, invitations or mounts;
- opening source workspace reads;
- building target indexes or offline snapshots;
- persisting imported/derived shared content;
- sending mounted content to models;
- publishing admin, MCP or SDK surfaces;
- propagating revocation/purge against real data.

A separate explicit instruction is required before implementation.
