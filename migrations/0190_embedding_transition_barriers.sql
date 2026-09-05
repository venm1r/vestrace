-- P04 Task 8: barriers and supersession.
--
-- A barrier is durable history for work that must wait on a predecessor.  Its
-- recipe rows are immutable; only the header may make its one open-to-terminal
-- lifecycle transition.

DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_p04_transition_barriers_upgrade()') IS NOT NULL
       AND has_function_privilege(
           current_user,
           'public.vestrace_prepare_p04_transition_barriers_upgrade()',
           'EXECUTE'
       ) THEN
        PERFORM public.vestrace_prepare_p04_transition_barriers_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P04 transition-barrier ownership hand-back must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        ALTER FUNCTION public.vestrace_plan_embedding_transition_version(
            UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
            UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
            UUID, UUID[], JSONB
        ) OWNER TO vestrace;
        ALTER FUNCTION public.vestrace_classify_embedding_transition_ambiguity_carries(
            UUID, UUID, UUID
        ) OWNER TO vestrace;
        ALTER FUNCTION public.vestrace_acknowledge_carried_transition_batch_after_unknown(
            UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID
        ) OWNER TO vestrace;
    END IF;
END
$$;

-- SQLx applies migrations without the compose provisioner.  Its superuser
-- fallback changes ownership directly, so preserve the provisioner's runtime
-- ACL for the forward-replaced entry points in that test-only path.
DO $$
BEGIN
    IF COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        GRANT EXECUTE ON FUNCTION vestrace_plan_embedding_transition_version(
            UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
            UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
            UUID, UUID[], JSONB
        ) TO vestrace;
        GRANT EXECUTE ON FUNCTION vestrace_acknowledge_carried_transition_batch_after_unknown(
            UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID
        ) TO vestrace;
    END IF;
END
$$;

