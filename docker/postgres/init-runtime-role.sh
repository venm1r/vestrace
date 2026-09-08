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
        'embedding_corpus_generation_members',
        'embedding_jobs',
        'embedding_job_material_intents',
        'embedding_job_termination_receipts',
        'embedding_delivery_acceptance_receipts',
        'embedding_delivery_source_memberships',
        'embedding_job_pre_dispatch_retirement_authorities',
        'embedding_output_key_retirement_requests',
        'embedding_output_key_receipts',
        'embedding_output_key_retirement_receipts',
        'embedding_space_corpus_states',
        'embedding_index_generation_guards',
        'embedding_job_result_preparations',
        'embedding_projection_entries',
        'embedding_job_result_prepared_attachments',
        'embedding_projection_source_dependencies',
        'embedding_transitions',
        'embedding_transition_plans',
        'embedding_transition_plan_recipes',
        'embedding_transition_ambiguity_carries',
        'embedding_transition_ambiguity_carry_recipes',
        'embedding_transition_barriers',
        'embedding_transition_barrier_recipes',
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
    target_types TEXT;
    output_key_target BOOLEAN := FALSE;
    runtime_output_key_target BOOLEAN := FALSE;
    result_preparation_target BOOLEAN := FALSE;
    runtime_result_preparation_target BOOLEAN := FALSE;
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
        ,to_regprocedure('public.vestrace_open_embedding_corpus_generation(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_enrol_embedding_corpus_generation_member(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_stale_embedding_corpus_generations_for(UUID, UUID)')
        ,to_regprocedure('public.vestrace_remove_embedding_corpus_generation_members_for(UUID, UUID)')
        ,to_regprocedure('public.vestrace_validate_embedding_corpus_generation_member()')
        ,to_regprocedure('public.vestrace_publish_connection_admission_policy(UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER)')
        ,to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)')
        ,to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(UUID, UUID)')
        ,to_regprocedure('public.vestrace_finalize_embedding_job_unknown(UUID, UUID)')
        ,to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)')
        ,to_regprocedure('public.vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_validate_embedding_transition_barrier_header()')
        ,to_regprocedure('public.vestrace_observe_embedding_transition_barriers(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_abandon_embedding_transition_barrier_candidate(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_validate_embedding_job_output_membership()')
        ,to_regprocedure('public.vestrace_reserve_embedding_job_output_intent(UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_terminate_embedding_job_pre_dispatch(UUID, UUID, UUID, UUID, BIGINT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, TEXT, TEXT, TEXT)')
        ,to_regprocedure('public.vestrace_fence_embedding_job_dispatching()')
        ,to_regprocedure('public.vestrace_lock_embedding_job_pre_dispatch_gate(UUID, UUID, BOOLEAN)')
        ,to_regprocedure('public.vestrace_validate_delivery_source_membership()')
        ,to_regprocedure('public.vestrace_begin_delivery_embedding_outputs(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT, JSONB, JSONB)')
        ,to_regprocedure('public.vestrace_finalize_delivery_embedding_outputs(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_request_embedding_output_retirement(UUID, UUID, UUID, UUID, BIGINT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, TEXT, TEXT, TEXT)')
        ,to_regprocedure('public.vestrace_claim_embedding_output_key(UUID)')
        ,to_regprocedure('public.vestrace_record_embedding_output_key_receipt(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_embedding_output_key_progress(UUID, UUID)')
        ,to_regprocedure('public.vestrace_record_embedding_output_key_retirement(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_validate_embedding_output_termination_authority()')
        ,to_regprocedure('public.vestrace_validate_embedding_result_preparation()')
        ,to_regprocedure('public.vestrace_validate_embedding_projection_dependency()')
        ,to_regprocedure('public.vestrace_create_embedding_result_space_guards()')
        ,to_regprocedure('public.vestrace_lock_embedding_result_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_load_embedding_result_eligibility(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_commit_embedding_result_preparation(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT, UUID[], BYTEA[], INTEGER[])')
        ,to_regprocedure('public.vestrace_reject_result_prepared_pre_dispatch_terminalization()')
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
        to_regprocedure('public.vestrace_open_embedding_corpus_generation(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_enrol_embedding_corpus_generation_member(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_stale_embedding_corpus_generations_for(UUID, UUID)'),
        to_regprocedure('public.vestrace_remove_embedding_corpus_generation_members_for(UUID, UUID)'),
        to_regprocedure('public.vestrace_validate_embedding_corpus_generation_member()'),
        to_regprocedure('public.vestrace_publish_connection_admission_policy(UUID, UUID, UUID, BIGINT, SMALLINT, INTEGER, INTEGER, INTEGER)'),
        to_regprocedure('public.vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)'),
        to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(UUID, UUID)'),
        to_regprocedure('public.vestrace_finalize_embedding_job_unknown(UUID, UUID)'),
        to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'),
        to_regprocedure('public.vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_observe_embedding_transition_barriers(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_abandon_embedding_transition_barrier_candidate(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_reserve_embedding_job_output_intent(UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_terminate_embedding_job_pre_dispatch(UUID, UUID, UUID, UUID, BIGINT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, TEXT, TEXT, TEXT)')
        ,to_regprocedure('public.vestrace_begin_delivery_embedding_outputs(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT, JSONB, JSONB)')
        ,to_regprocedure('public.vestrace_finalize_delivery_embedding_outputs(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_request_embedding_output_retirement(UUID, UUID, UUID, UUID, BIGINT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, TEXT, TEXT, TEXT)')
        ,to_regprocedure('public.vestrace_claim_embedding_output_key(UUID)')
        ,to_regprocedure('public.vestrace_record_embedding_output_key_receipt(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_embedding_output_key_progress(UUID, UUID)')
        ,to_regprocedure('public.vestrace_record_embedding_output_key_retirement(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_lock_embedding_result_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_load_embedding_result_eligibility(UUID, UUID, UUID)')
        ,to_regprocedure('public.vestrace_commit_embedding_result_preparation(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT, UUID[], BYTEA[], INTEGER[])')
    ];
    migration_trigger_targets REGPROCEDURE[] := ARRAY[
        to_regprocedure('public.vestrace_reject_p03_immutable_mutation()'),
        to_regprocedure('public.vestrace_reject_raw_p03_mutation()')
    ];
