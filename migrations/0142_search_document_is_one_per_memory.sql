-- Migration: 0142_search_document_is_one_per_memory.sql
--
-- Make the search index a projection with a key.
--
-- `search_documents` is the only thing the text retrieval channel reads, and
-- nothing ever wrote it: the table held zero rows in a database that had been
-- serving a console for weeks. Every retrieval returned nothing, successfully.
-- That could not be seen until a memory could be created at all, which
-- migration 0141 was the last piece of.
--
-- The index is a projection of a memory's **active** revision, so there is one
-- row per memory — not one per revision, and not one per write. The table had
-- an index on `(workspace_id, memory_id)` but no constraint, so nothing stopped
-- a second row appearing and nothing gave an upsert something to conflict on.
-- Both follow from stating the key.
--
-- Duplicates are collapsed before the constraint is added rather than assumed
-- absent. There are none today, but a migration that fails on data it did not
-- check is a migration that fails in the one deployment that matters.

DELETE FROM search_documents a
USING search_documents b
WHERE a.workspace_id = b.workspace_id
  AND a.memory_id = b.memory_id
  AND a.ctid < b.ctid;

ALTER TABLE search_documents
    DROP CONSTRAINT IF EXISTS uq_search_documents_workspace_memory;
ALTER TABLE search_documents
    ADD CONSTRAINT uq_search_documents_workspace_memory
        UNIQUE (workspace_id, memory_id);
