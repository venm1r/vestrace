-- Migration: 0024_policy_decisions_tickets_and_approval_grants.sql

CREATE TABLE IF NOT EXISTS authorization_tickets (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    operation_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'granted',
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE authorization_tickets ENABLE ROW LEVEL SECURITY;

CREATE POLICY authorization_tickets_workspace_isolation ON authorization_tickets
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
