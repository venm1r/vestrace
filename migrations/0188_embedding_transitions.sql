-- P04 Task 6: immutable embedding transition-version planning.
--
-- A transition snapshot is positive-scoped.  A snapshot with no scope row is
-- unusable: the deferred invariant below makes an omitted scope fail at COMMIT,
-- while the two ordinary callers refuse it immediately.

DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_p04_embedding_transition_upgrade()') IS NOT NULL
       AND has_function_privilege(
           current_user,
           'public.vestrace_prepare_p04_embedding_transition_upgrade()',
           'EXECUTE'
       ) THEN
        PERFORM public.vestrace_prepare_p04_embedding_transition_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'P04 embedding-transition ownership hand-back must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        ALTER FUNCTION public.vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)
            OWNER TO vestrace;
        ALTER FUNCTION public.vestrace_accept_embedding_job(
            UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
        ) OWNER TO vestrace;
        ALTER FUNCTION public.vestrace_create_model_request_evidence(
            UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[]
        ) OWNER TO vestrace;
        ALTER TABLE public.model_binding_snapshots OWNER TO vestrace;
    END IF;
END
$$;

CREATE TABLE embedding_transitions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    target_space_registration_id UUID NOT NULL,
    state TEXT NOT NULL DEFAULT 'planned'
        CHECK (state IN ('planned', 'rebuilding', 'ready_to_activate', 'activated', 'stale', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_transitions_target_space_fkey
        FOREIGN KEY (workspace_id, target_space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transitions_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE embedding_transition_plans (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    source_connection_id UUID NOT NULL,
    source_connection_revision_id UUID NOT NULL,
    source_branch TEXT NOT NULL CHECK (source_branch IN ('credential', 'no_auth')),
    source_credential_revision_id UUID,
    source_no_auth_binding_revision_id UUID,
    target_connection_id UUID NOT NULL,
    target_connection_revision_id UUID NOT NULL,
    target_connection_qualification_revision_id UUID NOT NULL,
    target_model_revision_id UUID NOT NULL,
    target_model_qualification_revision_id UUID NOT NULL,
    target_branch TEXT NOT NULL CHECK (target_branch IN ('credential', 'no_auth')),
    target_credential_revision_id UUID,
    target_credential_slot_id UUID,
    target_credential_activation_guard_id UUID,
    target_expected_slot_version BIGINT CHECK (target_expected_slot_version >= 0),
    target_no_auth_binding_revision_id UUID,
    target_space_registration_id UUID NOT NULL,
    transition_batch_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_transition_plans_transition_fkey
        FOREIGN KEY (workspace_id, transition_id)
        REFERENCES embedding_transitions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_target_space_fkey
        FOREIGN KEY (workspace_id, target_space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_target_snapshot_tuple_fkey
        FOREIGN KEY (
            workspace_id, target_connection_id, target_connection_revision_id
        ) REFERENCES connection_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_target_no_auth_fkey
        FOREIGN KEY (
            workspace_id, target_connection_id, target_connection_revision_id,
            target_no_auth_binding_revision_id
        ) REFERENCES no_auth_binding_revisions(
            workspace_id, connection_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_source_no_auth_fkey
        FOREIGN KEY (
            workspace_id, source_connection_id, source_connection_revision_id,
            source_no_auth_binding_revision_id
        ) REFERENCES no_auth_binding_revisions(
            workspace_id, connection_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_source_credential_fkey
        FOREIGN KEY (workspace_id, source_credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_target_credential_fkey
        FOREIGN KEY (workspace_id, target_credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plans_source_auth_xor CHECK (
        (source_branch = 'credential'
            AND source_credential_revision_id IS NOT NULL
            AND source_no_auth_binding_revision_id IS NULL)
        OR
        (source_branch = 'no_auth'
            AND source_credential_revision_id IS NULL
            AND source_no_auth_binding_revision_id IS NOT NULL)
    ),
    CONSTRAINT embedding_transition_plans_target_auth_xor CHECK (
        (target_branch = 'credential'
            AND target_credential_revision_id IS NOT NULL
            AND target_credential_slot_id IS NOT NULL
            AND target_credential_activation_guard_id IS NOT NULL
            AND target_expected_slot_version IS NOT NULL
            AND target_no_auth_binding_revision_id IS NULL)
        OR
        (target_branch = 'no_auth'
            AND target_credential_revision_id IS NULL
            AND target_credential_slot_id IS NULL
            AND target_credential_activation_guard_id IS NULL
            AND target_expected_slot_version IS NULL
            AND target_no_auth_binding_revision_id IS NOT NULL)
    ),
    CONSTRAINT embedding_transition_plans_identity_key UNIQUE (workspace_id, transition_id, version),
    CONSTRAINT embedding_transition_plans_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE OR REPLACE FUNCTION vestrace_zero_based_contiguous_ordinals(input_values BIGINT[])
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT cardinality(input_values) > 0
       AND input_values = ARRAY(
            SELECT (index - 1)::BIGINT
              FROM generate_subscripts(input_values, 1) AS index
             ORDER BY index
       )
$$;

CREATE TABLE embedding_transition_plan_recipes (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_plan_id UUID NOT NULL,
    recipe_ordinal BIGINT NOT NULL CHECK (recipe_ordinal >= 0),
    recipe_identity UUID NOT NULL,
    input_ordinals BIGINT[] NOT NULL
        CHECK (vestrace_zero_based_contiguous_ordinals(input_ordinals)),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, transition_plan_id, recipe_ordinal),
    CONSTRAINT embedding_transition_plan_recipes_plan_fkey
        FOREIGN KEY (workspace_id, transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_plan_recipes_identity_key
        UNIQUE (workspace_id, transition_plan_id, recipe_identity)
);

CREATE TABLE model_binding_snapshot_scopes (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    snapshot_id UUID NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('ordinary', 'transition')),
    transition_plan_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, snapshot_id),
    CONSTRAINT model_binding_snapshot_scopes_snapshot_fkey
        FOREIGN KEY (workspace_id, snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshot_scopes_transition_plan_fkey
        FOREIGN KEY (workspace_id, transition_plan_id)
        REFERENCES embedding_transition_plans(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshot_scopes_transition_xor CHECK (
        (scope = 'transition' AND transition_plan_id IS NOT NULL)
        OR
        (scope = 'ordinary' AND transition_plan_id IS NULL)
    )
);

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_transition_recipe_ordinals()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_workspace_id UUID := COALESCE(NEW.workspace_id, OLD.workspace_id);
    target_plan_id UUID := COALESCE(NEW.transition_plan_id, OLD.transition_plan_id);
    ordinals BIGINT[];
BEGIN
    SELECT array_agg(recipe_ordinal ORDER BY recipe_ordinal)
      INTO ordinals
      FROM embedding_transition_plan_recipes
     WHERE workspace_id = target_workspace_id
       AND transition_plan_id = target_plan_id;
    IF NOT vestrace_zero_based_contiguous_ordinals(ordinals) THEN
        RAISE EXCEPTION 'embedding transition recipe ordinals must be contiguous from zero'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_model_binding_snapshot_scope()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_workspace_id UUID := COALESCE(NEW.workspace_id, OLD.workspace_id);
    target_snapshot_id UUID := COALESCE(NEW.id, OLD.id);
BEGIN
    IF NOT EXISTS (
        SELECT 1
          FROM model_binding_snapshot_scopes
         WHERE workspace_id = target_workspace_id
           AND snapshot_id = target_snapshot_id
    ) THEN
        RAISE EXCEPTION 'model binding snapshot requires a durable scope'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER embedding_transitions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transitions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_transition_plans_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_plans
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_transition_plan_recipes_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_transition_plan_recipes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_binding_snapshot_scopes_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON model_binding_snapshot_scopes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE embedding_transitions ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transitions FORCE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_plans FORCE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_plan_recipes ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_plan_recipes FORCE ROW LEVEL SECURITY;
ALTER TABLE model_binding_snapshot_scopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_binding_snapshot_scopes FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_transitions_workspace_policy ON embedding_transitions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY embedding_transition_plans_workspace_policy ON embedding_transition_plans
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY embedding_transition_plan_recipes_workspace_policy ON embedding_transition_plan_recipes
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_binding_snapshot_scopes_workspace_policy ON model_binding_snapshot_scopes
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_transitions FROM PUBLIC;
REVOKE ALL ON embedding_transition_plans FROM PUBLIC;
REVOKE ALL ON embedding_transition_plan_recipes FROM PUBLIC;
REVOKE ALL ON model_binding_snapshot_scopes FROM PUBLIC;

-- Existing P03 snapshots predate positive scopes.  This is intentionally before
-- the deferred trigger: a trigger first would reject its own migration commit.
INSERT INTO model_binding_snapshot_scopes (workspace_id, snapshot_id, scope, transition_plan_id)
SELECT workspace_id, id, 'ordinary', NULL
  FROM model_binding_snapshots;

CREATE CONSTRAINT TRIGGER embedding_transition_plan_recipes_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON embedding_transition_plan_recipes
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_transition_recipe_ordinals();
CREATE CONSTRAINT TRIGGER model_binding_snapshots_scope_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON model_binding_snapshots
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_model_binding_snapshot_scope();

CREATE OR REPLACE FUNCTION vestrace_snapshot_scope(
    target_workspace_id UUID,
    target_snapshot_id UUID
)
RETURNS TEXT
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT scope
      FROM model_binding_snapshot_scopes
     WHERE workspace_id = target_workspace_id
       AND snapshot_id = target_snapshot_id
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
    predecessor_recipes UUID[];
    existing embedding_transition_plans%ROWTYPE;
    recipe_index INTEGER;
    recipe_inputs BIGINT[];
BEGIN
    IF target_transition_id IS NULL OR target_plan_id IS NULL OR target_workspace_id IS NULL
       OR target_version IS NULL OR target_version < 1
       OR target_source_connection_id IS NULL OR target_source_connection_revision_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_connection_qualification_revision_id IS NULL OR target_model_revision_id IS NULL
       OR target_model_qualification_revision_id IS NULL OR target_space_registration_id IS NULL
       OR target_batch_id IS NULL OR target_snapshot_id IS NULL
       OR target_recipe_identities IS NULL
       OR target_recipe_input_ordinals IS NULL
       OR jsonb_typeof(target_recipe_input_ordinals) <> 'array'
       OR jsonb_array_length(target_recipe_input_ordinals) <> cardinality(target_recipe_identities)
       OR target_source_branch NOT IN ('credential', 'no_auth')
       OR target_branch NOT IN ('credential', 'no_auth')
       OR target_workspace_id IS DISTINCT FROM
           NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding transition planning arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF NOT (
        (target_source_branch = 'credential'
            AND target_source_credential_revision_id IS NOT NULL
            AND target_source_no_auth_binding_revision_id IS NULL)
        OR
        (target_source_branch = 'no_auth'
            AND target_source_credential_revision_id IS NULL
            AND target_source_no_auth_binding_revision_id IS NOT NULL)
    ) OR NOT (
        (target_branch = 'credential'
            AND target_credential_revision_id IS NOT NULL
            AND target_credential_slot_id IS NOT NULL
            AND target_credential_activation_guard_id IS NOT NULL
            AND target_expected_slot_version IS NOT NULL
            AND target_no_auth_binding_revision_id IS NULL)
        OR
        (target_branch = 'no_auth'
            AND target_credential_revision_id IS NULL
            AND target_credential_slot_id IS NULL
            AND target_credential_activation_guard_id IS NULL
            AND target_expected_slot_version IS NULL
            AND target_no_auth_binding_revision_id IS NOT NULL)
    ) THEN
        RAISE EXCEPTION 'embedding transition plan auth-binding branch is malformed'
            USING ERRCODE = '22023';
    END IF;

    -- Canonical ordering: ConnectionExecutionGuard, optional exact activation
    -- guard, exact registered embedding space, then transition identity.
    SELECT id INTO guard_id
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition requires its existing permanent connection guard'
            USING ERRCODE = '23514';
    END IF;
    IF target_branch = 'credential' THEN
        PERFORM 1
          FROM credential_activation_guards
         WHERE id = target_credential_activation_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = target_credential_slot_id
           AND execution_guard_id = guard_id
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding transition requires its exact credential activation guard'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    PERFORM 1
      FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id AND id = target_space_registration_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition requires its authorized embedding space'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO embedding_transitions (id, workspace_id, target_space_registration_id)
    VALUES (target_transition_id, target_workspace_id, target_space_registration_id)
    ON CONFLICT (id) DO NOTHING;
    SELECT id INTO predecessor_id
      FROM embedding_transitions
     WHERE workspace_id = target_workspace_id AND id = target_transition_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition identity belongs to another workspace'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO existing
      FROM embedding_transition_plans
     WHERE workspace_id = target_workspace_id
       AND transition_id = target_transition_id
       AND version = target_version
     FOR UPDATE;
    IF FOUND THEN
        IF existing.id = target_plan_id
           AND existing.source_connection_id = target_source_connection_id
           AND existing.source_connection_revision_id = target_source_connection_revision_id
           AND existing.source_branch = target_source_branch
           AND existing.source_credential_revision_id IS NOT DISTINCT FROM target_source_credential_revision_id
           AND existing.source_no_auth_binding_revision_id IS NOT DISTINCT FROM target_source_no_auth_binding_revision_id
           AND existing.target_connection_id = target_connection_id
           AND existing.target_connection_revision_id = target_connection_revision_id
           AND existing.target_connection_qualification_revision_id = target_connection_qualification_revision_id
           AND existing.target_model_revision_id = target_model_revision_id
           AND existing.target_model_qualification_revision_id = target_model_qualification_revision_id
           AND existing.target_branch = target_branch
           AND existing.target_credential_revision_id IS NOT DISTINCT FROM target_credential_revision_id
           AND existing.target_no_auth_binding_revision_id IS NOT DISTINCT FROM target_no_auth_binding_revision_id
           AND existing.target_space_registration_id = target_space_registration_id
           AND existing.transition_batch_id = target_batch_id
           AND (SELECT array_agg(recipe_identity ORDER BY recipe_ordinal)
                  FROM embedding_transition_plan_recipes
                 WHERE workspace_id = target_workspace_id AND transition_plan_id = existing.id)
               = target_recipe_identities THEN
            RETURN existing.id;
        END IF;
        RAISE EXCEPTION 'EMBEDDING_TRANSITION_PLAN_IDENTITY_CONFLICT'
            USING ERRCODE = '40001';
    END IF;

    IF cardinality(target_recipe_identities) = 0 THEN
        IF target_version > 1 THEN
            RAISE EXCEPTION 'embedding transition successor recipes must preserve predecessor identities and order'
                USING ERRCODE = '23514';
        END IF;
        RAISE EXCEPTION 'embedding transition plan requires at least one recipe'
            USING ERRCODE = '23514';
    END IF;

    IF target_version > 1 THEN
        SELECT id INTO predecessor_id
          FROM embedding_transition_plans
         WHERE workspace_id = target_workspace_id
           AND transition_id = target_transition_id
           AND version = target_version - 1
         FOR SHARE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding transition successor requires its exact predecessor version'
                USING ERRCODE = '23514';
        END IF;
        SELECT array_agg(recipe_identity ORDER BY recipe_ordinal)
          INTO predecessor_recipes
          FROM embedding_transition_plan_recipes
         WHERE workspace_id = target_workspace_id AND transition_plan_id = predecessor_id;
        IF predecessor_recipes IS DISTINCT FROM target_recipe_identities THEN
            RAISE EXCEPTION 'embedding transition successor recipes must preserve predecessor identities and order'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    FOR recipe_index IN 1..cardinality(target_recipe_identities) LOOP
        SELECT array_agg(value::BIGINT ORDER BY ordinal)
          INTO recipe_inputs
          FROM jsonb_array_elements_text(target_recipe_input_ordinals -> (recipe_index - 1))
               WITH ORDINALITY AS inputs(value, ordinal);
        IF NOT vestrace_zero_based_contiguous_ordinals(recipe_inputs) THEN
            RAISE EXCEPTION 'embedding transition recipe input ordinals must be contiguous from zero'
                USING ERRCODE = '23514';
        END IF;
    END LOOP;

    INSERT INTO embedding_transition_plans (
        id, workspace_id, transition_id, version,
        source_connection_id, source_connection_revision_id, source_branch,
        source_credential_revision_id, source_no_auth_binding_revision_id,
        target_connection_id, target_connection_revision_id,
        target_connection_qualification_revision_id, target_model_revision_id,
        target_model_qualification_revision_id, target_branch,
        target_credential_revision_id, target_credential_slot_id,
        target_credential_activation_guard_id, target_expected_slot_version,
        target_no_auth_binding_revision_id, target_space_registration_id, transition_batch_id
    ) VALUES (
        target_plan_id, target_workspace_id, target_transition_id, target_version,
        target_source_connection_id, target_source_connection_revision_id, target_source_branch,
        target_source_credential_revision_id, target_source_no_auth_binding_revision_id,
        target_connection_id, target_connection_revision_id,
        target_connection_qualification_revision_id, target_model_revision_id,
        target_model_qualification_revision_id, target_branch,
        target_credential_revision_id, target_credential_slot_id,
        target_credential_activation_guard_id, target_expected_slot_version,
        target_no_auth_binding_revision_id, target_space_registration_id, target_batch_id
    );
    FOR recipe_index IN 1..cardinality(target_recipe_identities) LOOP
        SELECT array_agg(value::BIGINT ORDER BY ordinal)
          INTO recipe_inputs
          FROM jsonb_array_elements_text(target_recipe_input_ordinals -> (recipe_index - 1))
               WITH ORDINALITY AS inputs(value, ordinal);
        INSERT INTO embedding_transition_plan_recipes (
            workspace_id, transition_plan_id, recipe_ordinal, recipe_identity, input_ordinals
        ) VALUES (
            target_workspace_id, target_plan_id, recipe_index - 1,
            target_recipe_identities[recipe_index], recipe_inputs
        );
    END LOOP;

    INSERT INTO model_binding_snapshots (
        id, workspace_id, connection_id, connection_revision_id,
        connection_qualification_revision_id, model_revision_id,
        model_qualification_revision_id, branch, credential_revision_id,
        credential_slot_id, credential_activation_guard_id, expected_slot_version,
        no_auth_binding_revision_id
    ) VALUES (
        target_snapshot_id, target_workspace_id, target_connection_id, target_connection_revision_id,
        target_connection_qualification_revision_id, target_model_revision_id,
        target_model_qualification_revision_id, target_branch, target_credential_revision_id,
        target_credential_slot_id, target_credential_activation_guard_id, target_expected_slot_version,
        target_no_auth_binding_revision_id
    );
    INSERT INTO model_binding_snapshot_scopes (
        workspace_id, snapshot_id, scope, transition_plan_id
    ) VALUES (
        target_workspace_id, target_snapshot_id, 'transition', target_plan_id
    );
    RETURN target_plan_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_run_model_binding_snapshot(
    target_snapshot_id UUID,
    target_workspace_id UUID,
    target_run_id UUID,
    target_purpose TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    default_row workspace_model_defaults%ROWTYPE;
    model_row model_revisions%ROWTYPE;
    connection_row connection_revisions%ROWTYPE;
    connection_qualification_row connection_qualification_revisions%ROWTYPE;
    model_qualification_row model_qualification_revisions%ROWTYPE;
    target_binding_row qualification_target_bindings%ROWTYPE;
    credential_slot_row credential_slots%ROWTYPE;
    connection_guard_id UUID;
    activation_guard_id UUID;
BEGIN
    IF target_snapshot_id IS NULL OR target_workspace_id IS NULL
       OR target_run_id IS NULL OR target_purpose IS NULL
       OR length(btrim(target_purpose)) = 0 THEN
        RAISE EXCEPTION 'model binding snapshot arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_purpose <> 'chat' THEN
        RAISE EXCEPTION 'legacy Run model binding accepts only the chat default'
            USING ERRCODE = '22023';
    END IF;

    SELECT * INTO default_row
      FROM workspace_model_defaults
     WHERE workspace_id = target_workspace_id
       AND purpose = target_purpose;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy Run acceptance has no configured chat model default'
            USING ERRCODE = '23514', CONSTRAINT = 'legacy_run_chat_default_absent';
    END IF;

    -- This lookup identifies the connection whose permanent guard must be the
    -- first lock. It is rechecked after the lock, below, before anything is
    -- persisted.
    SELECT revision.* INTO model_row
      FROM model_revision_heads AS model_head
      JOIN model_revisions AS revision
        ON revision.workspace_id = model_head.workspace_id
       AND revision.model_id = model_head.model_id
       AND revision.id = model_head.current_revision_id
     WHERE model_head.workspace_id = target_workspace_id
       AND model_head.model_id = default_row.model_id
       AND revision.kind = 'chat';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy Run chat default has no current chat model revision'
            USING ERRCODE = '23514', CONSTRAINT = 'legacy_run_chat_default_not_current_chat';
    END IF;

    SELECT * INTO connection_row
      FROM connection_revisions
     WHERE workspace_id = target_workspace_id
       AND id = model_row.connection_revision_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding references a missing connection revision'
            USING ERRCODE = '23514';
    END IF;

    -- Canonical lock order: connection execution guard first; credential
    -- activation guard second only for the credential branch; all predicates
    -- follow those locks.
    SELECT id INTO connection_guard_id
      FROM connection_execution_guards
     WHERE id = connection_row.execution_guard_id
       AND workspace_id = target_workspace_id
       AND connection_id = connection_row.connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires the exact connection execution guard'
            USING ERRCODE = '23514';
    END IF;

    -- A head advance may have waited on the guard. Resolve its current model
    -- again before taking the optional branch lock so acceptance observes a
    -- complete old tuple or a complete new tuple, never their mixture.
    SELECT * INTO default_row
      FROM workspace_model_defaults
     WHERE workspace_id = target_workspace_id
       AND purpose = target_purpose;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy Run acceptance has no configured chat model default'
            USING ERRCODE = '23514', CONSTRAINT = 'legacy_run_chat_default_absent';
    END IF;
    SELECT revision.* INTO model_row
      FROM model_revision_heads AS model_head
      JOIN model_revisions AS revision
        ON revision.workspace_id = model_head.workspace_id
       AND revision.model_id = model_head.model_id
       AND revision.id = model_head.current_revision_id
     WHERE model_head.workspace_id = target_workspace_id
       AND model_head.model_id = default_row.model_id
       AND revision.kind = 'chat';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'legacy Run chat default has no current chat model revision'
            USING ERRCODE = '23514', CONSTRAINT = 'legacy_run_chat_default_not_current_chat';
    END IF;
    SELECT * INTO connection_row
      FROM connection_revisions
     WHERE workspace_id = target_workspace_id
       AND id = model_row.connection_revision_id
       AND execution_guard_id = connection_guard_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding default changed outside its locked connection guard'
            USING ERRCODE = '23514';
    END IF;

    IF connection_row.auth_mode <> 'none' THEN
        SELECT id INTO activation_guard_id
          FROM credential_activation_guards
         WHERE workspace_id = target_workspace_id
           AND connection_id = connection_row.connection_id
           AND credential_slot_id = connection_row.credential_slot_id
           AND execution_guard_id = connection_guard_id
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'model binding requires the exact credential activation guard'
                USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
        END IF;
    END IF;

    SELECT revision.* INTO connection_row
      FROM connection_revision_heads AS connection_head
      JOIN connection_revisions AS revision
        ON revision.workspace_id = connection_head.workspace_id
       AND revision.connection_id = connection_head.connection_id
       AND revision.id = connection_head.current_revision_id
     WHERE connection_head.workspace_id = target_workspace_id
       AND connection_head.connection_id = connection_row.connection_id
       AND connection_head.current_revision_id = model_row.connection_revision_id
       AND connection_head.state = 'enabled'
       AND revision.execution_guard_id = connection_guard_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires the current enabled connection revision'
            USING ERRCODE = '23514';
    END IF;

    SELECT revision.* INTO model_row
      FROM model_revision_heads AS model_head
      JOIN model_revisions AS revision
        ON revision.workspace_id = model_head.workspace_id
       AND revision.model_id = model_head.model_id
       AND revision.id = model_head.current_revision_id
     WHERE model_head.workspace_id = target_workspace_id
       AND model_head.model_id = default_row.model_id
       AND revision.id = model_row.id
       AND revision.connection_revision_id = connection_row.id
       AND revision.kind = 'chat';
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires the current chat model revision'
            USING ERRCODE = '23514', CONSTRAINT = 'legacy_run_chat_default_not_current_chat';
    END IF;

    SELECT qualification_revision.* INTO connection_qualification_row
      FROM connection_qualification_heads AS qualification_head
      JOIN connection_qualification_revisions AS qualification_revision
        ON qualification_revision.workspace_id = qualification_head.workspace_id
       AND qualification_revision.connection_revision_id = qualification_head.connection_revision_id
       AND qualification_revision.id = qualification_head.current_qualification_revision_id
     WHERE qualification_head.workspace_id = target_workspace_id
       AND qualification_head.connection_revision_id = connection_row.id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires a current connection qualification'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;
    IF connection_qualification_row.valid_until <= NOW() THEN
        RAISE EXCEPTION 'model binding connection qualification has expired'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_expired';
    END IF;
    IF NOT connection_qualification_row.capabilities @> default_row.required_capabilities THEN
        RAISE EXCEPTION 'model binding connection qualification is incompatible'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;

    SELECT qualification_revision.* INTO model_qualification_row
      FROM model_qualification_heads AS qualification_head
      JOIN model_qualification_revisions AS qualification_revision
        ON qualification_revision.workspace_id = qualification_head.workspace_id
       AND qualification_revision.model_revision_id = qualification_head.model_revision_id
       AND qualification_revision.id = qualification_head.current_qualification_revision_id
     WHERE qualification_head.workspace_id = target_workspace_id
       AND qualification_head.model_revision_id = model_row.id
       AND qualification_revision.connection_revision_id = connection_row.id
       AND qualification_revision.connection_qualification_revision_id = connection_qualification_row.id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires a current compatible model qualification'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;
    IF model_qualification_row.valid_until <= NOW() THEN
        RAISE EXCEPTION 'model binding model qualification has expired'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_expired';
    END IF;
    IF NOT model_qualification_row.capabilities @> default_row.required_capabilities THEN
        RAISE EXCEPTION 'model binding model qualification is incompatible'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_qualification_incompatible';
    END IF;

    SELECT binding.* INTO target_binding_row
      FROM qualification_target_bindings AS binding
     WHERE binding.workspace_id = target_workspace_id
       AND binding.qualification_job_id = connection_qualification_row.qualification_job_id
       AND binding.connection_id = connection_row.connection_id
       AND binding.connection_revision_id = connection_row.id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'model binding requires an exact qualification target branch'
            USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
    END IF;

    IF connection_row.auth_mode = 'none' THEN
        IF target_binding_row.branch <> 'no_auth'
           OR target_binding_row.credential_revision_id IS NOT NULL
           OR target_binding_row.credential_slot_id IS NOT NULL
           OR target_binding_row.credential_activation_guard_id IS NOT NULL
           OR target_binding_row.expected_slot_version IS NOT NULL
           OR target_binding_row.no_auth_binding_revision_id IS NULL
           OR NOT EXISTS (
                SELECT 1 FROM no_auth_binding_revisions AS no_auth
                 WHERE no_auth.workspace_id = target_workspace_id
                   AND no_auth.connection_id = connection_row.connection_id
                   AND no_auth.connection_revision_id = connection_row.id
                   AND no_auth.id = target_binding_row.no_auth_binding_revision_id
           ) THEN
            RAISE EXCEPTION 'no-auth model binding must carry no credential reference'
                USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
        END IF;

        INSERT INTO model_binding_snapshots (
            id, workspace_id, connection_id, connection_revision_id,
            connection_qualification_revision_id, model_revision_id,
            model_qualification_revision_id, branch, no_auth_binding_revision_id
        ) VALUES (
            target_snapshot_id, target_workspace_id, connection_row.connection_id,
            connection_row.id, connection_qualification_row.id, model_row.id,
            model_qualification_row.id, 'no_auth', target_binding_row.no_auth_binding_revision_id
        );
    ELSE
        SELECT * INTO credential_slot_row
          FROM credential_slots
         WHERE workspace_id = target_workspace_id
           AND connection_id = connection_row.connection_id
           AND id = connection_row.credential_slot_id;
        IF NOT FOUND
           OR credential_slot_row.current_revision_id IS NULL
           OR credential_slot_row.tombstone_version IS NOT NULL
           OR target_binding_row.branch <> 'credential'
           OR target_binding_row.credential_revision_id IS DISTINCT FROM credential_slot_row.current_revision_id
           OR target_binding_row.credential_slot_id IS DISTINCT FROM credential_slot_row.id
           OR target_binding_row.credential_activation_guard_id IS DISTINCT FROM activation_guard_id
           OR target_binding_row.expected_slot_version IS DISTINCT FROM credential_slot_row.current_revision_version
           OR target_binding_row.no_auth_binding_revision_id IS NOT NULL
           OR NOT EXISTS (
                SELECT 1 FROM credential_key_creation_intents AS intent
                 WHERE intent.workspace_id = target_workspace_id
                   AND intent.connection_id = connection_row.connection_id
                   AND intent.credential_slot_id = credential_slot_row.id
                   AND intent.credential_revision_id = credential_slot_row.current_revision_id
                   AND intent.state = 'active'
           ) THEN
            RAISE EXCEPTION 'credential model binding requires the exact active guarded credential'
                USING ERRCODE = '23514', CONSTRAINT = 'model_binding_auth_branch_refused';
        END IF;

        INSERT INTO model_binding_snapshots (
            id, workspace_id, connection_id, connection_revision_id,
            connection_qualification_revision_id, model_revision_id,
            model_qualification_revision_id, branch, credential_revision_id,
            credential_slot_id, credential_activation_guard_id, expected_slot_version
        ) VALUES (
            target_snapshot_id, target_workspace_id, connection_row.connection_id,
            connection_row.id, connection_qualification_row.id, model_row.id,
            model_qualification_row.id, 'credential', credential_slot_row.current_revision_id,
            credential_slot_row.id, activation_guard_id,
            credential_slot_row.current_revision_version
        );
    END IF;

    INSERT INTO model_binding_snapshot_scopes (
        workspace_id, snapshot_id, scope, transition_plan_id
    ) VALUES (
        target_workspace_id, target_snapshot_id, 'ordinary', NULL
    );

    INSERT INTO run_model_binding_snapshots (workspace_id, run_id, snapshot_id)
    VALUES (target_workspace_id, target_run_id, target_snapshot_id);
    RETURN target_snapshot_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_accept_embedding_job(
    target_job_id UUID,
    target_workspace_id UUID,
    target_space_registration_id UUID,
    target_kind TEXT,
    target_snapshot_id UUID,
    target_external_effect_id UUID,
    target_model_request_evidence_id UUID,
    target_retries_unknown_embedding_job_id UUID,
    target_expected_predecessor_version BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    snapshot RECORD;
    guard_id UUID;
    existing embedding_jobs%ROWTYPE;
BEGIN
    IF target_job_id IS NULL OR target_workspace_id IS NULL
       OR target_space_registration_id IS NULL OR target_kind IS NULL
       OR target_snapshot_id IS NULL OR target_external_effect_id IS NULL
       OR target_model_request_evidence_id IS NULL
       OR target_kind NOT IN ('retrieval_query', 'delivery', 'rebuild')
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding job acceptance arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    -- The pinned snapshot names the Connection. Nothing else may.
    SELECT id, connection_id, branch, credential_slot_id,
           credential_activation_guard_id
      INTO snapshot
      FROM model_binding_snapshots
     WHERE workspace_id = target_workspace_id AND id = target_snapshot_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job requires its exact pinned binding snapshot'
            USING ERRCODE = '23514';
    END IF;

    IF vestrace_snapshot_scope(target_workspace_id, target_snapshot_id) IS DISTINCT FROM 'ordinary' THEN
        RAISE EXCEPTION 'embedding job requires an ordinary binding snapshot'
            USING ERRCODE = '23514';
    END IF;

    -- 1. ConnectionExecutionGuard.
    SELECT id INTO guard_id
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = snapshot.connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job requires its existing permanent connection guard'
            USING ERRCODE = '23514';
    END IF;

    -- 2. The optional exact CredentialActivationGuard. Line 219: the snapshot
    -- "structurally pins exactly one auth branch ... never both or neither", so
    -- a credential branch missing its slot or guard is a refusal rather than a
    -- quiet fall through to the no-auth path.
    IF snapshot.branch = 'credential' THEN
        IF snapshot.credential_slot_id IS NULL
           OR snapshot.credential_activation_guard_id IS NULL THEN
            RAISE EXCEPTION 'embedding job credential branch is not exact'
                USING ERRCODE = '23514';
        END IF;
        PERFORM 1
          FROM credential_activation_guards
         WHERE workspace_id = target_workspace_id
           AND connection_id = snapshot.connection_id
           AND credential_slot_id = snapshot.credential_slot_id
           AND id = snapshot.credential_activation_guard_id
           AND execution_guard_id = guard_id
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job credential branch requires its exact activation guard'
                USING ERRCODE = '23514';
        END IF;
    ELSIF snapshot.credential_slot_id IS NOT NULL
          OR snapshot.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding job no-auth branch cannot reference a credential'
            USING ERRCODE = '23514';
    END IF;

    -- 3. The exact (workspace, EmbeddingSpaceKey) guard, which is the
    -- registration row: the key is unique on the whole tuple, so locking the
    -- registration locks the key.
    PERFORM 1 FROM embedding_space_registrations
     WHERE workspace_id = target_workspace_id AND id = target_space_registration_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job requires its authorized embedding space'
            USING ERRCODE = '23514';
    END IF;

    -- 4. The job itself. Convergence is by identity: a replay that names the
    -- same job with the same tuple gets the same row, and one that names it with
    -- a different tuple is a conflict rather than a silent second acceptance.
    SELECT * INTO existing FROM embedding_jobs
     WHERE workspace_id = target_workspace_id AND id = target_job_id
     FOR UPDATE;
    IF FOUND THEN
        IF existing.space_registration_id = target_space_registration_id
           AND existing.kind = target_kind
           AND existing.model_binding_snapshot_id = target_snapshot_id
           AND existing.external_effect_id = target_external_effect_id
           AND existing.model_request_evidence_id = target_model_request_evidence_id
           AND existing.retries_unknown_embedding_job_id
               IS NOT DISTINCT FROM target_retries_unknown_embedding_job_id
           AND (
                target_retries_unknown_embedding_job_id IS NULL
                OR EXISTS (
                    SELECT 1 FROM embedding_jobs AS predecessor
                     WHERE predecessor.workspace_id = target_workspace_id
                       AND predecessor.id = target_retries_unknown_embedding_job_id
                       AND predecessor.version = target_expected_predecessor_version
                )
           ) THEN
            RETURN existing.id;
        END IF;
        RAISE EXCEPTION 'embedding job identity conflicts with its existing tuple'
            USING ERRCODE = '23514';
    END IF;

    -- Line 257: a successor may only retry a terminal InconclusiveUnknown
    -- predecessor. Checked here rather than by the caller, because the caller
    -- that wanted to retry a running job is exactly the caller that would skip
    -- the check.
    IF target_retries_unknown_embedding_job_id IS NOT NULL THEN
        IF target_expected_predecessor_version IS NULL THEN
            RAISE EXCEPTION 'embedding job successor must name the predecessor version it read'
                USING ERRCODE = '22023';
        END IF;
        PERFORM 1 FROM embedding_jobs
         WHERE workspace_id = target_workspace_id
           AND id = target_retries_unknown_embedding_job_id
           AND kind = target_kind
           AND state = 'inconclusive_unknown'
           AND version = target_expected_predecessor_version
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job successor requires a terminal inconclusive predecessor at the version it read'
                USING ERRCODE = '23514';
        END IF;
    ELSIF target_expected_predecessor_version IS NOT NULL THEN
        RAISE EXCEPTION 'embedding job without a predecessor cannot name a predecessor version'
            USING ERRCODE = '22023';
    END IF;

    INSERT INTO embedding_jobs (
        id, workspace_id, space_registration_id, kind, model_binding_snapshot_id,
        external_effect_id, model_request_evidence_id,
        retries_unknown_embedding_job_id
    ) VALUES (
        target_job_id, target_workspace_id, target_space_registration_id, target_kind,
        target_snapshot_id, target_external_effect_id, target_model_request_evidence_id,
        target_retries_unknown_embedding_job_id
    );
    RETURN target_job_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_model_request_evidence(
    target_root_id UUID,
    target_workspace_id UUID,
    target_external_effect_id UUID,
    target_request_kind TEXT,
    target_binding_snapshot_id UUID,
    target_qualification_binding_id UUID,
    target_cause_kind TEXT,
    target_cause_id UUID,
    node_kinds TEXT[],
    node_ids UUID[],
    node_versions BIGINT[],
    node_safe_ordinals TEXT[]
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_root model_request_evidence_roots%ROWTYPE;
    node_count INTEGER;
    node_index INTEGER;
    snapshot_row model_binding_snapshots%ROWTYPE;
    target_row qualification_target_bindings%ROWTYPE;
    model_kind TEXT;
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model request evidence workspace mismatch' USING ERRCODE = '42501';
    END IF;
    node_count := cardinality(node_kinds);
    IF node_count IS NULL OR node_count = 0 OR node_count > 8200
       OR cardinality(node_ids) <> node_count
       OR cardinality(node_versions) <> node_count
       OR cardinality(node_safe_ordinals) <> node_count THEN
        RAISE EXCEPTION 'model request evidence node arrays are malformed' USING ERRCODE = '22023';
    END IF;
    IF target_request_kind NOT IN ('models_list', 'chat_completions', 'embeddings')
       OR target_cause_kind NOT IN ('run_step', 'qualification_probe', 'embedding_job') THEN
        RAISE EXCEPTION 'model request evidence kind is malformed' USING ERRCODE = '22023';
    END IF;

    IF target_cause_kind IN ('run_step', 'embedding_job')
       AND vestrace_snapshot_scope(target_workspace_id, target_binding_snapshot_id) IS DISTINCT FROM 'ordinary' THEN
        RAISE EXCEPTION 'model request evidence requires an ordinary binding snapshot'
            USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended(target_external_effect_id::TEXT, 0));
    SELECT * INTO existing_root FROM model_request_evidence_roots
     WHERE external_effect_id = target_external_effect_id;
    IF FOUND THEN
        IF existing_root.workspace_id = target_workspace_id
           AND existing_root.request_kind = target_request_kind
           AND existing_root.binding_snapshot_id IS NOT DISTINCT FROM target_binding_snapshot_id
           AND existing_root.qualification_target_binding_id IS NOT DISTINCT FROM target_qualification_binding_id
           AND existing_root.cause_kind = target_cause_kind
           AND existing_root.cause_id = target_cause_id
           AND (SELECT array_agg(reference_kind ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) = node_kinds
           AND (SELECT array_agg(reference_id ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) = node_ids
           AND (SELECT array_agg(reference_version ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) IS NOT DISTINCT FROM node_versions
           AND (SELECT array_agg(safe_ordinal ORDER BY ordinal) FROM model_request_evidence_nodes WHERE evidence_root_id = existing_root.id) IS NOT DISTINCT FROM node_safe_ordinals THEN
            RETURN existing_root.id;
        END IF;
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    IF EXISTS (SELECT 1 FROM model_request_evidence_roots WHERE id = target_root_id) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM external_effect_intents
         WHERE id = target_external_effect_id AND workspace_id = target_workspace_id
    ) THEN
        RAISE EXCEPTION 'model request evidence effect is absent or cross-workspace' USING ERRCODE = '23514';
    END IF;
    IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'external_effect') <> 1
       OR node_ids[array_position(node_kinds, 'external_effect')] <> target_external_effect_id
       OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'connection_revision') <> 1
       OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'request_shape_revision') <> 1
       OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'limits_revision') <> 1 THEN
        RAISE EXCEPTION 'model request evidence is missing an exact root node' USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM connection_revisions
         WHERE id = node_ids[array_position(node_kinds, 'connection_revision')]
           AND workspace_id = target_workspace_id
    ) THEN
        RAISE EXCEPTION 'model request evidence connection revision is absent or cross-workspace' USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM model_request_shape_revisions
         WHERE id = node_ids[array_position(node_kinds, 'request_shape_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'request_shape_revision')]
           AND request_kind = target_request_kind
    ) OR NOT EXISTS (
        SELECT 1 FROM model_limits_revisions
         WHERE id = node_ids[array_position(node_kinds, 'limits_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'limits_revision')]
    ) THEN
        RAISE EXCEPTION 'model request evidence canonical source version is absent or wrong' USING ERRCODE = '23514';
    END IF;
    IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') >
       (SELECT max_inputs FROM model_limits_revisions
         WHERE id = node_ids[array_position(node_kinds, 'limits_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'limits_revision')]) THEN
        RAISE EXCEPTION 'model request evidence input count exceeds its pinned limit' USING ERRCODE = '23514';
    END IF;

    IF target_request_kind = 'models_list' THEN
        IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN (
            'model_revision', 'sampling_revision', 'tool_schema_revision', 'governed_input_material'
        )) <> 0 THEN
            RAISE EXCEPTION 'models-list evidence contains a forbidden node' USING ERRCODE = '23514';
        END IF;
    ELSIF target_request_kind = 'chat_completions' THEN
        IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'sampling_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') < 1 THEN
            RAISE EXCEPTION 'chat evidence node matrix is incomplete' USING ERRCODE = '23514';
        END IF;
    ELSE
        IF (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') < 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN ('sampling_revision', 'tool_schema_revision')) <> 0 THEN
            RAISE EXCEPTION 'embeddings evidence node matrix is incomplete or contains a forbidden node' USING ERRCODE = '23514';
        END IF;
    END IF;
    IF target_request_kind = 'chat_completions' AND (
        SELECT cardinality(input_roles)
          FROM model_request_shape_revisions
         WHERE id = node_ids[array_position(node_kinds, 'request_shape_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'request_shape_revision')]
    ) <> (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'governed_input_material') THEN
        RAISE EXCEPTION 'chat evidence roles do not match its ordered governed inputs'
            USING ERRCODE = '23514';
    END IF;

    IF target_request_kind <> 'models_list' THEN
        SELECT kind INTO model_kind FROM model_revisions
         WHERE id = node_ids[array_position(node_kinds, 'model_revision')]
           AND workspace_id = target_workspace_id
           AND connection_revision_id = node_ids[array_position(node_kinds, 'connection_revision')];
        IF model_kind IS NULL
           OR (target_request_kind = 'chat_completions' AND model_kind <> 'chat')
           OR (target_request_kind = 'embeddings' AND model_kind <> 'embedding') THEN
            RAISE EXCEPTION 'model request evidence model kind is incompatible' USING ERRCODE = '23514';
        END IF;
    END IF;
    IF target_request_kind = 'chat_completions' AND NOT EXISTS (
        SELECT 1 FROM model_sampling_revisions
         WHERE id = node_ids[array_position(node_kinds, 'sampling_revision')]
           AND workspace_id = target_workspace_id
           AND version = node_versions[array_position(node_kinds, 'sampling_revision')]
    ) THEN
        RAISE EXCEPTION 'model request evidence sampling version is absent or wrong' USING ERRCODE = '23514';
    END IF;
    FOR node_index IN 1..node_count LOOP
        IF node_kinds[node_index] IN (
            'request_shape_revision', 'sampling_revision', 'limits_revision', 'tool_schema_revision'
        ) AND (node_versions[node_index] IS NULL OR node_versions[node_index] < 1) THEN
            RAISE EXCEPTION 'model request evidence source node lacks an exact version' USING ERRCODE = '23514';
        ELSIF node_kinds[node_index] NOT IN (
            'request_shape_revision', 'sampling_revision', 'limits_revision', 'tool_schema_revision'
        ) AND node_versions[node_index] IS NOT NULL THEN
            RAISE EXCEPTION 'model request evidence nonversioned node carries a version' USING ERRCODE = '23514';
        END IF;
        IF node_kinds[node_index] IN ('governed_input_material', 'tool_schema_revision') THEN
            IF node_safe_ordinals[node_index] IS NULL
               OR node_safe_ordinals[node_index] !~ '^(0|[1-9][0-9]*)$'
               OR node_safe_ordinals[node_index]::INTEGER <> (
                   SELECT count(*) - 1 FROM generate_subscripts(node_kinds, 1) AS prior
                    WHERE prior <= node_index AND node_kinds[prior] = node_kinds[node_index]
               ) THEN
                RAISE EXCEPTION 'model request evidence ordered node ordinal is non-contiguous' USING ERRCODE = '23514';
            END IF;
        ELSIF node_kinds[node_index] <> 'qualification_probe'
              AND node_safe_ordinals[node_index] IS NOT NULL THEN
            RAISE EXCEPTION 'model request evidence unordered node carries a safe ordinal' USING ERRCODE = '23514';
        END IF;
        IF node_kinds[node_index] = 'tool_schema_revision' AND NOT EXISTS (
            SELECT 1 FROM model_tool_schema_revisions
             WHERE id = node_ids[node_index] AND workspace_id = target_workspace_id
               AND version = node_versions[node_index]
        ) THEN
            RAISE EXCEPTION 'model request evidence tool version is absent or wrong' USING ERRCODE = '23514';
        ELSIF node_kinds[node_index] = 'governed_input_material' AND NOT EXISTS (
            SELECT 1 FROM content_materials
             WHERE id = node_ids[node_index] AND workspace_id = target_workspace_id
        ) THEN
            RAISE EXCEPTION 'model request evidence input is absent or cross-workspace' USING ERRCODE = '23514';
        END IF;
    END LOOP;

    IF target_cause_kind IN ('run_step', 'embedding_job') THEN
        IF target_request_kind = 'models_list'
           OR target_binding_snapshot_id IS NULL OR target_qualification_binding_id IS NOT NULL
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'binding_snapshot') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'connection_qualification_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_qualification_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN ('qualification_target', 'qualification_probe')) <> 0 THEN
            RAISE EXCEPTION 'snapshot-rooted evidence cause matrix is invalid'
                USING ERRCODE = '23514';
        END IF;
        SELECT * INTO snapshot_row FROM model_binding_snapshots
         WHERE id = target_binding_snapshot_id AND workspace_id = target_workspace_id;
        IF NOT FOUND
           OR snapshot_row.connection_revision_id <> node_ids[array_position(node_kinds, 'connection_revision')]
           OR snapshot_row.connection_qualification_revision_id <> node_ids[array_position(node_kinds, 'connection_qualification_revision')]
           OR snapshot_row.model_revision_id <> node_ids[array_position(node_kinds, 'model_revision')]
           OR snapshot_row.model_qualification_revision_id <> node_ids[array_position(node_kinds, 'model_qualification_revision')]
           OR node_ids[array_position(node_kinds, 'binding_snapshot')] <> target_binding_snapshot_id THEN
            RAISE EXCEPTION 'snapshot-rooted evidence tuple is incompatible'
                USING ERRCODE = '23514';
        END IF;
        -- The Run branch proves its cause is a step of the named run. The
        -- embedding branch proves its cause is the job that owns this exact
        -- effect and pinned this exact snapshot, which is the same claim for a
        -- non-Run owner.
        IF target_cause_kind = 'embedding_job' AND NOT EXISTS (
            SELECT 1 FROM embedding_jobs AS job
             WHERE job.workspace_id = target_workspace_id
               AND job.id = target_cause_id
               AND job.external_effect_id = target_external_effect_id
               AND job.model_binding_snapshot_id = target_binding_snapshot_id
        ) THEN
            RAISE EXCEPTION 'embedding-job evidence tuple is incompatible'
                USING ERRCODE = '23514';
        END IF;
    ELSE
        IF target_binding_snapshot_id IS NOT NULL OR target_qualification_binding_id IS NULL
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'qualification_target') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'qualification_probe') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN (
               'binding_snapshot', 'connection_qualification_revision', 'model_qualification_revision'
           )) <> 0 THEN
            RAISE EXCEPTION 'qualification-probe evidence cause matrix is invalid' USING ERRCODE = '23514';
        END IF;
        SELECT * INTO target_row FROM qualification_target_bindings
         WHERE id = target_qualification_binding_id AND workspace_id = target_workspace_id;
        IF NOT FOUND
           OR target_row.connection_revision_id <> node_ids[array_position(node_kinds, 'connection_revision')]
           OR target_row.qualification_job_id <> target_cause_id
           OR node_ids[array_position(node_kinds, 'qualification_target')] <> target_qualification_binding_id
           OR node_ids[array_position(node_kinds, 'qualification_probe')] <> target_cause_id
           OR node_safe_ordinals[array_position(node_kinds, 'qualification_probe')] NOT IN (
               '00','10','15','20','30','35','40','50','60','70','80','90'
           ) THEN
            RAISE EXCEPTION 'qualification-probe evidence tuple is incompatible' USING ERRCODE = '23514';
        END IF;
    END IF;

    IF EXISTS (
        SELECT 1 FROM generate_subscripts(node_kinds, 1) AS left_index
        JOIN generate_subscripts(node_kinds, 1) AS right_index
          ON left_index < right_index
         AND node_kinds[left_index] = node_kinds[right_index]
         AND node_ids[left_index] = node_ids[right_index]
    ) THEN
        RAISE EXCEPTION 'model request evidence contains a duplicate node' USING ERRCODE = '23514';
    END IF;

    INSERT INTO model_request_evidence_roots (
        id, workspace_id, external_effect_id, request_kind, binding_snapshot_id,
        qualification_target_binding_id, cause_kind, cause_id
    ) VALUES (
        target_root_id, target_workspace_id, target_external_effect_id, target_request_kind,
        target_binding_snapshot_id, target_qualification_binding_id, target_cause_kind, target_cause_id
    );
    FOR node_index IN 1..node_count LOOP
        INSERT INTO model_request_evidence_nodes (
            id, workspace_id, evidence_root_id, ordinal, reference_kind,
            reference_id, reference_version, safe_ordinal
        ) VALUES (
            gen_random_uuid(), target_workspace_id, target_root_id, node_index - 1,
            node_kinds[node_index], node_ids[node_index], node_versions[node_index],
            node_safe_ordinals[node_index]
        );
    END LOOP;
    RETURN target_root_id;
END
$$;

DO $$
DECLARE
    target_table TEXT;
BEGIN
    FOREACH target_table IN ARRAY ARRAY[
        'embedding_transitions',
        'embedding_transition_plans',
        'embedding_transition_plan_recipes',
        'model_binding_snapshot_scopes'
    ]::TEXT[] LOOP
        BEGIN
            PERFORM vestrace_assign_p03_table_owner(format('public.%I', target_table)::REGCLASS);
        EXCEPTION WHEN insufficient_privilege THEN
            IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
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

SELECT vestrace_assign_p03_table_owner('model_binding_snapshots'::REGCLASS);

DO $$
BEGIN
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_plan_embedding_transition_version(UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID, UUID[], JSONB)'::REGPROCEDURE
    );
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER FUNCTION vestrace_plan_embedding_transition_version(
        UUID, UUID, UUID, BIGINT, UUID, UUID, TEXT, UUID, UUID, UUID, UUID,
        UUID, UUID, UUID, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID,
        UUID, UUID[], JSONB
    ) OWNER TO vestrace_guarded_owner;
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
END
$$;
DO $$
BEGIN
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_accept_embedding_job(UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])'::REGPROCEDURE
    );
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER FUNCTION vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)
        OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_accept_embedding_job(
        UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_create_model_request_evidence(
        UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[]
    ) OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON FUNCTION vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)
        FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_accept_embedding_job(
        UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
    ) FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_create_model_request_evidence(
        UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[]
    ) FROM PUBLIC;
    GRANT EXECUTE ON FUNCTION vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)
        TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_accept_embedding_job(
        UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, BIGINT
    ) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_create_model_request_evidence(
        UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[]
    ) TO vestrace;
END
$$;
REVOKE ALL ON FUNCTION vestrace_zero_based_contiguous_ordinals(BIGINT[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_validate_embedding_transition_recipe_ordinals() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_validate_model_binding_snapshot_scope() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_snapshot_scope(UUID, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_zero_based_contiguous_ordinals(BIGINT[])
    TO vestrace_guarded_owner;
GRANT EXECUTE ON FUNCTION vestrace_validate_embedding_transition_recipe_ordinals()
    TO vestrace_guarded_owner;
GRANT EXECUTE ON FUNCTION vestrace_validate_model_binding_snapshot_scope()
    TO vestrace_guarded_owner;
GRANT EXECUTE ON FUNCTION vestrace_snapshot_scope(UUID, UUID)
    TO vestrace_guarded_owner;
