-- Migration: 0135_access_token_authentication.sql
--
-- Per-principal authentication credentials.
--
-- Migration 0012 declared `access_tokens` with a workspace, a principal, a hash
-- and a label, and no code ever read or wrote it. Authentication instead used a
-- single shared bearer token mapped to one hardcoded identity, so every
-- authenticated request in the system was the same administrator and nothing
-- could be attributed to whoever actually did it. This migration completes the
-- table so a credential can name a real principal.
--
-- What is missing from 0012 and added here:
--
--   * `revoked_at` — a credential must be withdrawable without deleting the row,
--     because deleting it destroys the record that it ever existed, which is
--     precisely what an incident investigation needs.
--   * `last_used_at` — the cheapest signal that a credential is still live, and
--     the one an operator needs before revoking a token nobody can identify.
--   * FORCE ROW LEVEL SECURITY — 0012 enabled RLS but did not force it, so the
--     owning role read every workspace's credentials. Every other table in this
--     schema forces it.

ALTER TABLE access_tokens
    ADD COLUMN IF NOT EXISTS revoked_at   TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_used_at TIMESTAMPTZ;

-- A label is what an operator revokes by. Blank labels make a credential list
-- unusable at exactly the moment it matters.
ALTER TABLE access_tokens
    DROP CONSTRAINT IF EXISTS chk_access_tokens_label_present;
ALTER TABLE access_tokens
    ADD CONSTRAINT chk_access_tokens_label_present CHECK (length(btrim(label)) > 0);

-- SHA-256 hex. The stored value must never be a token: a 64-character hex
-- string cannot be presented as a credential, so a leaked dump of this table
-- authenticates nobody.
ALTER TABLE access_tokens
    DROP CONSTRAINT IF EXISTS chk_access_tokens_hash_is_sha256_hex;
ALTER TABLE access_tokens
    ADD CONSTRAINT chk_access_tokens_hash_is_sha256_hex CHECK (token_hash ~ '^[0-9a-f]{64}$');

-- The principal must live in the same workspace as the credential. Without the
-- composite key this is two independent foreign keys, and a token could name a
-- principal from another tenant.
ALTER TABLE access_tokens
    DROP CONSTRAINT IF EXISTS access_tokens_principal_in_workspace_fkey;
ALTER TABLE access_tokens
    ADD CONSTRAINT access_tokens_principal_in_workspace_fkey
        FOREIGN KEY (workspace_id, principal_id)
        REFERENCES principals (workspace_id, id) ON DELETE CASCADE;

ALTER TABLE access_tokens FORCE ROW LEVEL SECURITY;

-- Listing a workspace's live credentials is the common query; the partial index
-- keeps revoked rows out of it while they stay on the table as history.
CREATE INDEX IF NOT EXISTS ix_access_tokens_workspace_live
    ON access_tokens (workspace_id, created_at DESC)
    WHERE revoked_at IS NULL;

-- Resolving a presented credential.
--
-- Authentication cannot be workspace-scoped: the workspace is what the token
-- *tells* us, so the lookup must run before any workspace is known, and RLS
-- forbids exactly that. This function is the one narrow exception, and it is
-- shaped so the exception discloses nothing:
--
--   * it takes a hash, never a token — a caller who can invoke it already holds
--     something that hashes to the row it returns;
--   * it returns three identifiers and no other column — not the label, not the
--     timestamps, not the hash;
--   * it returns nothing at all for an expired or revoked credential, so those
--     states are enforced here rather than left to the caller to remember.
--
-- `STABLE` and not `VOLATILE`: it must not be usable to write anything.
CREATE OR REPLACE FUNCTION vestrace_resolve_access_token(candidate_hash TEXT)
RETURNS TABLE (token_id UUID, workspace_id UUID, principal_id UUID)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT t.id, t.workspace_id, t.principal_id
    FROM access_tokens t
    WHERE t.token_hash = candidate_hash
      AND t.revoked_at IS NULL
      AND (t.expires_at IS NULL OR t.expires_at > now());
$$;

REVOKE ALL ON FUNCTION vestrace_resolve_access_token(TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_resolve_access_token(TEXT) TO vestrace;

-- Recording that a credential was used is a write, so it cannot live in the
-- STABLE resolver above. Same narrow shape: it touches one row by id and
-- returns nothing.
CREATE OR REPLACE FUNCTION vestrace_touch_access_token(target_id UUID)
RETURNS VOID
LANGUAGE sql
VOLATILE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    UPDATE access_tokens SET last_used_at = now() WHERE id = target_id;
$$;

REVOKE ALL ON FUNCTION vestrace_touch_access_token(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_touch_access_token(UUID) TO vestrace;
