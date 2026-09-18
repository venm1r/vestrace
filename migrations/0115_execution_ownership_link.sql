ALTER TABLE workflow_executions
    ADD COLUMN IF NOT EXISTS run_id UUID REFERENCES agent_runs(id) ON DELETE SET NULL;

ALTER TABLE step_executions
    ADD COLUMN IF NOT EXISTS run_id UUID REFERENCES agent_runs(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_workflow_executions_run ON workflow_executions(run_id);
CREATE INDEX IF NOT EXISTS idx_step_executions_run ON step_executions(run_id);
