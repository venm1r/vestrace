#!/usr/bin/env bash
set -Eeuo pipefail

: "${POSTGRES_DB:?POSTGRES_DB is required}"
: "${POSTGRES_USER:?POSTGRES_USER is required}"
: "${VESTRACE_RUNTIME_PASSWORD:?VESTRACE_RUNTIME_PASSWORD is required}"
: "${VESTRACE_GUARDED_OWNER:=vestrace_guarded_owner}"

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 \
  --set=database_name="$POSTGRES_DB" \
  --set=runtime_password="$VESTRACE_RUNTIME_PASSWORD" \
  --set=guarded_owner="$VESTRACE_GUARDED_OWNER" <<'SQL'
SELECT 'CREATE ROLE vestrace LOGIN'
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'vestrace')
\gexec

ALTER ROLE vestrace WITH
  LOGIN
  NOSUPERUSER
  NOCREATEDB
  NOCREATEROLE
  NOINHERIT
  NOREPLICATION
  NOBYPASSRLS
  PASSWORD :'runtime_password';

SELECT format(
    'CREATE ROLE %I NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS',
    :'guarded_owner'
)
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = :'guarded_owner')
\gexec

SELECT format(
    'ALTER ROLE %I WITH NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS',
    :'guarded_owner'
)
\gexec

SELECT format(
    'GRANT %I TO vestrace WITH ADMIN FALSE, INHERIT FALSE, SET FALSE',
    :'guarded_owner'
)
WHERE NOT pg_has_role('vestrace', :'guarded_owner', 'MEMBER')
\gexec

CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

SELECT format('ALTER DATABASE %I OWNER TO vestrace', :'database_name')
\gexec

ALTER SCHEMA public OWNER TO vestrace;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT USAGE, CREATE ON SCHEMA public TO vestrace;

-- P02 migrations run as the runtime role so pre-existing tables retain that
-- ownership. This bootstrap-owned, bounded bridge is the sole exception: it
-- can hand only the declared P02 tables and declared P02 functions to
-- the hard-coded non-login guarded owner. It never accepts an owner argument.
CREATE OR REPLACE FUNCTION public.vestrace_assign_p02_table_owner(target REGCLASS)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    target_schema TEXT;
    target_name TEXT;
BEGIN
    SELECT namespace.nspname, relation.relname
      INTO target_schema, target_name
      FROM pg_class AS relation
      JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
     WHERE relation.oid = target AND relation.relkind = 'r';

    IF target_schema <> 'public' OR target_name <> ALL (ARRAY[
        'p02_guarded_operation_probe',
        'governed_mutation_audit_marks',
        'installation_fingerprint_continuity',
        'installation_mutation_watermark',
        'installation_mutation_watermark_advances',
        'material_key_creation_intents',
        'material_key_creation_intent_erasure_receipts',
        'content_materials',
        'content_material_bytes',
        'prepared_material_attachments',
        'content_material_ordinary_references',
        'connection_execution_guards',
        'credential_activation_guards',
        'credential_slots',
        'credential_revisions',
        'credential_guard_occupancies',
        'credential_key_creation_intents',
        'credential_prepared_materials',
        'credential_prepared_attachments',
        'credential_association_events',
        'credential_lifecycle_events',
        'credential_key_creation_intent_erasure_receipts',
        'material_erasure_preparations',
        'material_erasure_events',
        'material_erasure_audit_tombstones',
        'material_erasure_blockers'
    ]::TEXT[]) THEN
        RAISE EXCEPTION 'only declared P02 tables may be handed to the guarded owner'
            USING ERRCODE = '42501';
    END IF;

    EXECUTE format(
        'ALTER TABLE %I.%I OWNER TO vestrace_guarded_owner',
        target_schema,
        target_name
    );
    EXECUTE format(
        'REVOKE ALL ON TABLE %I.%I FROM PUBLIC',
        target_schema,
        target_name
    );
    EXECUTE format(
        'REVOKE ALL ON TABLE %I.%I FROM vestrace',
        target_schema,
        target_name
    );
END
$$;

