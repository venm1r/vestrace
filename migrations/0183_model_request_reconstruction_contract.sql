SELECT vestrace_grant_p03_dependency_references();

CREATE TABLE model_request_shape_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    version BIGINT NOT NULL CHECK (version >= 1),
    request_kind TEXT NOT NULL CHECK (request_kind IN ('models_list', 'chat_completions', 'embeddings')),
    stream BOOLEAN NOT NULL,
    input_roles TEXT[] NOT NULL CHECK (
        input_roles <@ ARRAY['system', 'user', 'assistant', 'tool']::TEXT[]
        AND array_position(input_roles, NULL) IS NULL
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, id, version),
    CONSTRAINT model_request_shape_stream_kind_check
        CHECK (NOT stream OR request_kind = 'chat_completions'),
    CONSTRAINT model_request_shape_input_roles_check CHECK (
        (request_kind = 'chat_completions' AND cardinality(input_roles) BETWEEN 1 AND 4096)
        OR (request_kind <> 'chat_completions' AND cardinality(input_roles) = 0)
    )
);

CREATE TABLE model_sampling_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    version BIGINT NOT NULL CHECK (version >= 1),
    temperature DOUBLE PRECISION NOT NULL CHECK (temperature >= 0.0 AND temperature <= 2.0),
    top_p DOUBLE PRECISION NOT NULL CHECK (top_p > 0.0 AND top_p <= 1.0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, id, version)
);

CREATE TABLE model_limits_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    version BIGINT NOT NULL CHECK (version >= 1),
    max_output_tokens INTEGER NOT NULL CHECK (max_output_tokens > 0),
    max_inputs INTEGER NOT NULL CHECK (max_inputs > 0 AND max_inputs <= 4096),
    max_input_bytes INTEGER NOT NULL CHECK (max_input_bytes > 0 AND max_input_bytes <= 4194304),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, id, version)
);

CREATE TABLE model_tool_schema_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    version BIGINT NOT NULL CHECK (version >= 1),
    tool_name TEXT NOT NULL CHECK (
        octet_length(btrim(tool_name)) >= 1 AND octet_length(tool_name) <= 128
    ),
    description TEXT CHECK (description IS NULL OR octet_length(description) <= 1024),
    parameters_schema JSONB NOT NULL CHECK (
        jsonb_typeof(parameters_schema) = 'object'
        AND pg_column_size(parameters_schema) <= 1048576
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, id, version)
);

