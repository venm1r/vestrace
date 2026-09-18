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

-- 0197 executes a closed table-DDL inventory without lending table ownership.
DO $canonical_generation_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        EXECUTE 'SELECT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=197 AND success)' INTO applied;
    END IF;
    IF applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_canonical_generation_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_canonical_generation_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_canonical_generation_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF to_regclass('public._sqlx_migrations') IS NOT NULL AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=197 AND success) THEN
                RAISE EXCEPTION 'canonical generation upgrade already closed' USING ERRCODE='42501';
            END IF;
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid='public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer)'::regprocedure) IS DISTINCT FROM 'vestrace_guarded_owner'
               OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid='public.vestrace_validate_embedding_corpus_generation_member()'::regprocedure) IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                RAISE EXCEPTION 'canonical generation exact function ownership unavailable' USING ERRCODE='42501';
            END IF;
ALTER TABLE embedding_space_registrations
    ALTER COLUMN space_id DROP NOT NULL,
    DROP CONSTRAINT embedding_space_registrations_tuple_key,
    ADD COLUMN registration_kind TEXT NOT NULL DEFAULT 'legacy_upgrade'
        CHECK(registration_kind IN ('legacy_upgrade','canonical')),
    ADD COLUMN model_revision_id UUID,
    ADD COLUMN model_qualification_revision_id UUID,
    ADD COLUMN request_shape_revision_id UUID,
    ADD COLUMN adapter_profile_revision TEXT,
    ADD COLUMN returned_model TEXT,
    ADD COLUMN encoding_format TEXT,
    ADD CONSTRAINT embedding_space_registration_representation CHECK (
        (registration_kind='legacy_upgrade' AND space_id IS NOT NULL
         AND model_revision_id IS NULL AND model_qualification_revision_id IS NULL
         AND request_shape_revision_id IS NULL AND adapter_profile_revision IS NULL
         AND returned_model IS NULL AND encoding_format IS NULL)
        OR (registration_kind='canonical' AND space_id IS NULL
         AND model_revision_id IS NOT NULL AND model_qualification_revision_id IS NOT NULL
         AND request_shape_revision_id IS NOT NULL AND adapter_profile_revision IS NOT NULL
         AND btrim(adapter_profile_revision)<>'' AND returned_model IS NOT NULL
         AND returned_model=model AND encoding_format IS NOT NULL AND encoding_format='float'));
CREATE UNIQUE INDEX embedding_space_registrations_legacy_tuple_key
    ON embedding_space_registrations(workspace_id,name,model,dimensions)
    WHERE registration_kind='legacy_upgrade';
CREATE UNIQUE INDEX embedding_space_registrations_canonical_tuple_key
    ON embedding_space_registrations(workspace_id,name,model_revision_id,
        model_qualification_revision_id,adapter_profile_revision,request_shape_revision_id,
        returned_model,encoding_format,dimensions) WHERE registration_kind='canonical';
ALTER TABLE model_qualification_heads ADD COLUMN active_space_registration_id UUID;
ALTER TABLE embedding_index_generation_guards
    ADD COLUMN guard_version BIGINT NOT NULL DEFAULT 1 CHECK(guard_version>0),
    ADD COLUMN current_generation_id UUID;
ALTER TABLE embedding_corpus_generations
    DROP CONSTRAINT embedding_corpus_generations_state_check,
    ADD CONSTRAINT embedding_corpus_generations_state_check CHECK(state IN ('building','ready','stale','revoked')),
    ADD COLUMN member_representation TEXT NOT NULL DEFAULT 'legacy_upgrade'
        CHECK(member_representation IN ('legacy_upgrade','encrypted_projection')),
    ADD COLUMN generation_epoch BIGINT,
    ADD COLUMN captured_guard_version BIGINT,
    ADD COLUMN corpus_revision BIGINT,
    ADD COLUMN built_through_projection_ordinal BIGINT,
    ADD CONSTRAINT embedding_generation_capture_complete CHECK (
        (member_representation='legacy_upgrade' AND generation_epoch IS NULL
            AND captured_guard_version IS NULL AND corpus_revision IS NULL
            AND built_through_projection_ordinal IS NULL)
        OR (member_representation='encrypted_projection' AND generation_epoch IS NOT NULL
            AND generation_epoch>0 AND captured_guard_version IS NOT NULL AND captured_guard_version>0
            AND corpus_revision IS NOT NULL AND corpus_revision>=0
            AND built_through_projection_ordinal IS NOT NULL AND built_through_projection_ordinal>=0));
ALTER TABLE embedding_corpus_generations
    ALTER COLUMN published_at DROP NOT NULL,
    ALTER COLUMN published_at DROP DEFAULT,
    ADD COLUMN created_at TIMESTAMPTZ,
    ADD COLUMN state_changed_at TIMESTAMPTZ,
    ADD COLUMN lifecycle_reason TEXT NOT NULL DEFAULT 'legacy_upgrade'
        CHECK(lifecycle_reason IN ('legacy_upgrade','captured','published','invalidated','revoked'));
ALTER TABLE embedding_corpus_generations DISABLE TRIGGER embedding_corpus_generations_guarded;
UPDATE embedding_corpus_generations SET created_at=published_at,state_changed_at=published_at;
SET CONSTRAINTS ALL IMMEDIATE;
SET CONSTRAINTS ALL DEFERRED;
ALTER TABLE embedding_corpus_generations ENABLE TRIGGER embedding_corpus_generations_guarded;
ALTER TABLE embedding_corpus_generations
    ALTER COLUMN created_at SET NOT NULL,
    ALTER COLUMN created_at SET DEFAULT NOW(),
    ALTER COLUMN state_changed_at SET NOT NULL,
    ALTER COLUMN state_changed_at SET DEFAULT NOW();
ALTER TABLE embedding_corpus_generation_members
    DROP CONSTRAINT embedding_corpus_generation_members_pkey,
    ALTER COLUMN memory_embedding_id DROP NOT NULL,
    ADD COLUMN member_ordinal BIGINT,
    ADD COLUMN legacy_embedding_id UUID,
    ADD COLUMN embedding_projection_entry_id UUID;
ALTER TABLE embedding_corpus_generation_members DISABLE TRIGGER embedding_corpus_generation_members_guarded;
WITH ordered AS (
    SELECT workspace_id,corpus_generation_id,memory_embedding_id,
        row_number() OVER(PARTITION BY workspace_id,corpus_generation_id ORDER BY memory_embedding_id) AS ordinal
    FROM embedding_corpus_generation_members
)
UPDATE embedding_corpus_generation_members m SET legacy_embedding_id=m.memory_embedding_id,member_ordinal=o.ordinal
FROM ordered o WHERE o.workspace_id=m.workspace_id AND o.corpus_generation_id=m.corpus_generation_id
    AND o.memory_embedding_id=m.memory_embedding_id;
ALTER TABLE embedding_corpus_generation_members ENABLE TRIGGER embedding_corpus_generation_members_guarded;
ALTER TABLE embedding_corpus_generation_members
    ALTER COLUMN member_ordinal SET NOT NULL,
    ADD PRIMARY KEY(workspace_id,corpus_generation_id,member_ordinal),
    ADD CHECK(member_ordinal>0),
    ADD CONSTRAINT embedding_generation_member_xor CHECK (
        (legacy_embedding_id IS NOT NULL AND embedding_projection_entry_id IS NULL
          AND memory_embedding_id IS NOT DISTINCT FROM legacy_embedding_id)
        OR (legacy_embedding_id IS NULL AND embedding_projection_entry_id IS NOT NULL AND memory_embedding_id IS NULL));
CREATE UNIQUE INDEX embedding_generation_members_legacy_unique
    ON embedding_corpus_generation_members(workspace_id,corpus_generation_id,legacy_embedding_id)
    WHERE legacy_embedding_id IS NOT NULL;
CREATE UNIQUE INDEX embedding_generation_members_projection_unique
    ON embedding_corpus_generation_members(workspace_id,corpus_generation_id,embedding_projection_entry_id)
    WHERE embedding_projection_entry_id IS NOT NULL;
DROP TRIGGER embedding_corpus_generation_members_consistent ON embedding_corpus_generation_members;
ALTER TABLE model_qualification_heads ADD FOREIGN KEY(workspace_id,active_space_registration_id)
    REFERENCES embedding_space_registrations(workspace_id,id) ON DELETE RESTRICT;
ALTER TABLE embedding_index_generation_guards ADD FOREIGN KEY(workspace_id,current_generation_id)
    REFERENCES embedding_corpus_generations(workspace_id,id) ON DELETE RESTRICT;
ALTER TABLE embedding_corpus_generation_members ADD FOREIGN KEY(workspace_id,embedding_projection_entry_id)
    REFERENCES embedding_projection_entries(workspace_id,id) ON DELETE RESTRICT;
ALTER TABLE embedding_space_registrations
    ADD FOREIGN KEY(workspace_id,model_revision_id) REFERENCES model_revisions(workspace_id,id) ON DELETE RESTRICT,
    ADD FOREIGN KEY(workspace_id,model_qualification_revision_id) REFERENCES model_qualification_revisions(workspace_id,id) ON DELETE RESTRICT,
    ADD FOREIGN KEY(workspace_id,request_shape_revision_id) REFERENCES model_request_shape_revisions(workspace_id,id) ON DELETE RESTRICT;

ALTER FUNCTION public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer) OWNER TO vestrace;
ALTER FUNCTION public.vestrace_validate_embedding_corpus_generation_member() OWNER TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_canonical_generation_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_canonical_generation_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_canonical_generation_upgrade() TO vestrace;
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_canonical_generation_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGPROCEDURE;
            allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_register_canonical_embedding_space(uuid,uuid,text,uuid,uuid,uuid,text,text,integer)'),
                to_regprocedure('public.vestrace_set_initial_embedding_active_space(uuid,uuid,bigint,uuid)'),
                to_regprocedure('public.vestrace_capture_embedding_generation(uuid,uuid,uuid,bigint)'),
                to_regprocedure('public.vestrace_publish_embedding_generation(uuid,uuid,uuid,bigint)'),
                to_regprocedure('public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer)'),
                to_regprocedure('public.vestrace_assert_canonical_embedding_space(uuid,uuid)'),
                to_regprocedure('public.vestrace_validate_embedding_active_space()'),
                to_regprocedure('public.vestrace_invalidate_canonical_generation_guard()'),
                to_regprocedure('public.vestrace_set_canonical_generation_lifecycle()'),
                to_regprocedure('public.vestrace_normalize_legacy_generation_member()'),
                to_regprocedure('public.vestrace_validate_embedding_corpus_generation_member()'),
                to_regprocedure('public.vestrace_validate_canonical_generation()'),
                to_regprocedure('public.vestrace_validate_canonical_generation_guard()'),
                to_regprocedure('public.vestrace_validate_canonical_space_registration()'),
                to_regprocedure('public.vestrace_validate_canonical_member_liveness()')
            ];
            runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_register_canonical_embedding_space(uuid,uuid,text,uuid,uuid,uuid,text,text,integer)'),
                to_regprocedure('public.vestrace_set_initial_embedding_active_space(uuid,uuid,bigint,uuid)'),
                to_regprocedure('public.vestrace_capture_embedding_generation(uuid,uuid,uuid,bigint)'),
                to_regprocedure('public.vestrace_publish_embedding_generation(uuid,uuid,uuid,bigint)'),
                to_regprocedure('public.vestrace_register_embedding_space(uuid,uuid,uuid,text,text,integer)')
            ];
        BEGIN
            IF to_regclass('public._sqlx_migrations') IS NOT NULL AND EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=197 AND success) THEN
                RAISE EXCEPTION 'canonical generation upgrade already closed' USING ERRCODE='42501';
            END IF;
CREATE CONSTRAINT TRIGGER model_qualification_heads_active_space_consistent
    AFTER INSERT OR UPDATE ON model_qualification_heads DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_active_space();
CREATE TRIGGER embedding_index_generation_guards_corpus_invalidation
    BEFORE UPDATE ON embedding_index_generation_guards FOR EACH ROW
    EXECUTE FUNCTION vestrace_invalidate_canonical_generation_guard();
CREATE TRIGGER embedding_corpus_generations_lifecycle
    BEFORE INSERT OR UPDATE ON embedding_corpus_generations FOR EACH ROW
    EXECUTE FUNCTION vestrace_set_canonical_generation_lifecycle();
CREATE TRIGGER embedding_corpus_generation_members_legacy_alias
    BEFORE INSERT ON embedding_corpus_generation_members FOR EACH ROW
    EXECUTE FUNCTION vestrace_normalize_legacy_generation_member();
CREATE CONSTRAINT TRIGGER embedding_corpus_generation_members_consistent
    AFTER INSERT OR UPDATE OR DELETE ON embedding_corpus_generation_members DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_corpus_generation_member();
CREATE CONSTRAINT TRIGGER embedding_corpus_generations_canonical_consistent
    AFTER INSERT OR UPDATE OR DELETE ON embedding_corpus_generations DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_generation();
CREATE CONSTRAINT TRIGGER embedding_index_generation_guards_current_consistent
    AFTER INSERT OR UPDATE OR DELETE ON embedding_index_generation_guards DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_generation_guard();
CREATE CONSTRAINT TRIGGER embedding_space_registrations_canonical_consistent
    AFTER INSERT OR UPDATE ON embedding_space_registrations DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_space_registration();
CREATE CONSTRAINT TRIGGER content_materials_canonical_generation_live
    AFTER UPDATE ON content_materials DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_member_liveness();
CREATE CONSTRAINT TRIGGER embedding_projections_canonical_generation_live
    AFTER UPDATE ON embedding_projection_entries DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_canonical_member_liveness();
ALTER TABLE public.memory_embeddings OWNER TO vestrace_guarded_owner;
ALTER TABLE public.memory_embeddings ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.memory_embeddings FORCE ROW LEVEL SECURITY;
REVOKE ALL ON TABLE public.memory_embeddings FROM PUBLIC,vestrace;
            FOREACH target IN ARRAY allowed_targets LOOP
                IF target IS NULL OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target) IS DISTINCT FROM 'vestrace' THEN
                    RAISE EXCEPTION 'canonical generation function handback unavailable' USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target);
                IF target=ANY(runtime_executable_targets) THEN
                    EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target);
                END IF;
            END LOOP;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_canonical_generation_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_canonical_generation_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_canonical_generation_upgrade() TO vestrace;
    END IF;
END $canonical_generation_bootstrap$;

-- Closed Task 4 DDL bridge; no runtime REFERENCES or table-ownership lending.
DO $embedding_index_bootstrap$
DECLARE applied BOOLEAN:=false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        EXECUTE 'SELECT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=198 AND success)' INTO applied;
    END IF;
    IF applied THEN
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_index_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_index_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_index_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=197 AND success)
              OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=198 AND success)
              OR (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='public.embedding_index_rebuild_events'::regclass) IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                RAISE EXCEPTION 'index upgrade requires exact accepted predecessor' USING ERRCODE='42501';
            END IF;
ALTER TABLE embedding_index_rebuild_events
    ALTER COLUMN publication_id DROP NOT NULL,
    ADD COLUMN cause TEXT NOT NULL DEFAULT 'result_publication'
        CHECK(cause IN ('result_publication','material_erasure','transition_publication','legacy_cutover','operator_rebuild')),
    ADD COLUMN material_erasure_preparation_id UUID,
    ADD COLUMN transition_activation_receipt_id UUID,
    ADD COLUMN legacy_cutover_receipt_id UUID,
    ADD COLUMN operator_rebuild_request_id UUID,
    ADD UNIQUE(workspace_id,id),
    ADD CONSTRAINT embedding_index_change_cause_xor CHECK(
        num_nonnulls(publication_id,material_erasure_preparation_id,transition_activation_receipt_id,legacy_cutover_receipt_id,operator_rebuild_request_id)=1
        AND ((cause='result_publication' AND publication_id IS NOT NULL)
          OR (cause='material_erasure' AND material_erasure_preparation_id IS NOT NULL)
          OR (cause='transition_publication' AND transition_activation_receipt_id IS NOT NULL)
          OR (cause='legacy_cutover' AND legacy_cutover_receipt_id IS NOT NULL)
          OR (cause='operator_rebuild' AND operator_rebuild_request_id IS NOT NULL)));
CREATE TABLE embedding_index_build_attempts (
    id UUID PRIMARY KEY,workspace_id UUID NOT NULL,space_registration_id UUID NOT NULL,
    generation_id UUID NOT NULL,event_id UUID,
    cause TEXT NOT NULL CHECK(cause IN ('corpus_change','startup','lazy_load')),
    claim_owner TEXT NOT NULL CHECK(length(claim_owner) BETWEEN 1 AND 128),
    claim_deadline TIMESTAMPTZ NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('claimed','building','published','loaded','discarded','failed')),
    safe_reason TEXT CHECK(safe_reason IN ('invalid_vector','memory_limit','material_unavailable','generation_changed','storage_unavailable','claim_expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),terminal_at TIMESTAMPTZ,
    UNIQUE(workspace_id,id),
    FOREIGN KEY(workspace_id,space_registration_id) REFERENCES embedding_space_registrations(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,generation_id) REFERENCES embedding_corpus_generations(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,event_id) REFERENCES embedding_index_rebuild_events(workspace_id,id) ON DELETE RESTRICT,
    CHECK((cause='corpus_change' AND event_id IS NOT NULL) OR (cause IN ('startup','lazy_load') AND event_id IS NULL)),
    CHECK(claim_deadline>created_at),
    CHECK((state IN ('claimed','building') AND terminal_at IS NULL AND safe_reason IS NULL)
       OR (state IN ('published','loaded') AND terminal_at IS NOT NULL AND safe_reason IS NULL)
       OR (state IN ('discarded','failed') AND terminal_at IS NOT NULL AND safe_reason IS NOT NULL))
);
CREATE INDEX embedding_index_attempt_claims ON embedding_index_build_attempts(workspace_id,space_registration_id,claim_deadline) WHERE state IN ('claimed','building');
CREATE UNIQUE INDEX embedding_index_attempt_active_owner ON embedding_index_build_attempts(workspace_id,generation_id,cause,claim_owner) WHERE state IN ('claimed','building');
CREATE TABLE embedding_index_build_observations (
    id UUID PRIMARY KEY,workspace_id UUID NOT NULL,attempt_id UUID NOT NULL,
    safe_reason TEXT NOT NULL CHECK(safe_reason IN ('invalid_vector','memory_limit','material_unavailable','generation_changed','storage_unavailable','claim_expired')),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id,attempt_id,safe_reason),
    FOREIGN KEY(workspace_id,attempt_id) REFERENCES embedding_index_build_attempts(workspace_id,id) ON DELETE RESTRICT
);

            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_embedding_index_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_index_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_index_upgrade() TO vestrace;
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_index_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGPROCEDURE;allowed_targets REGPROCEDURE[]:=ARRAY[
                to_regprocedure('public.vestrace_validate_embedding_index_event_cause()'),
                to_regprocedure('public.vestrace_embedding_index_snapshot(uuid,uuid)'),
                to_regprocedure('public.vestrace_embedding_index_attempt_plan(uuid,uuid)'),
                to_regprocedure('public.vestrace_validate_embedding_index_attempt()'),
                to_regprocedure('public.vestrace_claim_embedding_index_build(uuid,text,integer)'),
                to_regprocedure('public.vestrace_validate_local_embedding_generation(uuid,uuid,bigint,bigint,bigint,bigint,bigint)'),
                to_regprocedure('public.vestrace_claim_embedding_index_load(uuid,uuid,bigint,bigint,bigint,bigint,bigint,text)'),
                to_regprocedure('public.vestrace_lock_embedding_index_attempt(uuid,uuid,text)'),
                to_regprocedure('public.vestrace_load_embedding_index_chunk(uuid,uuid,text,bigint,integer)'),
                to_regprocedure('public.vestrace_publish_embedding_index_build(uuid,uuid,text)'),
                to_regprocedure('public.vestrace_finish_embedding_index_attempt(uuid,uuid,text,text,text)'),
                to_regprocedure('public.vestrace_observe_embedding_index_attempt(uuid,uuid,text,text)')];runtime_executable_targets REGPROCEDURE[]:=ARRAY[
                to_regprocedure('public.vestrace_claim_embedding_index_build(uuid,text,integer)'),
                to_regprocedure('public.vestrace_validate_local_embedding_generation(uuid,uuid,bigint,bigint,bigint,bigint,bigint)'),
                to_regprocedure('public.vestrace_claim_embedding_index_load(uuid,uuid,bigint,bigint,bigint,bigint,bigint,text)'),
                to_regprocedure('public.vestrace_load_embedding_index_chunk(uuid,uuid,text,bigint,integer)'),
                to_regprocedure('public.vestrace_publish_embedding_index_build(uuid,uuid,text)'),
                to_regprocedure('public.vestrace_finish_embedding_index_attempt(uuid,uuid,text,text,text)'),
                to_regprocedure('public.vestrace_observe_embedding_index_attempt(uuid,uuid,text,text)')];
        BEGIN
            FOREACH target IN ARRAY allowed_targets LOOP
                IF target IS NULL OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target)<>'vestrace' THEN
                    RAISE EXCEPTION 'index finish requires exact runtime-created functions' USING ERRCODE='42501';
                END IF;
            END LOOP;
