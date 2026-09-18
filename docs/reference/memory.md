# Memory API and revisions

**Scope:** Current source-defined HTTP behavior. No runtime execution is claimed.

| Method and path | Purpose | Boundary |
| --- | --- | --- |
| POST `/v1/events` | Record a source event | An arbitrary payload is not automatically confirmed knowledge. |
| POST `/v1/memories` | Create memory with a source | kind/content/confidence/importance/source_event_id/evidence_role and optional classification. |
| GET `/v1/memories/{id}` | Read metadata | id/kind/status/classification/created_at/updated_at; no content/history. |
| POST `/v1/memories/{id}/revisions` | Create a content revision | Expected content revision is passed as numeric `If-Match`. |
| DELETE `/v1/memories/{id}` | Hard purge through separate authority | Critical risk, MemoryPurge, and operation-specific reason/approval; not a hide button. |

The exact optional fields and errors are defined in [api/memory.rs](../../crates/vestrace-http/src/api/memory.rs), not this condensed table.

## Content and provenance

Memory owns stable identity and an active revision pointer. MemoryRevision carries content and temporal metadata. A read must not combine a memory with an unrelated revision. Provenance records where an assertion came from; it does not prove truth.

The current memory_sources links are memory-level. MW proposes exact new revision/source links. Do not assign historical links to particular revisions when that relationship was never recorded.

## Writes and concurrency

Repository CAS compares state/content versions; revision updates are not unconditional overwrites. The service subsequently writes outbox/idempotency separately. [MW-02](../implementation/memory-workspace/plans/02-atomic-corrections.md) strengthens the overall transaction boundary rather than replacing absent CAS.

Restoring earlier content creates a new revision. Validity, created_at, and revision number are separate dimensions. A missing event time must not be inferred from a local file's mtime.

## Classification

Creation accepts labels from the installation's vocabulary. On revision, an **omitted** field inherits the current label; an identical explicit label is allowed. Explicit `null` requests clearing and is not equivalent to omission. Changes or clearing are rejected by the current ordinary revision path because a separate classification transition is not implemented there.

Keep classification read-only in the ordinary editor. Do not infer severity ordering from arbitrary label names, spelling, or alphabetic order.

## Proposed next surface

Detail, history, browse, corrections/restore, import, and portable export belong to the [MW API design](../implementation/memory-workspace/03-api.md), with separate acceptance. They are not existing endpoints simply because a schema describes them.

**Sources:** [use cases](../../crates/vestrace-application/src/memory/mod.rs), [service](../../crates/vestrace-application/src/memory/services.rs), [repository](../../crates/vestrace-infrastructure/src/postgres/memory_repository.rs), [HTTP](../../crates/vestrace-http/src/api/memory.rs).
