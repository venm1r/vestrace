-- Migration: 0018_workflows_and_evaluations.sql

CREATE TABLE IF NOT EXISTS workflow_definitions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    current_revision INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE workflow_definitions ENABLE ROW LEVEL SECURITY;

CREATE POLICY workflow_definitions_workspace_isolation ON workflow_definitions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_workflow_definitions_workspace ON workflow_definitions(workspace_id);

CREATE TABLE IF NOT EXISTS workflow_revisions (
    revision_id UUID PRIMARY KEY,
    workflow_id UUID NOT NULL REFERENCES workflow_definitions(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL,
    definition JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workflow_id, revision_number)
);

ALTER TABLE workflow_revisions ENABLE ROW LEVEL SECURITY;

CREATE POLICY workflow_revisions_workspace_isolation ON workflow_revisions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_workflow_revisions_workflow ON workflow_revisions(workflow_id);

CREATE TABLE IF NOT EXISTS evaluations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    model_id UUID REFERENCES models(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    score DOUBLE PRECISION,
    summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE evaluations ENABLE ROW LEVEL SECURITY;

CREATE POLICY evaluations_workspace_isolation ON evaluations
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_evaluations_workspace ON evaluations(workspace_id);
CREATE INDEX idx_evaluations_model ON evaluations(model_id);
