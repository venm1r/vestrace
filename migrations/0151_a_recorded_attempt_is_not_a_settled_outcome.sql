-- The reconciliation sweep asked about every effect whose outcome nobody knew,
-- and dropped an effect from that set as soon as *any* reconciliation row
-- existed for it. `Inconclusive` — the provider answered and could not tell us
-- — therefore retired the effect permanently: its receipt stayed 'unknown',
-- nothing ever asked again, and the sweep looked clean because the thing it
-- should have been asking about was no longer in the set it swept.
--
-- Distinguishing a settled outcome from a recorded attempt needs the attempt's
-- own time. `created_at` is when the row was inserted, which is not the same
-- thing as when the observation was made: a sweep that recovers evidence after
-- the fact inserts rows now for observations taken earlier, and backing off from
-- the insert time would ask again too late. The domain's `reconciled_at` is
-- authoritative and lives in the payload; this lifts it out to be indexed, the
-- same way 0128 lifted `effect_id` and `receipt_id`.

ALTER TABLE external_reconciliations
    ADD COLUMN IF NOT EXISTS reconciled_at TIMESTAMPTZ;

-- Why the backfill lifts FORCE first.
--
-- 0150 forced row level security on this table, and migrations run as the
-- runtime role, which owns it. A forced policy applies to the owner too, and the
-- policy admits only rows matching `vestrace.workspace_id` — which a migration
-- has no single value for, because it is fixing every workspace at once.
--
-- So the backfill below sees **no rows at all** and reports `UPDATE 0`. This was
-- not a guess: the first version of this migration did exactly that, and only
-- failed at `SET NOT NULL` because the rows it had not updated were still there.
-- A backfill that did not add a constraint would have reported success having
-- changed nothing.
--
-- Lifting FORCE is safe here and nowhere else: `ALTER TABLE` takes an ACCESS
-- EXCLUSIVE lock, this migration is one transaction, and DDL in Postgres is
-- transactional — so no other session can read the table while the policy is
-- lifted, and a failure anywhere below rolls the table back to FORCE rather than
-- leaving it open.
ALTER TABLE external_reconciliations NO FORCE ROW LEVEL SECURITY;

UPDATE external_reconciliations
SET reconciled_at = COALESCE((payload ->> 'reconciled_at')::TIMESTAMPTZ, created_at)
WHERE reconciled_at IS NULL;

ALTER TABLE external_reconciliations FORCE ROW LEVEL SECURITY;

ALTER TABLE external_reconciliations
    ALTER COLUMN reconciled_at SET NOT NULL;

-- The sweep asks for the most recent attempt per (effect, receipt), so it reads
-- backwards through this.
CREATE INDEX IF NOT EXISTS idx_external_reconciliations_latest_attempt
    ON external_reconciliations(workspace_id, effect_id, receipt_id, reconciled_at DESC, id DESC);
