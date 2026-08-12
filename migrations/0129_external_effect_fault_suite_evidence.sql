-- Durable deterministic fault-suite evidence. The JSONB payload remains
-- authoritative; indexed fields are validated on read.

CREATE TABLE IF NOT EXISTS external_effect_fault_suite_evidence (
    id UUID PRIMARY KEY,
    target_digest TEXT NOT NULL,
    passed BOOLEAN NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT external_effect_fault_suite_evidence_target_not_blank
        CHECK (btrim(target_digest) <> ''),
    CONSTRAINT external_effect_fault_suite_evidence_payload_object
        CHECK (jsonb_typeof(payload) = 'object')
);

CREATE INDEX IF NOT EXISTS idx_external_effect_fault_suite_evidence_target
    ON external_effect_fault_suite_evidence(target_digest, created_at DESC, id DESC);
