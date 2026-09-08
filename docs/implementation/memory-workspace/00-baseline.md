# 00. Original source review and current delta

**Original baseline:** `6f6102536e9a535b7086db14573bf45fe750ad71`;
parent `58e7dac3cef7cc106d59f7ba3560bdbd37de76c0`. The 2026-09-07 commit was
`feat(p04): advance reopened embedding lifecycle`. The original review read selected
GitHub files; it did not inspect the author's local checkout or execute the runtime.

## 0.1 Historical observation and editorial update

[S01](sources.md#s01) originally recorded accepted `worker --once` outcomes (0: work, 3: idle,
1: error), 14B pre-dispatch termination, and final 14D acceptance through durable ResultPrepared.
That verdict did not include Live publication, Succeeded, complete rebuild/retrieval-query
paths, or worker composition. At the end of that historical section, 14E was proposed.

**The supplied `3e05dfbd` archive is newer.** Its evidence ends with final Task 14E approval,
not merely a proposal. The approval remains task-scoped, retains a rotation-before-adoption
deferral, and explicitly leaves P04/G0/v1.0 incomplete. This edition read the record but
ran none of its runtime tests. See [current status](../../status.md).

Do not reimplement accepted 14B–14E slices. Read/editor work can be developed separately;
context availability still depends on real RetrievalService generation/policy readiness.
The original proposed MW-M01 filename uses 0196, which is now occupied by
`0196_retired_credential_erasure.sql`. Reconcile all candidate references in MW-00 before SQL.

## 0.2 Source gap map

| ID | Original static observation | Consequence |
| --- | --- | --- |
| GAP-01 | MemoryResponse has id/kind/status/classification/dates but no content/current revision [S10]. | Add a separate authorized detail DTO; preserve legacy shape. |
| GAP-02 | ContextPackDto exposes counters, not sections [S11]. | Return actual permitted sections through an additive endpoint. |
| GAP-03 | MemoryUseCases provides record/remember/revise/find but no list/history [S03]. | Add a query port, not N browser requests to assemble a library. |
| GAP-04 | Repository commits memory/revision/source/search; service writes outbox/idempotency later [S04–S06]. | Include all durable command facts in a caller-owned UoW. |
| GAP-05 | GovernedMutationRepository::commit_in exists [S07]. | Reuse transaction authority, without nested independently committing operations. |
| GAP-06 | Memory fingerprints already exclude newly allocated IDs [S05]. | Do not report that fixed problem as new; test server races/replay. |
| GAP-07 | Idempotency supports save_in and workspace-scoped records [S09]. | Define operation/principal result ownership and explicit legacy-key compatibility. |
| GAP-08 | OutboxHandler is at-least-once; replay can occur after commit/before acknowledgement [S08]. | Atomic item receipts and convergent application are required. |
| GAP-09 | MemorySource INSERT lacks revision_id [S06]. | Add exact links for new revisions; never infer historical attribution. |
| GAP-10 | resolve_revision_classification rejects label changes/clearing [S05]. | Keep the editor read-only for classification; changed-label sync is blocked. |
| GAP-11 | MemoryConsole.tsx exists but main.tsx lacks a full memory route [S13–S14]. | Wire the screen and SDK into the existing Console. |
| GAP-12 | package.json has typecheck/build/test:protocol, not a general test script [S16]. | Implement the named new scripts before using them. |
| GAP-13 | conservative_token_count estimates ceil(UTF-8 bytes / 4) [S12]. | Not a strict bound for arbitrary tokenizers. |
| GAP-14 | The context builder can substitute explanation for empty content [S12]. | New output cannot present technical explanation as source knowledge. |
| GAP-15 | MaterialIntentCommands supplies shared content lifecycle [S17]. | Staging consumes ordinary materials, not an embedding-output shortcut. |

The earlier S03–S17 blob hashes match the supplied archive; that byte comparison preserves
applicability of those selected source observations, not runtime qualification. Evidence S01
has changed. Full details are in the [source delta](../../maintenance/source-delta.json).

## 0.3 Unestablished claims

The review does not prove absence of every import/export implementation anywhere in the
repository. MW-00 must check names and pending branches. Build time, RAM consumption, current
CI success, actual retrieval quality, and production readiness are not established here.

The existing memory repository has database CAS. The gap is the larger atomic/replay
boundary, not complete absence of concurrency protection.

## 0.4 Stop conditions

Confirm compatibility with frozen architecture before extending it; MW neither replaces
P05 nor closes P04. Block import when lawful material read/staging cannot support a label;
a new plaintext directory/table is not a fallback. A source record does not mean its index
is Ready. Historical detail reads require the same appropriate disclosure checks as retrieval.
Pin the actual implementation checkout and review its delta before writing.