CREATE OR REPLACE FUNCTION public.vestrace_assign_p02_function_owner(target REGPROCEDURE)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    target_schema TEXT;
    target_name TEXT;
    target_arguments TEXT;
    allowed_targets REGPROCEDURE[] := ARRAY[
        to_regprocedure('public.vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])'),
        to_regprocedure('public.vestrace_assert_credential_guard_workspace(UUID)'),
        to_regprocedure('public.vestrace_assert_material_erasure_workspace(UUID)'),
        to_regprocedure('public.vestrace_assert_material_intent_workspace(UUID)'),
        to_regprocedure('public.vestrace_bind_credential_key_creation_intent(UUID, UUID)'),
        to_regprocedure('public.vestrace_bind_material_key_creation_intent(UUID, UUID)'),
        to_regprocedure('public.vestrace_content_material_is_live(UUID)'),
        to_regprocedure('public.vestrace_create_credential_prepared_material(UUID, UUID, BYTEA)'),
        to_regprocedure('public.vestrace_credential_revision_is_candidate(UUID)'),
        to_regprocedure('public.vestrace_ensure_connection_execution_guard(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_bound_content_material(UUID)'),
        to_regprocedure('public.vestrace_finalize_bound_credential_candidate(UUID)'),
        to_regprocedure('public.vestrace_finalize_content_material_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_credential_key_abandon(UUID)'),
        to_regprocedure('public.vestrace_finalize_credential_material_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_material_key_abandon(UUID)'),
        to_regprocedure('public.vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)'),
        to_regprocedure('public.vestrace_material_erasure_has_blocker(TEXT, UUID, UUID)'),
        to_regprocedure('public.vestrace_prepare_content_abandon(UUID)'),
        to_regprocedure('public.vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_content_material_erasure(UUID)'),
        to_regprocedure('public.vestrace_prepare_credential_material_erasure(UUID)'),
        to_regprocedure('public.vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_material_key_content(UUID, UUID, BYTEA, BIGINT, TEXT)'),
        to_regprocedure('public.vestrace_prepare_pre_prepared_material_abandon(UUID)'),
        to_regprocedure('public.vestrace_prepare_result_material(UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_record_credential_key_provisional_created(UUID)'),
        to_regprocedure('public.vestrace_record_credential_key_provisional_receipt(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_credential_unbound_key_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_record_governed_mutation_audit_mark_and_advance(UUID, UUID, UUID, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_record_guarded_operation_probe(UUID)'),
        to_regprocedure('public.vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)'),
        to_regprocedure('public.vestrace_record_material_erasure_audit(UUID, TEXT, TEXT, UUID, UUID, UUID, JSONB)'),
        to_regprocedure('public.vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_record_material_erasure_fence(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_material_key_provisional_created(UUID)'),
        to_regprocedure('public.vestrace_record_material_key_provisional_receipt(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_unbound_material_key_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_reject_content_material_state_reversal()'),
        to_regprocedure('public.vestrace_reject_credential_intent_state_reversal()'),
        to_regprocedure('public.vestrace_reject_material_intent_state_reversal()'),
        to_regprocedure('public.vestrace_reject_raw_connection_execution_guard_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_credential_guard_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_credential_intent_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_credential_revision_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_material_erasure_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_material_intent_mutation()'),
        to_regprocedure('public.vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)'),
        to_regprocedure('public.vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_validate_credential_key_creation_intent()'),
        to_regprocedure('public.vestrace_validate_material_erasure()'),
        to_regprocedure('public.vestrace_validate_material_key_creation_intent()')
    ];
    runtime_executable_targets REGPROCEDURE[] := ARRAY[
        to_regprocedure('public.vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])'),
        to_regprocedure('public.vestrace_bind_credential_key_creation_intent(UUID, UUID)'),
        to_regprocedure('public.vestrace_bind_material_key_creation_intent(UUID, UUID)'),
        to_regprocedure('public.vestrace_content_material_is_live(UUID)'),
        to_regprocedure('public.vestrace_create_credential_prepared_material(UUID, UUID, BYTEA)'),
        to_regprocedure('public.vestrace_credential_revision_is_candidate(UUID)'),
        to_regprocedure('public.vestrace_ensure_connection_execution_guard(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_bound_content_material(UUID)'),
        to_regprocedure('public.vestrace_finalize_bound_credential_candidate(UUID)'),
        to_regprocedure('public.vestrace_finalize_content_material_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_credential_key_abandon(UUID)'),
        to_regprocedure('public.vestrace_finalize_credential_material_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_material_key_abandon(UUID)'),
        to_regprocedure('public.vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)'),
        to_regprocedure('public.vestrace_prepare_content_abandon(UUID)'),
        to_regprocedure('public.vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_content_material_erasure(UUID)'),
        to_regprocedure('public.vestrace_prepare_credential_material_erasure(UUID)'),
        to_regprocedure('public.vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_pre_prepared_material_abandon(UUID)'),
        to_regprocedure('public.vestrace_record_credential_key_provisional_created(UUID)'),
        to_regprocedure('public.vestrace_record_credential_key_provisional_receipt(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_credential_unbound_key_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_record_governed_mutation_audit_mark_and_advance(UUID, UUID, UUID, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_record_guarded_operation_probe(UUID)'),
        to_regprocedure('public.vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)'),
        to_regprocedure('public.vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_record_material_erasure_fence(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_material_key_provisional_created(UUID)'),
        to_regprocedure('public.vestrace_record_material_key_provisional_receipt(UUID, UUID)'),
        to_regprocedure('public.vestrace_record_unbound_material_key_erasure(UUID, UUID)'),
        to_regprocedure('public.vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)'),
        to_regprocedure('public.vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)')
    ];
BEGIN
    SELECT namespace.nspname, procedure.proname, pg_get_function_identity_arguments(procedure.oid)
      INTO target_schema, target_name, target_arguments
      FROM pg_proc AS procedure
      JOIN pg_namespace AS namespace ON namespace.oid = procedure.pronamespace
     WHERE procedure.oid = target;

    IF target_schema <> 'public' OR NOT COALESCE(target = ANY (allowed_targets), FALSE) THEN
        RAISE EXCEPTION 'only exact declared P02 function signatures may be handed to the guarded owner'
            USING ERRCODE = '42501';
    END IF;

    EXECUTE format(
        'ALTER FUNCTION %I.%I(%s) OWNER TO vestrace_guarded_owner',
        target_schema,
        target_name,
        target_arguments
    );
    EXECUTE format(
        'REVOKE ALL ON FUNCTION %I.%I(%s) FROM PUBLIC',
        target_schema,
        target_name,
        target_arguments
    );
    IF COALESCE(target = ANY (runtime_executable_targets), FALSE) THEN
        EXECUTE format(
            'GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace',
            target_schema,
            target_name,
            target_arguments
        );
    END IF;
END
$$;

REVOKE ALL ON FUNCTION public.vestrace_assign_p02_table_owner(REGCLASS) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.vestrace_assign_p02_function_owner(REGPROCEDURE) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.vestrace_assign_p02_table_owner(REGCLASS) TO vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_assign_p02_function_owner(REGPROCEDURE) TO vestrace;

-- P03 migrations run as the runtime role, but their guarded tables and
-- functions have a separate exact allowlist. Keeping this bridge separate is
-- what prevents a P03 change from broadening the accepted P02 authority.
CREATE OR REPLACE FUNCTION public.vestrace_assign_p03_table_owner(target REGCLASS)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    target_schema TEXT;
    target_name TEXT;
