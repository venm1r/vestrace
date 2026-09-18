# 12. Integration with the documentation system

**Original integration:** 0.1.1, 2026-09-07; source review `6f610253`.
**English editorial edition:** 2026-09-08 from supplied archive `3e05dfbd`.
**Scope:** Documentation and its checks—not product implementation.

## 12.1 One package location

The full package lives at docs/implementation/memory-workspace/. The [documentation index](../../README.md),
[extension index](../README.md), [architecture](../../architecture.md), [normative index](../../specs/README.md),
and [planning index](../../plans/README.md) lead here. Do not create independent copies under
specs/ or superpowers/specs/.

Monolithic Markdown/ZIP files are derived deliveries. Correct the package source files first.
source-manifest.json keeps original review pins/ranges/hashes; translated commentary does not
refresh those source observations.

## 12.2 Independent statuses

| Dimension | This package | Required next step |
| --- | --- | --- |
| Documentation placement | Integrated Proposed extension. | Ordinary editorial review/merge. |
| MW-D01–MW-D12 decisions | Proposed; identifiers retained. | Accept specific choices and compatibility gates. |
| Implementation | Not executed by this documentation task. | MW-00, exact scope, accepted predecessor interfaces. |
| Product qualification | Not claimed. | Fresh runtime/PostgreSQL/browser/fault evidence. |

Placement approval is not blanket authority for code, migrations, dependencies, or protected
P04 changes. MUST describes the proposed contract after acceptance.

## 12.3 Authority mapping

Architecture Contract, newer Accepted ADRs, and specialized normative specifications retain
precedence. MW either conforms or requires a named accepted amendment. An existing entity name
is not permission to create a second owner.

| Existing boundary | MW application | Specification |
| --- | --- | --- |
| Memory-first, canonical/derived (Architecture §3, Block 1) | UI/CLI read canonical memory; preview/indexes do not define truth. | [Design](01-design.md), [data](02-data-and-transactions.md). |
| Immutable revisions and occurrence/recording/validity (Blocks 1–2) | New correction revision; files cannot dictate fact time/trust. | [Data](02-data-and-transactions.md), [sync](05-import-sync.md). |
| Explicit mutation/conflict/reconciliation (Block 3) | Preserve source/manual versions and resolve B/I/M explicitly. | [Sync decision](contracts/sync-decision-contract.md), [MW-05](plans/05-sync.md). |
| Context provenance/destination/hard-token bound (Block 4) | Shared read gate; byte-only output has an explicitly narrower claim. | [API](03-api.md), [MW-01](plans/01-memory-read-context.md). |
| Capability/workspace authority (Blocks 6–7) | Trusted identity and current rights, no imported authority. | [Security](07-security-operations.md), [portability](06-portability.md). |
| Materials/erasure/disclosure | Existing ordinary materials for source staging/export. | [MW-04](plans/04-import.md), [MW-06](plans/06-portability.md). |
| Atomic audit/idempotency/outbox | Mutation and receipt commit together. | [MW-02](plans/02-atomic-corrections.md). |
| Single execution/recovery authority | Existing worker/outbox; no generic Task/Attempt replacement. | [Program](08-program.md), [import](05-import-sync.md). |
| Scoped qualification | MW-07 does not establish TRUSTED or close P12. | [Acceptance](09-acceptance.md), [MW-07](plans/07-qualification.md). |

The mapping links requirements; it does not prove these mechanisms are fully implemented.
Exact original sources and ranges remain in [sources](sources.md) and [manifest](source-manifest.json).

## 12.4 Existing programs and release placement

Historical 36-PR, P01–P12, and MW-00–MW-07 are separate. MW neither renumbers them nor creates
P13–P20. Release placement is unassigned until an explicit program amendment or separately
named milestone is approved with its dependencies/evidence.

Original 14D acceptance ended at ResultPrepared; current supplied evidence records 14E approval.
Neither implies full P04. Context/indexing still needs real generation readiness. Candidate
migration 0196 is occupied; reconcile all references in MW-00. Documentation reserves no numbers.

## 12.5 Cross-cutting constraints

HTTP, Console, and import share one writer with no CAS/audit/replay bypass. Reupload cannot
erase human corrections or edit source snapshots. Detail/history/context/export/diagnostics
apply current read policy; caches are not authorities. Vault/SQL recovery uses existing material
protocols. Applied and indexed remain distinct. Portable history transfers data annotations,
not actors, grants, qualification, or classification rights.

## 12.6 Compatibility decisions

Preserve preflight gates for ordinary-material owner/read support, current content policy,
idempotency namespace, shared lock order, qualified counters, and protected P04 overlap.
Integration alone closes none of them. Accept byte-only output explicitly; hard-token consumers
receive a qualified path or refusal, not an invisible fallback.

## 12.7 Editorial verification

Check the complete documentation diff, preserved requirement graph, single package location,
proposed/runtime schema separation, unchanged frozen authorities, and absence of credentials.
Original integration-result/report and validation-result/report remain historical observations,
not this translation's result. Current tests and limitations are recorded in the
[English validation report](../../maintenance/english-validation-report.md).
