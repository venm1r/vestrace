-- Migration: 0032_tool_definitions_and_invocations.sql

CREATE TABLE IF NOT EXISTS tool_definitions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    binding_kind TEXT NOT NULL,
    parameters_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE tool_definitions ENABLE ROW LEVEL SECURITY;

CREATE POLICY tool_definitions_workspace_isolation ON tool_definitions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS tool_invocations (
    id UUID PRIMARY KEY,
    tool_id UUID NOT NULL REFERENCES tool_definitions(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    arguments JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'prepared',
    output JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE tool_invocations ENABLE ROW LEVEL SECURITY;

CREATE POLICY tool_invocations_workspace_isolation ON tool_invocations
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
