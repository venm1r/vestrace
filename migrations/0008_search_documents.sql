-- Migration: 0008_search_documents.sql

CREATE TABLE IF NOT EXISTS search_documents (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    fts_vector TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', title || ' ' || content)) STORED,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_search_documents_fts ON search_documents USING GIN (fts_vector);
CREATE INDEX idx_search_documents_trgm ON search_documents USING GIN (content gin_trgm_ops);

ALTER TABLE search_documents ENABLE ROW LEVEL SECURITY;

CREATE POLICY search_documents_workspace_isolation ON search_documents
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
