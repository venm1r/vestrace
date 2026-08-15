ALTER TABLE events
    ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS recorded_at TIMESTAMPTZ;

UPDATE events
SET recorded_at = created_at
WHERE recorded_at IS NULL;

ALTER TABLE events
    ALTER COLUMN recorded_at SET DEFAULT NOW(),
    ALTER COLUMN recorded_at SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_events_workspace_occurred_at
    ON events (workspace_id, occurred_at)
    WHERE occurred_at IS NOT NULL;
