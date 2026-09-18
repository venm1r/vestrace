# Architecture

**Type:** Explanation. [Normative contracts and Accepted ADRs](specs/README.md) remain authoritative; [status](status.md) describes implementation boundaries.

## Shared persistent knowledge

Vestrace stores long-lived knowledge and governs its use. Memory, source events, revisions, relationships, and authority do not belong to a transient model session. HTTP, MCP, and Console expose one model rather than independently maintained copies of truth.

| Layer | Responsibility | Must not own |
| --- | --- | --- |
| Domain | Entities, valid transitions, identity, and evidence types | SQL, transport, or secret plaintext in ordinary DTOs |
| Application | Use cases, ports, policy, and effect coordination | HTTP/React details or client-local storage semantics |
| Infrastructure | PostgreSQL, vault, provider transport, and adapters | Undeclared authority or competing business rules |
| HTTP / MCP / CLI | Authenticated entry points, request types, process composition | Bypasses around shared mutation and policy contracts |
| Console | User-facing views and commands | Direct database writes or locally assigned trust/status |

This is a responsibility map, not a claim that every target mechanism is implemented.

## Canonical records and derived views

A source event records provenance. Memory preserves identity; MemoryRevision preserves particular content. Claims and conflicts represent assertions and disagreements under their contracts. Embeddings, search documents, summaries, and ContextPacks are derived views.

A derived view can be rebuilt from retained, lawfully available inputs. It cannot unilaterally correct more authoritative data. A source reference explains origin; it does not establish truth.

## One execution authority

Run is canonical execution. Adapters translate commands and observations at the boundary. A retry handler, importer, or Console must not create another generic Task/Attempt runtime. Specialized operation state is appropriate when it records a domain outcome rather than competing with execution authority.

Read [memory and time](design/memory-time.md), [retrieval](design/retrieval-context.md), [transactions](design/transactions.md), [external effects](design/execution.md), and [materials](design/materials.md).

## Brain–Face–Organ

The accepted system-level decomposition separates persistent cognition, active reasoning, a user/host interface, and replaceable execution endpoints. Its existence in an ADR does not establish implementation of a separate Brain runtime, Host Broker, or Organ. Adapters must not acquire independent authority or authoritative history. See [ADR-0011](adr/0011-brain-face-organ-system-decomposition.md) and the [system model](specs/en/vestrace-brain-face-organ-system-model.md).

## Memory Workspace

[Memory Workspace](implementation/memory-workspace/README.md) proposes accessible reads, editing, import, sync, and portability around the existing model. A manual edit does not replace a source snapshot. All writers share a mutation boundary; export checks current authorization. Its requirements live in one package, not competing copies across architecture and roadmap pages.

**Sources:** [Architecture Contract](specs/en/vestrace-architecture-contract-v0.2.md), [Domain Model](specs/en/vestrace-domain-model-v0.2.md), and [source register](maintenance/sources.md).