BEGIN
    SELECT namespace.nspname, relation.relname
      INTO target_schema, target_name
      FROM pg_class AS relation
      JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
     WHERE relation.oid = target AND relation.relkind = 'r';

    IF target_schema <> 'public' OR target_name <> ALL (ARRAY[
        'connection_revision_heads',
        'connection_revisions',
        'no_auth_binding_revisions',
        'qualification_jobs',
        'qualification_target_bindings',
        'qualification_probe_results',
        'qualification_q1_mre_sources',
        'connection_qualification_revisions',
        'connection_qualification_heads',
        'model_revision_heads',
        'model_revisions',
        'model_qualification_revisions',
        'model_qualification_heads',
        'workspace_model_defaults',
        'model_binding_snapshots',
        'run_model_binding_snapshots',
        'model_request_evidence_roots',
        'model_request_evidence_nodes',
        'model_request_evidence_checks',
        'model_request_shape_revisions',
        'model_sampling_revisions',
        'model_limits_revisions',
        'model_tool_schema_revisions',
        'model_request_evidence_check_missing_references',
        'connection_admission_policy_heads',
        'connection_admission_policy_revisions',
        'connection_admission_states',
        'connection_dispatch_admissions',
        'provider_admission_waits',
        'provider_concurrency_leases',
        'provider_throttle_observations',
        'credential_dispatch_leases',
        'credential_activation_events',
        'credential_rotation_events',
        'provider_result_preparations',
        'provider_result_publications',
        'artifact_revision_contents',
        'provider_dispatch_causes',
        'run_step_execution_attempts',
        'embedding_space_registrations',
        'embedding_corpus_generations',
        'embedding_jobs',
        'embedding_transitions',
        'embedding_transition_plans',
        'embedding_transition_plan_recipes',
        'embedding_transition_ambiguity_carries',
        'embedding_transition_ambiguity_carry_recipes',
        'model_binding_snapshot_scopes',
        'model_data_policy_decisions'
    ]::TEXT[]) THEN
        RAISE EXCEPTION 'only declared P03 tables may be handed to the guarded owner'
            USING ERRCODE = '42501';
    END IF;

    EXECUTE format(
        'ALTER TABLE %I.%I OWNER TO vestrace_guarded_owner',
        target_schema,
        target_name
    );
    EXECUTE format(
        'REVOKE ALL ON TABLE %I.%I FROM PUBLIC',
        target_schema,
        target_name
    );
    EXECUTE format(
        'REVOKE ALL ON TABLE %I.%I FROM vestrace',
        target_schema,
        target_name
    );
    EXECUTE format(
        'GRANT SELECT, REFERENCES ON TABLE %I.%I TO vestrace',
        target_schema,
        target_name
    );
    IF target_name = 'model_data_policy_decisions' THEN
        EXECUTE format(
            'REVOKE REFERENCES ON TABLE %I.%I FROM vestrace',
            target_schema,
            target_name
        );
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION public.vestrace_assign_p03_function_owner(target REGPROCEDURE)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    target_schema TEXT;
    target_name TEXT;
    target_arguments TEXT;
    allowed_targets REGPROCEDURE[] := ARRAY[
        to_regprocedure('public.vestrace_create_connection_revision_and_advance_head(UUID, UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_create_no_auth_binding_revision(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_create_model_revision_and_advance_head(UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, INTEGER, UUID, TEXT, INTEGER, UUID, TEXT, BIGINT)'),
        to_regprocedure('public.vestrace_set_workspace_model_default(UUID, UUID, TEXT, UUID, TEXT[], BIGINT)'),
        to_regprocedure('public.vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_finalize_provider_result(UUID, BYTEA)'),
        to_regprocedure('public.vestrace_prepare_candidate_abandon_and_erasure(UUID, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_witness_provider_result_receipt(UUID, UUID)'),
        to_regprocedure('public.vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)'),
        to_regprocedure('public.vestrace_release_provider_dispatch(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_record_provider_throttle(UUID, UUID, UUID, UUID, INTEGER)'),
        to_regprocedure('public.vestrace_record_model_data_policy_decision(UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_lock_model_request_evidence_for_reconstruction(UUID, UUID)'),
        to_regprocedure('public.vestrace_prepare_retired_or_revoked_credential_erasure(UUID)'),
        to_regprocedure('public.vestrace_activate_first_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_rotate_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_revoke_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_issue_credential_dispatch_lease(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_consume_credential_dispatch_lease(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_reject_credential_dispatch_lease_state_rewrite()'),
        to_regprocedure('public.vestrace_reject_p03_immutable_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_p03_mutation()'),
        to_regprocedure('public.vestrace_validate_connection_revision_identity()'),
        to_regprocedure('public.vestrace_validate_provider_result_completion()'),
        to_regprocedure('public.vestrace_validate_task10_deferred_contract()'),
        to_regprocedure('public.vestrace_validate_provider_result_material_live()'),
        to_regprocedure('public.vestrace_create_model_request_shape_revision(UUID, UUID, BIGINT, TEXT, BOOLEAN, TEXT[])'),
        to_regprocedure('public.vestrace_create_model_sampling_revision(UUID, UUID, BIGINT, DOUBLE PRECISION, DOUBLE PRECISION)'),
        to_regprocedure('public.vestrace_create_model_limits_revision(UUID, UUID, BIGINT, INTEGER, INTEGER, INTEGER)'),
        to_regprocedure('public.vestrace_create_model_tool_schema_revision(UUID, UUID, BIGINT, TEXT, TEXT, JSONB)'),
        to_regprocedure('public.vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])'),
        to_regprocedure('public.vestrace_append_model_request_evidence_check(UUID, UUID, UUID, TEXT, TEXT[], UUID[], UUID[], UUID[])'),
        to_regprocedure('public.vestrace_request_qualification_job(UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_record_qualification_probe_result(UUID, UUID, UUID, TEXT, TEXT, UUID, UUID)'),
        to_regprocedure('public.vestrace_cancel_qualification_job(UUID, UUID)'),
        to_regprocedure('public.vestrace_recover_qualification_dispatch_unknown(UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_finalize_qualification_job(UUID, UUID, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_create_qualification_q1_mre_source(UUID, UUID, TEXT, TEXT, TEXT, BOOLEAN, TEXT, BOOLEAN, BOOLEAN, TEXT)')
        ,to_regprocedure('public.vestrace_prepare_qualification_probe_dispatch(UUID, UUID, TEXT, UUID, UUID)')
        ,to_regprocedure('public.vestrace_lock_provider_dispatch_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_reserve_run_step_execution_attempt(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_transition_run_step_execution_attempt(UUID, UUID, UUID, TEXT)')
        ,to_regprocedure('public.vestrace_lock_run_step_attempt_recovery_authority(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_enqueue_run_step_after_input_ready(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT)')
        ,to_regprocedure('public.vestrace_register_embedding_space(UUID, UUID, UUID, TEXT, TEXT, INTEGER)')
        ,to_regprocedure('public.vestrace_publish_embedding_corpus_generation(UUID, UUID, UUID, BIGINT)')
        ,to_regprocedure('public.vestrace_publish_connection_admission_policy(UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER)')
        ,to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)')
        ,to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(UUID, UUID)')
        ,to_regprocedure('public.vestrace_finalize_embedding_job_unknown(UUID, UUID)')
        ,to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)')
        ,to_regprocedure('public.vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)')
    ];
    runtime_executable_targets REGPROCEDURE[] := ARRAY[
        to_regprocedure('public.vestrace_create_connection_revision_and_advance_head(UUID, UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_create_no_auth_binding_revision(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_create_model_revision_and_advance_head(UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, INTEGER, UUID, TEXT, INTEGER, UUID, TEXT, BIGINT)'),
        to_regprocedure('public.vestrace_set_workspace_model_default(UUID, UUID, TEXT, UUID, TEXT[], BIGINT)'),
        to_regprocedure('public.vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_finalize_provider_result(UUID, BYTEA)'),
        to_regprocedure('public.vestrace_prepare_candidate_abandon_and_erasure(UUID, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'),
        to_regprocedure('public.vestrace_witness_provider_result_receipt(UUID, UUID)'),
        to_regprocedure('public.vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)'),
        to_regprocedure('public.vestrace_release_provider_dispatch(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_record_provider_throttle(UUID, UUID, UUID, UUID, INTEGER)'),
        to_regprocedure('public.vestrace_record_model_data_policy_decision(UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_lock_model_request_evidence_for_reconstruction(UUID, UUID)'),
        to_regprocedure('public.vestrace_prepare_retired_or_revoked_credential_erasure(UUID)'),
        to_regprocedure('public.vestrace_activate_first_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_rotate_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_revoke_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_issue_credential_dispatch_lease(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, TIMESTAMPTZ)'),
        to_regprocedure('public.vestrace_consume_credential_dispatch_lease(UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_create_model_request_shape_revision(UUID, UUID, BIGINT, TEXT, BOOLEAN, TEXT[])'),
        to_regprocedure('public.vestrace_create_model_sampling_revision(UUID, UUID, BIGINT, DOUBLE PRECISION, DOUBLE PRECISION)'),
        to_regprocedure('public.vestrace_create_model_limits_revision(UUID, UUID, BIGINT, INTEGER, INTEGER, INTEGER)'),
        to_regprocedure('public.vestrace_create_model_tool_schema_revision(UUID, UUID, BIGINT, TEXT, TEXT, JSONB)'),
        to_regprocedure('public.vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])'),
        to_regprocedure('public.vestrace_append_model_request_evidence_check(UUID, UUID, UUID, TEXT, TEXT[], UUID[], UUID[], UUID[])'),
        to_regprocedure('public.vestrace_request_qualification_job(UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_record_qualification_probe_result(UUID, UUID, UUID, TEXT, TEXT, UUID, UUID)'),
        to_regprocedure('public.vestrace_cancel_qualification_job(UUID, UUID)'),
        to_regprocedure('public.vestrace_recover_qualification_dispatch_unknown(UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_finalize_qualification_job(UUID, UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_create_qualification_q1_mre_source(UUID, UUID, TEXT, TEXT, TEXT, BOOLEAN, TEXT, BOOLEAN, BOOLEAN, TEXT)'),
        to_regprocedure('public.vestrace_prepare_qualification_probe_dispatch(UUID, UUID, TEXT, UUID, UUID)'),
        to_regprocedure('public.vestrace_lock_provider_dispatch_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_reserve_run_step_execution_attempt(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_transition_run_step_execution_attempt(UUID, UUID, UUID, TEXT)'),
        to_regprocedure('public.vestrace_lock_run_step_attempt_recovery_authority(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_enqueue_run_step_after_input_ready(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT)'),
        to_regprocedure('public.vestrace_register_embedding_space(UUID, UUID, UUID, TEXT, TEXT, INTEGER)'),
        to_regprocedure('public.vestrace_publish_embedding_corpus_generation(UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_publish_connection_admission_policy(UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER)'),
        to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_embedding_job_unknown(UUID, UUID)'),
        to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'),
        to_regprocedure('public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)')
    ];
    migration_trigger_targets REGPROCEDURE[] := ARRAY[
        to_regprocedure('public.vestrace_reject_p03_immutable_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_p03_mutation()')
    ];
