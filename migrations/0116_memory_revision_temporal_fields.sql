ALTER TABLE memory_revisions
    ADD COLUMN IF NOT EXISTS valid_from TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS valid_until TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS change_reason TEXT,
    ADD COLUMN IF NOT EXISTS canonical_hash TEXT,
    ADD COLUMN IF NOT EXISTS classification TEXT;

ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS state_revision INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_memory_revisions_valid_range
    ON memory_revisions(memory_id, valid_from, valid_until)
    WHERE valid_from IS NOT NULL;
