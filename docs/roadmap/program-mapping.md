# Product priorities and existing programs

The roadmap cannot cancel an existing obligation, change the frozen package count, or assign
PASS. Keep the historical 36-PR transition, current P01–P12 program, and MW-00–MW-07 distinct.

| Package | Responsibility | Priority relationship | Required handling |
| --- | --- | --- | --- |
| P01 | Baseline and protocol locks. | P0/F006 | Reuse accepted locks and recheck applicability. |
| P02 | Security, material, and atomic authority. | P0/F002/F003/F008 | New consumers pass the same gates; no duplicate authority. |
| P03 | Connections, models, providers, effects. | Foundation; P3/F301 | No config-only execution or data-policy bypass. |
| P04 | Embedding, corpus, generation. | P0/F001; P2/F207 | Reuse 14E acceptance, but complete remaining P04 obligations separately. |
| P05 | Restore, installation, roles, G0. | P0/F004/F005 | Required operational boundary. |
| P06 | Real configuration, agents, Runs. | P3/F301 | Required for full v1.0. |
| P07 | Interaction, artifacts, retention. | P3/F302 | Not replaced by an importer runtime. |
| P08 | Complete pinned AG-UI. | P3/F303 | Acceptance through the pinned official client. |
| P09 | A2A server. | P3/F304 | Establish server compatibility first. |
| P10 | Outbound A2A. | P3/F304 | Then qualify external steps and ambiguous outcomes. |
| P11 | Remaining menu workflows. | P3/F305 | Real actions, not enabled stubs. |
| P12 | Exact-environment release. | P3/F306 | Fresh evidence and owner release decision. |

P05 depends on P02/P04. P06 follows P05, P07 follows P06, P08/P09 follow P07, P10 follows
P09, P11 follows P08/P10, and P12 follows P11. Product priorities do not alter that graph.

| MW package | Related initiatives | Outcome |
| --- | --- | --- |
| MW-00 | Baseline/scope for P1. | Proposed preflight requiring owner review. |
| MW-01 | F101/F102/F108 | Useful reads and context. |
| MW-02 | F003/F103 | Atomic corrections and replay. |
| MW-03 | F104 | Console library/editor. |
| MW-04 | F105, F007/F008 | Sources, preview, worker application. |
| MW-05 | F106 | Conflicts, cancellation, Missing. |
| MW-06 | F107 | Portability. |
| MW-07 | F109/F204, F005/F006 | Feature acceptance and upgrade, not P12. |

Keep MW as a separately named milestone until an explicit release-placement amendment.
A bounded alpha is not full v1.0. Inclusion of MW requires changes to the shipping manifest,
dependencies, and scope; adding documentation is not that amendment. New P2/P4 work requires
its own detailed specification rather than automatically becoming P13+.

The shared writer, current content checks, material staging/read paths, idempotency namespace,
lock order, and generation readiness retain their owners. Concurrent changes need a shared
accepted contract before consumer implementations. A successful Git merge cannot resolve
a semantic authority conflict.

[Roadmap](README.md) · [MW program](../implementation/memory-workspace/08-program.md) · [Sources](../maintenance/sources.md)
