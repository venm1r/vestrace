-- Migration: 0061_agent_packages_and_extension_registry.sql

CREATE TABLE IF NOT EXISTS agent_packages (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    publisher TEXT NOT NULL,
    manifest JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_agent_package_version UNIQUE (workspace_id, name, version)
);

ALTER TABLE agent_packages ENABLE ROW LEVEL SECURITY;

CREATE POLICY agent_packages_workspace_isolation ON agent_packages
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
