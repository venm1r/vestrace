-- P04 Task 9: make a corpus generation a real member set before retrieval pins it.

DO $$
BEGIN
    PERFORM vestrace_prepare_p04_generation_fence_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE EXCEPTION 'P04 generation-fence ownership hand-back must be provisioned before runtime migration'
            USING ERRCODE = '42501';
    END IF;
END
$$;

ALTER TABLE embedding_corpus_generations
    DROP CONSTRAINT embedding_corpus_generations_state_check;
ALTER TABLE embedding_corpus_generations
    ADD CONSTRAINT embedding_corpus_generations_state_check
    CHECK (state IN ('building', 'ready', 'stale'));

CREATE UNIQUE INDEX embedding_corpus_generations_one_building_per_space
    ON embedding_corpus_generations (workspace_id, space_registration_id)
    WHERE state = 'building';

CREATE TABLE embedding_corpus_generation_members (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    corpus_generation_id UUID NOT NULL,
    memory_embedding_id UUID NOT NULL,
    PRIMARY KEY (workspace_id, corpus_generation_id, memory_embedding_id),
    CONSTRAINT embedding_corpus_generation_members_generation_fkey
        FOREIGN KEY (workspace_id, corpus_generation_id)
        REFERENCES embedding_corpus_generations(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_corpus_generation_members_embedding_fkey
        FOREIGN KEY (memory_embedding_id)
        REFERENCES memory_embeddings(id) ON DELETE RESTRICT
);

ALTER TABLE embedding_corpus_generation_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_corpus_generation_members FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_corpus_generation_members_workspace_policy
    ON embedding_corpus_generation_members
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_corpus_generation_members FROM PUBLIC;
CREATE TRIGGER embedding_corpus_generation_members_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_corpus_generation_members
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_corpus_generation_member()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    generation_state TEXT;
    generation_space UUID;
    embedding_space UUID;
BEGIN
    IF TG_OP = 'INSERT' THEN
        SELECT generation.state, registration.space_id, embedding.space_id
          INTO generation_state, generation_space, embedding_space
          FROM embedding_corpus_generations AS generation
          JOIN embedding_space_registrations AS registration
            ON registration.workspace_id = generation.workspace_id
           AND registration.id = generation.space_registration_id
          JOIN memory_embeddings AS embedding
            ON embedding.id = NEW.memory_embedding_id
           AND embedding.workspace_id = NEW.workspace_id
         WHERE generation.workspace_id = NEW.workspace_id
           AND generation.id = NEW.corpus_generation_id;
        IF generation_state IS NULL OR generation_state <> 'building' THEN
            RAISE EXCEPTION 'embedding corpus members may be inserted only into a building generation'
                USING ERRCODE = '23514';
        END IF;
        IF generation_space IS DISTINCT FROM embedding_space THEN
            RAISE EXCEPTION 'embedding corpus member must belong to its generation space'
                USING ERRCODE = '23514';
        END IF;
    ELSIF TG_OP = 'DELETE' THEN
        SELECT state INTO generation_state
          FROM embedding_corpus_generations
         WHERE workspace_id = OLD.workspace_id AND id = OLD.corpus_generation_id;
        IF generation_state = 'ready' THEN
            RAISE EXCEPTION 'embedding corpus members may not be removed from a ready generation'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER embedding_corpus_generation_members_consistent
    AFTER INSERT OR DELETE ON embedding_corpus_generation_members
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_corpus_generation_member();

CREATE OR REPLACE FUNCTION vestrace_open_embedding_corpus_generation(
    target_generation_id UUID,
    target_workspace_id UUID,
    target_registration_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE next_ordinal BIGINT; existing_generation_id UUID;
BEGIN
    IF target_generation_id IS NULL OR target_workspace_id IS NULL OR target_registration_id IS NULL THEN
        RAISE EXCEPTION 'embedding corpus generation arguments are malformed' USING ERRCODE = '22023';
    END IF;
    PERFORM 1 FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id AND id = target_registration_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding corpus generation requires its registered space' USING ERRCODE = '23514';
    END IF;
    SELECT id INTO existing_generation_id FROM embedding_corpus_generations
     WHERE workspace_id = target_workspace_id AND space_registration_id = target_registration_id
       AND state = 'building';
    IF FOUND THEN RETURN existing_generation_id; END IF;
    SELECT COALESCE(MAX(ordinal), 0) + 1 INTO next_ordinal FROM embedding_corpus_generations
     WHERE workspace_id = target_workspace_id AND space_registration_id = target_registration_id;
    INSERT INTO embedding_corpus_generations
        (id, workspace_id, space_registration_id, ordinal, member_count, state)
    VALUES (target_generation_id, target_workspace_id, target_registration_id, next_ordinal, 0, 'building');
    RETURN target_generation_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_enrol_embedding_corpus_generation_member(
    target_workspace_id UUID,
    target_generation_id UUID,
    target_memory_embedding_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    INSERT INTO embedding_corpus_generation_members (
        workspace_id, corpus_generation_id, memory_embedding_id
    ) VALUES (target_workspace_id, target_generation_id, target_memory_embedding_id)
    ON CONFLICT DO NOTHING;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_stale_embedding_corpus_generations_for(
    target_workspace_id UUID,
    target_memory_embedding_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    UPDATE embedding_corpus_generations AS generation
       SET state = 'stale'
      FROM embedding_corpus_generation_members AS member
     WHERE generation.workspace_id = target_workspace_id
       AND generation.id = member.corpus_generation_id
       AND member.workspace_id = target_workspace_id
       AND member.memory_embedding_id = target_memory_embedding_id
       AND generation.state = 'ready';
END
$$;

CREATE OR REPLACE FUNCTION vestrace_remove_embedding_corpus_generation_members_for(
    target_workspace_id UUID,
    target_memory_embedding_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    DELETE FROM embedding_corpus_generation_members
     WHERE workspace_id = target_workspace_id AND memory_embedding_id = target_memory_embedding_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_publish_embedding_corpus_generation(
    target_generation_id UUID,
    target_workspace_id UUID,
    target_registration_id UUID,
    target_member_count BIGINT
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE next_ordinal BIGINT; actual_member_count BIGINT;
BEGIN
    IF target_generation_id IS NULL OR target_workspace_id IS NULL OR target_registration_id IS NULL
       OR target_member_count IS NULL OR target_member_count < 0 THEN
        RAISE EXCEPTION 'embedding corpus generation arguments are malformed' USING ERRCODE = '22023';
    END IF;
    SELECT ordinal INTO next_ordinal FROM embedding_corpus_generations
     WHERE id = target_generation_id AND workspace_id = target_workspace_id
       AND space_registration_id = target_registration_id AND state = 'building' FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding corpus generation must be building before publication' USING ERRCODE = '23514';
    END IF;
    SELECT count(*) INTO actual_member_count FROM embedding_corpus_generation_members
     WHERE workspace_id = target_workspace_id AND corpus_generation_id = target_generation_id;
    UPDATE embedding_corpus_generations SET state = 'stale'
     WHERE workspace_id = target_workspace_id AND space_registration_id = target_registration_id AND state = 'ready';
    UPDATE embedding_corpus_generations SET state = 'ready', member_count = actual_member_count
     WHERE workspace_id = target_workspace_id AND id = target_generation_id;
    RETURN next_ordinal;
END
$$;

-- The validator needs identity columns only, never model-derived embedding values.
GRANT SELECT (id, workspace_id, space_id) ON memory_embeddings TO vestrace_guarded_owner;

DO $$
DECLARE target REGPROCEDURE;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        to_regprocedure('public.vestrace_open_embedding_corpus_generation(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_enrol_embedding_corpus_generation_member(UUID, UUID, UUID)'),
        to_regprocedure('public.vestrace_stale_embedding_corpus_generations_for(UUID, UUID)'),
        to_regprocedure('public.vestrace_remove_embedding_corpus_generation_members_for(UUID, UUID)')
    ]::REGPROCEDURE[] LOOP
        IF target IS NULL OR NOT has_function_privilege(current_user, target, 'EXECUTE') THEN
            RAISE EXCEPTION 'P04 Task 9 ownership helper is unavailable' USING ERRCODE = '42501';
        END IF;
        PERFORM vestrace_assign_p03_function_owner(target);
    END LOOP;
    PERFORM vestrace_prepare_p04_generation_fence_upgrade();
    PERFORM vestrace_assign_p03_function_owner(
        'public.vestrace_validate_embedding_corpus_generation_member()'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_table_owner('public.embedding_corpus_generation_members'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN RAISE; END IF;
    ALTER TABLE embedding_corpus_generation_members OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_open_embedding_corpus_generation(UUID, UUID, UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_enrol_embedding_corpus_generation_member(UUID, UUID, UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_stale_embedding_corpus_generations_for(UUID, UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_remove_embedding_corpus_generation_members_for(UUID, UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_publish_embedding_corpus_generation(UUID, UUID, UUID, BIGINT) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_validate_embedding_corpus_generation_member() OWNER TO vestrace_guarded_owner;
    GRANT SELECT ON embedding_corpus_generation_members TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_open_embedding_corpus_generation(UUID, UUID, UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_enrol_embedding_corpus_generation_member(UUID, UUID, UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_stale_embedding_corpus_generations_for(UUID, UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_remove_embedding_corpus_generation_members_for(UUID, UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_publish_embedding_corpus_generation(UUID, UUID, UUID, BIGINT) TO vestrace;
END
$$;

DO $$
DECLARE
    workspace RECORD;
    space RECORD;
    registration_id UUID;
    generation_id UUID;
    member RECORD;
    caller_can_see_all_workspaces BOOLEAN;
BEGIN
    SELECT COALESCE(rolsuper, FALSE) OR COALESCE(rolbypassrls, FALSE)
      INTO caller_can_see_all_workspaces
      FROM pg_roles
     WHERE rolname = current_user;
    IF NOT COALESCE(caller_can_see_all_workspaces, FALSE) THEN
        RAISE WARNING
            'P04 embedding corpus-generation backfill skipped: migration role % cannot see across workspace RLS; affected workspaces will refuse retrieval until an operator backfill runs',
            current_user;
        RETURN;
    END IF;
    FOR workspace IN SELECT id FROM workspaces LOOP
        PERFORM set_config('vestrace.workspace_id', workspace.id::TEXT, true);
        FOR space IN SELECT id, name, model, dimensions FROM embedding_spaces WHERE workspace_id = workspace.id LOOP
            registration_id := vestrace_register_embedding_space(gen_random_uuid(), workspace.id, space.id, space.name, space.model, space.dimensions);
            generation_id := vestrace_open_embedding_corpus_generation(gen_random_uuid(), workspace.id, registration_id);
            FOR member IN SELECT id FROM memory_embeddings WHERE workspace_id = workspace.id AND space_id = space.id LOOP
                PERFORM vestrace_enrol_embedding_corpus_generation_member(workspace.id, generation_id, member.id);
            END LOOP;
            PERFORM vestrace_publish_embedding_corpus_generation(generation_id, workspace.id, registration_id, 0);
        END LOOP;
    END LOOP;
END
$$;