BEGIN
    SELECT namespace.nspname, procedure.proname, pg_get_function_identity_arguments(procedure.oid)
      INTO target_schema, target_name, target_arguments
      FROM pg_proc AS procedure
      JOIN pg_namespace AS namespace ON namespace.oid = procedure.pronamespace
     WHERE procedure.oid = target;

    IF target_schema <> 'public' OR NOT COALESCE(target = ANY (allowed_targets), FALSE) THEN
        RAISE EXCEPTION 'only exact declared P03 function signatures may be handed to the guarded owner'
            USING ERRCODE = '42501';
    END IF;

    IF target = to_regprocedure('public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)') THEN
        EXECUTE format(
            'REVOKE ALL ON FUNCTION %I.%I(%s) FROM PUBLIC',
            target_schema,
            target_name,
            target_arguments
        );
        EXECUTE format(
            'GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace',
            target_schema,
            target_name,
            target_arguments
        );
        RETURN;
    END IF;

    EXECUTE format(
        'ALTER FUNCTION %I.%I(%s) OWNER TO vestrace_guarded_owner',
        target_schema,
        target_name,
        target_arguments
    );
    EXECUTE format(
        'REVOKE ALL ON FUNCTION %I.%I(%s) FROM PUBLIC',
        target_schema,
        target_name,
        target_arguments
    );
    EXECUTE format(
        'REVOKE ALL ON FUNCTION %I.%I(%s) FROM vestrace',
        target_schema,
        target_name,
        target_arguments
    );
    IF COALESCE(target = ANY (runtime_executable_targets), FALSE)
       OR target = to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)')
       OR target = to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)')
       OR target = to_regprocedure('public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)') THEN
        EXECUTE format(
            'GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace',
            target_schema,
            target_name,
            target_arguments
        );
    END IF;
    IF COALESCE(target = ANY (migration_trigger_targets), FALSE) THEN
        EXECUTE format(
            'GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace',
            target_schema,
            target_name,
            target_arguments
        );
    END IF;
    IF target = to_regprocedure('public.vestrace_finalize_provider_result(UUID, BYTEA)')
       OR target = to_regprocedure('public.vestrace_append_model_request_evidence_check(UUID, UUID, UUID, TEXT, TEXT[], UUID[], UUID[], UUID[])') THEN
        EXECUTE 'REVOKE EXECUTE ON FUNCTION public.vestrace_reject_p03_immutable_mutation() FROM vestrace';
        EXECUTE 'REVOKE EXECUTE ON FUNCTION public.vestrace_reject_raw_p03_mutation() FROM vestrace';
    END IF;