BEGIN
    SELECT namespace.nspname, procedure.proname, pg_get_function_identity_arguments(procedure.oid),
           pg_catalog.oidvectortypes(procedure.proargtypes)
      INTO target_schema, target_name, target_arguments, target_types
      FROM pg_proc AS procedure
      JOIN pg_namespace AS namespace ON namespace.oid = procedure.pronamespace
     WHERE procedure.oid = target;

    output_key_target := target_schema='public' AND (
        (target_name='vestrace_validate_delivery_source_membership' AND target_types='')
        OR (target_name='vestrace_begin_delivery_embedding_outputs' AND target_types='uuid, uuid, uuid, text, uuid, uuid, text, uuid, uuid, uuid, uuid, bigint, jsonb, jsonb')
        OR (target_name='vestrace_finalize_delivery_embedding_outputs' AND target_types='uuid, uuid, uuid')
        OR (target_name='vestrace_request_embedding_output_retirement' AND target_types='uuid, uuid, uuid, uuid, bigint, text, text, text, uuid, text, text, text, text, text')
        OR (target_name='vestrace_claim_embedding_output_key' AND target_types='uuid')
        OR (target_name='vestrace_record_embedding_output_key_receipt' AND target_types='uuid, uuid, uuid')
        OR (target_name='vestrace_embedding_output_key_progress' AND target_types='uuid, uuid')
        OR (target_name='vestrace_record_embedding_output_key_retirement' AND target_types='uuid, uuid, uuid')
        OR (target_name='vestrace_validate_embedding_output_termination_authority' AND target_types='')
    );
    runtime_output_key_target := output_key_target
        AND target_name NOT IN (
            'vestrace_validate_delivery_source_membership',
            'vestrace_validate_embedding_output_termination_authority'
        );
    result_preparation_target := target_schema='public' AND (
        (target_name='vestrace_validate_embedding_result_preparation' AND target_types='')
        OR (target_name='vestrace_validate_embedding_projection_dependency' AND target_types='')
        OR (target_name='vestrace_create_embedding_result_space_guards' AND target_types='')
        OR (target_name='vestrace_lock_embedding_result_completion_authority' AND target_types='uuid, uuid, uuid, uuid, uuid, uuid, uuid')
        OR (target_name='vestrace_load_embedding_result_eligibility' AND target_types='uuid, uuid, uuid')
        OR (target_name='vestrace_commit_embedding_result_preparation' AND target_types='uuid, uuid, uuid, uuid, uuid, bigint, text, uuid[], bytea[], integer[]')
        OR (target_name='vestrace_reject_result_prepared_pre_dispatch_terminalization' AND target_types='')
    );
    runtime_result_preparation_target := result_preparation_target
        AND target_name IN (
            'vestrace_lock_embedding_result_completion_authority',
            'vestrace_load_embedding_result_eligibility',
            'vestrace_commit_embedding_result_preparation'
        );

    IF target_schema <> 'public'
       OR (NOT COALESCE(target = ANY (allowed_targets), FALSE)
           AND NOT output_key_target AND NOT result_preparation_target) THEN
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
       OR runtime_output_key_target
       OR runtime_result_preparation_target
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

-- Task 8 forward-replaces the guarded transition planner, classifier, and
-- acknowledgement.  It needs its own one-shot bridge: 0189's bridge has
-- already revoked itself once its migration committed.
DO $bootstrap$
DECLARE
    migration_0190_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (
            SELECT 1 FROM public._sqlx_migrations WHERE version = 190 AND success
        ) INTO migration_0190_applied;
    END IF;
    IF migration_0190_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_transition_barriers_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_transition_barriers_upgrade()
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
                         WHERE version = 190 AND success
                    ) THEN
                    RAISE EXCEPTION 'P04 transition-barrier ownership hand-back is closed'
                        USING ERRCODE = '42501';
                END IF;
                FOREACH target_function IN ARRAY ARRAY[
                    to_regprocedure('public.vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'),
                    to_regprocedure('public.vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)')
                ]::REGPROCEDURE[] LOOP
                    IF target_function IS NULL THEN
                        RAISE EXCEPTION 'P04 transition-barrier function is absent'
                            USING ERRCODE = '42501';
                    END IF;
                    SELECT pg_get_userbyid(proowner) INTO target_owner
                      FROM pg_proc WHERE oid = target_function;
                    IF target_owner <> ALL (ARRAY[
                        'vestrace', 'vestrace_guarded_owner', current_user
                    ]::TEXT[]) THEN
                        RAISE EXCEPTION 'P04 transition-barrier function has an unexpected owner'
                            USING ERRCODE = '42501';
                    END IF;
                    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                END LOOP;
                -- Keep this explicit postcondition beside the one-shot revoke:
                -- 0190 re-declares acknowledgement after the preamble, and a
                -- missing hand-off fails only under the restricted migrator.
                target_function := to_regprocedure(
                    'public.vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)'
                );
                IF target_function IS NULL THEN
                    RAISE EXCEPTION 'P04 transition-barrier acknowledgement function is absent'
                        USING ERRCODE = '42501';
                END IF;
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target_function);
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_transition_barriers_upgrade()
                    FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_transition_barriers_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_transition_barriers_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;

