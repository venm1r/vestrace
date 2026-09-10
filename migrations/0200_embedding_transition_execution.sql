-- P04 Task 6: exact transition recipes, physical attempts, and completeness.
--
-- Batches are immutable wire structures.  A batch recipe is first materialized
-- from the transition plan, then bound once to its exact old Live projection
-- before an attempt may start.  The only public observations derive their
-- satisfaction kind from terminal database facts; no caller supplies it.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_transition_execution_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_transition_execution_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding transition execution upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

ALTER TABLE embedding_transitions
    ADD COLUMN version BIGINT NOT NULL DEFAULT 1 CHECK(version >= 1);

ALTER TABLE embedding_transition_plan_recipes
    ADD COLUMN old_projection_id UUID,
    ADD COLUMN target_input_ordinal BIGINT;

CREATE TABLE embedding_transition_batches (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_id UUID NOT NULL,
    transition_plan_id UUID NOT NULL,
    target_space_registration_id UUID NOT NULL,
    state TEXT NOT NULL DEFAULT 'planned'
        CHECK(state IN ('planned','rebuilding','ready_to_activate','stale','failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(workspace_id,id),
    UNIQUE(workspace_id,transition_plan_id),
    FOREIGN KEY(workspace_id,transition_id)
        REFERENCES embedding_transitions(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,target_space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id,id) ON DELETE RESTRICT
);

CREATE TABLE embedding_transition_batch_recipes (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    batch_id UUID NOT NULL,
    recipe_ordinal BIGINT NOT NULL CHECK(recipe_ordinal >= 0),
    recipe_identity UUID NOT NULL,
    input_ordinals BIGINT[] NOT NULL
        CHECK(vestrace_zero_based_contiguous_ordinals(input_ordinals)),
    old_projection_id UUID,
    target_input_ordinal BIGINT,
    binding_state TEXT NOT NULL DEFAULT 'unbound'
        CHECK(binding_state IN ('unbound','bound')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(workspace_id,batch_id,recipe_ordinal),
    UNIQUE(workspace_id,batch_id,recipe_identity),
    UNIQUE(workspace_id,batch_id,recipe_ordinal,recipe_identity),
    FOREIGN KEY(workspace_id,batch_id)
        REFERENCES embedding_transition_batches(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,old_projection_id)
        REFERENCES embedding_projection_entries(workspace_id,id) ON DELETE RESTRICT,
    CHECK(
        (binding_state='unbound' AND old_projection_id IS NULL AND target_input_ordinal IS NULL)
        OR
        (binding_state='bound' AND old_projection_id IS NOT NULL AND target_input_ordinal IS NOT NULL)
    )
);

CREATE UNIQUE INDEX embedding_transition_batch_recipes_one_old_projection
    ON embedding_transition_batch_recipes(workspace_id,batch_id,old_projection_id)
    WHERE old_projection_id IS NOT NULL;
CREATE UNIQUE INDEX embedding_transition_batch_recipes_one_target_input
    ON embedding_transition_batch_recipes(workspace_id,batch_id,target_input_ordinal)
    WHERE target_input_ordinal IS NOT NULL;

CREATE TABLE embedding_transition_recipe_dependencies (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    batch_id UUID NOT NULL,
    recipe_ordinal BIGINT NOT NULL CHECK(recipe_ordinal >= 0),
    source_ordinal INTEGER NOT NULL CHECK(source_ordinal >= 0),
    source_material_id UUID NOT NULL,
    source_intent_id UUID NOT NULL,
    erasure_blocker_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(workspace_id,batch_id,recipe_ordinal,source_ordinal),
    FOREIGN KEY(workspace_id,batch_id,recipe_ordinal)
        REFERENCES embedding_transition_batch_recipes(workspace_id,batch_id,recipe_ordinal)
        ON DELETE RESTRICT,
    FOREIGN KEY(source_material_id,workspace_id,source_intent_id)
        REFERENCES content_materials(id,workspace_id,intent_id) ON DELETE RESTRICT,
    FOREIGN KEY(erasure_blocker_id,workspace_id)
        REFERENCES material_erasure_blockers(id,workspace_id) ON DELETE RESTRICT
);

CREATE TABLE embedding_transition_job_attempts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    batch_id UUID NOT NULL,
    recipe_ordinal BIGINT NOT NULL CHECK(recipe_ordinal >= 0),
    job_id UUID NOT NULL,
    model_binding_snapshot_id UUID NOT NULL,
    expected_job_version BIGINT NOT NULL CHECK(expected_job_version >= 1),
    state TEXT NOT NULL DEFAULT 'created'
        CHECK(state IN ('created','observed','rejected')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    observed_at TIMESTAMPTZ,
    UNIQUE(workspace_id,id),
    UNIQUE(workspace_id,job_id),
    UNIQUE(workspace_id,batch_id,recipe_ordinal,job_id),
    UNIQUE(workspace_id,id,batch_id,recipe_ordinal),
    FOREIGN KEY(workspace_id,batch_id,recipe_ordinal)
        REFERENCES embedding_transition_batch_recipes(workspace_id,batch_id,recipe_ordinal)
        ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,job_id)
        REFERENCES embedding_jobs(workspace_id,id) ON DELETE RESTRICT,
    CHECK((state='created' AND observed_at IS NULL) OR (state IN ('observed','rejected') AND observed_at IS NOT NULL))
);

CREATE TABLE embedding_transition_recipe_satisfactions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    batch_id UUID NOT NULL,
    recipe_ordinal BIGINT NOT NULL CHECK(recipe_ordinal >= 0),
    satisfaction_kind TEXT NOT NULL CHECK(satisfaction_kind IN ('satisfied_result','satisfied_existing')),
    attempt_id UUID,
    satisfying_projection_id UUID NOT NULL,
    satisfying_material_id UUID NOT NULL,
    terminal_job_id UUID NOT NULL,
    terminal_job_version BIGINT NOT NULL CHECK(terminal_job_version >= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(workspace_id,batch_id,recipe_ordinal),
    UNIQUE(workspace_id,batch_id,satisfying_projection_id),
    UNIQUE(workspace_id,id),
    FOREIGN KEY(workspace_id,batch_id,recipe_ordinal)
        REFERENCES embedding_transition_batch_recipes(workspace_id,batch_id,recipe_ordinal)
        ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,attempt_id,batch_id,recipe_ordinal)
        REFERENCES embedding_transition_job_attempts(workspace_id,id,batch_id,recipe_ordinal)
        ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,satisfying_projection_id)
        REFERENCES embedding_projection_entries(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(satisfying_material_id,workspace_id)
        REFERENCES content_materials(id,workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,terminal_job_id)
        REFERENCES embedding_jobs(workspace_id,id) ON DELETE RESTRICT,
    CHECK(
        (satisfaction_kind='satisfied_result' AND attempt_id IS NOT NULL)
        OR
        (satisfaction_kind='satisfied_existing' AND attempt_id IS NULL)
    )
);

CREATE TABLE embedding_transition_observations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_id UUID NOT NULL,
    transition_plan_id UUID NOT NULL,
    batch_id UUID NOT NULL,
    attempt_id UUID,
    observation_kind TEXT NOT NULL CHECK(observation_kind IN (
        'attempt_created','satisfied_result','satisfied_existing','ready_to_activate','refused'
    )),
    safe_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(workspace_id,id),
    FOREIGN KEY(workspace_id,transition_id)
        REFERENCES embedding_transitions(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,batch_id)
        REFERENCES embedding_transition_batches(workspace_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(workspace_id,attempt_id)
        REFERENCES embedding_transition_job_attempts(workspace_id,id) ON DELETE RESTRICT
);

-- Existing plan rows remain historical planning evidence.  Their new batch
-- rows preserve the fixed recipe wire structure but are deliberately unbound:
-- 0200 never invents an old projection or a dependency lineage.
INSERT INTO embedding_transition_batches(
    id,workspace_id,transition_id,transition_plan_id,target_space_registration_id,state,created_at
)
SELECT plan.transition_batch_id,plan.workspace_id,plan.transition_id,plan.id,
       plan.target_space_registration_id,'planned',plan.created_at
  FROM embedding_transition_plans AS plan
ON CONFLICT(workspace_id,id) DO NOTHING;

INSERT INTO embedding_transition_batch_recipes(
    workspace_id,batch_id,recipe_ordinal,recipe_identity,input_ordinals,binding_state,created_at
)
SELECT recipe.workspace_id,plan.transition_batch_id,recipe.recipe_ordinal,
       recipe.recipe_identity,recipe.input_ordinals,'unbound',recipe.created_at
  FROM embedding_transition_plan_recipes AS recipe
  JOIN embedding_transition_plans AS plan
    ON plan.workspace_id=recipe.workspace_id AND plan.id=recipe.transition_plan_id
ON CONFLICT(workspace_id,batch_id,recipe_ordinal) DO NOTHING;

-- Carry and barrier rows keep their observed ordinal mappings, but the new
-- nullable identity links make any future exact mapping foreign-keyed rather
-- than a second independent recipe authority.  Historical rows cannot be
-- fabricated into exact batch recipes by this forward migration.
ALTER TABLE embedding_transition_ambiguity_carry_recipes
    ADD COLUMN old_batch_id UUID,
    ADD COLUMN old_recipe_identity UUID,
    ADD COLUMN new_batch_id UUID,
    ADD COLUMN new_recipe_identity UUID,
    ADD CONSTRAINT embedding_transition_carry_old_batch_recipe_fkey
        FOREIGN KEY(workspace_id,old_batch_id,old_recipe_ordinal,old_recipe_identity)
        REFERENCES embedding_transition_batch_recipes(
            workspace_id,batch_id,recipe_ordinal,recipe_identity
        )
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT embedding_transition_carry_new_batch_recipe_fkey
        FOREIGN KEY(workspace_id,new_batch_id,new_recipe_ordinal,new_recipe_identity)
        REFERENCES embedding_transition_batch_recipes(
            workspace_id,batch_id,recipe_ordinal,recipe_identity
        )
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT embedding_transition_carry_exact_batch_recipe_pairing
        CHECK(
            (old_batch_id IS NULL AND old_recipe_identity IS NULL)
            OR (old_batch_id IS NOT NULL AND old_recipe_identity IS NOT NULL)
        ),
    ADD CONSTRAINT embedding_transition_carry_exact_successor_batch_recipe_pairing
        CHECK(
            (new_batch_id IS NULL AND new_recipe_identity IS NULL)
            OR (new_batch_id IS NOT NULL AND new_recipe_identity IS NOT NULL)
        );

ALTER TABLE embedding_transition_barrier_recipes
    ADD COLUMN old_batch_id UUID,
    ADD COLUMN old_recipe_identity UUID,
    ADD COLUMN new_batch_id UUID,
    ADD COLUMN new_recipe_identity UUID,
    ADD CONSTRAINT embedding_transition_barrier_old_batch_recipe_fkey
        FOREIGN KEY(workspace_id,old_batch_id,old_recipe_ordinal,old_recipe_identity)
        REFERENCES embedding_transition_batch_recipes(
            workspace_id,batch_id,recipe_ordinal,recipe_identity
        )
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT embedding_transition_barrier_new_batch_recipe_fkey
        FOREIGN KEY(workspace_id,new_batch_id,new_recipe_ordinal,new_recipe_identity)
        REFERENCES embedding_transition_batch_recipes(
            workspace_id,batch_id,recipe_ordinal,recipe_identity
        )
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT embedding_transition_barrier_exact_batch_recipe_pairing
        CHECK(
            (old_batch_id IS NULL AND old_recipe_identity IS NULL)
            OR (old_batch_id IS NOT NULL AND old_recipe_identity IS NOT NULL)
        ),
    ADD CONSTRAINT embedding_transition_barrier_exact_successor_batch_recipe_pairing
        CHECK(
            (new_batch_id IS NULL AND new_recipe_identity IS NULL)
            OR (new_batch_id IS NOT NULL AND new_recipe_identity IS NOT NULL)
        );

-- Carry and barrier callers continue to state only their already-authorized
-- ordinal correspondence.  These guarded triggers derive the corresponding
-- batch and recipe identities from the immutable headers, and reject a caller
-- that somehow supplies a conflicting identity tuple.
CREATE OR REPLACE FUNCTION vestrace_derive_embedding_transition_carry_recipe_identity()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE derived_old_batch UUID;
        derived_old_identity UUID;
        derived_old_inputs BIGINT[];
        derived_new_batch UUID;
        derived_new_identity UUID;
        derived_new_inputs BIGINT[];
BEGIN
    PERFORM set_config('vestrace.workspace_id',NEW.workspace_id::TEXT,true);
    SELECT source_plan.transition_batch_id,recipe.recipe_identity,recipe.input_ordinals
      INTO derived_old_batch,derived_old_identity,derived_old_inputs
      FROM embedding_transition_ambiguity_carries AS carry
      JOIN embedding_transition_plans AS source_plan
        ON source_plan.workspace_id=carry.workspace_id AND source_plan.id=carry.source_transition_plan_id
      JOIN embedding_transition_batch_recipes AS recipe
        ON recipe.workspace_id=source_plan.workspace_id
       AND recipe.batch_id=source_plan.transition_batch_id
       AND recipe.recipe_ordinal=NEW.old_recipe_ordinal
     WHERE carry.workspace_id=NEW.workspace_id AND carry.id=NEW.carry_id;
    IF NOT FOUND OR NEW.old_input_ordinal<1
       OR NEW.old_input_ordinal>cardinality(derived_old_inputs) THEN
        RAISE EXCEPTION 'embedding transition carry maps an absent exact old recipe input'
            USING ERRCODE='23514';
    END IF;
    IF NEW.state='awaiting_acknowledgement' THEN
        SELECT target_plan.transition_batch_id,recipe.recipe_identity,recipe.input_ordinals
          INTO derived_new_batch,derived_new_identity,derived_new_inputs
          FROM embedding_transition_ambiguity_carries AS carry
          JOIN embedding_transition_plans AS target_plan
            ON target_plan.workspace_id=carry.workspace_id AND target_plan.id=carry.target_transition_plan_id
          JOIN embedding_transition_batch_recipes AS recipe
            ON recipe.workspace_id=target_plan.workspace_id
           AND recipe.batch_id=target_plan.transition_batch_id
           AND recipe.recipe_ordinal=NEW.new_recipe_ordinal
         WHERE carry.workspace_id=NEW.workspace_id AND carry.id=NEW.carry_id;
        IF NOT FOUND OR NEW.new_input_ordinal<1
           OR NEW.new_input_ordinal>cardinality(derived_new_inputs) THEN
            RAISE EXCEPTION 'embedding transition carry maps an absent exact successor recipe input'
                USING ERRCODE='23514';
        END IF;
    END IF;
    IF (NEW.old_batch_id IS NOT NULL AND NEW.old_batch_id<>derived_old_batch)
       OR (NEW.old_recipe_identity IS NOT NULL AND NEW.old_recipe_identity<>derived_old_identity)
       OR (NEW.new_batch_id IS NOT NULL AND NEW.new_batch_id IS DISTINCT FROM derived_new_batch)
       OR (NEW.new_recipe_identity IS NOT NULL AND NEW.new_recipe_identity IS DISTINCT FROM derived_new_identity) THEN
        RAISE EXCEPTION 'embedding transition carry cannot assert a different exact recipe identity'
            USING ERRCODE='23514';
    END IF;
    NEW.old_batch_id:=derived_old_batch;
    NEW.old_recipe_identity:=derived_old_identity;
    NEW.new_batch_id:=derived_new_batch;
    NEW.new_recipe_identity:=derived_new_identity;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_derive_embedding_transition_barrier_recipe_identity()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE derived_old_batch UUID;
        derived_old_identity UUID;
        derived_old_inputs BIGINT[];
        derived_new_batch UUID;
        derived_new_identity UUID;
        derived_new_inputs BIGINT[];
BEGIN
    PERFORM set_config('vestrace.workspace_id',NEW.workspace_id::TEXT,true);
    SELECT barrier.predecessor_transition_batch_id,recipe.recipe_identity,recipe.input_ordinals
      INTO derived_old_batch,derived_old_identity,derived_old_inputs
      FROM embedding_transition_barriers AS barrier
      JOIN embedding_transition_batch_recipes AS recipe
        ON recipe.workspace_id=barrier.workspace_id
       AND recipe.batch_id=barrier.predecessor_transition_batch_id
       AND recipe.recipe_ordinal=NEW.old_recipe_ordinal
     WHERE barrier.workspace_id=NEW.workspace_id AND barrier.id=NEW.barrier_id;
    IF NOT FOUND OR NEW.old_input_ordinal<1
       OR NEW.old_input_ordinal>cardinality(derived_old_inputs) THEN
        RAISE EXCEPTION 'embedding transition barrier maps an absent exact old recipe input'
            USING ERRCODE='23514';
    END IF;
    SELECT target_plan.transition_batch_id,recipe.recipe_identity,recipe.input_ordinals
      INTO derived_new_batch,derived_new_identity,derived_new_inputs
      FROM embedding_transition_barriers AS barrier
      JOIN embedding_transition_plans AS target_plan
        ON target_plan.workspace_id=barrier.workspace_id AND target_plan.id=barrier.target_transition_plan_id
      JOIN embedding_transition_batch_recipes AS recipe
        ON recipe.workspace_id=target_plan.workspace_id
       AND recipe.batch_id=target_plan.transition_batch_id
       AND recipe.recipe_ordinal=NEW.new_recipe_ordinal
     WHERE barrier.workspace_id=NEW.workspace_id AND barrier.id=NEW.barrier_id;
    IF NOT FOUND OR NEW.new_input_ordinal<1
       OR NEW.new_input_ordinal>cardinality(derived_new_inputs) THEN
        RAISE EXCEPTION 'embedding transition barrier maps an absent exact successor recipe input'
            USING ERRCODE='23514';
    END IF;
    IF (NEW.old_batch_id IS NOT NULL AND NEW.old_batch_id<>derived_old_batch)
       OR (NEW.old_recipe_identity IS NOT NULL AND NEW.old_recipe_identity<>derived_old_identity)
       OR (NEW.new_batch_id IS NOT NULL AND NEW.new_batch_id<>derived_new_batch)
       OR (NEW.new_recipe_identity IS NOT NULL AND NEW.new_recipe_identity<>derived_new_identity) THEN
        RAISE EXCEPTION 'embedding transition barrier cannot assert a different exact recipe identity'
            USING ERRCODE='23514';
    END IF;
    NEW.old_batch_id:=derived_old_batch;
    NEW.old_recipe_identity:=derived_old_identity;
    NEW.new_batch_id:=derived_new_batch;
    NEW.new_recipe_identity:=derived_new_identity;
    RETURN NEW;
END
$$;

CREATE TRIGGER embedding_transition_carry_recipes_exact_identity
    BEFORE INSERT ON embedding_transition_ambiguity_carry_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_derive_embedding_transition_carry_recipe_identity();
CREATE TRIGGER embedding_transition_barrier_recipes_exact_identity
    BEFORE INSERT ON embedding_transition_barrier_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_derive_embedding_transition_barrier_recipe_identity();

CREATE OR REPLACE FUNCTION vestrace_materialize_embedding_transition_batch()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    PERFORM set_config('vestrace.workspace_id',NEW.workspace_id::TEXT,true);
    INSERT INTO embedding_transition_batches(
        id,workspace_id,transition_id,transition_plan_id,target_space_registration_id,state,created_at
    ) VALUES(
        NEW.transition_batch_id,NEW.workspace_id,NEW.transition_id,NEW.id,
        NEW.target_space_registration_id,'planned',NEW.created_at
    );
    RETURN NULL;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_materialize_embedding_transition_batch_recipe()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE target_batch_id UUID;
BEGIN
    PERFORM set_config('vestrace.workspace_id',NEW.workspace_id::TEXT,true);
    SELECT transition_batch_id INTO target_batch_id
      FROM embedding_transition_plans
     WHERE workspace_id=NEW.workspace_id AND id=NEW.transition_plan_id;
    IF target_batch_id IS NULL THEN
        RAISE EXCEPTION 'embedding transition batch header is absent' USING ERRCODE='23514';
    END IF;
    INSERT INTO embedding_transition_batch_recipes(
        workspace_id,batch_id,recipe_ordinal,recipe_identity,input_ordinals,binding_state,created_at
    ) VALUES(
        NEW.workspace_id,target_batch_id,NEW.recipe_ordinal,NEW.recipe_identity,
        NEW.input_ordinals,'unbound',NEW.created_at
    );
    RETURN NULL;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_guard_embedding_transition_header()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='INSERT' THEN
        RETURN NEW;
    END IF;
    IF TG_OP='DELETE'
       OR OLD.workspace_id IS DISTINCT FROM NEW.workspace_id
       OR OLD.id IS DISTINCT FROM NEW.id
       OR OLD.target_space_registration_id IS DISTINCT FROM NEW.target_space_registration_id
       OR OLD.created_at IS DISTINCT FROM NEW.created_at
       OR NEW.version<>OLD.version+1
       OR NOT (
           (OLD.state='planned' AND NEW.state='rebuilding')
           OR (OLD.state='rebuilding' AND NEW.state='ready_to_activate')
       ) THEN
        RAISE EXCEPTION 'embedding transition permits only guarded progress' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_guard_embedding_transition_batch_recipe()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='INSERT' THEN
        RETURN NEW;
    END IF;
    IF TG_OP='DELETE'
       OR OLD.workspace_id IS DISTINCT FROM NEW.workspace_id
       OR OLD.batch_id IS DISTINCT FROM NEW.batch_id
       OR OLD.recipe_ordinal IS DISTINCT FROM NEW.recipe_ordinal
       OR OLD.recipe_identity IS DISTINCT FROM NEW.recipe_identity
       OR OLD.input_ordinals IS DISTINCT FROM NEW.input_ordinals
       OR OLD.created_at IS DISTINCT FROM NEW.created_at
       OR OLD.binding_state<>'unbound'
       OR NEW.binding_state<>'bound'
       OR NEW.old_projection_id IS NULL
       OR NEW.target_input_ordinal IS NULL THEN
        RAISE EXCEPTION 'embedding transition batch recipe permits only its exact guarded binding'
            USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_guard_embedding_transition_batch_header()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF TG_OP='INSERT' THEN
        RETURN NEW;
    END IF;
    IF TG_OP='DELETE'
       OR OLD.workspace_id IS DISTINCT FROM NEW.workspace_id
       OR OLD.id IS DISTINCT FROM NEW.id
       OR OLD.transition_id IS DISTINCT FROM NEW.transition_id
       OR OLD.transition_plan_id IS DISTINCT FROM NEW.transition_plan_id
       OR OLD.target_space_registration_id IS DISTINCT FROM NEW.target_space_registration_id
       OR OLD.created_at IS DISTINCT FROM NEW.created_at
       OR NOT (
           (OLD.state='planned' AND NEW.state='rebuilding')
           OR (OLD.state='rebuilding' AND NEW.state='ready_to_activate')
       ) THEN
        RAISE EXCEPTION 'embedding transition batch permits only guarded progress' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END
$$;

DROP TRIGGER embedding_transitions_immutable ON embedding_transitions;
CREATE TRIGGER embedding_transitions_guarded_progress
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transitions
    FOR EACH ROW EXECUTE FUNCTION vestrace_guard_embedding_transition_header();
CREATE TRIGGER embedding_transition_batches_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_batches
    FOR EACH ROW EXECUTE FUNCTION vestrace_guard_embedding_transition_batch_header();
CREATE TRIGGER embedding_transition_batch_recipes_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_batch_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_guard_embedding_transition_batch_recipe();
CREATE TRIGGER embedding_transition_recipe_dependencies_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_recipe_dependencies
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_transition_job_attempts_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_job_attempts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER embedding_transition_recipe_satisfactions_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_recipe_satisfactions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_transition_observations_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_observations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_transition_plan_batch_materialized
    AFTER INSERT ON embedding_transition_plans
    FOR EACH ROW EXECUTE FUNCTION vestrace_materialize_embedding_transition_batch();
CREATE TRIGGER embedding_transition_plan_recipe_batch_materialized
    AFTER INSERT ON embedding_transition_plan_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_materialize_embedding_transition_batch_recipe();

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_transition_bijection(
    target_workspace UUID,
    target_batch UUID,
    target_require_complete BOOLEAN DEFAULT FALSE
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE batch_row embedding_transition_batches%ROWTYPE;
BEGIN
    SELECT * INTO batch_row FROM embedding_transition_batches
     WHERE workspace_id=target_workspace AND id=target_batch;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition batch is absent' USING ERRCODE='23514';
    END IF;
    IF batch_row.state<>'ready_to_activate' AND NOT target_require_complete THEN
        RETURN;
    END IF;
    IF EXISTS(
        SELECT 1 FROM embedding_transition_batch_recipes
         WHERE workspace_id=target_workspace AND batch_id=target_batch AND binding_state<>'bound'
    ) THEN
        RAISE EXCEPTION 'embedding transition batch has an omitted exact recipe binding'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1
          FROM embedding_transition_batch_recipes AS recipe
          JOIN embedding_projection_entries AS old_projection
            ON old_projection.workspace_id=recipe.workspace_id
           AND old_projection.id=recipe.old_projection_id
          LEFT JOIN embedding_transition_recipe_dependencies AS dependency
            ON dependency.workspace_id=recipe.workspace_id
           AND dependency.batch_id=recipe.batch_id
           AND dependency.recipe_ordinal=recipe.recipe_ordinal
          LEFT JOIN content_materials AS source_material
            ON source_material.workspace_id=dependency.workspace_id
           AND source_material.id=dependency.source_material_id
         WHERE recipe.workspace_id=target_workspace
           AND recipe.batch_id=target_batch
           AND (
               old_projection.state<>'live'
               OR dependency.source_material_id IS NULL
               OR source_material.state<>'live'
           )
    ) THEN
        RAISE EXCEPTION 'embedding transition recipe source lineage is no longer Live'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1
          FROM embedding_transition_batch_recipes AS recipe
          LEFT JOIN embedding_transition_recipe_satisfactions AS satisfaction
            ON satisfaction.workspace_id=recipe.workspace_id
           AND satisfaction.batch_id=recipe.batch_id
           AND satisfaction.recipe_ordinal=recipe.recipe_ordinal
         WHERE recipe.workspace_id=target_workspace
           AND recipe.batch_id=target_batch
           AND satisfaction.id IS NULL
    ) OR EXISTS(
        SELECT 1 FROM embedding_transition_recipe_satisfactions AS satisfaction
         LEFT JOIN embedding_transition_batch_recipes AS recipe
           ON recipe.workspace_id=satisfaction.workspace_id
          AND recipe.batch_id=satisfaction.batch_id
          AND recipe.recipe_ordinal=satisfaction.recipe_ordinal
         WHERE satisfaction.workspace_id=target_workspace
           AND satisfaction.batch_id=target_batch
           AND recipe.recipe_ordinal IS NULL
    ) THEN
        RAISE EXCEPTION 'embedding transition batch requires exactly one satisfier per recipe'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1
          FROM embedding_transition_recipe_satisfactions AS satisfaction
          JOIN embedding_transition_batch_recipes AS recipe
            ON recipe.workspace_id=satisfaction.workspace_id
           AND recipe.batch_id=satisfaction.batch_id
           AND recipe.recipe_ordinal=satisfaction.recipe_ordinal
          JOIN embedding_transition_batches AS batch
            ON batch.workspace_id=recipe.workspace_id AND batch.id=recipe.batch_id
          JOIN embedding_projection_entries AS projection
            ON projection.workspace_id=satisfaction.workspace_id
           AND projection.id=satisfaction.satisfying_projection_id
          JOIN content_materials AS material
            ON material.workspace_id=projection.workspace_id AND material.id=projection.material_id
          JOIN embedding_jobs AS job
            ON job.workspace_id=satisfaction.workspace_id AND job.id=satisfaction.terminal_job_id
         WHERE satisfaction.workspace_id=target_workspace
           AND satisfaction.batch_id=target_batch
           AND (
               projection.state<>'live'
               OR material.state<>'live'
               OR job.state<>'succeeded'
               OR satisfaction.terminal_job_version<>job.version
               OR satisfaction.satisfying_material_id<>projection.material_id
               OR projection.job_id<>job.id
               OR projection.space_registration_id<>batch.target_space_registration_id
               OR (satisfaction.satisfaction_kind='satisfied_result' AND NOT EXISTS(
                    SELECT 1
                      FROM embedding_transition_job_attempts AS attempt
                     WHERE attempt.workspace_id=satisfaction.workspace_id
                       AND attempt.id=satisfaction.attempt_id
                       AND attempt.batch_id=recipe.batch_id
                       AND attempt.recipe_ordinal=recipe.recipe_ordinal
                       AND attempt.job_id=job.id
                       AND attempt.state='observed'
                       AND projection.job_id=attempt.job_id
                       AND projection.input_ordinal=recipe.target_input_ordinal
                       AND EXISTS(
                            SELECT 1 FROM embedding_job_result_publications AS publication
                             WHERE publication.workspace_id=job.workspace_id
                               AND publication.job_id=job.id
                               AND publication.terminal_job_version=job.version
                       )
               ))
               OR (satisfaction.satisfaction_kind='satisfied_existing' AND (
                    satisfaction.attempt_id IS NOT NULL
                    OR projection.id<>recipe.old_projection_id
                    OR projection.input_ordinal<>recipe.target_input_ordinal
                    OR NOT EXISTS(
                        SELECT 1 FROM embedding_job_result_publications AS publication
                         WHERE publication.workspace_id=job.workspace_id
                           AND publication.job_id=job.id
                           AND publication.terminal_job_version=job.version
                    )
               ))
           )
    ) THEN
        RAISE EXCEPTION 'embedding transition satisfier is not its exact terminal Live lineage'
            USING ERRCODE='23514';
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_transition_bijection_trigger()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE changed_row JSONB;
        target_workspace UUID;
        target_batch UUID;