END
$$;

DO $$
DECLARE
    replaced_prepare REGPROCEDURE := to_regprocedure(
        'public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'
    );
BEGIN
    IF replaced_prepare IS NOT NULL THEN
        ALTER FUNCTION public.vestrace_prepare_provider_result(
            UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT
        ) OWNER TO vestrace;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_provider_result(
            UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT
        ) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_provider_result(
            UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT
        ) TO vestrace;
    END IF;
END
$$;

REVOKE ALL ON FUNCTION public.vestrace_assign_p03_table_owner(REGCLASS) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.vestrace_assign_p03_function_owner(REGPROCEDURE) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.vestrace_assign_p03_table_owner(REGCLASS) TO vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_assign_p03_function_owner(REGPROCEDURE) TO vestrace;

-- Migration 0184 must alter a closed set of P03 tables and forward-replace
-- two guarded functions. This no-argument bridge hands only those exact
-- objects back to the runtime migrator and revokes itself in the same
-- transaction. A failed migration rolls both actions back for a safe retry.
CREATE OR REPLACE FUNCTION public.vestrace_prepare_task10_p03_upgrade()
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
DECLARE
    target_name TEXT;
    target_relation REGCLASS;
    target_function REGPROCEDURE;
    target_owner TEXT;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL
       AND EXISTS (
           SELECT 1 FROM public._sqlx_migrations
            WHERE version = 184 AND success
       ) THEN
        RAISE EXCEPTION 'Task 10 P03 ownership hand-back is closed'
            USING ERRCODE = '42501';
    END IF;

    FOREACH target_name IN ARRAY ARRAY[
        'model_request_evidence_checks',
        'qualification_target_bindings',
        'provider_admission_waits',
        'connection_dispatch_admissions',
        'provider_concurrency_leases',
        'provider_throttle_observations',
        'provider_result_preparations',
        'artifact_revision_contents',
        'model_data_policy_decisions'
    ]::TEXT[] LOOP
        target_relation := to_regclass(format('public.%I', target_name));
        IF target_relation IS NOT NULL THEN
            SELECT pg_get_userbyid(relowner)
              INTO target_owner
              FROM pg_class
             WHERE oid = target_relation;
            IF target_owner <> ALL (ARRAY[
                'vestrace', 'vestrace_guarded_owner', current_user
            ]::TEXT[]) THEN
                RAISE EXCEPTION 'Task 10 P03 table has an unexpected owner'
                    USING ERRCODE = '42501';
            END IF;
            EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace', target_name);
        END IF;
    END LOOP;

    FOREACH target_name IN ARRAY ARRAY[
        'public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)',
        'public.vestrace_finalize_provider_result(UUID, BYTEA)',
        'public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)',
        'public.vestrace_consume_credential_dispatch_lease(UUID, UUID, UUID, UUID)'
    ]::TEXT[] LOOP
        target_function := to_regprocedure(target_name);
        IF target_function IS NOT NULL THEN
            SELECT pg_get_userbyid(proowner)
              INTO target_owner
              FROM pg_proc
             WHERE oid = target_function;
            IF target_owner <> ALL (ARRAY[
                'vestrace', 'vestrace_guarded_owner', current_user
            ]::TEXT[]) THEN
                RAISE EXCEPTION 'Task 10 P03 function has an unexpected owner'
                    USING ERRCODE = '42501';
            END IF;
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
        END IF;
    END LOOP;

    IF to_regprocedure(
        'public.vestrace_reject_p03_immutable_mutation()'
    ) IS NOT NULL THEN
        GRANT EXECUTE ON FUNCTION
            public.vestrace_reject_p03_immutable_mutation()
        TO vestrace;
    END IF;

    REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_task10_p03_upgrade()
        FROM vestrace;
END
$$;
REVOKE ALL ON FUNCTION public.vestrace_prepare_task10_p03_upgrade() FROM PUBLIC;
DO $$
DECLARE
    migration_0184_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 184 AND success
        ) INTO migration_0184_applied;
    END IF;
    IF migration_0184_applied THEN
        REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_task10_p03_upgrade()
            FROM vestrace;
    ELSE
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_task10_p03_upgrade()
            TO vestrace;
    END IF;
END
$$;

DO $bootstrap$
DECLARE
    migration_0184_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 184 AND success
        ) INTO migration_0184_applied;
    END IF;
    IF migration_0184_applied THEN
        DROP FUNCTION IF EXISTS
            public.vestrace_install_provider_result_live_trigger();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_install_provider_result_live_trigger()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
            BEGIN
                IF to_regprocedure(
                    'public.vestrace_validate_provider_result_material_live()'
                ) IS NULL THEN
                    RAISE EXCEPTION 'provider-result Live trigger function is absent'
                        USING ERRCODE = '42501';
                END IF;
                DROP TRIGGER IF EXISTS material_key_creation_intents_provider_result_live
                    ON public.material_key_creation_intents;
                CREATE CONSTRAINT TRIGGER material_key_creation_intents_provider_result_live
                    AFTER UPDATE ON public.material_key_creation_intents
                    DEFERRABLE INITIALLY DEFERRED
                    FOR EACH ROW
                    EXECUTE FUNCTION public.vestrace_validate_provider_result_material_live();
                REVOKE EXECUTE ON FUNCTION
                    public.vestrace_install_provider_result_live_trigger()
                FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION
            public.vestrace_install_provider_result_live_trigger()
        FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION
            public.vestrace_install_provider_result_live_trigger()
        TO vestrace;
    END IF;
END
$bootstrap$;

