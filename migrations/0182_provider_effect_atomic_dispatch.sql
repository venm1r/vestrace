DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'public.run_steps'::regclass AND conname = 'run_steps_workspace_run_id_key') THEN
        ALTER TABLE run_steps ADD CONSTRAINT run_steps_workspace_run_id_key UNIQUE (workspace_id, run_id, id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'public.artifacts'::regclass AND conname = 'artifacts_workspace_id_id_key') THEN
        ALTER TABLE artifacts ADD CONSTRAINT artifacts_workspace_id_id_key UNIQUE (workspace_id, id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'public.artifact_revisions'::regclass AND conname = 'artifact_revisions_workspace_artifact_id_key') THEN
        ALTER TABLE artifact_revisions ADD CONSTRAINT artifact_revisions_workspace_artifact_id_key UNIQUE (workspace_id, artifact_id, id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'public.model_executions'::regclass AND conname = 'model_executions_workspace_id_id_key') THEN
        ALTER TABLE model_executions ADD CONSTRAINT model_executions_workspace_id_id_key UNIQUE (workspace_id, id);
    END IF;
END
$$;

CREATE TABLE provider_result_preparations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    external_effect_id UUID NOT NULL UNIQUE,
    run_id UUID NOT NULL,
    step_id UUID NOT NULL,
    material_intent_id UUID NOT NULL UNIQUE,
    prepared_attachment_id UUID NOT NULL UNIQUE,
    artifact_id UUID NOT NULL,
    artifact_revision_id UUID NOT NULL,
    model_execution_id UUID NOT NULL,
    size_class BIGINT NOT NULL CHECK (size_class >= 4096),
    expected_run_version BIGINT NOT NULL CHECK (expected_run_version >= 1),
    state TEXT NOT NULL DEFAULT 'result_prepared' CHECK (state IN ('result_prepared', 'published')),
    prepared_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    CONSTRAINT provider_result_preparations_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_result_preparations_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES agent_runs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_result_preparations_step_fkey
        FOREIGN KEY (workspace_id, run_id, step_id)
        REFERENCES run_steps(workspace_id, run_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_result_preparations_intent_fkey
        FOREIGN KEY (material_intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_result_preparations_publication_pair CHECK (
        (state = 'result_prepared' AND published_at IS NULL)
        OR (state = 'published' AND published_at IS NOT NULL)
    ),
    CONSTRAINT provider_result_preparations_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT provider_result_preparations_fixed_output_key
        UNIQUE (external_effect_id, run_id, step_id, artifact_id, artifact_revision_id, model_execution_id),
    CONSTRAINT provider_result_preparations_exact_publication_key UNIQUE (
        workspace_id, id, external_effect_id, run_id, step_id, material_intent_id,
        prepared_attachment_id, artifact_id, artifact_revision_id, model_execution_id,
        size_class
    )
);

CREATE TABLE provider_result_publications (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    provider_result_preparation_id UUID NOT NULL UNIQUE,
    external_effect_id UUID NOT NULL UNIQUE,
    run_id UUID NOT NULL,
    step_id UUID NOT NULL,
    artifact_id UUID NOT NULL,
    artifact_revision_id UUID NOT NULL UNIQUE,
    model_execution_id UUID NOT NULL UNIQUE,
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    material_intent_id UUID NOT NULL,
    prepared_attachment_id UUID NOT NULL,
    size_class BIGINT NOT NULL,
    CONSTRAINT provider_result_publications_exact_preparation_fkey
        FOREIGN KEY (
            workspace_id, provider_result_preparation_id, external_effect_id, run_id,
            step_id, material_intent_id, prepared_attachment_id, artifact_id,
            artifact_revision_id, model_execution_id, size_class
        ) REFERENCES provider_result_preparations(
            workspace_id, id, external_effect_id, run_id, step_id, material_intent_id,
            prepared_attachment_id, artifact_id, artifact_revision_id,
            model_execution_id, size_class
        ) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT provider_result_publications_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_result_publications_run_fkey
        FOREIGN KEY (workspace_id, run_id)
        REFERENCES agent_runs(workspace_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT provider_result_publications_step_fkey
        FOREIGN KEY (workspace_id, run_id, step_id)
        REFERENCES run_steps(workspace_id, run_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT provider_result_publications_artifact_fkey
        FOREIGN KEY (workspace_id, artifact_id)
        REFERENCES artifacts(workspace_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT provider_result_publications_artifact_revision_fkey
        FOREIGN KEY (workspace_id, artifact_id, artifact_revision_id)
        REFERENCES artifact_revisions(workspace_id, artifact_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT provider_result_publications_model_execution_fkey
        FOREIGN KEY (workspace_id, model_execution_id)
        REFERENCES model_executions(workspace_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT provider_result_publications_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE artifact_revision_contents (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    artifact_id UUID NOT NULL,
    artifact_revision_id UUID NOT NULL UNIQUE,
    content_material_id UUID NOT NULL UNIQUE,
    provider_result_preparation_id UUID NOT NULL UNIQUE,
    external_effect_id UUID NOT NULL,
    run_id UUID NOT NULL,
    step_id UUID NOT NULL,
    material_intent_id UUID NOT NULL,
    prepared_attachment_id UUID NOT NULL,
    model_execution_id UUID NOT NULL,
    erasure_bound_commitment BYTEA NOT NULL UNIQUE,
    size_class BIGINT NOT NULL CHECK (size_class >= 4096),
    media_class TEXT NOT NULL CHECK (media_class IN ('text', 'json', 'binary', 'image', 'audio')),
    CONSTRAINT artifact_revision_contents_erasure_bound_commitment_length_check
        CHECK (octet_length(erasure_bound_commitment) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT artifact_revision_contents_artifact_fkey
        FOREIGN KEY (workspace_id, artifact_id)
        REFERENCES artifacts(workspace_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT artifact_revision_contents_artifact_revision_fkey
        FOREIGN KEY (workspace_id, artifact_id, artifact_revision_id)
        REFERENCES artifact_revisions(workspace_id, artifact_id, id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT artifact_revision_contents_exact_material_fkey
        FOREIGN KEY (content_material_id, workspace_id, material_intent_id)
        REFERENCES content_materials(id, workspace_id, intent_id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT artifact_revision_contents_exact_preparation_fkey
        FOREIGN KEY (
            workspace_id, provider_result_preparation_id, external_effect_id, run_id,
            step_id, material_intent_id, prepared_attachment_id, artifact_id,
            artifact_revision_id, model_execution_id, size_class
        ) REFERENCES provider_result_preparations(
            workspace_id, id, external_effect_id, run_id, step_id, material_intent_id,
            prepared_attachment_id, artifact_id, artifact_revision_id,
            model_execution_id, size_class
        ) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT artifact_revision_contents_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TRIGGER provider_result_preparations_guarded BEFORE INSERT OR UPDATE OR DELETE ON provider_result_preparations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER provider_result_publications_immutable BEFORE INSERT OR UPDATE OR DELETE ON provider_result_publications
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER artifact_revision_contents_immutable BEFORE INSERT OR UPDATE OR DELETE ON artifact_revision_contents
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
ALTER TABLE provider_result_preparations ENABLE ROW LEVEL SECURITY; ALTER TABLE provider_result_preparations FORCE ROW LEVEL SECURITY;
ALTER TABLE provider_result_publications ENABLE ROW LEVEL SECURITY; ALTER TABLE provider_result_publications FORCE ROW LEVEL SECURITY;
ALTER TABLE artifact_revision_contents ENABLE ROW LEVEL SECURITY; ALTER TABLE artifact_revision_contents FORCE ROW LEVEL SECURITY;
CREATE POLICY provider_result_preparations_workspace_policy ON provider_result_preparations USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY provider_result_publications_workspace_policy ON provider_result_publications USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY artifact_revision_contents_workspace_policy ON artifact_revision_contents USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

CREATE OR REPLACE FUNCTION vestrace_prepare_provider_result(
    target_effect_id UUID,
    target_run_id UUID,
    target_step_id UUID,
    target_intent_id UUID,
    target_attachment_id UUID,
    target_artifact_id UUID,
    target_artifact_revision_id UUID,
    target_model_execution_id UUID,
    target_ciphertext BYTEA,
    target_size_class BIGINT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_workspace_id UUID;
    target_preparation_id UUID;
    target_expected_run_version BIGINT;
    intent_row material_key_creation_intents%ROWTYPE;
BEGIN
    SELECT workspace_id INTO target_workspace_id
      FROM external_effect_intents WHERE id = target_effect_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result effect is absent' USING ERRCODE = '23514';
    END IF;
    IF NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       IS DISTINCT FROM target_workspace_id THEN
        RAISE EXCEPTION 'provider result workspace context is required' USING ERRCODE = '42501';
    END IF;
    SELECT run_version INTO target_expected_run_version FROM agent_runs
      WHERE workspace_id = target_workspace_id AND id = target_run_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result run is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM 1 FROM run_steps
      WHERE workspace_id = target_workspace_id AND run_id = target_run_id AND id = target_step_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result step is absent from its run' USING ERRCODE = '23514';
    END IF;
    SELECT * INTO intent_row FROM material_key_creation_intents
      WHERE id = target_intent_id AND workspace_id = target_workspace_id FOR UPDATE;
    IF NOT FOUND
       OR intent_row.owner_kind <> 'provider_result'
       OR intent_row.owner_id <> target_effect_id
       OR intent_row.output_ordinal <> 0 THEN
        RAISE EXCEPTION 'provider result requires its exact receipted material intent and output tuple'
            USING ERRCODE = '23514';
    END IF;
    IF target_size_class < 4096 OR target_ciphertext IS NULL OR octet_length(target_ciphertext) = 0 THEN
        RAISE EXCEPTION 'provider result ciphertext preparation is invalid' USING ERRCODE = '23514';
    END IF;

    SELECT id INTO target_preparation_id FROM provider_result_preparations
        WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND THEN
        IF NOT EXISTS (
            SELECT 1
              FROM provider_result_preparations AS preparation
              JOIN prepared_material_attachments AS attachment
                ON attachment.id = preparation.prepared_attachment_id
               AND attachment.intent_id = preparation.material_intent_id
               AND attachment.marker = 'result_prepared'
              JOIN content_material_bytes AS bytes
                ON bytes.intent_id = preparation.material_intent_id
             WHERE preparation.id = target_preparation_id
               AND preparation.run_id = target_run_id
               AND preparation.step_id = target_step_id
               AND preparation.material_intent_id = target_intent_id
               AND preparation.prepared_attachment_id = target_attachment_id
               AND preparation.artifact_id = target_artifact_id
               AND preparation.artifact_revision_id = target_artifact_revision_id
               AND preparation.model_execution_id = target_model_execution_id
               AND preparation.size_class = target_size_class
               AND bytes.ciphertext = target_ciphertext
               AND octet_length(bytes.ciphertext) = target_size_class
               AND intent_row.state IN ('result_prepared', 'bound', 'live')
        ) THEN
            RAISE EXCEPTION 'provider result replay tuple mismatch' USING ERRCODE = '23514';
        END IF;
        RETURN target_preparation_id;
    END IF;

    IF intent_row.state <> 'provisional_receipted' THEN
        RAISE EXCEPTION 'provider result requires its exact receipted material intent and output tuple'
            USING ERRCODE = '23514';
    END IF;

    PERFORM vestrace_prepare_result_material(
        target_intent_id, target_attachment_id, target_ciphertext, target_size_class
    );
    target_preparation_id := gen_random_uuid();
    INSERT INTO provider_result_preparations (
        id, workspace_id, external_effect_id, run_id, step_id, material_intent_id,
        prepared_attachment_id, artifact_id, artifact_revision_id, model_execution_id,
        size_class, expected_run_version
    ) VALUES (
        target_preparation_id, target_workspace_id, target_effect_id, target_run_id,
        target_step_id, target_intent_id, target_attachment_id, target_artifact_id,
        target_artifact_revision_id, target_model_execution_id, target_size_class,
        target_expected_run_version
    );
    RETURN target_preparation_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_provider_result(
    target_provider_result_preparation_id UUID,
    target_erasure_bound_commitment BYTEA
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preparation_row provider_result_preparations%ROWTYPE;
    intent_row material_key_creation_intents%ROWTYPE;
    material_row content_materials%ROWTYPE;
BEGIN
    -- Host caller obligation: compute this as HMAC-SHA256 over a domain-separated
    -- message binding the content material id and sealed ciphertext, under the
    -- material DEK borrowed through MaterialKeyVault::unwrap, following the
    -- FINGERPRINT_CONTINUITY_DOMAIN precedent. No caller may pass a random or
    -- content-independent value; erasing that DEK makes this value non-recomputable.
    IF target_erasure_bound_commitment IS NULL
       OR octet_length(target_erasure_bound_commitment) <> 32 THEN
        RAISE EXCEPTION 'provider result erasure-bound commitment must be exactly 32 bytes'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO preparation_row FROM provider_result_preparations
        WHERE id = target_provider_result_preparation_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider result preparation is absent' USING ERRCODE = '23514';
    END IF;
    IF NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       IS DISTINCT FROM preparation_row.workspace_id THEN
        RAISE EXCEPTION 'provider result workspace context is required' USING ERRCODE = '42501';
    END IF;
    IF preparation_row.state = 'published' THEN
        RETURN;
    END IF;
    SELECT * INTO intent_row FROM material_key_creation_intents
      WHERE id = preparation_row.material_intent_id FOR UPDATE;
    IF intent_row.state <> 'bound'
       OR intent_row.bound_receipt IS NULL
       OR intent_row.prepared_marker <> 'result_prepared' THEN
        RAISE EXCEPTION 'provider result finalization requires its witnessed Bound receipt'
            USING ERRCODE = '23514';
    END IF;
    PERFORM 1 FROM artifacts WHERE workspace_id = preparation_row.workspace_id
        AND id = preparation_row.artifact_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'provider result Artifact envelope is absent' USING ERRCODE = '23514'; END IF;
    PERFORM 1 FROM artifact_revisions WHERE workspace_id = preparation_row.workspace_id
        AND artifact_id = preparation_row.artifact_id AND id = preparation_row.artifact_revision_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'provider result Artifact revision envelope is absent' USING ERRCODE = '23514'; END IF;
    PERFORM 1 FROM model_executions WHERE workspace_id = preparation_row.workspace_id
        AND id = preparation_row.model_execution_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'provider result model execution envelope is absent' USING ERRCODE = '23514'; END IF;

    PERFORM vestrace_finalize_bound_content_material(intent_row.id);
    SELECT * INTO material_row FROM content_materials WHERE intent_id = intent_row.id;
    INSERT INTO artifact_revision_contents (
        id, workspace_id, artifact_id, artifact_revision_id, content_material_id,
        provider_result_preparation_id, external_effect_id, run_id, step_id,
        material_intent_id, prepared_attachment_id, model_execution_id,
        erasure_bound_commitment, size_class, media_class
    ) VALUES (
        gen_random_uuid(), preparation_row.workspace_id, preparation_row.artifact_id,
        preparation_row.artifact_revision_id, material_row.id, preparation_row.id,
        preparation_row.external_effect_id, preparation_row.run_id, preparation_row.step_id,
        preparation_row.material_intent_id, preparation_row.prepared_attachment_id,
        preparation_row.model_execution_id, target_erasure_bound_commitment,
        preparation_row.size_class, 'text'
    );
    INSERT INTO provider_result_publications (
        id, workspace_id, provider_result_preparation_id, external_effect_id,
        run_id, step_id, artifact_id, artifact_revision_id, model_execution_id,
        material_intent_id, prepared_attachment_id, size_class
    ) VALUES (
        gen_random_uuid(), preparation_row.workspace_id, preparation_row.id,
        preparation_row.external_effect_id, preparation_row.run_id, preparation_row.step_id,
        preparation_row.artifact_id, preparation_row.artifact_revision_id,
        preparation_row.model_execution_id, preparation_row.material_intent_id,
        preparation_row.prepared_attachment_id, preparation_row.size_class
    );
    UPDATE provider_result_preparations SET state = 'published', published_at = NOW()
        WHERE id = preparation_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_provider_result_completion()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_preparation_id UUID;
    preparation_row provider_result_preparations%ROWTYPE;
BEGIN
    IF TG_TABLE_NAME = 'provider_result_preparations' THEN
        target_preparation_id := COALESCE(NEW.id, OLD.id);
    ELSE
        target_preparation_id := COALESCE(
            NEW.provider_result_preparation_id,
            OLD.provider_result_preparation_id
        );
    END IF;
    SELECT * INTO preparation_row FROM provider_result_preparations
        WHERE id = target_preparation_id;
    IF NOT FOUND OR preparation_row.state = 'result_prepared' THEN
        RETURN NULL;
    END IF;

    IF (SELECT COUNT(*) FROM provider_result_publications
         WHERE provider_result_preparation_id = preparation_row.id) <> 1
       OR (SELECT COUNT(*) FROM artifact_revision_contents
            WHERE provider_result_preparation_id = preparation_row.id) <> 1
       OR NOT EXISTS (
           SELECT 1 FROM run_steps
            WHERE workspace_id = preparation_row.workspace_id
              AND run_id = preparation_row.run_id
              AND id = preparation_row.step_id
              AND status = 'succeeded'
              AND output_references @> jsonb_build_array(jsonb_build_object(
                  'artifact_id', preparation_row.artifact_id,
                  'artifact_revision_id', preparation_row.artifact_revision_id,
                  'model_execution_id', preparation_row.model_execution_id
              ))
       )
       OR NOT EXISTS (
           SELECT 1 FROM agent_runs
            WHERE workspace_id = preparation_row.workspace_id
              AND id = preparation_row.run_id
              AND run_version = preparation_row.expected_run_version + 1
       )
       OR NOT EXISTS (
           SELECT 1 FROM run_work_items
            WHERE workspace_id = preparation_row.workspace_id
              AND run_id = preparation_row.run_id
              AND step_id = preparation_row.step_id
              AND kind = 'advance_run'
              AND expected_run_version = preparation_row.expected_run_version + 1
              AND status = 'ready'
       ) THEN
        RAISE EXCEPTION 'published provider result requires its exact publication, content map, succeeded step, Run version and AdvanceRun continuation'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER provider_result_preparations_deferred_completion
    AFTER INSERT OR UPDATE OR DELETE ON provider_result_preparations
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_provider_result_completion();
CREATE CONSTRAINT TRIGGER provider_result_publications_deferred_completion
    AFTER INSERT OR UPDATE OR DELETE ON provider_result_publications
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_provider_result_completion();
CREATE CONSTRAINT TRIGGER artifact_revision_contents_deferred_completion
    AFTER INSERT OR UPDATE OR DELETE ON artifact_revision_contents
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_provider_result_completion();

SELECT vestrace_assign_p03_table_owner('provider_result_preparations'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_result_publications'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('artifact_revision_contents'::REGCLASS);
SELECT vestrace_assign_p03_function_owner('vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_finalize_provider_result(UUID, BYTEA)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_validate_provider_result_completion()'::REGPROCEDURE);
