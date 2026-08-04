-- Migration: 0111_run_event_store_foundation.sql

ALTER TABLE run_events
    ADD COLUMN IF NOT EXISTS event_version SMALLINT,
    ADD COLUMN IF NOT EXISTS actor JSONB,
    ADD COLUMN IF NOT EXISTS causation_id UUID,
    ADD COLUMN IF NOT EXISTS correlation_id UUID,
    ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ;

UPDATE run_events
SET event_version = COALESCE(event_version, 1),
    actor = COALESCE(
        actor,
        jsonb_build_object(
            'system',
            jsonb_build_object('component', 'legacy_migration')
        )
    ),
    causation_id = COALESCE(causation_id, id),
    correlation_id = COALESCE(correlation_id, run_id),
    occurred_at = COALESCE(occurred_at, created_at);

ALTER TABLE run_events
    ALTER COLUMN event_version SET NOT NULL,
    ALTER COLUMN actor SET NOT NULL,
    ALTER COLUMN causation_id SET NOT NULL,
    ALTER COLUMN correlation_id SET NOT NULL,
    ALTER COLUMN occurred_at SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'agent_runs'::regclass
          AND conname = 'agent_runs_workspace_id_id_key'
    ) THEN
        ALTER TABLE agent_runs
            ADD CONSTRAINT agent_runs_workspace_id_id_key
            UNIQUE (workspace_id, id);
    END IF;
END
$$;

ALTER TABLE run_events
    DROP CONSTRAINT IF EXISTS uq_run_event_sequence,
    DROP CONSTRAINT IF EXISTS run_events_workspace_run_sequence_key,
    DROP CONSTRAINT IF EXISTS run_events_sequence_positive,
    DROP CONSTRAINT IF EXISTS run_events_event_version_positive,
    DROP CONSTRAINT IF EXISTS run_events_event_type_non_empty,
    DROP CONSTRAINT IF EXISTS run_events_workspace_run_fkey;

ALTER TABLE run_events
    ADD CONSTRAINT run_events_workspace_run_sequence_key
        UNIQUE (workspace_id, run_id, sequence),
    ADD CONSTRAINT run_events_sequence_positive
        CHECK (sequence > 0),
    ADD CONSTRAINT run_events_event_version_positive
        CHECK (event_version > 0),
    ADD CONSTRAINT run_events_event_type_non_empty
        CHECK (length(btrim(event_type)) > 0),
    ADD CONSTRAINT run_events_workspace_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES agent_runs (workspace_id, id)
        ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS run_events_workspace_run_sequence_idx
    ON run_events (workspace_id, run_id, sequence);