-- Task 9 alters the guarded generation table and replaces its publisher.  This
-- is deliberately a fresh one-shot bridge: earlier P04 bridges have revoked
-- themselves by the time 0191 runs.
DO $bootstrap$
DECLARE migration_0191_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 191 AND success)
          INTO migration_0191_applied;
    END IF;
    IF migration_0191_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_generation_fence_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_generation_fence_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $body$
            DECLARE target REGPROCEDURE; owner_name TEXT; table_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 191 AND success) THEN
                    RAISE EXCEPTION 'P04 generation-fence ownership hand-back is closed' USING ERRCODE = '42501';
                END IF;
                SELECT pg_get_userbyid(relowner) INTO table_owner
                  FROM pg_class WHERE oid = 'public.embedding_corpus_generations'::REGCLASS;
                IF table_owner = 'vestrace_guarded_owner' THEN
                    ALTER TABLE public.embedding_corpus_generations OWNER TO vestrace;
                ELSIF table_owner = 'vestrace' THEN
                    ALTER TABLE public.embedding_corpus_generations OWNER TO vestrace_guarded_owner;
                ELSE
                    RAISE EXCEPTION 'P04 generation-fence table has an unexpected owner' USING ERRCODE = '42501';
                END IF;
                target := to_regprocedure('public.vestrace_publish_embedding_corpus_generation(UUID, UUID, UUID, BIGINT)');
                IF target IS NULL OR NOT has_function_privilege(current_user, target, 'EXECUTE') THEN
                    RAISE EXCEPTION 'P04 generation-fence publisher is unavailable' USING ERRCODE = '42501';
                END IF;
                SELECT pg_get_userbyid(proowner) INTO owner_name FROM pg_proc WHERE oid = target;
                IF owner_name = 'vestrace_guarded_owner' THEN
                    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target);
                ELSIF owner_name = 'vestrace' THEN
                    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target);
                    REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_generation_fence_upgrade() FROM vestrace;
                ELSE
                    RAISE EXCEPTION 'P04 generation-fence publisher has an unexpected owner' USING ERRCODE = '42501';
                END IF;
            END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_generation_fence_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_generation_fence_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;

-- Task 14B alters the guarded provider lease and forward-replaces the shared
-- embedding dispatch/recovery fences.  Keep this hand-back limited to the
-- exact objects 0192 replaces; the effect lifecycle table remains runtime
-- owned so its narrow trigger installation needs no ownership transfer.
DO $bootstrap$
DECLARE migration_0192_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 192 AND success)
          INTO migration_0192_applied;
    END IF;
    IF migration_0192_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_termination_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_p04_termination_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_termination_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $body$
            DECLARE target REGPROCEDURE; owner_name TEXT; table_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 192 AND success) THEN
                    RAISE EXCEPTION 'P04 termination ownership hand-back is closed' USING ERRCODE = '42501';
                END IF;
                SELECT pg_get_userbyid(relowner) INTO table_owner
                  FROM pg_class WHERE oid = 'public.provider_concurrency_leases'::REGCLASS;
                IF table_owner = 'vestrace_guarded_owner' THEN
                    ALTER TABLE public.provider_concurrency_leases OWNER TO vestrace;
                ELSIF table_owner <> 'vestrace' THEN
                    RAISE EXCEPTION 'P04 termination lease table has an unexpected owner' USING ERRCODE = '42501';
                END IF;
                GRANT TRIGGER ON TABLE public.material_key_creation_intents TO vestrace;
                FOREACH target IN ARRAY ARRAY[
                    to_regprocedure('public.vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)'),
                    to_regprocedure('public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(UUID, UUID)')
                ]::REGPROCEDURE[] LOOP
                    IF target IS NULL THEN
                        RAISE EXCEPTION 'P04 termination shared fence is absent' USING ERRCODE = '42501';
                    END IF;
                    SELECT pg_get_userbyid(proowner) INTO owner_name FROM pg_proc WHERE oid = target;
                    IF owner_name <> ALL (ARRAY['vestrace', 'vestrace_guarded_owner']::TEXT[]) THEN
                        RAISE EXCEPTION 'P04 termination shared fence has an unexpected owner' USING ERRCODE = '42501';
                    END IF;
                    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace', target);
                END LOOP;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_termination_upgrade() FROM vestrace;
            END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_termination_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_termination_upgrade() TO vestrace;
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_finish_p04_termination_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $body$
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 192 AND success) THEN
                    RAISE EXCEPTION 'P04 termination finish is closed' USING ERRCODE = '42501';
                END IF;
                IF NOT EXISTS (
                    SELECT 1
                      FROM pg_trigger AS trigger
                     WHERE trigger.tgrelid='public.material_key_creation_intents'::REGCLASS
                       AND trigger.tgname='material_intents_embedding_owner_deferred_membership'
                       AND trigger.tgfoid=to_regprocedure('public.vestrace_validate_embedding_job_output_membership()')
                ) THEN
                    RAISE EXCEPTION 'P04 termination finish requires its exact installed trigger'
                        USING ERRCODE = '42501';
                END IF;
                REVOKE TRIGGER ON TABLE public.material_key_creation_intents FROM vestrace;
                REVOKE EXECUTE ON FUNCTION public.vestrace_finish_p04_termination_upgrade() FROM vestrace;
            END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_p04_termination_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_p04_termination_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;

