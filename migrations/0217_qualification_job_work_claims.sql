-- Qualification jobs have no lease/claim mechanism today: nothing in any
-- production binary ever executes QualificationJobService::run_next_probe;
-- its only callers are integration tests. This migration adds the same
-- claim/finish shape migrations/0199_embedding_executor_work.sql already
-- proved for embedding work, simplified to one work kind.

CREATE TABLE qualification_job_work_claims (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    claim_owner TEXT NOT NULL CHECK (length(btrim(claim_owner)) BETWEEN 1 AND 128),
    claim_deadline TIMESTAMPTZ NOT NULL,
    last_outcome TEXT CHECK (last_outcome IN ('completed', 'retryable_failure', 'definite_failure')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, job_id),
    FOREIGN KEY (workspace_id, job_id) REFERENCES qualification_jobs(workspace_id, id) ON DELETE RESTRICT,
    CHECK (claim_deadline > created_at)
);

CREATE INDEX qualification_job_work_claims_available
    ON qualification_job_work_claims(workspace_id, claim_deadline, job_id);

ALTER TABLE qualification_job_work_claims ENABLE ROW LEVEL SECURITY;

CREATE POLICY qualification_job_work_claims_workspace_isolation ON qualification_job_work_claims
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE FUNCTION vestrace_claim_qualification_work(
    target_workspace UUID, target_owner TEXT, target_limit INTEGER
) RETURNS TABLE(job_id UUID, claim_owner TEXT, claim_deadline TIMESTAMPTZ)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
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
END $$;

CREATE FUNCTION vestrace_finish_qualification_work(
    target_workspace UUID, target_job UUID, target_owner TEXT, target_outcome TEXT
) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
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
END $$;

-- Production migrations run as the `vestrace` role itself (there is no
-- separate migration-admin credential), so without this closing block the
-- new table and both new functions would end up owned by `vestrace` with
-- none of the hardening `migrations/0199_embedding_executor_work.sql` already
-- applies to its own analogous claim table. `ENABLE ROW LEVEL SECURITY`
-- alone exempts the table's owner from its own policies; `FORCE ROW LEVEL
-- SECURITY` is what actually binds the policy to the role that will run
-- every production query once ownership moves off `vestrace` -- the same
-- defect class `migrations/0139_force_rls_on_scoped_tables.sql` had to fix
-- codebase-wide. Reusing `vestrace_reject_raw_p03_mutation` (already defined,
-- already guarding `qualification_jobs` itself in migration 0177) means a
-- raw write is refused twice over: once by the table ACL, once by the
-- trigger, exactly like every other P03-family guarded table in this schema.
ALTER TABLE qualification_job_work_claims FORCE ROW LEVEL SECURITY;
REVOKE ALL ON TABLE qualification_job_work_claims FROM PUBLIC, vestrace;
ALTER TABLE qualification_job_work_claims OWNER TO vestrace_guarded_owner;
CREATE TRIGGER qualification_job_work_claims_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON qualification_job_work_claims
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
ALTER FUNCTION vestrace_claim_qualification_work(uuid,text,integer) OWNER TO vestrace_guarded_owner;
ALTER FUNCTION vestrace_finish_qualification_work(uuid,uuid,text,text) OWNER TO vestrace_guarded_owner;
REVOKE ALL ON FUNCTION vestrace_claim_qualification_work(uuid,text,integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finish_qualification_work(uuid,uuid,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_claim_qualification_work(uuid,text,integer) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finish_qualification_work(uuid,uuid,text,text) TO vestrace;
