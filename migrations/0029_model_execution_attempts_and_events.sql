-- Migration: 0029_model_execution_attempts_and_events.sql

CREATE TABLE IF NOT EXISTS model_execution_attempts (
    id UUID PRIMARY KEY,
    model_execution_id UUID NOT NULL REFERENCES model_executions(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    attempt_number INT NOT NULL,
    provider_id UUID NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_model_execution_attempt_number UNIQUE (model_execution_id, attempt_number)
);

ALTER TABLE model_execution_attempts ENABLE ROW LEVEL SECURITY;

CREATE POLICY model_execution_attempts_workspace_isolation ON model_execution_attempts
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