CREATE CONSTRAINT TRIGGER embedding_index_rebuild_event_cause_consistent AFTER INSERT OR UPDATE ON embedding_index_rebuild_events
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_index_event_cause();
CREATE CONSTRAINT TRIGGER embedding_index_attempt_consistent AFTER INSERT OR UPDATE OR DELETE ON embedding_index_build_attempts
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_index_attempt();
ALTER TABLE public.embedding_index_build_attempts OWNER TO vestrace_guarded_owner;
ALTER TABLE public.embedding_index_build_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.embedding_index_build_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_index_build_attempts_workspace_policy ON public.embedding_index_build_attempts
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
REVOKE ALL ON TABLE public.embedding_index_build_attempts FROM PUBLIC,vestrace;
GRANT SELECT ON TABLE public.embedding_index_build_attempts TO vestrace;
CREATE TRIGGER embedding_index_build_attempts_guarded BEFORE INSERT OR UPDATE OR DELETE ON public.embedding_index_build_attempts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
ALTER TABLE public.embedding_index_build_observations OWNER TO vestrace_guarded_owner;
ALTER TABLE public.embedding_index_build_observations ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.embedding_index_build_observations FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_index_build_observations_workspace_policy ON public.embedding_index_build_observations
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
REVOKE ALL ON TABLE public.embedding_index_build_observations FROM PUBLIC,vestrace;
GRANT SELECT ON TABLE public.embedding_index_build_observations TO vestrace;
CREATE TRIGGER embedding_index_build_observations_guarded BEFORE INSERT OR UPDATE OR DELETE ON public.embedding_index_build_observations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER embedding_index_build_observations_immutable BEFORE UPDATE OR DELETE ON embedding_index_build_observations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
            FOREACH target IN ARRAY allowed_targets LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target);
                IF target=ANY(runtime_executable_targets) THEN EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target); END IF;
            END LOOP;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_index_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_index_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_index_upgrade() TO vestrace;
    END IF;
END $embedding_index_bootstrap$;

-- Task 5 needs to forward-replace guarded delivery-only validators.  The
-- prepare/finish pair grants the migration process only the temporary
-- ownership necessary for that exact DDL, then restores guarded ownership.
DO $embedding_executor_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        EXECUTE 'SELECT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=199 AND success)' INTO applied;
    END IF;
    IF applied THEN
        IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='public.embedding_job_work_claims'::regclass) IS DISTINCT FROM 'vestrace_guarded_owner'
           OR NOT (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE oid='public.embedding_job_work_claims'::regclass)
           OR NOT has_function_privilege('vestrace','public.vestrace_claim_embedding_work(uuid,text,text,integer)'::regprocedure,'EXECUTE')
           OR NOT has_function_privilege('vestrace','public.vestrace_finish_embedding_work(uuid,uuid,text,text,text)'::regprocedure,'EXECUTE')
           OR NOT has_function_privilege('vestrace','public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)'::regprocedure,'EXECUTE')
           OR has_function_privilege('public','public.vestrace_claim_embedding_work(uuid,text,text,integer)'::regprocedure,'EXECUTE')
           OR has_function_privilege('public','public.vestrace_finish_embedding_work(uuid,uuid,text,text,text)'::regprocedure,'EXECUTE') THEN
            RAISE EXCEPTION 'embedding executor owner or runtime ACL posture is unavailable' USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_executor_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_executor_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_executor_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGPROCEDURE;
        BEGIN
            IF EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=199 AND success) THEN
                RAISE EXCEPTION 'embedding executor upgrade already closed' USING ERRCODE='42501';
            END IF;
            ALTER TABLE public.embedding_delivery_acceptance_receipts OWNER TO vestrace;
            FOR target IN
                SELECT procedure.oid::regprocedure FROM pg_proc AS procedure
                JOIN pg_namespace AS namespace ON namespace.oid=procedure.pronamespace
                WHERE namespace.nspname='public' AND procedure.proname=ANY(ARRAY[
                    'vestrace_begin_delivery_embedding_outputs',
                    'vestrace_finalize_delivery_embedding_outputs',
                    'vestrace_validate_embedding_credential_completion_owner',
                    'vestrace_ensure_embedding_credential_completion_blocker',
                    'vestrace_adopt_embedding_result_credential_blocker',
                    'vestrace_assert_embedding_result_phase',
                    'vestrace_lock_embedding_result_finalization',
                    'vestrace_load_embedding_result_finalization',
                    'vestrace_record_embedding_result_key_binding',
                    'vestrace_publish_embedding_job_result',
                    'vestrace_load_embedding_result_eligibility',
                    'vestrace_lock_embedding_result_completion_authority',
                    'vestrace_commit_embedding_result_preparation',
                    'vestrace_lock_embedding_job_recovery_authority',
                    'vestrace_validate_embedding_projection_dependency',
                    'vestrace_validate_embedding_result_preparation',
                    'vestrace_claim_embedding_work',
                    'vestrace_finish_embedding_work'
                ])
            LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace',target);
            END LOOP;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_executor_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_executor_upgrade() TO vestrace;
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_executor_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGPROCEDURE;
        allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_claim_embedding_work(uuid,text,text,integer)'),
                to_regprocedure('public.vestrace_finish_embedding_work(uuid,uuid,text,text,text)')
            ]::REGPROCEDURE[];
        runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_claim_embedding_work(uuid,text,text,integer)'),
                to_regprocedure('public.vestrace_finish_embedding_work(uuid,uuid,text,text,text)')
            ]::REGPROCEDURE[];
        BEGIN
            ALTER TABLE public.embedding_delivery_acceptance_receipts OWNER TO vestrace_guarded_owner;
            ALTER TABLE public.embedding_job_work_claims ENABLE ROW LEVEL SECURITY;
            ALTER TABLE public.embedding_job_work_claims FORCE ROW LEVEL SECURITY;
            CREATE POLICY embedding_job_work_claims_workspace_policy ON public.embedding_job_work_claims
                USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
                WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
            REVOKE ALL ON TABLE public.embedding_job_work_claims FROM PUBLIC,vestrace;
            CREATE TRIGGER embedding_job_work_claims_guarded
                BEFORE INSERT OR UPDATE OR DELETE ON public.embedding_job_work_claims
                FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
            ALTER TABLE public.embedding_job_work_claims OWNER TO vestrace_guarded_owner;
            -- Restored, not assumed. The REVOKE above names `vestrace`, which
            -- owned this table at that moment, and revoking from an owner
            -- removes its own ACL entry rather than leaving a default one. The
            -- ownership transfer then had an empty ACL to carry, so the guarded
            -- owner ended with no privilege on the table its own SECURITY
            -- DEFINER function must insert into -- and every dispatch claim
            -- failed with 42501 from the first one.
            GRANT ALL ON TABLE public.embedding_job_work_claims TO vestrace_guarded_owner;
            FOR target IN
                SELECT procedure.oid::regprocedure FROM pg_proc AS procedure
                JOIN pg_namespace AS namespace ON namespace.oid=procedure.pronamespace
                WHERE namespace.nspname='public' AND procedure.proname=ANY(ARRAY[
                    'vestrace_begin_delivery_embedding_outputs',
                    'vestrace_finalize_delivery_embedding_outputs',
                    'vestrace_validate_embedding_credential_completion_owner',
                    'vestrace_ensure_embedding_credential_completion_blocker',
                    'vestrace_adopt_embedding_result_credential_blocker',
                    'vestrace_assert_embedding_result_phase',
                    'vestrace_lock_embedding_result_finalization',
                    'vestrace_load_embedding_result_finalization',
                    'vestrace_record_embedding_result_key_binding',
                    'vestrace_publish_embedding_job_result',
                    'vestrace_load_embedding_result_eligibility',
                    'vestrace_lock_embedding_result_completion_authority',
                    'vestrace_commit_embedding_result_preparation',
                    'vestrace_lock_embedding_job_recovery_authority',
                    'vestrace_validate_embedding_projection_dependency',
                    'vestrace_validate_embedding_result_preparation',
                    -- The two work functions 0199 created. The prepare helper
                    -- above lends both to `vestrace` for the migration, and
                    -- this list -- the one that hands ownership back -- named
                    -- neither, so on every provisioned database both stayed
                    -- owned by the role the REVOKE below strips of all
                    -- privilege on the claim table. A SECURITY DEFINER function
                    -- owned by that role cannot touch the table it exists to
                    -- write: `vestrace_finish_embedding_work` failed with 42501
                    -- on its first call and on every call since, so a worker
                    -- could claim work and never retire the claim, every lease
                    -- ran its full sixty seconds, and no outcome was ever
                    -- recorded. `vestrace_claim_embedding_work` had the same
                    -- fault and was repaired incidentally by 0205's own
                    -- hand-back, which is why claiming worked and finishing did
                    -- not -- and why nothing noticed.
                    'vestrace_claim_embedding_work',
                    'vestrace_finish_embedding_work'
                ])
            LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target);
            END LOOP;
            FOREACH target IN ARRAY allowed_targets LOOP
                IF target IS NULL THEN
                    RAISE EXCEPTION 'executor finish requires exact runtime-created functions'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC',target);
            END LOOP;
            FOREACH target IN ARRAY runtime_executable_targets LOOP
                EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target);
            END LOOP;
            GRANT EXECUTE ON FUNCTION public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_finalize_delivery_embedding_outputs(uuid,uuid,uuid) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_adopt_embedding_result_credential_blocker(uuid,uuid,uuid,uuid) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_load_embedding_result_finalization(uuid,uuid,uuid,uuid) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_record_embedding_result_key_binding(uuid,uuid,uuid,uuid,bigint,uuid) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[]) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_load_embedding_result_eligibility(uuid,uuid,uuid) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[]) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_lock_embedding_job_recovery_authority(uuid,uuid) TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_executor_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_executor_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_executor_upgrade() TO vestrace;
    END IF;
END $embedding_executor_bootstrap$;

-- Task 6 temporarily lends the runtime migrator only the guarded transition
-- tables whose schema changes in 0200.  The finish bridge checks every new
-- table/function before returning it to the guarded owner, so fresh and
-- 0199-to-0200 upgrades end with identical ownership and ACL posture.
DO $transition_execution_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=200 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                'embedding_transitions',
                'embedding_transition_plans',
                'embedding_transition_plan_recipes',
                'embedding_transition_ambiguity_carry_recipes',
                'embedding_transition_barrier_recipes',
                'embedding_transition_batches',
                'embedding_transition_batch_recipes',
                'embedding_transition_recipe_dependencies',
                'embedding_transition_job_attempts',
                'embedding_transition_recipe_satisfactions',
                'embedding_transition_observations'
              ]::TEXT[]) AS required(relname)
             WHERE to_regclass('public.'||required.relname) IS NULL
                OR (SELECT pg_get_userbyid(relowner) FROM pg_class
                     WHERE oid=to_regclass('public.'||required.relname))<>'vestrace_guarded_owner'
                OR NOT (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class
                         WHERE oid=to_regclass('public.'||required.relname))
                OR NOT has_table_privilege('vestrace','public.'||required.relname,'SELECT,REFERENCES')
        ) OR EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                to_regprocedure('public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'),
                to_regprocedure('public.vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint)'),
                to_regprocedure('public.vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint)')
              ]::REGPROCEDURE[]) AS required(target)
             WHERE required.target IS NULL
                OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=required.target)<>'vestrace_guarded_owner'
                OR NOT has_function_privilege('vestrace',required.target,'EXECUTE')
                OR has_function_privilege('public',required.target,'EXECUTE')
        ) THEN
            RAISE EXCEPTION 'embedding transition execution owner or runtime ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_transition_execution_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_transition_execution_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_transition_execution_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS;
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=199 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=200 AND success) THEN
                RAISE EXCEPTION 'transition execution upgrade requires exact accepted 0199 predecessor'
                    USING ERRCODE='42501';
            END IF;
            FOREACH target IN ARRAY ARRAY[
                'embedding_transitions'::REGCLASS,
                'embedding_transition_plans'::REGCLASS,
                'embedding_transition_plan_recipes'::REGCLASS,
                'embedding_transition_ambiguity_carry_recipes'::REGCLASS,
                'embedding_transition_barrier_recipes'::REGCLASS
            ] LOOP
                IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=target)
                   IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                    RAISE EXCEPTION 'transition execution upgrade table owner is unavailable'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace',target);
            END LOOP;
            GRANT SELECT, REFERENCES ON TABLE public.embedding_transition_plans,
                public.embedding_projection_entries, public.material_erasure_blockers TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_embedding_transition_execution_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_transition_execution_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_transition_execution_upgrade() TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_transition_execution_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS; target_function REGPROCEDURE;
        allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_derive_embedding_transition_carry_recipe_identity()'),
                to_regprocedure('public.vestrace_derive_embedding_transition_barrier_recipe_identity()'),
                to_regprocedure('public.vestrace_materialize_embedding_transition_batch()'),
                to_regprocedure('public.vestrace_materialize_embedding_transition_batch_recipe()'),
                to_regprocedure('public.vestrace_guard_embedding_transition_header()'),
                to_regprocedure('public.vestrace_guard_embedding_transition_batch_header()'),
                to_regprocedure('public.vestrace_guard_embedding_transition_batch_recipe()'),
                to_regprocedure('public.vestrace_validate_embedding_transition_bijection(uuid,uuid,boolean)'),
                to_regprocedure('public.vestrace_validate_embedding_transition_bijection_trigger()'),
                to_regprocedure('public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'),
                to_regprocedure('public.vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint)'),
                to_regprocedure('public.vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint)')
            ]::REGPROCEDURE[];
        runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_derive_embedding_transition_carry_recipe_identity()'),
                to_regprocedure('public.vestrace_derive_embedding_transition_barrier_recipe_identity()'),
                to_regprocedure('public.vestrace_materialize_embedding_transition_batch()'),
                to_regprocedure('public.vestrace_materialize_embedding_transition_batch_recipe()'),
                to_regprocedure('public.vestrace_guard_embedding_transition_header()'),
                to_regprocedure('public.vestrace_guard_embedding_transition_batch_header()'),
                to_regprocedure('public.vestrace_guard_embedding_transition_batch_recipe()'),
                to_regprocedure('public.vestrace_validate_embedding_transition_bijection(uuid,uuid,boolean)'),
                to_regprocedure('public.vestrace_validate_embedding_transition_bijection_trigger()'),
                to_regprocedure('public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'),
                to_regprocedure('public.vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint)'),
                to_regprocedure('public.vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint)')
            ]::REGPROCEDURE[];
        BEGIN
            FOREACH target_function IN ARRAY allowed_targets LOOP
                IF target_function IS NULL
                   OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target_function)<>'vestrace' THEN
                    RAISE EXCEPTION 'transition execution function hand-back is unavailable'
                        USING ERRCODE='42501';
                END IF;
            END LOOP;
            FOREACH target IN ARRAY ARRAY[
                'embedding_transitions'::REGCLASS,
                'embedding_transition_plans'::REGCLASS,
                'embedding_transition_plan_recipes'::REGCLASS,
                'embedding_transition_ambiguity_carry_recipes'::REGCLASS,
                'embedding_transition_barrier_recipes'::REGCLASS,
                'embedding_transition_batches'::REGCLASS,
                'embedding_transition_batch_recipes'::REGCLASS,
                'embedding_transition_recipe_dependencies'::REGCLASS,
                'embedding_transition_job_attempts'::REGCLASS,
                'embedding_transition_recipe_satisfactions'::REGCLASS,
                'embedding_transition_observations'::REGCLASS
            ] LOOP
                IF to_regclass(format('public.%s',target::TEXT)) IS NULL THEN
                    RAISE EXCEPTION 'transition execution guarded relation is absent'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace',target);
                EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace',target);
            END LOOP;
            FOREACH target_function IN ARRAY runtime_executable_targets LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target_function);
            END LOOP;
            REVOKE REFERENCES ON TABLE public.embedding_transition_plans,
                public.embedding_projection_entries, public.material_erasure_blockers FROM vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint) TO vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint) TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_transition_execution_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_transition_execution_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_transition_execution_upgrade() TO vestrace;
    END IF;
END $transition_execution_bootstrap$;

DO $transition_activation_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=201 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF EXISTS(
            SELECT 1
              FROM unnest(ARRAY['embedding_transition_activation_receipts']) AS required(relname)
             WHERE to_regclass('public.'||required.relname) IS NULL
                OR (SELECT pg_get_userbyid(relowner) FROM pg_class
                     WHERE oid=to_regclass('public.'||required.relname))<>'vestrace_guarded_owner'
                OR NOT (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class
                         WHERE oid=to_regclass('public.'||required.relname))
                OR NOT has_table_privilege('vestrace','public.'||required.relname,'SELECT,REFERENCES')
                OR has_table_privilege('vestrace','public.'||required.relname,'INSERT,UPDATE,DELETE')
        ) OR EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                to_regprocedure('public.vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid)')
              ]::REGPROCEDURE[]) AS required(target)
             WHERE required.target IS NULL
                OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=required.target)<>'vestrace_guarded_owner'
                OR NOT has_function_privilege('vestrace',required.target,'EXECUTE')
                OR has_function_privilege('public',required.target,'EXECUTE')
        ) THEN
            RAISE EXCEPTION 'embedding transition activation owner or runtime ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_transition_activation_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_transition_activation_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_transition_activation_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS;
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=200 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=201 AND success) THEN
                RAISE EXCEPTION 'transition activation upgrade requires exact accepted 0200 predecessor'
                    USING ERRCODE='42501';
            END IF;
            FOREACH target IN ARRAY ARRAY[
                'embedding_transitions'::REGCLASS,
                'model_qualification_heads'::REGCLASS
            ] LOOP
                IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid=target)
                   IS DISTINCT FROM 'vestrace_guarded_owner' THEN
                    RAISE EXCEPTION 'transition activation upgrade table owner is unavailable'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace',target);
            END LOOP;
            GRANT REFERENCES ON TABLE public.workspaces, public.audit_events,
                public.embedding_space_registrations, public.embedding_transitions TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_embedding_transition_activation_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_transition_activation_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_transition_activation_upgrade() TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_transition_activation_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS; target_function REGPROCEDURE;
        allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_guard_embedding_activation_receipt()'),
                to_regprocedure('public.vestrace_require_embedding_activation_receipt()'),
                to_regprocedure('public.vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid)')
            ]::REGPROCEDURE[];
        runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_guard_embedding_activation_receipt()'),
                to_regprocedure('public.vestrace_require_embedding_activation_receipt()'),
                to_regprocedure('public.vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid)')
            ]::REGPROCEDURE[];
        BEGIN
            FOREACH target_function IN ARRAY allowed_targets LOOP
                IF target_function IS NULL
                   OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target_function)<>'vestrace' THEN
                    RAISE EXCEPTION 'transition activation function hand-back is unavailable'
                        USING ERRCODE='42501';
                END IF;
            END LOOP;
            FOREACH target IN ARRAY ARRAY[
                'embedding_transitions'::REGCLASS,
                'model_qualification_heads'::REGCLASS,
                'embedding_transition_activation_receipts'::REGCLASS
            ] LOOP
                IF to_regclass(format('public.%s',target::TEXT)) IS NULL THEN
                    RAISE EXCEPTION 'transition activation guarded relation is absent'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace',target);
                EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace',target);
            END LOOP;
            FOREACH target_function IN ARRAY runtime_executable_targets LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target_function);
            END LOOP;
            REVOKE REFERENCES ON TABLE public.workspaces, public.audit_events,
                public.embedding_space_registrations FROM vestrace;
            GRANT EXECUTE ON FUNCTION
                public.vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid) TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_transition_activation_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_transition_activation_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_transition_activation_upgrade() TO vestrace;
    END IF;