BEGIN
    changed_row:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
    target_workspace:=(changed_row->>'workspace_id')::UUID;
    target_batch:=CASE TG_TABLE_NAME
        WHEN 'embedding_transition_batches' THEN (changed_row->>'id')::UUID
        ELSE (changed_row->>'batch_id')::UUID
    END;
    IF target_workspace IS NOT NULL AND target_batch IS NOT NULL THEN
        PERFORM vestrace_validate_embedding_transition_bijection(target_workspace,target_batch);
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER embedding_transition_batch_bijection_deferred
    AFTER INSERT OR UPDATE OR DELETE ON embedding_transition_batches
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_embedding_transition_bijection_trigger();
CREATE CONSTRAINT TRIGGER embedding_transition_batch_recipes_bijection_deferred
    AFTER INSERT OR UPDATE OR DELETE ON embedding_transition_batch_recipes
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_embedding_transition_bijection_trigger();
CREATE CONSTRAINT TRIGGER embedding_transition_attempts_bijection_deferred
    AFTER INSERT OR UPDATE OR DELETE ON embedding_transition_job_attempts
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_embedding_transition_bijection_trigger();
CREATE CONSTRAINT TRIGGER embedding_transition_satisfactions_bijection_deferred
    AFTER INSERT OR UPDATE OR DELETE ON embedding_transition_recipe_satisfactions
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_embedding_transition_bijection_trigger();

