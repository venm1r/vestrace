-- P03 Task 7: qualification is durable state, not a best-effort worker loop.
--
-- 0177--0184 are historical migrations.  This forward-only migration adds the
-- guarded lifecycle and the model identities which q1 must pin before it can
-- make any request.  The deployment bootstrap hands the closed set of
-- existing objects to the runtime migrator before this file executes.

DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_task7_qualification_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_task7_qualification_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'Task 7 qualification ownership hand-back must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
    END IF;
END
$$;

ALTER TABLE qualification_target_bindings
    ADD COLUMN chat_model_revision_id UUID,
    ADD COLUMN embedding_model_revision_id UUID;

ALTER TABLE qualification_target_bindings
    ADD CONSTRAINT qualification_target_bindings_chat_model_fkey
        FOREIGN KEY (workspace_id, chat_model_revision_id, connection_revision_id)
        REFERENCES model_revisions(workspace_id, id, connection_revision_id) ON DELETE RESTRICT,
    ADD CONSTRAINT qualification_target_bindings_embedding_model_fkey
        FOREIGN KEY (workspace_id, embedding_model_revision_id, connection_revision_id)
        REFERENCES model_revisions(workspace_id, id, connection_revision_id) ON DELETE RESTRICT;

ALTER TABLE qualification_probe_results
    ADD COLUMN model_request_evidence_id UUID,
    ADD CONSTRAINT qualification_probe_results_evidence_fkey
        FOREIGN KEY (workspace_id, model_request_evidence_id)
        REFERENCES model_request_evidence_roots(workspace_id, id) ON DELETE RESTRICT;