END $transition_activation_bootstrap$;

DO $retrieval_results_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=202 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                'embedding_retrieval_fences',
                'embedding_retrieval_results',
                'embedding_retrieval_result_references',
                'embedding_retrieval_generation_changes',
                'embedding_retrieval_retry_edges'
              ]) AS required(relname)
             WHERE to_regclass('public.'||required.relname) IS NULL
                OR (SELECT pg_get_userbyid(relowner) FROM pg_class
                     WHERE oid=to_regclass('public.'||required.relname))<>'vestrace_guarded_owner'
                OR NOT (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class
                         WHERE oid=to_regclass('public.'||required.relname))
                OR NOT has_table_privilege('vestrace','public.'||required.relname,'SELECT,REFERENCES')
                OR has_table_privilege('vestrace','public.'||required.relname,'INSERT,UPDATE,DELETE')
        ) OR EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                to_regprocedure('public.vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'),
                to_regprocedure('public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'),
                to_regprocedure('public.vestrace_observe_embedding_retrieval_generation_change(uuid,uuid,uuid,text)'),
                to_regprocedure('public.vestrace_authorize_embedding_retrieval_retry(uuid,uuid,uuid,uuid,text)')
              ]::REGPROCEDURE[]) AS required(target)
             WHERE required.target IS NULL
                OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=required.target)<>'vestrace_guarded_owner'
                OR (has_function_privilege('vestrace',required.target,'EXECUTE') IS DISTINCT FROM (NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=208 AND success) OR required.target NOT IN ('vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'::regprocedure,'vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'::regprocedure)))
                OR has_function_privilege('public',required.target,'EXECUTE')
        ) THEN
            RAISE EXCEPTION 'embedding retrieval results owner or runtime ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_retrieval_results_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_retrieval_results_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_retrieval_results_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=201 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=202 AND success) THEN
                RAISE EXCEPTION 'retrieval results upgrade requires exact accepted 0201 predecessor'
                    USING ERRCODE='42501';
            END IF;
            GRANT REFERENCES ON TABLE public.workspaces, public.embedding_jobs,
                public.embedding_space_registrations, public.embedding_corpus_generations
                TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_embedding_retrieval_results_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_retrieval_results_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_retrieval_results_upgrade() TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_retrieval_results_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS; target_function REGPROCEDURE;
        allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'),
                to_regprocedure('public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'),
                to_regprocedure('public.vestrace_observe_embedding_retrieval_generation_change(uuid,uuid,uuid,text)'),
                to_regprocedure('public.vestrace_authorize_embedding_retrieval_retry(uuid,uuid,uuid,uuid,text)')
            ]::REGPROCEDURE[];
        runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'),
                to_regprocedure('public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'),
                to_regprocedure('public.vestrace_observe_embedding_retrieval_generation_change(uuid,uuid,uuid,text)'),
                to_regprocedure('public.vestrace_authorize_embedding_retrieval_retry(uuid,uuid,uuid,uuid,text)')
            ]::REGPROCEDURE[];
        BEGIN
            FOREACH target_function IN ARRAY allowed_targets LOOP
                IF target_function IS NULL
                   OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target_function)<>'vestrace' THEN
                    RAISE EXCEPTION 'retrieval results function hand-back is unavailable'
                        USING ERRCODE='42501';
                END IF;
            END LOOP;
            FOREACH target IN ARRAY ARRAY[
                'embedding_retrieval_fences'::REGCLASS,
                'embedding_retrieval_results'::REGCLASS,
                'embedding_retrieval_result_references'::REGCLASS,
                'embedding_retrieval_generation_changes'::REGCLASS,
                'embedding_retrieval_retry_edges'::REGCLASS
            ] LOOP
                IF to_regclass(format('public.%s',target::TEXT)) IS NULL THEN
                    RAISE EXCEPTION 'retrieval results guarded relation is absent'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace',target);
                EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace',target);
            END LOOP;
            FOREACH target_function IN ARRAY runtime_executable_targets LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target_function);
                EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target_function);
            END LOOP;
            REVOKE REFERENCES ON TABLE public.workspaces, public.embedding_jobs,
                public.embedding_space_registrations, public.embedding_corpus_generations
                FROM vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_retrieval_results_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_retrieval_results_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_retrieval_results_upgrade() TO vestrace;
    END IF;
END $retrieval_results_bootstrap$;

DO $erasure_propagation_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=203 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                'embedding_erasure_propagations',
                'embedding_erasure_revoked_members'
              ]) AS required(relname)
             WHERE to_regclass('public.'||required.relname) IS NULL
                OR (SELECT pg_get_userbyid(relowner) FROM pg_class
                     WHERE oid=to_regclass('public.'||required.relname))<>'vestrace_guarded_owner'
                OR NOT (SELECT relrowsecurity AND relforcerowsecurity FROM pg_class
                         WHERE oid=to_regclass('public.'||required.relname))
                OR NOT has_table_privilege('vestrace','public.'||required.relname,'SELECT,REFERENCES')
                OR has_table_privilege('vestrace','public.'||required.relname,'INSERT,UPDATE,DELETE')
        ) OR EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                to_regprocedure('public.vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)')
              ]::REGPROCEDURE[]) AS required(target)
             WHERE required.target IS NULL
                OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=required.target)<>'vestrace_guarded_owner'
                OR NOT has_function_privilege('vestrace',required.target,'EXECUTE')
                OR has_function_privilege('public',required.target,'EXECUTE')
        ) OR (SELECT pg_get_userbyid(relowner) FROM pg_class
               WHERE oid='public.embedding_projection_entries'::REGCLASS)<>'vestrace_guarded_owner'
          OR NOT has_table_privilege('vestrace','public.embedding_projection_entries','SELECT')
          OR has_table_privilege('vestrace','public.embedding_projection_entries','INSERT,UPDATE,DELETE')
          OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
               WHERE oid=to_regprocedure(
                   'public.vestrace_guard_embedding_projection_publication()'))<>'vestrace_guarded_owner'
          OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
               WHERE oid=to_regprocedure(
                   'public.vestrace_prepare_content_material_erasure(uuid)'))<>'vestrace_guarded_owner'
          OR NOT has_function_privilege('vestrace',
               'public.vestrace_prepare_content_material_erasure(uuid)','EXECUTE')
          OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
               WHERE oid=to_regprocedure(
                   'public.vestrace_validate_embedding_projection_dependency()'))
              <>'vestrace_guarded_owner'
          OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
               WHERE oid=to_regprocedure(
                   'public.vestrace_finalize_content_material_erasure(uuid,uuid)'))
              <>'vestrace_guarded_owner'
          OR NOT has_function_privilege('vestrace',
               'public.vestrace_finalize_content_material_erasure(uuid,uuid)','EXECUTE')
        THEN
            RAISE EXCEPTION 'embedding erasure propagation owner or runtime ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_erasure_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_erasure_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_erasure_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=202 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=203 AND success) THEN
                RAISE EXCEPTION 'erasure propagation upgrade requires exact accepted 0202 predecessor'
                    USING ERRCODE='42501';
            END IF;
            GRANT REFERENCES ON TABLE public.workspaces TO vestrace;
            -- 0203 forward-replaces the 0195 publication validator for the
            -- post-erasure branch, and CREATE OR REPLACE requires ownership.
            -- Lent for the migration and handed straight back below.
            ALTER FUNCTION public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)
                OWNER TO vestrace;
            -- 0203 also replaces the corpus-change stream's blanket growth rule
            -- with a cause-specific one, and ALTER TABLE needs ownership.
            ALTER TABLE public.embedding_index_rebuild_events OWNER TO vestrace;
            -- And the cause guard 0198 left refusing every cause but result
            -- publication, which this migration teaches to admit erasure.
            ALTER FUNCTION public.vestrace_validate_embedding_index_event_cause()
                OWNER TO vestrace;
            -- 0203 gives a projection a retired phase and teaches 0195's
            -- projection guard to admit exactly that one further transition.
            -- Both need ownership; both are handed straight back below, with
            -- the table's standing SELECT restored rather than assumed.
            ALTER TABLE public.embedding_projection_entries OWNER TO vestrace;
            ALTER FUNCTION public.vestrace_guard_embedding_projection_publication()
                OWNER TO vestrace;
            -- And 0174's content erasure primitive, whose ordinary-reference
            -- test refused every embedding vector by construction. 0203 widens
            -- it by one alternative so a retired projection's ciphertext can be
            -- erased at all.
            ALTER FUNCTION public.vestrace_prepare_content_material_erasure(uuid)
                OWNER TO vestrace;
            ALTER FUNCTION public.vestrace_finalize_content_material_erasure(uuid,uuid)
                OWNER TO vestrace;
            -- And 0194's dependency validator, which refuses a projection whose
            -- source has left Live -- the invariant the retirement satisfies.
            ALTER FUNCTION public.vestrace_validate_embedding_projection_dependency()
                OWNER TO vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_embedding_erasure_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_erasure_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_erasure_upgrade() TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_erasure_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS; target_function REGPROCEDURE;
        allowed_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)')
            ]::REGPROCEDURE[];
        runtime_executable_targets REGPROCEDURE[] := ARRAY[
                to_regprocedure('public.vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)')
            ]::REGPROCEDURE[];
        BEGIN
            FOREACH target_function IN ARRAY allowed_targets LOOP
                IF target_function IS NULL
                   OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target_function)<>'vestrace' THEN
                    RAISE EXCEPTION 'erasure propagation function hand-back is unavailable'
                        USING ERRCODE='42501';
                END IF;
            END LOOP;
            FOREACH target IN ARRAY ARRAY[
                'embedding_erasure_propagations'::REGCLASS,
                'embedding_erasure_revoked_members'::REGCLASS
            ] LOOP
                IF to_regclass(format('public.%s',target::TEXT)) IS NULL THEN
                    RAISE EXCEPTION 'erasure propagation guarded relation is absent'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace',target);
                EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace',target);
            END LOOP;
            FOREACH target_function IN ARRAY runtime_executable_targets LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target_function);
                EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target_function);
            END LOOP;
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)'))
               <>'vestrace' THEN
                RAISE EXCEPTION 'publication validator hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_assert_embedding_result_phase(uuid,uuid,uuid,uuid)
                OWNER TO vestrace_guarded_owner;
            IF (SELECT pg_get_userbyid(relowner) FROM pg_class
                 WHERE oid='public.embedding_index_rebuild_events'::REGCLASS)<>'vestrace' THEN
                RAISE EXCEPTION 'corpus-change stream hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_validate_embedding_index_event_cause()'))<>'vestrace' THEN
                RAISE EXCEPTION 'corpus-change cause guard hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_validate_embedding_index_event_cause()
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON FUNCTION public.vestrace_validate_embedding_index_event_cause()
                FROM PUBLIC,vestrace;
            ALTER TABLE public.embedding_index_rebuild_events
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON TABLE public.embedding_index_rebuild_events FROM PUBLIC,vestrace;
            GRANT SELECT, REFERENCES ON TABLE public.embedding_index_rebuild_events TO vestrace;
            IF (SELECT pg_get_userbyid(relowner) FROM pg_class
                 WHERE oid='public.embedding_projection_entries'::REGCLASS)<>'vestrace'
               OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
                    WHERE oid=to_regprocedure(
                        'public.vestrace_guard_embedding_projection_publication()'))<>'vestrace'
            THEN
                RAISE EXCEPTION 'projection retirement hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_guard_embedding_projection_publication()
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON FUNCTION public.vestrace_guard_embedding_projection_publication()
                FROM PUBLIC,vestrace;
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_prepare_content_material_erasure(uuid)'))<>'vestrace' THEN
                RAISE EXCEPTION 'content erasure primitive hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_validate_embedding_projection_dependency()'))<>'vestrace'
            THEN
                RAISE EXCEPTION 'projection dependency validator hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_validate_embedding_projection_dependency()
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON FUNCTION public.vestrace_validate_embedding_projection_dependency()
                FROM PUBLIC,vestrace;
            ALTER FUNCTION public.vestrace_prepare_content_material_erasure(uuid)
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON FUNCTION public.vestrace_prepare_content_material_erasure(uuid)
                FROM PUBLIC;
            -- Standing since 0174: the runtime role calls this on the ordinary
            -- erasure path, so the hand-back restores EXECUTE rather than
            -- leaving the primitive unreachable.
            GRANT EXECUTE ON FUNCTION public.vestrace_prepare_content_material_erasure(uuid)
                TO vestrace;
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_finalize_content_material_erasure(uuid,uuid)'))<>'vestrace'
            THEN
                RAISE EXCEPTION 'content erasure finalizer hand-back is unavailable'
                    USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_finalize_content_material_erasure(uuid,uuid)
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON FUNCTION public.vestrace_finalize_content_material_erasure(uuid,uuid)
                FROM PUBLIC;
            GRANT EXECUTE ON FUNCTION public.vestrace_finalize_content_material_erasure(uuid,uuid)
                TO vestrace;
            ALTER TABLE public.embedding_projection_entries OWNER TO vestrace_guarded_owner;
            GRANT ALL ON TABLE public.embedding_projection_entries TO vestrace_guarded_owner;
            -- 0195's exact posture, restored rather than a blanket revoke: this
            -- table carries a standing SELECT and no REFERENCES, and stripping
            -- the read would break every path that hydrates a projection.
            REVOKE INSERT,UPDATE,DELETE,TRUNCATE,TRIGGER,REFERENCES
                ON TABLE public.embedding_projection_entries FROM PUBLIC,vestrace;
            GRANT SELECT ON TABLE public.embedding_projection_entries TO vestrace;
            REVOKE REFERENCES ON TABLE public.workspaces FROM vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_erasure_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_erasure_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_erasure_upgrade() TO vestrace;
    END IF;
END $erasure_propagation_bootstrap$;

DO $legacy_adoption_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=204 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                'embedding_legacy_adoptions',
                'embedding_legacy_adoption_members',
                'embedding_legacy_adoption_blockers',
                'embedding_legacy_cutover_receipts',
                'embedding_legacy_identity_tombstones',
                'embedding_legacy_retirement_gate'
              ]) AS required(relname)
             WHERE to_regclass('public.'||required.relname) IS NULL
                OR (SELECT pg_get_userbyid(relowner) FROM pg_class
                     WHERE oid=to_regclass('public.'||required.relname))<>'vestrace_guarded_owner'
                OR NOT has_table_privilege('vestrace','public.'||required.relname,'SELECT,REFERENCES')
                OR has_table_privilege('vestrace','public.'||required.relname,'INSERT,UPDATE,DELETE')
        ) OR EXISTS(
            SELECT 1
              FROM unnest(ARRAY[
                to_regprocedure('public.vestrace_start_or_resume_legacy_adoption(uuid,uuid,uuid,uuid,text)'),
                to_regprocedure('public.vestrace_bind_legacy_adoption_source(uuid,uuid,bigint,uuid,uuid)'),
                to_regprocedure('public.vestrace_bind_legacy_adoption_rebuild(uuid,uuid,bigint,uuid)'),
                to_regprocedure('public.vestrace_satisfy_legacy_adoption_member(uuid,uuid,bigint,uuid)'),
                to_regprocedure('public.vestrace_record_legacy_adoption_blocker(uuid,uuid,bigint,text)'),
                to_regprocedure('public.vestrace_prove_legacy_adoption_ready(uuid,uuid,uuid)'),
                to_regprocedure('public.vestrace_commit_legacy_adoption_cutover(uuid,uuid,uuid,bigint,uuid)'),
                to_regprocedure('public.vestrace_commit_legacy_plaintext_retirement()')
              ]::REGPROCEDURE[]) AS required(target)
             WHERE required.target IS NULL
                OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=required.target)<>'vestrace_guarded_owner'
                OR NOT has_function_privilege('vestrace',required.target,'EXECUTE')
                OR has_function_privilege('public',required.target,'EXECUTE')
        ) OR NOT has_table_privilege('vestrace_guarded_owner','public.memories','SELECT') THEN
            RAISE EXCEPTION 'embedding legacy adoption owner or runtime ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_legacy_adoption_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_legacy_adoption_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_legacy_adoption_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=202 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=204 AND success) THEN
                RAISE EXCEPTION 'legacy adoption upgrade requires exact accepted 0202 predecessor'
                    USING ERRCODE='42501';
            END IF;
            GRANT REFERENCES ON TABLE public.workspaces, public.embedding_space_registrations,
                public.embedding_corpus_generations, public.content_materials,
                public.embedding_projection_entries
                TO vestrace;
            GRANT SELECT ON TABLE public.memories TO vestrace_guarded_owner;
            REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_embedding_legacy_adoption_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_legacy_adoption_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_legacy_adoption_upgrade() TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_legacy_adoption_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        DECLARE target REGCLASS; target_function REGPROCEDURE;
        allowed_targets REGPROCEDURE[] := ARRAY[
            to_regprocedure('public.vestrace_start_or_resume_legacy_adoption(uuid,uuid,uuid,uuid,text)'),
            to_regprocedure('public.vestrace_bind_legacy_adoption_source(uuid,uuid,bigint,uuid,uuid)'),
            to_regprocedure('public.vestrace_bind_legacy_adoption_rebuild(uuid,uuid,bigint,uuid)'),
            to_regprocedure('public.vestrace_satisfy_legacy_adoption_member(uuid,uuid,bigint,uuid)'),
            to_regprocedure('public.vestrace_record_legacy_adoption_blocker(uuid,uuid,bigint,text)'),
            to_regprocedure('public.vestrace_prove_legacy_adoption_ready(uuid,uuid,uuid)'),
            to_regprocedure('public.vestrace_commit_legacy_adoption_cutover(uuid,uuid,uuid,bigint,uuid)'),
            to_regprocedure('public.vestrace_commit_legacy_plaintext_retirement()')];
        runtime_executable_targets REGPROCEDURE[] := ARRAY[
            to_regprocedure('public.vestrace_start_or_resume_legacy_adoption(uuid,uuid,uuid,uuid,text)'),
            to_regprocedure('public.vestrace_bind_legacy_adoption_source(uuid,uuid,bigint,uuid,uuid)'),
            to_regprocedure('public.vestrace_bind_legacy_adoption_rebuild(uuid,uuid,bigint,uuid)'),
            to_regprocedure('public.vestrace_satisfy_legacy_adoption_member(uuid,uuid,bigint,uuid)'),
            to_regprocedure('public.vestrace_record_legacy_adoption_blocker(uuid,uuid,bigint,text)'),
            to_regprocedure('public.vestrace_prove_legacy_adoption_ready(uuid,uuid,uuid)'),
            to_regprocedure('public.vestrace_commit_legacy_adoption_cutover(uuid,uuid,uuid,bigint,uuid)'),
            to_regprocedure('public.vestrace_commit_legacy_plaintext_retirement()')];
        BEGIN
            FOREACH target_function IN ARRAY allowed_targets LOOP
                IF target_function IS NULL
                   OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=target_function)<>'vestrace' THEN
                    RAISE EXCEPTION 'legacy adoption function hand-back is unavailable'
                        USING ERRCODE='42501';
                END IF;
            END LOOP;
            FOREACH target IN ARRAY ARRAY[
                'embedding_legacy_adoptions'::REGCLASS,
                'embedding_legacy_adoption_members'::REGCLASS,
                'embedding_legacy_adoption_blockers'::REGCLASS,
                'embedding_legacy_cutover_receipts'::REGCLASS,
                'embedding_legacy_identity_tombstones'::REGCLASS,
                'embedding_legacy_retirement_gate'::REGCLASS
            ] LOOP
                IF to_regclass(format('public.%s',target::TEXT)) IS NULL THEN
                    RAISE EXCEPTION 'legacy adoption guarded relation is absent'
                        USING ERRCODE='42501';
                END IF;
                EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',target);
                EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',target);
                EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace',target);
                EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace',target);
            END LOOP;
            FOREACH target_function IN ARRAY runtime_executable_targets LOOP
                EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
                EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target_function);
                EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',target_function);
            END LOOP;
            -- Only the two grants this upgrade actually added are given back.
            -- content_materials, embedding_space_registrations and
            -- embedding_corpus_generations carry a standing REFERENCES grant
            -- from earlier migrations, and revoking those here would silently
            -- strip a privilege this migration never owned.
            REVOKE REFERENCES ON TABLE public.workspaces,
                public.embedding_projection_entries
                FROM vestrace;
            REVOKE EXECUTE ON FUNCTION public.vestrace_finish_embedding_legacy_adoption_upgrade()
                FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_legacy_adoption_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_legacy_adoption_upgrade() TO vestrace;
    END IF;