CREATE OR REPLACE FUNCTION vestrace_create_embedding_transition_batch_attempt(
    target_workspace UUID,
    target_plan UUID,
    target_batch UUID,
    target_attempt UUID,
    target_job UUID,
    target_recipe_ordinal BIGINT,
    target_old_projection UUID,
    target_input_ordinal BIGINT,
    target_expected_job_version BIGINT
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE plan_row embedding_transition_plans%ROWTYPE;
        batch_row embedding_transition_batches%ROWTYPE;
        recipe_row embedding_transition_batch_recipes%ROWTYPE;
        job_row embedding_jobs%ROWTYPE;
        projection_row embedding_projection_entries%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_plan IS NULL OR target_batch IS NULL
       OR target_attempt IS NULL OR target_job IS NULL OR target_recipe_ordinal IS NULL
       OR target_old_projection IS NULL OR target_input_ordinal IS NULL
       OR target_expected_job_version IS NULL OR target_expected_job_version<1
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding transition attempt arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO plan_row FROM embedding_transition_plans
     WHERE workspace_id=target_workspace AND id=target_plan FOR UPDATE;
    IF NOT FOUND OR plan_row.transition_batch_id<>target_batch THEN
        RAISE EXCEPTION 'embedding transition attempt names the wrong batch' USING ERRCODE='23514';
    END IF;
    SELECT * INTO batch_row FROM embedding_transition_batches
     WHERE workspace_id=target_workspace AND id=target_batch AND transition_plan_id=target_plan
     FOR UPDATE;
    IF NOT FOUND OR batch_row.state NOT IN ('planned','rebuilding') THEN
        RAISE EXCEPTION 'embedding transition batch is not rebuildable' USING ERRCODE='23514';
    END IF;
    SELECT * INTO recipe_row FROM embedding_transition_batch_recipes
     WHERE workspace_id=target_workspace AND batch_id=target_batch
       AND recipe_ordinal=target_recipe_ordinal FOR UPDATE;
    IF NOT FOUND OR NOT target_input_ordinal=ANY(recipe_row.input_ordinals) THEN
        RAISE EXCEPTION 'embedding transition attempt has the wrong recipe input order'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO projection_row FROM embedding_projection_entries
     WHERE workspace_id=target_workspace AND id=target_old_projection FOR SHARE;
    IF NOT FOUND OR projection_row.state<>'live' THEN
        RAISE EXCEPTION 'embedding transition attempt has a non-Live old projection'
            USING ERRCODE='23514';
    END IF;
    IF recipe_row.binding_state='unbound' THEN
        UPDATE embedding_transition_batch_recipes
           SET old_projection_id=target_old_projection,
               target_input_ordinal=vestrace_create_embedding_transition_batch_attempt.target_input_ordinal,
               binding_state='bound'
         WHERE workspace_id=target_workspace AND batch_id=target_batch
           AND recipe_ordinal=target_recipe_ordinal;
        INSERT INTO embedding_transition_recipe_dependencies(
            workspace_id,batch_id,recipe_ordinal,source_ordinal,source_material_id,
            source_intent_id,erasure_blocker_id
        )
        SELECT dependency.workspace_id,target_batch,target_recipe_ordinal,
               dependency.source_ordinal,dependency.source_material_id,
               dependency.source_intent_id,dependency.erasure_blocker_id
          FROM embedding_projection_source_dependencies AS dependency
         WHERE dependency.workspace_id=target_workspace
           AND dependency.projection_id=target_old_projection;
    ELSIF recipe_row.old_projection_id<>target_old_projection
       OR recipe_row.target_input_ordinal<>target_input_ordinal THEN
        RAISE EXCEPTION 'embedding transition batch recipe has a different exact binding'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO job_row FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
    IF NOT FOUND OR job_row.space_registration_id<>batch_row.target_space_registration_id
       OR job_row.kind NOT IN ('delivery','rebuild')
       OR job_row.version<>target_expected_job_version THEN
        RAISE EXCEPTION 'embedding transition attempt has the wrong physical job target space or version'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_transition_job_attempts WHERE workspace_id=target_workspace AND job_id=target_job) THEN
        RAISE EXCEPTION 'embedding transition physical job may not be reused' USING ERRCODE='23514';
    END IF;
    INSERT INTO embedding_transition_job_attempts(
        id,workspace_id,batch_id,recipe_ordinal,job_id,model_binding_snapshot_id,expected_job_version,state
    ) VALUES(
        target_attempt,target_workspace,target_batch,target_recipe_ordinal,target_job,
        job_row.model_binding_snapshot_id,target_expected_job_version,'created'
    );
    IF batch_row.state='planned' THEN
        UPDATE embedding_transition_batches SET state='rebuilding'
         WHERE workspace_id=target_workspace AND id=target_batch;
        UPDATE embedding_transitions SET state='rebuilding',version=version+1
         WHERE workspace_id=target_workspace AND id=plan_row.transition_id AND state='planned';
    END IF;
    INSERT INTO embedding_transition_observations(
        id,workspace_id,transition_id,transition_plan_id,batch_id,attempt_id,observation_kind
    ) VALUES(
        gen_random_uuid(),target_workspace,plan_row.transition_id,target_plan,target_batch,target_attempt,'attempt_created'
    );
    RETURN target_job;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_observe_embedding_transition_attempt(
    target_workspace UUID,
    target_plan UUID,
    target_batch UUID,
    target_recipe_ordinal BIGINT,
    target_attempt UUID,
    target_expected_job_version BIGINT
) RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE plan_row embedding_transition_plans%ROWTYPE;
        recipe_row embedding_transition_batch_recipes%ROWTYPE;
        attempt_row embedding_transition_job_attempts%ROWTYPE;
        job_row embedding_jobs%ROWTYPE;
        projection_row embedding_projection_entries%ROWTYPE;
        material_state TEXT;
        observation_kind TEXT;