-- Exact P02 guarded dependencies referenced by P03 composite foreign keys.
-- This no-argument bootstrap bridge is callable only after P02 migrations have
-- created the tables. Ownership and P02 allowlists remain unchanged.
CREATE OR REPLACE FUNCTION public.vestrace_grant_p03_dependency_references()
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.credential_revisions'::regclass
           AND conname = 'credential_revisions_exact_slot_key'
    ) THEN
        ALTER TABLE public.credential_revisions
            ADD CONSTRAINT credential_revisions_exact_slot_key
            UNIQUE (workspace_id, credential_slot_id, id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.external_effect_authorizations'::regclass
           AND conname = 'external_effect_authorizations_exact_identity_key'
    ) THEN
        ALTER TABLE public.external_effect_authorizations
            ADD CONSTRAINT external_effect_authorizations_exact_identity_key
            UNIQUE (id, effect_id, workspace_id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.credential_slots'::regclass
           AND conname = 'credential_slots_exact_connection_key'
    ) THEN
        ALTER TABLE public.credential_slots
            ADD CONSTRAINT credential_slots_exact_connection_key
            UNIQUE (workspace_id, connection_id, id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.credential_key_creation_intents'::regclass
           AND conname = 'credential_key_creation_intents_exact_identity_key'
    ) THEN
        ALTER TABLE public.credential_key_creation_intents
            ADD CONSTRAINT credential_key_creation_intents_exact_identity_key
            UNIQUE (workspace_id, connection_id, credential_slot_id, credential_revision_id, id);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'public.content_materials'::regclass
           AND conname = 'content_materials_exact_intent_key'
    ) THEN
        ALTER TABLE public.content_materials
            ADD CONSTRAINT content_materials_exact_intent_key
            UNIQUE (id, workspace_id, intent_id);
    END IF;
    GRANT SELECT, REFERENCES ON TABLE
        connection_execution_guards,
        credential_activation_guards,
        credential_slots,
        credential_revisions,
        credential_guard_occupancies,
        credential_key_creation_intents,
        material_key_creation_intents,
        content_materials,
        material_erasure_preparations,
        material_erasure_audit_tombstones
    TO vestrace;
    GRANT SELECT ON TABLE public.content_material_bytes TO vestrace;
    REVOKE REFERENCES, INSERT, UPDATE, DELETE, TRUNCATE, TRIGGER
        ON TABLE public.content_material_bytes FROM vestrace;
    REVOKE ALL PRIVILEGES ON TABLE
        public.credential_prepared_materials,
        public.credential_prepared_attachments
    FROM vestrace, PUBLIC;
    REVOKE SELECT (id, workspace_id, intent_id, credential_revision_id, ciphertext, created_at),
           INSERT (id, workspace_id, intent_id, credential_revision_id, ciphertext, created_at),
           UPDATE (id, workspace_id, intent_id, credential_revision_id, ciphertext, created_at),
           REFERENCES (id, workspace_id, intent_id, credential_revision_id, ciphertext, created_at)
        ON TABLE public.credential_prepared_materials FROM vestrace, PUBLIC;
    REVOKE SELECT (id, workspace_id, intent_id, credential_revision_id, created_at),
           INSERT (id, workspace_id, intent_id, credential_revision_id, created_at),
           UPDATE (id, workspace_id, intent_id, credential_revision_id, created_at),
           REFERENCES (id, workspace_id, intent_id, credential_revision_id, created_at)
        ON TABLE public.credential_prepared_attachments FROM vestrace, PUBLIC;
    GRANT SELECT (workspace_id, intent_id, credential_revision_id, ciphertext)
        ON TABLE public.credential_prepared_materials TO vestrace;
    GRANT SELECT ON TABLE public.credential_prepared_attachments TO vestrace;
    GRANT SELECT, INSERT ON TABLE
        public.external_effect_authorizations,
        public.audit_events,
        public.idempotency_keys
    TO vestrace;
    GRANT SELECT, INSERT, UPDATE ON TABLE
        public.outbox
    TO vestrace;
    IF to_regprocedure('public.vestrace_reject_p03_immutable_mutation()') IS NOT NULL THEN
        GRANT EXECUTE ON FUNCTION
            vestrace_reject_p03_immutable_mutation(),
            vestrace_reject_raw_p03_mutation()
        TO vestrace;
    END IF;
    GRANT SELECT ON TABLE
        external_effect_intents,
        external_effect_receipts,
        external_effect_lifecycle_transitions,
        agent_runs,
        run_steps,
        run_work_items,
        artifacts,
        artifact_revisions,
        model_executions
    TO vestrace_guarded_owner;
    REVOKE INSERT, UPDATE, DELETE, TRUNCATE, TRIGGER
        ON TABLE public.external_effect_intents FROM vestrace_guarded_owner;
    GRANT UPDATE (id) ON TABLE public.external_effect_intents
        TO vestrace_guarded_owner;
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE
        external_effect_intents,
        external_effect_receipts,
        external_effect_lifecycle_transitions,
        agent_runs,
        run_steps,
        run_work_items,
        artifacts,
        artifact_revisions,
        model_executions
    TO vestrace;
    GRANT SELECT, INSERT, UPDATE ON TABLE public.run_streams TO vestrace;
    GRANT SELECT, INSERT ON TABLE public.run_events TO vestrace;
    GRANT SELECT ON TABLE public.run_checkpoints TO vestrace;
    GRANT USAGE, SELECT ON SEQUENCE
        external_effect_lifecycle_transitions_ordinal_seq
    TO vestrace;
END
$$;
REVOKE ALL ON FUNCTION public.vestrace_grant_p03_dependency_references() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.vestrace_grant_p03_dependency_references() TO vestrace;

-- This one-shot has no target/role/owner input. Before migration 0181 it is
-- available only so that the runtime migrator can close the old Candidate-only
-- entrypoint after both replacements are final. Afterwards bootstrap performs
-- the same static revoke administratively and removes the residual helper.
DO $bootstrap$
DECLARE
    migration_0181_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 181 AND success
        ) INTO migration_0181_applied;
    END IF;
    IF NOT migration_0181_applied THEN
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_disable_superseded_p02_credential_erasure()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = public, pg_temp
            AS $body$
            BEGIN
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_credential_material_erasure(UUID)
                    FROM vestrace;
                REVOKE EXECUTE ON FUNCTION public.vestrace_disable_superseded_p02_credential_erasure()
                    FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_disable_superseded_p02_credential_erasure()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_disable_superseded_p02_credential_erasure()
            TO vestrace;
    ELSE
        REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_credential_material_erasure(UUID)
            FROM vestrace;
        DROP FUNCTION IF EXISTS public.vestrace_disable_superseded_p02_credential_erasure();
    END IF;
