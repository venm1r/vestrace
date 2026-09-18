-- Migration: 0006_provenance_scopes_relations.sql

CREATE TABLE IF NOT EXISTS derivations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    method JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE derivations ENABLE ROW LEVEL SECURITY;

CREATE POLICY derivations_workspace_isolation ON derivations
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS memory_sources (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    derivation_id UUID REFERENCES derivations(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE memory_sources ENABLE ROW LEVEL SECURITY;

CREATE POLICY memory_sources_workspace_isolation ON memory_sources
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS knowledge_relations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_relation_distinct CHECK (source_memory_id <> target_memory_id)
);

ALTER TABLE knowledge_relations ENABLE ROW LEVEL SECURITY;

CREATE POLICY knowledge_relations_workspace_isolation ON knowledge_relations
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
