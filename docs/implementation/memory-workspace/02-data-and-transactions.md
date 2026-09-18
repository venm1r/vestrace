# 02. Data model and transaction authority

**All new objects/tables below are proposed.** They extend existing Memory, MemoryRevision,
Event, and MemorySource rather than replacing them.

## 2.1 Objects and ownership

| Object | Immutable identity/content | Mutable state |
| --- | --- | --- |
| SourceCollection | Workspace and local identity. | Name/configuration revision; one active Apply. |
| KnowledgeSource | Workspace, collection_id, external_id. | Current revision, locator, state/version. |
| SourceRevision | Source, ordinal, exact payload material reference, format, declared classification, capture event. | None; a change creates a new revision. |
| MemorySourceBinding | Memory-to-source relationship for whole-document representation. | Source revision, last imported memory revision, manual_override, version. |
| MemoryRevisionSourceLink | Exact revision-to-event/source-revision relationship and origin kind. | None. |
| ImportOperation | Actor, workspace, inputs, mode, selected policy, request identity. | Staging/preview/apply progress, not an independent scheduler. |
| ImportItem | Input identity, saved payload, disposition, base versions. | Receipt, conflict, or cancellation outcome. |
| SourceConflict | B/I/M triple and detection versions. | Explicit version-preconditioned resolution. |
| ExportOperation | Actor, pinned selection, mode, format. | Preparation state and protected result reference. |

Memory owns effective content. SourceRevision cannot become a competing current-memory head.
Source text and human-corrected memory may differ; display that valid state explicitly.

## 2.2 Identity and provenance

Source identity is `(workspace_id, collection_id, external_id)`, not path or hash. A stable
external UUID comes from the scanner/user; path is a locator. Initially each source produces
one MemoryKind::Observation with origin_kind=source_excerpt, not an automatically confirmed
semantic fact. Require explicit confidence/importance in preview settings and label them
as user assessments. Never assign 1.0 as evidence of truth. Ordinals increase per object;
timestamps do not provide a global revision order.

Write new revision-source links in the same transaction as their revision. Mark historical
memory_sources without exact attribution `legacy_unattributed`; do not backfill from timestamp
proximity. These links may describe memory-level history but not the proven basis of a
particular revision. Foreign IDs are annotations, never local actor/workspace/policy/key authority.

## 2.3 Proposed relational schema

```text
memory_workspace_epochs(workspace_id PK, version bigint)
memory_revision_source_links(workspace_id, memory_id, revision_id, event_id,
  source_revision_id nullable, origin_kind, actor_id, created_at)
source_collections(workspace_id, id, name, version, active_import_id nullable, created_at)
knowledge_sources(workspace_id, id, collection_id, external_id, relative_path,
  state, version, current_revision_id, created_at)
source_revisions(workspace_id, id, source_id, revision_number, payload_material_id,
  payload_intent_id, format, classification, capture_event_id, recorded_at)
memory_source_bindings(workspace_id, memory_id, source_id, source_revision_id,
  last_import_memory_revision_id, manual_override, version)
source_import_operations(workspace_id, id, principal_id, mode, state, version,
  collection_id, base_collection_version, preview_revision, expires_at,
  accepted_policy_version, created_at)
source_import_items(workspace_id, id, operation_id, ordinal, external_id,
  payload_material_id, base_source_revision_id, base_memory_revision_id,
  disposition, state, applied_revision_id, conflict_id)
source_conflicts(workspace_id, id, source_id, memory_id, base_source_revision_id,
  incoming_source_revision_id, manual_revision_id, version, state,
  resolution_event_id nullable)
memory_export_operations(workspace_id, id, principal_id, state, version,
  format, expires_at, pinned_selection, result_material_id nullable, created_at)
```

Epochs invalidate browse cursors; they are not canonical knowledge. Revision-source links use
PK `(workspace_id,revision_id,event_id)` with same-workspace composite FKs to MemoryRevision
and Event. Collections are unique by `(workspace_id,id)`. Sources are unique by
`(workspace_id,collection_id,external_id)`. Source ordinals are unique by
`(workspace_id,source_id,revision_number)`; payload and capture are immutable.

