-- Migration: 0062_a2a_interoperability_gateway.sql

CREATE TABLE IF NOT EXISTS remote_agent_invocations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    target_agent_url TEXT NOT NULL,
    protocol_version TEXT NOT NULL DEFAULT 'v1',
    status TEXT NOT NULL DEFAULT 'initiated',
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE remote_agent_invocations ENABLE ROW LEVEL SECURITY;

CREATE POLICY remote_agent_invocations_workspace_isolation ON remote_agent_invocations
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
