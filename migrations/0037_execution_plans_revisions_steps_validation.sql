-- Migration: 0037_execution_plans_revisions_steps_validation.sql

CREATE TABLE IF NOT EXISTS execution_plans (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'direct',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE execution_plans ENABLE ROW LEVEL SECURITY;

CREATE POLICY execution_plans_workspace_isolation ON execution_plans
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS execution_plan_revisions (
    id UUID PRIMARY KEY,
    plan_id UUID NOT NULL REFERENCES execution_plans(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    revision_number INT NOT NULL,
    content_hash TEXT NOT NULL,
    steps JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_execution_plan_revision UNIQUE (plan_id, revision_number)
);

ALTER TABLE execution_plan_revisions ENABLE ROW LEVEL SECURITY;

CREATE POLICY execution_plan_revisions_workspace_isolation ON execution_plan_revisions
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
