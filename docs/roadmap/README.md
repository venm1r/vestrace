# Development roadmap

**Status:** Proposed product priorities, not an amendment to the frozen v1.0 program.
**Original review:** `07e2977a`; **editorial source:** supplied archive `3e05dfbd`.

## Direction

Develop Vestrace as governed long-term project memory: accessible through a small public
contract, correctable by users, and safely replenished from sources. Execution, security,
recovery, and protocols support that workflow rather than displace memory from the center.

The roadmap preserves Accepted ADRs, frozen specifications, and release gates. Memory
Workspace was integrated as documentation, not implemented by that integration. Later
Task 14E acceptance is recorded in [current status](../status.md); do not repeat already
accepted work or treat that task as completion of P04.

## Priorities

| Priority | Area | Initiatives | Decision rule |
| --- | --- | ---: | --- |
| P0 | [Foundation and trust blockers](p0-foundation.md) | 8 | Close production paths, isolation, and preservation before expanding claims. |
| P1 | [Governed project memory](p1-memory-workspace.md) | 9 | Read → correct → import → synchronize → transfer. |
| P2 | [Knowledge quality and integration](p2-knowledge-quality.md) | 7 | Improve useful, explainable outcomes on measured tasks. |
| P3 | [Complete approved platform](p3-full-platform.md) | 6 | Preserve every full-v1.0 obligation despite product sequencing. |
| P4 | [Expansion after validation](p4-expansion.md) | 4 | Add scope only when demand and operational readiness support it. |

**P0–P4 are priorities. P01–P12 are implementation packages.** A lower product priority
never removes AG-UI, A2A, workflows, or full qualification from the approved v1.0 contract.

## Reuse before adding mechanisms

The reviewed source already has the Rust workspace, PostgreSQL, memory identities/revisions,
CAS, scoped persistence, HTTP/MCP, route inventory and Bearer authentication, Run/outbox/worker,
material primitives, provider/embedding mechanisms, and Console. These are source observations,
not tests run for this edition. New APIs, importer, and UI must reuse them and close complete
production paths.

The current external gaps remain concrete: MemoryResponse does not return content/current
revision, ContextPackDto does not return rendered sections, and the memory component is not
a complete library/editor route. Closing those gaps is more useful than inventing another
orchestration layer. See [open gaps](../status/open-gaps.md).

## Milestones

| Milestone | Observable outcome | Entry and acceptance boundary |
| --- | --- | --- |
| M0 | Agreed baseline, gaps, and exact scope. | Source delta, owner decisions, documentation and test-infrastructure checks. |
| M1 | Verified preservation and recovery of data and operations. | Remaining P04 and P05 contracts; new consumers do not bypass guards. |
| M2 | Read content/history, correct it, and see the durable result in Console. | MW-00–MW-03; context readiness is separate from detail-read success. |
| M3 | Import → find → edit → sync conflict → resolve → export. | MW-04–MW-07, safety/upgrade gates, and the first bounded external pilot. |
| M4 | Better temporal/conflict/context outcomes and one useful external integration. | Choose P2 work from M2/M3 observations; consolidate only after evaluation. |
| M5 | All approved full-v1.0 workflows on a qualified environment. | P06–P12 and explicitly included extensions; independent release verdict. |
| M6 | Demand-supported team, hosting, connector, or tooling expansion. | New scope, support owner, measured operating cost. |

M4 and M5 are **separate branches**, not an unconditional sequence. P06–P12 can continue
after P05 without waiting for every P2 initiative. M0–M3 do not rename the full-v1.0 gates.

## Next sequence

First reconcile the source delta and authority boundaries, then finish the remaining P04
and installation/restore work under their existing contracts. After a separate scope review,
proceed to MW reads, atomic corrections, Console, then bounded import/sync/export.
Read-only design and an initial quality corpus may begin earlier; that does not establish
context or indexing readiness.

After the first complete workflow, choose the next feature from an observed blocker:
missing retrieval evidence, wrong temporal meaning, lost editorial work, difficult setup,
or integration demand. Do not add a feature solely because a competitor has a checkbox.

## Public alpha versus full v1.0

A proposed alpha has a narrow approved shipping manifest: one supported installation mode,
memory/edit/history, declared import formats, current context, and required safety/fault
checks. Start with synthetic or non-sensitive data in an isolated pilot. Production use
requires independently established operational readiness and authorization.

`0.1.0-alpha.1` is a suggested label, not an assigned release/date. Full v1.0 still requires
P12. MW remains a separate feature milestone unless explicitly included by program amendment.

No reliable calendar estimate follows from the number of files or unchecked tasks. Sequence
work from accepted vertical slices and available database/provider/browser environments.
Default to one active implementation package plus an independent documentation/evaluation task.

The [feature register](feature-register.json) is the editable source of the generated P0–P4
cards. All feature verification remains `NOT_RUN_HERE`. See [milestones](milestones.md),
[program mapping](program-mapping.md), [next actions](next-actions.md),
[risks](risks-and-decisions.md), and [adoption criteria](adoption.md).

**Sources:** [R09, R10, R11, R18, S01, S10, S11, S14](../maintenance/sources.md).
