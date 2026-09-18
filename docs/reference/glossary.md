# Project glossary

| Term | Meaning in this documentation |
| --- | --- |
| Memory | Stable identity of a long-lived record; not proof of truth. |
| MemoryRevision | Immutable content/metadata version of a memory. |
| Source | Provenance basis; its existence alone does not establish correctness. |
| Claim | An explicit assertion when a separate semantic model is needed. |
| Provenance | Links to sources and transformations, not automatic trust. |
| Canonical state | Authoritative state for its own domain. |
| Projection | Rebuildable derived representation that cannot rewrite its basis. |
| ContextPack | Governed context representation with constraints and provenance. |
| Classification label | An entry in a policy vocabulary; do not invent a severity ordering. |
| Sensitivity | A separate sensitivity dimension of content or a channel. |
| Capability | A bounded right to perform an operation under current policy. |
| Run | Canonical execution, distinct from a worker process. |
| Outbox | At-least-once work delivery, not another canonical event log. |
| Idempotency | Repeated attempts of one logical operation converge within the contract's scope. |
| CAS | Compare-and-swap: change state only if its expected version matches. |
| Content-addressed storage | Storage identified by byte content; also sometimes abbreviated CAS, but not compare-and-swap. |
| UNKNOWN | The outcome is not established; not permission to retry. |
| ResultPrepared | Durable preparation, not automatically Live or Succeeded. |
| Generation | Identity of a derived index state under the applicable contract. |
| Material | Content governed by lifecycle and key authorities. |
| B/I/M | Base imported source, Incoming source, and current effective Memory in synchronization. |
| Qualification | Acceptance of specified properties through evidence on an exact target. |
| P0–P4 | Proposed product-priority groups, not frozen package numbers. |
| P01–P12 | Existing full-v1 implementation/gate program. |
| MW-00–MW-07 | Proposed Memory Workspace program, not automatically P13 onward. |
| NOT_RUN_HERE | The relevant runtime check was not executed in this work. |

This glossary explains usage but does not replace exact types or normative contracts.
Applicable Accepted documents control when wording differs.

[Architecture](../architecture.md) · [Normative specifications](../specs/README.md)