END
$bootstrap$;

-- Task 7 may alter only the qualification lifecycle objects and forward-replace
-- the shared credential lease/activation functions.  The bridge is intentionally
-- no-argument and one-shot: a runtime migrator cannot choose another table,
-- function, role, or owner.
DO $bootstrap$
DECLARE
    migration_0185_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 185 AND success
        ) INTO migration_0185_applied;
    END IF;
    IF migration_0185_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_task7_qualification_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_task7_qualification_upgrade()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
            DECLARE
                target_name TEXT;
                target_relation REGCLASS;
                target_function REGPROCEDURE;
                target_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM public._sqlx_migrations
                        WHERE version = 185 AND success
                   ) THEN
                    RAISE EXCEPTION 'Task 7 qualification ownership hand-back is closed'
                        USING ERRCODE = '42501';
                END IF;
                FOREACH target_name IN ARRAY ARRAY[
                    'qualification_target_bindings',
                    'qualification_probe_results'
                ]::TEXT[] LOOP
                    target_relation := to_regclass(format('public.%I', target_name));
                    IF target_relation IS NOT NULL THEN
                        SELECT pg_get_userbyid(relowner) INTO target_owner
                          FROM pg_class WHERE oid = target_relation;
                        IF target_owner <> ALL (ARRAY[
                            'vestrace', 'vestrace_guarded_owner', current_user
                        ]::TEXT[]) THEN
                            RAISE EXCEPTION 'Task 7 qualification table has an unexpected owner'
                                USING ERRCODE = '42501';
                        END IF;
                        EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace', target_name);
                    END IF;
                END LOOP;
                FOREACH target_name IN ARRAY ARRAY[
                    'public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)',
                    'public.vestrace_issue_credential_dispatch_lease(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, TIMESTAMPTZ)',
                    'public.vestrace_consume_credential_dispatch_lease(UUID, UUID, UUID, UUID)',
                    'public.vestrace_activate_first_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)',
                    'public.vestrace_rotate_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'
                ]::TEXT[] LOOP
                    target_function := to_regprocedure(target_name);
                    IF target_function IS NOT NULL THEN
                        SELECT pg_get_userbyid(proowner) INTO target_owner
                          FROM pg_proc WHERE oid = target_function;
                        IF target_owner <> ALL (ARRAY[
                            'vestrace', 'vestrace_guarded_owner', current_user
                        ]::TEXT[]) THEN
                            RAISE EXCEPTION 'Task 7 qualification function has an unexpected owner'
                                USING ERRCODE = '42501';
                        END IF;
                        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                    END IF;
                END LOOP;
                IF to_regprocedure('public.vestrace_reject_p03_immutable_mutation()') IS NOT NULL THEN
                    GRANT EXECUTE ON FUNCTION public.vestrace_reject_p03_immutable_mutation()
                        TO vestrace;
                    GRANT EXECUTE ON FUNCTION public.vestrace_reject_raw_p03_mutation()
                        TO vestrace;
                END IF;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_task7_qualification_upgrade()
                    FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_task7_qualification_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_task7_qualification_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;

-- Task 11 forward-replaces exactly one guarded Candidate-abandon function so
-- replay can use the original association version.  The runtime migrator gets
-- no object, role, or owner selector, and the one-shot revokes itself in the
-- same transaction as the replacement.
DO $bootstrap$
DECLARE
    migration_0186_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 186 AND success
        ) INTO migration_0186_applied;
    END IF;
    IF migration_0186_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_task11_candidate_abandon_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_task11_candidate_abandon_upgrade()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
            DECLARE
                target_function REGPROCEDURE := to_regprocedure(
                    'public.vestrace_prepare_candidate_abandon_and_erasure(UUID, BIGINT)'
                );
                target_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM public._sqlx_migrations
                        WHERE version = 186 AND success
                   ) THEN
                    RAISE EXCEPTION 'Task 11 Candidate-abandon ownership hand-back is closed'
                        USING ERRCODE = '42501';
                END IF;
                IF target_function IS NULL THEN
                    RAISE EXCEPTION 'Task 11 Candidate-abandon function is absent'
                        USING ERRCODE = '42501';
                END IF;
                SELECT pg_get_userbyid(proowner) INTO target_owner
                  FROM pg_proc WHERE oid = target_function;
                IF target_owner <> ALL (ARRAY[
                    'vestrace', 'vestrace_guarded_owner'
                ]::TEXT[]) THEN
                    RAISE EXCEPTION 'Task 11 Candidate-abandon function has an unexpected owner'
                        USING ERRCODE = '42501';
                END IF;
                IF target_owner = 'vestrace_guarded_owner' THEN
                    -- First call, before CREATE OR REPLACE: lend this exact
                    -- object to the restricted migration role and keep the
                    -- bridge callable for the mandatory hand-back below.
                    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                    RETURN;
                END IF;

                -- Second call, after CREATE OR REPLACE: close the loan and
                -- make the bridge inert before the migration commits.
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', target_function);
                EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace', target_function);
                EXECUTE format(
                    'ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function
                );
                REVOKE EXECUTE ON FUNCTION
                    public.vestrace_prepare_task11_candidate_abandon_upgrade()
                FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_task11_candidate_abandon_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_task11_candidate_abandon_upgrade()
            TO vestrace;
    END IF;
END
$bootstrap$;

