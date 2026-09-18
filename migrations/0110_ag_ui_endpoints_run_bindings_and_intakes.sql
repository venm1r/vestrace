-- Migration: 0110_ag_ui_endpoints_run_bindings_and_intakes.sql

CREATE TABLE IF NOT EXISTS ag_ui_endpoints (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE ag_ui_endpoints ENABLE ROW LEVEL SECURITY;

CREATE POLICY ag_ui_endpoints_workspace_isolation ON ag_ui_endpoints
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
