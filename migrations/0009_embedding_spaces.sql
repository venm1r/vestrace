-- Migration: 0009_embedding_spaces.sql

CREATE TABLE IF NOT EXISTS embedding_spaces (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    dimensions INT NOT NULL,
    model TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE embedding_spaces ENABLE ROW LEVEL SECURITY;

CREATE POLICY embedding_spaces_workspace_isolation ON embedding_spaces
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS memory_embeddings (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    space_id UUID NOT NULL REFERENCES embedding_spaces(id) ON DELETE CASCADE,
    embedding VECTOR(1536),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE memory_embeddings ENABLE ROW LEVEL SECURITY;

CREATE POLICY memory_embeddings_workspace_isolation ON memory_embeddings
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