END $legacy_adoption_bootstrap$;

DO $retrieval_dispatch_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=205 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
             WHERE oid=to_regprocedure(
                 'public.vestrace_claim_embedding_work(uuid,text,text,integer)'))
           <>'vestrace_guarded_owner'
           OR NOT has_function_privilege('vestrace',
                'public.vestrace_claim_embedding_work(uuid,text,text,integer)','EXECUTE')
           OR has_function_privilege('public',
                'public.vestrace_claim_embedding_work(uuid,text,text,integer)','EXECUTE')
        THEN
            RAISE EXCEPTION 'embedding retrieval dispatch owner or runtime ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_retrieval_dispatch_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_retrieval_dispatch_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_retrieval_dispatch_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=204 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=205 AND success) THEN
                RAISE EXCEPTION 'retrieval dispatch upgrade requires exact accepted 0204 predecessor'
                    USING ERRCODE='42501';
            END IF;
            -- 0205 forward-replaces 0199's work claim so a fenced retrieval
            -- query becomes claimable, and CREATE OR REPLACE requires
            -- ownership. Lent for the migration and handed straight back below.
            ALTER FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
                OWNER TO vestrace;
            REVOKE EXECUTE ON FUNCTION
                public.vestrace_prepare_embedding_retrieval_dispatch_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_retrieval_dispatch_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_retrieval_dispatch_upgrade()
            TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_retrieval_dispatch_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_claim_embedding_work(uuid,text,text,integer)'))<>'vestrace'
            THEN
                RAISE EXCEPTION 'work claim hand-back is unavailable' USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
                OWNER TO vestrace_guarded_owner;
            REVOKE ALL ON FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
                FROM PUBLIC;
            -- Standing since 0199: the worker calls this every cycle, so the
            -- hand-back restores EXECUTE rather than leaving it unreachable.
            GRANT EXECUTE ON FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
                TO vestrace;
            REVOKE EXECUTE ON FUNCTION
                public.vestrace_finish_embedding_retrieval_dispatch_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_retrieval_dispatch_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_retrieval_dispatch_upgrade()
            TO vestrace;
    END IF;
END $retrieval_dispatch_bootstrap$;

DO $transition_header_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=206 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
             WHERE oid=to_regprocedure(
                 'public.vestrace_guard_embedding_transition_header()'))
           <>'vestrace_guarded_owner'
           OR has_function_privilege('vestrace',
                'public.vestrace_guard_embedding_transition_header()','EXECUTE')
           OR has_function_privilege('public',
                'public.vestrace_guard_embedding_transition_header()','EXECUTE')
        THEN
            RAISE EXCEPTION 'embedding transition header owner or ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_transition_header_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_transition_header_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_transition_header_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=205 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=206 AND success) THEN
                RAISE EXCEPTION 'transition header upgrade requires exact accepted 0205 predecessor'
                    USING ERRCODE='42501';
            END IF;
            -- 0206 forward-replaces 0200's header guard so the moves 0201 and
            -- 0203 write become admissible, and CREATE OR REPLACE requires
            -- ownership. Lent for the migration and handed straight back below.
            ALTER FUNCTION public.vestrace_guard_embedding_transition_header()
                OWNER TO vestrace;
            REVOKE EXECUTE ON FUNCTION
                public.vestrace_prepare_embedding_transition_header_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_transition_header_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_transition_header_upgrade()
            TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_transition_header_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_guard_embedding_transition_header()'))<>'vestrace'
            THEN
                RAISE EXCEPTION 'transition header hand-back is unavailable' USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_guard_embedding_transition_header()
                OWNER TO vestrace_guarded_owner;
            -- A trigger function is never called by name, so 0200 granted it no
            -- EXECUTE and the hand-back restores none.
            REVOKE ALL ON FUNCTION public.vestrace_guard_embedding_transition_header()
                FROM PUBLIC,vestrace;
            REVOKE EXECUTE ON FUNCTION
                public.vestrace_finish_embedding_transition_header_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_transition_header_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_transition_header_upgrade()
            TO vestrace;
    END IF;
END $transition_header_bootstrap$;

DO $delivery_rebuild_xor_bootstrap$
DECLARE applied BOOLEAN := false;
BEGIN
    IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM public._sqlx_migrations WHERE version=207 AND success
        ) INTO applied;
    END IF;
    IF applied THEN
        IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
             WHERE oid=to_regprocedure(
                 'public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'))
           <>'vestrace_guarded_owner'
           OR NOT has_function_privilege('vestrace',
                'public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)','EXECUTE')
           OR has_function_privilege('public',
                'public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)','EXECUTE')
           OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
                WHERE oid=to_regprocedure(
                    'public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)'))
              <>'vestrace_guarded_owner'
           OR NOT has_function_privilege('vestrace',
                'public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)','EXECUTE')
           OR has_function_privilege('public',
                'public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)','EXECUTE')
        THEN
            RAISE EXCEPTION 'embedding delivery/rebuild XOR owner or ACL posture is unavailable'
                USING ERRCODE='42501';
        END IF;
        DROP FUNCTION IF EXISTS public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade();
        DROP FUNCTION IF EXISTS public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade();
    ELSE
        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=206 AND success)
               OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=207 AND success) THEN
                RAISE EXCEPTION 'delivery/rebuild XOR upgrade requires exact accepted 0206 predecessor'
                    USING ERRCODE='42501';
            END IF;
            -- 0207 forward-replaces both ends of the delivery/rebuild XOR, and
            -- CREATE OR REPLACE requires ownership of each. Lent for the
            -- migration and handed straight back below.
            ALTER FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
                uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) OWNER TO vestrace;
            ALTER FUNCTION public.vestrace_begin_delivery_embedding_outputs(
                uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
                OWNER TO vestrace;
            REVOKE EXECUTE ON FUNCTION
                public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_embedding_delivery_rebuild_xor_upgrade()
            TO vestrace;

        EXECUTE $function$
        CREATE OR REPLACE FUNCTION public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade()
        RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
        BEGIN
            IF (SELECT pg_get_userbyid(proowner) FROM pg_proc
                 WHERE oid=to_regprocedure(
                     'public.vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'))<>'vestrace'
               OR (SELECT pg_get_userbyid(proowner) FROM pg_proc
                    WHERE oid=to_regprocedure(
                        'public.vestrace_begin_delivery_embedding_outputs(uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)'))<>'vestrace'
            THEN
                RAISE EXCEPTION 'delivery/rebuild XOR hand-back is unavailable' USING ERRCODE='42501';
            END IF;
            ALTER FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
                uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)
                OWNER TO vestrace_guarded_owner;
            ALTER FUNCTION public.vestrace_begin_delivery_embedding_outputs(
                uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
                OWNER TO vestrace_guarded_owner;
            -- Both are called by name from the runtime role, so unlike the
            -- header guard the hand-back restores their exact EXECUTE grant.
            REVOKE ALL ON FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
                uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) FROM PUBLIC,vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_create_embedding_transition_batch_attempt(
                uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) TO vestrace;
            REVOKE ALL ON FUNCTION public.vestrace_begin_delivery_embedding_outputs(
                uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
                FROM PUBLIC,vestrace;
            GRANT EXECUTE ON FUNCTION public.vestrace_begin_delivery_embedding_outputs(
                uuid,uuid,uuid,text,uuid,uuid,text,uuid,uuid,uuid,uuid,bigint,jsonb,jsonb)
                TO vestrace;
            REVOKE EXECUTE ON FUNCTION
                public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade() FROM vestrace;
        END $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade()
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_delivery_rebuild_xor_upgrade()
            TO vestrace;
    END IF;
END $delivery_rebuild_xor_bootstrap$;


DO $memory_references_bootstrap$
DECLARE applied BOOLEAN:=false; item REGPROCEDURE;
BEGIN
 IF to_regclass('public._sqlx_migrations') IS NOT NULL THEN
  SELECT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=208 AND success) INTO applied;
 END IF;
 IF applied THEN
  FOREACH item IN ARRAY ARRAY[
    to_regprocedure('vestrace_issue_canonical_retrieval_snapshot(uuid,uuid,uuid,uuid)'),
    to_regprocedure('vestrace_resolve_embedding_memory_references(uuid,uuid,uuid[])'),
    to_regprocedure('vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,uuid,timestamptz)'),
    to_regprocedure('vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],uuid[],uuid[],bigint[],double precision[])')
  ] LOOP
   IF item IS NULL OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid=item)<>'vestrace_guarded_owner'
    OR NOT has_function_privilege('vestrace',item,'EXECUTE') OR has_function_privilege('public',item,'EXECUTE') THEN
    RAISE EXCEPTION 'embedding memory reference authority posture is unavailable' USING ERRCODE='42501'; END IF;
  END LOOP;
  IF (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid='vestrace_validate_canonical_member_liveness()'::regprocedure)<>'vestrace_guarded_owner'
   OR has_function_privilege('vestrace','vestrace_validate_canonical_member_liveness()','EXECUTE')
   OR has_function_privilege('public','vestrace_validate_canonical_member_liveness()','EXECUTE') THEN
   RAISE EXCEPTION 'canonical member retirement trigger posture is unavailable' USING ERRCODE='42501'; END IF;
  DROP FUNCTION IF EXISTS vestrace_prepare_embedding_memory_references_upgrade();
  DROP FUNCTION IF EXISTS vestrace_finish_embedding_memory_references_upgrade();
 ELSE
  EXECUTE $function$
  CREATE OR REPLACE FUNCTION vestrace_prepare_embedding_memory_references_upgrade()
  RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
  DECLARE item REGPROCEDURE; relation REGCLASS;
  BEGIN
   IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=207 AND success)
    OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=208 AND success) THEN
    RAISE EXCEPTION 'memory references upgrade requires exact 0207 predecessor' USING ERRCODE='42501'; END IF;
   FOREACH relation IN ARRAY ARRAY['embedding_retrieval_results'::regclass,'embedding_retrieval_result_references'::regclass,'embedding_retrieval_fences'::regclass] LOOP
    EXECUTE format('ALTER TABLE %s OWNER TO vestrace',relation);
   END LOOP;
   FOREACH item IN ARRAY ARRAY[
    'vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'::regprocedure,
    'vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'::regprocedure,
    'vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)'::regprocedure,
    'vestrace_finalize_bound_content_material(uuid)'::regprocedure,
    'vestrace_prepare_content_material_erasure(uuid)'::regprocedure,
    'vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)'::regprocedure,
    'vestrace_fence_embedding_job_dispatching()'::regprocedure,
    'vestrace_validate_canonical_member_liveness()'::regprocedure
   ] LOOP
    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace',item);
   END LOOP;
   REVOKE EXECUTE ON FUNCTION vestrace_prepare_embedding_memory_references_upgrade() FROM vestrace;
  END $body$
  $function$;
  REVOKE ALL ON FUNCTION vestrace_prepare_embedding_memory_references_upgrade() FROM PUBLIC;
  GRANT EXECUTE ON FUNCTION vestrace_prepare_embedding_memory_references_upgrade() TO vestrace;
  EXECUTE $function$
  CREATE OR REPLACE FUNCTION vestrace_finish_embedding_memory_references_upgrade()
  RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $body$
  DECLARE
   item REGPROCEDURE; relation REGCLASS;
   allowed_targets REGPROCEDURE[] := ARRAY[
    'public.vestrace_issue_canonical_retrieval_snapshot(uuid,uuid,uuid,uuid)'::regprocedure,
    'public.vestrace_resolve_embedding_memory_references(uuid,uuid,uuid[])'::regprocedure,
    'public.vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,uuid,timestamptz)'::regprocedure,
    'public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],uuid[],uuid[],bigint[],double precision[])'::regprocedure,
    'public.vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)'::regprocedure,
    'public.vestrace_finalize_bound_content_material(uuid)'::regprocedure,
    'public.vestrace_prepare_content_material_erasure(uuid)'::regprocedure,
    'public.vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)'::regprocedure,
    'public.vestrace_fence_embedding_job_dispatching()'::regprocedure,
    'public.vestrace_validate_canonical_member_liveness()'::regprocedure,
    'public.vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'::regprocedure,
    'public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'::regprocedure
   ];
   runtime_executable_targets REGPROCEDURE[] := ARRAY[
    'public.vestrace_issue_canonical_retrieval_snapshot(uuid,uuid,uuid,uuid)'::regprocedure,
    'public.vestrace_resolve_embedding_memory_references(uuid,uuid,uuid[])'::regprocedure,
    'public.vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,uuid,timestamptz)'::regprocedure,
    'public.vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],uuid[],uuid[],bigint[],double precision[])'::regprocedure,
    'public.vestrace_propagate_embedding_source_erasure(uuid,uuid,uuid)'::regprocedure,
    'public.vestrace_finalize_bound_content_material(uuid)'::regprocedure,
    'public.vestrace_prepare_content_material_erasure(uuid)'::regprocedure
   ];
  BEGIN
   IF NOT EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=207 AND success)
    OR EXISTS(SELECT 1 FROM public._sqlx_migrations WHERE version=208 AND success) THEN
    RAISE EXCEPTION 'memory references hand-back requires exact 0207 predecessor' USING ERRCODE='42501'; END IF;
   FOREACH relation IN ARRAY ARRAY['embedding_retrieval_results'::regclass,'embedding_retrieval_result_references'::regclass,'embedding_retrieval_fences'::regclass] LOOP
    EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',relation);
    EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',relation);
    EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace',relation);
    EXECUTE format('GRANT SELECT,REFERENCES ON TABLE %s TO vestrace',relation);
   END LOOP;
   GRANT SELECT,UPDATE ON memories,memory_revisions TO vestrace_guarded_owner;
   FOREACH item IN ARRAY allowed_targets LOOP
    EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',item);
    EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',item);
    IF item = ANY(runtime_executable_targets) THEN
     EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace',item);
    END IF;
   END LOOP;
   REVOKE EXECUTE ON FUNCTION vestrace_finish_embedding_memory_references_upgrade() FROM vestrace;
  END $body$
  $function$;
  REVOKE ALL ON FUNCTION vestrace_finish_embedding_memory_references_upgrade() FROM PUBLIC;
  GRANT EXECUTE ON FUNCTION vestrace_finish_embedding_memory_references_upgrade() TO vestrace;
 END IF;
END $memory_references_bootstrap$;

SQL
# P05 safety supervisor role — do not move
: "${VESTRACE_SAFETY_SUPERVISOR_PASSWORD:?VESTRACE_SAFETY_SUPERVISOR_PASSWORD is required}"

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 \
  --set=safety_supervisor_password="$VESTRACE_SAFETY_SUPERVISOR_PASSWORD" <<'SQL'
SELECT 'CREATE ROLE vestrace_safety_supervisor LOGIN'
WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'vestrace_safety_supervisor')
\gexec

ALTER ROLE vestrace_safety_supervisor WITH
  LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS
  PASSWORD :'safety_supervisor_password';

