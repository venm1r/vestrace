-- Migration: 0137_access_token_touch_visibility.sql
--
-- Let the usage timestamp actually be written.
--
-- Migration 0136 added two policies: read by hash, update by id. The update
-- silently affected no rows, and the reason is a rule that is easy to miss —
-- an `UPDATE ... WHERE` does not run on the UPDATE policy alone. Postgres also
-- applies SELECT policies to the rows the statement scans, because the WHERE
-- clause reads them. The read policy keyed only on the hash, the update
-- targeted a row by id, no SELECT policy matched, and the statement updated
-- nothing while reporting success.
--
-- It failed quietly in exactly the way that is hardest to notice: authentication
-- worked, so nothing looked broken, and `last_used_at` simply stayed NULL — the
-- one signal an operator would use to decide whether a credential is still in
-- use.
--
-- The read policy now also matches the token id. That is the same
-- knowledge-bounded exception as before rather than a wider one: an id is
-- learned only by having already resolved the credential from its hash.

DROP POLICY IF EXISTS access_tokens_authentication_read ON access_tokens;
CREATE POLICY access_tokens_authentication_read ON access_tokens
    FOR SELECT
    USING (
        token_hash = current_setting('vestrace.authenticating_token_hash', true)
        OR id::text = current_setting('vestrace.authenticating_token_id', true)
    );
