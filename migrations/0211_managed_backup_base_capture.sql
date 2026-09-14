-- P05-B base-capture completion installs the table-bound guards after 0209
-- through the provisioned guarded owner; the migration role has no DDL grant.
SELECT public.vestrace_install_p05_base_capture_guards();

DO $p05_base_capture$
BEGIN
  IF to_regprocedure('public.vestrace_enforce_managed_backup_base_checkpoint()') IS NULL
     OR to_regprocedure('public.vestrace_enforce_managed_backup_base_ready()') IS NULL
     OR to_regprocedure('public.vestrace_enforce_managed_backup_lifecycle_base_ready()') IS NULL
     OR to_regprocedure('public.vestrace_install_p05_base_capture_guards()') IS NULL THEN
    RAISE EXCEPTION 'P05 base-checkpoint enforcement functions are unavailable' USING ERRCODE = '42501';
  END IF;
  IF NOT EXISTS (
    SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_backup_archive_objects'::regclass
      AND tgname='managed_backup_base_checkpoint_required' AND NOT tgisinternal
  ) OR NOT EXISTS (
    SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_backup_restore_holds'::regclass
      AND tgname='managed_backup_hold_requires_base_checkpoint' AND NOT tgisinternal
  ) OR NOT EXISTS (
    SELECT 1 FROM pg_trigger WHERE tgrelid='public.managed_backup_sets'::regclass
      AND tgname='managed_backup_lifecycle_requires_base_checkpoint' AND NOT tgisinternal
  ) THEN
    RAISE EXCEPTION 'P05 base-checkpoint trigger enforcement is unavailable' USING ERRCODE = '42501';
  END IF;
END
$p05_base_capture$;
