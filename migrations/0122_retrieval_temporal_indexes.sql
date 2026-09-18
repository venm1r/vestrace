CREATE INDEX IF NOT EXISTS idx_memory_revisions_temporal_lookup
    ON memory_revisions (workspace_id, memory_id, valid_from, valid_until, revision_number);

CREATE INDEX IF NOT EXISTS idx_search_documents_workspace_memory
    ON search_documents (workspace_id, memory_id);
