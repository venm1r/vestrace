-- Migration: 0063_observability_and_metric_rollups.sql

CREATE TABLE IF NOT EXISTS metric_rollups (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE metric_rollups ENABLE ROW LEVEL SECURITY;

CREATE POLICY metric_rollups_workspace_isolation ON metric_rollups
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
