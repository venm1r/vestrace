-- Migration: 0133_workspace_secret_envelope_storage.sql
--
-- Envelope-encrypted secret storage.
--
-- Migration 0087 declared `workspace_keks` with an algorithm column and nothing
-- to hold key material, and no code ever used it. This migration completes it
-- and adds the ciphertext table it was evidently intended to protect.
--
-- Why an envelope rather than encrypting each secret directly under the master
-- key: rotating the master key then rewraps one data key per workspace instead
-- of rewriting every ciphertext, and it bounds how much data sits under any one
-- key. Rotation that requires touching every row is rotation that never happens.
--
-- No column here can hold plaintext. `ciphertext` is AEAD output and cannot be
-- read without the master key, which lives outside the database — a dump of
-- this table alone discloses nothing but names and sizes.

ALTER TABLE workspace_keks
    ADD COLUMN IF NOT EXISTS wrapped_dek  BYTEA,
    ADD COLUMN IF NOT EXISTS wrap_nonce   BYTEA,
    ADD COLUMN IF NOT EXISTS kek_version  TEXT,
    ADD COLUMN IF NOT EXISTS rotated_at   TIMESTAMPTZ;

-- Nullable above because the table pre-exists; enforced together so a row is
-- either a fully-formed wrapped key or an untouched legacy row, never half of
-- one. AES-256-GCM: a 256-bit key wraps to 32 bytes of ciphertext plus a
-- 16-byte tag, under a 96-bit nonce.
ALTER TABLE workspace_keks
    DROP CONSTRAINT IF EXISTS chk_workspace_keks_wrapped_key_complete;
ALTER TABLE workspace_keks
    ADD CONSTRAINT chk_workspace_keks_wrapped_key_complete CHECK (
        (wrapped_dek IS NULL AND wrap_nonce IS NULL AND kek_version IS NULL)
        OR (
            octet_length(wrapped_dek) = 48
            AND octet_length(wrap_nonce) = 12
            AND kek_version IS NOT NULL
            AND length(kek_version) > 0
        )
    );

-- One active data key per workspace and alias.
CREATE UNIQUE INDEX IF NOT EXISTS uq_workspace_keks_workspace_alias
    ON workspace_keks (workspace_id, key_alias);

ALTER TABLE workspace_keks FORCE ROW LEVEL SECURITY;

CREATE TABLE IF NOT EXISTS workspace_secrets (
    id            UUID PRIMARY KEY,
    workspace_id  UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kek_id        UUID NOT NULL REFERENCES workspace_keks(id) ON DELETE RESTRICT,
    name          TEXT NOT NULL,
    purpose       TEXT NOT NULL,
    nonce         BYTEA NOT NULL,
    ciphertext    BYTEA NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A 96-bit nonce is what AES-GCM takes; any other length means the writer
    -- and reader disagree about the construction.
    CONSTRAINT chk_workspace_secrets_nonce_length
        CHECK (octet_length(nonce) = 12),
    -- 16 bytes of authentication tag, so even an empty secret is 16 bytes.
    CONSTRAINT chk_workspace_secrets_ciphertext_authenticated
        CHECK (octet_length(ciphertext) >= 16),
    CONSTRAINT chk_workspace_secrets_name_present
        CHECK (length(name) > 0),
    CONSTRAINT chk_workspace_secrets_purpose_present
        CHECK (length(purpose) > 0)
);

-- ON DELETE RESTRICT above, not CASCADE: dropping a data key must not silently
-- delete the secrets it protects. Deleting the key is a rotation decision, and
-- it should fail loudly while ciphertext still depends on it.

CREATE UNIQUE INDEX IF NOT EXISTS uq_workspace_secrets_workspace_purpose_name
    ON workspace_secrets (workspace_id, purpose, name);

ALTER TABLE workspace_secrets ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_secrets FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS workspace_secrets_workspace_isolation ON workspace_secrets;
CREATE POLICY workspace_secrets_workspace_isolation ON workspace_secrets
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
