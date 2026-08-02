-- Migration: 0088_event_schemas_exports_and_capture_profiles.sql

CREATE TABLE IF NOT EXISTS run_exports (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    signature TEXT NOT NULL,
    profile_name TEXT NOT NULL DEFAULT 'Operational',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE run_exports ENABLE ROW LEVEL SECURITY;

CREATE POLICY run_exports_workspace_isolation ON run_exports
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