CREATE TABLE model_request_evidence_check_missing_references (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    evidence_check_id UUID NOT NULL,
    reference_kind TEXT NOT NULL CHECK (reference_kind IN (
        'connection_revision', 'connection_qualification_revision', 'model_revision',
        'model_qualification_revision', 'binding_snapshot', 'qualification_target',
        'external_effect', 'qualification_probe', 'governed_input_material',
        'tool_schema_revision', 'sampling_revision', 'limits_revision', 'request_shape_revision'
    )),
    reference_id UUID NOT NULL,
    erasure_preparation_id UUID,
    erasure_tombstone_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_request_evidence_missing_check_fkey
        FOREIGN KEY (evidence_check_id)
        REFERENCES model_request_evidence_checks(id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_missing_preparation_fkey
        FOREIGN KEY (erasure_preparation_id, workspace_id)
        REFERENCES material_erasure_preparations(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_missing_tombstone_fkey
        FOREIGN KEY (erasure_tombstone_id)
        REFERENCES material_erasure_audit_tombstones(id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_missing_erasure_pair CHECK (
        (erasure_preparation_id IS NULL AND erasure_tombstone_id IS NULL)
        OR (erasure_preparation_id IS NOT NULL AND erasure_tombstone_id IS NOT NULL)
    ),
    UNIQUE (evidence_check_id, reference_kind, reference_id)
);

CREATE TRIGGER model_request_shape_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON model_request_shape_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_sampling_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON model_sampling_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_limits_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON model_limits_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_tool_schema_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON model_tool_schema_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_request_evidence_missing_references_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON model_request_evidence_check_missing_references
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE model_request_shape_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_request_shape_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_sampling_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_sampling_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_limits_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_limits_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_tool_schema_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_tool_schema_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_request_evidence_check_missing_references ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_request_evidence_check_missing_references FORCE ROW LEVEL SECURITY;

CREATE POLICY model_request_shape_revisions_workspace_policy ON model_request_shape_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_sampling_revisions_workspace_policy ON model_sampling_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_limits_revisions_workspace_policy ON model_limits_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_tool_schema_revisions_workspace_policy ON model_tool_schema_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_request_evidence_missing_references_workspace_policy
    ON model_request_evidence_check_missing_references
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

CREATE OR REPLACE FUNCTION vestrace_create_model_request_shape_revision(
    target_id UUID,
    target_workspace_id UUID,
    target_version BIGINT,
    target_request_kind TEXT,
    target_stream BOOLEAN,
    target_input_roles TEXT[]
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model request shape workspace mismatch' USING ERRCODE = '42501';
    END IF;
    INSERT INTO model_request_shape_revisions (
        id, workspace_id, version, request_kind, stream, input_roles
    ) VALUES (
        target_id, target_workspace_id, target_version, target_request_kind,
        target_stream, target_input_roles
    )
    ON CONFLICT (id) DO NOTHING;
    IF NOT EXISTS (
        SELECT 1 FROM model_request_shape_revisions
         WHERE id = target_id AND workspace_id = target_workspace_id
           AND version = target_version AND request_kind = target_request_kind
           AND stream = target_stream AND input_roles = target_input_roles
    ) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    RETURN target_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_model_sampling_revision(
    target_id UUID,
    target_workspace_id UUID,
    target_version BIGINT,
    target_temperature DOUBLE PRECISION,
    target_top_p DOUBLE PRECISION
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model sampling workspace mismatch' USING ERRCODE = '42501';
    END IF;
    INSERT INTO model_sampling_revisions (id, workspace_id, version, temperature, top_p)
    VALUES (target_id, target_workspace_id, target_version, target_temperature, target_top_p)
    ON CONFLICT (id) DO NOTHING;
    IF NOT EXISTS (
        SELECT 1 FROM model_sampling_revisions
         WHERE id = target_id AND workspace_id = target_workspace_id
           AND version = target_version AND temperature = target_temperature AND top_p = target_top_p
    ) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    RETURN target_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_model_limits_revision(
    target_id UUID,
    target_workspace_id UUID,
    target_version BIGINT,
    target_max_output_tokens INTEGER,
    target_max_inputs INTEGER,
    target_max_input_bytes INTEGER
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model limits workspace mismatch' USING ERRCODE = '42501';
    END IF;
    INSERT INTO model_limits_revisions (
        id, workspace_id, version, max_output_tokens, max_inputs, max_input_bytes
    ) VALUES (
        target_id, target_workspace_id, target_version, target_max_output_tokens,
        target_max_inputs, target_max_input_bytes
    ) ON CONFLICT (id) DO NOTHING;
    IF NOT EXISTS (
        SELECT 1 FROM model_limits_revisions
         WHERE id = target_id AND workspace_id = target_workspace_id
           AND version = target_version AND max_output_tokens = target_max_output_tokens
           AND max_inputs = target_max_inputs AND max_input_bytes = target_max_input_bytes
    ) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    RETURN target_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_model_tool_schema_revision(
    target_id UUID,
    target_workspace_id UUID,
    target_version BIGINT,
    target_tool_name TEXT,
    target_description TEXT,
    target_parameters_schema JSONB
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model tool schema workspace mismatch' USING ERRCODE = '42501';
    END IF;
    INSERT INTO model_tool_schema_revisions (
        id, workspace_id, version, tool_name, description, parameters_schema
    ) VALUES (
        target_id, target_workspace_id, target_version, target_tool_name,
        target_description, target_parameters_schema
    ) ON CONFLICT (id) DO NOTHING;
    IF NOT EXISTS (
        SELECT 1 FROM model_tool_schema_revisions
         WHERE id = target_id AND workspace_id = target_workspace_id
           AND version = target_version AND tool_name = target_tool_name
           AND description IS NOT DISTINCT FROM target_description
           AND parameters_schema = target_parameters_schema
    ) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    RETURN target_id;
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
       OR target_cause_kind NOT IN ('run_step', 'qualification_probe') THEN
        RAISE EXCEPTION 'model request evidence kind is malformed' USING ERRCODE = '22023';
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

    IF target_cause_kind = 'run_step' THEN
        IF target_request_kind = 'models_list'
           OR target_binding_snapshot_id IS NULL OR target_qualification_binding_id IS NOT NULL
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'binding_snapshot') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'connection_qualification_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind = 'model_qualification_revision') <> 1
           OR (SELECT count(*) FROM unnest(node_kinds) AS kind WHERE kind IN ('qualification_target', 'qualification_probe')) <> 0 THEN
            RAISE EXCEPTION 'run-step evidence cause matrix is invalid' USING ERRCODE = '23514';
        END IF;
        SELECT * INTO snapshot_row FROM model_binding_snapshots
         WHERE id = target_binding_snapshot_id AND workspace_id = target_workspace_id;
        IF NOT FOUND
           OR snapshot_row.connection_revision_id <> node_ids[array_position(node_kinds, 'connection_revision')]
           OR snapshot_row.connection_qualification_revision_id <> node_ids[array_position(node_kinds, 'connection_qualification_revision')]
           OR snapshot_row.model_revision_id <> node_ids[array_position(node_kinds, 'model_revision')]
           OR snapshot_row.model_qualification_revision_id <> node_ids[array_position(node_kinds, 'model_qualification_revision')]
           OR node_ids[array_position(node_kinds, 'binding_snapshot')] <> target_binding_snapshot_id THEN
            RAISE EXCEPTION 'run-step evidence tuple is incompatible' USING ERRCODE = '23514';
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

CREATE OR REPLACE FUNCTION vestrace_append_model_request_evidence_check(
    target_check_id UUID,
    target_workspace_id UUID,
    target_root_id UUID,
    target_status TEXT,
    missing_kinds TEXT[],
    missing_ids UUID[],
    preparation_ids UUID[],
    tombstone_ids UUID[]
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    missing_count INTEGER := cardinality(missing_ids);
    missing_index INTEGER;
BEGIN
    IF target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'model request evidence check workspace mismatch' USING ERRCODE = '42501';
    END IF;
    IF missing_count IS NULL
       OR cardinality(missing_kinds) <> missing_count
       OR cardinality(preparation_ids) <> missing_count
       OR cardinality(tombstone_ids) <> missing_count
       OR (target_status = 'complete' AND missing_count <> 0)
       OR (target_status IN ('incomplete', 'expired') AND missing_count = 0)
       OR target_status NOT IN ('complete', 'incomplete', 'expired') THEN
        RAISE EXCEPTION 'model request evidence check is malformed' USING ERRCODE = '22023';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM model_request_evidence_roots
         WHERE id = target_root_id AND workspace_id = target_workspace_id
    ) THEN
        RAISE EXCEPTION 'model request evidence check root is absent or cross-workspace'
            USING ERRCODE = '23514';
    END IF;
    FOR missing_index IN 1..missing_count LOOP
        IF NOT EXISTS (
            SELECT 1 FROM model_request_evidence_nodes
             WHERE workspace_id = target_workspace_id
               AND evidence_root_id = target_root_id
               AND reference_kind = missing_kinds[missing_index]
               AND reference_id = missing_ids[missing_index]
        ) THEN
            RAISE EXCEPTION 'model request evidence check cites a non-root reference'
                USING ERRCODE = '23514';
        END IF;
    END LOOP;
    IF EXISTS (
        SELECT 1 FROM generate_subscripts(missing_ids, 1) AS left_index
        JOIN generate_subscripts(missing_ids, 1) AS right_index
          ON left_index < right_index
         AND missing_kinds[left_index] = missing_kinds[right_index]
         AND missing_ids[left_index] = missing_ids[right_index]
    ) THEN
        RAISE EXCEPTION 'model request evidence check contains a duplicate reference'
            USING ERRCODE = '23514';
    END IF;
    IF target_status = 'expired' THEN
        PERFORM node.id
          FROM model_request_evidence_nodes AS node
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_root_id
           AND node.reference_kind = 'governed_input_material'
         ORDER BY node.id
         FOR SHARE OF node;
        PERFORM material.id
          FROM model_request_evidence_nodes AS node
          JOIN content_materials AS material
            ON material.workspace_id = node.workspace_id
           AND material.id = node.reference_id
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_root_id
           AND node.reference_kind = 'governed_input_material'
         ORDER BY material.id
         FOR SHARE OF material;
        PERFORM bytes.material_id
          FROM model_request_evidence_nodes AS node
          JOIN content_material_bytes AS bytes
            ON bytes.workspace_id = node.workspace_id
           AND bytes.material_id = node.reference_id
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_root_id
           AND node.reference_kind = 'governed_input_material'
         ORDER BY bytes.material_id
         FOR SHARE OF bytes;
        FOR missing_index IN 1..missing_count LOOP
            IF missing_kinds[missing_index] <> 'governed_input_material'
               OR NOT EXISTS (
                   SELECT 1
                     FROM material_erasure_preparations AS preparation
                     JOIN material_erasure_audit_tombstones AS tombstone
                       ON tombstone.preparation_id = preparation.id
                      AND tombstone.workspace_id = preparation.workspace_id
                    WHERE preparation.id = preparation_ids[missing_index]
                      AND preparation.workspace_id = target_workspace_id
                      AND preparation.target_kind = 'content'
                      AND preparation.content_material_id = missing_ids[missing_index]
                      AND preparation.finalized_at IS NOT NULL
                      AND preparation.erasure_receipt IS NOT NULL
                      AND tombstone.id = tombstone_ids[missing_index]
               ) THEN
                RAISE EXCEPTION 'expired evidence lacks exact finalized erasure' USING ERRCODE = '23514';
            END IF;
        END LOOP;
        IF EXISTS (
            SELECT 1
              FROM model_request_evidence_nodes AS node
              LEFT JOIN content_materials AS material
                ON material.workspace_id = node.workspace_id
               AND material.id = node.reference_id
              LEFT JOIN content_material_bytes AS bytes
                ON bytes.workspace_id = node.workspace_id
               AND bytes.material_id = node.reference_id
             WHERE node.workspace_id = target_workspace_id
               AND node.evidence_root_id = target_root_id
               AND node.reference_kind = 'governed_input_material'
               AND NOT EXISTS (
                   SELECT 1
                     FROM generate_subscripts(missing_ids, 1) AS item
                    WHERE missing_kinds[item] = 'governed_input_material'
                      AND missing_ids[item] = node.reference_id
               )
               AND (
                   material.id IS NULL
                   OR material.state <> 'live'
                   OR bytes.material_id IS NULL
                   OR octet_length(bytes.ciphertext) < 4096
                   OR octet_length(bytes.ciphertext) > 1048576
                   OR (
                       octet_length(bytes.ciphertext)
                       & (octet_length(bytes.ciphertext) - 1)
                   ) <> 0
                   OR substring(bytes.ciphertext FROM 1 FOR 4) <> decode('564d5246', 'hex')
                   OR get_byte(bytes.ciphertext, 4) <> 1
               )
        ) THEN
            RAISE EXCEPTION 'expired evidence omits an unavailable governed input'
                USING ERRCODE = '23514';
        END IF;
    ELSIF EXISTS (
        SELECT 1 FROM generate_subscripts(preparation_ids, 1) AS item
         WHERE preparation_ids[item] IS NOT NULL OR tombstone_ids[item] IS NOT NULL
    ) THEN
        RAISE EXCEPTION 'non-expired evidence cannot cite erasure' USING ERRCODE = '23514';
    END IF;

    INSERT INTO model_request_evidence_checks (
        id, workspace_id, evidence_root_id, status, missing_reference_count,
        erasure_preparation_id
    ) VALUES (
        target_check_id, target_workspace_id, target_root_id, target_status, missing_count,
        CASE WHEN target_status = 'expired' THEN preparation_ids[1] ELSE NULL END
    );
    FOR missing_index IN 1..missing_count LOOP
        INSERT INTO model_request_evidence_check_missing_references (
            id, workspace_id, evidence_check_id, reference_kind, reference_id,
            erasure_preparation_id, erasure_tombstone_id
        ) VALUES (
            gen_random_uuid(), target_workspace_id, target_check_id,
            missing_kinds[missing_index], missing_ids[missing_index],
            preparation_ids[missing_index], tombstone_ids[missing_index]
        );
    END LOOP;
    RETURN target_check_id;
END
$$;

SELECT vestrace_assign_p03_table_owner('model_request_shape_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_sampling_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_limits_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_tool_schema_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_request_evidence_check_missing_references'::REGCLASS);

SELECT vestrace_assign_p03_function_owner('vestrace_create_model_request_shape_revision(UUID, UUID, BIGINT, TEXT, BOOLEAN, TEXT[])'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_create_model_sampling_revision(UUID, UUID, BIGINT, DOUBLE PRECISION, DOUBLE PRECISION)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_create_model_limits_revision(UUID, UUID, BIGINT, INTEGER, INTEGER, INTEGER)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_create_model_tool_schema_revision(UUID, UUID, BIGINT, TEXT, TEXT, JSONB)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_create_model_request_evidence(UUID, UUID, UUID, TEXT, UUID, UUID, TEXT, UUID, TEXT[], UUID[], BIGINT[], TEXT[])'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_append_model_request_evidence_check(UUID, UUID, UUID, TEXT, TEXT[], UUID[], UUID[], UUID[])'::REGPROCEDURE);
