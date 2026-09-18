-- P05-C forward-only mirror for signed non-generation restore advances.
SELECT public.vestrace_install_p05_restore_refusal_guards();
REVOKE CREATE ON SCHEMA public FROM vestrace_guarded_owner;

DO $p05_restore_events$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version=213 AND success) THEN
    RAISE EXCEPTION 'P05 restore event upgrade requires successful 0213' USING ERRCODE='42501';
  END IF;
  IF to_regprocedure('public.vestrace_record_restore_safety_event(uuid,uuid,bytea,uuid,bigint,bytea,bytea,smallint,uuid,bigint,bytea,bytea)') IS NULL THEN
    RAISE EXCEPTION 'P05 restore event guard is unavailable' USING ERRCODE='42501';
  END IF;
  IF has_function_privilege('vestrace','public.vestrace_record_restore_safety_event(uuid,uuid,bytea,uuid,bigint,bytea,bytea,smallint,uuid,bigint,bytea,bytea)','EXECUTE')
     OR NOT has_function_privilege('vestrace_safety_supervisor','public.vestrace_record_restore_safety_event(uuid,uuid,bytea,uuid,bigint,bytea,bytea,smallint,uuid,bigint,bytea,bytea)','EXECUTE') THEN
    RAISE EXCEPTION 'P05 restore event function grant is not supervisor-only' USING ERRCODE='42501';
  END IF;
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid='public.installation_safety_journal_events'::regclass
      AND conname='installation_safety_journal_events_event_kind_check'
      AND pg_get_constraintdef(oid) LIKE '%15%'
  ) THEN
    RAISE EXCEPTION 'P05 restore event kind guard is unavailable' USING ERRCODE='42501';
  END IF;
END $p05_restore_events$;
