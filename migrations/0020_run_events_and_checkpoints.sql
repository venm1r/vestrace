-- Migration: 0020_run_events_and_checkpoints.sql
-- Event journal and immutable checkpoints for durable Run replay.

-- ─── run_events ──────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS run_events (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    sequence        BIGINT NOT NULL,
    event_type      TEXT NOT NULL,
    payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_run_event_sequence UNIQUE (run_id, sequence)
);

-- Add columns required by the durable run core
ALTER TABLE run_events
    ADD COLUMN IF NOT EXISTS run_version BIGINT,
    ADD COLUMN IF NOT EXISTS sequence_value BIGINT,
    ADD COLUMN IF NOT EXISTS actor JSONB,
    ADD COLUMN IF NOT EXISTS payload_kind TEXT,
    ADD COLUMN IF NOT EXISTS correlation_id UUID,
    ADD COLUMN IF NOT EXISTS causation_event_id UUID,
    ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Backfill run_version and sequence_value from sequence
UPDATE run_events
SET run_version = COALESCE(run_version, sequence),
    sequence_value = COALESCE(sequence_value, sequence),
    occurred_at = COALESCE(occurred_at, created_at),
    payload_kind = COALESCE(payload_kind, event_type),
    actor = COALESCE(actor, jsonb_build_object('system', jsonb_build_object('component', 'legacy')))
WHERE run_version IS NULL OR sequence_value IS NULL;

ALTER TABLE run_events ALTER COLUMN run_version SET NOT NULL;
ALTER TABLE run_events ALTER COLUMN sequence_value SET NOT NULL;
ALTER TABLE run_events ALTER COLUMN occurred_at SET NOT NULL;

-- Add check constraints
ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS chk_run_events_sequence_gte1;
ALTER TABLE run_events
    ADD CONSTRAINT chk_run_events_sequence_gte1
    CHECK (sequence >= 1);

ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS chk_run_events_version_gte1;
ALTER TABLE run_events
    ADD CONSTRAINT chk_run_events_version_gte1
    CHECK (run_version >= 1);

ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS chk_run_events_sequence_eq_version;
ALTER TABLE run_events
    ADD CONSTRAINT chk_run_events_sequence_eq_version
    CHECK (sequence = run_version);

ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS chk_run_events_cursor_eq_version;
ALTER TABLE run_events
    ADD CONSTRAINT chk_run_events_cursor_eq_version
    CHECK (sequence_value = run_version);

ALTER TABLE run_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS run_events_workspace_isolation ON run_events;
CREATE POLICY run_events_workspace_isolation ON run_events
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX IF NOT EXISTS ix_run_events_run_id_sequence
    ON run_events (run_id, sequence);

-- causation self-reference
ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS fk_run_events_causation;
ALTER TABLE run_events
    ADD CONSTRAINT fk_run_events_causation
    FOREIGN KEY (causation_event_id) REFERENCES run_events(id) ON DELETE SET NULL;

-- ─── run_checkpoints ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS run_checkpoints (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_version     BIGINT NOT NULL,
    state_snapshot  JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_run_checkpoint_version UNIQUE (run_id, run_version)
);

-- Add columns required by the durable run core
ALTER TABLE run_checkpoints
    ADD COLUMN IF NOT EXISTS resume_cursor BIGINT,
    ADD COLUMN IF NOT EXISTS active_plan_revision_id UUID,
    ADD COLUMN IF NOT EXISTS payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS payload_version TEXT NOT NULL DEFAULT 'v1';

-- Backfill resume_cursor from run_version
UPDATE run_checkpoints
SET resume_cursor = COALESCE(resume_cursor, run_version),
    payload = COALESCE(payload, state_snapshot)
WHERE resume_cursor IS NULL;

ALTER TABLE run_checkpoints ALTER COLUMN resume_cursor SET NOT NULL;

ALTER TABLE run_checkpoints
    DROP CONSTRAINT IF EXISTS chk_run_checkpoints_version_gte1;
ALTER TABLE run_checkpoints
    ADD CONSTRAINT chk_run_checkpoints_version_gte1
    CHECK (run_version >= 1);

ALTER TABLE run_checkpoints
    DROP CONSTRAINT IF EXISTS chk_run_checkpoints_cursor_eq_version;
ALTER TABLE run_checkpoints
    ADD CONSTRAINT chk_run_checkpoints_cursor_eq_version
    CHECK (resume_cursor = run_version);

ALTER TABLE run_checkpoints
    DROP CONSTRAINT IF EXISTS chk_run_checkpoints_payload_version;
ALTER TABLE run_checkpoints
    ADD CONSTRAINT chk_run_checkpoints_payload_version
    CHECK (payload_version = 'v1');

ALTER TABLE run_checkpoints ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS run_checkpoints_workspace_isolation ON run_checkpoints;
CREATE POLICY run_checkpoints_workspace_isolation ON run_checkpoints
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX IF NOT EXISTS ix_run_checkpoints_run_id_version_desc
    ON run_checkpoints (run_id, run_version DESC);

-- ─── agent_runs.checkpoint_id FK (added after checkpoints table exists) ──────

ALTER TABLE agent_runs
    DROP CONSTRAINT IF EXISTS fk_agent_runs_checkpoint_id;
ALTER TABLE agent_runs
    ADD CONSTRAINT fk_agent_runs_checkpoint_id
    FOREIGN KEY (checkpoint_id) REFERENCES run_checkpoints(id) ON DELETE SET NULL;

-- ─── application role restrictions: no UPDATE or DELETE on events/checkpoints ──

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vestrace_app') THEN
        REVOKE UPDATE, DELETE ON run_events FROM vestrace_app;
        REVOKE UPDATE, DELETE ON run_checkpoints FROM vestrace_app;
    END IF;
END
$$;
