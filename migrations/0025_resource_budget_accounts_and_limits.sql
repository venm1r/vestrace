-- Migration: 0025_resource_budget_accounts_and_limits.sql

CREATE TABLE IF NOT EXISTS budget_accounts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    account_name TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    hard_limit REAL NOT NULL CHECK (hard_limit >= 0),
    balance REAL NOT NULL DEFAULT 0 CHECK (balance >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE budget_accounts ENABLE ROW LEVEL SECURITY;

CREATE POLICY budget_accounts_workspace_isolation ON budget_accounts
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
