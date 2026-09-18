CREATE TABLE qualification_jobs (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_revision_id UUID NOT NULL,
    profile_revision TEXT NOT NULL CHECK (length(btrim(profile_revision)) > 0),
    state TEXT NOT NULL CHECK (state IN (
        'requested', 'running', 'succeeded', 'failed_definite',
        'inconclusive_unknown', 'cancelled'
    )),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT qualification_jobs_revision_fkey
        FOREIGN KEY (workspace_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_jobs_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT qualification_jobs_exact_identity_key
        UNIQUE (workspace_id, id, connection_revision_id),
    CONSTRAINT qualification_jobs_exact_profile_key
        UNIQUE (workspace_id, id, connection_revision_id, profile_revision),
    CONSTRAINT qualification_jobs_terminal_time CHECK (
        (state IN ('requested', 'running') AND completed_at IS NULL)
        OR (state IN ('succeeded', 'failed_definite', 'inconclusive_unknown', 'cancelled')
            AND completed_at IS NOT NULL)
    )
);

CREATE TABLE qualification_target_bindings (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    qualification_job_id UUID NOT NULL UNIQUE,
    connection_id UUID NOT NULL,
    connection_revision_id UUID NOT NULL,
    branch TEXT NOT NULL CHECK (branch IN ('credential', 'no_auth')),
    credential_revision_id UUID,
    credential_slot_id UUID,
    credential_activation_guard_id UUID,
    expected_slot_version BIGINT CHECK (expected_slot_version >= 0),
    no_auth_binding_revision_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT qualification_target_bindings_exact_job_fkey
        FOREIGN KEY (workspace_id, qualification_job_id, connection_revision_id)
        REFERENCES qualification_jobs(workspace_id, id, connection_revision_id) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_revision_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_credential_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_credential_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_exact_credential_fkey
        FOREIGN KEY (workspace_id, credential_slot_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, credential_slot_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_exact_revision_slot_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id, credential_slot_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id, credential_slot_id)
        ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_activation_guard_fkey
        FOREIGN KEY (
            credential_activation_guard_id, workspace_id, connection_id, credential_slot_id
        ) REFERENCES credential_activation_guards(
            id, workspace_id, connection_id, credential_slot_id
        ) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_exact_no_auth_fkey
        FOREIGN KEY (
            workspace_id, connection_id, connection_revision_id, no_auth_binding_revision_id
        ) REFERENCES no_auth_binding_revisions(
            workspace_id, connection_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT qualification_target_bindings_auth_xor CHECK (
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
    CONSTRAINT qualification_target_bindings_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT qualification_target_bindings_exact_identity_key
        UNIQUE (workspace_id, connection_id, connection_revision_id, id)
);

CREATE TABLE qualification_probe_results (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    qualification_job_id UUID NOT NULL,
    probe_ordinal TEXT NOT NULL CHECK (
        probe_ordinal IN ('00', '10', '15', '20', '30', '35', '40', '50', '60', '70', '80', '90')
    ),
    result TEXT NOT NULL CHECK (result IN (
        'pass', 'unsupported_definite', 'failed_definite',
        'inconclusive_unknown', 'skipped_prerequisite'
    )),
    external_effect_id UUID,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT qualification_probe_results_job_fkey
        FOREIGN KEY (workspace_id, qualification_job_id)
        REFERENCES qualification_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_probe_results_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT qualification_probe_results_job_ordinal_key
        UNIQUE (qualification_job_id, probe_ordinal)
);

CREATE TABLE connection_qualification_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_revision_id UUID NOT NULL,
    qualification_job_id UUID NOT NULL UNIQUE,
    profile_revision TEXT NOT NULL CHECK (length(btrim(profile_revision)) > 0),
    valid_until TIMESTAMPTZ NOT NULL,
    capabilities TEXT[] NOT NULL CHECK (cardinality(capabilities) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT connection_qualification_revisions_connection_fkey
        FOREIGN KEY (workspace_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_qualification_revisions_exact_job_fkey
        FOREIGN KEY (
            workspace_id, qualification_job_id, connection_revision_id, profile_revision
        ) REFERENCES qualification_jobs(
            workspace_id, id, connection_revision_id, profile_revision
        ) ON DELETE RESTRICT,
    CONSTRAINT connection_qualification_revisions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT connection_qualification_revisions_identity_key
        UNIQUE (workspace_id, connection_revision_id, id),
    CONSTRAINT connection_qualification_revisions_exact_job_key
        UNIQUE (workspace_id, connection_revision_id, qualification_job_id, id)
);

CREATE TABLE connection_qualification_heads (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_revision_id UUID NOT NULL,
    current_qualification_revision_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, connection_revision_id),
    CONSTRAINT connection_qualification_heads_revision_fkey
        FOREIGN KEY (workspace_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_qualification_heads_qualification_fkey
        FOREIGN KEY (
            workspace_id, connection_revision_id, current_qualification_revision_id
        ) REFERENCES connection_qualification_revisions(
            workspace_id, connection_revision_id, id
        ) ON DELETE RESTRICT
);

CREATE TRIGGER qualification_jobs_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON qualification_jobs
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER connection_qualification_heads_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON connection_qualification_heads
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER qualification_target_bindings_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON qualification_target_bindings
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER qualification_probe_results_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON qualification_probe_results
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER connection_qualification_revisions_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON connection_qualification_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE qualification_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE qualification_jobs FORCE ROW LEVEL SECURITY;
ALTER TABLE qualification_target_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE qualification_target_bindings FORCE ROW LEVEL SECURITY;
ALTER TABLE qualification_probe_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE qualification_probe_results FORCE ROW LEVEL SECURITY;
ALTER TABLE connection_qualification_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_qualification_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE connection_qualification_heads ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_qualification_heads FORCE ROW LEVEL SECURITY;

CREATE POLICY qualification_jobs_workspace_policy ON qualification_jobs
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY qualification_target_bindings_workspace_policy ON qualification_target_bindings
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY qualification_probe_results_workspace_policy ON qualification_probe_results
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY connection_qualification_revisions_workspace_policy ON connection_qualification_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY connection_qualification_heads_workspace_policy ON connection_qualification_heads
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

SELECT vestrace_assign_p03_table_owner('qualification_jobs'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('qualification_target_bindings'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('qualification_probe_results'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_qualification_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_qualification_heads'::REGCLASS);
