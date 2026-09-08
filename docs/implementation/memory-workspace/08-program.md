# 08. Memory Workspace implementation program

**Status:** Proposed; not a P01–P12 amendment or blanket write authorization.
Original source review remains `6f610253`. MW-00 reconciles the supplied `3e05dfbd` archive
and any later implementation checkout.

| Package | Deliverable | Predecessors | Exit boundary |
| --- | --- | --- | --- |
| MW-00 | Accepted delta, scope, and dependency map. | This design. | Fresh tree, exact paths/authority, no unresolved P04 overlap. |
| MW-01 | Read schema/epoch, detail/history/browse, actual context. | MW-00. | Runtime disclosure tests; qualify context separately from detail reads. |
| MW-02 | Shared atomic writer, correction/restore/replay. | MW-00, MW-01. | One UoW, CAS, exact replay, no legacy bypass. |
| MW-03 | Wired Console library/editor. | MW-01, MW-02. | Real browser → HTTP → database edits/history/conflicts. |
| MW-04 | Collections, staging, preview, scanner, import handler. | MW-02, MW-03. | Exact bytes, lawful owner, production worker application. |
| MW-05 | Resync, B/I/M resolution, cancellation, Missing. | MW-04. | Preserved edits and both race outcomes verified. |
| MW-06 | Export/portable import, UI/CLI. | MW-04, MW-05. | No authority laundering; protected download. |
| MW-07 | Upgrade, negative tests, end-to-end feature acceptance. | MW-01–MW-06. | Complete declared workflow and recorded limitations. |

Accept packages separately. Read-only MW-01 design can start before complete embedding closure,
but its context gate requires an actually available retrieval generation. MW-04 adds no model
calls and cannot bypass P04. Existing embedding outbox obligations remain even when indexing
is blocked. Committing memory alone does not make an import indexed.

## Scope and migrations

The [file plan](file-plan.json) distinguishes original inspected sources, references needing
reread, and candidate new files. Each package needs its own preflight/exact allowlist; a new
file or dependency requires a scoped amendment. Do not rewrite P04 scope lists.

Candidate 0196–0199 names are not reservations. **0196 is already occupied in the supplied
archive.** Reconcile all related paths/references before the first SQL write. Accepted P04
publication work must be reused rather than repeated from an older proposal.

## Execution discipline

Follow specification → behavioral RED → minimal implementation → GREEN → negative/mutation
acceptance → independent review → evidence. A builder's self-review is not independent review.
Partial successful log excerpts are not a complete test run.

Create valid targets/fixtures before expecting meaningful RED. Missing targets, compilation
errors, and absent DATABASE_URL are setup failures. E2E data is produced through actual
entrypoints; administrative preparation is limited to installation/migration/grants, not
fabricated completion rows, source records, or receipts.

Shared commands and the full task procedure are in [plans/README](plans/README.md). Every
requirement links to a task and acceptance case through [traceability](traceability.json).
New interfaces and test names are proposed until implemented. No commit/push/deploy permission
arises merely from this program. Release requires a separate exact-target decision.
