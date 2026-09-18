-- P05-C forward-only upgrade for installations where 0212 predates the
-- source-resume terminal state. The fixed provisioner owns guarded objects.
SELECT public.vestrace_install_p05_restore_refusal_guards();
REVOKE CREATE ON SCHEMA public FROM vestrace_guarded_owner;

DO $p05_restore_refusal$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version=212 AND success) THEN
    RAISE EXCEPTION 'P05 refusal upgrade requires successful 0212' USING ERRCODE='42501';
  END IF;
  IF to_regprocedure('public.vestrace_record_source_resume_prepared(uuid,bytea)') IS NULL THEN
    RAISE EXCEPTION 'P05 source-resume guard is unavailable' USING ERRCODE='42501';
  END IF;
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid='public.managed_restore_attempts'::regclass
      AND conname='managed_restore_attempts_state_check'
      AND pg_get_constraintdef(oid) LIKE '%source_resume_prepared%'
  ) THEN
    RAISE EXCEPTION 'P05 source-resume state guard is unavailable' USING ERRCODE='42501';
  END IF;
  IF has_function_privilege('vestrace','public.vestrace_record_source_resume_prepared(uuid,bytea)','EXECUTE')
     OR NOT has_function_privilege('vestrace_safety_supervisor','public.vestrace_record_source_resume_prepared(uuid,bytea)','EXECUTE') THEN
    RAISE EXCEPTION 'P05 source-resume function grant is not supervisor-only' USING ERRCODE='42501';
  END IF;
END $p05_restore_refusal$;
