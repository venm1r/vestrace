# Walkthrough: event, memory, and retrieval

**Scope:** Source-defined current request shapes. This walkthrough was not executed during the refactor. Use synthetic data in a disposable workspace with configured server/policy and EventWrite, MemoryWrite, MemoryRead, and ContextRetrieve capabilities.

All requests below use Bearer authentication; mutations use a distinct stable Idempotency-Key per logical operation. See [HTTP conventions](../reference/http.md). These requests create durable data.

## 1. Record the source event

Send `POST /v1/events`:

```json
{
  "event_type": "documentation.example",
  "payload": {"statement": "The tutorial project selected PostgreSQL."},
  "session_id": null
}
```

Keep the returned id. It identifies provenance, not independent proof that the statement is true.

## 2. Create memory

Send `POST /v1/memories` with a different stable idempotency key:

```json
{
  "kind": "decision",
  "content": "Tutorial decision: use PostgreSQL.",
  "confidence": 0.5,
  "importance": 0.5,
  "source_event_id": "10000000-0000-4000-8000-000000000003",
  "evidence_role": "direct_source"
}
```

**Replace the synthetic UUID with the actual event ID from step 1.** Scores are example assessments, not measured probabilities. Supply classification only according to the installation's vocabulary. Successful persistence does not establish index/generation readiness.

## 3. Read metadata

`GET /v1/memories/{id}` returns id, kind, status, classification, created_at, and updated_at. The reviewed DTO does not include content or active content-revision number. Do not infer response fields from an internal domain object. Full external detail/history is proposed in MW-01.

## 4. Search

Send `POST /v1/retrieval/search`:

```json
{
  "query": "Which database did the tutorial project select?",
  "intent": "decision_recall",
  "limit": 10,
  "time_perspective": "current",
  "token_budget": 512
}
```

Inspect candidates, revision references, warnings, and degraded channels. The current HTTP ContextPack is a summary, not rendered model input. An empty result does not prove absence: check scope, lifecycle, and channel availability.

## 5. Add a revision

Record another source event first. `POST /v1/memories/{id}/revisions` requires numeric If-Match for the expected content-revision number and a revision body:

```json
{
  "content": "Corrected tutorial decision: use PostgreSQL with pgvector.",
  "confidence": 0.5,
  "importance": 0.5,
  "source_event_id": "10000000-0000-4000-8000-000000000004",
  "change_reason": "Record the corrected tutorial decision."
}
```

Replace this UUID with the new event ID. Omit classification to inherit; `null` requests clearing and is not equivalent. See [classification](../reference/memory.md#classification).

Do not guess a revision number on shared data. An isolated newly created example starts at content revision 1, but a concurrent writer can advance it. Handle a conflict rather than bypassing the precondition. A convenient full read/edit loop remains MW work.

## Record results safely

Record source/environment identities and sanitized request IDs/outcomes, not tokens, credential URLs, or user content. One walkthrough does not replace concurrency, restart, authorization, or release testing.

**Sources:** [event and memory handlers](../../crates/vestrace-http/src/api/memory.rs), [memory handlers](../../crates/vestrace-http/src/api/memory.rs), [retrieval](../../crates/vestrace-http/src/api/retrieval.rs).
