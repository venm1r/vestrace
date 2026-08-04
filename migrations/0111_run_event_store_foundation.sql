-- Migration: 0111_run_event_store_foundation.sql

ALTER TABLE run_events
    ADD COLUMN IF NOT EXISTS event_version SMALLINT,
    ADD COLUMN IF NOT EXISTS actor JSONB,
    ADD COLUMN IF NOT EXISTS causation_id UUID,
    ADD COLUMN IF NOT EXISTS correlation_id UUID,
    ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ;
