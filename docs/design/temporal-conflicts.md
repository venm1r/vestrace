# Temporal knowledge and semantic conflicts

**Status:** Proposed functional design for F201/F202, not an accepted schema.

## Scope and query contract

Review existing TimePerspective, Claim/Conflict types, and normative temporal specifications first. Do not conflate current, valid-as-of, and recorded-as-known with the existing as_of API.

A query identifies its temporal question, time boundary, and permitted scope. Bind answers to exact source/memory revisions, validity, and recording time. When as-known history is insufficient, report unsupported/incomplete semantics rather than reconstructing it from latest timestamps.

Semantic conflicts concern type-compatible but substantively incompatible assertions. They differ from technical B/I/M sync conflicts. A shared UI may show both, but their resolution authorities remain distinct.

## Resolution

Domain policy may permit treating a source as a correction, retaining competing assertions, narrowing validity, or recording an authorized human decision. Preserve basis, actor, and expected state. Latest-wins is valid only as an explicitly accepted domain policy, never a universal heuristic.

A caller without source access must not receive revealing content or conflict details, including hidden names/counts. Reclassification is not ordinary editing.

## Invalidation

Source, revision, and policy changes trigger reconsideration of dependent context/summary projections without rewriting facts of past runs. Historical reads obey current access and retention. Do not reconstruct erased content from approximate summaries.

## Implementation after scope approval

Define query semantics and a minimal corpus; map existing temporal columns/indexes and missing historical facts; add only necessary backward-compatible query/mutation contracts; test exact revisions, late arrival, competing resolution, and revocation; then implement explanations and evaluate usefulness on the same corpus.

Do not start with automatic semantic merging. Users first need to see the conflict and its basis.

**Sources:** [temporal model](../specs/en/vestrace-data-temporal-model-v0.2.md), [hydrator](../../crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs), [retrieval](../../crates/vestrace-http/src/api/retrieval.rs).
