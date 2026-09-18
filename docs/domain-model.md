# Domain model

**Type:** Concept guide, not an implementation checklist. See [status](status.md) and the [normative Domain Model](specs/en/vestrace-domain-model-v0.2.md).

| Concept | Meaning | Important boundary |
| --- | --- | --- |
| Event | Recorded source event | Observation is not automatically a verified fact |
| Memory | Stable identity of a unit of memory | Content changes must not erase earlier versions |
| MemoryRevision | A particular content version | Exact revision identity supports historical references |
| MemorySource / EvidenceRef | Provenance and supporting evidence | A source may be incomplete or wrong |
| Claim | Structured assertion | Assertion is not synonymous with truth |
| Conflict | Explicit disagreement | A later timestamp does not resolve semantic disagreement |
| ContextPack | Selected derived context | Inclusion does not raise source authority |
| Run | Canonical execution | A UI session or isolated tool call does not replace it |
| ExternalEffect | External operation and its evidence | Unknown outcome is not established failure |
| Capability | Bounded authority to act | Identity or role alone is insufficient |
| ContentMaterial | Governed content and lifecycle | Prepared is not Live |

## Status and versions

The reviewed Memory HTTP parser accepts `fact`, `preference`, `constraint`, `decision`, `task`, `procedure`, `observation`, `outcome`, and `summary`. The lifecycle includes `candidate`, `active`, `superseded`, `rejected`, `expired`, and `deleted`.

Active is a lifecycle state, not proof of truth. Confidence is a supplied score, not a calibrated probability. Content revision numbering and state_revision protect different dimensions; do not assume they are interchangeable.

Classification is separate from lifecycle and confidence. The current optional label follows [Memory API inheritance rules](reference/memory.md#classification); having a field does not prove universal enforcement.

## Time and provenance

Occurrence, recording, and validity answer different questions. A source can report an old event today. A later correction must not rewrite when the earlier conclusion became available. [Memory and time](design/memory-time.md) separates historical-content selection from reconstruction of past knowledge.

A memory-level source link does not establish attribution for a particular revision. Missing historical attribution must remain explicit rather than be reconstructed from nearby timestamps.

## Imports and editing

Source versions, local memory, and human corrections have different roles. Link rather than collapse them into one mutable record. A portable foreign_id preserves origin references without impersonating local actors or importing permissions.

MW remains Proposed. Accepted normative documents take precedence. Conflicts require an amended proposal or explicit architectural decision, not silent reinterpretation in a guide.

**Sources:** [Memory DTOs](../crates/vestrace-http/src/api/memory.rs), [repository](../crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [normative model](specs/en/vestrace-domain-model-v0.2.md).