CREATE EXTENSION IF NOT EXISTS vestrace_safety_verify;
ALTER FUNCTION public.vestrace_safety_ed25519_verify(BYTEA, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_safety_ed25519_verify(BYTEA, BYTEA, BYTEA) FROM PUBLIC, vestrace, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_safety_ed25519_verify(BYTEA, BYTEA, BYTEA) TO vestrace_guarded_owner;

CREATE OR REPLACE FUNCTION public.vestrace_install_p05_safety_schema()
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $installer$
BEGIN
  IF session_user <> 'vestrace_bootstrap' AND NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = session_user), FALSE) THEN
    RAISE EXCEPTION 'P05 safety installation requires bootstrap custody' USING ERRCODE = '42501';
  END IF;
  IF EXISTS (SELECT 1 FROM pg_stat_activity WHERE usename = 'vestrace' AND pid <> pg_backend_pid()) THEN
    RAISE EXCEPTION 'P05 safety installation requires quiesced runtime sessions' USING ERRCODE = '55006';
  END IF;
  ALTER SCHEMA public OWNER TO vestrace_guarded_owner;
  REVOKE CREATE ON SCHEMA public FROM PUBLIC, vestrace;
  GRANT USAGE ON SCHEMA public TO vestrace;
  IF has_schema_privilege('vestrace', 'public', 'CREATE') THEN
    RAISE EXCEPTION 'P05 safety installation failed to remove runtime schema CREATE' USING ERRCODE = '42501';
  END IF;

  CREATE TABLE IF NOT EXISTS public.installation_safety_state (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    installation_id UUID NOT NULL UNIQUE,
    fingerprint_key_id UUID NOT NULL,
    fingerprint_continuity_proof BYTEA NOT NULL CHECK (octet_length(fingerprint_continuity_proof) = 32),
    journal_signer_public_key BYTEA NOT NULL CHECK (octet_length(journal_signer_public_key) = 32),
    witness_public_key BYTEA NOT NULL CHECK (octet_length(witness_public_key) = 32),
    witness_sequence BIGINT NOT NULL CHECK (witness_sequence >= 0),
    journal_digest BYTEA NOT NULL CHECK (octet_length(journal_digest) = 32),
    witness_state BYTEA NOT NULL,
    witness_state_digest BYTEA NOT NULL CHECK (octet_length(witness_state_digest) = 32),
    active_generation_id UUID NOT NULL,
    activation_epoch BIGINT NOT NULL CHECK (activation_epoch >= 0),
    supervisor_workspace_id UUID NOT NULL,
    supervisor_principal_id UUID NOT NULL
  );
  CREATE TABLE IF NOT EXISTS public.installation_safety_generations (
    installation_id UUID NOT NULL REFERENCES public.installation_safety_state(installation_id),
    generation_id UUID NOT NULL,
    activation_epoch BIGINT NOT NULL CHECK (activation_epoch >= 0),
    journal_sequence BIGINT NOT NULL CHECK (journal_sequence >= 0),
    journal_digest BYTEA NOT NULL CHECK (octet_length(journal_digest) = 32),
    PRIMARY KEY (installation_id, generation_id),
    UNIQUE (installation_id, activation_epoch),
    UNIQUE (installation_id, journal_sequence)
  );
  CREATE TABLE IF NOT EXISTS public.installation_safety_journal_events (
    installation_id UUID NOT NULL REFERENCES public.installation_safety_state(installation_id),
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    request_id UUID NOT NULL,
    event_kind SMALLINT NOT NULL CHECK (event_kind IN (0, 1)),
    generation_id UUID NOT NULL,
    activation_epoch BIGINT NOT NULL CHECK (activation_epoch >= 0),
    previous_digest BYTEA NOT NULL CHECK (octet_length(previous_digest) = 32),
    journal_digest BYTEA NOT NULL CHECK (octet_length(journal_digest) = 32),
    journal_signature BYTEA NOT NULL CHECK (octet_length(journal_signature) = 64),
    PRIMARY KEY (installation_id, sequence),
    UNIQUE (installation_id, request_id),
    UNIQUE (installation_id, journal_digest)
  );
  ALTER TABLE public.installation_safety_state OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.installation_safety_generations OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.installation_safety_journal_events OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.installation_safety_state ENABLE ROW LEVEL SECURITY;
  ALTER TABLE public.installation_safety_state FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.installation_safety_generations ENABLE ROW LEVEL SECURITY;
  ALTER TABLE public.installation_safety_generations FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.installation_safety_journal_events ENABLE ROW LEVEL SECURITY;
  ALTER TABLE public.installation_safety_journal_events FORCE ROW LEVEL SECURITY;
  IF NOT EXISTS (SELECT 1 FROM pg_policy WHERE polrelid = 'public.installation_safety_state'::REGCLASS AND polname = 'installation_safety_state_guarded_owner_only') THEN
    CREATE POLICY installation_safety_state_guarded_owner_only
      ON public.installation_safety_state
      TO vestrace_guarded_owner
      USING (TRUE) WITH CHECK (TRUE);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_policy WHERE polrelid = 'public.installation_safety_generations'::REGCLASS AND polname = 'installation_safety_generations_guarded_owner_only') THEN
    CREATE POLICY installation_safety_generations_guarded_owner_only
      ON public.installation_safety_generations
      TO vestrace_guarded_owner
      USING (TRUE) WITH CHECK (TRUE);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_policy WHERE polrelid = 'public.installation_safety_journal_events'::REGCLASS AND polname = 'installation_safety_journal_events_guarded_owner_only') THEN
    CREATE POLICY installation_safety_journal_events_guarded_owner_only
      ON public.installation_safety_journal_events
      TO vestrace_guarded_owner
      USING (TRUE) WITH CHECK (TRUE);
  END IF;
  REVOKE ALL ON TABLE public.installation_safety_state, public.installation_safety_generations, public.installation_safety_journal_events FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  EXECUTE $p05_context$
    CREATE OR REPLACE FUNCTION public.vestrace_assert_installation_supervisor_context()
    RETURNS VOID
    LANGUAGE plpgsql
    SECURITY DEFINER
    SET search_path = pg_catalog, public
    AS $context$
    BEGIN
      IF session_user <> 'vestrace_safety_supervisor'
         OR current_setting('vestrace.workspace_id', TRUE) IS DISTINCT FROM '00000000-0000-0000-0000-000000000005'
         OR current_setting('vestrace.principal_id', TRUE) IS DISTINCT FROM '00000000-0000-0000-0000-000000000006' THEN
        RAISE EXCEPTION 'P05 installation supervisor context is required' USING ERRCODE = '42501';
      END IF;
    END
    $context$;
  $p05_context$;
  ALTER FUNCTION public.vestrace_assert_installation_supervisor_context() OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_assert_installation_supervisor_context() FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  EXECUTE $p05_helpers$
    CREATE OR REPLACE FUNCTION public.vestrace_p05_field(value BYTEA)
    RETURNS BYTEA LANGUAGE sql IMMUTABLE STRICT
    SET search_path = pg_catalog
    AS $$ SELECT int8send(octet_length(value)::BIGINT) || value $$;

    CREATE OR REPLACE FUNCTION public.vestrace_p05_receipt_payload(
      installation UUID, fingerprint UUID, proof BYTEA, sequence BIGINT,
      journal BYTEA, generation UUID, epoch BIGINT, state BYTEA, witness_key BYTEA
    ) RETURNS BYTEA LANGUAGE sql IMMUTABLE STRICT SET search_path = pg_catalog, public AS $$
      SELECT public.vestrace_p05_field(convert_to('vestrace-installation-witness-receipt-v1', 'UTF8'))
        || public.vestrace_p05_field(uuid_send(installation))
        || public.vestrace_p05_field(uuid_send(fingerprint))
        || public.vestrace_p05_field(proof)
        || int8send(sequence)
        || public.vestrace_p05_field(journal)
        || public.vestrace_p05_field(uuid_send(generation))
        || int8send(epoch)
        || public.vestrace_p05_field(state)
        || public.vestrace_p05_field(witness_key)
    $$;

    CREATE OR REPLACE FUNCTION public.vestrace_p05_journal_payload(
      installation UUID, fingerprint UUID, proof BYTEA, request UUID, sequence BIGINT,
      previous BYTEA, event_kind SMALLINT, generation UUID, epoch BIGINT, state BYTEA,
      journal_key BYTEA
    ) RETURNS BYTEA LANGUAGE sql IMMUTABLE STRICT SET search_path = pg_catalog, public AS $$
      SELECT public.vestrace_p05_field(convert_to('vestrace-installation-safety-journal-v1', 'UTF8'))
        || public.vestrace_p05_field(uuid_send(installation))
        || public.vestrace_p05_field(uuid_send(fingerprint))
        || public.vestrace_p05_field(proof)
        || public.vestrace_p05_field(uuid_send(request))
        || int8send(sequence)
        || public.vestrace_p05_field(previous)
        || set_byte(decode('00', 'hex'), 0, event_kind)
        || public.vestrace_p05_field(uuid_send(generation))
        || int8send(epoch)
        || public.vestrace_p05_field(state)
        || public.vestrace_p05_field(journal_key)
    $$;
  $p05_helpers$;
  ALTER FUNCTION public.vestrace_p05_field(BYTEA) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_p05_receipt_payload(UUID, UUID, BYTEA, BIGINT, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_p05_journal_payload(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, SMALLINT, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_p05_field(BYTEA), public.vestrace_p05_receipt_payload(UUID, UUID, BYTEA, BIGINT, BYTEA, UUID, BIGINT, BYTEA, BYTEA), public.vestrace_p05_journal_payload(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, SMALLINT, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  EXECUTE $p05_mutations$
    CREATE OR REPLACE FUNCTION public.vestrace_initialize_installation_safety(
      p_bootstrap_digest BYTEA, p_installation UUID, p_fingerprint UUID, p_proof BYTEA,
      p_request UUID, p_journal_key BYTEA, p_witness_key BYTEA, p_sequence BIGINT,
      p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID,
      p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
    ) RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $init$
    DECLARE
      expected_bootstrap BYTEA;
      previous BYTEA := decode('fffed62a80e302ee8dc20e064cc576f7fb945ae8097fb44d61d6be6a217e2bbe', 'hex');
      journal_payload BYTEA;
      expected_journal BYTEA;
    BEGIN
      PERFORM public.vestrace_assert_installation_supervisor_context();
      IF octet_length(p_bootstrap_digest) <> 32 OR octet_length(p_proof) <> 32
         OR octet_length(p_journal_key) <> 32 OR octet_length(p_witness_key) <> 32
         OR octet_length(p_journal_digest) <> 32 OR octet_length(p_journal_signature) <> 64
         OR octet_length(p_receipt_signature) <> 64 OR p_sequence <> 1 OR p_epoch <> 0 THEN
        RAISE EXCEPTION 'P05 initializer has malformed or out-of-order authority data' USING ERRCODE = '22023';
      END IF;
      expected_bootstrap := digest(
        public.vestrace_p05_field(convert_to('vestrace-installation-safety-bootstrap-v1', 'UTF8'))
        || public.vestrace_p05_field(uuid_send(p_installation))
        || public.vestrace_p05_field(uuid_send(p_fingerprint))
        || public.vestrace_p05_field(p_proof)
        || public.vestrace_p05_field(p_journal_key)
        || public.vestrace_p05_field(p_witness_key), 'sha256');
      IF expected_bootstrap <> p_bootstrap_digest
         OR NOT public.vestrace_safety_ed25519_verify(
              public.vestrace_p05_receipt_payload(p_installation, p_fingerprint, p_proof, p_sequence, p_journal_digest, p_generation, p_epoch, p_state, p_witness_key),
              p_receipt_signature, p_witness_key) THEN
        RAISE EXCEPTION 'P05 initializer signature or bootstrap binding is invalid' USING ERRCODE = '42501';
      END IF;
      journal_payload := public.vestrace_p05_journal_payload(p_installation, p_fingerprint, p_proof, p_request, p_sequence, previous, 0::SMALLINT, p_generation, p_epoch, p_state, p_journal_key);
      expected_journal := digest(journal_payload, 'sha256');
      IF expected_journal <> p_journal_digest
         OR NOT public.vestrace_safety_ed25519_verify(journal_payload, p_journal_signature, p_journal_key) THEN
        RAISE EXCEPTION 'P05 initializer journal entry is invalid' USING ERRCODE = '42501';
      END IF;
      IF NOT EXISTS (SELECT 1 FROM public.installation_fingerprint_continuity WHERE installation_id = p_installation AND fingerprint_key_id = p_fingerprint AND fingerprint_key_version = 1 AND continuity_proof = p_proof) THEN
        RAISE EXCEPTION 'P05 initializer fingerprint continuity does not match' USING ERRCODE = '42501';
      END IF;
      IF EXISTS (SELECT 1 FROM public.installation_safety_state) THEN
        IF NOT EXISTS (SELECT 1 FROM public.installation_safety_state WHERE installation_id = p_installation AND fingerprint_key_id = p_fingerprint AND fingerprint_continuity_proof = p_proof AND journal_signer_public_key = p_journal_key AND witness_public_key = p_witness_key AND witness_sequence = p_sequence AND journal_digest = p_journal_digest AND witness_state = p_state AND active_generation_id = p_generation AND activation_epoch = p_epoch) THEN
          RAISE EXCEPTION 'P05 initializer refuses a different binding or retry' USING ERRCODE = '23505';
        END IF;
        RETURN jsonb_build_object('installation_id', p_installation, 'sequence', p_sequence, 'journal_digest', encode(p_journal_digest, 'hex'), 'generation_id', p_generation, 'activation_epoch', p_epoch);
      END IF;
      INSERT INTO public.installation_safety_state(singleton, installation_id, fingerprint_key_id, fingerprint_continuity_proof, journal_signer_public_key, witness_public_key, witness_sequence, journal_digest, witness_state, witness_state_digest, active_generation_id, activation_epoch, supervisor_workspace_id, supervisor_principal_id)
      VALUES (TRUE, p_installation, p_fingerprint, p_proof, p_journal_key, p_witness_key, p_sequence, p_journal_digest, p_state, digest(p_state, 'sha256'), p_generation, p_epoch, '00000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000006');
      INSERT INTO public.installation_safety_journal_events VALUES (p_installation, p_sequence, p_request, 0, p_generation, p_epoch, previous, p_journal_digest, p_journal_signature);
      INSERT INTO public.installation_safety_generations VALUES (p_installation, p_generation, p_epoch, p_sequence, p_journal_digest);
      RETURN jsonb_build_object('installation_id', p_installation, 'sequence', p_sequence, 'journal_digest', encode(p_journal_digest, 'hex'), 'generation_id', p_generation, 'activation_epoch', p_epoch);
    END $init$;

    CREATE OR REPLACE FUNCTION public.vestrace_register_database_generation(
      p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID,
      p_sequence BIGINT, p_journal_digest BYTEA, p_journal_signature BYTEA,
      p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
    ) RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $register$
    DECLARE
      current_state public.installation_safety_state%ROWTYPE;
      journal_payload BYTEA;
      expected_journal BYTEA;
    BEGIN
      PERFORM public.vestrace_assert_installation_supervisor_context();
      SELECT * INTO current_state FROM public.installation_safety_state WHERE singleton FOR UPDATE;
      IF NOT FOUND OR current_state.installation_id <> p_installation OR current_state.fingerprint_key_id <> p_fingerprint OR current_state.fingerprint_continuity_proof <> p_proof THEN
        RAISE EXCEPTION 'P05 generation registration identity does not match singleton' USING ERRCODE = '42501';
      END IF;
      IF octet_length(p_journal_digest) <> 32 OR octet_length(p_journal_signature) <> 64 OR octet_length(p_receipt_signature) <> 64
         OR p_sequence <> current_state.witness_sequence + 1 OR p_epoch <> current_state.activation_epoch + 1
         OR p_state = current_state.witness_state THEN
        RAISE EXCEPTION 'P05 generation registration is not a permitted monotonic transition' USING ERRCODE = '22023';
      END IF;
      IF NOT public.vestrace_safety_ed25519_verify(
           public.vestrace_p05_receipt_payload(p_installation, p_fingerprint, p_proof, p_sequence, p_journal_digest, p_generation, p_epoch, p_state, current_state.witness_public_key),
           p_receipt_signature, current_state.witness_public_key) THEN
        RAISE EXCEPTION 'P05 generation receipt signature is invalid' USING ERRCODE = '42501';
      END IF;
      journal_payload := public.vestrace_p05_journal_payload(p_installation, p_fingerprint, p_proof, p_request, p_sequence, current_state.journal_digest, 1::SMALLINT, p_generation, p_epoch, p_state, current_state.journal_signer_public_key);
      expected_journal := digest(journal_payload, 'sha256');
      IF expected_journal <> p_journal_digest OR NOT public.vestrace_safety_ed25519_verify(journal_payload, p_journal_signature, current_state.journal_signer_public_key) THEN
        RAISE EXCEPTION 'P05 generation journal entry is invalid' USING ERRCODE = '42501';
      END IF;
      INSERT INTO public.installation_safety_journal_events VALUES (p_installation, p_sequence, p_request, 1, p_generation, p_epoch, current_state.journal_digest, p_journal_digest, p_journal_signature);
      INSERT INTO public.installation_safety_generations VALUES (p_installation, p_generation, p_epoch, p_sequence, p_journal_digest);
      UPDATE public.installation_safety_state SET witness_sequence = p_sequence, journal_digest = p_journal_digest, witness_state = p_state, witness_state_digest = digest(p_state, 'sha256'), active_generation_id = p_generation, activation_epoch = p_epoch WHERE singleton;
      RETURN jsonb_build_object('installation_id', p_installation, 'sequence', p_sequence, 'journal_digest', encode(p_journal_digest, 'hex'), 'generation_id', p_generation, 'activation_epoch', p_epoch);
    END $register$;
  $p05_mutations$;
  ALTER FUNCTION public.vestrace_initialize_installation_safety(BYTEA, UUID, UUID, BYTEA, UUID, BYTEA, BYTEA, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_register_database_generation(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_initialize_installation_safety(BYTEA, UUID, UUID, BYTEA, UUID, BYTEA, BYTEA, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA), public.vestrace_register_database_generation(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
  GRANT EXECUTE ON FUNCTION public.vestrace_initialize_installation_safety(BYTEA, UUID, UUID, BYTEA, UUID, BYTEA, BYTEA, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA), public.vestrace_register_database_generation(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;
  PERFORM public.vestrace_install_p05_backup_archive_schema();
END
$installer$;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_safety_schema() FROM PUBLIC, vestrace, vestrace_guarded_owner, vestrace_safety_supervisor;
SQL

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 <<'SQL'
CREATE OR REPLACE FUNCTION public.vestrace_install_p05_backup_archive_schema()
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $archive_installer$
BEGIN
  IF session_user <> 'vestrace_bootstrap' AND NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = session_user), FALSE) THEN
    RAISE EXCEPTION 'P05 archive installation requires bootstrap custody' USING ERRCODE = '42501';
  END IF;
  ALTER TABLE public.installation_safety_journal_events
    DROP CONSTRAINT IF EXISTS installation_safety_journal_events_event_kind_check;
  ALTER TABLE public.installation_safety_journal_events
    ADD CONSTRAINT installation_safety_journal_events_event_kind_check
      CHECK (event_kind IN (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
  CREATE TABLE IF NOT EXISTS public.managed_backup_sets (
    backup_set_id UUID PRIMARY KEY,
    installation_id UUID NOT NULL REFERENCES public.installation_safety_state(installation_id),
    generation_id UUID NOT NULL,
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('streaming','sealing','sealed','deletion_prepared','archive_key_erased','deleted')),
    envelope_reference UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
  );
  CREATE TABLE IF NOT EXISTS public.managed_backup_archive_heads (
    backup_set_id UUID PRIMARY KEY REFERENCES public.managed_backup_sets(backup_set_id),
    checkpoint_ordinal BIGINT NOT NULL CHECK (checkpoint_ordinal >= 0),
    checkpoint_digest BYTEA NOT NULL CHECK (octet_length(checkpoint_digest) = 32)
  );
  CREATE TABLE IF NOT EXISTS public.managed_backup_archive_objects (
    backup_set_id UUID NOT NULL REFERENCES public.managed_backup_sets(backup_set_id),
    ordinal BIGINT NOT NULL CHECK (ordinal > 0), object_id UUID NOT NULL, object_kind SMALLINT NOT NULL CHECK (object_kind BETWEEN 0 AND 2), timeline INTEGER NOT NULL CHECK (timeline > 0), start_lsn BIGINT NOT NULL, end_lsn BIGINT NOT NULL CHECK (end_lsn >= start_lsn), object_digest BYTEA NOT NULL CHECK (octet_length(object_digest) = 32), plaintext_digest BYTEA NOT NULL CHECK (octet_length(plaintext_digest) = 32), object_length BIGINT NOT NULL CHECK (object_length > 0), predecessor_head_digest BYTEA NOT NULL CHECK (octet_length(predecessor_head_digest) = 32), checkpoint_digest BYTEA NOT NULL CHECK (octet_length(checkpoint_digest) = 32),
    PRIMARY KEY (backup_set_id, ordinal), UNIQUE (backup_set_id, object_id), UNIQUE (backup_set_id, object_digest)
  );
  CREATE TABLE IF NOT EXISTS public.managed_backup_append_intents (intent_id UUID PRIMARY KEY, backup_set_id UUID NOT NULL REFERENCES public.managed_backup_sets(backup_set_id), ordinal BIGINT NOT NULL, expected_head_digest BYTEA NOT NULL CHECK (octet_length(expected_head_digest) = 32), object_digest BYTEA NOT NULL CHECK (octet_length(object_digest) = 32), UNIQUE (backup_set_id, ordinal));
  CREATE TABLE IF NOT EXISTS public.managed_backup_restore_holds (hold_id UUID PRIMARY KEY, backup_set_id UUID NOT NULL REFERENCES public.managed_backup_sets(backup_set_id), released_at TIMESTAMPTZ);
  CREATE TABLE IF NOT EXISTS public.managed_backup_deletion_preparations (backup_set_id UUID PRIMARY KEY REFERENCES public.managed_backup_sets(backup_set_id), preparation_digest BYTEA NOT NULL CHECK (octet_length(preparation_digest) = 32));
  CREATE TABLE IF NOT EXISTS public.managed_backup_events (event_id BIGSERIAL PRIMARY KEY, backup_set_id UUID NOT NULL REFERENCES public.managed_backup_sets(backup_set_id), event_kind TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp());
  ALTER TABLE public.managed_backup_sets OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_archive_heads OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_archive_objects OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_append_intents OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_restore_holds OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_deletion_preparations OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_events OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_backup_sets ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_sets FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_backup_archive_heads ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_archive_heads FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_backup_archive_objects ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_archive_objects FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_backup_append_intents ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_append_intents FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_backup_restore_holds ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_restore_holds FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_backup_deletion_preparations ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_deletion_preparations FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_backup_events ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_backup_events FORCE ROW LEVEL SECURITY;
  IF NOT EXISTS (SELECT 1 FROM pg_policy WHERE polrelid = 'public.managed_backup_sets'::regclass AND polname = 'managed_backup_sets_guarded_owner_only') THEN
    CREATE POLICY managed_backup_sets_guarded_owner_only ON public.managed_backup_sets TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
    CREATE POLICY managed_backup_archive_heads_guarded_owner_only ON public.managed_backup_archive_heads TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
    CREATE POLICY managed_backup_archive_objects_guarded_owner_only ON public.managed_backup_archive_objects TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
    CREATE POLICY managed_backup_append_intents_guarded_owner_only ON public.managed_backup_append_intents TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
    CREATE POLICY managed_backup_restore_holds_guarded_owner_only ON public.managed_backup_restore_holds TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
    CREATE POLICY managed_backup_deletion_preparations_guarded_owner_only ON public.managed_backup_deletion_preparations TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
    CREATE POLICY managed_backup_events_guarded_owner_only ON public.managed_backup_events TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
  END IF;
  REVOKE ALL ON TABLE public.managed_backup_sets, public.managed_backup_archive_heads, public.managed_backup_archive_objects, public.managed_backup_append_intents, public.managed_backup_restore_holds, public.managed_backup_deletion_preparations, public.managed_backup_events FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  CREATE OR REPLACE FUNCTION public.vestrace_reject_managed_backup_event_mutation()
  RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog AS $immutable$
  BEGIN
    RAISE EXCEPTION 'managed backup events are immutable' USING ERRCODE = '55000';
  END $immutable$;
  ALTER FUNCTION public.vestrace_reject_managed_backup_event_mutation() OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_reject_managed_backup_event_mutation() FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid = 'public.managed_backup_events'::regclass AND tgname = 'managed_backup_events_immutable') THEN
    CREATE TRIGGER managed_backup_events_immutable BEFORE UPDATE OR DELETE ON public.managed_backup_events FOR EACH ROW EXECUTE FUNCTION public.vestrace_reject_managed_backup_event_mutation();
  END IF;
  CREATE OR REPLACE FUNCTION public.vestrace_assert_archive_safety_authority(
    p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT,
    p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA,
    p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA,
    p_event_kind SMALLINT
  ) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $archive_authority$
  DECLARE safety public.installation_safety_state%ROWTYPE; journal_payload BYTEA;
  BEGIN
    PERFORM public.vestrace_assert_installation_supervisor_context();
    IF p_event_kind NOT BETWEEN 2 AND 10
       OR octet_length(p_proof) <> 32 OR octet_length(p_previous_digest) <> 32
       OR octet_length(p_journal_digest) <> 32 OR octet_length(p_journal_signature) <> 64
       OR octet_length(p_receipt_signature) <> 64 THEN
      RAISE EXCEPTION 'archive transition has malformed signed authority data' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO safety FROM public.installation_safety_state WHERE singleton FOR UPDATE;
    IF NOT FOUND OR safety.installation_id <> p_installation
       OR safety.fingerprint_key_id <> p_fingerprint
       OR safety.fingerprint_continuity_proof <> p_proof
       OR p_sequence <> safety.witness_sequence + 1
       OR p_previous_digest <> safety.journal_digest
       OR p_generation <> safety.active_generation_id
       OR p_epoch <> safety.activation_epoch
       OR NOT public.vestrace_safety_ed25519_verify(
            public.vestrace_p05_receipt_payload(p_installation,p_fingerprint,p_proof,p_sequence,p_journal_digest,p_generation,p_epoch,p_state,safety.witness_public_key),
            p_receipt_signature,safety.witness_public_key) THEN
      RAISE EXCEPTION 'archive transition receipt does not match pinned safety authority' USING ERRCODE = '42501';
    END IF;
    journal_payload := public.vestrace_p05_journal_payload(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_event_kind,p_generation,p_epoch,p_state,safety.journal_signer_public_key);
    IF digest(journal_payload,'sha256') <> p_journal_digest
       OR NOT public.vestrace_safety_ed25519_verify(journal_payload,p_journal_signature,safety.journal_signer_public_key) THEN
      RAISE EXCEPTION 'archive transition journal signature is invalid' USING ERRCODE = '42501';
    END IF;
  END $archive_authority$;
  ALTER FUNCTION public.vestrace_assert_archive_safety_authority(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA, SMALLINT) OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_assert_archive_safety_authority(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA, SMALLINT) FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  CREATE OR REPLACE FUNCTION public.vestrace_record_archive_safety_authority(
    p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT,
    p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA,
    p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA,
    p_event_kind SMALLINT
  ) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $archive_record$
  BEGIN
    PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,p_event_kind);
    UPDATE public.installation_safety_state
    SET witness_sequence = p_sequence, journal_digest = p_journal_digest,
        witness_state = p_state, witness_state_digest = digest(p_state, 'sha256')
    WHERE singleton;
    INSERT INTO public.installation_safety_journal_events
    VALUES (p_installation,p_sequence,p_request,p_event_kind,p_generation,p_epoch,p_previous_digest,p_journal_digest,p_journal_signature);
  END $archive_record$;
  ALTER FUNCTION public.vestrace_record_archive_safety_authority(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA, SMALLINT) OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_record_archive_safety_authority(UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA, SMALLINT) FROM PUBLIC, vestrace, vestrace_safety_supervisor;
CREATE OR REPLACE FUNCTION public.vestrace_start_managed_backup_set(
  p_set UUID, p_initial_head_digest BYTEA, p_envelope UUID,
  p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT,
  p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA,
  p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $start$
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,3::SMALLINT);
  IF octet_length(p_initial_head_digest) <> 32 THEN
    RAISE EXCEPTION 'managed backup start has malformed initial archive head' USING ERRCODE = '22023';
  END IF;
  INSERT INTO public.managed_backup_sets(backup_set_id,installation_id,generation_id,lifecycle,envelope_reference)
  VALUES (p_set,p_installation,p_generation,'streaming',p_envelope);
  INSERT INTO public.managed_backup_archive_heads(backup_set_id,checkpoint_ordinal,checkpoint_digest)
  VALUES (p_set,0,p_initial_head_digest);
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'backup_started');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,3::SMALLINT);
END $start$;
ALTER FUNCTION public.vestrace_start_managed_backup_set(UUID, BYTEA, UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_start_managed_backup_set(UUID, BYTEA, UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_start_managed_backup_set(UUID, BYTEA, UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;
CREATE OR REPLACE FUNCTION public.vestrace_reserve_backup_archive_append(
  p_set UUID, p_expected_ordinal BIGINT, p_expected_digest BYTEA, p_intent UUID, p_object_digest BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $reserve$
DECLARE current_head public.managed_backup_archive_heads%ROWTYPE; current_set public.managed_backup_sets%ROWTYPE;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  SELECT * INTO current_set FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  SELECT * INTO current_head FROM public.managed_backup_archive_heads WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_set.lifecycle <> 'streaming' OR current_head.checkpoint_ordinal <> p_expected_ordinal OR current_head.checkpoint_digest <> p_expected_digest OR octet_length(p_object_digest) <> 32 THEN
    RAISE EXCEPTION 'backup archive append reservation does not match live head' USING ERRCODE = '23514';
  END IF;
  INSERT INTO public.managed_backup_append_intents(intent_id, backup_set_id, ordinal, expected_head_digest, object_digest)
  VALUES (p_intent, p_set, p_expected_ordinal + 1, p_expected_digest, p_object_digest);
END $reserve$;
ALTER FUNCTION public.vestrace_reserve_backup_archive_append(UUID, BIGINT, BYTEA, UUID, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_reserve_backup_archive_append(UUID, BIGINT, BYTEA, UUID, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_reserve_backup_archive_append(UUID, BIGINT, BYTEA, UUID, BYTEA) TO vestrace_safety_supervisor;
CREATE OR REPLACE FUNCTION public.vestrace_abandon_backup_archive_append(
  p_set UUID, p_expected_ordinal BIGINT, p_expected_digest BYTEA, p_intent UUID, p_object_digest BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $abandon$
DECLARE current_head public.managed_backup_archive_heads%ROWTYPE; current_set public.managed_backup_sets%ROWTYPE;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  SELECT * INTO current_set FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  SELECT * INTO current_head FROM public.managed_backup_archive_heads WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_set.lifecycle <> 'streaming' OR current_head.checkpoint_ordinal <> p_expected_ordinal OR current_head.checkpoint_digest <> p_expected_digest THEN
    RAISE EXCEPTION 'backup archive append abandonment does not match live head' USING ERRCODE = '23514';
  END IF;
  DELETE FROM public.managed_backup_append_intents
  WHERE intent_id = p_intent AND backup_set_id = p_set
    AND ordinal = p_expected_ordinal + 1
    AND expected_head_digest = p_expected_digest
    AND object_digest = p_object_digest;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'backup archive append abandonment does not match exact intent' USING ERRCODE = '23514';
  END IF;
END $abandon$;
ALTER FUNCTION public.vestrace_abandon_backup_archive_append(UUID, BIGINT, BYTEA, UUID, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_abandon_backup_archive_append(UUID, BIGINT, BYTEA, UUID, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_abandon_backup_archive_append(UUID, BIGINT, BYTEA, UUID, BYTEA) TO vestrace_safety_supervisor;
CREATE OR REPLACE FUNCTION public.vestrace_list_pending_backup_archive_appends()
RETURNS TABLE(backup_set_id UUID, intent_id UUID, ordinal BIGINT, expected_head_digest BYTEA, object_digest BYTEA)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $pending$
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  RETURN QUERY
  SELECT intent.backup_set_id, intent.intent_id, intent.ordinal, intent.expected_head_digest, intent.object_digest
  FROM public.managed_backup_append_intents AS intent
  ORDER BY intent.backup_set_id, intent.ordinal, intent.intent_id;
END $pending$;
ALTER FUNCTION public.vestrace_list_pending_backup_archive_appends() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_list_pending_backup_archive_appends() FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_list_pending_backup_archive_appends() TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_commit_backup_archive_checkpoint(
  p_set UUID, p_intent UUID, p_ordinal BIGINT, p_object_id UUID, p_object_kind SMALLINT, p_timeline INTEGER, p_start_lsn BIGINT, p_end_lsn BIGINT, p_object_digest BYTEA, p_plaintext_digest BYTEA, p_object_length BIGINT, p_predecessor_head_digest BYTEA, p_checkpoint_digest BYTEA,
  p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA,
  p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $commit$
DECLARE head public.managed_backup_archive_heads%ROWTYPE; safety public.installation_safety_state%ROWTYPE; journal_payload BYTEA;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  IF octet_length(p_proof) <> 32 OR octet_length(p_previous_digest) <> 32 OR octet_length(p_journal_digest) <> 32 OR octet_length(p_journal_signature) <> 64 OR octet_length(p_receipt_signature) <> 64 OR octet_length(p_object_digest) <> 32 OR octet_length(p_plaintext_digest) <> 32 OR octet_length(p_predecessor_head_digest) <> 32 OR octet_length(p_checkpoint_digest) <> 32 THEN
    RAISE EXCEPTION 'backup checkpoint has malformed signed authority data' USING ERRCODE = '22023';
  END IF;
  SELECT * INTO safety FROM public.installation_safety_state WHERE singleton FOR UPDATE;
  -- A crash after this guarded transaction commits but before host object
  -- promotion must be recoverable.  Accept a retry only when every durable
  -- database fact is already the exact checkpoint named by this receipt.
  IF FOUND
     AND safety.installation_id = p_installation
     AND safety.fingerprint_key_id = p_fingerprint
     AND safety.fingerprint_continuity_proof = p_proof
     AND safety.witness_sequence = p_sequence
     AND safety.journal_digest = p_journal_digest
     AND safety.witness_state = p_state
     AND EXISTS (
       SELECT 1 FROM public.installation_safety_journal_events event
       WHERE event.installation_id = p_installation AND event.sequence = p_sequence
         AND event.request_id = p_request AND event.event_kind = 2
         AND event.generation_id = p_generation AND event.activation_epoch = p_epoch
         AND event.previous_digest = p_previous_digest AND event.journal_digest = p_journal_digest
         AND event.journal_signature = p_journal_signature
     ) THEN
    SELECT * INTO head FROM public.managed_backup_archive_heads WHERE backup_set_id = p_set FOR UPDATE;
    IF head.checkpoint_ordinal = p_ordinal AND head.checkpoint_digest = p_checkpoint_digest
       AND EXISTS (
         SELECT 1 FROM public.managed_backup_archive_objects object
         WHERE object.backup_set_id = p_set AND object.ordinal = p_ordinal
           AND object.object_id = p_object_id AND object.object_kind = p_object_kind
           AND object.timeline = p_timeline AND object.start_lsn = p_start_lsn
           AND object.end_lsn = p_end_lsn AND object.object_digest = p_object_digest
           AND object.plaintext_digest = p_plaintext_digest AND object.object_length = p_object_length
           AND object.predecessor_head_digest = p_predecessor_head_digest
           AND object.checkpoint_digest = p_checkpoint_digest
       ) THEN
      RETURN;
    END IF;
    RAISE EXCEPTION 'backup checkpoint retry does not match its durable receipt' USING ERRCODE = '23514';
  END IF;
  IF NOT FOUND OR safety.installation_id <> p_installation OR safety.fingerprint_key_id <> p_fingerprint OR safety.fingerprint_continuity_proof <> p_proof
     OR p_sequence <> safety.witness_sequence + 1 OR p_previous_digest <> safety.journal_digest
     OR p_generation <> safety.active_generation_id OR p_epoch <> safety.activation_epoch
     OR NOT public.vestrace_safety_ed25519_verify(public.vestrace_p05_receipt_payload(p_installation,p_fingerprint,p_proof,p_sequence,p_journal_digest,p_generation,p_epoch,p_state,safety.witness_public_key),p_receipt_signature,safety.witness_public_key) THEN
    RAISE EXCEPTION 'backup checkpoint receipt does not match pinned safety authority' USING ERRCODE = '42501';
  END IF;
  journal_payload := public.vestrace_p05_journal_payload(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,2::SMALLINT,p_generation,p_epoch,p_state,safety.journal_signer_public_key);
  IF digest(journal_payload,'sha256') <> p_journal_digest OR NOT public.vestrace_safety_ed25519_verify(journal_payload,p_journal_signature,safety.journal_signer_public_key) THEN
    RAISE EXCEPTION 'backup checkpoint journal signature is invalid' USING ERRCODE = '42501';
  END IF;
  SELECT * INTO head FROM public.managed_backup_archive_heads WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR p_ordinal <> head.checkpoint_ordinal + 1 OR p_timeline <= 0 OR p_end_lsn < p_start_lsn OR p_object_length <= 0 OR p_object_kind NOT BETWEEN 0 AND 2 OR p_predecessor_head_digest <> head.checkpoint_digest
     OR NOT EXISTS (SELECT 1 FROM public.managed_backup_append_intents WHERE intent_id = p_intent AND backup_set_id = p_set AND ordinal = p_ordinal AND expected_head_digest = head.checkpoint_digest AND object_digest = p_object_digest) THEN
    RAISE EXCEPTION 'backup checkpoint does not match exact reserved archive head' USING ERRCODE = '23514';
  END IF;
  INSERT INTO public.managed_backup_archive_objects(backup_set_id, ordinal, object_id, object_kind, timeline, start_lsn, end_lsn, object_digest, plaintext_digest, object_length, predecessor_head_digest, checkpoint_digest) VALUES (p_set,p_ordinal,p_object_id,p_object_kind,p_timeline,p_start_lsn,p_end_lsn,p_object_digest,p_plaintext_digest,p_object_length,p_predecessor_head_digest,p_checkpoint_digest);
  UPDATE public.managed_backup_archive_heads SET checkpoint_ordinal=p_ordinal, checkpoint_digest=p_checkpoint_digest WHERE backup_set_id=p_set;
  DELETE FROM public.managed_backup_append_intents WHERE intent_id=p_intent;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'checkpoint_committed');
  UPDATE public.installation_safety_state
  SET witness_sequence = p_sequence,
      journal_digest = p_journal_digest,
      witness_state = p_state,
      witness_state_digest = digest(p_state, 'sha256')
  WHERE singleton;
  INSERT INTO public.installation_safety_journal_events
  VALUES (p_installation, p_sequence, p_request, 2, p_generation, p_epoch, p_previous_digest, p_journal_digest, p_journal_signature);
END $commit$;
ALTER FUNCTION public.vestrace_commit_backup_archive_checkpoint(UUID, UUID, BIGINT, UUID, SMALLINT, INTEGER, BIGINT, BIGINT, BYTEA, BYTEA, BIGINT, BYTEA, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_commit_backup_archive_checkpoint(UUID, UUID, BIGINT, UUID, SMALLINT, INTEGER, BIGINT, BIGINT, BYTEA, BYTEA, BIGINT, BYTEA, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_commit_backup_archive_checkpoint(UUID, UUID, BIGINT, UUID, SMALLINT, INTEGER, BIGINT, BIGINT, BYTEA, BYTEA, BIGINT, BYTEA, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;
CREATE OR REPLACE FUNCTION public.vestrace_acquire_managed_backup_restore_hold(
  p_set UUID, p_hold UUID, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $hold$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,4::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'streaming' THEN
    RAISE EXCEPTION 'managed backup is not restore-eligible' USING ERRCODE = '23514';
  END IF;
  INSERT INTO public.managed_backup_restore_holds(hold_id,backup_set_id) VALUES (p_hold,p_set);
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'restore_hold_acquired');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,4::SMALLINT);
END $hold$;
ALTER FUNCTION public.vestrace_acquire_managed_backup_restore_hold(UUID, UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_acquire_managed_backup_restore_hold(UUID, UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_acquire_managed_backup_restore_hold(UUID, UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_begin_managed_backup_sealing(
  p_set UUID, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $sealing$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,5::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'streaming' OR EXISTS (SELECT 1 FROM public.managed_backup_restore_holds WHERE backup_set_id=p_set AND released_at IS NULL) THEN
    RAISE EXCEPTION 'managed backup cannot begin sealing while restore-held or non-streaming' USING ERRCODE = '23514';
  END IF;
  UPDATE public.managed_backup_sets SET lifecycle='sealing' WHERE backup_set_id=p_set;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'sealing_started');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,5::SMALLINT);
END $sealing$;
ALTER FUNCTION public.vestrace_begin_managed_backup_sealing(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_begin_managed_backup_sealing(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_begin_managed_backup_sealing(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_commit_managed_backup_sealed(
  p_set UUID, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $sealed$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,6::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'sealing' OR EXISTS (SELECT 1 FROM public.managed_backup_restore_holds WHERE backup_set_id=p_set AND released_at IS NULL) OR EXISTS (SELECT 1 FROM public.managed_backup_append_intents WHERE backup_set_id=p_set) THEN
    RAISE EXCEPTION 'managed backup cannot seal before holds and append intents drain' USING ERRCODE = '23514';
  END IF;
  UPDATE public.managed_backup_sets SET lifecycle='sealed' WHERE backup_set_id=p_set;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'sealed');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,6::SMALLINT);
END $sealed$;
ALTER FUNCTION public.vestrace_commit_managed_backup_sealed(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_commit_managed_backup_sealed(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_commit_managed_backup_sealed(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;
CREATE OR REPLACE FUNCTION public.vestrace_prepare_managed_backup_deletion(
  p_set UUID, p_preparation_digest BYTEA, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $prepare_delete$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,7::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'sealed' OR octet_length(p_preparation_digest) <> 32 OR EXISTS (SELECT 1 FROM public.managed_backup_restore_holds WHERE backup_set_id=p_set AND released_at IS NULL) THEN
    RAISE EXCEPTION 'managed backup is not eligible for deletion preparation' USING ERRCODE = '23514';
  END IF;
  INSERT INTO public.managed_backup_deletion_preparations(backup_set_id,preparation_digest) VALUES (p_set,p_preparation_digest);
  UPDATE public.managed_backup_sets SET lifecycle='deletion_prepared' WHERE backup_set_id=p_set;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'deletion_prepared');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,7::SMALLINT);
END $prepare_delete$;
ALTER FUNCTION public.vestrace_prepare_managed_backup_deletion(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_prepare_managed_backup_deletion(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_prepare_managed_backup_deletion(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_prepare_archive_key_erasure(
  p_set UUID, p_preparation_digest BYTEA, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $prepare_key_erasure$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,8::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'deletion_prepared' OR NOT EXISTS (SELECT 1 FROM public.managed_backup_deletion_preparations WHERE backup_set_id=p_set AND preparation_digest=p_preparation_digest) THEN
    RAISE EXCEPTION 'archive key erasure is not bound to the prepared deletion' USING ERRCODE = '23514';
  END IF;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'archive_key_erasure_prepared');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,8::SMALLINT);
END $prepare_key_erasure$;
ALTER FUNCTION public.vestrace_prepare_archive_key_erasure(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_prepare_archive_key_erasure(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_prepare_archive_key_erasure(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_record_archive_key_erased(
  p_set UUID, p_preparation_digest BYTEA, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $key_erased$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,9::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'deletion_prepared' OR NOT EXISTS (SELECT 1 FROM public.managed_backup_deletion_preparations WHERE backup_set_id=p_set AND preparation_digest=p_preparation_digest) OR NOT EXISTS (SELECT 1 FROM public.managed_backup_events WHERE backup_set_id=p_set AND event_kind='archive_key_erasure_prepared') THEN
    RAISE EXCEPTION 'archive key erasure is not bound to the prepared deletion' USING ERRCODE = '23514';
  END IF;
  UPDATE public.managed_backup_sets SET lifecycle='archive_key_erased' WHERE backup_set_id=p_set;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'archive_key_erased');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,9::SMALLINT);
END $key_erased$;
ALTER FUNCTION public.vestrace_record_archive_key_erased(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_record_archive_key_erased(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_record_archive_key_erased(UUID, BYTEA, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_list_prepared_backup_archive_objects(
  p_set UUID, p_preparation_digest BYTEA
) RETURNS TABLE(
  object_id UUID, ordinal BIGINT, object_kind SMALLINT, timeline INTEGER,
  start_lsn BIGINT, end_lsn BIGINT, object_digest BYTEA, plaintext_digest BYTEA,
  object_length BIGINT, predecessor_head_digest BYTEA
) LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $prepared_manifest$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id=p_set FOR SHARE;
  IF NOT FOUND OR current_lifecycle <> 'archive_key_erased'
     OR NOT EXISTS (SELECT 1 FROM public.managed_backup_deletion_preparations WHERE backup_set_id=p_set AND preparation_digest=p_preparation_digest) THEN
    RAISE EXCEPTION 'prepared archive manifest is not available for exact deletion' USING ERRCODE = '23514';
  END IF;
  RETURN QUERY
    SELECT object.object_id, object.ordinal, object.object_kind, object.timeline,
           object.start_lsn, object.end_lsn, object.object_digest, object.plaintext_digest,
           object.object_length, object.predecessor_head_digest
    FROM public.managed_backup_archive_objects object
    WHERE object.backup_set_id=p_set
    ORDER BY object.ordinal;
END $prepared_manifest$;
ALTER FUNCTION public.vestrace_list_prepared_backup_archive_objects(UUID, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_list_prepared_backup_archive_objects(UUID, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_list_prepared_backup_archive_objects(UUID, BYTEA) TO vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_finalize_managed_backup_deleted(
  p_set UUID, p_installation UUID, p_fingerprint UUID, p_proof BYTEA, p_request UUID, p_sequence BIGINT, p_previous_digest BYTEA, p_journal_digest BYTEA, p_journal_signature BYTEA, p_generation UUID, p_epoch BIGINT, p_state BYTEA, p_receipt_signature BYTEA
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $deleted$
DECLARE current_lifecycle TEXT;
BEGIN
  PERFORM public.vestrace_assert_installation_supervisor_context();
  PERFORM public.vestrace_assert_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,10::SMALLINT);
  SELECT lifecycle INTO current_lifecycle FROM public.managed_backup_sets WHERE backup_set_id = p_set FOR UPDATE;
  IF NOT FOUND OR current_lifecycle <> 'archive_key_erased' THEN
    RAISE EXCEPTION 'managed backup is not ready for final deletion' USING ERRCODE = '23514';
  END IF;
  UPDATE public.managed_backup_sets SET lifecycle='deleted' WHERE backup_set_id=p_set;
  INSERT INTO public.managed_backup_events(backup_set_id,event_kind) VALUES (p_set,'deleted');
  PERFORM public.vestrace_record_archive_safety_authority(p_installation,p_fingerprint,p_proof,p_request,p_sequence,p_previous_digest,p_journal_digest,p_journal_signature,p_generation,p_epoch,p_state,p_receipt_signature,10::SMALLINT);
END $deleted$;
ALTER FUNCTION public.vestrace_finalize_managed_backup_deleted(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_finalize_managed_backup_deleted(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) FROM PUBLIC, vestrace;
GRANT EXECUTE ON FUNCTION public.vestrace_finalize_managed_backup_deleted(UUID, UUID, UUID, BYTEA, UUID, BIGINT, BYTEA, BYTEA, BYTEA, UUID, BIGINT, BYTEA, BYTEA) TO vestrace_safety_supervisor;
END
$archive_installer$;
ALTER FUNCTION public.vestrace_install_p05_backup_archive_schema() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_backup_archive_schema() FROM PUBLIC, vestrace, vestrace_safety_supervisor;

-- P05-B base-capture completion. These guards apply to every existing
-- guarded procedure, including a stale supervisor binary that still calls
-- the former reservation signature.
CREATE OR REPLACE FUNCTION public.vestrace_enforce_managed_backup_base_checkpoint()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $base_object$
BEGIN
  IF (NEW.ordinal = 1 AND NEW.object_kind <> 0)
     OR (NEW.ordinal > 1 AND NEW.object_kind = 0)
     OR (NEW.ordinal > 1 AND NOT EXISTS (
       SELECT 1 FROM public.managed_backup_archive_objects
       WHERE backup_set_id = NEW.backup_set_id AND ordinal = 1 AND object_kind = 0
     ))
     OR (NEW.object_kind = 1 AND EXISTS (
       SELECT 1 FROM public.managed_backup_archive_objects
       WHERE backup_set_id = NEW.backup_set_id AND ordinal = 1 AND object_kind = 0
         AND (timeline <> NEW.timeline OR NEW.start_lsn < start_lsn)
     )) THEN
    RAISE EXCEPTION 'managed backup object violates the first-base checkpoint invariant' USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END $base_object$;
ALTER FUNCTION public.vestrace_enforce_managed_backup_base_checkpoint() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_enforce_managed_backup_base_checkpoint() FROM PUBLIC, vestrace, vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_enforce_managed_backup_base_ready()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $base_ready$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM public.managed_backup_archive_objects
    WHERE backup_set_id = NEW.backup_set_id AND ordinal = 1 AND object_kind = 0
  ) THEN
    RAISE EXCEPTION 'managed backup is not base-checkpoint ready' USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END $base_ready$;
ALTER FUNCTION public.vestrace_enforce_managed_backup_base_ready() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_enforce_managed_backup_base_ready() FROM PUBLIC, vestrace, vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_enforce_managed_backup_lifecycle_base_ready()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $base_lifecycle$
BEGIN
  IF NEW.lifecycle <> 'streaming' AND NOT EXISTS (
    SELECT 1 FROM public.managed_backup_archive_objects
    WHERE backup_set_id = NEW.backup_set_id AND ordinal = 1 AND object_kind = 0
  ) THEN
    RAISE EXCEPTION 'managed backup lifecycle cannot leave streaming before its base checkpoint' USING ERRCODE = '23514';
  END IF;
  RETURN NEW;
END $base_lifecycle$;
ALTER FUNCTION public.vestrace_enforce_managed_backup_lifecycle_base_ready() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_enforce_managed_backup_lifecycle_base_ready() FROM PUBLIC, vestrace, vestrace_safety_supervisor;

CREATE OR REPLACE FUNCTION public.vestrace_install_p05_base_capture_guards()
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $base_guards$
BEGIN
  IF to_regclass('public.managed_backup_archive_objects') IS NULL
     OR to_regclass('public.managed_backup_restore_holds') IS NULL
     OR to_regclass('public.managed_backup_sets') IS NULL THEN
    RAISE EXCEPTION 'P05 base capture guards require the archive schema' USING ERRCODE = '42501';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_backup_archive_objects'::regclass AND tgname='managed_backup_base_checkpoint_required') THEN
    EXECUTE 'CREATE TRIGGER managed_backup_base_checkpoint_required BEFORE INSERT ON public.managed_backup_archive_objects FOR EACH ROW EXECUTE FUNCTION public.vestrace_enforce_managed_backup_base_checkpoint()';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_backup_restore_holds'::regclass AND tgname='managed_backup_hold_requires_base_checkpoint') THEN
    EXECUTE 'CREATE TRIGGER managed_backup_hold_requires_base_checkpoint BEFORE INSERT ON public.managed_backup_restore_holds FOR EACH ROW EXECUTE FUNCTION public.vestrace_enforce_managed_backup_base_ready()';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_backup_sets'::regclass AND tgname='managed_backup_lifecycle_requires_base_checkpoint') THEN
    EXECUTE 'CREATE TRIGGER managed_backup_lifecycle_requires_base_checkpoint BEFORE UPDATE OF lifecycle ON public.managed_backup_sets FOR EACH ROW EXECUTE FUNCTION public.vestrace_enforce_managed_backup_lifecycle_base_ready()';
  END IF;
END $base_guards$;
ALTER FUNCTION public.vestrace_install_p05_base_capture_guards() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_base_capture_guards() FROM PUBLIC, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_install_p05_base_capture_guards() TO vestrace;

SQL

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 <<'SQL'
CREATE OR REPLACE FUNCTION public.vestrace_install_p05_restore_cutover_guards()
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $restore_installer$
BEGIN
  IF session_user <> 'vestrace' AND NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = session_user), FALSE) THEN
    RAISE EXCEPTION 'P05 restore installation requires the fixed P05 migration route' USING ERRCODE = '42501';
  END IF;
  ALTER TABLE public.installation_safety_journal_events
    DROP CONSTRAINT IF EXISTS installation_safety_journal_events_event_kind_check;
  ALTER TABLE public.installation_safety_journal_events
    ADD CONSTRAINT installation_safety_journal_events_event_kind_check
      CHECK (event_kind IN (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15));
  CREATE TABLE IF NOT EXISTS public.managed_restore_attempts (
    attempt_id UUID PRIMARY KEY, backup_set_id UUID NOT NULL REFERENCES public.managed_backup_sets(backup_set_id),
    hold_id UUID NOT NULL REFERENCES public.managed_backup_restore_holds(hold_id), target_id UUID NOT NULL UNIQUE,
    source_generation_id UUID NOT NULL, target_generation_id UUID NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('prepared','source_frozen','target_initialized','source_resume_prepared','released')),
    freeze_timeline INTEGER, freeze_lsn BIGINT, mutation_watermark BIGINT,
    terminal_receipt BYTEA CHECK (terminal_receipt IS NULL OR octet_length(terminal_receipt)=32),
    CHECK (source_generation_id <> target_generation_id),
    CHECK ((state='prepared' AND freeze_timeline IS NULL AND freeze_lsn IS NULL AND mutation_watermark IS NULL AND terminal_receipt IS NULL)
      OR (state='source_frozen' AND freeze_timeline > 0 AND freeze_lsn > 0 AND mutation_watermark >= 0 AND terminal_receipt IS NULL)
      OR (state IN ('target_initialized','source_resume_prepared','released') AND freeze_timeline > 0 AND freeze_lsn > 0 AND mutation_watermark >= 0 AND terminal_receipt IS NOT NULL))
  );
  -- PostgreSQL leaves the two inline state checks in place when this installer
  -- upgrades an existing P05-C table. Replace just those checks by their
  -- stable names so a source-resume terminal state is accepted on upgrade as
  -- well as on a freshly provisioned instance.
  ALTER TABLE public.managed_restore_attempts DROP CONSTRAINT IF EXISTS managed_restore_attempts_state_check;
  ALTER TABLE public.managed_restore_attempts DROP CONSTRAINT IF EXISTS managed_restore_attempts_check1;
  ALTER TABLE public.managed_restore_attempts DROP CONSTRAINT IF EXISTS managed_restore_attempts_freeze_state_check;
  ALTER TABLE public.managed_restore_attempts
    ADD CONSTRAINT managed_restore_attempts_state_check
    CHECK (state IN ('prepared','source_frozen','target_initialized','source_resume_prepared','released'));
  ALTER TABLE public.managed_restore_attempts
    ADD CONSTRAINT managed_restore_attempts_freeze_state_check
    CHECK ((state='prepared' AND freeze_timeline IS NULL AND freeze_lsn IS NULL AND mutation_watermark IS NULL AND terminal_receipt IS NULL)
      OR (state='source_frozen' AND freeze_timeline > 0 AND freeze_lsn > 0 AND mutation_watermark >= 0 AND terminal_receipt IS NULL)
      OR (state IN ('target_initialized','source_resume_prepared','released') AND freeze_timeline > 0 AND freeze_lsn > 0 AND mutation_watermark >= 0 AND terminal_receipt IS NOT NULL));
  CREATE UNIQUE INDEX IF NOT EXISTS managed_restore_attempts_one_live_source
    ON public.managed_restore_attempts(source_generation_id) WHERE state <> 'released';
  CREATE TABLE IF NOT EXISTS public.managed_restore_events (
    event_id BIGSERIAL PRIMARY KEY, attempt_id UUID NOT NULL REFERENCES public.managed_restore_attempts(attempt_id),
    event_kind TEXT NOT NULL, receipt BYTEA, created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (receipt IS NULL OR octet_length(receipt)=32)
  );
  ALTER TABLE public.managed_restore_attempts OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_restore_events OWNER TO vestrace_guarded_owner;
  ALTER TABLE public.managed_restore_attempts ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_restore_attempts FORCE ROW LEVEL SECURITY;
  ALTER TABLE public.managed_restore_events ENABLE ROW LEVEL SECURITY; ALTER TABLE public.managed_restore_events FORCE ROW LEVEL SECURITY;
  IF NOT EXISTS (SELECT 1 FROM pg_policy WHERE polrelid='public.managed_restore_attempts'::regclass AND polname='managed_restore_attempts_guarded_owner_only') THEN
    CREATE POLICY managed_restore_attempts_guarded_owner_only ON public.managed_restore_attempts TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_policy WHERE polrelid='public.managed_restore_events'::regclass AND polname='managed_restore_events_guarded_owner_only') THEN
    CREATE POLICY managed_restore_events_guarded_owner_only ON public.managed_restore_events TO vestrace_guarded_owner USING (TRUE) WITH CHECK (TRUE);
  END IF;
  REVOKE ALL ON TABLE public.managed_restore_attempts, public.managed_restore_events FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  CREATE OR REPLACE FUNCTION public.vestrace_reject_managed_restore_event_mutation()
  RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $event_append_only$
  BEGIN
    RAISE EXCEPTION 'managed restore events are append-only' USING ERRCODE='42501';
  END $event_append_only$;
  ALTER FUNCTION public.vestrace_reject_managed_restore_event_mutation() OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_reject_managed_restore_event_mutation() FROM PUBLIC, vestrace, vestrace_safety_supervisor;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_restore_events'::regclass AND tgname='managed_restore_events_append_only') THEN
    CREATE TRIGGER managed_restore_events_append_only BEFORE UPDATE OR DELETE ON public.managed_restore_events
      FOR EACH ROW EXECUTE FUNCTION public.vestrace_reject_managed_restore_event_mutation();
  END IF;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_prepare_restore_attempt(p_attempt UUID,p_set UUID,p_hold UUID,p_target UUID,p_source UUID,p_target_generation UUID)
    RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    BEGIN
      PERFORM public.vestrace_assert_installation_supervisor_context();
      IF NOT EXISTS (SELECT 1 FROM public.managed_backup_sets s WHERE s.backup_set_id=p_set AND s.lifecycle='streaming' AND s.generation_id=p_source)
         OR NOT EXISTS (SELECT 1 FROM public.managed_backup_restore_holds h WHERE h.hold_id=p_hold AND h.backup_set_id=p_set AND h.released_at IS NULL)
         OR NOT EXISTS (SELECT 1 FROM public.managed_backup_archive_objects o WHERE o.backup_set_id=p_set AND o.ordinal=1 AND o.object_kind=0) THEN
        RAISE EXCEPTION 'restore attempt is not bound to a live base-complete streaming source' USING ERRCODE='23514';
      END IF;
      INSERT INTO public.managed_restore_attempts(attempt_id,backup_set_id,hold_id,target_id,source_generation_id,target_generation_id,state)
      VALUES(p_attempt,p_set,p_hold,p_target,p_source,p_target_generation,'prepared');
      INSERT INTO public.managed_restore_events(attempt_id,event_kind) VALUES(p_attempt,'prepared');
    END $body$;
  $fn$;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_record_source_resume_prepared(p_attempt UUID,p_receipt BYTEA)
    RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    BEGIN PERFORM public.vestrace_assert_installation_supervisor_context();
      IF EXISTS (SELECT 1 FROM public.managed_restore_attempts WHERE attempt_id=p_attempt AND state IN ('source_resume_prepared','released') AND terminal_receipt=p_receipt) THEN RETURN; END IF;
      UPDATE public.managed_restore_attempts SET state='source_resume_prepared',terminal_receipt=p_receipt WHERE attempt_id=p_attempt AND state='source_frozen' AND octet_length(p_receipt)=32;
      IF NOT FOUND THEN RAISE EXCEPTION 'source resume is not an exact frozen successor' USING ERRCODE='23514'; END IF;
      INSERT INTO public.managed_restore_events(attempt_id,event_kind,receipt) VALUES(p_attempt,'source_resume_prepared',p_receipt); END $body$;
  $fn$;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_record_source_freeze(p_attempt UUID,p_timeline INTEGER,p_lsn BIGINT,p_watermark BIGINT)
    RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    BEGIN PERFORM public.vestrace_assert_installation_supervisor_context();
      UPDATE public.managed_restore_attempts SET state='source_frozen',freeze_timeline=p_timeline,freeze_lsn=p_lsn,mutation_watermark=p_watermark WHERE attempt_id=p_attempt AND state='prepared' AND p_timeline>0 AND p_lsn>0 AND p_watermark>=0;
      IF NOT FOUND THEN RAISE EXCEPTION 'restore source freeze is not an exact prepared successor' USING ERRCODE='23514'; END IF;
      INSERT INTO public.managed_restore_events(attempt_id,event_kind) VALUES(p_attempt,'source_frozen'); END $body$;
  $fn$;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_record_target_initialized(p_attempt UUID,p_receipt BYTEA)
    RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    BEGIN PERFORM public.vestrace_assert_installation_supervisor_context();
      IF EXISTS (SELECT 1 FROM public.managed_restore_attempts WHERE attempt_id=p_attempt AND state IN ('target_initialized','released') AND terminal_receipt=p_receipt) THEN RETURN; END IF;
      UPDATE public.managed_restore_attempts SET state='target_initialized',terminal_receipt=p_receipt WHERE attempt_id=p_attempt AND state='source_frozen' AND octet_length(p_receipt)=32;
      IF NOT FOUND THEN RAISE EXCEPTION 'target initialization is not an exact frozen successor' USING ERRCODE='23514'; END IF;
      INSERT INTO public.managed_restore_events(attempt_id,event_kind,receipt) VALUES(p_attempt,'target_initialized',p_receipt); END $body$;
  $fn$;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_list_restore_archive_objects(p_attempt UUID)
    RETURNS TABLE(
      backup_set_id UUID, object_id UUID, ordinal BIGINT, object_kind SMALLINT, timeline INTEGER,
      start_lsn BIGINT, end_lsn BIGINT, object_digest BYTEA, plaintext_digest BYTEA,
      object_length BIGINT, predecessor_head_digest BYTEA
    ) LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    DECLARE v_set UUID; v_timeline INTEGER; v_lsn BIGINT; v_state TEXT;
    BEGIN
      PERFORM public.vestrace_assert_installation_supervisor_context();
      SELECT attempt.backup_set_id, attempt.freeze_timeline, attempt.freeze_lsn, attempt.state
        INTO v_set, v_timeline, v_lsn, v_state
        FROM public.managed_restore_attempts AS attempt
        WHERE attempt.attempt_id=p_attempt FOR SHARE;
      IF NOT FOUND OR v_state <> 'source_frozen' THEN
        RAISE EXCEPTION 'restore archive manifest requires the exact frozen attempt' USING ERRCODE='23514';
      END IF;
      RETURN QUERY
        SELECT object.backup_set_id, object.object_id, object.ordinal, object.object_kind,
               object.timeline, object.start_lsn, object.end_lsn, object.object_digest,
               object.plaintext_digest, object.object_length, object.predecessor_head_digest
        FROM public.managed_backup_archive_objects object
        WHERE object.backup_set_id=v_set
          AND (object.object_kind IN (0,2)
               OR (object.object_kind=1 AND object.timeline=v_timeline AND object.start_lsn <= v_lsn))
        ORDER BY object.ordinal;
      IF NOT EXISTS (
        SELECT 1 FROM public.managed_backup_archive_objects object
        WHERE object.backup_set_id=v_set AND object.object_kind=1
          AND object.timeline=v_timeline AND object.start_lsn <= v_lsn AND object.end_lsn >= v_lsn
      ) THEN
        RAISE EXCEPTION 'restore archive manifest does not cover the witnessed source freeze' USING ERRCODE='23514';
      END IF;
    END $body$;
  $fn$;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_release_restore_hold(p_attempt UUID,p_receipt BYTEA)
    RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    DECLARE v_hold UUID; BEGIN PERFORM public.vestrace_assert_installation_supervisor_context();
      IF EXISTS (SELECT 1 FROM public.managed_restore_attempts WHERE attempt_id=p_attempt AND state='released' AND terminal_receipt=p_receipt) THEN RETURN; END IF;
      UPDATE public.managed_restore_attempts SET state='released' WHERE attempt_id=p_attempt AND state IN ('target_initialized','source_resume_prepared') AND terminal_receipt=p_receipt RETURNING hold_id INTO v_hold;
      IF NOT FOUND THEN RAISE EXCEPTION 'restore hold release does not match its terminal receipt' USING ERRCODE='23514'; END IF;
      UPDATE public.managed_backup_restore_holds SET released_at=clock_timestamp() WHERE hold_id=v_hold AND released_at IS NULL;
      IF NOT FOUND THEN RAISE EXCEPTION 'restore hold is not live' USING ERRCODE='23514'; END IF;
      INSERT INTO public.managed_restore_events(attempt_id,event_kind,receipt) VALUES(p_attempt,'hold_released',p_receipt); END $body$;
  $fn$;
  EXECUTE $fn$
    CREATE OR REPLACE FUNCTION public.vestrace_record_restore_safety_event(
      p_installation UUID,p_fingerprint UUID,p_proof BYTEA,p_request UUID,p_sequence BIGINT,
      p_journal_digest BYTEA,p_journal_signature BYTEA,p_event_kind SMALLINT,p_generation UUID,
      p_epoch BIGINT,p_state BYTEA,p_receipt_signature BYTEA
    ) RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,public AS $body$
    DECLARE current_state public.installation_safety_state%ROWTYPE; journal_payload BYTEA; expected_journal BYTEA;
    BEGIN
      PERFORM public.vestrace_assert_installation_supervisor_context();
      SELECT * INTO current_state FROM public.installation_safety_state WHERE singleton FOR UPDATE;
      IF NOT FOUND OR current_state.installation_id<>p_installation OR current_state.fingerprint_key_id<>p_fingerprint OR current_state.fingerprint_continuity_proof<>p_proof THEN
        RAISE EXCEPTION 'P05 restore safety event identity does not match singleton' USING ERRCODE='42501';
      END IF;
      IF current_state.witness_sequence=p_sequence AND current_state.journal_digest=p_journal_digest AND current_state.witness_state=p_state AND current_state.active_generation_id=p_generation AND current_state.activation_epoch=p_epoch THEN
        RETURN jsonb_build_object('installation_id',p_installation,'sequence',p_sequence,'journal_digest',encode(p_journal_digest,'hex'),'generation_id',p_generation,'activation_epoch',p_epoch);
      END IF;
      IF p_event_kind NOT IN (11,12,13,14,15) OR octet_length(p_journal_digest)<>32 OR octet_length(p_journal_signature)<>64 OR octet_length(p_receipt_signature)<>64
         OR p_sequence<>current_state.witness_sequence+1 OR p_generation<>current_state.active_generation_id OR p_epoch<>current_state.activation_epoch OR p_state=current_state.witness_state THEN
        RAISE EXCEPTION 'P05 restore safety event is not an exact monotonic successor' USING ERRCODE='22023';
      END IF;
      IF NOT public.vestrace_safety_ed25519_verify(public.vestrace_p05_receipt_payload(p_installation,p_fingerprint,p_proof,p_sequence,p_journal_digest,p_generation,p_epoch,p_state,current_state.witness_public_key),p_receipt_signature,current_state.witness_public_key) THEN
        RAISE EXCEPTION 'P05 restore safety receipt signature is invalid' USING ERRCODE='42501';
      END IF;
      journal_payload:=public.vestrace_p05_journal_payload(p_installation,p_fingerprint,p_proof,p_request,p_sequence,current_state.journal_digest,p_event_kind,p_generation,p_epoch,p_state,current_state.journal_signer_public_key);
      expected_journal:=digest(journal_payload,'sha256');
      IF expected_journal<>p_journal_digest OR NOT public.vestrace_safety_ed25519_verify(journal_payload,p_journal_signature,current_state.journal_signer_public_key) THEN
        RAISE EXCEPTION 'P05 restore safety journal entry is invalid' USING ERRCODE='42501';
      END IF;
      INSERT INTO public.installation_safety_journal_events VALUES(p_installation,p_sequence,p_request,p_event_kind,p_generation,p_epoch,current_state.journal_digest,p_journal_digest,p_journal_signature);
      UPDATE public.installation_safety_state SET witness_sequence=p_sequence,journal_digest=p_journal_digest,witness_state=p_state,witness_state_digest=digest(p_state,'sha256') WHERE singleton;
      RETURN jsonb_build_object('installation_id',p_installation,'sequence',p_sequence,'journal_digest',encode(p_journal_digest,'hex'),'generation_id',p_generation,'activation_epoch',p_epoch);
    END $body$
  $fn$;
  ALTER FUNCTION public.vestrace_prepare_restore_attempt(UUID,UUID,UUID,UUID,UUID,UUID) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_record_source_freeze(UUID,INTEGER,BIGINT,BIGINT) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_record_target_initialized(UUID,BYTEA) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_record_source_resume_prepared(UUID,BYTEA) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_record_restore_safety_event(UUID,UUID,BYTEA,UUID,BIGINT,BYTEA,BYTEA,SMALLINT,UUID,BIGINT,BYTEA,BYTEA) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_list_restore_archive_objects(UUID) OWNER TO vestrace_guarded_owner;
  ALTER FUNCTION public.vestrace_release_restore_hold(UUID,BYTEA) OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_prepare_restore_attempt(UUID,UUID,UUID,UUID,UUID,UUID),public.vestrace_record_source_freeze(UUID,INTEGER,BIGINT,BIGINT),public.vestrace_record_target_initialized(UUID,BYTEA),public.vestrace_record_source_resume_prepared(UUID,BYTEA),public.vestrace_record_restore_safety_event(UUID,UUID,BYTEA,UUID,BIGINT,BYTEA,BYTEA,SMALLINT,UUID,BIGINT,BYTEA,BYTEA),public.vestrace_list_restore_archive_objects(UUID),public.vestrace_release_restore_hold(UUID,BYTEA) FROM PUBLIC,vestrace;
  GRANT EXECUTE ON FUNCTION public.vestrace_prepare_restore_attempt(UUID,UUID,UUID,UUID,UUID,UUID),public.vestrace_record_source_freeze(UUID,INTEGER,BIGINT,BIGINT),public.vestrace_record_target_initialized(UUID,BYTEA),public.vestrace_record_source_resume_prepared(UUID,BYTEA),public.vestrace_record_restore_safety_event(UUID,UUID,BYTEA,UUID,BIGINT,BYTEA,BYTEA,SMALLINT,UUID,BIGINT,BYTEA,BYTEA),public.vestrace_list_restore_archive_objects(UUID),public.vestrace_release_restore_hold(UUID,BYTEA) TO vestrace_safety_supervisor;
  REVOKE ALL ON FUNCTION public.vestrace_install_p05_restore_cutover_guards() FROM PUBLIC, vestrace;
END $restore_installer$;
ALTER FUNCTION public.vestrace_install_p05_restore_cutover_guards() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_restore_cutover_guards() FROM PUBLIC, vestrace, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_install_p05_restore_cutover_guards() TO vestrace;
GRANT CREATE ON SCHEMA public TO vestrace_guarded_owner;

CREATE OR REPLACE FUNCTION public.vestrace_install_p05_restore_refusal_guards()
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $restore_refusal_installer$
BEGIN
  IF session_user <> 'vestrace' AND NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = session_user), FALSE) THEN
    RAISE EXCEPTION 'P05 refusal installation requires the fixed P05 migration route' USING ERRCODE = '42501';
  END IF;
  PERFORM public.vestrace_install_p05_restore_cutover_guards();
END $restore_refusal_installer$;
ALTER FUNCTION public.vestrace_install_p05_restore_refusal_guards() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_restore_refusal_guards() FROM PUBLIC, vestrace, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_install_p05_restore_refusal_guards() TO vestrace;
SQL

psql   --username "$POSTGRES_USER"   --dbname "$POSTGRES_DB"   --no-password   --no-psqlrc   --set=ON_ERROR_STOP=1 <<'SQL'
-- P05-D least-privilege readiness read.
--
-- The supervisor holds no SELECT privilege on any safety table, and P05
-- deliberately refuses to add one: a table grant would also expose every future
-- column added to the singleton. This is the whole read surface instead -- one
-- guarded owner function that returns exactly the columns a host continuity
-- check compares, and nothing else.
--
-- It is a read. It takes no arguments, so a caller cannot steer it at another
-- row, and it holds no key material the caller did not already have to possess
-- to reach this role. The public keys it returns are public by construction;
-- no private key, signature, or secret is exposed.
CREATE OR REPLACE FUNCTION public.vestrace_install_p05_safety_readiness_read()
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $readiness_installer$
BEGIN
  IF session_user <> 'vestrace' AND NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = session_user), FALSE) THEN
    RAISE EXCEPTION 'P05 readiness installation requires the fixed P05 migration route' USING ERRCODE = '42501';
  END IF;
  EXECUTE $readiness_body$
    CREATE OR REPLACE FUNCTION public.vestrace_read_installation_safety_readiness()
    RETURNS JSONB
    LANGUAGE plpgsql
    SECURITY DEFINER
    STABLE
    SET search_path = pg_catalog, public
    AS $readiness$
    DECLARE
      state public.installation_safety_state%ROWTYPE;
    BEGIN
      PERFORM public.vestrace_assert_installation_supervisor_context();
      SELECT * INTO state FROM public.installation_safety_state WHERE singleton;
      IF NOT FOUND THEN
        RETURN NULL;
      END IF;
      -- The stored state digest is recomputed here rather than echoed, so a
      -- row whose canonical bytes were edited without its digest -- or the
      -- reverse -- cannot be read back as internally consistent.
      IF state.witness_state_digest IS DISTINCT FROM digest(state.witness_state, 'sha256') THEN
        RAISE EXCEPTION 'P05 persisted witness state does not match its digest' USING ERRCODE = '22023';
      END IF;
      RETURN jsonb_build_object(
        'installation_id', state.installation_id,
        'fingerprint_key_id', state.fingerprint_key_id,
        'continuity_proof', encode(state.fingerprint_continuity_proof, 'hex'),
        'journal_signer_public_key', encode(state.journal_signer_public_key, 'hex'),
        'witness_public_key', encode(state.witness_public_key, 'hex'),
        'witness_sequence', state.witness_sequence,
        'journal_digest', encode(state.journal_digest, 'hex'),
        'witness_state', encode(state.witness_state, 'hex'),
        'active_generation_id', state.active_generation_id,
        'activation_epoch', state.activation_epoch
      );
    END $readiness$;
  $readiness_body$;
  ALTER FUNCTION public.vestrace_read_installation_safety_readiness() OWNER TO vestrace_guarded_owner;
  REVOKE ALL ON FUNCTION public.vestrace_read_installation_safety_readiness() FROM PUBLIC, vestrace;
  GRANT EXECUTE ON FUNCTION public.vestrace_read_installation_safety_readiness() TO vestrace_safety_supervisor;
END $readiness_installer$;
ALTER FUNCTION public.vestrace_install_p05_safety_readiness_read() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_safety_readiness_read() FROM PUBLIC, vestrace, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_install_p05_safety_readiness_read() TO vestrace;
SQL

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 <<'SQL'
-- P05-I DrainMutationPermit/Quiescing: creates installation_drain_requests,
-- the drained_by snapshot columns, the two new drain functions, and widens
-- the two existing reserve functions with one precondition each. Matches the
-- established P05 (0209+) pattern exactly: this is a SECURITY DEFINER
-- function owned by vestrace_guarded_owner (the owner of schema public
-- itself, so its body always has CREATE rights regardless of what has been
-- revoked from vestrace), callable by the restricted runtime role, invoked
-- from within migration 0216 itself -- the same shape as
-- vestrace_install_p05_base_capture_guards (0211) and
-- vestrace_install_p05_restore_cutover_guards (0212). No ownership hand-back
-- is needed: CREATE OR REPLACE FUNCTION issued from inside a SECURITY
-- DEFINER body executes, and creates new objects, as the function's owner.
CREATE OR REPLACE FUNCTION public.vestrace_install_p05_installation_drain_guards()
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $guards$
BEGIN
  IF to_regclass('public.material_key_creation_intents') IS NULL
     OR to_regclass('public.credential_key_creation_intents') IS NULL THEN
    RAISE EXCEPTION 'P05 installation-drain guards require the material/credential intent schema' USING ERRCODE = '42501';
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

  -- Pre-Quiescing states per docs/superpowers/specs/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit-design.md
  -- section 2. Bound and every terminal state are never stamped: g0-13 --
  -- "Bound never abandons" -- and a terminal intent needs no draining.
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

  -- Widens the existing reserve functions with one precondition: refuse a
  -- new Reserved intent while any drain request row exists (active or
  -- Frozen -- freezing is permanent for that request; a new drain would be a
  -- new request row). Every other line is copied unchanged from migrations
  -- 0169 and 0173 respectively.
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

  EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_request_installation_drain(UUID) FROM PUBLIC, vestrace';
  EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_request_installation_drain(UUID) TO vestrace';
  EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_reconcile_installation_drain(UUID) FROM PUBLIC, vestrace';
  EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_reconcile_installation_drain(UUID) TO vestrace';
  -- The two reserve functions already carry their 0169/0173 grants (EXECUTE
  -- to vestrace, revoked from PUBLIC); CREATE OR REPLACE preserves a
  -- function's existing ACL when replacing an object that already exists.
END
$guards$;
ALTER FUNCTION public.vestrace_install_p05_installation_drain_guards() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_installation_drain_guards() FROM PUBLIC, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_install_p05_installation_drain_guards() TO vestrace;
SQL

psql \
  --username "$POSTGRES_USER" \
  --dbname "$POSTGRES_DB" \
  --no-password \
  --no-psqlrc \
  --set=ON_ERROR_STOP=1 <<'SQL'
-- P06-followup Task 1 (qualification_job_work_claims): creates the
-- claim/finish lease table and its two functions. Matches the
-- established P05 (0209+) pattern exactly: this is a SECURITY DEFINER
-- function owned by vestrace_guarded_owner (the owner of schema public
-- itself, so its body always has CREATE rights regardless of what has
-- been revoked from vestrace), callable by the restricted runtime role,
-- invoked from within migration 0217 itself -- the same shape as
-- vestrace_install_p05_installation_drain_guards (0216). No ownership
-- hand-back is needed: CREATE FUNCTION issued from inside a SECURITY
-- DEFINER body executes, and creates new objects, as the function's
-- owner.
CREATE OR REPLACE FUNCTION public.vestrace_install_p05_qualification_work_claims()
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $qguards$
BEGIN
  IF to_regclass('public.qualification_jobs') IS NULL THEN
    RAISE EXCEPTION 'P05 qualification work claims require the qualification_jobs schema' USING ERRCODE = '42501';
  END IF;

  IF to_regclass('public.qualification_job_work_claims') IS NULL THEN
    EXECUTE 'CREATE TABLE public.qualification_job_work_claims (
        workspace_id UUID NOT NULL,
        job_id UUID NOT NULL,
        claim_owner TEXT NOT NULL CHECK (length(btrim(claim_owner)) BETWEEN 1 AND 128),
        claim_deadline TIMESTAMPTZ NOT NULL,
        last_outcome TEXT CHECK (last_outcome IN (''completed'', ''retryable_failure'', ''definite_failure'')),
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        PRIMARY KEY (workspace_id, job_id),
        FOREIGN KEY (workspace_id, job_id) REFERENCES public.qualification_jobs(workspace_id, id) ON DELETE RESTRICT,
        CHECK (claim_deadline > created_at)
    )';
    EXECUTE 'CREATE INDEX qualification_job_work_claims_available
        ON public.qualification_job_work_claims(workspace_id, claim_deadline, job_id)';
    EXECUTE 'ALTER TABLE public.qualification_job_work_claims ENABLE ROW LEVEL SECURITY';
    EXECUTE 'ALTER TABLE public.qualification_job_work_claims FORCE ROW LEVEL SECURITY';
    EXECUTE 'CREATE POLICY qualification_job_work_claims_workspace_isolation ON public.qualification_job_work_claims
        FOR ALL
        USING (workspace_id = vestrace_current_workspace_id())
        WITH CHECK (workspace_id = vestrace_current_workspace_id())';
    EXECUTE 'REVOKE ALL ON TABLE public.qualification_job_work_claims FROM PUBLIC, vestrace';
    EXECUTE 'CREATE TRIGGER qualification_job_work_claims_guarded
        BEFORE INSERT OR UPDATE OR DELETE ON public.qualification_job_work_claims
        FOR EACH ROW EXECUTE FUNCTION public.vestrace_reject_raw_p03_mutation()';
  END IF;

  EXECUTE $clmfn$
  CREATE OR REPLACE FUNCTION public.vestrace_claim_qualification_work(
      target_workspace UUID, target_owner TEXT, target_limit INTEGER
  ) RETURNS TABLE(job_id UUID, claim_owner TEXT, claim_deadline TIMESTAMPTZ)
  LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $clmbody$
  #variable_conflict use_column
  DECLARE deadline TIMESTAMPTZ := now() + INTERVAL '60 seconds';
  BEGIN
      IF target_workspace IS NULL
         OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
         OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128
         OR target_limit IS NULL OR target_limit NOT BETWEEN 1 AND 128 THEN
          RAISE EXCEPTION 'qualification work claim arguments are malformed' USING ERRCODE = '22023';
      END IF;
      RETURN QUERY
      WITH candidates AS (
          SELECT job.id
            FROM qualification_jobs AS job
           WHERE job.workspace_id = target_workspace
             AND job.state IN ('requested', 'running')
             AND NOT EXISTS (
                 SELECT 1 FROM qualification_job_work_claims AS claim
                  WHERE claim.workspace_id = job.workspace_id AND claim.job_id = job.id
                    AND claim.claim_deadline > now())
           ORDER BY job.requested_at, job.id
           FOR UPDATE SKIP LOCKED
           LIMIT target_limit
      ), claimed AS (
          INSERT INTO qualification_job_work_claims(
              workspace_id, job_id, claim_owner, claim_deadline, last_outcome, created_at, updated_at)
          SELECT target_workspace, id, target_owner, deadline, NULL, now(), now() FROM candidates
          ON CONFLICT (workspace_id, job_id) DO UPDATE
             SET claim_owner = EXCLUDED.claim_owner, claim_deadline = EXCLUDED.claim_deadline,
                 last_outcome = NULL, updated_at = now()
           WHERE qualification_job_work_claims.claim_deadline <= now()
          RETURNING qualification_job_work_claims.job_id,
                    qualification_job_work_claims.claim_owner, qualification_job_work_claims.claim_deadline
      )
      SELECT job_id, claim_owner, claim_deadline FROM claimed ORDER BY job_id;
  END $clmbody$;
  $clmfn$;

  EXECUTE $finfn$
  CREATE OR REPLACE FUNCTION public.vestrace_finish_qualification_work(
      target_workspace UUID, target_job UUID, target_owner TEXT, target_outcome TEXT
  ) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $finbody$
  BEGIN
      IF target_workspace IS NULL OR target_job IS NULL
         OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
         OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128
         OR target_outcome NOT IN ('completed', 'retryable_failure', 'definite_failure') THEN
          RAISE EXCEPTION 'qualification work completion arguments are malformed' USING ERRCODE = '22023';
      END IF;
      IF target_outcome = 'retryable_failure' THEN
          UPDATE qualification_job_work_claims SET claim_deadline = now(), last_outcome = target_outcome, updated_at = now()
           WHERE workspace_id = target_workspace AND job_id = target_job
             AND claim_owner = target_owner AND claim_deadline > now();
      ELSE
          DELETE FROM qualification_job_work_claims
           WHERE workspace_id = target_workspace AND job_id = target_job
             AND claim_owner = target_owner AND claim_deadline > now();
      END IF;
      RETURN FOUND;
  END $finbody$;
  $finfn$;

  EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_claim_qualification_work(uuid,text,integer) FROM PUBLIC';
  EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_claim_qualification_work(uuid,text,integer) TO vestrace';
  EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_finish_qualification_work(uuid,uuid,text,text) FROM PUBLIC';
  EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_finish_qualification_work(uuid,uuid,text,text) TO vestrace';
END
$qguards$;
ALTER FUNCTION public.vestrace_install_p05_qualification_work_claims() OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION public.vestrace_install_p05_qualification_work_claims() FROM PUBLIC, vestrace_safety_supervisor;
GRANT EXECUTE ON FUNCTION public.vestrace_install_p05_qualification_work_claims() TO vestrace;
SQL

# A physical base backup opens a replication connection, which PostgreSQL's
# generic `host all ...` rule does not cover.  This script also runs from the
# separate role-provisioner container, which cannot see PGDATA; only the
# initialization invocation may alter this instance-owned configuration.
if [ -f "$PGDATA/PG_VERSION" ] && ! grep -qxF 'host replication all all scram-sha-256' "$PGDATA/pg_hba.conf"; then
  printf '%s\n' 'host replication all all scram-sha-256' >> "$PGDATA/pg_hba.conf"
  psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" --no-password --no-psqlrc --set=ON_ERROR_STOP=1 -c 'SELECT pg_reload_conf();' >/dev/null
fi