-- Task 14C forward-replaces the generic unprepared-intent guard and adds a
-- workspace-composite blocker reference.  The two one-shot helpers expose only
-- those exact objects and disappear on the first provisioner refresh after
-- 0193 succeeds.
DO $bootstrap$
DECLARE migration_0193_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 193 AND success)
          INTO migration_0193_applied;
    END IF;
    IF migration_0193_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_output_key_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_p04_output_key_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_output_key_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
            DECLARE function_owner TEXT; table_owner TEXT;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=193 AND success) THEN
                    RAISE EXCEPTION 'P04 output-key ownership hand-back is closed' USING ERRCODE='42501';
                END IF;
                SELECT pg_get_userbyid(proowner) INTO function_owner FROM pg_proc
                 WHERE oid=to_regprocedure('public.vestrace_prepare_pre_prepared_material_abandon(uuid)');
                IF function_owner='vestrace_guarded_owner' THEN
                    ALTER FUNCTION public.vestrace_prepare_pre_prepared_material_abandon(uuid) OWNER TO vestrace;
                ELSIF function_owner IS DISTINCT FROM 'vestrace' THEN
                    RAISE EXCEPTION 'P04 output-key abandonment guard has an unexpected owner' USING ERRCODE='42501';
                END IF;
                SELECT pg_get_userbyid(relowner) INTO table_owner FROM pg_class
                 WHERE oid=to_regclass('public.material_erasure_blockers');
                IF table_owner='vestrace_guarded_owner' THEN
                    ALTER TABLE public.material_erasure_blockers OWNER TO vestrace;
                ELSIF table_owner IS DISTINCT FROM 'vestrace' THEN
                    RAISE EXCEPTION 'P04 output-key blocker table has an unexpected owner' USING ERRCODE='42501';
                END IF;
                SELECT pg_get_userbyid(relowner) INTO table_owner FROM pg_class
                 WHERE oid=to_regclass('public.embedding_job_termination_receipts');
                IF table_owner='vestrace_guarded_owner' THEN
                    ALTER TABLE public.embedding_job_termination_receipts OWNER TO vestrace;
                ELSIF table_owner IS DISTINCT FROM 'vestrace' THEN
                    RAISE EXCEPTION 'P04 output-key termination table has an unexpected owner' USING ERRCODE='42501';
                END IF;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_output_key_upgrade() FROM vestrace;
            END $body$
        $function$;
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_finish_p04_output_key_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
            DECLARE object_owner TEXT; guarded_table TEXT; target_function REGPROCEDURE;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=193 AND success) THEN
                    RAISE EXCEPTION 'P04 output-key finish is closed' USING ERRCODE='42501';
                END IF;
                SELECT pg_get_userbyid(relowner) INTO object_owner FROM pg_class
                 WHERE oid=to_regclass('public.material_erasure_blockers');
                IF object_owner IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                    RAISE EXCEPTION 'P04 output-key blocker ownership was not restored' USING ERRCODE='42501';
                END IF;
                SELECT pg_get_userbyid(relowner) INTO object_owner FROM pg_class
                 WHERE oid=to_regclass('public.embedding_job_termination_receipts');
                IF object_owner IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                    RAISE EXCEPTION 'P04 output-key termination ownership was not restored' USING ERRCODE='42501';
                END IF;
                FOREACH guarded_table IN ARRAY ARRAY[
                    'embedding_delivery_acceptance_receipts',
                    'embedding_delivery_source_memberships',
                    'embedding_job_pre_dispatch_retirement_authorities',
                    'embedding_output_key_retirement_requests',
                    'embedding_output_key_receipts',
                    'embedding_output_key_retirement_receipts'
                ] LOOP
                    SELECT pg_get_userbyid(relowner) INTO object_owner FROM pg_class
                     WHERE oid=to_regclass('public.' || guarded_table);
                    IF object_owner IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                        RAISE EXCEPTION 'P04 output-key guarded table ownership was not restored: %',guarded_table USING ERRCODE='42501';
                    END IF;
                END LOOP;
                IF NOT EXISTS(
                    SELECT 1 FROM pg_constraint
                     WHERE conrelid='public.material_erasure_blockers'::regclass
                       AND conname='material_erasure_blockers_id_workspace_key'
                ) OR NOT EXISTS(
                    SELECT 1 FROM pg_trigger
                     WHERE tgrelid='public.embedding_delivery_source_memberships'::regclass
                       AND tgname='embedding_delivery_source_memberships_exact'
                       AND tgfoid=to_regprocedure('public.vestrace_validate_delivery_source_membership()')
                ) OR NOT EXISTS(
                    SELECT 1 FROM pg_trigger
                     WHERE tgrelid='public.embedding_job_termination_receipts'::regclass
                       AND tgname='embedding_job_termination_output_authority_exact'
                       AND tgfoid=to_regprocedure('public.vestrace_validate_embedding_output_termination_authority()')
                ) THEN
                    RAISE EXCEPTION 'P04 output-key finish requires its exact constraint and trigger' USING ERRCODE='42501';
                END IF;
                REVOKE EXECUTE ON FUNCTION public.vestrace_finish_p04_output_key_upgrade() FROM vestrace;
            END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_output_key_upgrade() FROM PUBLIC;
        REVOKE ALL ON FUNCTION public.vestrace_finish_p04_output_key_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_output_key_upgrade() TO vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_p04_output_key_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;