BEGIN
    IF target_workspace IS NULL OR target_plan IS NULL OR target_batch IS NULL
       OR target_recipe_ordinal IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID
       OR (target_attempt IS NOT NULL AND (target_expected_job_version IS NULL OR target_expected_job_version<1)) THEN
        RAISE EXCEPTION 'embedding transition observation arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO plan_row FROM embedding_transition_plans
     WHERE workspace_id=target_workspace AND id=target_plan AND transition_batch_id=target_batch
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition observation names the wrong batch' USING ERRCODE='23514';
    END IF;
    SELECT * INTO recipe_row FROM embedding_transition_batch_recipes
     WHERE workspace_id=target_workspace AND batch_id=target_batch
       AND recipe_ordinal=target_recipe_ordinal FOR UPDATE;
    IF NOT FOUND OR recipe_row.binding_state<>'bound' THEN
        RAISE EXCEPTION 'embedding transition observation has an omitted recipe binding'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1 FROM embedding_transition_recipe_satisfactions
         WHERE workspace_id=target_workspace AND batch_id=target_batch
           AND recipe_ordinal=target_recipe_ordinal
    ) THEN
        RAISE EXCEPTION 'embedding transition recipe already has a satisfier' USING ERRCODE='23514';
    END IF;
    IF target_attempt IS NULL THEN
        SELECT projection.* INTO projection_row
          FROM embedding_projection_entries AS projection
         WHERE projection.workspace_id=target_workspace
           AND projection.id=recipe_row.old_projection_id
         FOR UPDATE;
        SELECT * INTO job_row FROM embedding_jobs
         WHERE workspace_id=target_workspace AND id=projection_row.job_id
         FOR UPDATE;
        SELECT state INTO material_state FROM content_materials
         WHERE workspace_id=target_workspace AND id=projection_row.material_id FOR SHARE;
        IF NOT FOUND OR projection_row.state<>'live' OR material_state<>'live'
           OR job_row.state<>'succeeded' THEN
            RAISE EXCEPTION 'embedding transition existing projection is no longer Live'
                USING ERRCODE='23514';
        END IF;
        INSERT INTO embedding_transition_recipe_satisfactions(
            id,workspace_id,batch_id,recipe_ordinal,satisfaction_kind,attempt_id,
            satisfying_projection_id,satisfying_material_id,terminal_job_id,terminal_job_version
        ) VALUES(
            gen_random_uuid(),target_workspace,target_batch,target_recipe_ordinal,
            'satisfied_existing',NULL,projection_row.id,projection_row.material_id,
            projection_row.job_id,job_row.version
        );
        observation_kind:='satisfied_existing';
    ELSE
        SELECT * INTO attempt_row FROM embedding_transition_job_attempts
         WHERE workspace_id=target_workspace AND id=target_attempt
           AND batch_id=target_batch AND recipe_ordinal=target_recipe_ordinal
           AND state='created' FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding transition physical attempt is absent or already observed'
                USING ERRCODE='23514';
        END IF;
        SELECT * INTO job_row FROM embedding_jobs
         WHERE workspace_id=target_workspace AND id=attempt_row.job_id FOR UPDATE;
        IF NOT FOUND OR job_row.version<>target_expected_job_version OR job_row.state<>'succeeded' THEN
            RAISE EXCEPTION 'failed, cancelled, or ambiguous transition attempt cannot satisfy a recipe'
                USING ERRCODE='23514';
        END IF;
        SELECT projection.* INTO projection_row
          FROM embedding_projection_entries AS projection
          JOIN embedding_transition_batches AS batch
            ON batch.workspace_id=projection.workspace_id AND batch.id=target_batch
         WHERE projection.workspace_id=target_workspace
           AND projection.job_id=attempt_row.job_id
           AND projection.input_ordinal=recipe_row.target_input_ordinal
           AND projection.space_registration_id=batch.target_space_registration_id
           AND projection.state='live'
         FOR UPDATE OF projection;
        IF NOT FOUND OR NOT EXISTS(
            SELECT 1 FROM content_materials
             WHERE workspace_id=target_workspace AND id=projection_row.material_id AND state='live'
        ) OR NOT EXISTS(
            SELECT 1 FROM embedding_job_result_publications
             WHERE workspace_id=target_workspace AND job_id=job_row.id
               AND terminal_job_version=job_row.version
        ) THEN
            RAISE EXCEPTION 'embedding transition result is not its exact terminal Live projection'
                USING ERRCODE='23514';
        END IF;
        UPDATE embedding_transition_job_attempts
           SET state='observed',observed_at=now()
         WHERE workspace_id=target_workspace AND id=target_attempt;
        INSERT INTO embedding_transition_recipe_satisfactions(
            id,workspace_id,batch_id,recipe_ordinal,satisfaction_kind,attempt_id,
            satisfying_projection_id,satisfying_material_id,terminal_job_id,terminal_job_version
        ) VALUES(
            gen_random_uuid(),target_workspace,target_batch,target_recipe_ordinal,
            'satisfied_result',target_attempt,projection_row.id,projection_row.material_id,
            job_row.id,job_row.version
        );
        observation_kind:='satisfied_result';
    END IF;
    INSERT INTO embedding_transition_observations(
        id,workspace_id,transition_id,transition_plan_id,batch_id,attempt_id,observation_kind
    ) VALUES(
        gen_random_uuid(),target_workspace,plan_row.transition_id,target_plan,target_batch,
        target_attempt,observation_kind
    );
    RETURN 'rebuilding';
