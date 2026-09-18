-- Durable installation-level recovery and trust evidence.
-- Payloads remain authoritative; indexed columns are integrity-checked on read.

CREATE TABLE IF NOT EXISTS incidents (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL,
    scope JSONB NOT NULL,
    payload JSONB NOT NULL,
    opened_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT incidents_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT incidents_scope_object CHECK (jsonb_typeof(scope) IN ('object', 'string')),
    CONSTRAINT incidents_status_not_blank CHECK (btrim(status) <> '')
);

CREATE INDEX IF NOT EXISTS idx_incidents_scope_status
    ON incidents(scope, status, opened_at DESC);

CREATE TABLE IF NOT EXISTS revalidation_runs (
    id UUID PRIMARY KEY,
    incident_id UUID REFERENCES incidents(id) ON DELETE RESTRICT,
    result TEXT NOT NULL,
    scope JSONB NOT NULL,
    payload JSONB NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT revalidation_runs_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT revalidation_runs_result_not_blank CHECK (btrim(result) <> '')
);

CREATE INDEX IF NOT EXISTS idx_revalidation_runs_incident
    ON revalidation_runs(incident_id, completed_at DESC);

CREATE TABLE IF NOT EXISTS trust_state_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scope_key TEXT NOT NULL,
    scope JSONB NOT NULL,
    state TEXT NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT trust_state_records_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT trust_state_records_scope_key_not_blank CHECK (btrim(scope_key) <> ''),
    CONSTRAINT trust_state_records_state_not_blank CHECK (btrim(state) <> ''),
    CONSTRAINT trust_state_records_retry_identity UNIQUE (scope_key, updated_at, payload)
);

CREATE INDEX IF NOT EXISTS idx_trust_state_records_scope_latest
    ON trust_state_records(scope_key, updated_at DESC, created_at DESC);

CREATE TABLE IF NOT EXISTS recovery_points (
    id UUID PRIMARY KEY,
    integrity_status TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT recovery_points_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT recovery_points_integrity_not_blank CHECK (btrim(integrity_status) <> '')
);

CREATE INDEX IF NOT EXISTS idx_recovery_points_created
    ON recovery_points(created_at DESC);
