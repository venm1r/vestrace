-- Durable external-effect intent, receipt, and reconciliation evidence.
-- JSONB payloads remain authoritative; indexed fields are checked on read.

CREATE TABLE IF NOT EXISTS external_effect_intents (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    adapter TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT external_effect_intents_payload_object
        CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT external_effect_intents_adapter_not_blank
        CHECK (btrim(adapter) <> '')
);

CREATE INDEX IF NOT EXISTS idx_external_effect_intents_workspace
    ON external_effect_intents(workspace_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS external_effect_receipts (
    id UUID PRIMARY KEY,
    effect_id UUID NOT NULL REFERENCES external_effect_intents(id) ON DELETE RESTRICT,
    outcome_status TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT external_effect_receipts_payload_object
        CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT external_effect_receipts_outcome_not_blank
        CHECK (btrim(outcome_status) <> '')
);

CREATE INDEX IF NOT EXISTS idx_external_effect_receipts_effect
    ON external_effect_receipts(effect_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS external_reconciliations (
    id UUID PRIMARY KEY,
    outcome TEXT NOT NULL,
    evidence_strength TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT external_reconciliations_payload_object
        CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT external_reconciliations_outcome_not_blank
        CHECK (btrim(outcome) <> ''),
    CONSTRAINT external_reconciliations_evidence_strength_not_blank
        CHECK (btrim(evidence_strength) <> '')
);

CREATE INDEX IF NOT EXISTS idx_external_reconciliations_created
    ON external_reconciliations(created_at DESC, id DESC);
