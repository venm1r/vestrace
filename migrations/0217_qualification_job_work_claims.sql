-- Qualification jobs have no lease/claim mechanism today: nothing in any
-- production binary ever executes QualificationJobService::run_next_probe;
-- its only callers are integration tests. This migration adds the same
-- claim/finish shape migrations/0199_embedding_executor_work.sql already
-- proved for embedding work, simplified to one work kind.
--
-- Installs the guards through the provisioned guarded owner, exactly as
-- every P05 (0209+) migration does -- the migration role has no DDL grant
-- on public itself, only on objects owned by vestrace_guarded_owner via a
-- SECURITY DEFINER installer. See docker/postgres/init-runtime-role.sh's
-- vestrace_install_p05_qualification_work_claims for the authoritative
-- copy of the schema this creates (table, RLS, guard trigger, and the two
-- claim/finish functions).
--
-- Like 0216, this must also work against a fresh #[sqlx::test] database:
-- QUALIFICATION_WORK_CLAIMS_HISTORICAL_MIGRATOR (crates/vestrace-
-- infrastructure/src/postgres/pool.rs) is the first bounded migrator built
-- to include this migration in that harness, and #[sqlx::test] creates each
-- database with a plain `CREATE DATABASE`, which Postgres clones from
-- `template1` -- never from the provisioned
-- docker/postgres/init-runtime-role.sh state, so the guards function does
-- not exist there. The fallback below performs the identical DDL directly;
-- it is reached only when the authoritative function is absent AND the
-- connecting role is a superuser (true for every #[sqlx::test] database,
-- never true for the restricted runtime role in any real deployment), so
-- it introduces no privilege the production path does not already have to
-- defend against.
DO $$
BEGIN
    PERFORM public.vestrace_install_p05_qualification_work_claims();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE EXCEPTION 'qualification work claims guards must be provisioned before runtime migration'
            USING ERRCODE = '42501';
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
        EXECUTE 'ALTER TABLE public.qualification_job_work_claims OWNER TO vestrace_guarded_owner';
        EXECUTE 'REVOKE ALL ON TABLE public.qualification_job_work_claims FROM PUBLIC, vestrace';
        EXECUTE 'CREATE TRIGGER qualification_job_work_claims_guarded
            BEFORE INSERT OR UPDATE OR DELETE ON public.qualification_job_work_claims
            FOR EACH ROW EXECUTE FUNCTION public.vestrace_reject_raw_p03_mutation()';
    END IF;

    EXECUTE $clmfn$
    CREATE FUNCTION public.vestrace_claim_qualification_work(
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
    CREATE FUNCTION public.vestrace_finish_qualification_work(
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

    EXECUTE 'ALTER FUNCTION public.vestrace_claim_qualification_work(uuid,text,integer) OWNER TO vestrace_guarded_owner';
    EXECUTE 'ALTER FUNCTION public.vestrace_finish_qualification_work(uuid,uuid,text,text) OWNER TO vestrace_guarded_owner';
    EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_claim_qualification_work(uuid,text,integer) FROM PUBLIC';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_claim_qualification_work(uuid,text,integer) TO vestrace';
    EXECUTE 'REVOKE ALL ON FUNCTION public.vestrace_finish_qualification_work(uuid,uuid,text,text) FROM PUBLIC';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.vestrace_finish_qualification_work(uuid,uuid,text,text) TO vestrace';
END $$;
