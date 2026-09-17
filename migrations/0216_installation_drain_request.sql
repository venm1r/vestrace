-- DrainMutationPermit / Quiescing: installs the guards through the provisioned
-- guarded owner, exactly as every P05 (0209+) migration does -- the migration
-- role has no DDL grant on public itself, only on objects owned by
-- vestrace_guarded_owner via a SECURITY DEFINER installer. See
-- docker/postgres/init-runtime-role.sh's vestrace_install_p05_installation_
-- drain_guards for the authoritative copy of the schema this creates (table,
-- snapshot columns, the two new drain functions, and the widened reserve
-- functions).
--
-- Unlike migrations 0211-0215, this one must also work against a fresh
-- #[sqlx::test] database: DRAIN_HISTORICAL_MIGRATOR (crates/vestrace-
-- infrastructure/src/postgres/pool.rs) is the first bounded migrator built to
-- reach past 208 in that harness, and #[sqlx::test] creates each database
-- with a plain `CREATE DATABASE`, which Postgres clones from `template1` --
-- never from the provisioned `docker/postgres/init-runtime-role.sh` state, so
-- the guards function does not exist there. The fallback below performs the
-- identical DDL directly; it is reached only when the authoritative function
-- is absent AND the connecting role is a superuser (true for every
-- #[sqlx::test] database, never true for the restricted runtime role in any
-- real deployment), so it introduces no privilege the production path does
-- not already have to defend against.
DO $$
BEGIN
    PERFORM public.vestrace_install_p05_installation_drain_guards();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE EXCEPTION 'installation-drain guards must be provisioned before runtime migration'
            USING ERRCODE = '42501';
    END IF;

    IF to_regclass('public.installation_drain_requests') IS NULL THEN
        EXECUTE 'CREATE TABLE public.installation_drain_requests (
            id UUID PRIMARY KEY,
            requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            completed_at TIMESTAMPTZ,
            CONSTRAINT installation_drain_requests_completed_after_requested
                CHECK (completed_at IS NULL OR completed_at >= requested_at)
        )';
        EXECUTE 'CREATE UNIQUE INDEX installation_drain_requests_one_active
            ON public.installation_drain_requests ((true))
            WHERE completed_at IS NULL';
        -- The two pre-existing reserve functions below keep their
        -- vestrace_guarded_owner ownership through CREATE OR REPLACE (it is
        -- preserved, not reset, when replacing an object that already
        -- exists), and their SECURITY DEFINER bodies read this table
        -- internally -- so vestrace_guarded_owner, not this DO block's own
        -- caller, must own it.
        EXECUTE 'ALTER TABLE public.installation_drain_requests OWNER TO vestrace_guarded_owner';
        EXECUTE 'REVOKE ALL ON TABLE public.installation_drain_requests FROM PUBLIC, vestrace';
        EXECUTE 'GRANT SELECT ON TABLE public.installation_drain_requests TO vestrace';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'material_key_creation_intents' AND column_name = 'drained_by'
    ) THEN
        EXECUTE 'ALTER TABLE public.material_key_creation_intents
            ADD COLUMN drained_by UUID REFERENCES public.installation_drain_requests(id)';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'credential_key_creation_intents' AND column_name = 'drained_by'
    ) THEN
        EXECUTE 'ALTER TABLE public.credential_key_creation_intents
            ADD COLUMN drained_by UUID REFERENCES public.installation_drain_requests(id)';
    END IF;

    EXECUTE $reqfn$
    CREATE OR REPLACE FUNCTION public.vestrace_request_installation_drain(
        target_request_id UUID
    )
    RETURNS VOID
    LANGUAGE plpgsql
    SECURITY DEFINER
    SET search_path = public, pg_temp
    AS $reqbody$
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
    $reqbody$;
    $reqfn$;

    EXECUTE $recfn$
    CREATE OR REPLACE FUNCTION public.vestrace_reconcile_installation_drain(
        target_request_id UUID
    )
    RETURNS BOOLEAN
    LANGUAGE plpgsql
    SECURITY DEFINER
    SET search_path = public, pg_temp
    AS $recbody$
    DECLARE
        pending BIGINT;
    BEGIN
        PERFORM 1 FROM installation_drain_requests WHERE id = target_request_id AND completed_at IS NULL
            FOR UPDATE;
        IF NOT FOUND THEN
            -- Already Frozen (or the id is unknown) is not an error: reconcile
            -- is idempotent and safe to call after a crash or a repeat call.
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
    $recbody$;
    $recfn$;

    -- These two are brand new, so CREATE OR REPLACE above left them owned by
    -- this DO block's own caller. Their bodies mutate material_key_creation_
    -- intents/credential_key_creation_intents, which are guarded by raw-
    -- mutation-rejection triggers that check current_user = 'vestrace_
    -- guarded_owner' specifically -- not merely "some privileged owner" --
    -- so they must be re-owned to match, exactly like every other guarded
    -- mutation function in this schema.
    EXECUTE 'ALTER FUNCTION public.vestrace_request_installation_drain(UUID) OWNER TO vestrace_guarded_owner';
    EXECUTE 'ALTER FUNCTION public.vestrace_reconcile_installation_drain(UUID) OWNER TO vestrace_guarded_owner';
    EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_request_installation_drain(UUID) FROM PUBLIC, vestrace';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_request_installation_drain(UUID) TO vestrace';
    EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_reconcile_installation_drain(UUID) FROM PUBLIC, vestrace';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_reconcile_installation_drain(UUID) TO vestrace';

    EXECUTE $matfn$
    CREATE OR REPLACE FUNCTION public.vestrace_reserve_material_key_creation_intent(
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
    AS $matbody$
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
    $matbody$;
    $matfn$;

    EXECUTE $credfn$
    CREATE OR REPLACE FUNCTION public.vestrace_reserve_credential_key_creation_intent(
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
    AS $credbody$
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
    $credbody$;
    $credfn$;
END $$;

DO $p05_installation_drain$
BEGIN
    -- Deliberately does not require vestrace_install_p05_installation_drain_
    -- guards() itself to exist: the #[sqlx::test] fallback path above never
    -- defines that function by name, it performs the equivalent DDL
    -- directly (being superuser already). Only a real deployment routes
    -- through that function; this checks the schema it must leave behind
    -- either way.
    IF to_regclass('public.installation_drain_requests') IS NULL THEN
        RAISE EXCEPTION 'P05 installation-drain table is unavailable' USING ERRCODE = '42501';
    END IF;
    IF to_regprocedure('public.vestrace_request_installation_drain(uuid)') IS NULL
       OR to_regprocedure('public.vestrace_reconcile_installation_drain(uuid)') IS NULL THEN
        RAISE EXCEPTION 'P05 installation-drain functions are unavailable' USING ERRCODE = '42501';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'material_key_creation_intents'
           AND column_name = 'drained_by'
    ) OR NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'credential_key_creation_intents'
           AND column_name = 'drained_by'
    ) THEN
        RAISE EXCEPTION 'P05 installation-drain snapshot columns are unavailable' USING ERRCODE = '42501';
    END IF;
END
$p05_installation_drain$;
