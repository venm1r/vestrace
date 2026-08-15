# Fusion had one channel

**Date:** 2026-08-14
**Scope:** embeddings, the vector retrieval channel, and the invariant that says
when the index is behind.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

`memory_embeddings` has been in the schema since migration 0009 and held **zero
rows**, because nothing ever wrote one. `RetrievalService::with_channels` has
taken an optional `SharedVectorRetriever` since it was written, and nothing ever
supplied one.

So reciprocal rank fusion — the step whose determinism has its own conformance
case, and whose non-determinism was a real defect found two weeks of slices ago —
has been fusing a single channel with itself. "Degraded because a channel failed"
could not arise, because there was only ever one channel to degrade.

## What the schema said versus what it meant

`memory_embeddings.embedding` was `vector(1536)` while `embedding_spaces` already
carried `dimensions` and `model`. Two statements about the same thing, and the
column won: any model not emitting exactly 1536 dimensions could not be stored,
which is most of them. The locally available `nomic-embed-text-v1.5` emits 768.

Migration `0148` makes the column dimension-free and the space authoritative,
which is what having an `embedding_spaces` table meant in the first place. The
constraint moved rather than disappeared — the adapter checks a vector against
its space's declared width before storing it, and refuses to write a second model
into an existing space:

> embedding space nomic-768 holds text-embedding-nomic-embed-text-v1.5 at 768
> dimensions and cannot accept … ; create a new space instead

Two geometries in one index would give distances that are arithmetically fine and
meaningless.

**No index yet, deliberately.** pgvector's ivfflat and hnsw need a fixed
dimension, so they belong per space; with zero rows stored an index would be a
guess about a workload nobody has run. It is the obvious next thing once a space
has enough rows to measure.

## Why embedding is not part of the memory write

Producing an embedding is a network call to a model. Doing it inside the
transaction that writes a memory would make **creating a memory fail whenever the
model is unreachable** — trading a capability the system has for an index it
would like.

So the write path is unchanged and embeddings are filled in by
`vestrace rebuild embeddings`, which until now never rebuilt an embedding and
said so. The gap that leaves is *stated*: `memory.active_memory_is_embedded` is a
registered invariant, so a workspace whose index is behind reports it in
`vestrace doctor` rather than returning quietly incomplete results. That is the
lesson from `search_documents`, which nothing wrote for months while every
retrieval succeeded and returned nothing.

## Live

Against LM Studio on the host (`text-embedding-nomic-embed-text-v1.5`, 768
dimensions), with the server reaching it through `host.docker.internal`:

```text
vestrace rebuild embeddings   →  3 embeddings rebuilt
embedding_spaces              →  nomic-768 | text-embedding-nomic-embed-text-v1.5 | 768
memory_embeddings             →  3 rows, width 768
```

Then a query sharing **no words** with any stored memory:

```text
POST /v1/retrieval/search {"query":"what colour is the firmament overhead"}
  3 candidates, top result revision 2 — the memory reading "the sky is blue"
```

and the retrieval journal, which records every configured channel and its
outcome:

```json
[{"channel":"text","outcome":"succeeded","candidates":0},
 {"channel":"vector","outcome":"succeeded","candidates":3}]
```

The text channel correctly returned nothing — it filters on the query now — while
the vector channel found the semantically related memory. A lexical query for
`"blue"` gives `text: 1, vector: 3`, so both channels contribute and fusion has
something to fuse for the first time.

## Details worth keeping

- **The score is inverted at the boundary.** pgvector's `<=>` is cosine
  *distance*, where lower is better; a `RetrievalCandidate` carries a *score*,
  where higher is better. Storing the distance in a field named score would leave
  fusion, ranking and the context pack all sorting the wrong way while looking
  correct.
- **Embedding results are sorted by their `index`.** The API permits results in
  any order and carries an index for exactly that reason. Trusting arrival order
  would attach each vector to the wrong memory, and every one of them would look
  plausible.
- **The provider's failure modes are distinguished.** Unreachable is
  `Unavailable`; a rejected credential and an unserved model are
  `InvalidConfiguration`; rate limiting says to retry later. The completion
  client had all of these collapsed into one error until a previous slice split
  them, and repeating that here would repeat the same page at 3am.
- **A workspace with no space returns empty rather than failing.** Nothing
  embedded yet is a truthful empty answer, and the journal still records the
  channel as consulted — which is how "nothing indexed" stays distinguishable
  from "not configured".

## What this does not do

The write path still does not embed, so a memory is searchable by text
immediately and by vector after a backfill. Closing that needs the job processor
that does not exist — the same missing drain the outbox has been waiting for
since it was found.

`RET-011` is unchanged and now has a second index to invalidate rather than one.

## Test results

Full workspace suite against a live PostgreSQL 17: **855 passed, 0 failed, 132
suites, exit=0**. The new code has no unit tests of its own yet; what it has is
the live evidence above, which is the part that could not have been faked by a
double.
