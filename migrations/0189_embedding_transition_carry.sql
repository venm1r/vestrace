-- P04 Task 7: recipe-granular ambiguity carry.
--
-- This migration is deliberately forward-only.  It preserves Task 6's public
-- function signatures, strengthens its structural comparison, and records the
-- one lawful cross-version edge without creating a second dispatch authority.

DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_p04_transition_carry_upgrade()') IS NOT NULL
       AND has_function_privilege(
           current_user,
           'public.vestrace_prepare_p04_transition_carry_upgrade()',
           'EXECUTE'
       ) THEN
        PERFORM public.vestrace_prepare_p04_transition_carry_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P04 embedding-transition ownership hand-back must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        ALTER FUNCTION public.vestrace_plan_embedding_transition_version(
            UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
            UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
            UUID, UUID[], JSONB
        ) OWNER TO vestrace;
        ALTER FUNCTION public.vestrace_accept_embedding_job(
            UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
        ) OWNER TO vestrace;
    END IF;
END
$$;

CREATE TABLE embedding_transition_ambiguity_carries (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_id UUID NOT NULL,
    head_embedding_job_id UUID NOT NULL,
    source_transition_plan_id UUID NOT NULL,
    target_transition_plan_id UUID NOT NULL,
    predecessor_transition_batch_id UUID NOT NULL,
    successor_transition_batch_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'awaiting_acknowledgement', 'successor_created', 'no_longer_required'
    )),
    reason TEXT,
    successor_embedding_job_id UUID,
    supersedes_carry_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_transition_ambiguity_carries_transition_fkey
        FOREIGN KEY (workspace_id, transition_id)
        REFERENCES embedding_transitions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_ambiguity_carries_head_fkey
        FOREIGN KEY (workspace_id, head_embedding_job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_ambiguity_carries_source_plan_fkey
        FOREIGN KEY (workspace_id, source_transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_ambiguity_carries_target_plan_fkey
        FOREIGN KEY (workspace_id, target_transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_ambiguity_carries_successor_job_fkey
        FOREIGN KEY (workspace_id, successor_embedding_job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT embedding_transition_ambiguity_carries_supersedes_fkey
        FOREIGN KEY (workspace_id, supersedes_carry_id)
        REFERENCES embedding_transition_ambiguity_carries(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_ambiguity_carries_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT embedding_transition_ambiguity_carries_terminal_reason
        CHECK ((state = 'awaiting_acknowledgement' AND reason IS NULL AND successor_embedding_job_id IS NULL)
            OR (state = 'successor_created' AND reason IS NULL AND successor_embedding_job_id IS NOT NULL)
            OR (state = 'no_longer_required' AND reason IS NOT NULL AND successor_embedding_job_id IS NULL))
);

CREATE UNIQUE INDEX embedding_transition_ambiguity_carries_one_open_head
    ON embedding_transition_ambiguity_carries (workspace_id, head_embedding_job_id)
    WHERE state = 'awaiting_acknowledgement';

CREATE TABLE embedding_transition_ambiguity_carry_recipes (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    carry_id UUID NOT NULL,
    old_recipe_ordinal BIGINT NOT NULL CHECK (old_recipe_ordinal >= 0),
    old_input_ordinal BIGINT NOT NULL CHECK (old_input_ordinal >= 0),
    new_recipe_ordinal BIGINT,
    new_input_ordinal BIGINT,
    state TEXT NOT NULL CHECK (state IN ('awaiting_acknowledgement', 'no_longer_required')),
    PRIMARY KEY (workspace_id, carry_id, old_recipe_ordinal, old_input_ordinal),
    CONSTRAINT embedding_transition_ambiguity_carry_recipes_header_fkey
        FOREIGN KEY (workspace_id, carry_id)
        REFERENCES embedding_transition_ambiguity_carries(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_ambiguity_carry_recipes_mapping_xor CHECK (
        (state = 'awaiting_acknowledgement' AND new_recipe_ordinal IS NOT NULL AND new_input_ordinal IS NOT NULL)
        OR (state = 'no_longer_required' AND new_recipe_ordinal IS NULL AND new_input_ordinal IS NULL)
    ),
    CONSTRAINT embedding_transition_ambiguity_carry_recipes_new_unique
        UNIQUE NULLS NOT DISTINCT (workspace_id, carry_id, new_recipe_ordinal, new_input_ordinal)
);

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_transition_ambiguity_carry_header()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.state <> 'awaiting_acknowledgement' THEN
            RAISE EXCEPTION 'embedding transition carry must begin awaiting acknowledgement'
                USING ERRCODE = '23514';
        END IF;
        RETURN NEW;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'embedding transition carry is immutable history'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state <> 'awaiting_acknowledgement' THEN
        RAISE EXCEPTION 'terminal embedding transition carry is immutable'
            USING ERRCODE = '23514';
    END IF;
    IF NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
       OR NEW.id IS DISTINCT FROM OLD.id
       OR NEW.transition_id IS DISTINCT FROM OLD.transition_id
       OR NEW.head_embedding_job_id IS DISTINCT FROM OLD.head_embedding_job_id
       OR NEW.source_transition_plan_id IS DISTINCT FROM OLD.source_transition_plan_id
       OR NEW.target_transition_plan_id IS DISTINCT FROM OLD.target_transition_plan_id
       OR NEW.predecessor_transition_batch_id IS DISTINCT FROM OLD.predecessor_transition_batch_id
       OR NEW.successor_transition_batch_id IS DISTINCT FROM OLD.successor_transition_batch_id
       OR NEW.supersedes_carry_id IS DISTINCT FROM OLD.supersedes_carry_id
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.state NOT IN ('successor_created', 'no_longer_required') THEN
        RAISE EXCEPTION 'embedding transition carry lifecycle transition is invalid'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER embedding_transition_ambiguity_carries_lifecycle
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_ambiguity_carries
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_transition_ambiguity_carry_header();
CREATE TRIGGER embedding_transition_ambiguity_carry_recipes_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_ambiguity_carry_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE embedding_transition_ambiguity_carries ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_ambiguity_carries FORCE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_ambiguity_carry_recipes ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_ambiguity_carry_recipes FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_transition_ambiguity_carries_workspace_policy
    ON embedding_transition_ambiguity_carries
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY embedding_transition_ambiguity_carry_recipes_workspace_policy
    ON embedding_transition_ambiguity_carry_recipes
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_transition_ambiguity_carries FROM PUBLIC;
REVOKE ALL ON embedding_transition_ambiguity_carry_recipes FROM PUBLIC;

CREATE OR REPLACE FUNCTION vestrace_transition_recipe_structure(
    target_workspace_id UUID,
    target_plan_id UUID
) RETURNS JSONB
LANGUAGE sql
STABLE
SET search_path = public, pg_temp
AS $$
    SELECT jsonb_agg(
        jsonb_build_array(recipe_identity::TEXT, input_ordinals)
        ORDER BY recipe_ordinal
    )
      FROM embedding_transition_plan_recipes
     WHERE workspace_id = target_workspace_id AND transition_plan_id = target_plan_id
$$;

CREATE OR REPLACE FUNCTION vestrace_target_transition_recipe_structure(
    target_recipe_identities UUID[],
    target_recipe_input_ordinals JSONB
) RETURNS JSONB
LANGUAGE sql
IMMUTABLE
SET search_path = public, pg_temp
AS $$
    SELECT jsonb_agg(
        jsonb_build_array(target_recipe_identities[ordinality]::TEXT, value)
        ORDER BY ordinality
    )
      FROM jsonb_array_elements(target_recipe_input_ordinals)
           WITH ORDINALITY AS input_rows(value, ordinality)
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
BEGIN
    -- The caller holds the transition row lock.  This function has no separate
    -- lock because that would make the acceptance race proof vacuous.
    FOR lineage IN
        SELECT DISTINCT source_plan.id AS source_plan_id,
                        source_plan.transition_batch_id AS predecessor_batch_id
          FROM embedding_transition_plans AS source_plan
         WHERE source_plan.workspace_id = target_workspace_id
           AND source_plan.transition_id = target_transition_id
           AND source_plan.id <> target_transition_plan_id
    LOOP
        SELECT count(*) INTO candidate_count
          FROM embedding_jobs AS job
          JOIN model_binding_snapshot_scopes AS scope
            ON scope.workspace_id=job.workspace_id
           AND scope.snapshot_id=job.model_binding_snapshot_id
          JOIN embedding_transition_plans AS plan
            ON plan.workspace_id=scope.workspace_id AND plan.id=scope.transition_plan_id
         WHERE job.workspace_id=target_workspace_id
           AND plan.transition_batch_id=lineage.predecessor_batch_id
           AND job.state='inconclusive_unknown'
            AND NOT EXISTS (
                SELECT 1 FROM embedding_jobs AS child
                 WHERE child.workspace_id=job.workspace_id
                   AND child.retries_unknown_embedding_job_id=job.id
            );
        IF candidate_count > 1 THEN
            RAISE EXCEPTION 'embedding transition lineage has more than one eligible ambiguity head'
                USING ERRCODE = '23514';
        END IF;
        IF candidate_count = 0 THEN
            CONTINUE;
        END IF;

        SELECT job.id, job.kind, job.version
          INTO head
          FROM embedding_jobs AS job
          JOIN model_binding_snapshot_scopes AS scope
            ON scope.workspace_id=job.workspace_id
           AND scope.snapshot_id=job.model_binding_snapshot_id
          JOIN embedding_transition_plans AS plan
            ON plan.workspace_id=scope.workspace_id AND plan.id=scope.transition_plan_id
         WHERE job.workspace_id=target_workspace_id
           AND plan.transition_batch_id=lineage.predecessor_batch_id
           AND job.state='inconclusive_unknown'
           AND NOT EXISTS (SELECT 1 FROM embedding_jobs AS child WHERE child.workspace_id=job.workspace_id AND child.retries_unknown_embedding_job_id=job.id)
          FOR UPDATE OF job;

        SELECT * INTO old_carry
          FROM embedding_transition_ambiguity_carries
         WHERE workspace_id=target_workspace_id
           AND head_embedding_job_id=head.id
           AND state='awaiting_acknowledgement'
         FOR UPDATE;
        IF FOUND THEN
            UPDATE embedding_transition_ambiguity_carries
               SET state='no_longer_required', reason='superseded target mapping'
             WHERE workspace_id=old_carry.workspace_id AND id=old_carry.id;
        END IF;

        IF vestrace_transition_recipe_structure(target_workspace_id, lineage.source_plan_id)
           IS DISTINCT FROM vestrace_transition_recipe_structure(target_workspace_id, target_transition_plan_id) THEN
            RAISE EXCEPTION 'embedding transition carry recipes must preserve identities and input ordinals in order'
                USING ERRCODE = '23514';
        END IF;

        carry_id := gen_random_uuid();
        INSERT INTO embedding_transition_ambiguity_carries (
            id, workspace_id, transition_id, head_embedding_job_id,
            source_transition_plan_id, target_transition_plan_id,
            predecessor_transition_batch_id, successor_transition_batch_id,
            state, supersedes_carry_id
        ) VALUES (
            carry_id, target_workspace_id, target_transition_id, head.id,
            lineage.source_plan_id, target_transition_plan_id,
            lineage.predecessor_batch_id, gen_random_uuid(),
            'awaiting_acknowledgement', old_carry.id
        );
        INSERT INTO embedding_transition_ambiguity_carry_recipes (
            workspace_id, carry_id, old_recipe_ordinal, old_input_ordinal,
            new_recipe_ordinal, new_input_ordinal, state
        )
        SELECT target_workspace_id, carry_id,
               old_recipe.recipe_ordinal, old_input.ordinal,
               new_recipe.recipe_ordinal, new_input.ordinal,
               'awaiting_acknowledgement'
          FROM embedding_transition_plan_recipes AS old_recipe
          JOIN LATERAL unnest(old_recipe.input_ordinals) WITH ORDINALITY
               AS old_input(value, ordinal) ON TRUE
          JOIN embedding_transition_plan_recipes AS new_recipe
            ON new_recipe.workspace_id=target_workspace_id
           AND new_recipe.transition_plan_id=target_transition_plan_id
           AND new_recipe.recipe_identity=old_recipe.recipe_identity
           AND new_recipe.input_ordinals=old_recipe.input_ordinals
          JOIN LATERAL unnest(new_recipe.input_ordinals) WITH ORDINALITY
               AS new_input(value, ordinal) ON new_input.value=old_input.value
         WHERE old_recipe.workspace_id=target_workspace_id
           AND old_recipe.transition_plan_id=lineage.source_plan_id;
    END LOOP;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_plan_embedding_transition_version(
    target_transition_id UUID,
    target_plan_id UUID,
    target_workspace_id UUID,
    target_version BIGINT,
    target_source_connection_id UUID,
    target_source_connection_revision_id UUID,
    target_source_branch TEXT,
    target_source_credential_revision_id UUID,
    target_source_no_auth_binding_revision_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_connection_qualification_revision_id UUID,
    target_model_revision_id UUID,
    target_model_qualification_revision_id UUID,
    target_branch TEXT,
    target_credential_revision_id UUID,
    target_credential_slot_id UUID,
    target_credential_activation_guard_id UUID,
    target_expected_slot_version BIGINT,
    target_no_auth_binding_revision_id UUID,
    target_space_registration_id UUID,
    target_batch_id UUID,
    target_snapshot_id UUID,
    target_recipe_identities UUID[],
    target_recipe_input_ordinals JSONB
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    guard_id UUID;
    predecessor_id UUID;
    existing embedding_transition_plans%ROWTYPE;
    recipe_index INTEGER;
    recipe_inputs BIGINT[];
    target_structure JSONB;
BEGIN
    IF target_transition_id IS NULL OR target_plan_id IS NULL OR target_workspace_id IS NULL
       OR target_version IS NULL OR target_version < 1
       OR target_source_connection_id IS NULL OR target_source_connection_revision_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_connection_qualification_revision_id IS NULL OR target_model_revision_id IS NULL
       OR target_model_qualification_revision_id IS NULL OR target_space_registration_id IS NULL
       OR target_batch_id IS NULL OR target_snapshot_id IS NULL
       OR target_recipe_identities IS NULL OR target_recipe_input_ordinals IS NULL
       OR jsonb_typeof(target_recipe_input_ordinals) <> 'array'
       OR jsonb_array_length(target_recipe_input_ordinals) <> cardinality(target_recipe_identities)
       OR target_source_branch NOT IN ('credential', 'no_auth')
       OR target_branch NOT IN ('credential', 'no_auth')
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding transition planning arguments are malformed' USING ERRCODE='22023';
    END IF;
    IF NOT ((target_source_branch='credential' AND target_source_credential_revision_id IS NOT NULL AND target_source_no_auth_binding_revision_id IS NULL)
         OR (target_source_branch='no_auth' AND target_source_credential_revision_id IS NULL AND target_source_no_auth_binding_revision_id IS NOT NULL))
       OR NOT ((target_branch='credential' AND target_credential_revision_id IS NOT NULL AND target_credential_slot_id IS NOT NULL AND target_credential_activation_guard_id IS NOT NULL AND target_expected_slot_version IS NOT NULL AND target_no_auth_binding_revision_id IS NULL)
            OR (target_branch='no_auth' AND target_credential_revision_id IS NULL AND target_credential_slot_id IS NULL AND target_credential_activation_guard_id IS NULL AND target_expected_slot_version IS NULL AND target_no_auth_binding_revision_id IS NOT NULL)) THEN
        RAISE EXCEPTION 'embedding transition plan auth-binding branch is malformed' USING ERRCODE='22023';
    END IF;
    SELECT id INTO guard_id FROM connection_execution_guards
     WHERE workspace_id=target_workspace_id AND connection_id=target_connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding transition requires its existing permanent connection guard' USING ERRCODE='23514'; END IF;
    IF target_branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards
         WHERE id=target_credential_activation_guard_id AND workspace_id=target_workspace_id
           AND connection_id=target_connection_id AND credential_slot_id=target_credential_slot_id
           AND execution_guard_id=guard_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding transition requires its exact credential activation guard' USING ERRCODE='23514'; END IF;
    END IF;
    PERFORM 1 FROM embedding_space_registrations
     WHERE workspace_id=target_workspace_id AND id=target_space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding transition requires its authorized embedding space' USING ERRCODE='23514'; END IF;

    INSERT INTO embedding_transitions(id,workspace_id,target_space_registration_id)
    VALUES(target_transition_id,target_workspace_id,target_space_registration_id) ON CONFLICT(id) DO NOTHING;
    SELECT id INTO predecessor_id FROM embedding_transitions
     WHERE workspace_id=target_workspace_id AND id=target_transition_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding transition identity belongs to another workspace' USING ERRCODE='23514'; END IF;

    target_structure := vestrace_target_transition_recipe_structure(target_recipe_identities,target_recipe_input_ordinals);
    SELECT * INTO existing FROM embedding_transition_plans
     WHERE workspace_id=target_workspace_id AND transition_id=target_transition_id AND version=target_version FOR UPDATE;
    IF FOUND THEN
        IF existing.id=target_plan_id
           AND existing.source_connection_id=target_source_connection_id
           AND existing.source_connection_revision_id=target_source_connection_revision_id
           AND existing.source_branch=target_source_branch
           AND existing.source_credential_revision_id IS NOT DISTINCT FROM target_source_credential_revision_id
           AND existing.source_no_auth_binding_revision_id IS NOT DISTINCT FROM target_source_no_auth_binding_revision_id
           AND existing.target_connection_id=target_connection_id
           AND existing.target_connection_revision_id=target_connection_revision_id
           AND existing.target_connection_qualification_revision_id=target_connection_qualification_revision_id
           AND existing.target_model_revision_id=target_model_revision_id
           AND existing.target_model_qualification_revision_id=target_model_qualification_revision_id
           AND existing.target_branch=target_branch
           AND existing.target_credential_revision_id IS NOT DISTINCT FROM target_credential_revision_id
           AND existing.target_no_auth_binding_revision_id IS NOT DISTINCT FROM target_no_auth_binding_revision_id
           AND existing.target_space_registration_id=target_space_registration_id
           AND existing.transition_batch_id=target_batch_id
           AND vestrace_transition_recipe_structure(target_workspace_id,existing.id)=target_structure THEN
            RETURN existing.id;
        END IF;
        RAISE EXCEPTION 'EMBEDDING_TRANSITION_PLAN_IDENTITY_CONFLICT' USING ERRCODE='40001';
    END IF;
    IF cardinality(target_recipe_identities)=0 THEN
        IF target_version>1 THEN RAISE EXCEPTION 'embedding transition successor recipes must preserve predecessor identities and input ordinals in order' USING ERRCODE='23514'; END IF;
        RAISE EXCEPTION 'embedding transition plan requires at least one recipe' USING ERRCODE='23514';
    END IF;
    FOR recipe_index IN 1..cardinality(target_recipe_identities) LOOP
        SELECT array_agg(value::BIGINT ORDER BY ordinal) INTO recipe_inputs
          FROM jsonb_array_elements_text(target_recipe_input_ordinals -> (recipe_index-1)) WITH ORDINALITY AS inputs(value,ordinal);
        IF NOT vestrace_zero_based_contiguous_ordinals(recipe_inputs) THEN
            RAISE EXCEPTION 'embedding transition recipe input ordinals must be contiguous from zero' USING ERRCODE='23514';
        END IF;
    END LOOP;
    IF target_version>1 THEN
        SELECT id INTO predecessor_id FROM embedding_transition_plans
         WHERE workspace_id=target_workspace_id AND transition_id=target_transition_id AND version=target_version-1 FOR SHARE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding transition successor requires its exact predecessor version' USING ERRCODE='23514'; END IF;
        IF vestrace_transition_recipe_structure(target_workspace_id,predecessor_id) IS DISTINCT FROM target_structure THEN
            RAISE EXCEPTION 'embedding transition successor recipes must preserve predecessor identities and input ordinals in order' USING ERRCODE='23514';
        END IF;
    END IF;
    INSERT INTO embedding_transition_plans(
        id,workspace_id,transition_id,version,source_connection_id,source_connection_revision_id,source_branch,
        source_credential_revision_id,source_no_auth_binding_revision_id,target_connection_id,target_connection_revision_id,
        target_connection_qualification_revision_id,target_model_revision_id,target_model_qualification_revision_id,target_branch,
        target_credential_revision_id,target_credential_slot_id,target_credential_activation_guard_id,target_expected_slot_version,
        target_no_auth_binding_revision_id,target_space_registration_id,transition_batch_id
    ) VALUES (
        target_plan_id,target_workspace_id,target_transition_id,target_version,target_source_connection_id,target_source_connection_revision_id,target_source_branch,
        target_source_credential_revision_id,target_source_no_auth_binding_revision_id,target_connection_id,target_connection_revision_id,
        target_connection_qualification_revision_id,target_model_revision_id,target_model_qualification_revision_id,target_branch,
        target_credential_revision_id,target_credential_slot_id,target_credential_activation_guard_id,target_expected_slot_version,
        target_no_auth_binding_revision_id,target_space_registration_id,target_batch_id
    );
    FOR recipe_index IN 1..cardinality(target_recipe_identities) LOOP
        SELECT array_agg(value::BIGINT ORDER BY ordinal) INTO recipe_inputs
          FROM jsonb_array_elements_text(target_recipe_input_ordinals -> (recipe_index-1)) WITH ORDINALITY AS inputs(value,ordinal);
        INSERT INTO embedding_transition_plan_recipes(workspace_id,transition_plan_id,recipe_ordinal,recipe_identity,input_ordinals)
        VALUES(target_workspace_id,target_plan_id,recipe_index-1,target_recipe_identities[recipe_index],recipe_inputs);
    END LOOP;
    INSERT INTO model_binding_snapshots(
        id,workspace_id,connection_id,connection_revision_id,connection_qualification_revision_id,model_revision_id,
        model_qualification_revision_id,branch,credential_revision_id,credential_slot_id,credential_activation_guard_id,
        expected_slot_version,no_auth_binding_revision_id
    ) VALUES (
        target_snapshot_id,target_workspace_id,target_connection_id,target_connection_revision_id,target_connection_qualification_revision_id,target_model_revision_id,
        target_model_qualification_revision_id,target_branch,target_credential_revision_id,target_credential_slot_id,target_credential_activation_guard_id,
        target_expected_slot_version,target_no_auth_binding_revision_id
    );
    INSERT INTO model_binding_snapshot_scopes(workspace_id,snapshot_id,scope,transition_plan_id)
    VALUES(target_workspace_id,target_snapshot_id,'transition',target_plan_id);
    PERFORM vestrace_classify_embedding_transition_ambiguity_carries(target_workspace_id,target_transition_id,target_plan_id);
    RETURN target_plan_id;
END
$$;

-- Whole-body replacement: ordinary snapshots retain Task 6's exact behaviour;
-- the one additional branch is a durable carry-to-plan-to-job identity check.
CREATE OR REPLACE FUNCTION vestrace_accept_embedding_job(
    target_job_id UUID, target_workspace_id UUID, target_space_registration_id UUID,
    target_kind TEXT, target_snapshot_id UUID, target_external_effect_id UUID,
    target_model_request_evidence_id UUID, target_retries_unknown_embedding_job_id UUID,
    target_expected_predecessor_version BIGINT
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE snapshot RECORD; guard_id UUID; existing embedding_jobs%ROWTYPE;
BEGIN
    IF target_job_id IS NULL OR target_workspace_id IS NULL OR target_space_registration_id IS NULL
       OR target_kind IS NULL OR target_snapshot_id IS NULL OR target_external_effect_id IS NULL
       OR target_model_request_evidence_id IS NULL OR target_kind NOT IN ('retrieval_query','delivery','rebuild')
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding job acceptance arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT id,connection_id,branch,credential_slot_id,credential_activation_guard_id INTO snapshot
      FROM model_binding_snapshots WHERE workspace_id=target_workspace_id AND id=target_snapshot_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job requires its exact pinned binding snapshot' USING ERRCODE='23514'; END IF;
    IF vestrace_snapshot_scope(target_workspace_id,target_snapshot_id) IS DISTINCT FROM 'ordinary'
       AND NOT EXISTS (
           SELECT 1 FROM embedding_transition_ambiguity_carries AS carry
           JOIN model_binding_snapshot_scopes AS scope
             ON scope.workspace_id=carry.workspace_id AND scope.snapshot_id=target_snapshot_id
            WHERE carry.workspace_id=target_workspace_id
              AND carry.successor_embedding_job_id=target_job_id
              AND carry.target_transition_plan_id=scope.transition_plan_id
       ) THEN
        RAISE EXCEPTION 'embedding job requires an ordinary binding snapshot' USING ERRCODE='23514';
    END IF;
    SELECT id INTO guard_id FROM connection_execution_guards
     WHERE workspace_id=target_workspace_id AND connection_id=snapshot.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job requires its existing permanent connection guard' USING ERRCODE='23514'; END IF;
    IF snapshot.branch='credential' THEN
        IF snapshot.credential_slot_id IS NULL OR snapshot.credential_activation_guard_id IS NULL THEN
            RAISE EXCEPTION 'embedding job credential branch is not exact' USING ERRCODE='23514';
        END IF;
        PERFORM 1 FROM credential_activation_guards
         WHERE workspace_id=target_workspace_id AND connection_id=snapshot.connection_id
           AND credential_slot_id=snapshot.credential_slot_id AND id=snapshot.credential_activation_guard_id
           AND execution_guard_id=guard_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding job credential branch requires its exact activation guard' USING ERRCODE='23514'; END IF;
    ELSIF snapshot.credential_slot_id IS NOT NULL OR snapshot.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding job no-auth branch cannot reference a credential' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_registrations
     WHERE workspace_id=target_workspace_id AND id=target_space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job requires its authorized embedding space' USING ERRCODE='23514'; END IF;
    SELECT * INTO existing FROM embedding_jobs WHERE workspace_id=target_workspace_id AND id=target_job_id FOR UPDATE;
    IF FOUND THEN
        IF existing.space_registration_id=target_space_registration_id AND existing.kind=target_kind
           AND existing.model_binding_snapshot_id=target_snapshot_id AND existing.external_effect_id=target_external_effect_id
           AND existing.model_request_evidence_id=target_model_request_evidence_id
           AND existing.retries_unknown_embedding_job_id IS NOT DISTINCT FROM target_retries_unknown_embedding_job_id
           AND (target_retries_unknown_embedding_job_id IS NULL OR EXISTS(
               SELECT 1 FROM embedding_jobs AS predecessor WHERE predecessor.workspace_id=target_workspace_id
                 AND predecessor.id=target_retries_unknown_embedding_job_id AND predecessor.version=target_expected_predecessor_version
           )) THEN RETURN existing.id; END IF;
        RAISE EXCEPTION 'embedding job identity conflicts with its existing tuple' USING ERRCODE='23514';
    END IF;
    IF target_retries_unknown_embedding_job_id IS NOT NULL THEN
        IF target_expected_predecessor_version IS NULL THEN
            RAISE EXCEPTION 'embedding job successor must name the predecessor version it read' USING ERRCODE='22023';
        END IF;
        PERFORM 1 FROM embedding_jobs WHERE workspace_id=target_workspace_id
          AND id=target_retries_unknown_embedding_job_id AND kind=target_kind
          AND state='inconclusive_unknown' AND version=target_expected_predecessor_version FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job successor requires a terminal inconclusive predecessor at the version it read' USING ERRCODE='23514';
        END IF;
    ELSIF target_expected_predecessor_version IS NOT NULL THEN
        RAISE EXCEPTION 'embedding job without a predecessor cannot name a predecessor version' USING ERRCODE='22023';
    END IF;
    INSERT INTO embedding_jobs(id,workspace_id,space_registration_id,kind,model_binding_snapshot_id,external_effect_id,model_request_evidence_id,retries_unknown_embedding_job_id)
    VALUES(target_job_id,target_workspace_id,target_space_registration_id,target_kind,target_snapshot_id,target_external_effect_id,target_model_request_evidence_id,target_retries_unknown_embedding_job_id);
    RETURN target_job_id;
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
DECLARE carry embedding_transition_ambiguity_carries%ROWTYPE; target_plan embedding_transition_plans%ROWTYPE;
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
    SELECT * INTO target_plan FROM embedding_transition_plans
     WHERE workspace_id=target_workspace_id AND id=carry.target_transition_plan_id FOR SHARE;
    IF NOT FOUND
       OR target_snapshot_id NOT IN (SELECT snapshot_id FROM model_binding_snapshot_scopes WHERE workspace_id=target_workspace_id AND transition_plan_id=carry.target_transition_plan_id AND scope='transition') THEN
        RAISE EXCEPTION 'embedding transition carry acknowledgement mapping is stale' USING ERRCODE='23514';
    END IF;
    UPDATE embedding_transition_ambiguity_carries SET state='successor_created', successor_embedding_job_id=target_successor_embedding_job_id
     WHERE workspace_id=target_workspace_id AND id=target_carry_id;
    RETURN vestrace_accept_embedding_job(
        target_successor_embedding_job_id,target_workspace_id,target_space_registration_id,target_kind,target_snapshot_id,
        target_external_effect_id,target_model_request_evidence_id,target_head_embedding_job_id,target_expected_head_version
    );
END
$$;

DO $$
DECLARE target_table TEXT; target_function REGPROCEDURE;
BEGIN
    FOREACH target_table IN ARRAY ARRAY[
        'embedding_transition_ambiguity_carries',
        'embedding_transition_ambiguity_carry_recipes'
    ]::TEXT[] LOOP
        BEGIN
            PERFORM vestrace_assign_p03_table_owner(format('public.%I', target_table)::REGCLASS);
        EXCEPTION WHEN insufficient_privilege THEN
            IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN RAISE; END IF;
            EXECUTE format('ALTER TABLE public.%I OWNER TO vestrace_guarded_owner',target_table);
            EXECUTE format('REVOKE ALL ON TABLE public.%I FROM PUBLIC',target_table);
            EXECUTE format('REVOKE ALL ON TABLE public.%I FROM vestrace',target_table);
            EXECUTE format('GRANT SELECT, REFERENCES ON TABLE public.%I TO vestrace',target_table);
        END;
    END LOOP;
    FOREACH target_function IN ARRAY ARRAY[
        'vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'::REGPROCEDURE,
        'vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE,
        'vestrace_classify_embedding_transition_ambiguity_carries(UUID, UUID, UUID)'::REGPROCEDURE,
        'vestrace_acknowledge_carried_transition_batch_after_unknown(UUID, UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID)'::REGPROCEDURE
    ]::REGPROCEDURE[] LOOP
        BEGIN
            PERFORM vestrace_assign_p03_function_owner(target_function);
        EXCEPTION WHEN insufficient_privilege THEN
            IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN RAISE; END IF;
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
        END;
    END LOOP;
END
$$;

-- SQLx applies migrations as its superuser test role without re-running the
-- compose provisioner. The provisioner grants these to the restricted runtime
-- when it transfers ownership; preserve the same ACL in that test-only path.
DO $$
BEGIN
    IF COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        REVOKE ALL ON FUNCTION vestrace_classify_embedding_transition_ambiguity_carries(UUID,UUID,UUID) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB) TO vestrace;
        GRANT EXECUTE ON FUNCTION vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT) TO vestrace;
        GRANT EXECUTE ON FUNCTION vestrace_acknowledge_carried_transition_batch_after_unknown(UUID,UUID,UUID,UUID,BIGINT,UUID,UUID,TEXT,UUID,UUID,UUID) TO vestrace;
    END IF;
END
$$;