-- Task 14D requires only trigger installation on two previously guarded
-- terminalization tables. The one-shot hand-back makes that capability usable
-- by the real runtime migrator and withdraws it once 0194 has installed the
-- exact guarded result-preparation authority.
DO $bootstrap$
DECLARE migration_0194_applied BOOLEAN := FALSE;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS (SELECT 1 FROM public._sqlx_migrations WHERE version = 194 AND success)
          INTO migration_0194_applied;
    END IF;
    IF migration_0194_applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_result_preparation_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_p04_result_preparation_upgrade();
    ELSE
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_result_preparation_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
            DECLARE function_owner TEXT; guarded_table TEXT; target_function REGPROCEDURE;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=194 AND success) THEN
                    RAISE EXCEPTION 'P04 result-preparation ownership hand-back is closed' USING ERRCODE='42501';
                END IF;
                GRANT TRIGGER ON TABLE public.embedding_job_pre_dispatch_retirement_authorities TO vestrace;
                GRANT TRIGGER ON TABLE public.embedding_output_key_retirement_requests TO vestrace;
                GRANT REFERENCES ON TABLE public.material_erasure_blockers TO vestrace;
                GRANT REFERENCES ON TABLE public.prepared_material_attachments TO vestrace;
                FOREACH guarded_table IN ARRAY ARRAY[
                    'embedding_space_registrations',
                    'embedding_delivery_source_memberships'
                ] LOOP
                    SELECT pg_get_userbyid(relowner) INTO function_owner FROM pg_class
                     WHERE oid=to_regclass('public.' || guarded_table);
                    IF function_owner='vestrace_guarded_owner' THEN
                        EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace',guarded_table);
                    ELSIF function_owner IS DISTINCT FROM 'vestrace' THEN
                        RAISE EXCEPTION 'P04 result-preparation % ownership hand-back is unavailable',guarded_table USING ERRCODE='42501';
                    END IF;
                END LOOP;
                FOREACH target_function IN ARRAY ARRAY[
                    'public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)'::REGPROCEDURE,
                    'public.vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)'::REGPROCEDURE,
                    'public.vestrace_fence_embedding_job_dispatching()'::REGPROCEDURE
                ] LOOP
                    SELECT pg_get_userbyid(proowner) INTO function_owner FROM pg_proc
                     WHERE oid=target_function;
                    IF function_owner='vestrace_guarded_owner' THEN
                        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace',target_function);
                    ELSIF function_owner IS DISTINCT FROM 'vestrace' THEN
                        RAISE EXCEPTION 'P04 result-preparation guarded function hand-back is unavailable: %',target_function USING ERRCODE='42501';
                    END IF;
                END LOOP;
                REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_result_preparation_upgrade() FROM vestrace;
            END $body$
        $function$;
        EXECUTE $function$
            CREATE OR REPLACE FUNCTION public.vestrace_finish_p04_result_preparation_upgrade()
            RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
            DECLARE object_owner TEXT; guarded_table TEXT; target_function REGPROCEDURE;
            BEGIN
                IF to_regclass('public._sqlx_migrations') IS NOT NULL
                   AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=194 AND success) THEN
                    RAISE EXCEPTION 'P04 result-preparation finish is closed' USING ERRCODE='42501';
                END IF;
                FOREACH guarded_table IN ARRAY ARRAY[
                    'embedding_space_corpus_states',
                    'embedding_index_generation_guards',
                    'embedding_job_result_preparations',
                    'embedding_projection_entries',
                    'embedding_job_result_prepared_attachments',
                    'embedding_projection_source_dependencies',
                    'embedding_space_registrations',
                    'embedding_delivery_source_memberships'
                ] LOOP
                    SELECT pg_get_userbyid(relowner) INTO object_owner FROM pg_class
                     WHERE oid=to_regclass('public.' || guarded_table);
                    IF object_owner='vestrace'
                       OR (object_owner=current_user AND session_user=current_user) THEN
                        EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace_guarded_owner',guarded_table);
                    ELSIF object_owner IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                        RAISE EXCEPTION 'P04 result-preparation guarded table ownership was not restored: %',guarded_table USING ERRCODE='42501';
                    END IF;
                END LOOP;
                FOREACH target_function IN ARRAY ARRAY[
                    'public.vestrace_validate_embedding_result_preparation()'::REGPROCEDURE,
                    'public.vestrace_validate_embedding_projection_dependency()'::REGPROCEDURE,
                    'public.vestrace_create_embedding_result_space_guards()'::REGPROCEDURE,
                    'public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'::REGPROCEDURE,
                    'public.vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)'::REGPROCEDURE,
                    'public.vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])'::REGPROCEDURE,
                    'public.vestrace_reject_result_prepared_pre_dispatch_terminalization()'::REGPROCEDURE,
                    'public.vestrace_assign_embedding_delivery_source_intent()'::REGPROCEDURE,
                    'public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)'::REGPROCEDURE,
                    'public.vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)'::REGPROCEDURE,
                    'public.vestrace_fence_embedding_job_dispatching()'::REGPROCEDURE
                ] LOOP
                    SELECT pg_get_userbyid(proowner) INTO object_owner FROM pg_proc
                     WHERE oid=target_function;
                    IF object_owner='vestrace'
                       OR (object_owner=current_user AND session_user=current_user) THEN
                        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
                    ELSIF object_owner IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                        RAISE EXCEPTION 'P04 result-preparation guarded function ownership was not restored: %',target_function USING ERRCODE='42501';
                    END IF;
                    EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC',target_function);
                END LOOP;
                GRANT SELECT ON TABLE public.embedding_space_corpus_states,
                                      public.embedding_index_generation_guards,
                                      public.embedding_job_result_preparations,
                                      public.embedding_projection_entries,
                                      public.embedding_job_result_prepared_attachments,
                                      public.embedding_projection_source_dependencies,
                                      public.embedding_space_registrations,
                                      public.embedding_delivery_source_memberships
                    TO vestrace;
                GRANT EXECUTE ON FUNCTION public.vestrace_lock_embedding_result_completion_authority(
                    uuid,uuid,uuid,uuid,uuid,uuid,uuid
                ) TO vestrace;
                GRANT EXECUTE ON FUNCTION public.vestrace_load_embedding_result_eligibility(
                    uuid,uuid,uuid
                ) TO vestrace;
                GRANT EXECUTE ON FUNCTION public.vestrace_commit_embedding_result_preparation(
                    uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[]
                ) TO vestrace;
                GRANT EXECUTE ON FUNCTION public.vestrace_lock_embedding_job_recovery_authority(
                    uuid,uuid
                ) TO vestrace;
                IF NOT EXISTS(
                    SELECT 1 FROM pg_trigger
                     WHERE tgrelid='public.embedding_job_pre_dispatch_retirement_authorities'::regclass
                       AND tgname='embedding_result_prepared_pre_dispatch_terminal_fence'
                       AND tgfoid=to_regprocedure('public.vestrace_reject_result_prepared_pre_dispatch_terminalization()')
                ) OR NOT EXISTS(
                    SELECT 1 FROM pg_trigger
                     WHERE tgrelid='public.embedding_output_key_retirement_requests'::regclass
                       AND tgname='embedding_result_prepared_output_retirement_fence'
                       AND tgfoid=to_regprocedure('public.vestrace_reject_result_prepared_pre_dispatch_terminalization()')
                ) OR NOT EXISTS(
                    SELECT 1 FROM pg_trigger
                     WHERE tgrelid='public.embedding_delivery_source_memberships'::regclass
                       AND tgname='embedding_delivery_source_memberships_source_intent_assign'
                       AND tgfoid=to_regprocedure('public.vestrace_assign_embedding_delivery_source_intent()')
                ) OR NOT EXISTS(
                    SELECT 1 FROM pg_trigger
                     WHERE tgrelid='public.embedding_space_registrations'::regclass
                       AND tgname='embedding_result_space_guard_on_registration'
                       AND tgfoid=to_regprocedure('public.vestrace_create_embedding_result_space_guards()')
                ) OR NOT EXISTS(
                    SELECT 1 FROM pg_constraint
                     WHERE conrelid='public.embedding_delivery_source_memberships'::regclass
                       AND conname='embedding_delivery_source_memberships_result_exact_key'
                ) THEN
                    RAISE EXCEPTION 'P04 result-preparation finish requires its exact terminal, fresh-space, and source fences' USING ERRCODE='42501';
                END IF;
                SELECT pg_get_userbyid(proowner) INTO object_owner FROM pg_proc
                 WHERE oid=to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)');
                IF object_owner IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                    RAISE EXCEPTION 'P04 result-preparation recovery authority was not restored' USING ERRCODE='42501';
                END IF;
                REVOKE TRIGGER ON TABLE public.embedding_job_pre_dispatch_retirement_authorities FROM vestrace;
                REVOKE TRIGGER ON TABLE public.embedding_output_key_retirement_requests FROM vestrace;
                REVOKE REFERENCES ON TABLE public.material_erasure_blockers FROM vestrace;
                REVOKE REFERENCES ON TABLE public.prepared_material_attachments FROM vestrace;
                REVOKE EXECUTE ON FUNCTION public.vestrace_finish_p04_result_preparation_upgrade() FROM vestrace;
            END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_result_preparation_upgrade() FROM PUBLIC;
        REVOKE ALL ON FUNCTION public.vestrace_finish_p04_result_preparation_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_result_preparation_upgrade() TO vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_p04_result_preparation_upgrade() TO vestrace;
    END IF;
