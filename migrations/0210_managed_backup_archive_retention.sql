-- P05-B archive state is built solely by the quiesced bootstrap installer.
-- This migration records only an exact catalog/authority assertion.
DO $p05_archive$
DECLARE
  archive_tables TEXT[] := ARRAY[
    'managed_backup_sets',
    'managed_backup_archive_heads',
    'managed_backup_archive_objects',
    'managed_backup_append_intents',
    'managed_backup_restore_holds',
    'managed_backup_deletion_preparations',
    'managed_backup_events'
  ];
BEGIN
  IF EXISTS (
    SELECT 1
    FROM unnest(archive_tables) AS required(name)
    WHERE to_regclass('public.' || required.name) IS NULL
  ) THEN
    RAISE EXCEPTION 'P05 archive installer has not created every required table' USING ERRCODE = '42501';
  END IF;

  IF EXISTS (
    SELECT 1
    FROM pg_class relation
    JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
    LEFT JOIN pg_roles owner_role ON owner_role.oid = relation.relowner
    WHERE namespace.nspname = 'public'
      AND relation.relname = ANY (archive_tables)
      AND (
        owner_role.rolname IS DISTINCT FROM 'vestrace_guarded_owner'
        OR NOT relation.relrowsecurity
        OR NOT relation.relforcerowsecurity
        OR has_table_privilege('vestrace', relation.oid, 'SELECT')
        OR has_table_privilege('vestrace', relation.oid, 'INSERT')
        OR has_table_privilege('vestrace', relation.oid, 'UPDATE')
        OR has_table_privilege('vestrace', relation.oid, 'DELETE')
        OR has_table_privilege('vestrace_safety_supervisor', relation.oid, 'SELECT')
        OR has_table_privilege('vestrace_safety_supervisor', relation.oid, 'INSERT')
        OR has_table_privilege('vestrace_safety_supervisor', relation.oid, 'UPDATE')
        OR has_table_privilege('vestrace_safety_supervisor', relation.oid, 'DELETE')
      )
  ) THEN
    RAISE EXCEPTION 'P05 archive table ownership, RLS, or direct-DML grants are incompatible' USING ERRCODE = '42501';
  END IF;

  IF to_regprocedure('public.vestrace_start_managed_backup_set(uuid,bytea,uuid,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_reserve_backup_archive_append(uuid,bigint,bytea,uuid,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_abandon_backup_archive_append(uuid,bigint,bytea,uuid,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_list_pending_backup_archive_appends()') IS NULL
     OR to_regprocedure('public.vestrace_commit_backup_archive_checkpoint(uuid,uuid,bigint,uuid,smallint,integer,bigint,bigint,bytea,bytea,bigint,bytea,bytea,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_acquire_managed_backup_restore_hold(uuid,uuid,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_begin_managed_backup_sealing(uuid,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_commit_managed_backup_sealed(uuid,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_prepare_managed_backup_deletion(uuid,bytea,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_prepare_archive_key_erasure(uuid,bytea,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_record_archive_key_erased(uuid,bytea,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_list_prepared_backup_archive_objects(uuid,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_finalize_managed_backup_deleted(uuid,uuid,uuid,bytea,uuid,bigint,bytea,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL THEN
    RAISE EXCEPTION 'P05 archive installer has not created every guarded archive procedure' USING ERRCODE = '42501';
  END IF;

  IF EXISTS (
    SELECT 1
    FROM pg_proc procedure
    JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
    LEFT JOIN pg_roles owner_role ON owner_role.oid = procedure.proowner
    WHERE namespace.nspname = 'public'
      AND procedure.proname IN (
        'vestrace_start_managed_backup_set',
        'vestrace_reserve_backup_archive_append',
        'vestrace_abandon_backup_archive_append',
        'vestrace_list_pending_backup_archive_appends',
        'vestrace_commit_backup_archive_checkpoint',
        'vestrace_acquire_managed_backup_restore_hold',
        'vestrace_begin_managed_backup_sealing',
        'vestrace_commit_managed_backup_sealed',
        'vestrace_prepare_managed_backup_deletion',
        'vestrace_prepare_archive_key_erasure',
        'vestrace_record_archive_key_erased',
        'vestrace_list_prepared_backup_archive_objects',
        'vestrace_finalize_managed_backup_deleted'
      )
      AND (
        owner_role.rolname IS DISTINCT FROM 'vestrace_guarded_owner'
        OR has_function_privilege('vestrace', procedure.oid, 'EXECUTE')
        OR NOT has_function_privilege('vestrace_safety_supervisor', procedure.oid, 'EXECUTE')
      )
  ) THEN
    RAISE EXCEPTION 'P05 archive procedure ownership or execute grants are incompatible' USING ERRCODE = '42501';
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_trigger
    WHERE tgrelid = 'public.managed_backup_events'::regclass
      AND tgname = 'managed_backup_events_immutable'
      AND NOT tgisinternal
  ) THEN
    RAISE EXCEPTION 'P05 archive event journal is not immutable' USING ERRCODE = '42501';
  END IF;
END
$p05_archive$;
