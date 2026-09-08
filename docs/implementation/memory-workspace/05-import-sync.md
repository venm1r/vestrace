# 05. Import, synchronization, and editorial conflicts

**Status:** Proposed contract. MW-04 initially enables new/unchanged imports; updates of existing
sources require MW-05 acceptance.

## 5.1 Input formats and bounds

Documents use ImportPreviewRequest with mode=documents, collection_id, base_collection_version,
scan_kind, and documents. Each item contains UUID external_id, relative_path, format, content,
classification, and explicit confidence/importance. Store Markdown/plain text as exact UTF-8;
perform no semantic-claim extraction or frontmatter execution.

Accept 1–100 documents, each containing 1–65,536 UTF-8 bytes, with at most 8 MiB decoded content.
The server checks bytes, not just JSON maxLength. Reject BOM and invalid UTF-8. Preserve CRLF/LF
and content Unicode exactly; path handling must not normalize the text. Identical text replay
means identical UTF-8 bytes.

Portability uses mode=portable and its own schema, not a format guessed by a model. Reject
unknown fields, duplicate JSON keys, and dangling references before canonical Apply. Bound
parser depth to 32 and nodes to 20,000, as well as byte size. JSON NaN/Infinity are invalid.

## 5.2 Local scanner

The proposed CLI group `vestrace sources scan|preview|apply|status|export` is an HTTP client,
not another server or direct database client. scan requires explicit --root and --state-file
outside the scanned root. Persist stable UUID mappings through a temporary mapping file and
atomic replacement; the mapping is not server authority.

Read only regular files with permitted suffixes. Reject symlink, junction/reparse points,
traversal, absolute/UNC/drive paths, and root escape. A file changing during capture yields
retryable scan-changed, not mixed bytes. A preliminary realpath check is insufficient: open
handles without following links and verify the object before/after reading on the supported
platform. Refuse a mode when the OS-specific safe path is unimplemented. Browser uploads do
not read the server filesystem.

Exclude .git, .env, private-key/credential files, node_modules, target, hidden directories,
and external URLs by default. These filters prevent some accidents, not all secrets in .md
files. Show the exact selection and require permission to upload. Do not record absolute
paths in audit.

A partial scan reports listed files only. A complete scan is the trusted client's explicit
assertion that the entire selected scope was inspected successfully. An absent old external_id
then creates Missing, never automatic source/memory deletion. Failed/partial scans cannot claim
completeness. Stable external_id plus a new locator preserves identity; otherwise renames need
explicit mapping. Equal hashes alone cannot establish a rename; identical files can be distinct sources.

## 5.3 Protected staging

POST /v1/source-imports validates shape, current rights, and supported labels first. Reserve
operation/input identity with idempotency, then store payload through narrow SourcePayloadStore,
a consumer of existing MaterialIntentCommands. It uses no embedding-output key operations and
cannot assume an unreviewed generic vault callback meets the contract.

Follow reserve → ciphertext preparation → bind → Live under the existing material protocol.
Vault/filesystem work happens outside PostgreSQL transactions. Store typed refs, receipts,
and metadata; audit/outbox contain IDs, not content. Bytes lost before ContentPrepared cannot
be recovered from a dead process. Require retransmission, lawfully retire the old intent before
a successor, and bind the new attempt to the same operation; do not report PreviewReady.

Return 201 PreviewReady only when **all** approved input snapshots are durable. Exact replay
of a lost response returns the existing ready operation. Partial staging creates no active
memory and stays staging/needs_upload. Retransmitted content is usable only when the semantic
request tuple matches.

The proposed default preview expiry is 24 hours. Expiry initiates lawful retirement. Time alone
does not prove a live vault writer is gone: the expiry fence/material lifecycle must defeat
that writer before erasure. Staging cannot fetch arbitrary paths/URLs, download dependencies,
or invoke an LLM.

## 5.4 Immutable preview

Pin operation_id, preview_revision, collection configuration version, policy version, immutable
inputs, base source head, base memory head/state revision, and each item's disposition.

| Disposition | Decision basis |
| --- | --- |
| new | external_id is absent from the collection. |
| unchanged | Exact source content/format/label and locator did not change. A random new operation ID does not create a revision. |
| rename | Only locator changed under established identity; record metadata history without a content revision. |
| update | Source changed, effective memory still matches the last accepted import, and manual_override=false. |
| conflict | Source changed alongside manual editing/override, or saved versions cannot establish a safe semantic choice. |
| missing | Explicit complete-scan observation, not deletion. |

Changed classification on existing source-backed memory returns CLASSIFICATION_TRANSITION_REQUIRED,
not update. A needed label transition belongs to a separate accepted governance workflow.

