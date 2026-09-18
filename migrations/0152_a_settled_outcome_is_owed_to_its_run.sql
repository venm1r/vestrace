-- A settled reconciliation is a fact the run that asked for the effect has not
-- been told. Recording *that it was told* is what makes telling it safe.
--
-- Appending to the run's event stream and inserting the reconciliation are two
-- different aggregates and cannot share a transaction here. Doing them in either
-- order loses something: insert-then-append and the append can fail, leaving an
-- effect that is settled — so out of the sweep — with a run that never learned;
-- append-then-insert and a crash between them duplicates the run event.
--
-- So the two are separated by a marker instead. The reconciliation is inserted
-- as before, and stays *owed* to its run until `notified_at` is set. A failed
-- append leaves the debt outstanding and the next sweep retries it; a crash
-- after appending but before marking re-appends, which is at-least-once — the
-- same guarantee the outbox gives, and the effect id in the event lets a reader
-- collapse duplicates.
--
-- Nullable with no backfill, deliberately. The reconciliations already stored
-- are owed like any other, and the sweep will pick them up: their intents carry
-- the free-string execution references this system accepted until yesterday, so
-- no run resolves from them, and the sweep marks a debt to nobody as paid rather
-- than retrying it forever. That is the honest outcome — those effects genuinely
-- cannot be attributed — and it needs no backfill to reach.
--
-- No backfill also means no `UPDATE`, so this migration does not have to lift
-- the forced policy the way 0151 did.

ALTER TABLE external_reconciliations
    ADD COLUMN IF NOT EXISTS notified_at TIMESTAMPTZ;

-- Debts only. A partial index because the settled-and-told rows are the ones
-- that accumulate, and the sweep never looks at them again.
CREATE INDEX IF NOT EXISTS idx_external_reconciliations_owed
    ON external_reconciliations(workspace_id, reconciled_at ASC, id ASC)
    WHERE notified_at IS NULL;
