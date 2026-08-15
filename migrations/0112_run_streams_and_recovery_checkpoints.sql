-- Migration: 0112_run_streams_and_recovery_checkpoints.sql
--
-- Establish an independently durable run stream identity. Run events are
-- authoritative; agent_runs and run_checkpoints are derived data.

CREATE TABLE run_streams (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id UUID NOT NULL,
    current_version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT run_streams_pkey PRIMARY KEY (workspace_id, run_id),
    CONSTRAINT run_streams_current_version_non_negative
        CHECK (current_version >= 0)
);

-- Existing event-backed runs inherit the authoritative event head. Legacy
-- projection-only rows receive version zero rather than promoting projection
-- state into canonical history.
INSERT INTO run_streams (
    workspace_id,
    run_id,
    current_version,
    created_at,
    updated_at
)
SELECT
    agent_runs.workspace_id,
    agent_runs.id,
    COALESCE(MAX(run_events.sequence), 0),
    agent_runs.created_at,
    agent_runs.updated_at
FROM agent_runs
LEFT JOIN run_events
    ON run_events.workspace_id = agent_runs.workspace_id
   AND run_events.run_id = agent_runs.id
GROUP BY
    agent_runs.workspace_id,
    agent_runs.id,
    agent_runs.created_at,
    agent_runs.updated_at;

ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS run_events_run_id_fkey,
    DROP CONSTRAINT IF EXISTS run_events_workspace_run_fkey;

ALTER TABLE run_events
    ADD CONSTRAINT run_events_workspace_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES run_streams (workspace_id, run_id)
        ON DELETE CASCADE;

-- Checkpoints created before P1 did not carry a typed format or an integrity
-- hash and cannot be authoritatively validated. They are derived acceleration
-- data, so discard them instead of assigning a fabricated checksum.
ALTER TABLE agent_runs DROP CONSTRAINT IF EXISTS fk_agent_runs_checkpoint_id;
TRUNCATE TABLE run_checkpoints;

ALTER TABLE run_checkpoints
    DROP CONSTRAINT IF EXISTS run_checkpoints_pkey,
    DROP CONSTRAINT IF EXISTS run_checkpoints_run_id_fkey,
    DROP CONSTRAINT IF EXISTS run_checkpoints_workspace_id_fkey,
    DROP CONSTRAINT IF EXISTS uq_run_checkpoint_version;

ALTER TABLE run_checkpoints
    RENAME COLUMN run_version TO sequence;

ALTER TABLE run_checkpoints
    RENAME COLUMN state_snapshot TO state;

ALTER TABLE run_checkpoints
    DROP COLUMN id,
    ADD COLUMN format_version SMALLINT NOT NULL DEFAULT 1,
    ADD COLUMN state_hash TEXT NOT NULL;

ALTER TABLE run_checkpoints
    ADD CONSTRAINT run_checkpoints_pkey
        PRIMARY KEY (workspace_id, run_id, sequence),
    ADD CONSTRAINT run_checkpoints_workspace_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES run_streams (workspace_id, run_id)
        ON DELETE CASCADE,
    ADD CONSTRAINT run_checkpoints_sequence_positive
        CHECK (sequence > 0),
    ADD CONSTRAINT run_checkpoints_format_version_supported
        CHECK (format_version = 1),
    ADD CONSTRAINT run_checkpoints_state_hash_sha256
        CHECK (state_hash ~ '^[0-9a-f]{64}$');

CREATE INDEX run_checkpoints_workspace_run_sequence_desc_idx
    ON run_checkpoints (workspace_id, run_id, sequence DESC);

ALTER TABLE run_streams ENABLE ROW LEVEL SECURITY;
ALTER TABLE run_streams FORCE ROW LEVEL SECURITY;
ALTER TABLE run_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE run_checkpoints FORCE ROW LEVEL SECURITY;

CREATE POLICY run_streams_workspace_isolation ON run_streams
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- The policy name already exists from migration 0020 and continues to apply
-- after the checkpoint table is reshaped.

-- Transitional compatibility for the P0 direct projection path and the
-- integrated R1.1-R1.3 committer. Task 4 moves all callers to explicit stream
-- creation and removes this trigger.
CREATE FUNCTION vestrace_seed_run_stream_from_projection()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    INSERT INTO run_streams (
        workspace_id,
        run_id,
        current_version,
        created_at,
        updated_at
    ) VALUES (
        NEW.workspace_id,
        NEW.id,
        0,
        NEW.created_at,
        NEW.updated_at
    )
    ON CONFLICT (workspace_id, run_id) DO NOTHING;

    RETURN NEW;
END
$$;

CREATE TRIGGER agent_runs_seed_run_stream
BEFORE INSERT ON agent_runs
FOR EACH ROW
EXECUTE FUNCTION vestrace_seed_run_stream_from_projection();
