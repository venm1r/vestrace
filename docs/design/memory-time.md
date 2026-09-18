# Memory, provenance, and time

## Identity and revisions

Stable Memory identity names a unit of knowledge; revision identity names the content actually used. A correction creates another revision. Moving the active pointer does not erase history.

An exact historical read must load the requested revision. The reviewed PgRevisionHydrator has no latest-version fallback and checks the memory/revision pair after loading. This is a static code observation, not qualification of every historical query.

## Attribute the right object

Knowing a memory once had a source is insufficient to establish which source supports a particular revision. The current memory_sources INSERT has no explicit revision_id. MW must record exact revision/source links where it promises precise explanations. Old links must not be guessed from timestamp proximity.

Show unresolved attribution as a limitation. This matters when combining authors or reimporting documents.

## Three time questions

| Question | Meaning |
| --- | --- |
| What applies now? | Current selection |
| What applied on September 1? | Validity at that time |
| What did the system know on September 1? | Records available to the system then |

These may yield the same text but are not interchangeable. HTTP accepts `current`, `as_of`, `timeline`, and `all_history`; enum presence alone does not prove full bitemporal behavior. Examples must not transfer a target temporal capability to a current endpoint without verification.

## Correction, supersession, and deletion

An error correction, a newly adopted decision, and physical deletion are different operations. Corrections preserve history; supersession changes currency; deletion follows separate retention and permission rules. Revoked source access does not leave previous answers unconditionally readable.

Reproducibility is bounded by lawfully retained data and current read policy. Do not promise both permanent reproduction of every text and irreversible erasure of those texts.

## Verification

Test late-arriving records, competing revisions, missing sources, exact historical references, deletion, and revocation. Compare IDs and content, not merely search-result counts. See [MW acceptance](../implementation/memory-workspace/09-acceptance.md).

**Sources:** [temporal contract](../specs/en/vestrace-data-temporal-model-v0.2.md), [hydrator](../../crates/vestrace-infrastructure/src/postgres/revision_hydrator.rs), [memory repository](../../crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [retrieval](../../crates/vestrace-http/src/api/retrieval.rs).