## 5.5 Confirmation and per-item application

Apply accepts exact preview_revision and item_ids. Recheck current authorization, expiry,
configuration, and base collection version. Atomically reserve active_import_id, freeze selected
items, write audit/receipt, and enqueue one `memory.source_import.apply` message per mutable
item. The topic must have a production handler before it is published.

Preview does not permanently authorize future bytes. Changed input requires another preview;
a collection change before Apply returns PREVIEW_STALE without partial admission. At most one
batch applies per collection. Manual editing remains allowed and is observed by per-item CAS.

Resolve the initiating principal from durable identity and current grants. The worker's own
ExecutionWrite cannot elevate that actor. Revocation produces blocked_policy without undoing
already committed items. Apply each item through shared commit_in.

One item transaction includes capture Event, immutable SourceRevision, Memory/Revision/SourceLink,
binding, search projection, audit, exact receipt, and progress. No source head points to an
incomplete revision. A batch is not one long transaction and may finish completed_with_issues.

Death after item commit/before outbox acknowledgement replays from the receipt. Pre-commit
failure leaves no new head. Unchanged input records an observation outcome, not a canonical
revision or embedding request. Use the accepted index-invalidation path; no raw-provider shortcut.
Without an available supported indexing path, report pending/blocked instead of ready.

## 5.6 Progress, cancellation, and expiry

Operation states: staging, needs_upload, preview_ready, applying, completed,
completed_with_issues, cancelled, expired. These are business progress, not another runtime.

Item states: staged, queued, applied, unchanged, conflict, missing, skipped, blocked_policy,
failed. Applied items have immutable receipts. Existing outbox retry policy owns transport/
storage failures; exhausted attempts become failed with dead-letter evidence, not infinite pending.

Index states are separate: not_required, pending, ready, blocked, failed. Ready requires actual
qualified publication, not drained-message count. Worker --once exit 0 reports cycle work, not
success of the entire import.

Cancellation takes operation/collection guards. After the fence, new item transactions cannot
apply. An already-started canonical commit wins wholly or rolls back. Keep committed content;
mark unprocessed items skipped and report the applied subset. Expiry and Apply must serialize
so cleanup cannot retire material already owned by accepted application.

## 5.7 Three-way synchronization

B = accepted base source revision; I = immutable incoming source revision; M = current effective memory.

| Source changed | Manual edit/override | Outcome |
| --- | --- | --- |
| No | No | Unchanged. |
| No | Yes | Preserve M; another scan alone creates no conflict. |
| Yes | No | CAS applies I as a new effective memory revision. |
| Yes | Yes | Preserve B/I/M in SourceConflict and keep M unchanged. |

accept_source creates a revision from I, clears manual_override, and advances the binding.
keep_manual preserves M and its content revision, records a resolution Event binding override
to I, and keeps manual_override=true; another source change needs fresh review. merge uses
explicit user-authored text to create a new revision with B/I/M provenance and override=true.
It is human merging, not deterministic repair.

All decisions require expected_conflict_version, exact current memory/state, and incoming
source references. A changed basis after inspection returns 409. Resolution never rewrites I,
deletes B, or makes a source timestamp authoritative.

Initially allow one open conflict per source. New sync returns conflict_pending rather than
silently superseding its bases. Resolve or explicitly cancel the current conflict through an
accepted procedure before replacing it. This prevents false two-way merges.

## 5.8 Preview envelope and rollout

For complete scans, the 100-item limit covers the union of incoming and potentially Missing
sources. If larger, refuse INPUT_LIMIT_EXCEEDED and use bounded partial scans. Never truncate
or infer Missing from an incomplete snapshot. Empty complete scans need a separate future
explicit-empty confirmation contract and are not automatically executed in the MVP.

MW-04 enables new/unchanged inputs only; MW-05 enables updates. Show not_enabled for unsupported
sync rather than bypassing it with direct update. PreviewReady decisions are immutable;
repreview is new intent with a new key.

## 5.9 Additional editing after conflict detection

source_conflicts.manual_revision_id is detection-time M0 and remains immutable. ConflictDetail
returns original_manual_revision=M0, current permitted manual_revision=M, and memory_state_revision
consistently. Extra editing does not make a conflict permanently unresolvable: show M0→M,
then confirm against current expected_memory_revision_id/state. Preserve original B/I/M0;
record the M actually used in resolution evidence.

Ordinary correction of bound memory atomically sets manual_override=true and advances the
binding version inside the shared writer/UoW. Authorship comes from the typed route/command,
not caller-supplied origin_kind. Import updates last_import_memory_revision_id without
impersonating a manual edit. Explicit conflict resolutions update the binding according to
their decision.
