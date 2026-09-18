-- P04 Task 8: job-owned retrieval fences, terminal results, and one retry.
--
-- The inherited fence is request-level: vector retrieval resolves the active
-- canonical space and its Ready generation on the way past, and nothing
-- durable records which generation an attempt was admitted against.  That is
-- enough to refuse a query, but not enough to say afterwards what a stored
-- answer was an answer to.
--
-- Here every attempt takes a fence, one-to-one with the job that answers it.
-- A result may only become terminal while that exact pinned generation is
-- still current, so a generation that moves under an in-flight attempt closes
-- it as changed rather than letting a stale answer land.  A job carries a
-- result or a generation change, never both.
--
-- Nothing in this migration stores a query vector, a query digest, or any
-- candidate content: a result is an ordered list of safe references, ranks and
-- scores.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_retrieval_results_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_retrieval_results_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding retrieval results upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

CREATE TABLE embedding_retrieval_fences (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    job_id UUID NOT NULL,
    request_id UUID NOT NULL,
    space_registration_id UUID NOT NULL,
    generation_id UUID NOT NULL,
    generation_epoch BIGINT NOT NULL CHECK(generation_epoch >= 0),
    guard_version BIGINT NOT NULL CHECK(guard_version > 0),
    corpus_revision BIGINT NOT NULL CHECK(corpus_revision >= 0),
    member_count BIGINT NOT NULL CHECK(member_count >= 0),
    deadline TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_retrieval_fences_workspace_id_id_key UNIQUE (workspace_id, id),
    -- One fence per job, and one logical request identity per fence.
    CONSTRAINT embedding_retrieval_fences_job_key UNIQUE (workspace_id, job_id),
    CONSTRAINT embedding_retrieval_fences_request_key UNIQUE (workspace_id, request_id),
    CONSTRAINT embedding_retrieval_fences_job_fkey
        FOREIGN KEY (workspace_id, job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_retrieval_fences_space_fkey
        FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_retrieval_fences_generation_fkey
        FOREIGN KEY (workspace_id, generation_id)
        REFERENCES embedding_corpus_generations(workspace_id, id) ON DELETE RESTRICT
);

CREATE TABLE embedding_retrieval_results (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    job_id UUID NOT NULL,
    fence_id UUID NOT NULL,
    reference_count INTEGER NOT NULL CHECK(reference_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_retrieval_results_workspace_id_id_key UNIQUE (workspace_id, id),
    -- One terminal result per job, and per fence.
    CONSTRAINT embedding_retrieval_results_job_key UNIQUE (workspace_id, job_id),
    CONSTRAINT embedding_retrieval_results_fence_key UNIQUE (workspace_id, fence_id),
    CONSTRAINT embedding_retrieval_results_fence_fkey
        FOREIGN KEY (workspace_id, fence_id)
        REFERENCES embedding_retrieval_fences(workspace_id, id) ON DELETE RESTRICT
);

CREATE TABLE embedding_retrieval_result_references (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    result_id UUID NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    memory_id UUID NOT NULL,
    revision_id UUID NOT NULL,
    rank INTEGER NOT NULL CHECK(rank >= 0),
    score DOUBLE PRECISION NOT NULL CHECK(score = score),
    PRIMARY KEY (workspace_id, result_id, ordinal),
    CONSTRAINT embedding_retrieval_result_references_result_fkey
        FOREIGN KEY (workspace_id, result_id)
        REFERENCES embedding_retrieval_results(workspace_id, id) ON DELETE RESTRICT,
    -- One revision appears at most once in one result.
    CONSTRAINT embedding_retrieval_result_references_revision_key
        UNIQUE (workspace_id, result_id, revision_id)
);

CREATE TABLE embedding_retrieval_generation_changes (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    job_id UUID NOT NULL,
    fence_id UUID NOT NULL,
    reason TEXT NOT NULL CHECK(reason IN (
        'stale','revoked','replaced','corpus_changed','member_unavailable'
    )),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_retrieval_generation_changes_workspace_id_id_key UNIQUE (workspace_id, id),
    -- One terminal change per job, and per fence.
    CONSTRAINT embedding_retrieval_generation_changes_job_key UNIQUE (workspace_id, job_id),
    CONSTRAINT embedding_retrieval_generation_changes_fence_key UNIQUE (workspace_id, fence_id),
    CONSTRAINT embedding_retrieval_generation_changes_fence_fkey
        FOREIGN KEY (workspace_id, fence_id)
        REFERENCES embedding_retrieval_fences(workspace_id, id) ON DELETE RESTRICT
);

CREATE TABLE embedding_retrieval_retry_edges (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    predecessor_job_id UUID NOT NULL,
    successor_job_id UUID NOT NULL,
    successor_request_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK(btrim(idempotency_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- One successor per predecessor, for all time.
    PRIMARY KEY (workspace_id, predecessor_job_id),
    CONSTRAINT embedding_retrieval_retry_edges_successor_key
        UNIQUE (workspace_id, successor_job_id),
    CONSTRAINT embedding_retrieval_retry_edges_distinct CHECK(
        predecessor_job_id <> successor_job_id
    )
);

DO $rls$
DECLARE target REGCLASS;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'embedding_retrieval_fences'::REGCLASS,
        'embedding_retrieval_results'::REGCLASS,
        'embedding_retrieval_result_references'::REGCLASS,
        'embedding_retrieval_generation_changes'::REGCLASS,
        'embedding_retrieval_retry_edges'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', target);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', target);
        EXECUTE format(
            'CREATE POLICY %s_workspace_policy ON %s '
            'USING(workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',TRUE),'''')::UUID) '
            'WITH CHECK(workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',TRUE),'''')::UUID)',
            target::TEXT, target);
        EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC', target);
        EXECUTE format(
            'CREATE TRIGGER %s_guarded BEFORE INSERT OR UPDATE OR DELETE ON %s '
            'FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation()',
            target::TEXT, target);
    END LOOP;
END
$rls$;

-- Admission: pin the generation that is current right now, one-to-one with the
-- job that will answer.  Refusal here is the closed vocabulary the client maps
-- to a degradation; it never invents a reason of its own.
CREATE OR REPLACE FUNCTION vestrace_accept_embedding_retrieval_attempt(
    target_workspace UUID,
    target_job UUID,
    target_request UUID,
    target_space UUID,
    target_deadline TIMESTAMPTZ
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    job_row embedding_jobs%ROWTYPE;
    guard_row embedding_index_generation_guards%ROWTYPE;
    generation_row embedding_corpus_generations%ROWTYPE;
    corpus_row embedding_space_corpus_states%ROWTYPE;
    fence_id UUID;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_request IS NULL
       OR target_space IS NULL OR target_deadline IS NULL THEN
        RAISE EXCEPTION 'embedding retrieval acceptance arguments are malformed'
            USING ERRCODE='22023';
    END IF;
    SELECT * INTO job_row FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
    IF NOT FOUND OR job_row.kind <> 'retrieval_query' THEN
        RAISE EXCEPTION 'embedding retrieval acceptance requires its exact retrieval_query job'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO guard_row FROM embedding_index_generation_guards
     WHERE workspace_id=target_workspace AND space_registration_id=target_space FOR SHARE;
    IF NOT FOUND OR guard_row.current_generation_id IS NULL THEN
        RAISE EXCEPTION 'embedding retrieval acceptance requires a Ready current generation'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO generation_row FROM embedding_corpus_generations
     WHERE workspace_id=target_workspace AND id=guard_row.current_generation_id FOR SHARE;
    IF NOT FOUND OR generation_row.state <> 'ready'
       OR generation_row.space_registration_id <> target_space
       OR generation_row.generation_epoch IS DISTINCT FROM guard_row.generation_epoch THEN
        RAISE EXCEPTION 'embedding retrieval acceptance requires a Ready current generation'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO corpus_row FROM embedding_space_corpus_states
     WHERE workspace_id=target_workspace AND space_registration_id=target_space FOR SHARE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding retrieval acceptance requires its exact corpus state'
            USING ERRCODE='23514';
    END IF;

    fence_id := gen_random_uuid();
    INSERT INTO embedding_retrieval_fences(
        id, workspace_id, job_id, request_id, space_registration_id, generation_id,
        generation_epoch, guard_version, corpus_revision, member_count, deadline
    ) VALUES (
        fence_id, target_workspace, target_job, target_request, target_space,
        generation_row.id, generation_row.generation_epoch, guard_row.guard_version,
        generation_row.corpus_revision, generation_row.member_count, target_deadline
    );
    RETURN fence_id;
END
$$;

-- Terminal result.  The pinned generation must still be exactly current: this
-- is the moment the fence is cashed, and a generation that moved while the
-- local search ran closes the attempt instead of storing its answer.
CREATE OR REPLACE FUNCTION vestrace_finalize_embedding_retrieval_result(
    target_workspace UUID,
    target_job UUID,
    target_fence UUID,
    target_memory_ids UUID[],
    target_revision_ids UUID[],
    target_ranks BIGINT[],
    target_scores DOUBLE PRECISION[]
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    fence_row embedding_retrieval_fences%ROWTYPE;
    guard_row embedding_index_generation_guards%ROWTYPE;
    generation_row embedding_corpus_generations%ROWTYPE;
    result_id UUID;
    total INTEGER;
    position INTEGER;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_fence IS NULL
       OR target_memory_ids IS NULL OR target_revision_ids IS NULL
       OR target_ranks IS NULL OR target_scores IS NULL THEN
        RAISE EXCEPTION 'embedding retrieval finalization arguments are malformed'
            USING ERRCODE='22023';
    END IF;
    total := cardinality(target_memory_ids);
    IF cardinality(target_revision_ids) <> total
       OR cardinality(target_ranks) <> total
       OR cardinality(target_scores) <> total THEN
        RAISE EXCEPTION 'embedding retrieval finalization requires parallel reference arrays'
            USING ERRCODE='22023';
    END IF;

    SELECT * INTO fence_row FROM embedding_retrieval_fences
     WHERE workspace_id=target_workspace AND id=target_fence AND job_id=target_job
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding retrieval finalization requires its exact fence'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_retrieval_generation_changes
               WHERE workspace_id=target_workspace AND job_id=target_job) THEN
        RAISE EXCEPTION 'embedding retrieval attempt already closed as generation changed'
            USING ERRCODE='23514';
    END IF;

    -- Revalidate the exact pinned tuple.
    SELECT * INTO guard_row FROM embedding_index_generation_guards
     WHERE workspace_id=target_workspace AND space_registration_id=fence_row.space_registration_id
     FOR SHARE;
    SELECT * INTO generation_row FROM embedding_corpus_generations
     WHERE workspace_id=target_workspace AND id=fence_row.generation_id FOR SHARE;
    IF guard_row.current_generation_id IS DISTINCT FROM fence_row.generation_id
       OR guard_row.guard_version <> fence_row.guard_version
       OR generation_row.state <> 'ready'
       OR generation_row.generation_epoch <> fence_row.generation_epoch
       OR generation_row.corpus_revision <> fence_row.corpus_revision
       OR generation_row.member_count <> fence_row.member_count THEN
        RAISE EXCEPTION 'embedding retrieval result requires its exact pinned generation'
            USING ERRCODE='23514';
    END IF;

    result_id := gen_random_uuid();
    INSERT INTO embedding_retrieval_results(id, workspace_id, job_id, fence_id, reference_count)
    VALUES (result_id, target_workspace, target_job, target_fence, total);
    IF total > 0 THEN
        FOR position IN 1..total LOOP
            INSERT INTO embedding_retrieval_result_references(
                workspace_id, result_id, ordinal, memory_id, revision_id, rank, score
            ) VALUES (
                target_workspace, result_id, position - 1,
                target_memory_ids[position], target_revision_ids[position],
                target_ranks[position]::INTEGER, target_scores[position]
            );
        END LOOP;
    END IF;
    UPDATE embedding_jobs SET state='succeeded', version=version+1
     WHERE workspace_id=target_workspace AND id=target_job;
    RETURN result_id;
END
$$;

-- The other terminal branch.  A confirmed generation change closes the attempt
-- and is the only degradation an authorized retry may follow.
CREATE OR REPLACE FUNCTION vestrace_observe_embedding_retrieval_generation_change(
    target_workspace UUID,
    target_job UUID,
    target_fence UUID,
    target_reason TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    fence_row embedding_retrieval_fences%ROWTYPE;
    observation_id UUID;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_fence IS NULL
       OR target_reason IS NULL THEN
        RAISE EXCEPTION 'embedding retrieval observation arguments are malformed'
            USING ERRCODE='22023';
    END IF;
    SELECT * INTO fence_row FROM embedding_retrieval_fences
     WHERE workspace_id=target_workspace AND id=target_fence AND job_id=target_job
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding retrieval observation requires its exact fence'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_retrieval_results
               WHERE workspace_id=target_workspace AND job_id=target_job) THEN
        RAISE EXCEPTION 'embedding retrieval attempt already produced a terminal result'
            USING ERRCODE='23514';
    END IF;
    observation_id := gen_random_uuid();
    INSERT INTO embedding_retrieval_generation_changes(
        id, workspace_id, job_id, fence_id, reason
    ) VALUES (observation_id, target_workspace, target_job, target_fence, target_reason);
    UPDATE embedding_jobs SET state='failed_definite', version=version+1
     WHERE workspace_id=target_workspace AND id=target_job;
    RETURN observation_id;
END
$$;

-- One authorized successor.  A replay under the same key returns the same
-- successor; a different key against the same predecessor conflicts.
CREATE OR REPLACE FUNCTION vestrace_authorize_embedding_retrieval_retry(
    target_workspace UUID,
    target_predecessor UUID,
    target_successor UUID,
    target_successor_request UUID,
    target_key TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing embedding_retrieval_retry_edges%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_predecessor IS NULL OR target_successor IS NULL
       OR target_successor_request IS NULL OR target_key IS NULL OR btrim(target_key) = '' THEN
        RAISE EXCEPTION 'embedding retrieval retry arguments are malformed'
            USING ERRCODE='22023';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM embedding_retrieval_generation_changes
                   WHERE workspace_id=target_workspace AND job_id=target_predecessor) THEN
        RAISE EXCEPTION 'embedding retrieval retry requires a confirmed generation change'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_retrieval_retry_edges
     WHERE workspace_id=target_workspace AND predecessor_job_id=target_predecessor
     FOR UPDATE;
    IF FOUND THEN
        IF existing.idempotency_key <> target_key THEN
            RAISE EXCEPTION 'embedding retrieval retry predecessor already has its successor'
                USING ERRCODE='23505';
        END IF;
        RETURN existing.successor_job_id;
    END IF;
    INSERT INTO embedding_retrieval_retry_edges(
        workspace_id, predecessor_job_id, successor_job_id, successor_request_id, idempotency_key
    ) VALUES (
        target_workspace, target_predecessor, target_successor, target_successor_request, target_key
    );
    RETURN target_successor;
END
$$;

DO $upgrade$
DECLARE target REGCLASS; target_function REGPROCEDURE;
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_retrieval_results_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_retrieval_results_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
            RAISE EXCEPTION 'embedding retrieval results ownership hand-back is unavailable'
                USING ERRCODE='42501';
        END IF;
        FOREACH target IN ARRAY ARRAY[
            'embedding_retrieval_fences'::REGCLASS,
            'embedding_retrieval_results'::REGCLASS,
            'embedding_retrieval_result_references'::REGCLASS,
            'embedding_retrieval_generation_changes'::REGCLASS,
            'embedding_retrieval_retry_edges'::REGCLASS
        ] LOOP
            EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner', target);
            EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner', target);
            EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC,vestrace', target);
            EXECUTE format('GRANT SELECT, REFERENCES ON TABLE %s TO vestrace', target);
        END LOOP;
        FOREACH target_function IN ARRAY ARRAY[
            'vestrace_accept_embedding_retrieval_attempt(uuid,uuid,uuid,uuid,timestamptz)'::REGPROCEDURE,
            'vestrace_finalize_embedding_retrieval_result(uuid,uuid,uuid,uuid[],uuid[],bigint[],double precision[])'::REGPROCEDURE,
            'vestrace_observe_embedding_retrieval_generation_change(uuid,uuid,uuid,text)'::REGPROCEDURE,
            'vestrace_authorize_embedding_retrieval_retry(uuid,uuid,uuid,uuid,text)'::REGPROCEDURE
        ] LOOP
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function);
            EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace', target_function);
            EXECUTE format('GRANT EXECUTE ON FUNCTION %s TO vestrace', target_function);
        END LOOP;
    END IF;
END
$upgrade$;
