# 03. API and client contract

**Status:** Proposed additive HTTP surface. [JSON Schema](contracts/contracts.schema.json)
defines shapes; [OpenAPI](contracts/openapi.proposed.json) covers only these new operations,
not the whole Vestrace API.

## 3.1 Compatibility

Preserve legacy GET memory, POST memory, POST revisions, and POST retrieval/search wire shapes.
The new detail endpoint must not silently add confidential content to the old metadata endpoint.
Legacy numeric If-Match is not silently converted to an RFC entity-tag. Corrections use body
expected_revision_id and expected_state_revision only, without a hidden alternative If-Match.
Any legacy-header migration is separate work.

New mutation routes require a UUID Idempotency-Key, explicit reasons for editorial decisions,
and rejection of unknown body fields. Authentication owns RequestContext: imported JSON cannot
override principal/workspace. Console reuses the current transport/auth flow; tokens never
enter URLs, localStorage, or exports.

## 3.2 Proposed operations

| Method/path | Request → response | Authorization and meaning |
| --- | --- | --- |
| GET `/v1/memories` | Query → MemoryPage | MemoryRead; permitted filters/pagination. |
| GET `/v1/memories/{memory_id}/detail` | → MemoryDetail | MemoryRead and revision classification; content/current state. |
| GET `/v1/memories/{memory_id}/revisions` | Cursor → RevisionPage | Authorized history without guessed legacy attribution. |
| GET `/v1/memories/{memory_id}/revisions/{revision_id}` | → Revision | Exact historical revision with current read policy. |
| POST `/v1/memories/{memory_id}/corrections` | CorrectionRequest → MutationReceipt | MemoryWrite and readable basis; new revision, not overwrite. |
| POST `/v1/context-packs` | ContextRequest → ContextResponse | ContextRetrieve, approved destination, actual content. |
| GET `/v1/source-collections` | → CollectionPage | MemoryRead; no global scope enumeration. |
| POST `/v1/source-collections` | CreateCollectionRequest → Collection | MemoryWrite, reason, audit, receipt. |
| GET `/v1/source-collections/{collection_id}/sources` | Cursor → SourcePage | Permitted source identities/state. |
| GET `/v1/sources/{source_id}/revisions` | Cursor → SourceRevisionPage | History; payload requires content permission. |
| POST `/v1/source-imports` | ImportPreviewRequest → ImportPreview | MemoryWrite; staged input is not active knowledge. |
| GET `/v1/source-imports/{operation_id}` | → ImportOperation | Creator or explicitly authorized operator; actual progress. |
| GET `/v1/source-imports/{operation_id}/items/{item_id}` | → ImportItemDetail | Diff/B/I/M only after read checks. |
| POST `/v1/source-imports/{operation_id}/apply` | ApplyImportRequest → ImportOperation | Reauthorize every item; 202 is not indexed. |
| POST `/v1/source-imports/{operation_id}/cancel` | CancelRequest → ImportOperation | Stop uncommitted items; keep committed data. |
| GET `/v1/source-conflicts/{conflict_id}` | → ConflictDetail | Readable B/I/M and expected versions. |
| POST `/v1/source-conflicts/{conflict_id}/resolve` | ResolveConflictRequest → MutationReceipt | Atomic resolution/revision/binding. |
| POST `/v1/memory-exports` | ExportRequest → ExportOperation | ExportRead, MemoryRead, permitted revisions. |
| GET `/v1/memory-exports/{operation_id}` | → ExportOperation | State, expiry, safe counts. |
| GET `/v1/memory-exports/{operation_id}/content` | → Package JSON / Markdown | Reauthorize the whole pinned selection before delivery. |

Memory permission does not imply access to its source Event. A self-contained explanation
may show provenance_status=partial without denied names, IDs, or counts. Unknown and inaccessible
objects return the same 404; an explicitly forbidden operation on an accessible object returns 403.

## 3.3 Detail, history, and correction

MemoryDetail includes id, kind, status, state_revision, active_revision, and can_correct.
active_revision may be null when no active content exists; then editing is disabled. Revision
includes identity, ordinal, content, classification, created_at, validity, reason, visible
sources, and provenance_status. can_correct is a momentary UI hint—not a capability.

Restore reads the selected old revision under current policy, then creates a new revision
with a reason. Preserve current classification and refuse denied historical content. Reauthorize
on POST even when the earlier detail response allowed editing.

## 3.4 Actual context and budget

Return sections[].items[].text, source/revision refs, and representation levels. Deterministically
render the same items into rendered_context; source instructions/HTML gain no privilege.
Missing or withheld text must not become a technical explanation masquerading as knowledge.

max_utf8_bytes bounds the **whole** rendered string, including citations/separators.
estimated_tokens is an estimate; ceil(bytes/4) is not a hard-token guarantee. Optional token_budget
requires tokenizer_id registered and qualified for the model/destination. An unavailable counter
returns TOKEN_COUNTER_UNAVAILABLE, never a silent fallback. Without token_budget, set
token_budget_enforced=false and claim only byte-bounded output. Count final rendered text in
qualified token mode; the external model request envelope remains the client's responsibility.

Reuse RetrievalService, not a second ranking engine. Check epoch, policy, and eligibility before
disclosure. Query rewording, larger budget, and history mode cannot bypass denial. A generation
change between search and hydration permits one bounded retry without provider side effects,
or a typed conflict—not mixed generations.

## 3.5 Errors

| Code | HTTP | Client action |
| --- | ---: | --- |
| VALIDATION_ERROR | 400 | Correct input; no automatic retry. |
| IDEMPOTENCY_KEY_REQUIRED | 400 | Generate one UUID per logical operation. |
| NOT_FOUND | 404 | Reveal no other-scope existence. |
| POLICY_DENIED | 403 | Do not bypass through another endpoint. |
| REVISION_CONFLICT | 409 | Read fresh detail and compare changes. |
| IDEMPOTENCY_CONFLICT | 409 | New intent requires a new key. |
| PREVIEW_STALE | 409 | Create a new preview; old consent is insufficient. |
| COLLECTION_BUSY | 409 | Finish/cancel active Apply, not another queue. |
| CURSOR_EXPIRED | 409 | Restart browsing. |
| CLASSIFICATION_TRANSITION_REQUIRED | 409 | Use separate governance, not editor/import label changes. |
| INPUT_LIMIT_EXCEEDED | 413 | Reduce input; no truncated success. |
| TOKEN_COUNTER_UNAVAILABLE | 422 | Select a supported counter or explicitly request byte-only output. |
| PREVIEW_EXPIRED / EXPORT_EXPIRED | 410 | Prepare again; no expired-cache read. |
| RETRIEVAL_NOT_READY / MATERIAL_UNAVAILABLE | 503 | Show the blocker, not an empty successful response. |

Errors omit raw SQL, content, credentials, DEKs, and denied foreign IDs. request_id supports
diagnostics; it is not Idempotency-Key. Extend ApiErrorBody additively with safe details while
preserving code/message.

## 3.6 Client/MCP integration

Extend `apps/console/src/sdk/client.ts`; place typed memory methods in memoryClient.ts.
On 409 preserve the draft. For unknown POST results reuse identical body/key, never blindly
overwrite. Logout/scope switching clears content caches/drafts; query keys include authenticated
scope, resource, revision, and filters.

MCP uses the same authorized detail service, preserves existing tool names, and exposes neither
raw material references nor model-selected principal overrides. New write tools and external DSH
integration are outside this package. Regression-test existing tools when improving their reads.
