-- Migration: 0017_external_execution_history.sql

CREATE TABLE IF NOT EXISTS workflow_executions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    workflow_id UUID NOT NULL,
    workflow_revision INTEGER NOT NULL,
    workflow_revision_id UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    attempt INTEGER NOT NULL DEFAULT 1,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    correlation_id TEXT,
    causation_id TEXT
);

ALTER TABLE workflow_executions ENABLE ROW LEVEL SECURITY;

CREATE POLICY workflow_executions_workspace_isolation ON workflow_executions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_workflow_executions_workspace ON workflow_executions(workspace_id);
CREATE INDEX idx_workflow_executions_workflow ON workflow_executions(workflow_id);
CREATE INDEX idx_workflow_executions_status ON workflow_executions(status);

CREATE TABLE IF NOT EXISTS step_executions (
    id UUID PRIMARY KEY,
    workflow_execution_id UUID NOT NULL REFERENCES workflow_executions(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    node_id UUID NOT NULL,
    node_label TEXT NOT NULL,
    kind TEXT NOT NULL,
    agent_ref UUID,
    attempt INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    input_artifact_id UUID,
    output_artifact_id UUID,
    model_attempt_id UUID,
    tool_invocation_id UUID,
    error_message TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    UNIQUE (workflow_execution_id, node_id, attempt)
);

ALTER TABLE step_executions ENABLE ROW LEVEL SECURITY;

CREATE POLICY step_executions_workspace_isolation ON step_executions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_step_executions_workflow_execution ON step_executions(workflow_execution_id);
CREATE INDEX idx_step_executions_workspace ON step_executions(workspace_id);
CREATE INDEX idx_step_executions_status ON step_executions(status);

CREATE TABLE IF NOT EXISTS execution_artifacts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    step_execution_id UUID NOT NULL REFERENCES step_executions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    content_ref TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE execution_artifacts ENABLE ROW LEVEL SECURITY;

CREATE POLICY execution_artifacts_workspace_isolation ON execution_artifacts
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_execution_artifacts_step ON execution_artifacts(step_execution_id);
CREATE INDEX idx_execution_artifacts_workspace ON execution_artifacts(workspace_id);

CREATE TABLE IF NOT EXISTS execution_outcomes (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    workflow_execution_id UUID NOT NULL REFERENCES workflow_executions(id) ON DELETE CASCADE,
    step_execution_id UUID REFERENCES step_executions(id) ON DELETE SET NULL,
    outcome_kind TEXT NOT NULL,
    summary TEXT NOT NULL,
    error_code TEXT,
    error_detail TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE execution_outcomes ENABLE ROW LEVEL SECURITY;

CREATE POLICY execution_outcomes_workspace_isolation ON execution_outcomes
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_execution_outcomes_workflow_execution ON execution_outcomes(workflow_execution_id);
CREATE INDEX idx_execution_outcomes_workspace ON execution_outcomes(workspace_id);