END
$bootstrap$;

-- P04 0195 owns a distinct one-shot forward upgrade. The existing preparation
-- bridge remains unchanged for historical and freshly installed databases.
DO $finalization_bootstrap$
DECLARE applied BOOLEAN:=false;
BEGIN
 IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
   EXECUTE 'SELECT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=195 AND success)' INTO applied;
 END IF;
 IF applied THEN
   DROP FUNCTION IF EXISTS public.vestrace_prepare_p04_result_finalization_upgrade();
   DROP FUNCTION IF EXISTS public.vestrace_finish_p04_result_finalization_upgrade();
 ELSE
 EXECUTE $function$
 CREATE OR REPLACE FUNCTION public.vestrace_prepare_p04_result_finalization_upgrade()
 RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
 DECLARE item TEXT; target REGPROCEDURE; owner_name TEXT;
 BEGIN
   IF to_regclass('public._sqlx_migrations') IS NOT NULL AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=195 AND success) THEN
     RAISE EXCEPTION 'finalization upgrade already closed' USING ERRCODE='42501';
   END IF;
   FOREACH item IN ARRAY ARRAY['embedding_jobs','embedding_job_result_preparations','embedding_job_result_prepared_attachments','embedding_space_corpus_states','embedding_projection_entries','prepared_material_attachments','embedding_projection_source_dependencies','material_key_creation_intents','content_materials','content_material_bytes','content_material_ordinary_references','material_erasure_blockers','embedding_index_generation_guards','embedding_corpus_generations'] LOOP
     SELECT pg_get_userbyid(relowner) INTO owner_name FROM pg_class WHERE oid=to_regclass('public.'||item);
     IF owner_name IS DISTINCT FROM 'vestrace_guarded_owner' AND owner_name IS DISTINCT FROM 'vestrace' THEN
       RAISE EXCEPTION 'finalization table hand-back unavailable: %',item USING ERRCODE='42501';
     END IF;
     EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace',item);
   END LOOP;
   FOREACH item IN ARRAY ARRAY['vestrace_validate_embedding_credential_completion_owner()','vestrace_ensure_embedding_credential_completion_blocker(uuid,uuid,uuid)','vestrace_adopt_embedding_result_credential_blocker(uuid,uuid,uuid,uuid)','vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)','vestrace_embedding_output_binding_identities(uuid,uuid)','vestrace_embedding_publication_json(uuid,uuid)','vestrace_lock_embedding_result_finalization(uuid,uuid,uuid,uuid)','vestrace_load_embedding_result_finalization(uuid,uuid,uuid,uuid)','vestrace_record_embedding_result_key_binding(uuid,uuid,uuid,uuid,bigint,uuid)','vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])','vestrace_bind_material_key_creation_intent(uuid,uuid)','vestrace_finalize_bound_content_material(uuid)','vestrace_validate_material_key_creation_intent()','vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)','vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)','vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])','vestrace_lock_embedding_job_recovery_authority(uuid,uuid)','vestrace_validate_embedding_projection_dependency()','vestrace_validate_embedding_result_preparation()','vestrace_guard_embedding_projection_publication()','vestrace_guard_embedding_credential_completion_blocker()'] LOOP
     target:=to_regprocedure('public.'||item);
     IF target IS NOT NULL THEN
       SELECT pg_get_userbyid(proowner) INTO owner_name FROM pg_proc WHERE oid=target;
       IF owner_name IS DISTINCT FROM 'vestrace_guarded_owner' AND owner_name IS DISTINCT FROM 'vestrace' THEN
         RAISE EXCEPTION 'finalization function hand-back unavailable: %',item USING ERRCODE='42501';
       END IF;
       EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace',target);
     END IF;
   END LOOP;
   -- Restore baseline REFERENCES after earlier ownership roundtrips.
   GRANT REFERENCES ON TABLE public.credential_revisions,public.credential_key_creation_intents,public.embedding_space_registrations TO vestrace;
   REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_p04_result_finalization_upgrade() FROM vestrace;
 END $body$
 $function$;
 EXECUTE $function$
 CREATE OR REPLACE FUNCTION public.vestrace_finish_p04_result_finalization_upgrade()
 RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
 DECLARE item TEXT; target REGPROCEDURE;
   allowed_targets REGPROCEDURE[] := ARRAY[
     to_regprocedure('public.vestrace_validate_embedding_credential_completion_owner()'),
     to_regprocedure('public.vestrace_ensure_embedding_credential_completion_blocker(uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_adopt_embedding_result_credential_blocker(uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_embedding_output_binding_identities(uuid,uuid)'),
     to_regprocedure('public.vestrace_embedding_publication_json(uuid,uuid)'),
     to_regprocedure('public.vestrace_lock_embedding_result_finalization(uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_load_embedding_result_finalization(uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_record_embedding_result_key_binding(uuid,uuid,uuid,uuid,bigint,uuid)'),
     to_regprocedure('public.vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])'),
     to_regprocedure('public.vestrace_bind_material_key_creation_intent(uuid,uuid)'),
     to_regprocedure('public.vestrace_finalize_bound_content_material(uuid)'),
     to_regprocedure('public.vestrace_validate_material_key_creation_intent()'),
     to_regprocedure('public.vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])'),
     to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)'),
     to_regprocedure('public.vestrace_validate_embedding_projection_dependency()'),
     to_regprocedure('public.vestrace_validate_embedding_result_preparation()'),
     to_regprocedure('public.vestrace_guard_embedding_projection_publication()'),
     to_regprocedure('public.vestrace_guard_embedding_credential_completion_blocker()')
   ];
   runtime_executable_targets REGPROCEDURE[] := ARRAY[
     to_regprocedure('public.vestrace_adopt_embedding_result_credential_blocker(uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_load_embedding_result_finalization(uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_record_embedding_result_key_binding(uuid,uuid,uuid,uuid,bigint,uuid)'),
     to_regprocedure('public.vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])'),
     to_regprocedure('public.vestrace_bind_material_key_creation_intent(uuid,uuid)'),
     to_regprocedure('public.vestrace_finalize_bound_content_material(uuid)'),
     to_regprocedure('public.vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'),
     to_regprocedure('public.vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])'),
     to_regprocedure('public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid)')
   ];
 BEGIN
   IF to_regclass('public._sqlx_migrations') IS NOT NULL AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=195 AND success) THEN
     RAISE EXCEPTION 'finalization finish already closed' USING ERRCODE='42501';
   END IF;

 FOREACH item IN ARRAY ARRAY['embedding_jobs','embedding_job_result_preparations','embedding_job_result_prepared_attachments','embedding_space_corpus_states','embedding_projection_entries','prepared_material_attachments','embedding_projection_source_dependencies','material_key_creation_intents','content_materials','content_material_bytes','content_material_ordinary_references','material_erasure_blockers','embedding_index_generation_guards','embedding_corpus_generations','embedding_job_credential_completion_blockers','embedding_result_credential_blocker_adoptions','embedding_result_key_binding_receipts','embedding_job_result_publications','embedding_index_rebuild_events'] LOOP
   EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace_guarded_owner',item);
   EXECUTE format('REVOKE INSERT,UPDATE,DELETE,TRUNCATE,TRIGGER,REFERENCES ON TABLE public.%I FROM vestrace',item);
   -- Preserve only preexisting reads:0187:419-421,0194:1447-1448, provisioner922-934.
   IF item=ANY(ARRAY['embedding_jobs','embedding_corpus_generations','embedding_space_corpus_states','embedding_index_generation_guards','embedding_job_result_preparations','embedding_projection_entries','embedding_job_result_prepared_attachments','embedding_projection_source_dependencies','material_key_creation_intents','content_materials','content_material_bytes']) THEN EXECUTE format('GRANT SELECT ON TABLE public.%I TO vestrace',item); END IF;
   IF item=ANY(ARRAY['embedding_jobs','embedding_corpus_generations','material_key_creation_intents','content_materials']) THEN EXECUTE format('GRANT REFERENCES ON TABLE public.%I TO vestrace',item); END IF;
 END LOOP;
 FOREACH target IN ARRAY allowed_targets LOOP
   item:=target::text;
   IF target IS NULL THEN RAISE EXCEPTION 'finalization function absent: %',item USING ERRCODE='42501'; END IF;
   EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target);
   EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target);
   IF target=ANY(runtime_executable_targets) THEN EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target); END IF;
 END LOOP;
 -- Restore exact documented0191fallback grant lost in its two-phase owner bridge.
 GRANT EXECUTE ON FUNCTION public.vestrace_publish_embedding_corpus_generation(UUID,UUID,UUID,BIGINT) TO vestrace;

   REVOKE EXECUTE ON FUNCTION public.vestrace_finish_p04_result_finalization_upgrade() FROM vestrace;
 END $body$
 $function$;
 REVOKE ALL ON FUNCTION public.vestrace_prepare_p04_result_finalization_upgrade() FROM PUBLIC;
 REVOKE ALL ON FUNCTION public.vestrace_finish_p04_result_finalization_upgrade() FROM PUBLIC;
 GRANT EXECUTE ON FUNCTION public.vestrace_prepare_p04_result_finalization_upgrade() TO vestrace;
 GRANT EXECUTE ON FUNCTION public.vestrace_finish_p04_result_finalization_upgrade() TO vestrace;
 END IF;
