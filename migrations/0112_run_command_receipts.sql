-- Migration: 0112_run_command_receipts.sql

CREATE TABLE run_command_receipts (
    command_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    command_type TEXT NOT NULL,
    idempotency_key TEXT,
    request_payload JSONB NOT NULL,
    run_id UUID NOT NULL,
    result_version BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT run_command_receipts_command_type_non_empty
        CHECK (length(btrim(command_type)) > 0),
    CONSTRAINT run_command_receipts_idempotency_key_non_empty
        CHECK (idempotency_key IS NULL OR length(btrim(idempotency_key)) > 0),
    CONSTRAINT run_command_receipts_result_version_positive
        CHECK (result_version > 0),
    CONSTRAINT run_command_receipts_workspace_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES agent_runs (workspace_id, id)
        ON DELETE CASCADE
);

CREATE UNIQUE INDEX run_command_receipts_scoped_idempotency_key_idx
    ON run_command_receipts (
        workspace_id,
        principal_id,
        command_type,
        idempotency_key
    )
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX run_command_receipts_workspace_run_idx
    ON run_command_receipts (workspace_id, run_id);

ALTER TABLE run_command_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE run_command_receipts FORCE ROW LEVEL SECURITY;

CREATE POLICY run_command_receipts_workspace_isolation
    ON run_command_receipts
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