-- A q1 request source records only the structural choices that are not already
-- fixed by the immutable target, profile revision and probe ordinal.  In
-- particular it never stores a rendered request, a tool argument/body, prompt
-- content, authentication material or a provider response body.
CREATE TABLE qualification_q1_mre_sources (
    evidence_root_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    probe_ordinal TEXT NOT NULL CHECK (probe_ordinal IN ('10','20','30','35','40','50','60','70','80','90')),
    message_layout TEXT NOT NULL CHECK (message_layout IN (
        'plain_text', 'multipart_image_marker', 'assistant_tool_call_replay'
    )),
    tool_choice TEXT NOT NULL CHECK (tool_choice IN ('none', 'named_probe', 'required')),
    parallel_tool_calls BOOLEAN NOT NULL,
    response_format TEXT NOT NULL CHECK (response_format IN ('none', 'strict_nonce_json_schema')),
    stream BOOLEAN NOT NULL,
    stream_include_usage BOOLEAN NOT NULL,
    assistant_tool_call_id TEXT CHECK (
        assistant_tool_call_id IS NULL
        OR (octet_length(btrim(assistant_tool_call_id)) BETWEEN 1 AND 256)
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT qualification_q1_mre_sources_root_fkey
        FOREIGN KEY (workspace_id, evidence_root_id)
        REFERENCES model_request_evidence_roots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT qualification_q1_mre_sources_exact_frozen_tuple CHECK (
        (probe_ordinal IN ('10','20','90')
            AND message_layout = 'plain_text' AND tool_choice = 'none'
            AND NOT parallel_tool_calls AND response_format = 'none'
            AND NOT stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NULL)
        OR (probe_ordinal = '30'
            AND message_layout = 'plain_text' AND tool_choice = 'none'
            AND NOT parallel_tool_calls AND response_format = 'none'
            AND stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NULL)
        OR (probe_ordinal = '35'
            AND message_layout = 'plain_text' AND tool_choice = 'none'
            AND NOT parallel_tool_calls AND response_format = 'none'
            AND stream AND stream_include_usage
            AND assistant_tool_call_id IS NULL)
        OR (probe_ordinal = '40'
            AND message_layout = 'plain_text' AND tool_choice = 'named_probe'
            AND NOT parallel_tool_calls AND response_format = 'none'
            AND NOT stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NULL)
        OR (probe_ordinal = '50'
            AND message_layout = 'assistant_tool_call_replay' AND tool_choice = 'none'
            AND NOT parallel_tool_calls AND response_format = 'none'
            AND NOT stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NOT NULL)
        OR (probe_ordinal = '60'
            AND message_layout = 'plain_text' AND tool_choice = 'required'
            AND parallel_tool_calls AND response_format = 'none'
            AND NOT stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NULL)
        OR (probe_ordinal = '70'
            AND message_layout = 'plain_text' AND tool_choice = 'none'
            AND NOT parallel_tool_calls AND response_format = 'strict_nonce_json_schema'
            AND NOT stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NULL)
        OR (probe_ordinal = '80'
            AND message_layout = 'multipart_image_marker' AND tool_choice = 'none'
            AND NOT parallel_tool_calls AND response_format = 'none'
            AND NOT stream AND NOT stream_include_usage
            AND assistant_tool_call_id IS NULL)
    )
);

CREATE TRIGGER qualification_q1_mre_sources_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON qualification_q1_mre_sources
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
ALTER TABLE qualification_q1_mre_sources ENABLE ROW LEVEL SECURITY;
ALTER TABLE qualification_q1_mre_sources FORCE ROW LEVEL SECURITY;
CREATE POLICY qualification_q1_mre_sources_workspace_policy
    ON qualification_q1_mre_sources
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

CREATE OR REPLACE FUNCTION vestrace_create_qualification_q1_mre_source(
    target_evidence_root_id UUID,
    target_workspace_id UUID,
    target_probe_ordinal TEXT,
    target_message_layout TEXT,
    target_tool_choice TEXT,
    target_parallel_tool_calls BOOLEAN,
    target_response_format TEXT,
    target_stream BOOLEAN,
    target_stream_include_usage BOOLEAN,
    target_assistant_tool_call_id TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_evidence_root_id IS NULL OR target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_probe_ordinal NOT IN ('10','20','30','35','40','50','60','70','80','90')
       OR target_message_layout NOT IN ('plain_text', 'multipart_image_marker', 'assistant_tool_call_replay')
       OR target_tool_choice NOT IN ('none', 'named_probe', 'required')
       OR target_response_format NOT IN ('none', 'strict_nonce_json_schema')
       OR (target_assistant_tool_call_id IS NOT NULL
           AND (octet_length(btrim(target_assistant_tool_call_id)) NOT BETWEEN 1 AND 256)) THEN
        RAISE EXCEPTION 'q1 MRE source arguments are malformed' USING ERRCODE = '22023';
    END IF;

    IF NOT EXISTS (
        SELECT 1
          FROM model_request_evidence_roots AS root
          JOIN model_request_evidence_nodes AS probe_node
            ON probe_node.workspace_id = root.workspace_id
           AND probe_node.evidence_root_id = root.id
           AND probe_node.reference_kind = 'qualification_probe'
           AND probe_node.reference_id = root.cause_id
           AND probe_node.safe_ordinal = target_probe_ordinal
         WHERE root.id = target_evidence_root_id
           AND root.workspace_id = target_workspace_id
           AND root.cause_kind = 'qualification_probe'
           AND root.request_kind = CASE target_probe_ordinal
                WHEN '10' THEN 'models_list'
                WHEN '90' THEN 'embeddings'
                ELSE 'chat_completions'
           END
    ) THEN
        RAISE EXCEPTION 'q1 MRE source must name its exact qualification evidence root'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO qualification_q1_mre_sources (
        evidence_root_id, workspace_id, probe_ordinal, message_layout, tool_choice,
        parallel_tool_calls, response_format, stream, stream_include_usage,
        assistant_tool_call_id
    ) VALUES (
        target_evidence_root_id, target_workspace_id, target_probe_ordinal,
        target_message_layout, target_tool_choice, target_parallel_tool_calls,
        target_response_format, target_stream, target_stream_include_usage,
        target_assistant_tool_call_id
    ) ON CONFLICT (evidence_root_id) DO NOTHING;

    IF NOT EXISTS (
        SELECT 1 FROM qualification_q1_mre_sources
         WHERE evidence_root_id = target_evidence_root_id
           AND workspace_id = target_workspace_id
           AND probe_ordinal = target_probe_ordinal
           AND message_layout = target_message_layout
           AND tool_choice = target_tool_choice
           AND parallel_tool_calls = target_parallel_tool_calls
           AND response_format = target_response_format
           AND stream = target_stream
           AND stream_include_usage = target_stream_include_usage
           AND assistant_tool_call_id IS NOT DISTINCT FROM target_assistant_tool_call_id
    ) THEN
        RAISE EXCEPTION 'MODEL_REQUEST_EVIDENCE_CONFLICT' USING ERRCODE = '40001';
    END IF;
    RETURN target_evidence_root_id;
END
$$;

-- This is the only q1 admission before shared provider dispatch.  It holds
-- the job row while it either returns the original durable tuple or creates
-- the one effect intent and Complete structural MRE for a network ordinal.
-- The adapter is deliberately not reachable from this function.
CREATE OR REPLACE FUNCTION vestrace_prepare_qualification_probe_dispatch(
    target_workspace_id UUID,
    target_job_id UUID,
    target_probe_ordinal TEXT,
    target_external_effect_id UUID,
    target_evidence_id UUID
)
RETURNS TABLE(external_effect_id UUID, model_request_evidence_id UUID, intent_payload JSONB)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    job_row qualification_jobs%ROWTYPE;
    binding_row qualification_target_bindings%ROWTYPE;
    existing_effect_id UUID;
    existing_evidence_id UUID;
    ordinal_index INTEGER;
    request_kind_value TEXT;
    message_layout_value TEXT;
    tool_choice_value TEXT;
    parallel_tool_calls_value BOOLEAN;
    response_format_value TEXT;
    stream_value BOOLEAN;
    include_usage_value BOOLEAN;
    replay_call_id_value TEXT;
BEGIN
    IF target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_job_id IS NULL
       OR target_probe_ordinal NOT IN ('10','20','30','35','40','50','60','70','80','90')
       OR ((target_external_effect_id IS NULL) <> (target_evidence_id IS NULL)) THEN
        RAISE EXCEPTION 'qualification dispatch preparation arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT * INTO job_row
      FROM qualification_jobs
     WHERE workspace_id = target_workspace_id AND id = target_job_id
     FOR UPDATE;
    IF NOT FOUND OR job_row.state NOT IN ('requested', 'running') THEN
        RAISE EXCEPTION 'qualification job is not accepting its next dispatch'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO binding_row
      FROM qualification_target_bindings
     WHERE workspace_id = target_workspace_id
       AND qualification_job_id = target_job_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification dispatch requires its exact target binding'
            USING ERRCODE = '23514';
    END IF;

    ordinal_index := array_position(
        ARRAY['00','10','15','20','30','35','40','50','60','70','80','90'],
        target_probe_ordinal
    );
    IF EXISTS (
        SELECT 1
          FROM unnest(ARRAY['00','10','15','20','30','35','40','50','60','70','80','90'])
                   WITH ORDINALITY AS required(ordinal, required_ordinal_index)
         WHERE required.required_ordinal_index < ordinal_index
           AND NOT EXISTS (
               SELECT 1 FROM qualification_probe_results AS prior
                WHERE prior.workspace_id = target_workspace_id
                  AND prior.qualification_job_id = target_job_id
                  AND prior.probe_ordinal = required.ordinal
           )
    ) OR EXISTS (
        SELECT 1 FROM qualification_probe_results AS current_result
         WHERE current_result.workspace_id = target_workspace_id
           AND current_result.qualification_job_id = target_job_id
           AND current_result.probe_ordinal = target_probe_ordinal
    ) THEN
        RAISE EXCEPTION 'qualification dispatch is not the exact next q1 ordinal'
            USING ERRCODE = '23514';
    END IF;

    -- A crash after durable preparation is resumed from this exact tuple.  A
    -- new effect/MRE is never substituted for the original one.
    SELECT root.external_effect_id, root.id
      INTO existing_effect_id, existing_evidence_id
      FROM model_request_evidence_roots AS root
      JOIN model_request_evidence_nodes AS probe_node
        ON probe_node.workspace_id = root.workspace_id
       AND probe_node.evidence_root_id = root.id
       AND probe_node.reference_kind = 'qualification_probe'
       AND probe_node.reference_id = target_job_id
       AND probe_node.safe_ordinal = target_probe_ordinal
     WHERE root.workspace_id = target_workspace_id
       AND root.cause_kind = 'qualification_probe'
       AND root.cause_id = target_job_id
       AND root.qualification_target_binding_id = binding_row.id
     FOR KEY SHARE OF root, probe_node;
    IF FOUND THEN
        RETURN QUERY
        SELECT existing_effect_id, existing_evidence_id, intent.payload
          FROM external_effect_intents AS intent
         WHERE intent.workspace_id = target_workspace_id
           AND intent.id = existing_effect_id;
        RETURN;
    END IF;

    -- The first call in a scoped runtime transaction supplies NULL IDs.  It
    -- leaves the job/ordinal lock held while the existing external-effect
    -- repository writes the immutable intent in that same transaction.  A
    -- concurrent runner blocks here before it can create another intent.
    IF target_external_effect_id IS NULL THEN
        RETURN QUERY SELECT NULL::UUID, NULL::UUID, NULL::JSONB;
        RETURN;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM external_effect_intents AS intent
         WHERE intent.workspace_id = target_workspace_id
           AND intent.id = target_external_effect_id
           AND intent.adapter = 'openai-compatible'
    ) THEN
        RAISE EXCEPTION 'qualification dispatch requires its exact original effect intent'
            USING ERRCODE = '23514';
    END IF;

    request_kind_value := CASE target_probe_ordinal
        WHEN '10' THEN 'models_list'
        WHEN '90' THEN 'embeddings'
        ELSE 'chat_completions'
    END;
    SELECT source.message_layout, source.tool_choice, source.parallel_tool_calls,
           source.response_format, source.stream, source.stream_include_usage,
           source.assistant_tool_call_id
      INTO message_layout_value, tool_choice_value, parallel_tool_calls_value,
           response_format_value, stream_value, include_usage_value, replay_call_id_value
      FROM (VALUES
          ('10','plain_text','none',FALSE,'none',FALSE,FALSE,NULL::TEXT),
          ('20','plain_text','none',FALSE,'none',FALSE,FALSE,NULL::TEXT),
          ('30','plain_text','none',FALSE,'none',TRUE,FALSE,NULL::TEXT),
          ('35','plain_text','none',FALSE,'none',TRUE,TRUE,NULL::TEXT),
          ('40','plain_text','named_probe',FALSE,'none',FALSE,FALSE,NULL::TEXT),
          ('50','assistant_tool_call_replay','none',FALSE,'none',FALSE,FALSE,'call_q1'::TEXT),
          ('60','plain_text','required',TRUE,'none',FALSE,FALSE,NULL::TEXT),
          ('70','plain_text','none',FALSE,'strict_nonce_json_schema',FALSE,FALSE,NULL::TEXT),
          ('80','multipart_image_marker','none',FALSE,'none',FALSE,FALSE,NULL::TEXT),
          ('90','plain_text','none',FALSE,'none',FALSE,FALSE,NULL::TEXT)
      ) AS source(probe_ordinal, message_layout, tool_choice, parallel_tool_calls,
                  response_format, stream, stream_include_usage, assistant_tool_call_id)
     WHERE source.probe_ordinal = target_probe_ordinal;

    INSERT INTO model_request_evidence_roots (
        id, workspace_id, external_effect_id, request_kind,
        qualification_target_binding_id, cause_kind, cause_id
    ) VALUES (
        target_evidence_id, target_workspace_id, target_external_effect_id,
        request_kind_value, binding_row.id, 'qualification_probe', target_job_id
    );
    INSERT INTO model_request_evidence_nodes (
        id, workspace_id, evidence_root_id, ordinal, reference_kind, reference_id,
        reference_version, safe_ordinal
    ) VALUES
        (gen_random_uuid(), target_workspace_id, target_evidence_id, 0,
         'external_effect', target_external_effect_id, NULL, NULL),
        (gen_random_uuid(), target_workspace_id, target_evidence_id, 1,
         'connection_revision', binding_row.connection_revision_id, NULL, NULL),
        (gen_random_uuid(), target_workspace_id, target_evidence_id, 2,
         'qualification_target', binding_row.id, NULL, NULL),
        (gen_random_uuid(), target_workspace_id, target_evidence_id, 3,
         'qualification_probe', target_job_id, NULL, target_probe_ordinal);
    INSERT INTO model_request_evidence_checks (
        id, workspace_id, evidence_root_id, status, missing_reference_count
    ) VALUES (
        gen_random_uuid(), target_workspace_id, target_evidence_id, 'complete', 0
    );
    PERFORM vestrace_create_qualification_q1_mre_source(
        target_evidence_id, target_workspace_id, target_probe_ordinal,
        message_layout_value, tool_choice_value, parallel_tool_calls_value,
        response_format_value, stream_value, include_usage_value, replay_call_id_value
    );
    RETURN QUERY
    SELECT target_external_effect_id, target_evidence_id, intent.payload
      FROM external_effect_intents AS intent
     WHERE intent.workspace_id = target_workspace_id
       AND intent.id = target_external_effect_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_request_qualification_job(
    target_job_id UUID,
    target_binding_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_profile_revision TEXT,
    target_branch TEXT,
    target_credential_revision_id UUID,
    target_credential_slot_id UUID,
    target_activation_guard_id UUID,
    target_expected_slot_version BIGINT,
    target_no_auth_binding_revision_id UUID,
    target_chat_model_revision_id UUID,
    target_embedding_model_revision_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_guard_id UUID;
BEGIN
    IF target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_job_id IS NULL OR target_binding_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_profile_revision <> 'openai-chat-completions-v1/q1'
       OR target_chat_model_revision_id IS NULL OR target_embedding_model_revision_id IS NULL
       OR target_branch NOT IN ('credential', 'no_auth') THEN
        RAISE EXCEPTION 'qualification request arguments are malformed' USING ERRCODE = '22023';
    END IF;

    SELECT guard.id INTO existing_guard_id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification request requires its exact canonical connection guard'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_ensure_connection_execution_guard(
        existing_guard_id, target_workspace_id, target_connection_id
    );
    IF target_branch = 'credential' THEN
        PERFORM vestrace_acquire_credential_lock_chain(
            target_workspace_id, target_connection_id, target_credential_slot_id,
            ARRAY['connection_execution_guard', 'credential_activation_guard',
                  'credential_slot', 'revision_material']::TEXT[]
        );
    END IF;

    PERFORM connection_head.connection_id
      FROM connection_revision_heads AS connection_head
      JOIN connection_revisions AS connection_revision
        ON connection_revision.workspace_id = connection_head.workspace_id
       AND connection_revision.connection_id = connection_head.connection_id
       AND connection_revision.id = connection_head.current_revision_id
      JOIN model_revisions AS chat_revision
        ON chat_revision.workspace_id = connection_revision.workspace_id
       AND chat_revision.id = target_chat_model_revision_id
       AND chat_revision.connection_revision_id = connection_revision.id
       AND chat_revision.kind = 'chat'
      JOIN model_revision_heads AS chat_head
        ON chat_head.workspace_id = chat_revision.workspace_id
       AND chat_head.model_id = chat_revision.model_id
       AND chat_head.current_revision_id = chat_revision.id
      JOIN model_revisions AS embedding_revision
        ON embedding_revision.workspace_id = connection_revision.workspace_id
       AND embedding_revision.id = target_embedding_model_revision_id
       AND embedding_revision.connection_revision_id = connection_revision.id
       AND embedding_revision.kind = 'embedding'
      JOIN model_revision_heads AS embedding_head
        ON embedding_head.workspace_id = embedding_revision.workspace_id
       AND embedding_head.model_id = embedding_revision.model_id
       AND embedding_head.current_revision_id = embedding_revision.id
     WHERE connection_head.workspace_id = target_workspace_id
       AND connection_head.connection_id = target_connection_id
       AND connection_head.current_revision_id = target_connection_revision_id
       AND connection_head.state = 'enabled'
       AND ((target_branch = 'credential'
             AND connection_revision.auth_mode <> 'none'
             AND connection_revision.credential_slot_id = target_credential_slot_id)
            OR (target_branch = 'no_auth'
                AND connection_revision.auth_mode = 'none'
                AND connection_revision.credential_slot_id IS NULL))
     FOR UPDATE OF connection_head, chat_head, embedding_head;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification request must pin exact current connection chat and embedding heads'
            USING ERRCODE = '23514';
    END IF;

    IF (target_branch = 'credential' AND (
            target_credential_revision_id IS NULL OR target_credential_slot_id IS NULL
            OR target_activation_guard_id IS NULL OR target_expected_slot_version IS NULL
            OR target_no_auth_binding_revision_id IS NOT NULL
        )) OR (target_branch = 'no_auth' AND (
            target_credential_revision_id IS NOT NULL OR target_credential_slot_id IS NOT NULL
            OR target_activation_guard_id IS NOT NULL OR target_expected_slot_version IS NOT NULL
            OR target_no_auth_binding_revision_id IS NULL
        )) THEN
        RAISE EXCEPTION 'qualification request authentication binding is not an exact XOR'
            USING ERRCODE = '23514';
    END IF;

    IF target_branch = 'credential' AND NOT EXISTS (
        SELECT 1
          FROM credential_revisions AS revision
          JOIN credential_slots AS slot
            ON slot.workspace_id = revision.workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = revision.credential_slot_id
          JOIN credential_activation_guards AS guard
            ON guard.id = target_activation_guard_id
           AND guard.workspace_id = revision.workspace_id
           AND guard.connection_id = target_connection_id
           AND guard.credential_slot_id = revision.credential_slot_id
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = revision.workspace_id
           AND intent.connection_id = target_connection_id
           AND intent.credential_slot_id = revision.credential_slot_id
           AND intent.credential_revision_id = revision.id
         WHERE revision.workspace_id = target_workspace_id
           AND revision.id = target_credential_revision_id
           AND revision.credential_slot_id = target_credential_slot_id
           AND slot.current_revision_version = target_expected_slot_version
           AND slot.tombstone_version IS NULL
           AND intent.state IN ('candidate', 'active')
    ) THEN
        RAISE EXCEPTION 'qualification request must pin a complete Candidate or Active credential'
            USING ERRCODE = '23514';
    END IF;
    IF target_branch = 'no_auth' AND NOT EXISTS (
        SELECT 1 FROM no_auth_binding_revisions
         WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id
           AND connection_revision_id = target_connection_revision_id
           AND id = target_no_auth_binding_revision_id
    ) THEN
        RAISE EXCEPTION 'qualification request must pin its exact no-auth binding' USING ERRCODE = '23514';
    END IF;

    INSERT INTO qualification_jobs (
        id, workspace_id, connection_revision_id, profile_revision, state
    ) VALUES (
        target_job_id, target_workspace_id, target_connection_revision_id,
        target_profile_revision, 'requested'
    );
    INSERT INTO qualification_target_bindings (
        id, workspace_id, qualification_job_id, connection_id, connection_revision_id,
        branch, credential_revision_id, credential_slot_id,
        credential_activation_guard_id, expected_slot_version,
        no_auth_binding_revision_id, chat_model_revision_id, embedding_model_revision_id
    ) VALUES (
        target_binding_id, target_workspace_id, target_job_id, target_connection_id,
        target_connection_revision_id, target_branch, target_credential_revision_id,
        target_credential_slot_id, target_activation_guard_id, target_expected_slot_version,
        target_no_auth_binding_revision_id, target_chat_model_revision_id,
        target_embedding_model_revision_id
    );
    RETURN target_job_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_qualification_probe_result(
    target_probe_result_id UUID,
    target_workspace_id UUID,
    target_job_id UUID,
    target_probe_ordinal TEXT,
    target_result TEXT,
    target_external_effect_id UUID,
    target_model_request_evidence_id UUID
)
RETURNS TEXT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    ordinal_index INTEGER;
    job_row qualification_jobs%ROWTYPE;
BEGIN
    IF target_probe_result_id IS NULL OR target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_job_id IS NULL OR target_probe_ordinal NOT IN ('00','10','15','20','30','35','40','50','60','70','80','90')
       OR target_result NOT IN ('pass','unsupported_definite','failed_definite','inconclusive_unknown','skipped_prerequisite') THEN
        RAISE EXCEPTION 'qualification probe result arguments are malformed' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO job_row FROM qualification_jobs
     WHERE workspace_id = target_workspace_id AND id = target_job_id FOR UPDATE;
    IF NOT FOUND OR job_row.state NOT IN ('requested', 'running') THEN
        RAISE EXCEPTION 'qualification job is not accepting probe results' USING ERRCODE = '23514';
    END IF;
    ordinal_index := array_position(ARRAY['00','10','15','20','30','35','40','50','60','70','80','90'], target_probe_ordinal);
    IF EXISTS (
        SELECT 1
        FROM unnest(ARRAY['00','10','15','20','30','35','40','50','60','70','80','90']) WITH ORDINALITY AS required(ordinal, required_ordinal_index)
        WHERE required.required_ordinal_index < ordinal_index
          AND NOT EXISTS (
              SELECT 1 FROM qualification_probe_results
               WHERE qualification_job_id = target_job_id AND probe_ordinal = required.ordinal
          )
    ) THEN
        RAISE EXCEPTION 'qualification probe result is out of exact q1 order' USING ERRCODE = '23514';
    END IF;
    IF target_probe_ordinal IN ('00', '15') THEN
        IF target_external_effect_id IS NOT NULL OR target_model_request_evidence_id IS NOT NULL THEN
            RAISE EXCEPTION 'static q1 probes cannot create evidence or effects' USING ERRCODE = '23514';
        END IF;
        IF target_result <> 'pass' THEN
            RAISE EXCEPTION 'static q1 probes must pass before network dispatch' USING ERRCODE = '23514';
        END IF;
    ELSIF target_external_effect_id IS NULL OR target_model_request_evidence_id IS NULL
       OR NOT EXISTS (
           SELECT 1
             FROM provider_dispatch_causes AS cause
            WHERE cause.workspace_id = target_workspace_id
              AND cause.external_effect_id = target_external_effect_id
              AND cause.model_request_evidence_id = target_model_request_evidence_id
              AND cause.cause_kind = 'qualification_probe'
              AND cause.qualification_job_id = target_job_id
              AND cause.qualification_probe_ordinal = target_probe_ordinal
       ) OR NOT EXISTS (
           SELECT 1 FROM model_request_evidence_checks AS check_row
            WHERE check_row.workspace_id = target_workspace_id
              AND check_row.evidence_root_id = target_model_request_evidence_id
              AND check_row.status = 'complete'
              AND check_row.id = (
                  SELECT latest.id FROM model_request_evidence_checks AS latest
                   WHERE latest.workspace_id = target_workspace_id
                     AND latest.evidence_root_id = target_model_request_evidence_id
                   ORDER BY latest.checked_at DESC, latest.id DESC LIMIT 1
              )
       ) OR NOT EXISTS (
           SELECT 1 FROM qualification_q1_mre_sources AS source
            WHERE source.workspace_id = target_workspace_id
              AND source.evidence_root_id = target_model_request_evidence_id
              AND source.probe_ordinal = target_probe_ordinal
       ) THEN
        RAISE EXCEPTION 'network q1 probes require exactly one complete original effect and evidence'
            USING ERRCODE = '23514';
    END IF;
    IF target_result = 'unsupported_definite'
       AND target_probe_ordinal NOT IN ('30','35','40','50','60','70','80') THEN
        RAISE EXCEPTION 'only optional q1 probes may be definitely unsupported' USING ERRCODE = '23514';
    END IF;
    IF target_result = 'skipped_prerequisite' AND NOT (
        (target_probe_ordinal = '35' AND EXISTS (
            SELECT 1 FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id
               AND probe_ordinal = '30' AND result <> 'pass'
        )) OR (target_probe_ordinal IN ('50','60') AND EXISTS (
            SELECT 1 FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id
               AND probe_ordinal = '40' AND result <> 'pass'
        ))
    ) THEN
        RAISE EXCEPTION 'q1 prerequisite skips require the exact failed optional predecessor'
            USING ERRCODE = '23514';
    END IF;
    IF target_result <> 'skipped_prerequisite' AND (
        (target_probe_ordinal = '35' AND NOT EXISTS (
            SELECT 1 FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id
               AND probe_ordinal = '30' AND result = 'pass'
        )) OR (target_probe_ordinal IN ('50','60') AND NOT EXISTS (
            SELECT 1 FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id
               AND probe_ordinal = '40' AND result = 'pass'
        ))
    ) THEN
        RAISE EXCEPTION 'q1 probe requires its exact passing predecessor'
            USING ERRCODE = '23514';
    END IF;
    INSERT INTO qualification_probe_results (
        id, workspace_id, qualification_job_id, probe_ordinal, result,
        external_effect_id, model_request_evidence_id
    ) VALUES (
        target_probe_result_id, target_workspace_id, target_job_id, target_probe_ordinal,
        target_result, target_external_effect_id, target_model_request_evidence_id
    );
    IF target_result = 'inconclusive_unknown' THEN
        UPDATE qualification_jobs SET state = 'inconclusive_unknown', completed_at = NOW()
         WHERE id = target_job_id;
    ELSIF target_result = 'failed_definite' THEN
        UPDATE qualification_jobs SET state = 'failed_definite', completed_at = NOW()
         WHERE id = target_job_id;
    ELSE
        UPDATE qualification_jobs SET state = 'running' WHERE id = target_job_id;
    END IF;
    RETURN (SELECT state FROM qualification_jobs WHERE id = target_job_id);
END
$$;

CREATE OR REPLACE FUNCTION vestrace_cancel_qualification_job(
    target_workspace_id UUID,
    target_job_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_job_id IS NULL THEN
        RAISE EXCEPTION 'qualification cancellation arguments are malformed' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (
        SELECT 1 FROM provider_dispatch_causes AS cause
        WHERE cause.workspace_id = target_workspace_id
          AND cause.qualification_job_id = target_job_id
          AND (
              SELECT transition.status
                FROM external_effect_lifecycle_transitions AS transition
               WHERE transition.effect_id = cause.external_effect_id
                 AND transition.workspace_id = cause.workspace_id
               ORDER BY transition.ordinal DESC
               LIMIT 1
          ) = 'dispatching'
    ) THEN
        RAISE EXCEPTION 'a dispatching qualification probe cannot be cancelled' USING ERRCODE = '23514';
    END IF;
    UPDATE qualification_jobs SET state = 'cancelled', completed_at = NOW()
     WHERE workspace_id = target_workspace_id AND id = target_job_id
       AND state IN ('requested', 'running');
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification job cannot be cancelled in its current state' USING ERRCODE = '23514';
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_recover_qualification_dispatch_unknown(
    target_workspace_id UUID,
    target_job_id UUID,
    target_probe_ordinal TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    current_job_state TEXT;
BEGIN
    IF target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_job_id IS NULL OR target_probe_ordinal IN ('00','15')
       OR target_probe_ordinal NOT IN ('10','20','30','35','40','50','60','70','80','90') THEN
        RAISE EXCEPTION 'qualification recovery arguments are malformed' USING ERRCODE = '22023';
    END IF;
    SELECT state INTO current_job_state
      FROM qualification_jobs
     WHERE workspace_id = target_workspace_id AND id = target_job_id
     FOR UPDATE;
    IF NOT FOUND OR current_job_state NOT IN ('requested','running','inconclusive_unknown')
       OR NOT EXISTS (
           SELECT 1 FROM provider_dispatch_causes AS cause
            WHERE cause.workspace_id = target_workspace_id
              AND cause.qualification_job_id = target_job_id
              AND cause.qualification_probe_ordinal = target_probe_ordinal
              AND (
                  SELECT transition.status
                    FROM external_effect_lifecycle_transitions AS transition
                   WHERE transition.workspace_id = cause.workspace_id
                     AND transition.effect_id = cause.external_effect_id
                   ORDER BY transition.ordinal DESC LIMIT 1
              ) = 'unknown'
       ) THEN
        RAISE EXCEPTION 'qualification recovery requires the original dispatched cause' USING ERRCODE = '23514';
    END IF;
    IF current_job_state = 'inconclusive_unknown' THEN
        RETURN;
    END IF;
    UPDATE qualification_jobs SET state = 'inconclusive_unknown', completed_at = NOW()
     WHERE workspace_id = target_workspace_id AND id = target_job_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_qualification_job(
    target_workspace_id UUID,
    target_job_id UUID,
    target_connection_qualification_revision_id UUID,
    target_chat_model_qualification_revision_id UUID,
    target_embedding_model_qualification_revision_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_row qualification_target_bindings%ROWTYPE;
    job_row qualification_jobs%ROWTYPE;
    immediately_effective BOOLEAN;
    existing_guard_id UUID;
    preliminary RECORD;
BEGIN
    IF target_workspace_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_job_id IS NULL OR target_connection_qualification_revision_id IS NULL
       OR target_chat_model_qualification_revision_id IS NULL
       OR target_embedding_model_qualification_revision_id IS NULL THEN
        RAISE EXCEPTION 'qualification finalization arguments are malformed' USING ERRCODE = '22023';
    END IF;
    -- The immutable binding identifies the permanent connection lock before
    -- any job-local row is locked.  This is the same global ordering used by
    -- rotation and dispatch, so a finalizer cannot publish a tuple after a
    -- competing slot CAS or revision-head advance.
    SELECT binding.connection_id, binding.branch, binding.credential_slot_id
      INTO preliminary
      FROM qualification_target_bindings AS binding
     WHERE binding.workspace_id = target_workspace_id
       AND binding.qualification_job_id = target_job_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification finalization target is no longer current'
            USING ERRCODE = '23514';
    END IF;
    SELECT guard.id INTO existing_guard_id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = preliminary.connection_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification finalization target is no longer current'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_ensure_connection_execution_guard(
        existing_guard_id, target_workspace_id, preliminary.connection_id
    );
    IF preliminary.branch = 'credential' THEN
        PERFORM vestrace_acquire_credential_lock_chain(
            target_workspace_id, preliminary.connection_id, preliminary.credential_slot_id,
            ARRAY['connection_execution_guard', 'credential_activation_guard',
                  'credential_slot', 'revision_material']::TEXT[]
        );
    END IF;

    SELECT * INTO job_row FROM qualification_jobs
     WHERE workspace_id = target_workspace_id AND id = target_job_id FOR UPDATE;
    SELECT * INTO target_row FROM qualification_target_bindings
     WHERE workspace_id = target_workspace_id AND qualification_job_id = target_job_id FOR UPDATE;
    IF NOT FOUND
       OR target_row.connection_id IS DISTINCT FROM preliminary.connection_id
       OR target_row.branch IS DISTINCT FROM preliminary.branch
       OR target_row.credential_slot_id IS DISTINCT FROM preliminary.credential_slot_id THEN
        RAISE EXCEPTION 'qualification finalization target is no longer current'
            USING ERRCODE = '23514';
    END IF;

    PERFORM connection_head.connection_id
      FROM connection_revision_heads AS connection_head
      JOIN connection_revisions AS connection_revision
        ON connection_revision.workspace_id = connection_head.workspace_id
       AND connection_revision.connection_id = connection_head.connection_id
       AND connection_revision.id = connection_head.current_revision_id
      JOIN model_revisions AS chat_revision
        ON chat_revision.workspace_id = connection_revision.workspace_id
       AND chat_revision.id = target_row.chat_model_revision_id
       AND chat_revision.connection_revision_id = connection_revision.id
       AND chat_revision.kind = 'chat'
      JOIN model_revision_heads AS chat_head
        ON chat_head.workspace_id = chat_revision.workspace_id
       AND chat_head.model_id = chat_revision.model_id
       AND chat_head.current_revision_id = chat_revision.id
      JOIN model_revisions AS embedding_revision
        ON embedding_revision.workspace_id = connection_revision.workspace_id
       AND embedding_revision.id = target_row.embedding_model_revision_id
       AND embedding_revision.connection_revision_id = connection_revision.id
       AND embedding_revision.kind = 'embedding'
      JOIN model_revision_heads AS embedding_head
        ON embedding_head.workspace_id = embedding_revision.workspace_id
       AND embedding_head.model_id = embedding_revision.model_id
       AND embedding_head.current_revision_id = embedding_revision.id
     WHERE connection_head.workspace_id = target_workspace_id
       AND connection_head.connection_id = target_row.connection_id
       AND connection_head.current_revision_id = target_row.connection_revision_id
       AND connection_head.state = 'enabled'
       AND ((target_row.branch = 'credential'
             AND connection_revision.auth_mode <> 'none'
             AND connection_revision.credential_slot_id = target_row.credential_slot_id)
            OR (target_row.branch = 'no_auth'
                AND connection_revision.auth_mode = 'none'
                AND connection_revision.credential_slot_id IS NULL))
     FOR UPDATE OF connection_head, chat_head, embedding_head;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'qualification finalization target is no longer current'
            USING ERRCODE = '23514';
    END IF;

    IF target_row.branch = 'credential' THEN
        PERFORM slot.id
          FROM credential_slots AS slot
          JOIN credential_activation_guards AS activation
            ON activation.workspace_id = slot.workspace_id
           AND activation.connection_id = slot.connection_id
           AND activation.credential_slot_id = slot.id
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = slot.current_revision_id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_row.connection_id
           AND slot.id = target_row.credential_slot_id
           AND slot.current_revision_id = target_row.credential_revision_id
           AND slot.current_revision_version = target_row.expected_slot_version
           AND slot.tombstone_version IS NULL
           AND activation.id = target_row.credential_activation_guard_id
           AND intent.state IN ('candidate', 'active')
         FOR UPDATE OF slot, activation, intent;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'qualification finalization target is no longer current'
                USING ERRCODE = '23514';
        END IF;
    ELSE
        PERFORM binding.id
          FROM no_auth_binding_revisions AS binding
          JOIN connection_revisions AS revision
            ON revision.workspace_id = binding.workspace_id
           AND revision.connection_id = binding.connection_id
           AND revision.id = binding.connection_revision_id
         WHERE binding.workspace_id = target_workspace_id
           AND binding.connection_id = target_row.connection_id
           AND binding.connection_revision_id = target_row.connection_revision_id
           AND binding.id = target_row.no_auth_binding_revision_id
           AND revision.auth_mode = 'none'
           AND revision.credential_slot_id IS NULL
         FOR UPDATE OF binding;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'qualification finalization target is no longer current'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF job_row.state <> 'running'
       OR (SELECT array_agg(probe_ordinal ORDER BY array_position(
                    ARRAY['00','10','15','20','30','35','40','50','60','70','80','90'], probe_ordinal
                  ))
             FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id)
          IS DISTINCT FROM ARRAY['00','10','15','20','30','35','40','50','60','70','80','90']::TEXT[]
       OR EXISTS (
            SELECT 1 FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id
               AND result NOT IN ('pass','unsupported_definite','skipped_prerequisite')
       )
       OR EXISTS (
            SELECT 1 FROM qualification_probe_results
             WHERE qualification_job_id = target_job_id
               AND probe_ordinal IN ('00','10','15','20','90') AND result <> 'pass'
       ) OR EXISTS (
            SELECT 1
              FROM qualification_probe_results AS dependent
              JOIN qualification_probe_results AS predecessor
                ON predecessor.workspace_id = dependent.workspace_id
               AND predecessor.qualification_job_id = dependent.qualification_job_id
               AND predecessor.probe_ordinal = CASE dependent.probe_ordinal
                    WHEN '35' THEN '30'
                    WHEN '50' THEN '40'
                    WHEN '60' THEN '40'
               END
             WHERE dependent.workspace_id = target_workspace_id
               AND dependent.qualification_job_id = target_job_id
               AND dependent.probe_ordinal IN ('35','50','60')
               AND (dependent.result = 'skipped_prerequisite')
                   IS DISTINCT FROM (predecessor.result <> 'pass')
       ) OR EXISTS (
            SELECT 1
              FROM qualification_probe_results AS result_row
             WHERE result_row.workspace_id = target_workspace_id
               AND result_row.qualification_job_id = target_job_id
               AND result_row.probe_ordinal NOT IN ('00','15')
               AND (
                   result_row.external_effect_id IS NULL
                   OR result_row.model_request_evidence_id IS NULL
                   OR NOT EXISTS (
                       SELECT 1 FROM provider_dispatch_causes AS cause
                        WHERE cause.workspace_id = target_workspace_id
                          AND cause.external_effect_id = result_row.external_effect_id
                          AND cause.model_request_evidence_id = result_row.model_request_evidence_id
                          AND cause.cause_kind = 'qualification_probe'
                          AND cause.qualification_job_id = target_job_id
                          AND cause.qualification_probe_ordinal = result_row.probe_ordinal
                   ) OR NOT EXISTS (
                       SELECT 1 FROM model_request_evidence_checks AS check_row
                        WHERE check_row.workspace_id = target_workspace_id
                          AND check_row.evidence_root_id = result_row.model_request_evidence_id
                          AND check_row.status = 'complete'
                          AND check_row.id = (
                              SELECT latest.id FROM model_request_evidence_checks AS latest
                               WHERE latest.workspace_id = target_workspace_id
                                 AND latest.evidence_root_id = result_row.model_request_evidence_id
                               ORDER BY latest.checked_at DESC, latest.id DESC LIMIT 1
                          )
                   ) OR NOT EXISTS (
                       SELECT 1 FROM external_effect_receipts AS receipt
                       JOIN external_effect_lifecycle_transitions AS transition
                         ON transition.workspace_id = receipt.workspace_id
                        AND transition.effect_id = receipt.effect_id
                        AND transition.status = receipt.outcome_status
                        AND transition.cause = 'receipt_recorded'
                        AND transition.cause_ref = receipt.id::TEXT
                        WHERE receipt.workspace_id = target_workspace_id
                          AND receipt.effect_id = result_row.external_effect_id
                          AND receipt.outcome_status = 'acknowledged'
                   )
                   OR NOT EXISTS (
                       SELECT 1 FROM qualification_q1_mre_sources AS source
                        WHERE source.workspace_id = target_workspace_id
                          AND source.evidence_root_id = result_row.model_request_evidence_id
                          AND source.probe_ordinal = result_row.probe_ordinal
                   )
               )
       ) THEN
        RAISE EXCEPTION 'qualification finalization requires the exact q1 terminal matrix' USING ERRCODE = '23514';
    END IF;
    INSERT INTO connection_qualification_revisions (
        id, workspace_id, connection_revision_id, qualification_job_id,
        profile_revision, valid_until, capabilities
    ) VALUES (
        target_connection_qualification_revision_id, target_workspace_id,
        target_row.connection_revision_id, target_job_id, job_row.profile_revision,
        NOW() + INTERVAL '900 seconds', ARRAY['models_list','q1']
    );
    INSERT INTO model_qualification_revisions (
        id, workspace_id, model_revision_id, connection_revision_id,
        connection_qualification_revision_id, qualification_job_id, capabilities, valid_until
    ) VALUES
        (target_chat_model_qualification_revision_id, target_workspace_id,
         target_row.chat_model_revision_id, target_row.connection_revision_id,
         target_connection_qualification_revision_id, target_job_id,
         ARRAY['chat','q1'], NOW() + INTERVAL '86400 seconds'),
        (target_embedding_model_qualification_revision_id, target_workspace_id,
         target_row.embedding_model_revision_id, target_row.connection_revision_id,
         target_connection_qualification_revision_id, target_job_id,
         ARRAY['embeddings','q1'], NOW() + INTERVAL '86400 seconds');
    immediately_effective := target_row.branch = 'no_auth' OR EXISTS (
        SELECT 1 FROM credential_key_creation_intents
         WHERE workspace_id = target_workspace_id
           AND connection_id = target_row.connection_id
           AND credential_slot_id = target_row.credential_slot_id
           AND credential_revision_id = target_row.credential_revision_id
           AND state = 'active'
    );
    IF immediately_effective THEN
        INSERT INTO connection_qualification_heads (
            workspace_id, connection_revision_id, current_qualification_revision_id, version
        ) VALUES (
            target_workspace_id, target_row.connection_revision_id,
            target_connection_qualification_revision_id, 1
        ) ON CONFLICT (workspace_id, connection_revision_id) DO UPDATE
          SET current_qualification_revision_id = EXCLUDED.current_qualification_revision_id,
              version = connection_qualification_heads.version + 1, updated_at = NOW();
        INSERT INTO model_qualification_heads (
            workspace_id, model_revision_id, current_qualification_revision_id, version
        ) VALUES
            (target_workspace_id, target_row.chat_model_revision_id,
             target_chat_model_qualification_revision_id, 1),
            (target_workspace_id, target_row.embedding_model_revision_id,
             target_embedding_model_qualification_revision_id, 1)
        ON CONFLICT (workspace_id, model_revision_id) DO UPDATE
          SET current_qualification_revision_id = EXCLUDED.current_qualification_revision_id,
              version = model_qualification_heads.version + 1, updated_at = NOW();
    END IF;
    UPDATE qualification_jobs SET state = 'succeeded', completed_at = NOW()
     WHERE id = target_job_id;
END
$$;

-- Forward replacement of the Task 5 first-activation authority.  The legacy
-- current-head predicate is retained for existing Active evidence.  A q1
-- Candidate instead supplies one already-succeeded, pending tuple; its three
-- heads move only after the exact slot CAS and Candidate->Active transition
-- have succeeded in this same guarded function.
CREATE OR REPLACE FUNCTION vestrace_activate_first_credential(
    target_workspace_id UUID,
    target_connection_id UUID,
    target_execution_guard_id UUID,
    target_activation_guard_id UUID,
    target_slot_id UUID,
    target_credential_revision_id UUID,
    target_credential_intent_id UUID,
    target_connection_qualification_revision_id UUID,
    target_audit_event_id UUID,
    target_expected_slot_version BIGINT
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    slot_row credential_slots%ROWTYPE;
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
    resulting_slot_version BIGINT;
    pending_chat_model_revision_id UUID;
    pending_embedding_model_revision_id UUID;
    pending_connection_revision_id UUID;
    pending_chat_model_qualification_id UUID;
    pending_embedding_model_qualification_id UUID;
    legacy_current_qualification BOOLEAN;
BEGIN
    IF target_workspace_id IS NULL OR target_connection_id IS NULL
       OR target_execution_guard_id IS NULL OR target_activation_guard_id IS NULL
       OR target_slot_id IS NULL OR target_credential_revision_id IS NULL
       OR target_credential_intent_id IS NULL
       OR target_connection_qualification_revision_id IS NULL
       OR target_audit_event_id IS NULL OR target_expected_slot_version IS NULL
       OR target_expected_slot_version < 0 THEN
        RAISE EXCEPTION 'credential first activation arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, target_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    IF NOT EXISTS (
        SELECT 1 FROM connection_execution_guards
         WHERE id = target_execution_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
    ) OR NOT EXISTS (
        SELECT 1 FROM credential_activation_guards
         WHERE id = target_activation_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = target_slot_id
           AND execution_guard_id = target_execution_guard_id
    ) THEN
        RAISE EXCEPTION 'credential activation requires its exact canonical guards'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO slot_row FROM credential_slots
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_slot_id
     FOR UPDATE;
    IF slot_row.current_revision_version <> target_expected_slot_version THEN
        RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE = '40001';
    END IF;
    IF slot_row.current_revision_id IS NOT NULL OR slot_row.tombstone_version IS NOT NULL THEN
        RAISE EXCEPTION 'first activation requires an untombstoned empty credential slot'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO intent_row FROM credential_key_creation_intents
     WHERE id = target_credential_intent_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
       AND credential_revision_id = target_credential_revision_id
     FOR UPDATE;
    IF NOT FOUND OR intent_row.state <> 'candidate' THEN
        RAISE EXCEPTION 'first activation requires its exact Candidate intent'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO occupancy_row FROM credential_guard_occupancies
     WHERE id = intent_row.occupancy_id FOR UPDATE;
    IF occupancy_row.state <> 'candidate'
       OR NOT EXISTS (
           SELECT 1 FROM credential_lifecycle_events
            WHERE intent_id = intent_row.id AND event_kind = 'candidate'
       ) OR EXISTS (
           SELECT 1 FROM credential_association_events WHERE intent_id = intent_row.id
       ) OR EXISTS (
           SELECT 1 FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = intent_row.id
       ) THEN
        RAISE EXCEPTION 'first activation requires a complete uncancelled Candidate association'
            USING ERRCODE = '23514';
    END IF;

    SELECT EXISTS (
        SELECT 1
          FROM connection_revision_heads AS connection_head
          JOIN connection_revisions AS connection_revision
            ON connection_revision.workspace_id = connection_head.workspace_id
           AND connection_revision.connection_id = connection_head.connection_id
           AND connection_revision.id = connection_head.current_revision_id
          JOIN connection_qualification_heads AS qualification_head
            ON qualification_head.workspace_id = connection_head.workspace_id
           AND qualification_head.connection_revision_id = connection_head.current_revision_id
          JOIN connection_qualification_revisions AS qualification_revision
            ON qualification_revision.workspace_id = qualification_head.workspace_id
           AND qualification_revision.connection_revision_id = qualification_head.connection_revision_id
           AND qualification_revision.id = qualification_head.current_qualification_revision_id
         WHERE connection_head.workspace_id = target_workspace_id
           AND connection_head.connection_id = target_connection_id
           AND connection_head.state = 'enabled'
           AND connection_revision.credential_slot_id = target_slot_id
           AND connection_revision.auth_mode <> 'none'
           AND qualification_head.current_qualification_revision_id = target_connection_qualification_revision_id
           AND qualification_revision.id = target_connection_qualification_revision_id
           AND qualification_revision.valid_until > NOW()
    ) INTO legacy_current_qualification;
    IF NOT legacy_current_qualification THEN
        SELECT binding.connection_revision_id,
               binding.chat_model_revision_id,
               binding.embedding_model_revision_id,
               (
                   SELECT model_qualification.id
                     FROM model_qualification_revisions AS model_qualification
                    WHERE model_qualification.workspace_id = binding.workspace_id
                      AND model_qualification.connection_qualification_revision_id = connection_qualification.id
                      AND model_qualification.qualification_job_id = binding.qualification_job_id
                      AND model_qualification.model_revision_id = binding.chat_model_revision_id
                      AND model_qualification.valid_until > NOW()
               ),
               (
                   SELECT model_qualification.id
                     FROM model_qualification_revisions AS model_qualification
                    WHERE model_qualification.workspace_id = binding.workspace_id
                      AND model_qualification.connection_qualification_revision_id = connection_qualification.id
                      AND model_qualification.qualification_job_id = binding.qualification_job_id
                      AND model_qualification.model_revision_id = binding.embedding_model_revision_id
                      AND model_qualification.valid_until > NOW()
               )
          INTO pending_connection_revision_id,
               pending_chat_model_revision_id,
               pending_embedding_model_revision_id,
               pending_chat_model_qualification_id,
               pending_embedding_model_qualification_id
          FROM qualification_target_bindings AS binding
          JOIN qualification_jobs AS job
            ON job.workspace_id = binding.workspace_id
           AND job.id = binding.qualification_job_id
           AND job.state = 'succeeded'
          JOIN connection_qualification_revisions AS connection_qualification
            ON connection_qualification.workspace_id = binding.workspace_id
           AND connection_qualification.qualification_job_id = binding.qualification_job_id
           AND connection_qualification.connection_revision_id = binding.connection_revision_id
           AND connection_qualification.id = target_connection_qualification_revision_id
           AND connection_qualification.valid_until > NOW()
         WHERE binding.workspace_id = target_workspace_id
           AND binding.connection_id = target_connection_id
           AND binding.branch = 'credential'
           AND binding.credential_revision_id = target_credential_revision_id
           AND binding.credential_slot_id = target_slot_id
           AND binding.credential_activation_guard_id = target_activation_guard_id
           AND binding.expected_slot_version = target_expected_slot_version
           AND binding.chat_model_revision_id IS NOT NULL
           AND binding.embedding_model_revision_id IS NOT NULL;
        IF pending_chat_model_qualification_id IS NULL
           OR pending_embedding_model_qualification_id IS NULL THEN
            RAISE EXCEPTION 'first activation requires the exact current connection qualification'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    UPDATE credential_slots
       SET current_revision_id = target_credential_revision_id,
           current_revision_version = current_revision_version + 1,
           updated_at = NOW()
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_slot_id
       AND current_revision_id IS NULL
       AND current_revision_version = target_expected_slot_version
       AND tombstone_version IS NULL
     RETURNING current_revision_version INTO resulting_slot_version;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE = '40001';
    END IF;
    INSERT INTO credential_activation_events (
        id, workspace_id, connection_id, credential_slot_id, credential_revision_id,
        credential_intent_id, connection_qualification_revision_id, event_kind,
        expected_slot_version, resulting_slot_version, audit_event_id
    ) VALUES (
        gen_random_uuid(), target_workspace_id, target_connection_id, target_slot_id,
        target_credential_revision_id, target_credential_intent_id,
        target_connection_qualification_revision_id, 'active', target_expected_slot_version,
        resulting_slot_version, target_audit_event_id
    );
    UPDATE credential_guard_occupancies
       SET state = 'activated', updated_at = NOW()
     WHERE id = occupancy_row.id;
    UPDATE credential_key_creation_intents
       SET state = 'active', updated_at = NOW()
     WHERE id = intent_row.id;

    IF NOT legacy_current_qualification THEN
        INSERT INTO connection_qualification_heads (
            workspace_id, connection_revision_id, current_qualification_revision_id, version
        ) VALUES (
            target_workspace_id, pending_connection_revision_id,
            target_connection_qualification_revision_id, 1
        ) ON CONFLICT (workspace_id, connection_revision_id) DO UPDATE
          SET current_qualification_revision_id = EXCLUDED.current_qualification_revision_id,
              version = connection_qualification_heads.version + 1,
              updated_at = NOW();
        INSERT INTO model_qualification_heads (
            workspace_id, model_revision_id, current_qualification_revision_id, version
        ) VALUES
            (target_workspace_id, pending_chat_model_revision_id,
             pending_chat_model_qualification_id, 1),
            (target_workspace_id, pending_embedding_model_revision_id,
             pending_embedding_model_qualification_id, 1)
        ON CONFLICT (workspace_id, model_revision_id) DO UPDATE
          SET current_qualification_revision_id = EXCLUDED.current_qualification_revision_id,
              version = model_qualification_heads.version + 1,
              updated_at = NOW();
    END IF;
    RETURN resulting_slot_version;
END
$$;

-- Forward replacement of Task 5 rotation.  It keeps the previous Candidate,
-- guard, slot-CAS and P04 embedding-transition predicates, while accepting a
-- succeeded pending q1 tuple for the successor and publishing all three heads
-- atomically with that CAS.
CREATE OR REPLACE FUNCTION vestrace_rotate_credential(
    target_workspace_id UUID,
    target_connection_id UUID,
    target_execution_guard_id UUID,
    target_activation_guard_id UUID,
    target_slot_id UUID,
    target_previous_credential_revision_id UUID,
    target_activated_credential_revision_id UUID,
    target_activated_credential_intent_id UUID,
    target_connection_qualification_revision_id UUID,
    target_audit_event_id UUID,
    target_expected_slot_version BIGINT
)
RETURNS BIGINT
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    slot_row credential_slots%ROWTYPE;
    previous_intent_row credential_key_creation_intents%ROWTYPE;
    activated_intent_row credential_key_creation_intents%ROWTYPE;
    activated_occupancy_row credential_guard_occupancies%ROWTYPE;
    current_connection_revision_id UUID;
    resulting_slot_version BIGINT;
    pending_chat_model_revision_id UUID;
    pending_embedding_model_revision_id UUID;
    pending_chat_model_qualification_id UUID;
    pending_embedding_model_qualification_id UUID;
BEGIN
    IF target_workspace_id IS NULL OR target_connection_id IS NULL
       OR target_execution_guard_id IS NULL OR target_activation_guard_id IS NULL
       OR target_slot_id IS NULL OR target_previous_credential_revision_id IS NULL
       OR target_activated_credential_revision_id IS NULL
       OR target_activated_credential_intent_id IS NULL
       OR target_connection_qualification_revision_id IS NULL
       OR target_audit_event_id IS NULL OR target_expected_slot_version IS NULL
       OR target_expected_slot_version < 1
       OR target_previous_credential_revision_id = target_activated_credential_revision_id THEN
        RAISE EXCEPTION 'credential rotation arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, target_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    IF NOT EXISTS (
        SELECT 1 FROM connection_execution_guards
         WHERE id = target_execution_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
    ) OR NOT EXISTS (
        SELECT 1 FROM credential_activation_guards
         WHERE id = target_activation_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = target_slot_id
           AND execution_guard_id = target_execution_guard_id
    ) THEN
        RAISE EXCEPTION 'credential rotation requires its exact canonical guards'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO slot_row FROM credential_slots
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_slot_id
     FOR UPDATE;
    IF slot_row.current_revision_version <> target_expected_slot_version THEN
        RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE = '40001';
    END IF;
    IF slot_row.current_revision_id IS DISTINCT FROM target_previous_credential_revision_id
       OR slot_row.tombstone_version IS NOT NULL THEN
        RAISE EXCEPTION 'rotation requires the exact current untombstoned credential revision'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO previous_intent_row FROM credential_key_creation_intents
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
       AND credential_revision_id = target_previous_credential_revision_id
     FOR UPDATE;
    IF NOT FOUND OR previous_intent_row.state <> 'active' THEN
        RAISE EXCEPTION 'rotation requires its exact Active predecessor'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO activated_intent_row FROM credential_key_creation_intents
     WHERE id = target_activated_credential_intent_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
       AND credential_revision_id = target_activated_credential_revision_id
     FOR UPDATE;
    IF NOT FOUND OR activated_intent_row.state <> 'candidate' THEN
        RAISE EXCEPTION 'rotation requires its exact successor Candidate intent'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO activated_occupancy_row FROM credential_guard_occupancies
     WHERE id = activated_intent_row.occupancy_id FOR UPDATE;
    IF activated_occupancy_row.state <> 'candidate'
       OR NOT EXISTS (
           SELECT 1 FROM credential_lifecycle_events
            WHERE intent_id = activated_intent_row.id AND event_kind = 'candidate'
       ) OR EXISTS (
           SELECT 1 FROM credential_association_events WHERE intent_id = activated_intent_row.id
       ) OR EXISTS (
           SELECT 1 FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = activated_intent_row.id
       ) THEN
        RAISE EXCEPTION 'rotation requires a complete uncancelled successor Candidate association'
            USING ERRCODE = '23514';
    END IF;
    SELECT connection_head.current_revision_id INTO current_connection_revision_id
      FROM connection_revision_heads AS connection_head
      JOIN connection_revisions AS connection_revision
        ON connection_revision.workspace_id = connection_head.workspace_id
       AND connection_revision.connection_id = connection_head.connection_id
       AND connection_revision.id = connection_head.current_revision_id
      JOIN connection_qualification_heads AS qualification_head
        ON qualification_head.workspace_id = connection_head.workspace_id
       AND qualification_head.connection_revision_id = connection_head.current_revision_id
      JOIN connection_qualification_revisions AS qualification_revision
        ON qualification_revision.workspace_id = qualification_head.workspace_id
       AND qualification_revision.connection_revision_id = qualification_head.connection_revision_id
       AND qualification_revision.id = qualification_head.current_qualification_revision_id
     WHERE connection_head.workspace_id = target_workspace_id
       AND connection_head.connection_id = target_connection_id
       AND connection_head.state = 'enabled'
       AND connection_revision.credential_slot_id = target_slot_id
       AND connection_revision.auth_mode <> 'none'
       AND qualification_head.current_qualification_revision_id = target_connection_qualification_revision_id
       AND qualification_revision.id = target_connection_qualification_revision_id
       AND qualification_revision.valid_until > NOW()
     FOR KEY SHARE OF connection_head, connection_revision, qualification_head, qualification_revision;
    IF NOT FOUND THEN
        SELECT binding.connection_revision_id,
               binding.chat_model_revision_id,
               binding.embedding_model_revision_id,
               (
                   SELECT model_qualification.id
                     FROM model_qualification_revisions AS model_qualification
                    WHERE model_qualification.workspace_id = binding.workspace_id
                      AND model_qualification.connection_qualification_revision_id = connection_qualification.id
                      AND model_qualification.qualification_job_id = binding.qualification_job_id
                      AND model_qualification.model_revision_id = binding.chat_model_revision_id
                      AND model_qualification.valid_until > NOW()
               ),
               (
                   SELECT model_qualification.id
                     FROM model_qualification_revisions AS model_qualification
                    WHERE model_qualification.workspace_id = binding.workspace_id
                      AND model_qualification.connection_qualification_revision_id = connection_qualification.id
                      AND model_qualification.qualification_job_id = binding.qualification_job_id
                      AND model_qualification.model_revision_id = binding.embedding_model_revision_id
                      AND model_qualification.valid_until > NOW()
               )
          INTO current_connection_revision_id,
               pending_chat_model_revision_id,
               pending_embedding_model_revision_id,
               pending_chat_model_qualification_id,
               pending_embedding_model_qualification_id
          FROM qualification_target_bindings AS binding
          JOIN qualification_jobs AS job
            ON job.workspace_id = binding.workspace_id
           AND job.id = binding.qualification_job_id
           AND job.state = 'succeeded'
          JOIN connection_qualification_revisions AS connection_qualification
            ON connection_qualification.workspace_id = binding.workspace_id
           AND connection_qualification.qualification_job_id = binding.qualification_job_id
           AND connection_qualification.connection_revision_id = binding.connection_revision_id
           AND connection_qualification.id = target_connection_qualification_revision_id
           AND connection_qualification.valid_until > NOW()
         WHERE binding.workspace_id = target_workspace_id
           AND binding.connection_id = target_connection_id
           AND binding.branch = 'credential'
           AND binding.credential_revision_id = target_activated_credential_revision_id
           AND binding.credential_slot_id = target_slot_id
           AND binding.credential_activation_guard_id = target_activation_guard_id
           AND binding.expected_slot_version = target_expected_slot_version
           AND binding.chat_model_revision_id IS NOT NULL
           AND binding.embedding_model_revision_id IS NOT NULL;
        IF pending_chat_model_qualification_id IS NULL
           OR pending_embedding_model_qualification_id IS NULL THEN
            RAISE EXCEPTION 'rotation requires the exact current connection qualification'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    IF EXISTS (
        SELECT 1
          FROM model_revision_heads AS model_head
          JOIN model_revisions AS model_revision
            ON model_revision.workspace_id = model_head.workspace_id
           AND model_revision.model_id = model_head.model_id
           AND model_revision.id = model_head.current_revision_id
         WHERE model_head.workspace_id = target_workspace_id
           AND model_revision.connection_revision_id = current_connection_revision_id
           AND model_revision.kind = 'embedding'
    ) THEN
        RAISE EXCEPTION 'rotation requires P04 embedding transition evidence'
            USING ERRCODE = '23514';
    END IF;
    IF EXISTS (
        SELECT 1
          FROM model_revision_heads AS model_head
          JOIN model_revisions AS model_revision
            ON model_revision.workspace_id = model_head.workspace_id
           AND model_revision.model_id = model_head.model_id
           AND model_revision.id = model_head.current_revision_id
          LEFT JOIN model_qualification_heads AS model_qualification_head
            ON model_qualification_head.workspace_id = model_revision.workspace_id
           AND model_qualification_head.model_revision_id = model_revision.id
          LEFT JOIN model_qualification_revisions AS model_qualification_revision
            ON model_qualification_revision.workspace_id = model_qualification_head.workspace_id
           AND model_qualification_revision.model_revision_id = model_qualification_head.model_revision_id
           AND model_qualification_revision.id = model_qualification_head.current_qualification_revision_id
         WHERE model_head.workspace_id = target_workspace_id
           AND model_revision.connection_revision_id = current_connection_revision_id
           AND model_revision.kind <> 'embedding'
           AND (model_qualification_head.model_revision_id IS NULL
                OR model_qualification_revision.valid_until <= NOW())
    ) THEN
        RAISE EXCEPTION 'rotation requires every current non-embedding model qualification'
            USING ERRCODE = '23514';
    END IF;
    UPDATE credential_slots
       SET current_revision_id = target_activated_credential_revision_id,
           current_revision_version = current_revision_version + 1,
           updated_at = NOW()
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_slot_id
       AND current_revision_id = target_previous_credential_revision_id
       AND current_revision_version = target_expected_slot_version
       AND tombstone_version IS NULL
     RETURNING current_revision_version INTO resulting_slot_version;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE = '40001';
    END IF;
    INSERT INTO credential_activation_events (
        id, workspace_id, connection_id, credential_slot_id, credential_revision_id,
        credential_intent_id, connection_qualification_revision_id, event_kind,
        expected_slot_version, resulting_slot_version, audit_event_id
    ) VALUES (
        gen_random_uuid(), target_workspace_id, target_connection_id, target_slot_id,
        target_activated_credential_revision_id, target_activated_credential_intent_id,
        target_connection_qualification_revision_id, 'active', target_expected_slot_version,
        resulting_slot_version, target_audit_event_id
    );
    INSERT INTO credential_rotation_events (
        id, workspace_id, connection_id, credential_slot_id,
        previous_credential_revision_id, activated_credential_revision_id,
        expected_slot_version, resulting_slot_version, audit_event_id
    ) VALUES (
        gen_random_uuid(), target_workspace_id, target_connection_id, target_slot_id,
        target_previous_credential_revision_id, target_activated_credential_revision_id,
        target_expected_slot_version, resulting_slot_version, target_audit_event_id
    );
    UPDATE credential_guard_occupancies
       SET state = 'activated', updated_at = NOW()
     WHERE id = activated_occupancy_row.id;
    UPDATE credential_key_creation_intents
       SET state = 'retired', updated_at = NOW()
     WHERE id = previous_intent_row.id;
    UPDATE credential_key_creation_intents
       SET state = 'active', updated_at = NOW()
     WHERE id = activated_intent_row.id;
    IF pending_chat_model_qualification_id IS NOT NULL THEN
        INSERT INTO connection_qualification_heads (
            workspace_id, connection_revision_id, current_qualification_revision_id, version
        ) VALUES (
            target_workspace_id, current_connection_revision_id,
            target_connection_qualification_revision_id, 1
        ) ON CONFLICT (workspace_id, connection_revision_id) DO UPDATE
          SET current_qualification_revision_id = EXCLUDED.current_qualification_revision_id,
              version = connection_qualification_heads.version + 1,
              updated_at = NOW();
        INSERT INTO model_qualification_heads (
            workspace_id, model_revision_id, current_qualification_revision_id, version
        ) VALUES
            (target_workspace_id, pending_chat_model_revision_id,
             pending_chat_model_qualification_id, 1),
            (target_workspace_id, pending_embedding_model_revision_id,
             pending_embedding_model_qualification_id, 1)
        ON CONFLICT (workspace_id, model_revision_id) DO UPDATE
          SET current_qualification_revision_id = EXCLUDED.current_qualification_revision_id,
              version = model_qualification_heads.version + 1,
              updated_at = NOW();
    END IF;
    RETURN resulting_slot_version;
END
$$;

-- The shared lease authority is forward-replaced, rather than forked for q1.
-- A normal dispatch remains exact-current Active.  The one additional branch
-- is a provider-dispatch cause already bound to a q1 target's pinned Candidate
-- tuple, including its pre-activation slot version and guard.
CREATE OR REPLACE FUNCTION public.vestrace_lock_provider_dispatch_completion_authority(
    target_workspace_id UUID,
    target_external_effect_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_dispatch_transition_id UUID,
    target_concurrency_lease_id UUID
)
RETURNS TABLE(
    cause_kind TEXT,
    qualification_job_id UUID,
    qualification_probe_ordinal TEXT
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS NULL OR target_external_effect_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_dispatch_transition_id IS NULL OR target_concurrency_lease_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider dispatch completion authority arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    PERFORM guard.id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch completion requires its permanent connection guard'
            USING ERRCODE = '23514';
    END IF;

    RETURN QUERY
    SELECT cause.cause_kind, cause.qualification_job_id,
           cause.qualification_probe_ordinal
      FROM provider_dispatch_causes AS cause
      JOIN connection_dispatch_admissions AS admission
        ON admission.workspace_id = cause.workspace_id
       AND admission.external_effect_id = cause.external_effect_id
      JOIN provider_concurrency_leases AS lease
        ON lease.workspace_id = admission.workspace_id
       AND lease.external_effect_id = admission.external_effect_id
      JOIN external_effect_lifecycle_transitions AS dispatch
        ON dispatch.workspace_id = cause.workspace_id
       AND dispatch.effect_id = cause.external_effect_id
       AND dispatch.id = target_dispatch_transition_id
       AND dispatch.status = 'dispatching'
     WHERE cause.workspace_id = target_workspace_id
       AND cause.external_effect_id = target_external_effect_id
       AND admission.connection_id = target_connection_id
       AND admission.connection_revision_id = target_connection_revision_id
       AND lease.id = target_concurrency_lease_id
     FOR KEY SHARE OF cause, admission, lease, dispatch;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch completion authority is not the original admitted dispatch'
            USING ERRCODE = '23514';
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION public.vestrace_lock_provider_dispatch_routing(
    target_workspace_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_effect_id UUID,
    target_evidence_id UUID,
    target_cause_kind TEXT,
    target_run_id UUID,
    target_step_id UUID,
    target_snapshot_id UUID,
    target_qualification_job_id UUID,
    target_qualification_target_id UUID
)
RETURNS TABLE(
    branch TEXT,
    credential_revision_id UUID,
    credential_slot_id UUID,
    credential_activation_guard_id UUID,
    expected_slot_version BIGINT,
    no_auth_binding_revision_id UUID,
    runtime_base_url TEXT,
    auth_mode TEXT,
    revision_credential_slot_id UUID
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_guard_id UUID;
    preliminary RECORD;
    routing RECORD;
BEGIN
    IF target_workspace_id IS NULL OR target_connection_id IS NULL
       OR target_connection_revision_id IS NULL OR target_effect_id IS NULL
       OR target_evidence_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR NOT (
          (target_cause_kind = 'run_step'
           AND target_run_id IS NOT NULL AND target_step_id IS NOT NULL
           AND target_snapshot_id IS NOT NULL
           AND target_qualification_job_id IS NULL
           AND target_qualification_target_id IS NULL)
          OR
          (target_cause_kind = 'qualification_probe'
           AND target_run_id IS NULL AND target_step_id IS NULL
           AND target_snapshot_id IS NULL
           AND target_qualification_job_id IS NOT NULL
           AND target_qualification_target_id IS NOT NULL)
       ) THEN
        RAISE EXCEPTION 'provider dispatch routing arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT guard.id INTO existing_guard_id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch requires its existing permanent connection guard'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_ensure_connection_execution_guard(
        existing_guard_id, target_workspace_id, target_connection_id
    );

    SELECT revision.runtime_base_url, revision.auth_mode,
           revision.credential_slot_id
      INTO preliminary
      FROM connection_revisions AS revision
     WHERE revision.workspace_id = target_workspace_id
       AND revision.connection_id = target_connection_id
       AND revision.id = target_connection_revision_id;
    IF NOT FOUND OR NOT (
        (preliminary.auth_mode = 'none' AND preliminary.credential_slot_id IS NULL)
        OR
        (preliminary.auth_mode <> 'none' AND preliminary.credential_slot_id IS NOT NULL)
    ) THEN
        RAISE EXCEPTION 'provider dispatch connection revision is not routable'
            USING ERRCODE = '23514';
    END IF;

    IF preliminary.credential_slot_id IS NOT NULL THEN
        PERFORM vestrace_acquire_credential_lock_chain(
            target_workspace_id, target_connection_id,
            preliminary.credential_slot_id,
            ARRAY['connection_execution_guard', 'credential_activation_guard',
                  'credential_slot', 'revision_material']::TEXT[]
        );
        PERFORM intent.id
          FROM credential_slots AS slot
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = slot.current_revision_id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = preliminary.credential_slot_id
           AND slot.current_revision_id IS NOT NULL
           AND slot.tombstone_version IS NULL
           AND (
               intent.state = 'active'
               OR (target_cause_kind = 'qualification_probe'
                   AND intent.state = 'candidate')
           )
         FOR UPDATE OF intent;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'provider dispatch credential routing is no longer usable'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    PERFORM vestrace_lock_model_request_evidence_for_reconstruction(
        target_workspace_id, target_evidence_id
    );

    IF target_cause_kind = 'run_step' THEN
        SELECT snapshot.branch, snapshot.credential_revision_id,
               snapshot.credential_slot_id,
               snapshot.credential_activation_guard_id,
               snapshot.expected_slot_version,
               snapshot.no_auth_binding_revision_id,
               revision.runtime_base_url, revision.auth_mode,
               revision.credential_slot_id AS revision_credential_slot_id
          INTO routing
          FROM run_model_binding_snapshots AS run_snapshot
          JOIN run_steps AS step
            ON step.workspace_id = run_snapshot.workspace_id
           AND step.run_id = run_snapshot.run_id
           AND step.id = target_step_id
          JOIN model_binding_snapshots AS snapshot
            ON snapshot.workspace_id = run_snapshot.workspace_id
           AND snapshot.id = run_snapshot.snapshot_id
          JOIN connection_revisions AS revision
            ON revision.workspace_id = snapshot.workspace_id
           AND revision.connection_id = snapshot.connection_id
           AND revision.id = snapshot.connection_revision_id
          JOIN model_request_evidence_roots AS evidence
            ON evidence.workspace_id = snapshot.workspace_id
           AND evidence.id = target_evidence_id
           AND evidence.external_effect_id = target_effect_id
           AND evidence.cause_kind = 'run_step'
           AND evidence.cause_id = target_step_id
           AND evidence.binding_snapshot_id = snapshot.id
         WHERE run_snapshot.workspace_id = target_workspace_id
           AND run_snapshot.run_id = target_run_id
           AND run_snapshot.snapshot_id = target_snapshot_id
           AND snapshot.connection_id = target_connection_id
           AND snapshot.connection_revision_id = target_connection_revision_id;
    ELSE
        SELECT binding.branch, binding.credential_revision_id,
               binding.credential_slot_id,
               binding.credential_activation_guard_id,
               binding.expected_slot_version,
               binding.no_auth_binding_revision_id,
               revision.runtime_base_url, revision.auth_mode,
               revision.credential_slot_id AS revision_credential_slot_id
          INTO routing
          FROM qualification_target_bindings AS binding
          JOIN connection_revisions AS revision
            ON revision.workspace_id = binding.workspace_id
           AND revision.connection_id = binding.connection_id
           AND revision.id = binding.connection_revision_id
          JOIN model_request_evidence_roots AS evidence
            ON evidence.workspace_id = binding.workspace_id
           AND evidence.id = target_evidence_id
           AND evidence.external_effect_id = target_effect_id
           AND evidence.cause_kind = 'qualification_probe'
           AND evidence.cause_id = binding.qualification_job_id
           AND evidence.qualification_target_binding_id = binding.id
         WHERE binding.workspace_id = target_workspace_id
           AND binding.qualification_job_id = target_qualification_job_id
           AND binding.id = target_qualification_target_id
           AND binding.connection_id = target_connection_id
           AND binding.connection_revision_id = target_connection_revision_id;
    END IF;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch routing tuple is not current and exact'
            USING ERRCODE = '23514';
    END IF;

    IF routing.branch = 'credential' THEN
        IF routing.auth_mode = 'none'
           OR routing.revision_credential_slot_id IS DISTINCT FROM routing.credential_slot_id THEN
            RAISE EXCEPTION 'provider dispatch credential routing is no longer current and usable'
                USING ERRCODE = '23514';
        END IF;
        PERFORM 1
          FROM credential_slots AS slot
          JOIN credential_activation_guards AS activation
            ON activation.workspace_id = slot.workspace_id
           AND activation.connection_id = slot.connection_id
           AND activation.credential_slot_id = slot.id
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = slot.current_revision_id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = routing.credential_slot_id
           AND slot.id = preliminary.credential_slot_id
           AND slot.current_revision_id = routing.credential_revision_id
           AND slot.current_revision_version = routing.expected_slot_version
           AND slot.tombstone_version IS NULL
           AND activation.id = routing.credential_activation_guard_id
           AND (
               intent.state = 'active'
               OR (target_cause_kind = 'qualification_probe'
                   AND intent.state = 'candidate')
           );
        IF NOT FOUND THEN
            RAISE EXCEPTION 'provider dispatch credential routing is no longer current and usable'
                USING ERRCODE = '23514';
        END IF;
    ELSIF routing.branch <> 'no_auth' OR routing.auth_mode <> 'none'
          OR routing.revision_credential_slot_id IS NOT NULL
          OR routing.no_auth_binding_revision_id IS NULL
          OR preliminary.credential_slot_id IS NOT NULL THEN
        RAISE EXCEPTION 'provider dispatch no-auth routing is not exact'
            USING ERRCODE = '23514';
    END IF;

    RETURN QUERY SELECT
        routing.branch::TEXT,
        routing.credential_revision_id::UUID,
        routing.credential_slot_id::UUID,
        routing.credential_activation_guard_id::UUID,
        routing.expected_slot_version::BIGINT,
        routing.no_auth_binding_revision_id::UUID,
        routing.runtime_base_url::TEXT,
        routing.auth_mode::TEXT,
        routing.revision_credential_slot_id::UUID;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_issue_credential_dispatch_lease(
    target_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_external_effect_id UUID,
    target_authorization_id UUID,
    target_credential_slot_id UUID,
    target_credential_revision_id UUID,
    target_activation_guard_id UUID,
    target_destination_authority TEXT,
    target_auth_mode TEXT,
    target_expires_at TIMESTAMPTZ
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    latest_effect_state TEXT;
    latest_effect_cause_ref TEXT;
    active_credential BOOLEAN;
    q1_candidate_credential BOOLEAN;
BEGIN
    IF target_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_external_effect_id IS NULL
       OR target_authorization_id IS NULL OR target_credential_slot_id IS NULL
       OR target_credential_revision_id IS NULL OR target_activation_guard_id IS NULL
       OR target_destination_authority IS NULL OR target_auth_mode IS NULL
       OR target_expires_at IS NULL
       OR target_destination_authority <> lower(btrim(target_destination_authority))
       OR target_destination_authority = ''
       OR target_auth_mode NOT IN ('bearer', 'api_key', 'x_api_key') THEN
        RAISE EXCEPTION 'credential dispatch lease arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_expires_at <= NOW() THEN
        RAISE EXCEPTION 'credential dispatch lease must be issued live'
            USING ERRCODE = '23514';
    END IF;

    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, target_credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    IF NOT EXISTS (
        SELECT 1 FROM credential_activation_guards
         WHERE id = target_activation_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = target_credential_slot_id
    ) THEN
        RAISE EXCEPTION 'credential dispatch lease requires the exact current Active credential'
            USING ERRCODE = '23514';
    END IF;

    SELECT EXISTS (
        SELECT 1
          FROM credential_slots AS slot
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = target_credential_revision_id
           AND intent.state = 'active'
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = target_credential_slot_id
           AND slot.current_revision_id = target_credential_revision_id
           AND slot.tombstone_version IS NULL
    ) INTO active_credential;
    SELECT EXISTS (
        SELECT 1
          FROM credential_slots AS slot
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = target_credential_revision_id
           AND intent.state = 'candidate'
          JOIN qualification_target_bindings AS binding
            ON binding.workspace_id = intent.workspace_id
           AND binding.connection_id = intent.connection_id
           AND binding.branch = 'credential'
           AND binding.credential_revision_id = intent.credential_revision_id
           AND binding.credential_slot_id = intent.credential_slot_id
           AND binding.credential_activation_guard_id = target_activation_guard_id
           AND binding.expected_slot_version = slot.current_revision_version
          JOIN provider_dispatch_causes AS cause
            ON cause.workspace_id = binding.workspace_id
           AND cause.external_effect_id = target_external_effect_id
           AND cause.cause_kind = 'qualification_probe'
           AND cause.qualification_target_binding_id = binding.id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = target_credential_slot_id
           AND slot.tombstone_version IS NULL
    ) INTO q1_candidate_credential;
    IF NOT active_credential AND NOT q1_candidate_credential THEN
        RAISE EXCEPTION 'credential dispatch lease requires the exact current Active credential'
            USING ERRCODE = '23514';
    END IF;

    PERFORM 1 FROM external_effect_intents
     WHERE id = target_external_effect_id AND workspace_id = target_workspace_id
     FOR UPDATE;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM external_effect_authorizations
         WHERE id = target_authorization_id
           AND effect_id = target_external_effect_id
           AND workspace_id = target_workspace_id
           AND result = 'allow'
    ) THEN
        RAISE EXCEPTION 'credential dispatch lease requires its exact allowed authorization'
            USING ERRCODE = '23514';
    END IF;
    SELECT status, cause_ref
      INTO latest_effect_state, latest_effect_cause_ref
      FROM external_effect_lifecycle_transitions
     WHERE effect_id = target_external_effect_id AND workspace_id = target_workspace_id
     ORDER BY ordinal DESC
     LIMIT 1
     FOR KEY SHARE;
    IF NOT FOUND OR latest_effect_state <> 'authorized'
       OR latest_effect_cause_ref <> target_authorization_id::TEXT THEN
        RAISE EXCEPTION 'credential dispatch lease requires an effect currently authorized by that decision'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO credential_dispatch_leases (
        id, workspace_id, connection_id, external_effect_id, authorization_id,
        credential_revision_id, credential_slot_id, credential_activation_guard_id,
        destination_authority, auth_mode, issued_at, expires_at
    ) VALUES (
        target_lease_id, target_workspace_id, target_connection_id,
        target_external_effect_id, target_authorization_id,
        target_credential_revision_id, target_credential_slot_id, target_activation_guard_id,
        target_destination_authority, target_auth_mode, NOW(), target_expires_at
    );
    RETURN target_lease_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_consume_credential_dispatch_lease(
    target_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_external_effect_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    lease_row credential_dispatch_leases%ROWTYPE;
    preliminary_slot_id UUID;
    latest_effect_state TEXT;
    latest_effect_cause_ref TEXT;
    active_credential BOOLEAN;
    q1_candidate_credential BOOLEAN;
BEGIN
    IF target_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_external_effect_id IS NULL THEN
        RAISE EXCEPTION 'credential dispatch lease consumption arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    -- This ordinary immutable lookup is deliberately lock-free.  It learns the
    -- slot needed for the permanent outer guard without taking a lease lock
    -- before that guard chain.
    SELECT credential_slot_id INTO preliminary_slot_id
      FROM credential_dispatch_leases
     WHERE id = target_lease_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND external_effect_id = target_external_effect_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential dispatch lease is already terminal or absent'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, preliminary_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO lease_row FROM credential_dispatch_leases AS lease
     WHERE lease.id = target_lease_id
       AND lease.workspace_id = target_workspace_id
       AND lease.connection_id = target_connection_id
       AND lease.external_effect_id = target_external_effect_id
     FOR UPDATE OF lease;
    IF NOT FOUND OR lease_row.terminal_state IS NOT NULL THEN
        RAISE EXCEPTION 'credential dispatch lease is already terminal or absent'
            USING ERRCODE = '23514';
    END IF;
    IF lease_row.expires_at <= NOW() THEN
        RAISE EXCEPTION 'credential dispatch lease is expired'
            USING ERRCODE = '23514';
    END IF;
    SELECT EXISTS (
        SELECT 1
          FROM credential_slots AS slot
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = lease_row.credential_revision_id
           AND intent.state = 'active'
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = lease_row.credential_slot_id
           AND slot.current_revision_id = lease_row.credential_revision_id
           AND slot.tombstone_version IS NULL
    ) INTO active_credential;
    SELECT EXISTS (
        SELECT 1
          FROM credential_slots AS slot
          JOIN credential_key_creation_intents AS intent
            ON intent.workspace_id = slot.workspace_id
           AND intent.connection_id = slot.connection_id
           AND intent.credential_slot_id = slot.id
           AND intent.credential_revision_id = lease_row.credential_revision_id
           AND intent.state = 'candidate'
          JOIN qualification_target_bindings AS binding
            ON binding.workspace_id = intent.workspace_id
           AND binding.connection_id = intent.connection_id
           AND binding.branch = 'credential'
           AND binding.credential_revision_id = intent.credential_revision_id
           AND binding.credential_slot_id = intent.credential_slot_id
           AND binding.credential_activation_guard_id = lease_row.credential_activation_guard_id
           AND binding.expected_slot_version = slot.current_revision_version
          JOIN provider_dispatch_causes AS cause
            ON cause.workspace_id = binding.workspace_id
           AND cause.external_effect_id = lease_row.external_effect_id
           AND cause.cause_kind = 'qualification_probe'
           AND cause.qualification_target_binding_id = binding.id
         WHERE slot.workspace_id = target_workspace_id
           AND slot.connection_id = target_connection_id
           AND slot.id = lease_row.credential_slot_id
           AND slot.tombstone_version IS NULL
    ) INTO q1_candidate_credential;
    IF NOT active_credential AND NOT q1_candidate_credential THEN
        RAISE EXCEPTION 'credential dispatch lease no longer names the current Active credential'
            USING ERRCODE = '23514';
    END IF;
    SELECT status, cause_ref
      INTO latest_effect_state, latest_effect_cause_ref
      FROM external_effect_lifecycle_transitions
     WHERE effect_id = target_external_effect_id AND workspace_id = target_workspace_id
     ORDER BY ordinal DESC
     LIMIT 1
     FOR KEY SHARE;
    IF NOT FOUND OR latest_effect_state <> 'authorized'
       OR latest_effect_cause_ref <> lease_row.authorization_id::TEXT THEN
        RAISE EXCEPTION 'credential dispatch lease effect is no longer authorized for dispatch'
            USING ERRCODE = '23514';
    END IF;

    UPDATE credential_dispatch_leases
       SET terminal_state = 'consumed_for_dispatch', consumed_at = NOW()
     WHERE id = lease_row.id;
    RETURN lease_row.id;
END
$$;

SELECT vestrace_assign_p03_table_owner('qualification_target_bindings'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('qualification_probe_results'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('qualification_q1_mre_sources'::REGCLASS);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_request_qualification_job(UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_record_qualification_probe_result(UUID, UUID, UUID, TEXT, TEXT, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_cancel_qualification_job(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_recover_qualification_dispatch_unknown(UUID, UUID, TEXT)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_finalize_qualification_job(UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_create_qualification_q1_mre_source(UUID, UUID, TEXT, TEXT, TEXT, BOOLEAN, TEXT, BOOLEAN, BOOLEAN, TEXT)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_prepare_qualification_probe_dispatch(UUID, UUID, TEXT, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_lock_provider_dispatch_completion_authority(UUID, UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_activate_first_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE
);
SELECT vestrace_assign_p03_function_owner(
    'vestrace_rotate_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE
);
