-- Migration: 0131_drop_vestigial_run_checkpoint_columns.sql
--
-- Migration 0112 reshaped run_checkpoints for the durable run core: it renamed
-- run_version to sequence and state_snapshot to state, and truncated the table.
-- It left behind four columns from the previous shape that no writer populates:
--
--   resume_cursor           NOT NULL, no default, and constrained
--                           `resume_cursor = sequence` after the rename
--   active_plan_revision_id unused
--   payload                 dead twin of `state`
--   payload_version         constrained to 'v1', unused
--
-- Because resume_cursor is NOT NULL with no default and is never written,
-- every checkpoint insert failed with 23502. Checkpoint creation — and
-- therefore checkpoint-based run recovery — could not work against this schema.
--
-- Dropping is safe: 0112 truncated the table, and the current writer has never
-- been able to insert a row, so these columns hold no data in any deployment
-- running this code.

ALTER TABLE run_checkpoints
    DROP CONSTRAINT IF EXISTS chk_run_checkpoints_cursor_eq_version;

ALTER TABLE run_checkpoints
    DROP CONSTRAINT IF EXISTS chk_run_checkpoints_payload_version;

ALTER TABLE run_checkpoints
    DROP COLUMN IF EXISTS resume_cursor,
    DROP COLUMN IF EXISTS active_plan_revision_id,
    DROP COLUMN IF EXISTS payload,
    DROP COLUMN IF EXISTS payload_version;
