-- Migration: 0087_workspace_envelope_encryption_and_cross_sharing.sql

CREATE TABLE IF NOT EXISTS workspace_keks (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    key_alias TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'AeadAes256GcmV1',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE workspace_keks ENABLE ROW LEVEL SECURITY;

CREATE POLICY workspace_keks_workspace_isolation ON workspace_keks
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS cross_workspace_memory_grants (
    id UUID PRIMARY KEY,
    owner_workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    target_workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    memory_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE cross_workspace_memory_grants ENABLE ROW LEVEL SECURITY;

CREATE POLICY cross_workspace_grants_owner_isolation ON cross_workspace_memory_grants
    FOR ALL
    USING (owner_workspace_id = vestrace_current_workspace_id() OR target_workspace_id = vestrace_current_workspace_id());
