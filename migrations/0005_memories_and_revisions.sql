-- Migration: 0005_memories_and_revisions.sql

CREATE TABLE IF NOT EXISTS memories (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    active_revision_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_memories_workspace_status ON memories (workspace_id, status);

ALTER TABLE memories ENABLE ROW LEVEL SECURITY;

CREATE POLICY memories_workspace_isolation ON memories
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS memory_revisions (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    revision_number INT NOT NULL,
    content TEXT NOT NULL,
    structured JSONB,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    importance REAL NOT NULL CHECK (importance >= 0.0 AND importance <= 1.0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_memory_revision_number UNIQUE (memory_id, revision_number)
);

ALTER TABLE memory_revisions ENABLE ROW LEVEL SECURITY;

CREATE POLICY memory_revisions_workspace_isolation ON memory_revisions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

ALTER TABLE memories
    ADD CONSTRAINT fk_memories_active_revision
    FOREIGN KEY (active_revision_id) REFERENCES memory_revisions(id) ON DELETE SET NULL;
