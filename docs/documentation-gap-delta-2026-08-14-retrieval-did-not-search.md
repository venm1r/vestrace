# Retrieval did not search

**Date:** 2026-08-14
**Scope:** the memory revision route, the domain error mapping, and the text
retrieval channel.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

This slice set out to close three small things the previous ones had recorded
and left. The third one turned into the largest finding since the row level
security work.

## The finding: the text channel returned everything, for everything

`PgTextRetriever` built its query like this:

```sql
SELECT ..., ts_rank(sd.fts_vector, plainto_tsquery('english', $2)) AS rank
FROM search_documents sd
INNER JOIN memories m ON m.id = sd.memory_id
INNER JOIN memory_revisions mr ON mr.id = m.active_revision_id ...
WHERE sd.workspace_id = $1
  AND m.workspace_id = $1
  AND m.status = ANY($3)
  AND (cardinality($4::text[]) = 0 OR m.kind = ANY($4))
ORDER BY rank DESC, mr.revision_number DESC
LIMIT $5
```

The query text reaches `ts_rank` in the SELECT list and **nothing in the
`WHERE`**. There is no match predicate. So every active memory in the workspace
was a candidate for every query, scored zero when it did not match, and the
`LIMIT` decided which ones a caller saw.

`search_documents.fts_vector` — the generated column that exists for this, with
an index on it — was read only to compute a score that changed nothing.

Live, before the fix, in a workspace holding three memories:

```text
query 'blue'                     3 candidates, scores [0.0205, -0.0798, -0.0802]
query 'zzzzqqqq nonsense term'   3 candidates, scores [0.0205, -0.0798, -0.0802]
```

The same three memories, in the same order, with the same scores, for a word
present in one of them and a word present in none. The scores are identical
because the fused score comes from reciprocal rank fusion over positions, and
the positions never varied.

Afterwards:

```text
query 'blue'                     1 candidate
query 'green'                    0 candidates
query 'zzzzqqqq nonsense'        0 candidates
query 'retrieval'                1 candidate
```

### Why nothing caught it

**`PgTextRetriever` had no test of any kind.** The retrieval tests that exist
run against in-memory doubles at the application layer — fusion, degradation,
context packs, the invalidation gap — and none of them touch this SQL. The RET
conformance cases pass identically with and without the predicate.

The previous slice's live check — "`POST /v1/retrieval/search` returns one
candidate" — was true and weaker than it looked. It returned one candidate
because the workspace held one memory, not because the query matched it. A
correct-looking result from a workspace with one row cannot distinguish a search
from a list.

`crates/vestrace-infrastructure/tests/text_retriever.rs` is the first test for
this adapter: five database-backed cases, three of which fail without the match
predicate (verified by removing it and re-running). The one that matters most is
`a_query_matching_nothing_returns_nothing`, because "returns nothing" was the
outcome the old code could never produce.

### The predicate is the expression that is ranked

Each temporal perspective now carries a `match_expression` alongside its
`rank_expression`, and they are the same text in every case: `sd.fts_vector` for
the current-revision perspective, which uses the index, and
`to_tsvector('english', mr.content)` for the historical perspectives, which must
match against the revision actually joined. Ranking one expression while
filtering another would let a row be admitted on a match its own score knows
nothing about.

## Revising a memory is now possible

`MemoryService::revise_memory` had existed since the memory slice with nothing
exposing it, so the only way a memory could change was not to. The previous
slice fixed a real defect in it — memory, revision, source and search document
written in one transaction — and could not verify the fix, because there was no
route to call.

`POST /v1/memories/{id}/revisions` exists now. The expected revision travels in
`If-Match`, as the run version does; putting it in the body would let a client
omit it and get last-write-wins silently, which for a memory means losing an
edit with no error.

Verified live:

```text
POST /v1/memories/{id}/revisions   If-Match: 1   201
POST again with If-Match: 1                      409  revision_conflict: expected 1, current 2
POST to a memory that does not exist             404  not_found
```

and in the database afterwards:

```text
revision_number | content          | change_reason
              1 | the sky is green |
              2 | the sky is blue  | corrected colour

active_revision_id = revision 2      true
search_documents.content             "the sky is blue"
memory_sources                       2
```

The search document moved in the same commit as the revision — which is the
property that could previously only be argued for.

## A domain error kept none of its meaning

Every `ApplicationError::Domain(_)` collapsed into `400 invalid_request`, which
flattened four distinct outcomes into one. Two of them matter:

- **A missing resource answered 400 on every write path** while the equivalent
  `GET` answered 404. The previous slice noticed this as a wart on a
  cross-tenant run cancel and left it; it is the same mapping.
- **A revision conflict answered 400**, so a caller could not tell "your copy is
  stale, re-read and retry" — the one outcome with an obvious next step — from
  "your request was malformed". The revision route above depends on the
  difference.

The mapping now uses the domain's own `code()` rather than a second spelling, so
the code a client matches on cannot drift from the variant it describes. Two
tests cover it; nothing had asserted the old behaviour, which is why it survived.

## `PgTextRetriever` scopes through `begin_scoped`

It was already correct — it opened a transaction and set
`vestrace.workspace_id` by hand — and it was the last adapter doing that by
hand. A second way to establish the scope is a second place for it to drift; the
hand-rolled version did not set the principal id, which no policy reads today
and one might.

## Test results

Full workspace suite against a live PostgreSQL 17: **847 passed, 0 failed, 130
suites, exit=0** (842 before this slice, across 129 suites).

New: five database-backed text retrieval tests, two error-mapping tests.

## What this does not close

`RET-011` is still open and this slice makes its shape clearer. Superseding a
memory does not propagate into previously issued context packs; what it *does*
now is stop the old text matching, because the index is a projection of the
active revision. A memory revised from "green" to "blue" is no longer found by
"green" — but a context pack issued before the revision still quotes the old
text, and nothing tells its holder otherwise.
