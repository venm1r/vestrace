-- Migration: 0136_access_token_authentication_policy.sql
--
-- Make the authentication lookup possible without weakening row level security.
--
-- Migration 0135 tried to solve this with `SECURITY DEFINER` functions. That
-- does not work here and the reason is worth recording: `SECURITY DEFINER`
-- executes as the function's *owner*, the owner is the table owner, and
-- `FORCE ROW LEVEL SECURITY` applies to the table owner too. The function was
-- therefore filtered by the very policy it was meant to step around, and
-- returned no rows to anybody — including a superuser caller, since the
-- executing identity is the owner regardless of who calls.
--
-- The options were: grant BYPASSRLS to the runtime role (defeats RLS
-- everywhere), or introduce a privileged role that owns these two functions
-- (needs superuser at deploy time, so it cannot live in a migration).
--
-- Neither is necessary. The authentication lookup does not need a *privilege*
-- exception; it needs a *knowledge* one. A caller presenting a credential
-- already knows the value that hashes to the row they are asking about, so a
-- policy keyed on that hash discloses nothing they did not already hold:
--
--     USING (token_hash = current_setting('vestrace.authenticating_token_hash'))
--
-- This is strictly narrower than a role that bypasses RLS. It permits exactly
-- one row, only to a caller who already knows its hash, and it is stated
-- declaratively next to the isolation policy rather than hidden inside a
-- function body. `current_setting(..., true)` yields NULL when the setting is
-- absent, and `token_hash = NULL` is NULL rather than true, so an ordinary
-- connection that never sets it sees nothing extra.

DROP FUNCTION IF EXISTS vestrace_resolve_access_token(TEXT);
DROP FUNCTION IF EXISTS vestrace_touch_access_token(UUID);

-- Reading one credential by the hash the caller already presented.
DROP POLICY IF EXISTS access_tokens_authentication_read ON access_tokens;
CREATE POLICY access_tokens_authentication_read ON access_tokens
    FOR SELECT
    USING (token_hash = current_setting('vestrace.authenticating_token_hash', true));

-- Recording that the same credential was used. Keyed on the token id rather
-- than the hash so the update cannot be aimed at a row the caller has not
-- already resolved, and restricted to the one row: a policy on `id` cannot be
-- used to touch a set.
DROP POLICY IF EXISTS access_tokens_authentication_touch ON access_tokens;
CREATE POLICY access_tokens_authentication_touch ON access_tokens
    FOR UPDATE
    USING (id::text = current_setting('vestrace.authenticating_token_id', true))
    WITH CHECK (id::text = current_setting('vestrace.authenticating_token_id', true));

-- Resolution stays in SQL so expiry and revocation are enforced in one place
-- rather than remembered by each caller. `SECURITY INVOKER` now — the policy
-- above is what makes the row visible, not the function's owner.
CREATE OR REPLACE FUNCTION vestrace_resolve_access_token(candidate_hash TEXT)
RETURNS TABLE (token_id UUID, workspace_id UUID, principal_id UUID)
LANGUAGE sql
STABLE
SET search_path = public, pg_temp
AS $$
    SELECT t.id, t.workspace_id, t.principal_id
    FROM access_tokens t
    WHERE t.token_hash = candidate_hash
      AND t.revoked_at IS NULL
      AND (t.expires_at IS NULL OR t.expires_at > now());
$$;

GRANT EXECUTE ON FUNCTION vestrace_resolve_access_token(TEXT) TO vestrace;

CREATE OR REPLACE FUNCTION vestrace_touch_access_token(target_id UUID)
RETURNS VOID
LANGUAGE sql
VOLATILE
SET search_path = public, pg_temp
AS $$
    UPDATE access_tokens SET last_used_at = now() WHERE id = target_id;
$$;

GRANT EXECUTE ON FUNCTION vestrace_touch_access_token(UUID) TO vestrace;
