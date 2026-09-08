# Memory Workspace implementation design

**Status:** Proposed extension, integrated into documentation but not implemented or accepted
for release by this package. **Original edition:** 0.1.1, 2026-09-07.
**Original source review:** `6f6102536e9a535b7086db14573bf45fe750ad71`.
**English editorial edition:** 2026-09-08, from supplied archive `3e05dfbd`.

## Goal

Deliver one coherent workflow: import a document → find/read it → inspect provenance →
correct it → retrieve useful context → synchronize an updated source without losing manual
changes → export authorized knowledge.

Console and importer share one canonical memory model and one mutation/audit/outbox boundary.
Neither UI, CLI, nor worker obtains an independent table-writing path. Source-document
history remains distinct from editorial history.

## Reading order

| Purpose | Documents |
| --- | --- |
| Establish dependencies and design choices | [Baseline/gaps](00-baseline.md), [decisions](01-design.md). |
| Implement storage and API contracts | [Data/transactions](02-data-and-transactions.md), [API](03-api.md), [JSON Schema](contracts/contracts.schema.json). |
| Build user workflows | [Console](04-console.md), [import/sync](05-import-sync.md), [portability](06-portability.md). |
| Enforce boundaries and sequence work | [Security/operations](07-security-operations.md), [program](08-program.md), [plans](plans/README.md). |
| Verify and hand off | [Acceptance](09-acceptance.md), [migrations](10-migrations.md), [handoff](11-handoff.md). |
| Resolve documentation authority | [Integration contract](12-integration.md). |

The [traceability register](traceability.json) contains 45 requirements, 23 tasks, and 54 cases.
The [file plan](file-plan.json) distinguishes proposed paths from earlier inspected sources;
[sources](sources.md) explains the original source manifest.

## Documentation is not implementation

New DTOs, routes, tables, commands, and tests are proposed. Accepting a specification does not
execute Rust, PostgreSQL, Console, or CLI tests. Original review statements apply to their
pinned sources—not automatically to the date of translation.

The original review observed 14D acceptance through ResultPrepared. The supplied archive
now records **Task 14E approval**, but its verdict still leaves P04/G0/v1.0 incomplete.
Read [current status](../../status.md) before planning. Reuse accepted implementation rather
than repeating it. Candidate migration 0196 is occupied and requires MW-00 reconciliation.

## Repository integration and exclusions

[Documentation](../../README.md) → [implementation packages](../README.md) → this package.
Architecture, normative, and planning indexes link here rather than keeping independent
copies. MW-D01–MW-D12 and exact implementation scope require separate acceptance. This
package does not silently extend the frozen P01–P12 release program.

No new generic runtime, LLM extractor, automatic consolidation, autonomous agent, vault,
arbitrary server-path reader, cloud connector, deletion inferred from missing files,
imported authority, or classification bypass belongs to the first version.

Original verification records concern their original editions. New editorial checks are
recorded in [maintenance](../../maintenance/english-validation-report.md), not converted into
product qualification. Unicode fixtures retain their original input bytes deliberately.