END
$$;

CREATE OR REPLACE FUNCTION vestrace_prove_embedding_transition_completeness(
    target_workspace UUID,
    target_transition UUID,
    target_plan UUID,
    target_batch UUID,
    target_expected_transition_version BIGINT
) RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE transition_row embedding_transitions%ROWTYPE;
        batch_row embedding_transition_batches%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_transition IS NULL OR target_plan IS NULL
       OR target_batch IS NULL OR target_expected_transition_version IS NULL
       OR target_expected_transition_version<1
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding transition completeness arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT * INTO transition_row FROM embedding_transitions
     WHERE workspace_id=target_workspace AND id=target_transition FOR UPDATE;
    SELECT * INTO batch_row FROM embedding_transition_batches
     WHERE workspace_id=target_workspace AND id=target_batch AND transition_id=target_transition
       AND transition_plan_id=target_plan FOR UPDATE;
    IF NOT FOUND OR transition_row.version<>target_expected_transition_version
       OR transition_row.state<>'rebuilding' OR batch_row.state<>'rebuilding' THEN
        RAISE EXCEPTION 'embedding transition completeness has a stale transition version or batch'
            USING ERRCODE='23514';
    END IF;
    -- This is the statement boundary: deferred constraint triggers allow the
    -- coordinator to assemble a batch atomically, but this public operation
    -- validates the complete graph while all headers are locked before it can
    -- expose ReadyToActivate to its caller.
    PERFORM vestrace_validate_embedding_transition_bijection(target_workspace,target_batch,TRUE);
    UPDATE embedding_transition_batches SET state='ready_to_activate'
     WHERE workspace_id=target_workspace AND id=target_batch;
    UPDATE embedding_transitions SET state='ready_to_activate',version=version+1
     WHERE workspace_id=target_workspace AND id=target_transition;
    INSERT INTO embedding_transition_observations(
        id,workspace_id,transition_id,transition_plan_id,batch_id,attempt_id,observation_kind
    ) VALUES(
        gen_random_uuid(),target_workspace,target_transition,target_plan,target_batch,NULL,'ready_to_activate'
    );
    RETURN 'ready_to_activate';
