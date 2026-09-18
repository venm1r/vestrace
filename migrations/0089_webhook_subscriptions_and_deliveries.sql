-- Migration: 0089_webhook_subscriptions_and_deliveries.sql

CREATE TABLE IF NOT EXISTS webhook_subscriptions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    target_url TEXT NOT NULL,
    secret_token TEXT NOT NULL,
    events TEXT[] NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE webhook_subscriptions ENABLE ROW LEVEL SECURITY;

CREATE POLICY webhook_subscriptions_workspace_isolation ON webhook_subscriptions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
