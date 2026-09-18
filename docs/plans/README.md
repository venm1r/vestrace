# Plans, dependencies, and active obligations

| Program | Purpose | Do not infer |
| --- | --- | --- |
| [Historical 36-PR transition](v0.2-to-v1.0-pr-specification-index.md) | Requirements traceability from the original v0.2 baseline. | That its numbers describe the present work queue. |
| [Frozen P01–P12](../superpowers/plans/2026-08-26-vestrace-v1-gate-program.md) | Approved full-v1 delivery program. | That every historical stage is accepted on today's code. |
| [MW-00–MW-07](../implementation/memory-workspace/08-program.md) | Proposed memory/context API, Console, import/sync/export extension. | Automatic v1 inclusion or permission to alter protected P04 scope. |

The [roadmap](../roadmap/README.md) uses product priorities **P0–P4**, not implementation
package numbers **P01–P12**. P08 can have priority P3 and still be mandatory for full v1.0.

P04 continuation follows its own accepted contract. P05 depends on P02/P04; P06 follows P05,
P07 follows P06, P08/P09 follow P07, P10 follows P09, P11 follows P08/P10, and P12 follows P11.
The roadmap does not change this graph.

MW starts with a separate preflight and overlap review. Read-only work may proceed before
full P04 closure, but context/import readiness cannot bypass underlying guards. Shared writer
or migration changes need an explicit sequence rather than optimistic merging.

The owner must either select a separate milestone or explicitly amend the frozen program.
Until then, roadmap and later designs remain proposals. Changed premises require delta,
scope, and acceptance review. Neither historical plans nor this index independently authorizes
commit, push, or deployment.

[Implementation extensions](../implementation/README.md) · [Current status](../status.md)
