-- P05 safety authority objects are constructed only by the quiesced bootstrap installer.
DO $p05$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vestrace_safety_verify')
     OR to_regprocedure('public.vestrace_safety_ed25519_verify(bytea,bytea,bytea)') IS NULL THEN
    RAISE EXCEPTION 'P05 safety verifier extension must be provisioned before migration' USING ERRCODE = '42501';
  END IF;
  IF EXISTS (
    SELECT 1
    FROM pg_proc procedure
    JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
    LEFT JOIN pg_roles owner_role ON owner_role.oid = procedure.proowner
    WHERE namespace.nspname = 'public'
      AND procedure.proname = 'vestrace_safety_ed25519_verify'
      AND (owner_role.rolname IS DISTINCT FROM 'vestrace_guarded_owner'
           OR has_function_privilege('vestrace', procedure.oid, 'EXECUTE')
           OR has_function_privilege('vestrace_safety_supervisor', procedure.oid, 'EXECUTE'))
  ) THEN
    RAISE EXCEPTION 'P05 verifier ownership or direct execute grants are incompatible' USING ERRCODE = '42501';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vestrace_safety_supervisor') THEN
    RAISE EXCEPTION 'P05 safety supervisor role must be provisioned before migration' USING ERRCODE = '42501';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_namespace n JOIN pg_roles r ON r.oid = n.nspowner WHERE n.nspname = 'public' AND r.rolname = 'vestrace_guarded_owner')
     OR has_schema_privilege('vestrace', 'public', 'CREATE') THEN
    RAISE EXCEPTION 'P05 migration requires guarded public schema ownership without runtime CREATE' USING ERRCODE = '42501';
  END IF;
  IF to_regclass('public.installation_safety_state') IS NULL
     OR to_regclass('public.installation_safety_generations') IS NULL
     OR to_regclass('public.installation_safety_journal_events') IS NULL THEN
    RAISE EXCEPTION 'P05 safety installer has not created the required authority tables' USING ERRCODE = '42501';
  END IF;
  IF to_regprocedure('public.vestrace_assert_installation_supervisor_context()') IS NULL THEN
    RAISE EXCEPTION 'P05 safety installer has not created the supervisor context guard' USING ERRCODE = '42501';
  END IF;
  IF to_regprocedure('public.vestrace_initialize_installation_safety(bytea,uuid,uuid,bytea,uuid,bytea,bytea,bigint,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL
     OR to_regprocedure('public.vestrace_register_database_generation(uuid,uuid,bytea,uuid,bigint,bytea,bytea,uuid,bigint,bytea,bytea)') IS NULL THEN
    RAISE EXCEPTION 'P05 safety installer has not created the guarded authority functions' USING ERRCODE = '42501';
  END IF;
  IF EXISTS (
    SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
    LEFT JOIN pg_roles r ON r.oid = c.relowner
    WHERE n.nspname = 'public' AND c.relname IN ('installation_safety_state', 'installation_safety_generations', 'installation_safety_journal_events')
      AND (r.rolname IS DISTINCT FROM 'vestrace_guarded_owner' OR NOT c.relrowsecurity OR NOT c.relforcerowsecurity)
  ) THEN
    RAISE EXCEPTION 'P05 safety authority table ownership or RLS is incompatible' USING ERRCODE = '42501';
  END IF;
END
$p05$;
