# 01. Design decisions and boundaries

**Status:** Proposed for acceptance. MUST in this package expresses a requirement for the
extension once accepted, not a statement that the behavior exists today.

## Approach

Three approaches were considered: a separate memory service/database, a UI-only wrapper,
and an end-to-end extension of the current canonical model. Select the third. A separate
service introduces competing authority; the UI wrapper does not solve missing content or
atomic editing. Reuse current crates, PostgreSQL, HTTP, MCP, and Console. A separately
published SDK or external runtime integration is not required for initial acceptance.

## Proposed decisions

| ID | Decision | Rationale |
| --- | --- | --- |
| MW-D01 | One memory-first product and canonical writer. | Editing and import follow the same rules. |
| MW-D02 | Additive read/detail/context routes. | Preserve metadata DTOs and legacy If-Match. |
| MW-D03 | Corrections create revisions, not in-place historical edits. | Restoring old text preserves history. |
| MW-D04 | Source revisions are immutable and separate from human corrections. | Sync cannot silently overwrite editorial work. |
| MW-D05 | Initially one small text document produces one source-excerpt memory. | Avoid unstable chunk matching and LLM extraction. |
| MW-D06 | Preview binds stored exact inputs and base versions. | Confirmation cannot authorize a different file/state. |
| MW-D07 | Each item is atomic; batches may partially complete. | Avoid transactions spanning a hundred documents. |
| MW-D08 | Existing OutboxDispatcher performs background application. | No separate scheduler, lease, or retry runtime. |
| MW-D09 | Missing files create Missing observations, not Delete. | Lost access is not an instruction to destroy knowledge. |
| MW-D10 | Existing material lifecycle stores staged source snapshots. | No new unprotected content store. |
| MW-D11 | Knowledge portability is neither installation backup nor authority transfer. | Receiver assigns scope, labels, and trust anew. |
| MW-D12 | Qualify the specific feature, not the whole project by inheritance. | An alpha does not close P04/P12 or TRUSTED. |

These are decisions within a **proposed** package, not Accepted ADRs.

## User scenarios

| ID | Required outcome |
| --- | --- |
| UC-01 | Find memory, read permitted content, and inspect revision/provenance. |
| UC-02 | Correct text, receive a durable receipt, and see the new revision with old history. |
| UC-03 | Preview and explicitly confirm Markdown, plain text, or versioned JSON input. |
| UC-04 | Resynchronize sources: unchanged input creates no canonical revision; changed input creates source versions. |
| UC-05 | Preserve independent source/manual changes and resolve the conflict explicitly. |
| UC-06 | Obtain permitted ContextPack content with exact citations. |
| UC-07 | Export a bounded permitted selection into another workspace with new local identities and reauthorization. |

## Interface and authority boundaries

Add Library, Memory Detail, Sources, Import Preview/Progress, Conflict Resolution, and Export
to the existing Console using its design system. Ordinary Memory Detail does not provide
classification changes, capability grants, or irreversible purge.

Source content records what a file contained; it is not proof of truth. Active is a lifecycle
state, not automatic Confirmed/Trusted. Confidence is a supplied assessment, not a calibrated
probability. File timestamps do not automatically establish occurred_at.

Import does not execute Markdown, HTML, JavaScript, frontmatter, or JSON scripts. A model
cannot choose actor/scope. Memory content is data, not privileged system instructions.
Reimport after a correction cannot change memory kind, classification, or permissions under
the name of synchronization. Trust changes and text corrections are separate commands.

## First-version envelope

Allow at most **65,536 UTF-8 bytes per document**, **100 documents**, and **8 MiB decoded
content per batch**. Support `.md`, `.txt`, and JSON matching the supplied schema. These
are chosen product limits, not measured capacity. Reject oversized input entirely, with
no silent truncation or automatic chunking.

Exclude ZIP uploads, OCR/PDF parsing, crawling, automatic watchers, remote Git credentials,
multisource semantic merging, and LLM summaries. Sync is explicitly initiated and previewed.
Renames require stable external_id or confirmed mapping.

## Context-mode compatibility

Byte-only output is a bounded memory representation, not the normative hard-token ContextPack.
An endpoint name does not broaden its claim. Explicitly accept this limited API extension
before enabling it. Existing consumers requiring strict token bounds must select a qualified
tokenizer or receive refusal; never switch them silently to byte-only output.
