DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'public.models'::regclass
          AND conname = 'models_workspace_id_id_key'
    ) THEN
        ALTER TABLE models ADD CONSTRAINT models_workspace_id_id_key UNIQUE (workspace_id, id);
    END IF;
END
$$;

CREATE TABLE model_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    model_id UUID NOT NULL,
    connection_revision_id UUID NOT NULL,
    wire_model_id TEXT NOT NULL CHECK (length(btrim(wire_model_id)) > 0),
    kind TEXT NOT NULL CHECK (kind IN ('chat', 'embedding')),
    observed_context_window INTEGER CHECK (observed_context_window > 0),
    context_observation_qualification_revision_id UUID,
    context_observation_source TEXT CHECK (context_observation_source IN ('provider', 'operator')),
    observed_embedding_dimension INTEGER CHECK (observed_embedding_dimension > 0),
    embedding_observation_qualification_revision_id UUID,
    embedding_observation_source TEXT CHECK (embedding_observation_source IN ('provider', 'operator')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_revisions_model_fkey
        FOREIGN KEY (workspace_id, model_id)
        REFERENCES models(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_revisions_connection_fkey
        FOREIGN KEY (workspace_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_revisions_context_observation_complete CHECK (
        (observed_context_window IS NULL
            AND context_observation_qualification_revision_id IS NULL
            AND context_observation_source IS NULL)
        OR
        (observed_context_window IS NOT NULL
            AND context_observation_qualification_revision_id IS NOT NULL
            AND context_observation_source IS NOT NULL)
    ),
    CONSTRAINT model_revisions_embedding_observation_complete CHECK (
        (observed_embedding_dimension IS NULL
            AND embedding_observation_qualification_revision_id IS NULL
            AND embedding_observation_source IS NULL)
        OR
        (observed_embedding_dimension IS NOT NULL
            AND embedding_observation_qualification_revision_id IS NOT NULL
            AND embedding_observation_source IS NOT NULL)
    ),
    CONSTRAINT model_revisions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT model_revisions_workspace_model_id_key UNIQUE (workspace_id, model_id, id)
    ,CONSTRAINT model_revisions_exact_connection_key
        UNIQUE (workspace_id, id, connection_revision_id)
);

CREATE TABLE model_revision_heads (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    model_id UUID NOT NULL,
    current_revision_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, model_id),
    CONSTRAINT model_revision_heads_model_fkey
        FOREIGN KEY (workspace_id, model_id)
        REFERENCES models(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_revision_heads_revision_fkey
        FOREIGN KEY (workspace_id, model_id, current_revision_id)
        REFERENCES model_revisions(workspace_id, model_id, id) ON DELETE RESTRICT
);

CREATE TABLE model_qualification_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    model_revision_id UUID NOT NULL,
    connection_revision_id UUID NOT NULL,
    connection_qualification_revision_id UUID NOT NULL,
    qualification_job_id UUID NOT NULL,
    capabilities TEXT[] NOT NULL CHECK (cardinality(capabilities) > 0),
    valid_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_qualification_revisions_model_revision_fkey
        FOREIGN KEY (workspace_id, model_revision_id, connection_revision_id)
        REFERENCES model_revisions(workspace_id, id, connection_revision_id) ON DELETE RESTRICT,
    CONSTRAINT model_qualification_revisions_connection_qualification_fkey
        FOREIGN KEY (
            workspace_id, connection_revision_id, connection_qualification_revision_id
        ) REFERENCES connection_qualification_revisions(
            workspace_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT model_qualification_revisions_exact_tuple_fkey
        FOREIGN KEY (
            workspace_id, connection_revision_id, qualification_job_id,
            connection_qualification_revision_id
        ) REFERENCES connection_qualification_revisions(
            workspace_id, connection_revision_id, qualification_job_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT model_qualification_revisions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT model_qualification_revisions_identity_key
        UNIQUE (workspace_id, model_revision_id, id),
    CONSTRAINT model_qualification_revisions_exact_binding_key
        UNIQUE (
            workspace_id, model_revision_id, connection_qualification_revision_id, id
        )
);

ALTER TABLE model_revisions
    ADD CONSTRAINT model_revisions_context_observation_qualification_fkey
        FOREIGN KEY (workspace_id, context_observation_qualification_revision_id)
        REFERENCES model_qualification_revisions(workspace_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT model_revisions_embedding_observation_qualification_fkey
        FOREIGN KEY (workspace_id, embedding_observation_qualification_revision_id)
        REFERENCES model_qualification_revisions(workspace_id, id) ON DELETE RESTRICT;

CREATE OR REPLACE FUNCTION vestrace_create_model_revision_and_advance_head(
    target_revision_id UUID,
    target_workspace_id UUID,
    target_model_id UUID,
    target_connection_id UUID,
    target_execution_guard_id UUID,
    target_connection_revision_id UUID,
    target_wire_model_id TEXT,
    target_kind TEXT,
    target_observed_context_window INTEGER,
    target_context_observation_qualification_revision_id UUID,
    target_context_observation_source TEXT,
    target_observed_embedding_dimension INTEGER,
    target_embedding_observation_qualification_revision_id UUID,
    target_embedding_observation_source TEXT,
    target_expected_head_version BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    current_head_version BIGINT;
BEGIN
    IF target_expected_head_version IS NULL OR target_expected_head_version < 0
       OR target_revision_id IS NULL OR target_workspace_id IS NULL
       OR target_model_id IS NULL OR target_connection_id IS NULL
       OR target_execution_guard_id IS NULL OR target_connection_revision_id IS NULL
       OR target_wire_model_id IS NULL OR length(btrim(target_wire_model_id)) = 0
       OR target_kind IS NULL THEN
        RAISE EXCEPTION 'model revision arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    IF NOT EXISTS (
        SELECT 1
          FROM connection_revisions AS revision
          JOIN connection_execution_guards AS guard
            ON guard.id = revision.execution_guard_id
           AND guard.workspace_id = revision.workspace_id
           AND guard.connection_id = revision.connection_id
         WHERE revision.workspace_id = target_workspace_id
           AND revision.connection_id = target_connection_id
           AND revision.id = target_connection_revision_id
           AND revision.execution_guard_id = target_execution_guard_id
    ) THEN
        RAISE EXCEPTION 'model revision requires its exact referenced connection guard'
            USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended(target_workspace_id::TEXT || ':' || target_model_id::TEXT, 0)
    );

    SELECT version
      INTO current_head_version
      FROM model_revision_heads
     WHERE workspace_id = target_workspace_id
       AND model_id = target_model_id
     FOR UPDATE;

    IF NOT FOUND THEN
        IF target_expected_head_version <> 0 THEN
            RAISE EXCEPTION 'model revision head version conflict'
                USING ERRCODE = '40001';
        END IF;

        INSERT INTO model_revisions (
            id, workspace_id, model_id, connection_revision_id, wire_model_id, kind,
            observed_context_window, context_observation_qualification_revision_id,
            context_observation_source, observed_embedding_dimension,
            embedding_observation_qualification_revision_id, embedding_observation_source
        ) VALUES (
            target_revision_id, target_workspace_id, target_model_id,
            target_connection_revision_id, target_wire_model_id, target_kind,
            target_observed_context_window, target_context_observation_qualification_revision_id,
            target_context_observation_source, target_observed_embedding_dimension,
            target_embedding_observation_qualification_revision_id,
            target_embedding_observation_source
        );
        INSERT INTO model_revision_heads (
            workspace_id, model_id, current_revision_id, version
        ) VALUES (
            target_workspace_id, target_model_id, target_revision_id, 1
        );
        RETURN target_revision_id;
    END IF;

    IF current_head_version <> target_expected_head_version THEN
        RAISE EXCEPTION 'model revision head version conflict'
            USING ERRCODE = '40001';
    END IF;

    INSERT INTO model_revisions (
        id, workspace_id, model_id, connection_revision_id, wire_model_id, kind,
        observed_context_window, context_observation_qualification_revision_id,
        context_observation_source, observed_embedding_dimension,
        embedding_observation_qualification_revision_id, embedding_observation_source
    ) VALUES (
        target_revision_id, target_workspace_id, target_model_id,
        target_connection_revision_id, target_wire_model_id, target_kind,
        target_observed_context_window, target_context_observation_qualification_revision_id,
        target_context_observation_source, target_observed_embedding_dimension,
        target_embedding_observation_qualification_revision_id,
        target_embedding_observation_source
    );
    UPDATE model_revision_heads
       SET current_revision_id = target_revision_id,
           version = version + 1,
           updated_at = NOW()
     WHERE workspace_id = target_workspace_id
       AND model_id = target_model_id;
    RETURN target_revision_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_set_workspace_model_default(
    target_default_id UUID,
    target_workspace_id UUID,
    target_purpose TEXT,
    target_model_id UUID,
    target_required_capabilities TEXT[],
    target_expected_version BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    current_default_id UUID;
    current_version BIGINT;
BEGIN
    IF target_default_id IS NULL OR target_workspace_id IS NULL
       OR target_purpose IS NULL OR length(btrim(target_purpose)) = 0
       OR target_model_id IS NULL OR target_required_capabilities IS NULL
       OR cardinality(target_required_capabilities) = 0
       OR target_expected_version IS NULL OR target_expected_version < 0 THEN
        RAISE EXCEPTION 'workspace model default arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended(target_workspace_id::TEXT || ':' || target_purpose, 0)
    );

    SELECT id, version
      INTO current_default_id, current_version
      FROM workspace_model_defaults
     WHERE workspace_id = target_workspace_id
       AND purpose = target_purpose
     FOR UPDATE;

    IF NOT FOUND THEN
        IF target_expected_version <> 0 THEN
            RAISE EXCEPTION 'workspace model default version conflict'
                USING ERRCODE = '40001';
        END IF;
        INSERT INTO workspace_model_defaults (
            id, workspace_id, purpose, model_id, required_capabilities, version
        ) VALUES (
            target_default_id, target_workspace_id, target_purpose,
            target_model_id, target_required_capabilities, 1
        );
        RETURN target_default_id;
    END IF;

    IF current_version <> target_expected_version THEN
        RAISE EXCEPTION 'workspace model default version conflict'
            USING ERRCODE = '40001';
    END IF;

    UPDATE workspace_model_defaults
       SET model_id = target_model_id,
           required_capabilities = target_required_capabilities,
           version = version + 1
     WHERE id = current_default_id;
    RETURN current_default_id;
END
$$;

CREATE TABLE model_qualification_heads (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    model_revision_id UUID NOT NULL,
    current_qualification_revision_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, model_revision_id),
    CONSTRAINT model_qualification_heads_revision_fkey
        FOREIGN KEY (workspace_id, model_revision_id)
        REFERENCES model_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_qualification_heads_qualification_fkey
        FOREIGN KEY (workspace_id, model_revision_id, current_qualification_revision_id)
        REFERENCES model_qualification_revisions(workspace_id, model_revision_id, id)
        ON DELETE RESTRICT
);

CREATE TABLE workspace_model_defaults (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    purpose TEXT NOT NULL CHECK (length(btrim(purpose)) > 0),
    model_id UUID NOT NULL,
    required_capabilities TEXT[] NOT NULL CHECK (cardinality(required_capabilities) > 0),
    version BIGINT NOT NULL CHECK (version >= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workspace_model_defaults_model_fkey
        FOREIGN KEY (workspace_id, model_id)
        REFERENCES models(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT workspace_model_defaults_workspace_purpose_key UNIQUE (workspace_id, purpose),
    CONSTRAINT workspace_model_defaults_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE model_binding_snapshots (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    connection_revision_id UUID NOT NULL,
    connection_qualification_revision_id UUID NOT NULL,
    model_revision_id UUID NOT NULL,
    model_qualification_revision_id UUID NOT NULL,
    branch TEXT NOT NULL CHECK (branch IN ('credential', 'no_auth')),
    credential_revision_id UUID,
    credential_slot_id UUID,
    credential_activation_guard_id UUID,
    expected_slot_version BIGINT CHECK (expected_slot_version >= 0),
    no_auth_binding_revision_id UUID,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_binding_snapshots_connection_revision_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_connection_qualification_fkey
        FOREIGN KEY (workspace_id, connection_revision_id, connection_qualification_revision_id)
        REFERENCES connection_qualification_revisions(workspace_id, connection_revision_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_model_revision_fkey
        FOREIGN KEY (workspace_id, model_revision_id)
        REFERENCES model_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_exact_model_qualification_fkey
        FOREIGN KEY (
            workspace_id, model_revision_id, connection_qualification_revision_id,
            model_qualification_revision_id
        ) REFERENCES model_qualification_revisions(
            workspace_id, model_revision_id, connection_qualification_revision_id, id
        )
        ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_credential_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_credential_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_activation_guard_fkey
        FOREIGN KEY (
            credential_activation_guard_id, workspace_id, connection_id, credential_slot_id
        ) REFERENCES credential_activation_guards(
            id, workspace_id, connection_id, credential_slot_id
        ) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_exact_revision_slot_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id, credential_slot_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id, credential_slot_id)
        ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_exact_no_auth_fkey
        FOREIGN KEY (
            workspace_id, connection_id, connection_revision_id, no_auth_binding_revision_id
        ) REFERENCES no_auth_binding_revisions(
            workspace_id, connection_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT model_binding_snapshots_auth_xor CHECK (
        (branch = 'credential'
            AND credential_revision_id IS NOT NULL
            AND credential_slot_id IS NOT NULL
            AND credential_activation_guard_id IS NOT NULL
            AND expected_slot_version IS NOT NULL
            AND no_auth_binding_revision_id IS NULL)
        OR
        (branch = 'no_auth'
            AND credential_revision_id IS NULL
            AND credential_slot_id IS NULL
            AND credential_activation_guard_id IS NULL
            AND expected_slot_version IS NULL
            AND no_auth_binding_revision_id IS NOT NULL)
    ),
    CONSTRAINT model_binding_snapshots_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT model_binding_snapshots_exact_connection_key
        UNIQUE (workspace_id, connection_id, connection_revision_id, id)
);

CREATE TABLE run_model_binding_snapshots (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    run_id UUID NOT NULL UNIQUE,
    snapshot_id UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, run_id),
    CONSTRAINT run_model_binding_snapshots_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES run_streams(workspace_id, run_id) ON DELETE RESTRICT,
    CONSTRAINT run_model_binding_snapshots_snapshot_fkey
        FOREIGN KEY (workspace_id, snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT
);

CREATE TRIGGER model_revision_heads_guarded BEFORE INSERT OR UPDATE OR DELETE ON model_revision_heads
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER model_qualification_heads_guarded BEFORE INSERT OR UPDATE OR DELETE ON model_qualification_heads
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER workspace_model_defaults_guarded BEFORE INSERT OR UPDATE OR DELETE ON workspace_model_defaults
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER model_revisions_immutable BEFORE INSERT OR UPDATE OR DELETE ON model_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_qualification_revisions_immutable BEFORE INSERT OR UPDATE OR DELETE ON model_qualification_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_binding_snapshots_immutable BEFORE INSERT OR UPDATE OR DELETE ON model_binding_snapshots
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER run_model_binding_snapshots_immutable BEFORE INSERT OR UPDATE OR DELETE ON run_model_binding_snapshots
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE model_revision_heads ENABLE ROW LEVEL SECURITY; ALTER TABLE model_revision_heads FORCE ROW LEVEL SECURITY;
ALTER TABLE model_revisions ENABLE ROW LEVEL SECURITY; ALTER TABLE model_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_qualification_revisions ENABLE ROW LEVEL SECURITY; ALTER TABLE model_qualification_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_qualification_heads ENABLE ROW LEVEL SECURITY; ALTER TABLE model_qualification_heads FORCE ROW LEVEL SECURITY;
ALTER TABLE workspace_model_defaults ENABLE ROW LEVEL SECURITY; ALTER TABLE workspace_model_defaults FORCE ROW LEVEL SECURITY;
ALTER TABLE model_binding_snapshots ENABLE ROW LEVEL SECURITY; ALTER TABLE model_binding_snapshots FORCE ROW LEVEL SECURITY;
ALTER TABLE run_model_binding_snapshots ENABLE ROW LEVEL SECURITY; ALTER TABLE run_model_binding_snapshots FORCE ROW LEVEL SECURITY;

CREATE POLICY model_revision_heads_workspace_policy ON model_revision_heads USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_revisions_workspace_policy ON model_revisions USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_qualification_revisions_workspace_policy ON model_qualification_revisions USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_qualification_heads_workspace_policy ON model_qualification_heads USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY workspace_model_defaults_workspace_policy ON workspace_model_defaults USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_binding_snapshots_workspace_policy ON model_binding_snapshots USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY run_model_binding_snapshots_workspace_policy ON run_model_binding_snapshots USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

-- Legacy Runs do not carry a model selector. This guarded entry point resolves
-- their sole temporary `chat` default and pins every execution identity before
-- the Run transaction is allowed to commit.
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

    INSERT INTO run_model_binding_snapshots (workspace_id, run_id, snapshot_id)
    VALUES (target_workspace_id, target_run_id, target_snapshot_id);
    RETURN target_snapshot_id;
END
$$;

SELECT vestrace_assign_p03_table_owner('model_revision_heads'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_qualification_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_qualification_heads'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('workspace_model_defaults'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_binding_snapshots'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('run_model_binding_snapshots'::REGCLASS);
SELECT vestrace_assign_p03_function_owner('vestrace_create_model_revision_and_advance_head(UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, INTEGER, UUID, TEXT, INTEGER, UUID, TEXT, BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_set_workspace_model_default(UUID, UUID, TEXT, UUID, TEXT[], BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_create_run_model_binding_snapshot(UUID, UUID, UUID, TEXT)'::REGPROCEDURE);
