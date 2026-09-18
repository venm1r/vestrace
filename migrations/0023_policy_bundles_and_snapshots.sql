-- Migration: 0023_policy_bundles_and_snapshots.sql

CREATE TABLE IF NOT EXISTS policy_bundles (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    version INT NOT NULL DEFAULT 1,
    rules JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_policy_bundle_version UNIQUE (workspace_id, name, version)
);

ALTER TABLE policy_bundles ENABLE ROW LEVEL SECURITY;

CREATE POLICY policy_bundles_workspace_isolation ON policy_bundles
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
