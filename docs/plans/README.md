# Vestrace Planning Index

Planning documents describe intended work and acceptance criteria; they do not by themselves authorize implementation or prove runtime availability. Use the [documentation index](../README.md) to distinguish normative requirements, source-pinned observations and proposed extensions.

## Frozen v0.2 → v1.0 transition package

The historical 36-PR transition package remains intact:

- [PR specification index](v0.2-to-v1.0-pr-specification-index.md).
- [36-PR execution matrix](v0.2-to-v1.0-36-pr-execution-matrix.md).
- [Future PR review checklist](future-pr-review-checklist.md).
- [Consolidated implementation roadmap](../implementation-plan-v0.2.md).

Its original baseline and qualification claims are not advanced by registering a newer document.

## v1.0 P01–P12 gate program

Read the [twelve-package gate program](../superpowers/plans/2026-08-26-vestrace-v1-gate-program.md) together with its linked frozen specification and each detailed package plan. The program has its own dependencies, protected authorities and evidence gates. Its package numbers must not be confused with the 36 historical PR numbers.

The current repository also contains dated plans under `docs/superpowers/plans/`; the old archive-exclusion wording no longer describes this repository index. This navigation change does not rewrite or supersede those plans.

## Memory Workspace: proposed implementation extension

The selected API, Console and import/sync/export changes are registered separately:

| Document | Purpose |
| --- | --- |
| [Package entry](../implementation/memory-workspace/README.md) | Scope, source baseline and reading order |
| [Design and decisions](../implementation/memory-workspace/01-design.md) | Proposed decisions MW-D01–MW-D12 |
| [Integration contract](../implementation/memory-workspace/12-integration.md) | Precedence, overlap and release-program separation |
| [MW program](../implementation/memory-workspace/08-program.md) | MW-00–MW-07 dependencies and completion gates |
| [Package plans](../implementation/memory-workspace/plans/README.md) | 23 proposed implementation tasks |
| [MW-00 preflight](../implementation/memory-workspace/plans/00-preflight.md) | First entry point before product changes |
| [Acceptance catalog](../implementation/memory-workspace/09-acceptance.md) | Planned positive, negative, crash and race scenarios |

**Registration status:** documentation integration requested; technical decisions remain proposed and product implementation is not authorized by this index. MW-00–MW-07 are not P13–P20 and are not silently included in v1.0. Assigning them to a release requires an explicit program decision. Any conflict with active P04 work requires fresh scope coordination; the P04 allowlist is not expanded here.

## Precedence by question

For target architecture: Architecture Contract → newer Accepted ADR → specialized normative specifications/invariants → approved transition planning → proposed feature design → historical plan.

For current implementation: exact source/migrations and observed executable evidence → source-pinned implementation documentation → planning assumptions. An old PASS does not qualify a changed commit.

The Memory Workspace JSON Schema and OpenAPI files are proposed companions, not replacements for the runtime-exported schema or protocol lock. Changes to their contract must also update the feature specifications, examples and traceability map.
