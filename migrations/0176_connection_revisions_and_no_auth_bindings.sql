-- Automatic SQLx databases are created by the configured test superuser and
-- do not run the deployment provisioner. They may synthesize the same bounded
-- bridge; a non-superuser runtime migration must find the real bootstrap
-- helpers and receives no fallback authority.
DO $bootstrap$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
    IF to_regprocedure('public.vestrace_assign_p03_table_owner(regclass)') IS NULL THEN
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P03 table ownership helper must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        EXECUTE $function$
            CREATE FUNCTION public.vestrace_assign_p03_table_owner(target REGCLASS)
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
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
                    'connection_revision_heads', 'connection_revisions', 'no_auth_binding_revisions',
                    'qualification_jobs', 'qualification_target_bindings', 'qualification_probe_results',
                    'qualification_q1_mre_sources',
                    'connection_qualification_revisions', 'connection_qualification_heads',
                    'model_revision_heads', 'model_revisions', 'model_qualification_revisions',
                    'model_qualification_heads', 'workspace_model_defaults', 'model_binding_snapshots',
                    'run_model_binding_snapshots', 'model_request_evidence_roots',
                    'model_request_evidence_nodes', 'model_request_evidence_checks',
                    'model_request_shape_revisions', 'model_sampling_revisions',
                    'model_limits_revisions', 'model_tool_schema_revisions',
                    'model_request_evidence_check_missing_references',
                    'connection_admission_policy_heads', 'connection_admission_policy_revisions',
                    'connection_admission_states', 'connection_dispatch_admissions',
                    'provider_admission_waits', 'provider_concurrency_leases',
                    'provider_throttle_observations', 'credential_dispatch_leases',
                    'credential_activation_events', 'credential_rotation_events',
                    'provider_result_preparations', 'provider_result_publications',
                    'artifact_revision_contents', 'provider_dispatch_causes',
                    'model_data_policy_decisions'
                ]::TEXT[]) THEN
                    RAISE EXCEPTION 'only declared P03 tables may be handed to the guarded owner'
                        USING ERRCODE = '42501';
                END IF;
                EXECUTE format('ALTER TABLE %I.%I OWNER TO vestrace_guarded_owner', target_schema, target_name);
                EXECUTE format('REVOKE ALL ON TABLE %I.%I FROM PUBLIC', target_schema, target_name);
                EXECUTE format('REVOKE ALL ON TABLE %I.%I FROM vestrace', target_schema, target_name);
                EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %I.%I TO vestrace', target_schema, target_name);
                IF target_name = 'model_data_policy_decisions' THEN
                    EXECUTE format(
                        'REVOKE REFERENCES ON TABLE %I.%I FROM vestrace',
                        target_schema,
                        target_name
                    );
                END IF;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_assign_p03_table_owner(REGCLASS) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_assign_p03_table_owner(REGCLASS) TO vestrace;
    END IF;

    IF to_regprocedure('public.vestrace_assign_p03_function_owner(regprocedure)') IS NULL THEN
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P03 function ownership helper must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        EXECUTE $function$
            CREATE FUNCTION public.vestrace_assign_p03_function_owner(target REGPROCEDURE)
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $body$
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
                    to_regprocedure('public.vestrace_validate_provider_result_completion()')
                    ,to_regprocedure('public.vestrace_validate_task10_deferred_contract()')
                    ,to_regprocedure('public.vestrace_validate_provider_result_material_live()')
                    ,to_regprocedure('public.vestrace_create_model_request_shape_revision(UUID, UUID, BIGINT, TEXT, BOOLEAN, TEXT[])')
                    ,to_regprocedure('public.vestrace_create_model_sampling_revision(UUID, UUID, BIGINT, DOUBLE PRECISION, DOUBLE PRECISION)')
                    ,to_regprocedure('public.vestrace_create_model_limits_revision(UUID, UUID, BIGINT, INTEGER, INTEGER, INTEGER)')
                    ,to_regprocedure('public.vestrace_create_model_tool_schema_revision(UUID, UUID, BIGINT, TEXT, TEXT, JSONB)')
                    ,to_regprocedure('public.vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])')
                    ,to_regprocedure('public.vestrace_append_model_request_evidence_check(UUID, UUID, UUID, TEXT, TEXT[], UUID[], UUID[], UUID[])')
                    ,to_regprocedure('public.vestrace_request_qualification_job(UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)')
                    ,to_regprocedure('public.vestrace_record_qualification_probe_result(UUID, UUID, UUID, TEXT, TEXT, UUID, UUID)')
                    ,to_regprocedure('public.vestrace_cancel_qualification_job(UUID, UUID)')
                    ,to_regprocedure('public.vestrace_recover_qualification_dispatch_unknown(UUID, UUID, TEXT)')
                    ,to_regprocedure('public.vestrace_finalize_qualification_job(UUID, UUID, UUID, UUID, UUID)')
                    ,to_regprocedure('public.vestrace_create_qualification_q1_mre_source(UUID, UUID, TEXT, TEXT, TEXT, BOOLEAN, TEXT, BOOLEAN, BOOLEAN, TEXT)')
                    ,to_regprocedure('public.vestrace_prepare_qualification_probe_dispatch(UUID, UUID, TEXT, UUID, UUID)')
                    ,to_regprocedure('public.vestrace_lock_provider_dispatch_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID)')
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
                    to_regprocedure('public.vestrace_lock_provider_dispatch_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID)')
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
                -- 0184 forward-replaces this temporary Task 9 signature. Keep
                -- it runtime-owned just long enough for the same runtime
                -- migrator to drop it; the deployment provisioner performs
                -- the equivalent bounded hand-back on an existing P03 volume.
                IF target = to_regprocedure('public.vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)') THEN
                    EXECUTE format('REVOKE ALL ON FUNCTION %I.%I(%s) FROM PUBLIC', target_schema, target_name, target_arguments);
                    EXECUTE format('GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace', target_schema, target_name, target_arguments);
                    RETURN;
                END IF;
                EXECUTE format('ALTER FUNCTION %I.%I(%s) OWNER TO vestrace_guarded_owner', target_schema, target_name, target_arguments);
                EXECUTE format('REVOKE ALL ON FUNCTION %I.%I(%s) FROM PUBLIC', target_schema, target_name, target_arguments);
                EXECUTE format('REVOKE ALL ON FUNCTION %I.%I(%s) FROM vestrace', target_schema, target_name, target_arguments);
                IF COALESCE(target = ANY (runtime_executable_targets), FALSE) THEN
                    EXECUTE format('GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace', target_schema, target_name, target_arguments);
                END IF;
                IF COALESCE(target = ANY (migration_trigger_targets), FALSE) THEN
                    EXECUTE format('GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace', target_schema, target_name, target_arguments);
                END IF;
                IF target = to_regprocedure('public.vestrace_finalize_provider_result(UUID, BYTEA)')
                   OR target = to_regprocedure('public.vestrace_append_model_request_evidence_check(UUID, UUID, UUID, TEXT, TEXT[], UUID[], UUID[], UUID[])') THEN
                    EXECUTE 'REVOKE EXECUTE ON FUNCTION public.vestrace_reject_p03_immutable_mutation() FROM vestrace';
                    EXECUTE 'REVOKE EXECUTE ON FUNCTION public.vestrace_reject_raw_p03_mutation() FROM vestrace';
                END IF;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_assign_p03_function_owner(REGPROCEDURE) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_assign_p03_function_owner(REGPROCEDURE) TO vestrace;
    END IF;

    IF to_regprocedure('public.vestrace_prepare_task10_p03_upgrade()') IS NULL THEN
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'Task 10 P03 upgrade helper must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        EXECUTE $function$
            CREATE FUNCTION public.vestrace_prepare_task10_p03_upgrade()
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
                        EXECUTE format(
                            'ALTER TABLE public.%I OWNER TO vestrace', target_name
                        );
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
                        EXECUTE format(
                            'ALTER FUNCTION %s OWNER TO vestrace', target_function
                        );
                    END IF;
                END LOOP;

                IF to_regprocedure(
                    'public.vestrace_reject_p03_immutable_mutation()'
                ) IS NOT NULL THEN
                    GRANT EXECUTE ON FUNCTION
                        public.vestrace_reject_p03_immutable_mutation()
                    TO vestrace;
                END IF;

                REVOKE EXECUTE ON FUNCTION
                    public.vestrace_prepare_task10_p03_upgrade()
                FROM vestrace;
            END
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_prepare_task10_p03_upgrade() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_prepare_task10_p03_upgrade() TO vestrace;
    END IF;

    IF to_regprocedure(
        'public.vestrace_install_provider_result_live_trigger()'
    ) IS NULL THEN
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'provider-result Live trigger installer must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        EXECUTE $function$
            CREATE FUNCTION public.vestrace_install_provider_result_live_trigger()
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

    IF to_regprocedure('public.vestrace_grant_p03_dependency_references()') IS NULL THEN
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P03 dependency-reference helper must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        EXECUTE $function$
            CREATE FUNCTION public.vestrace_grant_p03_dependency_references()
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = public, pg_temp
            AS $body$
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
            $body$
        $function$;
        REVOKE ALL ON FUNCTION public.vestrace_grant_p03_dependency_references() FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_grant_p03_dependency_references() TO vestrace;
    END IF;
