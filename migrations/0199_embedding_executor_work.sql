-- P04 Task 5: bounded embedding work claims and delivery/rebuild execution.
--
-- The 0203 erasure lineage does not exist at this point in the forward chain.
-- Consequently post-erasure replay is deliberately closed here: a Live
-- projection still requires its exact ciphertext and publication evidence.
DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_executor_upgrade()') IS NOT NULL THEN
        PERFORM vestrace_prepare_embedding_executor_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
        RAISE EXCEPTION 'embedding executor upgrade must be provisioned' USING ERRCODE='42501';
    END IF;
END $upgrade$;

ALTER TABLE embedding_delivery_acceptance_receipts
    DROP CONSTRAINT IF EXISTS embedding_delivery_acceptance_receipts_job_kind_check,
    ADD CONSTRAINT embedding_delivery_acceptance_receipts_job_kind_check
        CHECK(job_kind IN ('delivery','rebuild'));

-- A claim identifies one existing job for a bounded worker cycle.  It is not
-- provider authority: dispatch still revalidates the immutable effect and all
-- admission/policy/credential facts through ProviderDispatchRepository.
CREATE TABLE embedding_job_work_claims (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    work_kind TEXT NOT NULL CHECK(work_kind IN (
        'dispatch','reconcile_keys','finalize_result','build_index',
        'coordinate_transition','propagate_erasure')),
    claim_owner TEXT NOT NULL CHECK(length(btrim(claim_owner)) BETWEEN 1 AND 128),
    claim_deadline TIMESTAMPTZ NOT NULL,
    last_outcome TEXT CHECK(last_outcome IN ('completed','retryable_failure','definite_failure')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(workspace_id,job_id,work_kind),
    FOREIGN KEY(workspace_id,job_id) REFERENCES embedding_jobs(workspace_id,id) ON DELETE RESTRICT,
    CHECK(claim_deadline > created_at)
);
CREATE INDEX embedding_job_work_claims_available
    ON embedding_job_work_claims(workspace_id,work_kind,claim_deadline,job_id);

CREATE FUNCTION vestrace_claim_embedding_work(
    target_workspace UUID,target_kind TEXT,target_owner TEXT,target_limit INTEGER
) RETURNS TABLE(job_id UUID,kind TEXT,claim_owner TEXT,claim_deadline TIMESTAMPTZ)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE deadline TIMESTAMPTZ := now()+INTERVAL '60 seconds';
BEGIN
    IF target_workspace IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
       OR target_kind NOT IN ('dispatch','reconcile_keys','finalize_result','build_index','coordinate_transition','propagate_erasure')
       OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128
       OR target_limit IS NULL OR target_limit NOT BETWEEN 1 AND 128 THEN
        RAISE EXCEPTION 'embedding work claim arguments are malformed' USING ERRCODE='22023';
    END IF;
    -- Task 5 owns job dispatch claims.  The remaining closed kinds have no
    -- producer until their later migrations, so returning no row is closed,
    -- not an implicit claim over a future table.
    IF target_kind <> 'dispatch' THEN RETURN; END IF;
    RETURN QUERY
    WITH candidates AS (
        SELECT job.id
          FROM embedding_jobs AS job
         WHERE job.workspace_id=target_workspace
           AND job.kind IN ('delivery','rebuild')
           AND job.state IN ('requested','running')
           AND NOT EXISTS(
               SELECT 1 FROM embedding_job_work_claims AS claim
                WHERE claim.workspace_id=job.workspace_id AND claim.job_id=job.id
                  AND claim.work_kind=target_kind AND claim.claim_deadline>now())
         ORDER BY job.created_at,job.id
         FOR UPDATE SKIP LOCKED
         LIMIT target_limit
    ), claimed AS (
        INSERT INTO embedding_job_work_claims(
            workspace_id,job_id,work_kind,claim_owner,claim_deadline,last_outcome,created_at,updated_at)
        SELECT target_workspace,id,target_kind,target_owner,deadline,NULL,now(),now() FROM candidates
        ON CONFLICT(workspace_id,job_id,work_kind) DO UPDATE
           SET claim_owner=EXCLUDED.claim_owner,claim_deadline=EXCLUDED.claim_deadline,
               last_outcome=NULL,updated_at=now()
         WHERE embedding_job_work_claims.claim_deadline<=now()
        RETURNING embedding_job_work_claims.job_id,embedding_job_work_claims.work_kind,
                  embedding_job_work_claims.claim_owner,embedding_job_work_claims.claim_deadline
    )
    SELECT job_id,work_kind,claim_owner,claim_deadline FROM claimed ORDER BY job_id;
END $$;

CREATE FUNCTION vestrace_finish_embedding_work(
    target_workspace UUID,target_job UUID,target_kind TEXT,target_owner TEXT,target_outcome TEXT
) RETURNS BOOLEAN LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
       OR target_kind NOT IN ('dispatch','reconcile_keys','finalize_result','build_index','coordinate_transition','propagate_erasure')
       OR target_owner IS NULL OR length(btrim(target_owner)) NOT BETWEEN 1 AND 128
       OR target_outcome NOT IN ('completed','retryable_failure','definite_failure') THEN
        RAISE EXCEPTION 'embedding work completion arguments are malformed' USING ERRCODE='22023';
    END IF;
    IF target_outcome='retryable_failure' THEN
        UPDATE embedding_job_work_claims SET claim_deadline=now(),last_outcome=target_outcome,updated_at=now()
         WHERE workspace_id=target_workspace AND job_id=target_job AND work_kind=target_kind
           AND claim_owner=target_owner AND claim_deadline>now();
    ELSE
        DELETE FROM embedding_job_work_claims
         WHERE workspace_id=target_workspace AND job_id=target_job AND work_kind=target_kind
           AND claim_owner=target_owner AND claim_deadline>now();
    END IF;
    RETURN FOUND;
END $$;

-- Forward-replace the 0193/0194/0195 delivery-only validators.  The stored
-- definitions are re-installed under the upgrade owner after exact lexical
-- substitutions; 0203-only erasure lineage remains absent rather than being
-- made selectable by this migration.
DO $replace_delivery_only$
DECLARE target OID; definition TEXT;
BEGIN
    FOR target IN
        SELECT procedure.oid FROM pg_proc AS procedure
         JOIN pg_namespace AS namespace ON namespace.oid=procedure.pronamespace
         WHERE namespace.nspname='public' AND procedure.proname=ANY(ARRAY[
            'vestrace_begin_delivery_embedding_outputs',
            'vestrace_finalize_delivery_embedding_outputs',
            'vestrace_validate_embedding_credential_completion_owner',
            'vestrace_ensure_embedding_credential_completion_blocker',
            'vestrace_adopt_embedding_result_credential_blocker',
            'vestrace_assert_embedding_result_phase',
            'vestrace_lock_embedding_result_finalization',
            'vestrace_load_embedding_result_finalization',
            'vestrace_record_embedding_result_key_binding',
            'vestrace_publish_embedding_job_result',
            'vestrace_load_embedding_result_eligibility',
            'vestrace_lock_embedding_result_completion_authority',
            'vestrace_commit_embedding_result_preparation',
            'vestrace_lock_embedding_job_recovery_authority',
            'vestrace_validate_embedding_projection_dependency',
            'vestrace_validate_embedding_result_preparation'
         ])
    LOOP
        definition:=pg_get_functiondef(target);
        definition:=replace(definition,'target_kind IS DISTINCT FROM ''delivery''::text','target_kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'target_kind IS DISTINCT FROM ''delivery''','target_kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind=''delivery''::embedding_job_kind','job.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind = ''delivery''::embedding_job_kind','job.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind<>''delivery''::embedding_job_kind','job.kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind <> ''delivery''::embedding_job_kind','job.kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'initial.kind=''delivery''::embedding_job_kind','initial.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'initial.kind = ''delivery''::embedding_job_kind','initial.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'initial_job.kind<>''delivery''::embedding_job_kind','initial_job.kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'initial_job.kind <> ''delivery''::embedding_job_kind','initial_job.kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'kind=''delivery''::embedding_job_kind','kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'kind = ''delivery''::embedding_job_kind','kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'kind=''delivery''','kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'kind = ''delivery''','kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind=''delivery''','job.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind = ''delivery''','job.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind<>''delivery''','job.kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'job.kind <> ''delivery''','job.kind NOT IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'initial.kind=''delivery''','initial.kind IN (''delivery'',''rebuild'')');
        definition:=replace(definition,'initial.kind = ''delivery''','initial.kind IN (''delivery'',''rebuild'')');
        EXECUTE definition;
    END LOOP;
END $replace_delivery_only$;

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_executor_upgrade()') IS NOT NULL THEN
        PERFORM vestrace_finish_embedding_executor_upgrade();
    ELSE
        ALTER TABLE public.embedding_job_work_claims ENABLE ROW LEVEL SECURITY;
        ALTER TABLE public.embedding_job_work_claims FORCE ROW LEVEL SECURITY;
        CREATE POLICY embedding_job_work_claims_workspace_policy ON public.embedding_job_work_claims
            USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
            WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
        REVOKE ALL ON TABLE public.embedding_job_work_claims FROM PUBLIC,vestrace;
        CREATE TRIGGER embedding_job_work_claims_guarded
            BEFORE INSERT OR UPDATE OR DELETE ON public.embedding_job_work_claims
            FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
        ALTER TABLE public.embedding_job_work_claims OWNER TO vestrace_guarded_owner;
        ALTER FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer) OWNER TO vestrace_guarded_owner;
        ALTER FUNCTION public.vestrace_finish_embedding_work(uuid,uuid,text,text,text) OWNER TO vestrace_guarded_owner;
        REVOKE ALL ON FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer) FROM PUBLIC;
        REVOKE ALL ON FUNCTION public.vestrace_finish_embedding_work(uuid,uuid,text,text,text) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer) TO vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_finish_embedding_work(uuid,uuid,text,text,text) TO vestrace;
    END IF;
END $upgrade$;
