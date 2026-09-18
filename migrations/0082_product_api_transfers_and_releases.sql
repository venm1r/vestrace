-- Migration: 0082_product_api_transfers_and_releases.sql

CREATE TABLE IF NOT EXISTS product_api_transfers (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    artifact_id UUID REFERENCES artifacts(id) ON DELETE CASCADE,
    transfer_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'initiated',
    byte_offset BIGINT NOT NULL DEFAULT 0,
    total_bytes BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE product_api_transfers ENABLE ROW LEVEL SECURITY;

CREATE POLICY product_api_transfers_workspace_isolation ON product_api_transfers
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