END
$bootstrap$;

DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_grant_p03_dependency_references()') IS NOT NULL THEN
        PERFORM public.vestrace_grant_p03_dependency_references();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P03 dependency-reference helper must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
    END IF;
END
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'public.connections'::regclass
          AND conname = 'connections_workspace_id_id_key'
    ) THEN
        ALTER TABLE connections ADD CONSTRAINT connections_workspace_id_id_key
            UNIQUE (workspace_id, id);
    END IF;
END
$$;

CREATE TABLE connection_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    execution_guard_id UUID NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('lm_studio_local', 'open_ai_chat_completions_v1')),
    logical_base_url TEXT NOT NULL CHECK (length(btrim(logical_base_url)) > 0),
    runtime_base_url TEXT NOT NULL CHECK (length(btrim(runtime_base_url)) > 0),
    adapter_profile_revision TEXT NOT NULL CHECK (length(btrim(adapter_profile_revision)) > 0),
    transport_policy TEXT NOT NULL CHECK (transport_policy IN ('loopback_only', 'remote_https')),
    auth_mode TEXT NOT NULL CHECK (auth_mode IN ('none', 'bearer', 'api_key', 'x_api_key')),
    credential_slot_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT connection_revisions_workspace_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_revisions_execution_guard_fkey
        FOREIGN KEY (execution_guard_id, workspace_id)
        REFERENCES connection_execution_guards(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT connection_revisions_credential_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_revisions_auth_xor CHECK (
        (auth_mode = 'none' AND credential_slot_id IS NULL)
        OR (auth_mode IN ('bearer', 'api_key', 'x_api_key') AND credential_slot_id IS NOT NULL)
    ),
    CONSTRAINT connection_revisions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT connection_revisions_workspace_connection_id_key
        UNIQUE (workspace_id, connection_id, id),
    CONSTRAINT connection_revisions_exact_auth_mode_key
        UNIQUE (workspace_id, connection_id, id, auth_mode),
    CONSTRAINT connection_revisions_exact_slot_binding_key
        UNIQUE (workspace_id, connection_id, id, credential_slot_id)
);

