# 04. Console: library, editor, and sources

**Status:** Proposed UI behavior; the corresponding API/application paths must be implemented.

## 4.1 Information architecture

Add `/memory`, `/memory/:memoryId`, `/memory/:memoryId/history`, `/sources`,
`/imports/:operationId`, `/conflicts/:conflictId`, and `/exports/:operationId` to the existing
BrowserRouter. Reuse AppLayout, ErrorBoundary, Surface/Button, and design tokens. No new brand,
second SPA, or graph editor is required.

Reuse MemoryConsole.tsx for detail, content/history, and editorial actions. Proposed route
modules are MemoryPage, SourcesPage, ImportPage, and SourceConflictPage. Export status can use
an existing panel rather than a large separate subsystem.

## 4.2 Library

Browse and search are separate: an empty browse input calls GET /v1/memories, not a fabricated
semantic query. Filters are kind, status, collection, and server-supported temporal modes.
Order by created_at/id, default page size 50; no totals from unauthorized record lists.

Represent loading, empty, ready, query-error, cursor-expired, forbidden, and unavailable
separately. Never substitute empty for 503. On CursorExpired, preserve filters, explain the
refresh, and request page one. Abort pending fetches on workspace switch and discard any late
response from the previous scope.

## 4.3 Detail and editing

Show kind, status, content, ordinal/state revision, declared classification, provenance_status,
source links, and the latest correction reason. Label percentage confidence **supplied assessment**,
not probability of truth.

Editor states are viewing → editing → submitting → applied, plus conflict, failed, and
result-unknown. Preserve drafts after conflict/network failure. For 409 display base, current,
and draft side by side. A deliberate new intent after comparison gets a new key. An unknown
result reuses the original body/key; never claim the mutation did not occur.

Restore requires a permitted revision, text preview, and reason, and creates a new revision.
Classification is read-only, including server refusal of developer-tool tampering. Restore
cannot copy denied history into a less restricted current record.

Distinguish **from source**, **corrected by user**, and **imported history**. Display
legacy-unattributed provenance as a limitation, not invented source/actor attribution.

## 4.4 Sources and import

Show name/relative_path, latest source revision, effective memory revision, manual override,
and processing state. The sender's absolute filesystem path is unnecessary.

The flow is select files → explicitly choose collection/label/ratings → upload and obtain a
durable preview → inspect new/update/unchanged/conflict/missing → select items → confirm exact
preview_revision → monitor progress. Disable Apply until PreviewReady. An unknown upload result
does not justify submitting a different batch automatically.

Show canonical application, FTS readiness, and embedding/retrieval readiness separately.
“100% imported” does not mean searchable. Expose item failures in partial batches, preserve
committed outcomes, and make retries nonduplicating. Cancellation stops only uncommitted items;
explain that already applied changes remain.

## 4.5 Source/editor conflicts

Show B (source basis of the edit), I (incoming source), and M (current effective manual content).
Allow exactly accept_source, keep_manual, or merge with explicit text/reason. No background
LLM merge exists in this scope.

Changed heads/policy before confirmation produce 409/403 and require fresh bases. An old diff
cannot authorize unseen content. keep_manual records an override instead of changing I; the
next source change requires review again. When M changed after detection, show immutable M0
and current M separately as defined in [the sync contract](05-import-sync.md).

## 4.6 Rendering and accessibility

Initially use text/pre-wrap. Any later Markdown renderer disables raw HTML, opens only safe
http/https links, and executes no javascript/data/file links, remote images, scripts, or frames.
Never pass imported content to dangerouslySetInnerHTML.

Dialogs need accessible names, focus trapping, and focus restoration. Associate errors with
fields; communicate status through more than color. Use unobtrusive aria-live progress. During
submission, show state rather than enabling a second request with a new key. Do not store drafts
in localStorage; export downloads require explicit user action.

## 4.7 Verification

Use Node tests for pure reducers/view-models, following existing mjs conventions. MW-03 adds
test-only Playwright with an exact lockfile version after dependency review. test:memory and
test:e2e:memory are proposed, not existing at the original baseline. Typechecking/component
tests do not establish a real browser → HTTP → PostgreSQL workflow.

Required browser cases include keyboard edit/restore, conflicting sessions, scope switching
during fetch, 503 versus empty, denied history, and hostile Markdown/HTML without network loads.
