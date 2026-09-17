DO $$
BEGIN
    PERFORM vestrace_prepare_p05_installation_drain_upgrade();
EXCEPTION WHEN undefined_function OR insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE EXCEPTION 'installation-drain ownership hand-back must be provisioned before runtime migration'
            USING ERRCODE = '42501';
    END IF;
END $$;

-- DrainMutationPermit / Quiescing: an installation-wide request to stop
-- admitting new provisional-key work and wait until every intent that was
-- already in flight reaches a state that can survive indefinitely, using only
-- the resume/abort logic those intents already have. This table is
-- installation-wide, not workspace-scoped -- no RLS, matching the convention
-- `installation_fingerprint_continuity` (migration 0167) already established
-- for genuinely installation-wide state.
CREATE TABLE installation_drain_requests (
    id UUID PRIMARY KEY,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT installation_drain_requests_completed_after_requested
        CHECK (completed_at IS NULL OR completed_at >= requested_at)
);

-- Exactly one undrained (completed_at IS NULL) request may exist at a time.
-- A drain request that reserve() must see and refuse against relies on this
-- being a real database constraint, not an application-level check that a
-- concurrent transaction could race past.
CREATE UNIQUE INDEX installation_drain_requests_one_active
    ON installation_drain_requests ((true))
    WHERE completed_at IS NULL;

ALTER TABLE material_key_creation_intents
    ADD COLUMN drained_by UUID REFERENCES installation_drain_requests(id);

ALTER TABLE credential_key_creation_intents
    ADD COLUMN drained_by UUID REFERENCES installation_drain_requests(id);

-- Pre-Quiescing states per docs/superpowers/specs/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit-design.md
-- section 2. Bound and every terminal state are never stamped: g0-13 --
-- "Bound never abandons" -- and a terminal intent needs no draining.
CREATE OR REPLACE FUNCTION vestrace_request_installation_drain(
    target_request_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtext('vestrace-installation-mutation-permit-v1')::bigint);
    IF EXISTS (SELECT 1 FROM installation_drain_requests WHERE completed_at IS NULL) THEN
        RAISE EXCEPTION 'an installation drain is already active' USING ERRCODE = '55000';
    END IF;

    INSERT INTO installation_drain_requests (id) VALUES (target_request_id);

    UPDATE material_key_creation_intents
       SET drained_by = target_request_id
     WHERE state IN ('reserved', 'provisional_created', 'provisional_receipted',
                      'content_prepared', 'result_prepared')
       AND drained_by IS NULL;

    UPDATE credential_key_creation_intents
       SET drained_by = target_request_id
     WHERE state IN ('reserved', 'provisional_created', 'provisional_receipted',
                      'credential_prepared')
       AND drained_by IS NULL;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reconcile_installation_drain(
    target_request_id UUID
)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    pending BIGINT;
BEGIN
    PERFORM 1 FROM installation_drain_requests WHERE id = target_request_id AND completed_at IS NULL
        FOR UPDATE;
    IF NOT FOUND THEN
        -- Already Frozen (or the id is unknown) is not an error: reconcile is
        -- idempotent and safe to call after a crash or a repeat call.
        RETURN EXISTS (
            SELECT 1 FROM installation_drain_requests
             WHERE id = target_request_id AND completed_at IS NOT NULL
        );
    END IF;

    SELECT
        (SELECT COUNT(*) FROM material_key_creation_intents
          WHERE drained_by = target_request_id
            AND state IN ('reserved', 'provisional_created', 'provisional_receipted',
                           'content_prepared', 'result_prepared'))
        +
        (SELECT COUNT(*) FROM credential_key_creation_intents
          WHERE drained_by = target_request_id
            AND state IN ('reserved', 'provisional_created', 'provisional_receipted',
                           'credential_prepared'))
      INTO pending;

    IF pending = 0 THEN
        UPDATE installation_drain_requests SET completed_at = NOW() WHERE id = target_request_id;
        RETURN TRUE;
    END IF;
    RETURN FALSE;
END
$$;

