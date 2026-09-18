-- Query support for durable startup discovery of UNKNOWN effects that still
-- need provider read-back. Reconciliation remains a separate immutable fact.

ALTER TABLE external_reconciliations
    ADD COLUMN IF NOT EXISTS effect_id UUID,
    ADD COLUMN IF NOT EXISTS receipt_id UUID;

UPDATE external_reconciliations
SET effect_id = (payload ->> 'effect_id')::UUID,
    receipt_id = (payload ->> 'receipt_id')::UUID
WHERE effect_id IS NULL OR receipt_id IS NULL;

ALTER TABLE external_reconciliations
    ALTER COLUMN effect_id SET NOT NULL,
    ALTER COLUMN receipt_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_external_effect_receipts_unknown
    ON external_effect_receipts(effect_id, created_at ASC, id ASC)
    WHERE outcome_status = 'unknown';

CREATE INDEX IF NOT EXISTS idx_external_reconciliations_effect_receipt
    ON external_reconciliations(effect_id, receipt_id);
