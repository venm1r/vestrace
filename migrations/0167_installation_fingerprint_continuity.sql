-- The fingerprint key itself is a create-only host-vault record. PostgreSQL
-- stores only its opaque identity, version, and a proof that can be recomputed
-- by the supervisor without exporting the key.
CREATE TABLE installation_fingerprint_continuity (
    installation_id UUID PRIMARY KEY,
    fingerprint_key_id UUID NOT NULL UNIQUE,
    fingerprint_key_version INTEGER NOT NULL CHECK (fingerprint_key_version > 0),
    continuity_proof BYTEA NOT NULL CHECK (octet_length(continuity_proof) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE OR REPLACE FUNCTION vestrace_record_installation_fingerprint_continuity(
    target_installation_id UUID,
    target_fingerprint_key_id UUID,
    target_fingerprint_key_version INTEGER,
    target_continuity_proof BYTEA
)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    INSERT INTO installation_fingerprint_continuity (
        installation_id,
        fingerprint_key_id,
        fingerprint_key_version,
        continuity_proof
    )
    VALUES (
        target_installation_id,
        target_fingerprint_key_id,
        target_fingerprint_key_version,
        target_continuity_proof
    )
    ON CONFLICT DO NOTHING;

    RETURN EXISTS (
        SELECT 1
        FROM installation_fingerprint_continuity
        WHERE installation_id = target_installation_id
          AND fingerprint_key_id = target_fingerprint_key_id
          AND fingerprint_key_version = target_fingerprint_key_version
          AND continuity_proof = target_continuity_proof
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_installation_fingerprint_continuity_matches(
    target_installation_id UUID,
    target_fingerprint_key_id UUID,
    target_fingerprint_key_version INTEGER,
    target_continuity_proof BYTEA
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM installation_fingerprint_continuity
        WHERE installation_id = target_installation_id
          AND fingerprint_key_id = target_fingerprint_key_id
          AND fingerprint_key_version = target_fingerprint_key_version
          AND continuity_proof = target_continuity_proof
    );
$$;