The initial binding is unique per `(workspace_id,source_id)` and `(workspace_id,memory_id)`.
Import items are unique by `(workspace_id,operation_id,ordinal)` and external_id within a
batch. Conflict references are immutable; resolutions are append-only.

Every child FK includes workspace_id. New content/authority tables require FORCE RLS,
guarded ownership, no direct runtime DML, and named scoped entrypoints. Test SQL CHECK
closed sets against domain enums. Do not store raw DEKs, Bearer tokens, global plaintext
hashes, or full content in audit/outbox.

Source payloads are material references, not new plaintext content columns. Effective memory
continues through its existing persistence/security envelope; this proposal does not claim
that historical memory storage is all encrypted. MW-00 must verify supported labels on every
used path; refuse unsafe labels rather than introduce a fallback store.

## 2.4 Atomic mutation

Use existing GovernedMutation and `commit_in(&mut dyn UnitOfWork, ...)`. The memory writer
MUST participate in the caller-owned transaction, without invoking independently committing
save_memory_with_revision as a nested operation.

1. Resolve trusted authentication and perform available preliminary checks.
2. Open a scoped transaction with InstallationMutationPermit.
3. Lock/check canonical idempotency identity. Exact committed replay returns its receipt
   **before repeated state CAS**, but **after current authorization of the result**.
4. Acquire configuration/policy, optional collection/source/binding, and memory guards in
   the agreed order; sort IDs within each class. Do not invoke provider/embedding/vault here.
5. Recheck permissions, classification, and expected revisions under current guards. Commit
   capture/correction Event, revision, source, exact link, active head, search projection,
   audit, outbox, and receipt through the shared authority.
6. Verify the transactional database trigger advances the browse epoch; do not advance it
   again manually. Commit.
7. Return the receipt; existing outbox processing handles derived work after commit.

MW-00/MW-02 reconcile guard order with every current writer. Do not reorder installation,
material, or provider guards for import convenience. Resolve order conflicts before code,
not by accepting a potentially deadlocking transaction.

## 2.5 Idempotency

New HTTP keys are UUIDs. Server storage is namespaced by operation, authenticated principal,
and workspace; target belongs in the fingerprint. Legacy routes retain their wire contract
through an explicit adapter. Exclude attempt IDs, generated event/revision IDs, and ciphertext
from logical request equality.

Normalize the closed schema with explicit omitted/null behavior. Array order is either
meaningful or canonically sorted as the API specifies. Sensitive inputs use approved keyed
commitments, never public content-hash lookup. Serialization failure is an error, not the
hash of an empty string.

A changed body under one key returns 409. Exact replay after a lost response returns the
same receipt without another revision, audit success, or outbox entry. An unfinished contender
waits boundedly or receives retryable busy rather than starting a second mutation. Retain
receipts while operation/export/retry references require them; do not silently extend legacy
retention. Current authorization always applies to receipt disclosure.

Import identity is `(operation_id,item_id,semantic input)`. Different dispatchers cannot
apply the item twice. After commit with lost acknowledgement, replay reads the durable
item receipt and finishes without repeating side effects.

## 2.6 Read consistency

Read active pointer, revision, and permitted sources consistently. Metadata access is not
historical-content access; check each revision/source. Rights changes before linearization
must be observed. Bytes already sent cannot be recalled.

Browse uses keyset `(created_at,id)` and a tamper-protected opaque cursor bound to principal,
workspace, filter hash, epoch, and policy version. Changed epoch/policy produces CURSOR_EXPIRED;
restart the query. Do not hold a multi-minute transaction or expose hidden IDs in cursors.
Omit totals unless safe semantics are established. History pins the highest visible revision
on its first page, but reauthorizes every page. Retention/erasure applies to history, caches,
receipts, and export-derived reads.