-- P04 widens two guarded tables so an embedding job can be a dispatch cause.
--
-- `ALTER TABLE` requires ownership, and both tables belong to
-- vestrace_guarded_owner. The runtime role is a member of that role with
-- `INHERIT FALSE, SET FALSE`, so it cannot impersonate the owner -- deliberately,
-- because every other path to those tables goes through a SECURITY DEFINER
-- function. This is the same one-shot hand-back P03 used for Task 7: ownership
-- moves to the migrator for exactly one migration, and 0187 hands it straight
-- back through vestrace_assign_p03_table_owner.
--
-- Without it the widening passes under `#[sqlx::test]`, where migrations run as a
-- superuser that owns everything, and fails in a real deployment with
-- `42501 must be owner of table provider_dispatch_causes`. p03_upgrade_provisioning
-- is the suite that models the real roles, and it is what caught this.
DO $bootstrap$
DECLARE
    migration_0187_applied BOOLEAN := FALSE;
    migration_0188_applied BOOLEAN := FALSE;
    migration_0189_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 187 AND success
        ) INTO migration_0187_applied;
    END IF;
    IF migration_0187_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_dispatch_cause_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_dispatch_cause_upgrade()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
            DECLARE
                target_name TEXT;
                target_relation REGCLASS;
                target_function REGPROCEDURE;
                target_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM public._sqlx_migrations
                        WHERE version = 187 AND success
                   ) THEN
                    RAISE EXCEPTION 'P04 dispatch-cause ownership hand-back is closed'
                        USING ERRCODE = '42501';
                END IF;
                FOREACH target_name IN ARRAY ARRAY[
                    'provider_dispatch_causes',
                    'model_request_evidence_roots'
                ]::TEXT[] LOOP
                    target_relation := to_regclass(format('public.%I', target_name));
                    IF target_relation IS NOT NULL THEN
                        SELECT pg_get_userbyid(relowner) INTO target_owner
                          FROM pg_class WHERE oid = target_relation;
                        IF target_owner <> ALL (ARRAY[
                            'vestrace', 'vestrace_guarded_owner', current_user
                        ]::TEXT[]) THEN
                            RAISE EXCEPTION 'P04 dispatch-cause table has an unexpected owner'
                                USING ERRCODE = '42501';
                        END IF;
                        EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace', target_name);
                    END IF;
                END LOOP;
                -- The four guarded functions 0187 replaces whole. Each is
                -- replaced with its signature unchanged and handed straight back
                -- by vestrace_assign_p03_function_owner at the end of that file.
                FOREACH target_name IN ARRAY ARRAY[
                    'public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)',
                    'public.vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)',
                    'public.vestrace_lock_model_request_evidence_for_reconstruction(UUID, UUID)',
                    'public.vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])'
                ]::TEXT[] LOOP
                    target_function := to_regprocedure(target_name);
                    IF target_function IS NOT NULL THEN
                        SELECT pg_get_userbyid(proowner) INTO target_owner
                          FROM pg_proc WHERE oid = target_function;
                        IF target_owner <> ALL (ARRAY[
                            'vestrace', 'vestrace_guarded_owner', current_user
                        ]::TEXT[]) THEN
                            RAISE EXCEPTION 'P04 dispatch-cause function has an unexpected owner'
                                USING ERRCODE = '42501';
                        END IF;
                        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                    END IF;
                END LOOP;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_dispatch_cause_upgrade()
                    FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_dispatch_cause_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_dispatch_cause_upgrade() TO vestrace;
    END IF;
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 188 AND success
        ) INTO migration_0188_applied;
    END IF;
    IF migration_0188_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_embedding_transition_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_embedding_transition_upgrade()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
            DECLARE
                target_function REGPROCEDURE;
                target_relation REGCLASS;
                target_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM public._sqlx_migrations
                        WHERE version = 188 AND success
                   ) THEN
                    RAISE EXCEPTION 'P04 embedding-transition ownership hand-back is closed'
                        USING ERRCODE = '42501';
                END IF;
                target_relation := to_regclass('public.model_binding_snapshots');
                IF target_relation IS NOT NULL THEN
                    SELECT pg_get_userbyid(relowner) INTO target_owner
                      FROM pg_class WHERE oid = target_relation;
                    IF target_owner <> ALL (ARRAY[
                        'vestrace', 'vestrace_guarded_owner', current_user
                    ]::TEXT[]) THEN
                        RAISE EXCEPTION 'P04 embedding-transition table has an unexpected owner'
                            USING ERRCODE = '42501';
                    END IF;
                    EXECUTE 'ALTER TABLE public.model_binding_snapshots OWNER TO vestrace';
                END IF;
                FOREACH target_function IN ARRAY ARRAY[
                    to_regprocedure('public.vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)'),
                    to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)'),
                    to_regprocedure('public.vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])')
                ]::REGPROCEDURE[] LOOP
                    IF target_function IS NOT NULL THEN
                        SELECT pg_get_userbyid(proowner) INTO target_owner
                          FROM pg_proc WHERE oid = target_function;
                        IF target_owner <> ALL (ARRAY[
                            'vestrace', 'vestrace_guarded_owner', current_user
                        ]::TEXT[]) THEN
                            RAISE EXCEPTION 'P04 embedding-transition function has an unexpected owner'
                                USING ERRCODE = '42501';
                        END IF;
                        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                    END IF;
                END LOOP;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_embedding_transition_upgrade()
                    FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_embedding_transition_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_embedding_transition_upgrade() TO vestrace;
    END IF;
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 189 AND success
        ) INTO migration_0189_applied;
    END IF;
    IF migration_0189_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_transition_carry_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_transition_carry_upgrade()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
            DECLARE
                target_function REGPROCEDURE;
                target_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (
                        SELECT 1 FROM public._sqlx_migrations
                         WHERE version = 189 AND success
                    ) THEN
                    RAISE EXCEPTION 'P04 transition-carry ownership hand-back is closed'
                        USING ERRCODE = '42501';
                END IF;
                FOREACH target_function IN ARRAY ARRAY[
                    to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'),
                    to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)')
                ]::REGPROCEDURE[] LOOP
                    IF target_function IS NOT NULL THEN
                        SELECT pg_get_userbyid(proowner) INTO target_owner
                          FROM pg_proc WHERE oid = target_function;
                        IF target_owner <> ALL (ARRAY[
                            'vestrace', 'vestrace_guarded_owner', current_user
                        ]::TEXT[]) THEN
                            RAISE EXCEPTION 'P04 transition-carry function has an unexpected owner'
                                USING ERRCODE = '42501';
                        END IF;
                        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                    END IF;
                END LOOP;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_transition_carry_upgrade()
                    FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_transition_carry_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_transition_carry_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;
SQL
