-- P05-D forward-only least-privilege read surface for host continuity readiness.
--
-- The supervisor could already mutate the safety authority and could not read
-- it back. Readiness needs the persisted row, so this admits exactly one
-- guarded owner function to return it -- not a table SELECT grant, which would
-- also expose every column the singleton gains later.
SELECT public.vestrace_install_p05_safety_readiness_read();
REVOKE CREATE ON SCHEMA public FROM vestrace_guarded_owner;

DO $p05_readiness$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version=214 AND success) THEN
    RAISE EXCEPTION 'P05 readiness read requires successful 0214' USING ERRCODE='42501';
  END IF;
  IF to_regprocedure('public.vestrace_read_installation_safety_readiness()') IS NULL THEN
    RAISE EXCEPTION 'P05 readiness read surface is unavailable' USING ERRCODE='42501';
  END IF;
  IF has_function_privilege('vestrace','public.vestrace_read_installation_safety_readiness()','EXECUTE')
     OR NOT has_function_privilege('vestrace_safety_supervisor','public.vestrace_read_installation_safety_readiness()','EXECUTE') THEN
    RAISE EXCEPTION 'P05 readiness read grant is not supervisor-only' USING ERRCODE='42501';
  END IF;
  -- SECURITY DEFINER with a caller-resolvable search_path would let the caller
  -- decide which `digest` and which `installation_safety_state` this reads.
  IF NOT EXISTS (
    SELECT 1
    FROM pg_proc procedure
    JOIN pg_namespace namespace ON namespace.oid = procedure.pronamespace
    JOIN pg_roles owner_role ON owner_role.oid = procedure.proowner
    WHERE namespace.nspname='public'
      AND procedure.proname='vestrace_read_installation_safety_readiness'
      AND procedure.prosecdef
      AND owner_role.rolname='vestrace_guarded_owner'
      AND procedure.proconfig @> ARRAY['search_path=pg_catalog, public']
  ) THEN
    RAISE EXCEPTION 'P05 readiness read is not a guarded owner function with a fixed search path' USING ERRCODE='42501';
  END IF;
  -- A read surface that took arguments could be steered; this one names no row.
  IF (SELECT pronargs FROM pg_proc WHERE oid='public.vestrace_read_installation_safety_readiness()'::regprocedure) <> 0 THEN
    RAISE EXCEPTION 'P05 readiness read must accept no caller-supplied arguments' USING ERRCODE='42501';
  END IF;
  -- Readiness must not become a second way to write the authority.
  IF NOT (SELECT provolatile='s' FROM pg_proc WHERE oid='public.vestrace_read_installation_safety_readiness()'::regprocedure) THEN
    RAISE EXCEPTION 'P05 readiness read must be declared STABLE' USING ERRCODE='42501';
  END IF;
  -- The mutation path stays exactly as narrow as it was; readiness adds no
  -- table privilege to the supervisor.
  IF has_table_privilege('vestrace_safety_supervisor','public.installation_safety_state','SELECT')
     OR has_table_privilege('vestrace_safety_supervisor','public.installation_safety_state','INSERT')
     OR has_table_privilege('vestrace_safety_supervisor','public.installation_safety_state','UPDATE')
     OR has_table_privilege('vestrace_safety_supervisor','public.installation_safety_state','DELETE') THEN
    RAISE EXCEPTION 'P05 readiness must not grant the supervisor safety-table privilege' USING ERRCODE='42501';
  END IF;
END $p05_readiness$;