CREATE TABLE connection_revision_heads (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    current_revision_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    state TEXT NOT NULL DEFAULT 'enabled' CHECK (state IN ('enabled', 'disabled', 'archived')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, connection_id),
    CONSTRAINT connection_revision_heads_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_revision_heads_revision_fkey
        FOREIGN KEY (workspace_id, connection_id, current_revision_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT
);

CREATE TABLE no_auth_binding_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    connection_revision_id UUID NOT NULL UNIQUE,
    auth_mode TEXT NOT NULL DEFAULT 'none' CHECK (auth_mode = 'none'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT no_auth_binding_revisions_revision_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT no_auth_binding_revisions_exact_mode_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id, auth_mode)
        REFERENCES connection_revisions(workspace_id, connection_id, id, auth_mode)
        ON DELETE RESTRICT,
    CONSTRAINT no_auth_binding_revisions_workspace_id_id_key UNIQUE (workspace_id, id)
    ,CONSTRAINT no_auth_binding_revisions_exact_identity_key
        UNIQUE (workspace_id, connection_id, connection_revision_id, id)
);

CREATE OR REPLACE FUNCTION vestrace_reject_raw_p03_mutation()
RETURNS TRIGGER LANGUAGE plpgsql SET search_path = public, pg_temp AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' THEN
        RAISE EXCEPTION 'P03 state changes require a guarded operation' USING ERRCODE = '42501';
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reject_p03_immutable_mutation()
RETURNS TRIGGER LANGUAGE plpgsql SET search_path = public, pg_temp AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' OR TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'P03 immutable evidence accepts guarded inserts only' USING ERRCODE = '42501';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_connection_revision_identity()
RETURNS TRIGGER LANGUAGE plpgsql SET search_path = public, pg_temp AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM connection_execution_guards
        WHERE id = NEW.execution_guard_id
          AND workspace_id = NEW.workspace_id
          AND connection_id = NEW.connection_id
    ) THEN
        RAISE EXCEPTION 'connection revision must name its exact permanent execution guard'
            USING ERRCODE = '23514';
    END IF;
    IF NEW.credential_slot_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM credential_slots
        WHERE workspace_id = NEW.workspace_id
          AND connection_id = NEW.connection_id
          AND id = NEW.credential_slot_id
    ) THEN
        RAISE EXCEPTION 'connection revision credential slot identity does not match'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_connection_revision_and_advance_head(
    target_revision_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_execution_guard_id UUID,
    target_kind TEXT,
    target_logical_base_url TEXT,
    target_runtime_base_url TEXT,
    target_adapter_profile_revision TEXT,
    target_transport_policy TEXT,
    target_auth_mode TEXT,
    target_credential_slot_id UUID,
    target_expected_head_version BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    current_head_version BIGINT;
BEGIN
    IF target_expected_head_version IS NULL OR target_expected_head_version < 0 THEN
        RAISE EXCEPTION 'connection revision expected head version must be nonnegative'
            USING ERRCODE = '22023';
    END IF;

    -- Keep head advances in the same canonical order as Run binding: the
    -- permanent connection execution guard precedes every mutable predicate.
    PERFORM 1
      FROM connection_execution_guards
     WHERE id = target_execution_guard_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'connection revision requires its exact execution guard'
            USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended(target_workspace_id::TEXT || ':' || target_connection_id::TEXT, 0)
    );

    SELECT version
      INTO current_head_version
      FROM connection_revision_heads
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;

    IF NOT FOUND THEN
        IF target_expected_head_version <> 0 THEN
            RAISE EXCEPTION 'connection revision head version conflict'
                USING ERRCODE = '40001';
        END IF;

        INSERT INTO connection_revisions (
            id, workspace_id, connection_id, execution_guard_id, kind,
            logical_base_url, runtime_base_url, adapter_profile_revision,
            transport_policy, auth_mode, credential_slot_id
        ) VALUES (
            target_revision_id, target_workspace_id, target_connection_id,
            target_execution_guard_id, target_kind, target_logical_base_url,
            target_runtime_base_url, target_adapter_profile_revision,
            target_transport_policy, target_auth_mode, target_credential_slot_id
        );
        INSERT INTO connection_revision_heads (
            workspace_id, connection_id, current_revision_id, version
        ) VALUES (
            target_workspace_id, target_connection_id, target_revision_id, 1
        );
        RETURN target_revision_id;
    END IF;

    IF current_head_version <> target_expected_head_version THEN
        RAISE EXCEPTION 'connection revision head version conflict'
            USING ERRCODE = '40001';
    END IF;

    INSERT INTO connection_revisions (
        id, workspace_id, connection_id, execution_guard_id, kind,
        logical_base_url, runtime_base_url, adapter_profile_revision,
        transport_policy, auth_mode, credential_slot_id
    ) VALUES (
        target_revision_id, target_workspace_id, target_connection_id,
        target_execution_guard_id, target_kind, target_logical_base_url,
        target_runtime_base_url, target_adapter_profile_revision,
        target_transport_policy, target_auth_mode, target_credential_slot_id
    );
    UPDATE connection_revision_heads
       SET current_revision_id = target_revision_id,
           version = version + 1,
           updated_at = NOW()
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id;
    RETURN target_revision_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_no_auth_binding_revision(
    target_no_auth_binding_revision_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_revision_auth_mode TEXT;
BEGIN
    SELECT auth_mode
      INTO target_revision_auth_mode
      FROM connection_revisions
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_connection_revision_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'no-auth binding requires an existing connection revision'
            USING ERRCODE = '23514';
    END IF;
    IF target_revision_auth_mode <> 'none' THEN
        RAISE EXCEPTION 'no-auth binding requires a connection revision with auth_mode none'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO no_auth_binding_revisions (
        id, workspace_id, connection_id, connection_revision_id
    ) VALUES (
        target_no_auth_binding_revision_id, target_workspace_id,
        target_connection_id, target_connection_revision_id
    );
    RETURN target_no_auth_binding_revision_id;
END
$$;

CREATE TRIGGER connection_revisions_validate_identity
    BEFORE INSERT ON connection_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_connection_revision_identity();
CREATE TRIGGER connection_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON connection_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER no_auth_binding_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON no_auth_binding_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER connection_revision_heads_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON connection_revision_heads
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

ALTER TABLE connection_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE connection_revision_heads ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_revision_heads FORCE ROW LEVEL SECURITY;
ALTER TABLE no_auth_binding_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE no_auth_binding_revisions FORCE ROW LEVEL SECURITY;
CREATE POLICY connection_revisions_workspace_policy ON connection_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY connection_revision_heads_workspace_policy ON connection_revision_heads
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY no_auth_binding_revisions_workspace_policy ON no_auth_binding_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

SELECT vestrace_assign_p03_table_owner('connection_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_revision_heads'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('no_auth_binding_revisions'::REGCLASS);
SELECT vestrace_assign_p03_function_owner('vestrace_create_connection_revision_and_advance_head(UUID, UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, UUID, BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_create_no_auth_binding_revision(UUID, UUID, UUID, UUID)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_reject_raw_p03_mutation()'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_reject_p03_immutable_mutation()'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_validate_connection_revision_identity()'::REGPROCEDURE);