END $finalization_bootstrap$;

-- 0196 repairs only the two existing credential-erasure functions. No table
-- ownership or lifecycle-table access is lent to the runtime migrator.
DO $credential_erasure_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        EXECUTE 'SELECT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=196 AND success)' INTO applied;
    END IF;
    IF applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_retired_credential_erasure_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_retired_credential_erasure_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_retired_credential_erasure_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
        DECLARE target REGPROCEDURE; owner_name TEXT;
            allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_validate_material_erasure()'),
                to_regprocedure('public.vestrace_finalize_credential_material_erasure(uuid,uuid)')
            ];
        BEGIN
            IF to_regclass('public._sqlx_migrations') IS NOT NULL AND EXISTS(
                SELECT 1 FROM public._sqlx_migrations WHERE version=196 AND success
            ) THEN
                RAISE EXCEPTION 'credential erasure upgrade already closed' USING ERRCODE='42501';
            END IF;
            FOREACH target IN ARRAY allowed_targets LOOP
                SELECT pg_get_userbyid(proowner) INTO owner_name FROM pg_proc WHERE oid=target;
                IF target IS NULL OR owner_name IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                    RAISE EXCEPTION 'credential erasure exact function hand-back unavailable' USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace',target);
            END LOOP;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_retired_credential_erasure_upgrade() FROM vestrace;
        END $body$
        $function$;
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_retired_credential_erasure_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $body$
        DECLARE target REGPROCEDURE; owner_name TEXT;
            allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_validate_material_erasure()'),
                to_regprocedure('public.vestrace_finalize_credential_material_erasure(uuid,uuid)')
            ];
            runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_finalize_credential_material_erasure(uuid,uuid)')
            ];
        BEGIN
            IF to_regclass('public._sqlx_migrations') IS NOT NULL AND EXISTS(
                SELECT 1 FROM public._sqlx_migrations WHERE version=196 AND success
            ) THEN
                RAISE EXCEPTION 'credential erasure finish already closed' USING ERRCODE='42501';
            END IF;
            FOREACH target IN ARRAY allowed_targets LOOP
                SELECT pg_get_userbyid(proowner) INTO owner_name FROM pg_proc WHERE oid=target;
                IF target IS NULL OR owner_name IS DISTINCT FROM 'vestrace' THEN
                    RAISE EXCEPTION 'credential erasure exact replacement unavailable' USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target);
                IF target=ANY(runtime_executable_targets) THEN
                    EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target);
                END IF;
            END LOOP;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_retired_credential_erasure_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_retired_credential_erasure_upgrade() FROM PUBLIC;
        REVOKE ALL ON FUNCTION public.vestrace_finish_retired_credential_erasure_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_retired_credential_erasure_upgrade() TO vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_retired_credential_erasure_upgrade() TO vestrace;
    END IF;
END $credential_erasure_bootstrap$;

SQL
