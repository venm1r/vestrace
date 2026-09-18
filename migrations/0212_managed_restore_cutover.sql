-- P05-C restores are provisioned by the guarded bootstrap owner; migration only asserts it.
SELECT public.vestrace_install_p05_restore_cutover_guards();
REVOKE CREATE ON SCHEMA public FROM vestrace_guarded_owner;

DO $p05_restore$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version=211 AND success) THEN
    RAISE EXCEPTION 'P05 restore/cutover requires successful 0211' USING ERRCODE='42501';
  END IF;
  IF to_regclass('public.managed_restore_attempts') IS NULL
     OR to_regclass('public.managed_restore_events') IS NULL
     OR to_regprocedure('public.vestrace_prepare_restore_attempt(uuid,uuid,uuid,uuid,uuid,uuid)') IS NULL
     OR to_regprocedure('public.vestrace_record_source_freeze(uuid,integer,bigint,bigint)') IS NULL
     OR to_regprocedure('public.vestrace_record_target_initialized(uuid,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_record_source_resume_prepared(uuid,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_list_restore_archive_objects(uuid)') IS NULL
     OR to_regprocedure('public.vestrace_release_restore_hold(uuid,bytea)') IS NULL THEN
    RAISE EXCEPTION 'P05 restore/cutover guarded catalog is unavailable' USING ERRCODE='42501';
  END IF;
  IF (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
      JOIN pg_roles r ON r.oid=c.relowner
      WHERE n.nspname='public' AND c.relname IN ('managed_restore_attempts','managed_restore_events')
        AND c.relkind='r' AND c.relrowsecurity AND c.relforcerowsecurity AND r.rolname='vestrace_guarded_owner') <> 2
     OR EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
               WHERE n.nspname='public' AND c.relname IN ('managed_restore_attempts','managed_restore_events')
                 AND (has_table_privilege('vestrace',c.oid,'INSERT,UPDATE,DELETE')
                   OR has_table_privilege('vestrace_safety_supervisor',c.oid,'INSERT,UPDATE,DELETE')))
     OR NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_restore_events'::regclass
                    AND tgname='managed_restore_events_append_only' AND NOT tgisinternal) THEN
    RAISE EXCEPTION 'P05 restore/cutover relations lack guarded authority' USING ERRCODE='42501';
  END IF;
  IF has_function_privilege('vestrace','public.vestrace_install_p05_restore_cutover_guards()'::regprocedure,'EXECUTE') THEN
    RAISE EXCEPTION 'P05 restore/cutover installer remains callable by runtime' USING ERRCODE='42501';
  END IF;
  IF (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace JOIN pg_roles r ON r.oid=p.proowner
      WHERE n.nspname='public' AND r.rolname='vestrace_guarded_owner' AND p.oid IN (
        'public.vestrace_prepare_restore_attempt(uuid,uuid,uuid,uuid,uuid,uuid)'::regprocedure,
        'public.vestrace_record_source_freeze(uuid,integer,bigint,bigint)'::regprocedure,
        'public.vestrace_record_target_initialized(uuid,bytea)'::regprocedure,
        'public.vestrace_record_source_resume_prepared(uuid,bytea)'::regprocedure,
        'public.vestrace_list_restore_archive_objects(uuid)'::regprocedure,
        'public.vestrace_release_restore_hold(uuid,bytea)'::regprocedure
      ) AND has_function_privilege('vestrace_safety_supervisor',p.oid,'EXECUTE')
        AND NOT has_function_privilege('vestrace',p.oid,'EXECUTE')) <> 6 THEN
    RAISE EXCEPTION 'P05 restore/cutover procedures lack supervisor-only ownership or grants' USING ERRCODE='42501';
  END IF;
END $p05_restore$;