END
$$;

DO $rls$
DECLARE target TEXT;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'embedding_transition_batches',
        'embedding_transition_batch_recipes',
        'embedding_transition_recipe_dependencies',
        'embedding_transition_job_attempts',
        'embedding_transition_recipe_satisfactions',
        'embedding_transition_observations'
    ] LOOP
        EXECUTE format('ALTER TABLE public.%I ENABLE ROW LEVEL SECURITY',target);
        EXECUTE format('ALTER TABLE public.%I FORCE ROW LEVEL SECURITY',target);
        EXECUTE format(
            'CREATE POLICY %I ON public.%I USING(workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',true),'''')::UUID) WITH CHECK(workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',true),'''')::UUID)',
            target||'_workspace_policy',target
        );
        EXECUTE format('REVOKE ALL ON TABLE public.%I FROM PUBLIC,vestrace',target);
        EXECUTE format('GRANT SELECT, REFERENCES ON TABLE public.%I TO vestrace',target);
    END LOOP;
END
$rls$;

DO $upgrade$
DECLARE target REGCLASS; target_function REGPROCEDURE;
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_transition_execution_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_transition_execution_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
            RAISE EXCEPTION 'embedding transition execution ownership hand-back is unavailable'
                USING ERRCODE='42501';
        END IF;
        FOREACH target IN ARRAY ARRAY[
            'embedding_transitions'::REGCLASS,
            'embedding_transition_plan_recipes'::REGCLASS,
            'embedding_transition_ambiguity_carry_recipes'::REGCLASS,
            'embedding_transition_barrier_recipes'::REGCLASS,
            'embedding_transition_batches'::REGCLASS,
            'embedding_transition_batch_recipes'::REGCLASS,
            'embedding_transition_recipe_dependencies'::REGCLASS,
            'embedding_transition_job_attempts'::REGCLASS,
            'embedding_transition_recipe_satisfactions'::REGCLASS,
            'embedding_transition_observations'::REGCLASS
        ] LOOP
            EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner',target);
            EXECUTE format('GRANT ALL ON TABLE %s TO vestrace_guarded_owner',target);
        END LOOP;
        REVOKE ALL ON TABLE embedding_transitions,embedding_transition_plan_recipes,
            embedding_transition_ambiguity_carry_recipes,embedding_transition_barrier_recipes,
            embedding_transition_batches,embedding_transition_batch_recipes,
            embedding_transition_recipe_dependencies,embedding_transition_job_attempts,
            embedding_transition_recipe_satisfactions,embedding_transition_observations
            FROM PUBLIC,vestrace;
        GRANT SELECT, REFERENCES ON TABLE embedding_transitions,embedding_transition_plan_recipes,
            embedding_transition_ambiguity_carry_recipes,embedding_transition_barrier_recipes,
            embedding_transition_batches,embedding_transition_batch_recipes,
            embedding_transition_recipe_dependencies,embedding_transition_job_attempts,
            embedding_transition_recipe_satisfactions,embedding_transition_observations
            TO vestrace;
        FOREACH target_function IN ARRAY ARRAY[
            'vestrace_derive_embedding_transition_carry_recipe_identity()'::REGPROCEDURE,
            'vestrace_derive_embedding_transition_barrier_recipe_identity()'::REGPROCEDURE,
            'vestrace_materialize_embedding_transition_batch()'::REGPROCEDURE,
            'vestrace_materialize_embedding_transition_batch_recipe()'::REGPROCEDURE,
            'vestrace_guard_embedding_transition_header()'::REGPROCEDURE,
            'vestrace_guard_embedding_transition_batch_recipe()'::REGPROCEDURE,
            'vestrace_validate_embedding_transition_bijection(uuid,uuid,boolean)'::REGPROCEDURE,
            'vestrace_validate_embedding_transition_bijection_trigger()'::REGPROCEDURE,
            'vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint)'::REGPROCEDURE,
            'vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint)'::REGPROCEDURE,
            'vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint)'::REGPROCEDURE,
            'vestrace_guard_embedding_transition_batch_header()'::REGPROCEDURE
        ] LOOP
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner',target_function);
            EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace',target_function);
        END LOOP;
        GRANT EXECUTE ON FUNCTION vestrace_create_embedding_transition_batch_attempt(uuid,uuid,uuid,uuid,uuid,bigint,uuid,bigint,bigint) TO vestrace;
        GRANT EXECUTE ON FUNCTION vestrace_observe_embedding_transition_attempt(uuid,uuid,uuid,bigint,uuid,bigint) TO vestrace;
        GRANT EXECUTE ON FUNCTION vestrace_prove_embedding_transition_completeness(uuid,uuid,uuid,uuid,bigint) TO vestrace;
    END IF;
END
$upgrade$;
