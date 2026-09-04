CREATE TABLE model_request_evidence_roots (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    external_effect_id UUID NOT NULL UNIQUE,
    request_kind TEXT NOT NULL CHECK (request_kind IN ('models_list', 'chat_completions', 'embeddings')),
    binding_snapshot_id UUID,
    qualification_target_binding_id UUID,
    cause_kind TEXT NOT NULL CHECK (cause_kind IN ('run_step', 'qualification_probe')),
    cause_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_request_evidence_roots_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_roots_snapshot_fkey
        FOREIGN KEY (workspace_id, binding_snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_roots_qualification_target_fkey
        FOREIGN KEY (workspace_id, qualification_target_binding_id)
        REFERENCES qualification_target_bindings(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_roots_cause_xor CHECK (
        (cause_kind = 'run_step' AND binding_snapshot_id IS NOT NULL AND qualification_target_binding_id IS NULL)
        OR
        (cause_kind = 'qualification_probe' AND binding_snapshot_id IS NULL AND qualification_target_binding_id IS NOT NULL)
    ),
    CONSTRAINT model_request_evidence_roots_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE model_request_evidence_nodes (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    evidence_root_id UUID NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    reference_kind TEXT NOT NULL CHECK (reference_kind IN (
        'connection_revision', 'connection_qualification_revision', 'model_revision',
        'model_qualification_revision', 'binding_snapshot', 'qualification_target',
        'external_effect', 'qualification_probe', 'governed_input_material',
        'tool_schema_revision', 'sampling_revision', 'limits_revision', 'request_shape_revision'
    )),
    reference_id UUID NOT NULL,
    reference_version BIGINT CHECK (reference_version >= 0),
    safe_ordinal TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_request_evidence_nodes_root_fkey
        FOREIGN KEY (workspace_id, evidence_root_id)
        REFERENCES model_request_evidence_roots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_nodes_root_ordinal_key UNIQUE (evidence_root_id, ordinal)
);

CREATE TABLE model_request_evidence_checks (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    evidence_root_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('complete', 'incomplete', 'expired')),
    missing_reference_count INTEGER NOT NULL DEFAULT 0 CHECK (missing_reference_count >= 0),
    erasure_preparation_id UUID,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT model_request_evidence_checks_root_fkey
        FOREIGN KEY (workspace_id, evidence_root_id)
        REFERENCES model_request_evidence_roots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_checks_erasure_fkey
        FOREIGN KEY (erasure_preparation_id, workspace_id)
        REFERENCES material_erasure_preparations(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT model_request_evidence_checks_status_evidence CHECK (
        (status = 'complete' AND missing_reference_count = 0 AND erasure_preparation_id IS NULL)
        OR (status = 'incomplete' AND missing_reference_count > 0 AND erasure_preparation_id IS NULL)
        OR (status = 'expired' AND missing_reference_count > 0 AND erasure_preparation_id IS NOT NULL)
    )
);

CREATE TRIGGER model_request_evidence_roots_immutable BEFORE INSERT OR UPDATE OR DELETE ON model_request_evidence_roots
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_request_evidence_nodes_immutable BEFORE INSERT OR UPDATE OR DELETE ON model_request_evidence_nodes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER model_request_evidence_checks_immutable BEFORE INSERT OR UPDATE OR DELETE ON model_request_evidence_checks
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE model_request_evidence_roots ENABLE ROW LEVEL SECURITY; ALTER TABLE model_request_evidence_roots FORCE ROW LEVEL SECURITY;
ALTER TABLE model_request_evidence_nodes ENABLE ROW LEVEL SECURITY; ALTER TABLE model_request_evidence_nodes FORCE ROW LEVEL SECURITY;
ALTER TABLE model_request_evidence_checks ENABLE ROW LEVEL SECURITY; ALTER TABLE model_request_evidence_checks FORCE ROW LEVEL SECURITY;
CREATE POLICY model_request_evidence_roots_workspace_policy ON model_request_evidence_roots USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_request_evidence_nodes_workspace_policy ON model_request_evidence_nodes USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY model_request_evidence_checks_workspace_policy ON model_request_evidence_checks USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

SELECT vestrace_assign_p03_table_owner('model_request_evidence_roots'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_request_evidence_nodes'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('model_request_evidence_checks'::REGCLASS);
