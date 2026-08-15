-- Migration: 0144_external_effects_belong_to_a_workspace.sql
--
-- Give external-effect receipts and reconciliations the tenant they always had
-- and could not express, then put all three tables under row level security.
--
-- # What was wrong
--
-- Migration 0127 created `external_effect_intents`, `external_effect_receipts`
-- and `external_reconciliations` with **no row level security at all** — not
-- enabled, not forced, no policy. The intents table carries a `workspace_id`;
-- the other two carry none, and inherit their tenant only by joining back to
-- the intent.
--
-- That is defensible in the domain — a receipt belongs to an effect, and the
-- effect belongs to a tenant — and indefensible in storage, because a table
-- with no tenant column cannot have a tenant policy. So a receipt or a
-- reconciliation had no boundary of any kind, and `find_intent`, `find_receipt`
-- and `find_reconciliation` each selected on an id alone: an id from any
-- tenant returned that tenant's row, including the external resource id and
-- provider response digest the receipt carries.
--
-- # Why a denormalized column rather than a policy that joins
--
-- A policy of the form `EXISTS (SELECT 1 FROM external_effect_intents ...)`
-- would work and would be wrong to rely on: it evaluates a subquery per row on
-- every read, and — worse — it leaves the receipt's tenant derivable but not
-- stated, so nothing can index it and nothing can constrain it.
--
-- The column is added instead, and made impossible to disagree with its intent
-- by a composite foreign key rather than by a comment. `external_effect_intents`
-- gains `UNIQUE (id, workspace_id)` so `(effect_id, workspace_id)` on the
-- receipt can reference it; a receipt claiming a workspace its effect does not
-- belong to is refused by the database, not by an adapter that might forget.
-- Reconciliations reference the receipt the same way.
--
-- # Backfill
--
-- Every existing row takes the workspace of its intent. There is no ambiguity
-- to resolve: the join is the only definition the value ever had.

ALTER TABLE external_effect_intents
    ADD CONSTRAINT external_effect_intents_id_workspace_unique
        UNIQUE (id, workspace_id);

ALTER TABLE external_effect_receipts
    ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE external_effect_receipts AS r
SET workspace_id = i.workspace_id
FROM external_effect_intents AS i
WHERE r.effect_id = i.id
  AND r.workspace_id IS NULL;

ALTER TABLE external_effect_receipts
    ALTER COLUMN workspace_id SET NOT NULL;

ALTER TABLE external_effect_receipts
    ADD CONSTRAINT external_effect_receipts_effect_workspace_fk
        FOREIGN KEY (effect_id, workspace_id)
        REFERENCES external_effect_intents (id, workspace_id)
        ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_external_effect_receipts_workspace
    ON external_effect_receipts (workspace_id, created_at DESC, id DESC);

ALTER TABLE external_effect_receipts
    ADD CONSTRAINT external_effect_receipts_id_workspace_unique
        UNIQUE (id, workspace_id);

ALTER TABLE external_reconciliations
    ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE external_reconciliations AS x
SET workspace_id = i.workspace_id
FROM external_effect_intents AS i
WHERE x.effect_id = i.id
  AND x.workspace_id IS NULL;

ALTER TABLE external_reconciliations
    ALTER COLUMN workspace_id SET NOT NULL;

-- Both parents, because a reconciliation names both and either one drifting
-- would make it evidence about a pair that never existed.
ALTER TABLE external_reconciliations
    ADD CONSTRAINT external_reconciliations_effect_workspace_fk
        FOREIGN KEY (effect_id, workspace_id)
        REFERENCES external_effect_intents (id, workspace_id)
        ON DELETE RESTRICT;

ALTER TABLE external_reconciliations
    ADD CONSTRAINT external_reconciliations_receipt_workspace_fk
        FOREIGN KEY (receipt_id, workspace_id)
        REFERENCES external_effect_receipts (id, workspace_id)
        ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_external_reconciliations_workspace
    ON external_reconciliations (workspace_id, created_at DESC, id DESC);

ALTER TABLE external_effect_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_effect_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY external_effect_intents_workspace_isolation ON external_effect_intents
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

ALTER TABLE external_effect_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_effect_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY external_effect_receipts_workspace_isolation ON external_effect_receipts
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

ALTER TABLE external_reconciliations ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_reconciliations FORCE ROW LEVEL SECURITY;
CREATE POLICY external_reconciliations_workspace_isolation ON external_reconciliations
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- `supersession_links` is the other table that held a `workspace_id` with no
-- policy. It has no adapter at all — no reader, no writer, anywhere in the
-- workspace — so this costs nothing today and means the boundary is in place
-- before the first caller arrives rather than after.
ALTER TABLE supersession_links ENABLE ROW LEVEL SECURITY;
ALTER TABLE supersession_links FORCE ROW LEVEL SECURITY;
CREATE POLICY supersession_links_workspace_isolation ON supersession_links
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
