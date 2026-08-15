-- Migration: 0148_embeddings_have_a_space.sql
--
-- Let an embedding be the shape its model produces.
--
-- # What was wrong
--
-- `memory_embeddings.embedding` was `vector(1536)` while `embedding_spaces`
-- already carried `dimensions` and `model` — so the schema said two different
-- things about how wide a vector is, and the column won. Any model that does not
-- emit exactly 1536 dimensions could not be stored at all, which is most of
-- them: the locally available `nomic-embed-text-v1.5` emits 768.
--
-- The column is dimension-free now and the space is the authority, which is what
-- having an `embedding_spaces` table meant in the first place. The adapter
-- checks a vector against its space's declared width before storing it, so the
-- constraint moves rather than disappears.
--
-- # The index that is deliberately absent
--
-- pgvector's ivfflat and hnsw indexes require a fixed dimension, so they belong
-- per space rather than on this column. There is no index here yet and none is
-- added: with no embeddings stored at all, an index would be a guess about a
-- workload nobody has run. It is the obvious next thing once a space has enough
-- rows to measure.
--
-- # Identity
--
-- One embedding per memory per space. Without the constraint, re-embedding a
-- memory would accumulate copies and the vector channel would return the same
-- memory several times over, each from a different generation of the model.

ALTER TABLE memory_embeddings ALTER COLUMN embedding TYPE vector;

ALTER TABLE memory_embeddings
    ADD CONSTRAINT memory_embeddings_one_per_space UNIQUE (workspace_id, memory_id, space_id);

-- Both tables enable row level security without forcing it, so the runtime role
-- that owns them is exempt from their own workspace policy. Their adapters are
-- written scoped in this slice, so they can be forced with the rest.
ALTER TABLE memory_embeddings FORCE ROW LEVEL SECURITY;
ALTER TABLE embedding_spaces FORCE ROW LEVEL SECURITY;
