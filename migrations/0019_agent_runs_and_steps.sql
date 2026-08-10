-- Migration: 0019_agent_runs_and_steps.sql
-- Full schema for durable Agent Run journal: runs and steps.

-- ─── agent_runs ──────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS agent_runs (
    id              UUID PRIMARY KEY,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    principal_id    UUID NOT NULL REFERENCES principals(id) ON DELETE CASCADE,
    title           TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'created',
    run_version     BIGINT NOT NULL DEFAULT 1,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Add columns required by the durable run core
ALTER TABLE agent_runs
    ADD COLUMN IF NOT EXISTS objective TEXT,
    ADD COLUMN IF NOT EXISTS coordinator_snapshot_id UUID,
    ADD COLUMN IF NOT EXISTS active_plan_revision_id UUID,
    ADD COLUMN IF NOT EXISTS execution_mode TEXT NOT NULL DEFAULT 'autopilot',
    ADD COLUMN IF NOT EXISTS current_step_id UUID,
    ADD COLUMN IF NOT EXISTS checkpoint_id UUID,
    ADD COLUMN IF NOT EXISTS parent_run_id UUID,
    ADD COLUMN IF NOT EXISTS parent_step_id UUID,
    ADD COLUMN IF NOT EXISTS root_run_id UUID,
    ADD COLUMN IF NOT EXISTS budget_snapshot_id UUID,
    ADD COLUMN IF NOT EXISTS resource_usage_snapshot_id UUID,
    ADD COLUMN IF NOT EXISTS result JSONB,
    ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ;

-- Backfill objective from title if objective is NULL
UPDATE agent_runs SET objective = title WHERE objective IS NULL;

-- Make objective NOT NULL after backfill
ALTER TABLE agent_runs ALTER COLUMN objective SET NOT NULL;

-- Add check constraints
ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS chk_agent_runs_status;
ALTER TABLE agent_runs
    ADD CONSTRAINT chk_agent_runs_status
    CHECK (status IN ('created','preparing','running','waiting_for_input','waiting_for_approval','waiting_for_dependency','paused','paused_policy_changed','succeeded','succeeded_with_warnings','partial','failed','cancelled','expired'));

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS chk_agent_runs_execution_mode;
ALTER TABLE agent_runs
    ADD CONSTRAINT chk_agent_runs_execution_mode
    CHECK (execution_mode IN ('autopilot','supervised','manual'));

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS chk_agent_runs_objective_len;
ALTER TABLE agent_runs
    ADD CONSTRAINT chk_agent_runs_objective_len
    CHECK (octet_length(objective) >= 1 AND octet_length(objective) <= 32768);

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS chk_agent_runs_version_gte1;
ALTER TABLE agent_runs
    ADD CONSTRAINT chk_agent_runs_version_gte1
    CHECK (run_version >= 1);

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS chk_agent_runs_terminal_has_finished_at;
ALTER TABLE agent_runs
    ADD CONSTRAINT chk_agent_runs_terminal_has_finished_at
    CHECK (
        (status NOT IN ('succeeded','succeeded_with_warnings','partial','failed','cancelled','expired'))
        OR (finished_at IS NOT NULL)
    );

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS chk_agent_runs_nonterminal_no_finished_at;
ALTER TABLE agent_runs
    ADD CONSTRAINT chk_agent_runs_nonterminal_no_finished_at
    CHECK (
        (status IN ('succeeded','succeeded_with_warnings','partial','failed','cancelled','expired'))
        OR (finished_at IS NULL)
    );

ALTER TABLE agent_runs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS agent_runs_workspace_isolation ON agent_runs;
CREATE POLICY agent_runs_workspace_isolation ON agent_runs
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- self-reference FKs (added after table creation)
ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS fk_agent_runs_root_run;
ALTER TABLE agent_runs
    ADD CONSTRAINT fk_agent_runs_root_run
    FOREIGN KEY (root_run_id) REFERENCES agent_runs(id) ON DELETE SET NULL;

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS fk_agent_runs_parent_run;
ALTER TABLE agent_runs
    ADD CONSTRAINT fk_agent_runs_parent_run
    FOREIGN KEY (parent_run_id) REFERENCES agent_runs(id) ON DELETE SET NULL;

-- ─── run_steps ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS run_steps (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    step_number     INT NOT NULL,
    title           TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_run_step_number UNIQUE (run_id, step_number)
);

-- Add columns required by the durable run core
ALTER TABLE run_steps
    ADD COLUMN IF NOT EXISTS plan_step_reference TEXT,
    ADD COLUMN IF NOT EXISTS assigned_actor JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS input_references JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS attempt INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS output_references JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS error JSONB,
    ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ;

ALTER TABLE run_steps
    DROP CONSTRAINT IF EXISTS chk_run_steps_status;
ALTER TABLE run_steps
    ADD CONSTRAINT chk_run_steps_status
    CHECK (status IN ('pending','ready','running','waiting','succeeded','failed','cancelled','skipped','unknown'));

ALTER TABLE run_steps
    DROP CONSTRAINT IF EXISTS chk_run_steps_attempt_gte0;
ALTER TABLE run_steps
    ADD CONSTRAINT chk_run_steps_attempt_gte0
    CHECK (attempt >= 0);

ALTER TABLE run_steps
    DROP CONSTRAINT IF EXISTS chk_run_steps_terminal_has_finished_at;
ALTER TABLE run_steps
    ADD CONSTRAINT chk_run_steps_terminal_has_finished_at
    CHECK (
        (status NOT IN ('succeeded','failed','cancelled','skipped'))
        OR (finished_at IS NOT NULL)
    );

ALTER TABLE run_steps ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS run_steps_workspace_isolation ON run_steps;
CREATE POLICY run_steps_workspace_isolation ON run_steps
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX IF NOT EXISTS ix_run_steps_run_id_created_at
    ON run_steps (run_id, created_at);

-- ─── deferred constraint: current_step_id must belong to this run ────────────

CREATE OR REPLACE FUNCTION fn_validate_current_step_belongs_to_run()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.current_step_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM run_steps
            WHERE id = NEW.current_step_id
              AND run_id = NEW.id
        ) THEN
            RAISE EXCEPTION 'current_step_id % does not belong to run %', NEW.current_step_id, NEW.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_validate_current_step ON agent_runs;
CREATE CONSTRAINT TRIGGER trg_validate_current_step
    AFTER INSERT OR UPDATE OF current_step_id ON agent_runs
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION fn_validate_current_step_belongs_to_run();

-- ─── deferred constraint: parent_step_id must belong to parent_run_id ────────

CREATE OR REPLACE FUNCTION fn_validate_parent_step_belongs_to_parent_run()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.parent_run_id IS NOT NULL AND NEW.parent_step_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM run_steps
            WHERE id = NEW.parent_step_id
              AND run_id = NEW.parent_run_id
        ) THEN
            RAISE EXCEPTION 'parent_step_id % does not belong to parent_run_id %', NEW.parent_step_id, NEW.parent_run_id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_validate_parent_step ON agent_runs;
CREATE CONSTRAINT TRIGGER trg_validate_parent_step
    AFTER INSERT OR UPDATE OF parent_run_id, parent_step_id ON agent_runs
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION fn_validate_parent_step_belongs_to_parent_run();
