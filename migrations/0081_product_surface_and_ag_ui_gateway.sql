-- Migration: 0081_product_surface_and_ag_ui_gateway.sql

CREATE TABLE IF NOT EXISTS product_releases (
    id UUID PRIMARY KEY,
    version TEXT NOT NULL UNIQUE,
    manifest JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS interaction_sessions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id UUID REFERENCES agent_runs(id) ON DELETE CASCADE,
    client_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE interaction_sessions ENABLE ROW LEVEL SECURITY;

CREATE POLICY interaction_sessions_workspace_isolation ON interaction_sessions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