-- Widens the existing reserve functions with one precondition: refuse a new
-- Reserved intent while a drain is active. Every other line is copied
-- unchanged from migrations/0169 and 0173 respectively -- this is a
-- CREATE OR REPLACE, the established pattern this schema already uses to
-- widen a guarded function in a later migration (e.g. migration 0174 widened
-- vestrace_record_unbound_material_key_erasure the same way).
CREATE OR REPLACE FUNCTION vestrace_reserve_material_key_creation_intent(
    target_intent_id UUID,
    target_workspace_id UUID,
    target_material_id UUID,
    target_material_key_id UUID,
    target_nonce UUID,
    target_owner_kind TEXT,
    target_owner_id UUID,
    target_output_ordinal BIGINT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM pg_advisory_xact_lock_shared(hashtext('vestrace-installation-mutation-permit-v1')::bigint);
    IF EXISTS (SELECT 1 FROM installation_drain_requests) THEN
        RAISE EXCEPTION 'the installation is draining; no new material key creation intent may be reserved'
            USING ERRCODE = '55000';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);

    INSERT INTO material_key_creation_intents (
        id,
        workspace_id,
        material_id,
        material_key_id,
        nonce,
        owner_kind,
        owner_id,
        output_ordinal,
        state
    )
    VALUES (
        target_intent_id,
        target_workspace_id,
        target_material_id,
        target_material_key_id,
        target_nonce,
        target_owner_kind,
        target_owner_id,
        target_output_ordinal,
        'reserved'
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reserve_credential_key_creation_intent(
    target_intent_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_slot_id UUID,
    target_occupancy_id UUID,
    target_revision_id UUID,
    target_material_key_id UUID,
    target_nonce UUID,
    target_associated_data_profile TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    occupancy_row credential_guard_occupancies%ROWTYPE;
BEGIN
    PERFORM pg_advisory_xact_lock_shared(hashtext('vestrace-installation-mutation-permit-v1')::bigint);
    IF EXISTS (SELECT 1 FROM installation_drain_requests) THEN
        RAISE EXCEPTION 'the installation is draining; no new credential key creation intent may be reserved'
            USING ERRCODE = '55000';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id,
        target_connection_id,
        target_slot_id,
        ARRAY[
            'connection_execution_guard',
            'credential_activation_guard',
            'credential_slot',
            'revision_material'
        ]::TEXT[]
    );
    SELECT * INTO occupancy_row
      FROM credential_guard_occupancies
     WHERE id = target_occupancy_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
     FOR UPDATE;
    IF NOT FOUND OR occupancy_row.state <> 'preparing' THEN
        RAISE EXCEPTION 'credential intent requires a Preparing association'
            USING ERRCODE = '23514';
    END IF;
    IF occupancy_row.intent_id IS NOT NULL THEN
        RAISE EXCEPTION 'credential association already has an intent'
            USING ERRCODE = '23505';
    END IF;
    IF target_associated_data_profile NOT IN ('credential_v2', 'legacy_v1') THEN
        RAISE EXCEPTION 'credential associated-data profile is invalid' USING ERRCODE = '23514';
    END IF;

    -- Allocate this immutable identity before any caller can encrypt.
    INSERT INTO credential_revisions (
        id, workspace_id, credential_slot_id, material_key_id, associated_data_profile
    ) VALUES (
        target_revision_id, target_workspace_id, target_slot_id,
        target_material_key_id, target_associated_data_profile
    );
    INSERT INTO credential_key_creation_intents (
        id, workspace_id, connection_id, credential_slot_id, occupancy_id,
        credential_revision_id, material_key_id, nonce, state
    ) VALUES (
        target_intent_id, target_workspace_id, target_connection_id, target_slot_id,
        target_occupancy_id, target_revision_id, target_material_key_id, target_nonce, 'reserved'
    );
    UPDATE credential_guard_occupancies
       SET intent_id = target_intent_id, updated_at = NOW()
     WHERE id = target_occupancy_id;
END
$$;

-- Hand installation_drain_requests and the two new functions to the guarded
-- owner, exactly as every migration since 0176 has for its own new guarded
-- objects: try the standing (but necessarily pre-0216) allowlisted helper
-- first, and fall back to a direct grant only when running as the SQLx
-- fresh-database superuser, which never runs the Compose provisioner that
-- would otherwise extend the real allowlist. Without this, the drain-guard
-- check the two widened reserve functions above just added would fail with
-- "permission denied for table installation_drain_requests" for every
-- caller, drain active or not -- confirmed by Task 7's own PostgreSQL suite.
--
-- Production upgrades run as the restricted runtime role. This migration's
-- own head guard (vestrace_prepare_p05_installation_drain_upgrade, see
-- docker/postgres/init-runtime-role.sh) lends the migrator temporary
-- ownership of the pre-existing objects it widens; docker/postgres/
-- init-runtime-role.sh separately extends the p03 allowlist below for the
-- two brand-new objects this migration creates. SQLx fresh databases are
-- provisioned by a superuser and never run the Compose bootstrap, which is
-- what the narrow fallback in every DO block below is for.
DO $$
BEGIN
    PERFORM vestrace_assign_p03_table_owner('installation_drain_requests'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER TABLE installation_drain_requests OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON TABLE installation_drain_requests FROM PUBLIC;
    REVOKE ALL ON TABLE installation_drain_requests FROM vestrace;
    GRANT SELECT ON TABLE installation_drain_requests TO vestrace;
END
$$;

DO $$
BEGIN
    PERFORM vestrace_assign_p03_function_owner('vestrace_request_installation_drain(UUID)'::REGPROCEDURE);
    PERFORM vestrace_assign_p03_function_owner('vestrace_reconcile_installation_drain(UUID)'::REGPROCEDURE);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER FUNCTION vestrace_request_installation_drain(UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_reconcile_installation_drain(UUID) OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON FUNCTION vestrace_request_installation_drain(UUID) FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_reconcile_installation_drain(UUID) FROM PUBLIC;
    GRANT EXECUTE ON FUNCTION vestrace_request_installation_drain(UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_reconcile_installation_drain(UUID) TO vestrace;
END
$$;

-- Hand material_key_creation_intents, credential_key_creation_intents, and
-- their reserve functions back to the guarded owner. The head guard above
-- lent these to the migrator only so the ADD COLUMN and CREATE OR REPLACE
-- statements in this file could run; the P02 ownership-assignment helper
-- (already allowlisting these exact objects since migration 0174 first
-- assigned them) is reused here exactly as migration 0193 reused it for its
-- own P02-era objects (material_erasure_blockers,
-- vestrace_prepare_pre_prepared_material_abandon).
DO $$
BEGIN
    PERFORM vestrace_assign_p02_table_owner('material_key_creation_intents'::REGCLASS);
    PERFORM vestrace_assign_p02_table_owner('credential_key_creation_intents'::REGCLASS);
    PERFORM vestrace_assign_p02_function_owner(
        'vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p02_function_owner(
        'vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)'::REGPROCEDURE
    );
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER TABLE material_key_creation_intents OWNER TO vestrace_guarded_owner;
    ALTER TABLE credential_key_creation_intents OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)
        OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)
        OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON FUNCTION vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)
        FROM PUBLIC, vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)
        TO vestrace;
    REVOKE ALL ON FUNCTION vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)
        FROM PUBLIC, vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)
        TO vestrace;
END
$$;
