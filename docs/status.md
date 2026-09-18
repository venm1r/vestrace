# Implementation status and evidence boundaries

**Scope:** Supplied archive `3e05dfbdce063aa44a3a9e5a7a84c274597e8188`.
**Method:** Selected source inspection and reading existing evidence; no runtime execution.
The ZIP comment identifies the commit. Hash comparison checks which earlier observations
still concern identical source bytes; it is not a full repository audit.

## Read the status dimensions separately

| Label | Meaning | Does not establish |
| --- | --- | --- |
| Source present | The relevant definition/implementation is in the inspected source. | A complete runtime path or supported deployment. |
| Limited surface | An existing interface exposes only part of the intended workflow. | Absence of useful internal components. |
| Recorded acceptance | An existing evidence record reports acceptance at its named boundary. | A new test run by the author of this edition. |
| Proposed | A documented extension awaits explicit acceptance and implementation. | An available endpoint, CLI command, or release feature. |
| Not established | The available review does not establish the broader claim. | Proof that the feature cannot exist elsewhere. |

## Important update: Task 14E is recorded as approved

The supplied [P04 evidence](development-evidence/v1-g0-04-embedding-transition-foundation.md)
ends with **“Task 14E final lead acceptance - APPROVED.”** It records the final independent
read-only review, the 17-test finalization suite, and the amended RED→restore→GREEN checks.
This edition **read that record; it did not rerun those tests**.

The approval closes **Task 14E only**. The rotation-before-adoption deferral remains binding;
**P04, G0, and full v1.0 remain incomplete according to that same record**. Earlier guides
ending at 14D/ResultPrepared must not continue to describe publication as merely proposed.
Nor may the newer task approval be expanded into full embedding/retrieval or release qualification.

## Current memory-first boundaries

| Area | Observed boundary | Next work / evidence |
| --- | --- | --- |
| HTTP identity | Bearer authentication resolves principal/workspace and replaces caller identity headers. | Verify each actual deployment/token/policy combination. |
| Route policy | Inventory declares capability/exposure/risk. | Read the handler and composition; an inventory entry is not completion. |
| Memory persistence | Repository commits memory, revision, source, and search projection with CAS. | Service outbox/idempotency persistence is separate; MW-02 proposes whole-command atomicity. |
| HTTP memory read | Current MemoryResponse contains metadata, not content/current content-revision number. | Proposed MW-01 detail/history/browse. |
| MCP memory read | The reviewed get_memory returns a metadata view. | Reuse the authorized query service for useful detail. |
| HTTP context | ContextPackDto exposes counters/summary, not rendered sections. | Proposed actual-content response and explicit budget semantics. |
| Token accounting | Existing helper estimates ceil(UTF-8 bytes / 4). | A hard token claim requires a qualified counter; no implicit fallback. |
| Exact revision hydration | The reviewed hydrator reads the requested revision without a latest fallback. | Validate full temporal/authorization scenarios separately. |
| Console memory | A presentation component exists; the reviewed router lacks the full library/editor workflow. | Wire real APIs and browser acceptance, not a second Console. |
| Import/sync/portability | The MW package defines a proposed bounded workflow. | MW-00 scope and dependencies, then implementation and feature acceptance. |
| Materials | Existing content/key lifecycle mechanisms are present. | Each new staging/read/export owner needs compatibility and recovery evidence. |
| Release | Frozen P01–P12 program remains authoritative. | Fresh P12 evidence on the exact shipping target. |

The [machine-readable capability register](status/capabilities.json) is an editorial summary,
not a test report. Original source records remain pinned; [source deltas](maintenance/source-delta.json)
identify changed and unchanged inspected files.

## Migration planning collision

The archive contains `migrations/0196_retired_credential_erasure.sql`. The MW proposal also
uses `0196_memory_workspace_receipts.sql` as a candidate name. That candidate is **not free**.
MW-00 must reassign the candidate consistently across the file plan, contracts, task plans,
and tests before implementation. This documentation does not reserve a replacement number
or alter any SQL migration.

## What was not tested here

Rust build/tests/doctests, PostgreSQL grants/races/migrations, HTTP/MCP/worker execution,
browser interoperability, real model calls, import/export, and backup/restore were not run.
Documentation checks have their own [validation report](maintenance/english-validation-report.md).
Do not use their PASS to close any product gate.

[Open gaps](status/open-gaps.md) · [Roadmap](roadmap/README.md) · [Sources](maintenance/sources.md)