CREATE TABLE embedding_transition_barriers (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_id UUID NOT NULL,
    target_transition_plan_id UUID NOT NULL,
    predecessor_embedding_job_id UUID NOT NULL,
    predecessor_transition_batch_id UUID NOT NULL,
    barrier_transition_batch_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'awaiting_predecessor_terminal', 'resolved_to_carry',
        'resolved_satisfied_existing', 'resolved_definite',
        'no_longer_required', 'superseded'
    )),
    resolution_evidence TEXT,
    supersedes_barrier_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_transition_barriers_transition_fkey
        FOREIGN KEY (workspace_id, transition_id)
        REFERENCES embedding_transitions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_barriers_target_plan_fkey
        FOREIGN KEY (workspace_id, target_transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_barriers_predecessor_job_fkey
        FOREIGN KEY (workspace_id, predecessor_embedding_job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_barriers_supersedes_fkey
        FOREIGN KEY (workspace_id, supersedes_barrier_id)
        REFERENCES embedding_transition_barriers(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_barriers_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT embedding_transition_barriers_open_has_no_resolution CHECK (
        (state = 'awaiting_predecessor_terminal' AND resolution_evidence IS NULL)
        OR (state <> 'awaiting_predecessor_terminal' AND resolution_evidence IS NOT NULL)
    ),
    CONSTRAINT embedding_transition_barriers_batch_is_dedicated UNIQUE (
        workspace_id, barrier_transition_batch_id
    )
);

CREATE UNIQUE INDEX embedding_transition_barriers_one_open_predecessor_lineage
    ON embedding_transition_barriers (
        workspace_id, predecessor_embedding_job_id, predecessor_transition_batch_id
    ) WHERE state = 'awaiting_predecessor_terminal';

CREATE TABLE embedding_transition_barrier_recipes (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    barrier_id UUID NOT NULL,
    old_recipe_ordinal BIGINT NOT NULL CHECK (old_recipe_ordinal >= 0),
    old_input_ordinal BIGINT NOT NULL CHECK (old_input_ordinal >= 0),
    new_recipe_ordinal BIGINT NOT NULL CHECK (new_recipe_ordinal >= 0),
    new_input_ordinal BIGINT NOT NULL CHECK (new_input_ordinal >= 0),
    PRIMARY KEY (workspace_id, barrier_id, old_recipe_ordinal, old_input_ordinal),
    CONSTRAINT embedding_transition_barrier_recipes_header_fkey
        FOREIGN KEY (workspace_id, barrier_id)
        REFERENCES embedding_transition_barriers(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_barrier_recipes_new_mapping_unique
        UNIQUE (workspace_id, barrier_id, new_recipe_ordinal, new_input_ordinal)
);

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_transition_barrier_header()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.state <> 'awaiting_predecessor_terminal' THEN
            RAISE EXCEPTION 'embedding transition barrier must begin awaiting predecessor terminal'
                USING ERRCODE = '23514';
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'embedding transition barrier is immutable history'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state <> 'awaiting_predecessor_terminal' THEN
        RAISE EXCEPTION 'terminal embedding transition barrier is immutable'
            USING ERRCODE = '23514';
    END IF;
    IF NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
       OR NEW.id IS DISTINCT FROM OLD.id
       OR NEW.transition_id IS DISTINCT FROM OLD.transition_id
       OR NEW.target_transition_plan_id IS DISTINCT FROM OLD.target_transition_plan_id
       OR NEW.predecessor_embedding_job_id IS DISTINCT FROM OLD.predecessor_embedding_job_id
       OR NEW.predecessor_transition_batch_id IS DISTINCT FROM OLD.predecessor_transition_batch_id
       OR NEW.barrier_transition_batch_id IS DISTINCT FROM OLD.barrier_transition_batch_id
       OR NEW.supersedes_barrier_id IS DISTINCT FROM OLD.supersedes_barrier_id
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.state NOT IN (
           'resolved_to_carry', 'resolved_satisfied_existing', 'resolved_definite',
           'no_longer_required', 'superseded'
       ) THEN
        RAISE EXCEPTION 'embedding transition barrier lifecycle transition is invalid'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_acknowledge_carried_transition_batch_after_unknown(
    target_workspace_id UUID, target_transition_id UUID, target_carry_id UUID,
    target_head_embedding_job_id UUID, target_expected_head_version BIGINT,
    target_successor_embedding_job_id UUID, target_space_registration_id UUID,
    target_kind TEXT, target_snapshot_id UUID, target_external_effect_id UUID,
    target_model_request_evidence_id UUID
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    carry embedding_transition_ambiguity_carries%ROWTYPE;
    target_plan embedding_transition_plans%ROWTYPE;
BEGIN
    IF target_workspace_id IS NULL OR target_transition_id IS NULL OR target_carry_id IS NULL
       OR target_head_embedding_job_id IS NULL OR target_expected_head_version IS NULL
       OR target_successor_embedding_job_id IS NULL OR target_space_registration_id IS NULL
       OR target_kind IS NULL OR target_snapshot_id IS NULL OR target_external_effect_id IS NULL
       OR target_model_request_evidence_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding transition carry acknowledgement arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM 1 FROM embedding_transitions WHERE workspace_id=target_workspace_id AND id=target_transition_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding transition carry acknowledgement requires its transition' USING ERRCODE='23514'; END IF;
    SELECT * INTO carry FROM embedding_transition_ambiguity_carries
     WHERE workspace_id=target_workspace_id AND id=target_carry_id FOR UPDATE;
    IF NOT FOUND OR carry.transition_id<>target_transition_id OR carry.head_embedding_job_id<>target_head_embedding_job_id
       OR carry.state<>'awaiting_acknowledgement' THEN
        RAISE EXCEPTION 'embedding transition carry acknowledgement requires its current exact mapping' USING ERRCODE='23514';
    END IF;
    IF EXISTS (
        SELECT 1 FROM embedding_transition_barriers
         WHERE workspace_id=target_workspace_id
           AND barrier_transition_batch_id=carry.successor_transition_batch_id
           AND state='awaiting_predecessor_terminal'
    ) THEN
        RAISE EXCEPTION 'embedding transition barrier batch is not dispatchable' USING ERRCODE='23514';
    END IF;
    SELECT * INTO target_plan FROM embedding_transition_plans
     WHERE workspace_id=target_workspace_id AND id=carry.target_transition_plan_id FOR SHARE;
    IF NOT FOUND OR target_snapshot_id NOT IN (
        SELECT snapshot_id FROM model_binding_snapshot_scopes
         WHERE workspace_id=target_workspace_id AND transition_plan_id=carry.target_transition_plan_id AND scope='transition'
    ) THEN
        RAISE EXCEPTION 'embedding transition carry acknowledgement mapping is stale' USING ERRCODE='23514';
    END IF;
    UPDATE embedding_transition_ambiguity_carries SET state='successor_created',successor_embedding_job_id=target_successor_embedding_job_id
     WHERE workspace_id=target_workspace_id AND id=target_carry_id;
    RETURN vestrace_accept_embedding_job(
        target_successor_embedding_job_id,target_workspace_id,target_space_registration_id,target_kind,target_snapshot_id,
        target_external_effect_id,target_model_request_evidence_id,target_head_embedding_job_id,target_expected_head_version
    );
END
$$;

DO $$
BEGIN
    IF COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        GRANT EXECUTE ON FUNCTION vestrace_plan_embedding_transition_version(
            UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
            UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
            UUID, UUID[], JSONB
        ) TO vestrace;
        GRANT EXECUTE ON FUNCTION vestrace_acknowledge_carried_transition_batch_after_unknown(
            UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID
        ) TO vestrace;
    END IF;
END
$$;

CREATE TRIGGER embedding_transition_barriers_lifecycle
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_barriers
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_transition_barrier_header();
CREATE TRIGGER embedding_transition_barrier_recipes_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_barrier_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

CREATE OR REPLACE FUNCTION vestrace_observe_embedding_transition_barriers(
    target_workspace_id UUID,
    target_transition_id UUID,
    target_transition_plan_id UUID
) RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    predecessor RECORD;
    candidate RECORD;
    old_barrier embedding_transition_barriers%ROWTYPE;
    barrier_id UUID;
    still_required BOOLEAN;
    has_old_barrier BOOLEAN;
BEGIN
    -- A classifier that began for an older target must not recreate a barrier
    -- after current-version planning has already superseded that target.
    IF EXISTS (
        SELECT 1
          FROM embedding_transition_plans AS target_plan
          JOIN embedding_transition_plans AS newer_plan
            ON newer_plan.workspace_id = target_plan.workspace_id
           AND newer_plan.transition_id = target_plan.transition_id
           AND newer_plan.version > target_plan.version
         WHERE target_plan.workspace_id = target_workspace_id
           AND target_plan.id = target_transition_plan_id
    ) THEN
        RETURN;
    END IF;
    -- The caller has already taken the transition and space guards.  Lock the
    -- predecessor job before its barrier lineage, then the old header before
    -- allocating the fresh dedicated batch.
    FOR predecessor IN
        SELECT id, transition_batch_id
          FROM embedding_transition_plans
         WHERE workspace_id = target_workspace_id
           AND transition_id = target_transition_id
           AND id <> target_transition_plan_id
    LOOP
        FOR candidate IN
            SELECT job.id
              FROM embedding_jobs AS job
              JOIN model_binding_snapshot_scopes AS scope
                ON scope.workspace_id = job.workspace_id
               AND scope.snapshot_id = job.model_binding_snapshot_id
              JOIN embedding_transition_plans AS plan
                ON plan.workspace_id = scope.workspace_id
               AND plan.id = scope.transition_plan_id
             WHERE job.workspace_id = target_workspace_id
               AND plan.transition_batch_id = predecessor.transition_batch_id
               AND (
                    job.state IN ('requested', 'running')
                    OR (
                        job.state IN ('succeeded', 'failed_definite', 'inconclusive_unknown', 'cancelled')
                        AND NOT EXISTS (
                            SELECT 1
                              FROM external_effect_lifecycle_transitions AS effect
                             WHERE effect.workspace_id = job.workspace_id
                               AND effect.effect_id = job.external_effect_id
                               AND effect.status IN ('acknowledged', 'failed', 'unknown')
                        )
                    )
               )
             FOR UPDATE OF job
        LOOP
            SELECT * INTO old_barrier
              FROM embedding_transition_barriers
             WHERE workspace_id = target_workspace_id
               AND predecessor_embedding_job_id = candidate.id
               AND predecessor_transition_batch_id = predecessor.transition_batch_id
               AND state = 'awaiting_predecessor_terminal'
             FOR UPDATE;
            has_old_barrier := FOUND;
            -- A concurrent current-version planner may have committed while
            -- this classifier was waiting for the header. Recheck after the
            -- lock, not only at function entry, before any new batch exists.
            IF EXISTS (
                SELECT 1
                  FROM embedding_transition_plans AS target_plan
                  JOIN embedding_transition_plans AS newer_plan
                    ON newer_plan.workspace_id = target_plan.workspace_id
                   AND newer_plan.transition_id = target_plan.transition_id
                   AND newer_plan.version > target_plan.version
                 WHERE target_plan.workspace_id = target_workspace_id
                   AND target_plan.id = target_transition_plan_id
            ) THEN
                RETURN;
            END IF;
            SELECT EXISTS (
                SELECT 1
                  FROM embedding_transition_plan_recipes AS old_recipe
                  JOIN embedding_transition_plan_recipes AS new_recipe
                    ON new_recipe.workspace_id = target_workspace_id
                   AND new_recipe.transition_plan_id = target_transition_plan_id
                   AND new_recipe.recipe_identity = old_recipe.recipe_identity
                   AND new_recipe.input_ordinals = old_recipe.input_ordinals
                 WHERE old_recipe.workspace_id = target_workspace_id
                   AND old_recipe.transition_plan_id = predecessor.id
            ) INTO still_required;

            -- Replanning the same target is idempotent.  A newer target may
            -- replace this header only when it still contains mapped work;
            -- otherwise the open observation becomes durable abandonment
            -- history and no successor batch is allocated.
            IF has_old_barrier AND old_barrier.target_transition_plan_id = target_transition_plan_id THEN
                CONTINUE;
            ELSIF has_old_barrier AND NOT still_required THEN
                UPDATE embedding_transition_barriers
                   SET state = 'no_longer_required',
                       resolution_evidence = 'target_version_no_longer_requires_mapped_recipes'
                 WHERE workspace_id = old_barrier.workspace_id AND id = old_barrier.id;
                CONTINUE;
            ELSIF has_old_barrier THEN
                UPDATE embedding_transition_barriers
                   SET state = 'superseded', resolution_evidence = 'target_version_superseded'
                 WHERE workspace_id = old_barrier.workspace_id AND id = old_barrier.id;
            ELSIF NOT still_required THEN
                CONTINUE;
            END IF;

            barrier_id := gen_random_uuid();
            INSERT INTO embedding_transition_barriers(
                id, workspace_id, transition_id, target_transition_plan_id,
                predecessor_embedding_job_id, predecessor_transition_batch_id,
                barrier_transition_batch_id, state, supersedes_barrier_id
            ) VALUES (
                barrier_id, target_workspace_id, target_transition_id, target_transition_plan_id,
                candidate.id, predecessor.transition_batch_id, gen_random_uuid(),
                'awaiting_predecessor_terminal', old_barrier.id
            );
            INSERT INTO embedding_transition_barrier_recipes(
                workspace_id, barrier_id, old_recipe_ordinal, old_input_ordinal,
                new_recipe_ordinal, new_input_ordinal
            )
            SELECT target_workspace_id, barrier_id,
                   old_recipe.recipe_ordinal, old_input.ordinal,
                   new_recipe.recipe_ordinal, new_input.ordinal
              FROM embedding_transition_plan_recipes AS old_recipe
              JOIN LATERAL unnest(old_recipe.input_ordinals) WITH ORDINALITY
                   AS old_input(value, ordinal) ON TRUE
              JOIN embedding_transition_plan_recipes AS new_recipe
                ON new_recipe.workspace_id = target_workspace_id
               AND new_recipe.transition_plan_id = target_transition_plan_id
               AND new_recipe.recipe_identity = old_recipe.recipe_identity
               AND new_recipe.input_ordinals = old_recipe.input_ordinals
              JOIN LATERAL unnest(new_recipe.input_ordinals) WITH ORDINALITY
                   AS new_input(value, ordinal) ON new_input.value = old_input.value
             WHERE old_recipe.workspace_id = target_workspace_id
               AND old_recipe.transition_plan_id = predecessor.id;
        END LOOP;
    END LOOP;
END
$$;

-- A planner cannot express an empty successor structure: 0189 deliberately
-- requires every successor to preserve its predecessor's recipe structure.
-- Candidate abandonment is therefore its own guarded terminal decision rather
-- than a direct header edit in a caller or test fixture.
CREATE OR REPLACE FUNCTION vestrace_abandon_embedding_transition_barrier_candidate(
    target_workspace_id UUID,
    target_transition_id UUID,
    target_barrier_id UUID
) RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    barrier embedding_transition_barriers%ROWTYPE;
BEGIN
    IF target_workspace_id IS NULL OR target_transition_id IS NULL OR target_barrier_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding transition barrier abandonment arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    SELECT * INTO barrier
      FROM embedding_transition_barriers
     WHERE workspace_id = target_workspace_id
       AND transition_id = target_transition_id
       AND id = target_barrier_id
       AND state = 'awaiting_predecessor_terminal'
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition barrier abandonment requires its current open mapping'
            USING ERRCODE = '23514';
    END IF;
    UPDATE embedding_transition_barriers
       SET state = 'no_longer_required', resolution_evidence = 'candidate_abandoned'
     WHERE workspace_id = barrier.workspace_id AND id = barrier.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_classify_embedding_transition_ambiguity_carries(
    target_workspace_id UUID,
    target_transition_id UUID,
    target_transition_plan_id UUID
) RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    lineage RECORD;
    head RECORD;
    old_carry embedding_transition_ambiguity_carries%ROWTYPE;
    candidate_count BIGINT;
    carry_id UUID;
    carry_successor_batch_id UUID;
    barrier RECORD;
BEGIN
    IF EXISTS (
        SELECT 1
          FROM embedding_transition_plans AS plan
          JOIN embedding_transition_barriers AS barrier_row
            ON barrier_row.workspace_id = plan.workspace_id
           AND barrier_row.barrier_transition_batch_id = plan.transition_batch_id
           AND barrier_row.state = 'awaiting_predecessor_terminal'
         WHERE plan.workspace_id = target_workspace_id
           AND plan.id = $3
    ) THEN
        RAISE EXCEPTION 'embedding transition barrier batch is not dispatchable'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_observe_embedding_transition_barriers(
        target_workspace_id, target_transition_id, target_transition_plan_id
    );

    -- Task 7's ambiguity classification stays intact; barriers are additional
    -- predecessor history, not a second successor authority.
    FOR lineage IN
        SELECT DISTINCT source_plan.id AS source_plan_id,
                        source_plan.transition_batch_id AS predecessor_batch_id
          FROM embedding_transition_plans AS source_plan
         WHERE source_plan.workspace_id=target_workspace_id
           AND source_plan.transition_id=target_transition_id
           AND source_plan.id<>target_transition_plan_id
    LOOP
        SELECT count(*) INTO candidate_count
          FROM embedding_jobs AS job
          JOIN model_binding_snapshot_scopes AS scope
            ON scope.workspace_id=job.workspace_id AND scope.snapshot_id=job.model_binding_snapshot_id
          JOIN embedding_transition_plans AS plan
            ON plan.workspace_id=scope.workspace_id AND plan.id=scope.transition_plan_id
         WHERE job.workspace_id=target_workspace_id
           AND plan.transition_batch_id=lineage.predecessor_batch_id
           AND job.state='inconclusive_unknown'
           AND NOT EXISTS (SELECT 1 FROM embedding_jobs AS child WHERE child.workspace_id=job.workspace_id AND child.retries_unknown_embedding_job_id=job.id);
        IF candidate_count>1 THEN
            RAISE EXCEPTION 'embedding transition lineage has more than one eligible ambiguity head' USING ERRCODE='23514';
        END IF;
        IF candidate_count=0 THEN CONTINUE; END IF;
        SELECT job.id,job.kind,job.version INTO head
          FROM embedding_jobs AS job
          JOIN model_binding_snapshot_scopes AS scope
            ON scope.workspace_id=job.workspace_id AND scope.snapshot_id=job.model_binding_snapshot_id
          JOIN embedding_transition_plans AS plan
            ON plan.workspace_id=scope.workspace_id AND plan.id=scope.transition_plan_id
         WHERE job.workspace_id=target_workspace_id
           AND plan.transition_batch_id=lineage.predecessor_batch_id
           AND job.state='inconclusive_unknown'
           AND NOT EXISTS (SELECT 1 FROM embedding_jobs AS child WHERE child.workspace_id=job.workspace_id AND child.retries_unknown_embedding_job_id=job.id)
         FOR UPDATE OF job;
        SELECT * INTO old_carry FROM embedding_transition_ambiguity_carries
         WHERE workspace_id=target_workspace_id AND head_embedding_job_id=head.id
           AND state='awaiting_acknowledgement' FOR UPDATE;
        IF FOUND AND old_carry.target_transition_plan_id=target_transition_plan_id THEN
            -- Reclassification of this exact target is idempotent.  It may
            -- resolve an open barrier below, but it must not rewrite the carry
            -- header or allocate another named successor batch.
            CONTINUE;
        ELSIF FOUND THEN
            UPDATE embedding_transition_ambiguity_carries SET state='no_longer_required',reason='superseded target mapping'
             WHERE workspace_id=old_carry.workspace_id AND id=old_carry.id;
        END IF;
        IF vestrace_transition_recipe_structure(target_workspace_id,lineage.source_plan_id)
           IS DISTINCT FROM vestrace_transition_recipe_structure(target_workspace_id,target_transition_plan_id) THEN
            RAISE EXCEPTION 'embedding transition carry recipes must preserve identities and input ordinals in order' USING ERRCODE='23514';
        END IF;
        -- An open barrier is the authority that owns this dedicated successor
        -- batch.  Reuse it for the matching ambiguity carry so acknowledgement
        -- names the same batch the barrier later refuses.
        SELECT barrier_row.barrier_transition_batch_id INTO carry_successor_batch_id
          FROM embedding_transition_barriers AS barrier_row
         WHERE barrier_row.workspace_id=target_workspace_id
           AND barrier_row.transition_id=target_transition_id
           AND barrier_row.target_transition_plan_id=$3
           AND barrier_row.predecessor_embedding_job_id=head.id
           AND barrier_row.predecessor_transition_batch_id=lineage.predecessor_batch_id
           AND barrier_row.state='awaiting_predecessor_terminal'
         FOR SHARE;
        IF NOT FOUND THEN
            carry_successor_batch_id:=gen_random_uuid();
        END IF;
        carry_id:=gen_random_uuid();
        INSERT INTO embedding_transition_ambiguity_carries(
            id,workspace_id,transition_id,head_embedding_job_id,source_transition_plan_id,target_transition_plan_id,
            predecessor_transition_batch_id,successor_transition_batch_id,state,supersedes_carry_id
        ) VALUES(
            carry_id,target_workspace_id,target_transition_id,head.id,lineage.source_plan_id,target_transition_plan_id,
            lineage.predecessor_batch_id,carry_successor_batch_id,'awaiting_acknowledgement',old_carry.id
        );
        INSERT INTO embedding_transition_ambiguity_carry_recipes(
            workspace_id,carry_id,old_recipe_ordinal,old_input_ordinal,new_recipe_ordinal,new_input_ordinal,state
        )
        SELECT target_workspace_id,carry_id,old_recipe.recipe_ordinal,old_input.ordinal,
               new_recipe.recipe_ordinal,new_input.ordinal,'awaiting_acknowledgement'
          FROM embedding_transition_plan_recipes AS old_recipe
          JOIN LATERAL unnest(old_recipe.input_ordinals) WITH ORDINALITY AS old_input(value,ordinal) ON TRUE
          JOIN embedding_transition_plan_recipes AS new_recipe
            ON new_recipe.workspace_id=target_workspace_id AND new_recipe.transition_plan_id=target_transition_plan_id
           AND new_recipe.recipe_identity=old_recipe.recipe_identity AND new_recipe.input_ordinals=old_recipe.input_ordinals
          JOIN LATERAL unnest(new_recipe.input_ordinals) WITH ORDINALITY AS new_input(value,ordinal) ON new_input.value=old_input.value
         WHERE old_recipe.workspace_id=target_workspace_id AND old_recipe.transition_plan_id=lineage.source_plan_id;
    END LOOP;

    FOR barrier IN
        SELECT barrier_row.id, job.state
          FROM embedding_transition_barriers AS barrier_row
          JOIN embedding_jobs AS job ON job.workspace_id=barrier_row.workspace_id AND job.id=barrier_row.predecessor_embedding_job_id
         WHERE barrier_row.workspace_id=target_workspace_id AND barrier_row.transition_id=target_transition_id
           AND barrier_row.state='awaiting_predecessor_terminal'
           AND job.state IN ('succeeded','failed_definite','inconclusive_unknown','cancelled')
           AND EXISTS (SELECT 1 FROM external_effect_lifecycle_transitions AS effect WHERE effect.workspace_id=job.workspace_id AND effect.effect_id=job.external_effect_id AND effect.status IN ('acknowledged','failed','unknown'))
         FOR UPDATE OF barrier_row,job
    LOOP
        UPDATE embedding_transition_barriers SET
            state=CASE barrier.state WHEN 'inconclusive_unknown' THEN 'resolved_to_carry' WHEN 'succeeded' THEN 'resolved_satisfied_existing' ELSE 'resolved_definite' END,
            resolution_evidence=CASE barrier.state WHEN 'inconclusive_unknown' THEN 'terminal_inconclusive_unknown_effect_outcome' WHEN 'succeeded' THEN 'terminal_succeeded_effect_outcome' ELSE 'terminal_definite_effect_outcome' END
         WHERE workspace_id=target_workspace_id AND id=barrier.id;
    END LOOP;
END
$$;

-- The preparation bridge lends these functions to the runtime role so it can
-- replace both functions and their trigger. Return them only after every
-- replacement and trigger declaration is complete.
DO $$
DECLARE target_function REGPROCEDURE;
BEGIN
    FOREACH target_function IN ARRAY ARRAY[
        'vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'::REGPROCEDURE,
        'vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID)'::REGPROCEDURE,
        'vestrace_abandon_embedding_transition_barrier_candidate(UUID, UUID, UUID)'::REGPROCEDURE,
        'vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)'::REGPROCEDURE,
        'vestrace_validate_embedding_transition_barrier_header()'::REGPROCEDURE
    ]::REGPROCEDURE[] LOOP
        BEGIN
            PERFORM vestrace_assign_p03_function_owner(target_function);
        EXCEPTION WHEN insufficient_privilege THEN
            IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
                RAISE;
            END IF;
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function);
        END;
    END LOOP;
END
$$;

DO $$
BEGIN
    BEGIN
        PERFORM vestrace_assign_p03_function_owner(
            'vestrace_observe_embedding_transition_barriers(UUID, UUID, UUID)'::REGPROCEDURE
        );
    EXCEPTION WHEN insufficient_privilege THEN
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
            RAISE;
        END IF;
        ALTER FUNCTION vestrace_observe_embedding_transition_barriers(UUID, UUID, UUID)
            OWNER TO vestrace_guarded_owner;
    END;
END
$$;

-- The ownership helper deliberately revokes runtime ACLs before deciding which
-- functions are callable.  Reassert the two public guarded entry points after
-- their final hand-back; this also gives SQLx's superuser migration path the
-- same ACL as the provisioner path.
REVOKE ALL ON FUNCTION vestrace_abandon_embedding_transition_barrier_candidate(UUID, UUID, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_abandon_embedding_transition_barrier_candidate(UUID, UUID, UUID) TO vestrace;
REVOKE ALL ON FUNCTION vestrace_plan_embedding_transition_version(
    UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
    UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
    UUID, UUID[], JSONB
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_plan_embedding_transition_version(
    UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
    UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
    UUID, UUID[], JSONB
) TO vestrace;
REVOKE ALL ON FUNCTION vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID) TO vestrace;
DO $$
BEGIN
    -- In a real runtime migration the guarded-owner bridge has already
    -- removed this ACL. SQLx applies migrations as its superuser test role,
    -- where the bridge fallback cannot do so after ownership is handed back.
    IF COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        REVOKE ALL ON FUNCTION vestrace_validate_embedding_transition_barrier_header() FROM PUBLIC;
        REVOKE EXECUTE ON FUNCTION vestrace_validate_embedding_transition_barrier_header() FROM vestrace;
    END IF;
END
$$;

ALTER TABLE embedding_transition_barriers ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_barriers FORCE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_barrier_recipes ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_barrier_recipes FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_transition_barriers_workspace_policy
    ON embedding_transition_barriers
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY embedding_transition_barrier_recipes_workspace_policy
    ON embedding_transition_barrier_recipes
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_transition_barriers FROM PUBLIC;
REVOKE ALL ON embedding_transition_barrier_recipes FROM PUBLIC;

DO $$
DECLARE target_table TEXT;
BEGIN
    FOREACH target_table IN ARRAY ARRAY[
        'embedding_transition_barriers',
        'embedding_transition_barrier_recipes'
    ]::TEXT[] LOOP
        BEGIN
            PERFORM vestrace_assign_p03_table_owner(format('public.%I', target_table)::REGCLASS);
        EXCEPTION WHEN insufficient_privilege THEN
            IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
                RAISE;
            END IF;
            EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace_guarded_owner', target_table);
            EXECUTE format('REVOKE ALL ON TABLE public.%I FROM PUBLIC', target_table);
            EXECUTE format('REVOKE ALL ON TABLE public.%I FROM vestrace', target_table);
            EXECUTE format('GRANT SELECT, REFERENCES ON TABLE public.%I TO vestrace', target_table);
        END;
    END LOOP;
END
$$;
