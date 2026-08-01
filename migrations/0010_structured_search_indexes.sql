-- Migration: 0010_structured_search_indexes.sql

CREATE TABLE IF NOT EXISTS structured_search_indexes (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    entity_value TEXT NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_structured_search_entity ON structured_search_indexes (workspace_id, entity_type, entity_value);

ALTER TABLE structured_search_indexes ENABLE ROW LEVEL SECURITY;

CREATE POLICY structured_search_indexes_workspace_isolation ON structured_search_indexes
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
