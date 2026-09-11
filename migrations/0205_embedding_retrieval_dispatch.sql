-- P04 Task 11: let the worker claim the retrieval-query jobs it is the only
-- thing able to answer.
--
-- Migration 0199 opened dispatch claims to `delivery` and `rebuild` and said
-- why the others were closed: "the remaining closed kinds have no producer
-- until their later migrations, so returning no row is closed, not an implicit
-- claim over a future table". Migration 0202 built that producer. A
-- `retrieval_query` job, its external effect, its model-request evidence and
-- its generation fence are all durable now, and nothing claims them, so every
-- one of them sits in `requested` for ever while the surface that created it
-- waits out its budget and reports a degradation it invented.
--
-- The corresponding half is not in SQL and cannot be: a retrieval query is
-- answered from a process-local index, so only a worker holding one may take
-- the claim. `EmbeddingExecutor` refuses such a job before dispatch when no
-- local index is composed, which is why a claim being available is safe rather
-- than a promise every build can keep.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_retrieval_dispatch_upgrade()')
       IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_retrieval_dispatch_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding retrieval dispatch upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

-- Forward-replace 0199's candidate set.
--
-- Two conditions, not one. A retrieval job is claimable only once its fence
-- exists, because the fence is what pins the generation the answer must come
-- from -- dispatching before it exists would embed a query against a corpus
-- nothing had agreed on. And only while that fence's deadline is ahead, because
-- past it nobody is waiting and the provider call would be paid for an answer
-- with no reader.
--
-- Written as a replacement rather than a rewrite so the rest of the claim --
-- the skip-locked candidate order, the lease renewal, the conflict rule --
-- stays exactly the text 0199 proved, and so a needle that stopped matching
-- fails loudly instead of silently reinstating the old candidate set.
-- The same replacement repairs a defect it did not cause.
--
-- `vestrace_claim_embedding_work` declares `job_id` as an OUT parameter and
-- then names an unqualified `job_id` three times inside its query, so plpgsql
-- refuses the whole statement with 42702 before it examines a single row. Every
-- call fails, which means the worker dispatch cycle Task 5 built has never
-- claimed anything -- no test had ever executed the function, and the cycle's
-- own error handling turns the refusal into an ordinary retryable failure, so a
-- running worker reports a backlog it can never drain rather than a fault.
--
-- Two of the three are repaired by naming what was meant: the conflict target
-- is the primary key, and the final projection is the CTE's own columns. The
-- `RETURNING` clause was already qualified and is left alone.
DO $forward_replace_claim$
DECLARE
    original TEXT;
    replaced TEXT;
    needle CONSTANT TEXT := 'AND job.kind IN (''delivery'',''rebuild'')';
    replacement CONSTANT TEXT :=
        'AND (job.kind IN (''delivery'',''rebuild'')'
        '     OR (job.kind=''retrieval_query'' AND EXISTS('
        '          SELECT 1 FROM embedding_retrieval_fences AS fence'
        '           WHERE fence.workspace_id=job.workspace_id'
        '             AND fence.job_id=job.id'
        '             AND fence.deadline>now())))';
    conflict_needle CONSTANT TEXT := 'ON CONFLICT(workspace_id,job_id,work_kind) DO UPDATE';
    conflict_replacement CONSTANT TEXT :=
        'ON CONFLICT ON CONSTRAINT embedding_job_work_claims_pkey DO UPDATE';
    projection_needle CONSTANT TEXT :=
        'SELECT job_id,work_kind,claim_owner,claim_deadline FROM claimed ORDER BY job_id';
    projection_replacement CONSTANT TEXT :=
        'SELECT claimed.job_id,claimed.work_kind,claimed.claim_owner,claimed.claim_deadline'
        ' FROM claimed ORDER BY claimed.job_id';
BEGIN
    original := pg_get_functiondef(
        'public.vestrace_claim_embedding_work(uuid,text,text,integer)'::REGPROCEDURE);
    IF position(needle IN original) = 0 THEN
        RAISE EXCEPTION
            'the 0199 dispatch candidate set was not found; retrieval jobs would stay unclaimed'
            USING ERRCODE='23514';
    END IF;
    IF position(conflict_needle IN original) = 0
       OR position(projection_needle IN original) = 0 THEN
        RAISE EXCEPTION
            'the 0199 ambiguous claim references were not found; the dispatch cycle would stay '
            'unable to claim anything'
            USING ERRCODE='23514';
    END IF;
    replaced := replace(original, needle, replacement);
    replaced := replace(replaced, conflict_needle, conflict_replacement);
    replaced := replace(replaced, projection_needle, projection_replacement);
    IF replaced = original OR position(replacement IN replaced) = 0
       OR position(conflict_replacement IN replaced) = 0
       OR position(projection_replacement IN replaced) = 0 THEN
        RAISE EXCEPTION 'the 0199 dispatch candidate set was not replaced'
            USING ERRCODE='23514';
    END IF;
    EXECUTE replaced;
    -- Read it back from the catalogue rather than trusting the string: EXECUTE
    -- could install a function that parses and still carries the old set.
    IF position(replacement IN pg_get_functiondef(
        'public.vestrace_claim_embedding_work(uuid,text,text,integer)'::REGPROCEDURE)) = 0
    THEN
        RAISE EXCEPTION 'the installed work claim does not admit a fenced retrieval query'
            USING ERRCODE='23514';
    END IF;
END
$forward_replace_claim$;

DO $ownership$
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_retrieval_dispatch_upgrade()')
       IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_retrieval_dispatch_upgrade();
    ELSE
        -- Fresh install: this migration runs as the superuser, so the function
        -- it replaced is restored to the posture 0199 gave it directly.
        ALTER FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
            OWNER TO vestrace_guarded_owner;
        -- And the privilege 0199's own ordering removed: it revoked ALL from
        -- the role that owned the table at that moment, which drops the
        -- owner's ACL entry, and the later ownership transfer had nothing left
        -- to carry. The guarded owner has been unable to insert a claim since.
        GRANT ALL ON TABLE public.embedding_job_work_claims TO vestrace_guarded_owner;
        -- Posture parity for the corpus-change stream. 0203's hand-back grants
        -- the runtime role SELECT on it, because the erasure sweep reads
        -- committed invalidation events every worker cycle; 0195's fresh-install
        -- branch never did, so a database migrated without the provisioner left
        -- that sweep refused with 42501 on every pass. Read-only, and the same
        -- read the upgrade path has always allowed.
        GRANT SELECT ON TABLE public.embedding_index_rebuild_events TO vestrace;
        REVOKE ALL ON FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
            FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_claim_embedding_work(uuid,text,text,integer)
            TO vestrace;
    END IF;
END
$ownership$;
