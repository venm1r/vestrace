-- P04 Task 14C: atomic delivery acceptance and provisional output-key
-- preparation.  Provider dispatch and result publication remain fenced by
-- 0192 and are deliberately absent here.

DO $$
BEGIN
    PERFORM vestrace_prepare_p04_output_key_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN
        RAISE EXCEPTION 'P04 output-key ownership hand-back must be provisioned before runtime migration'
            USING ERRCODE='42501';
    END IF;
END $$;

-- The source-membership FK must bind a blocker to the same workspace.  The
-- provisioning bridge temporarily hands this exact guarded table to the
-- migrator; ownership is returned below before any command is exposed.
ALTER TABLE material_erasure_blockers
    ADD CONSTRAINT material_erasure_blockers_id_workspace_key UNIQUE (id, workspace_id);

CREATE TABLE embedding_delivery_acceptance_receipts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    principal_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    job_id UUID NOT NULL,
    space_registration_id UUID NOT NULL,
    job_kind TEXT NOT NULL CHECK (job_kind='delivery'),
    model_binding_snapshot_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    model_request_evidence_id UUID NOT NULL,
    retries_unknown_embedding_job_id UUID,
    expected_predecessor_version BIGINT CHECK (expected_predecessor_version > 0),
    request_tuple JSONB NOT NULL CHECK (jsonb_typeof(request_tuple)='object'),
    accepted_sources JSONB NOT NULL CHECK (jsonb_typeof(accepted_sources)='array'),
    accepted_outputs JSONB NOT NULL CHECK (jsonb_typeof(accepted_outputs)='array'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_delivery_acceptance_receipts_workspace_id_key
        UNIQUE (id,workspace_id),
    CONSTRAINT embedding_delivery_acceptance_receipts_idempotency_key
        UNIQUE (workspace_id,idempotency_key),
    CONSTRAINT embedding_delivery_acceptance_receipts_job_key
        UNIQUE (workspace_id,job_id),
    CONSTRAINT embedding_delivery_acceptance_receipts_predecessor_pair CHECK (
        (retries_unknown_embedding_job_id IS NULL AND expected_predecessor_version IS NULL)
        OR (retries_unknown_embedding_job_id IS NOT NULL AND expected_predecessor_version IS NOT NULL)
    ),
    CONSTRAINT embedding_delivery_acceptance_receipts_principal_fkey
        FOREIGN KEY (workspace_id,principal_id) REFERENCES principals(workspace_id,id) ON DELETE RESTRICT,
    CONSTRAINT embedding_delivery_acceptance_receipts_job_fkey
        FOREIGN KEY (workspace_id,job_id) REFERENCES embedding_jobs(workspace_id,id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT embedding_delivery_acceptance_receipts_space_fkey
        FOREIGN KEY (workspace_id,space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id,id) ON DELETE RESTRICT,
    CONSTRAINT embedding_delivery_acceptance_receipts_snapshot_fkey
        FOREIGN KEY (workspace_id,model_binding_snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id,id) ON DELETE RESTRICT,
    CONSTRAINT embedding_delivery_acceptance_receipts_effect_fkey
        FOREIGN KEY (external_effect_id,workspace_id)
        REFERENCES external_effect_intents(id,workspace_id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT embedding_delivery_acceptance_receipts_mre_fkey
        FOREIGN KEY (workspace_id,model_request_evidence_id)
        REFERENCES model_request_evidence_roots(workspace_id,id) ON DELETE RESTRICT
);

CREATE TABLE embedding_delivery_source_memberships (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal>=0),
    source_material_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal>=0),
    intent_id UUID NOT NULL,
    blocker_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(workspace_id,job_id,source_ordinal,output_ordinal),
    CONSTRAINT embedding_delivery_source_memberships_blocker_key UNIQUE(workspace_id,blocker_id),
    CONSTRAINT embedding_delivery_source_memberships_source_fkey
        FOREIGN KEY(source_material_id,workspace_id)
        REFERENCES content_materials(id,workspace_id) ON DELETE RESTRICT,
    CONSTRAINT embedding_delivery_source_memberships_output_fkey
        FOREIGN KEY(workspace_id,job_id,output_ordinal)
        REFERENCES embedding_job_material_intents(workspace_id,job_id,output_ordinal) ON DELETE RESTRICT,
    CONSTRAINT embedding_delivery_source_memberships_intent_fkey
        FOREIGN KEY(intent_id,workspace_id)
        REFERENCES material_key_creation_intents(id,workspace_id) ON DELETE RESTRICT,
    CONSTRAINT embedding_delivery_source_memberships_blocker_fkey
        FOREIGN KEY(blocker_id,workspace_id)
        REFERENCES material_erasure_blockers(id,workspace_id) ON DELETE RESTRICT
);

CREATE TABLE embedding_job_pre_dispatch_retirement_authorities (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    job_id UUID NOT NULL,
    expected_version BIGINT NOT NULL CHECK(expected_version>0),
    idempotency_key TEXT NOT NULL CHECK(btrim(idempotency_key)<>''),
    terminal_state TEXT NOT NULL CHECK(terminal_state IN ('cancelled','failed_definite')),
    evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('cancellation_authorization','external_effect_denied','admission_timeout')),
    evidence_id UUID NOT NULL,
    authorization_policy_version TEXT,
    authorization_capability TEXT,
    authorization_operation TEXT,
    authorization_resource_scope TEXT,
    authorization_risk TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_job_pre_dispatch_retirement_authorities_workspace_id_key
        UNIQUE(id,workspace_id),
    CONSTRAINT embedding_job_pre_dispatch_retirement_authorities_job_key
        UNIQUE(workspace_id,job_id),
    CONSTRAINT embedding_job_pre_dispatch_retirement_authorities_idempotency_key
        UNIQUE(workspace_id,idempotency_key),
    CONSTRAINT embedding_job_pre_dispatch_retirement_authorities_principal_fkey
        FOREIGN KEY(workspace_id,principal_id) REFERENCES principals(workspace_id,id) ON DELETE RESTRICT,
    CONSTRAINT embedding_job_pre_dispatch_retirement_authorities_job_fkey
        FOREIGN KEY(workspace_id,job_id) REFERENCES embedding_jobs(workspace_id,id) ON DELETE RESTRICT
);

CREATE TABLE embedding_output_key_retirement_requests (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK(output_ordinal>=0),
    intent_id UUID NOT NULL,
    termination_receipt_id UUID NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(workspace_id,intent_id),
    CONSTRAINT embedding_output_key_retirement_requests_output_key
        UNIQUE(workspace_id,job_id,output_ordinal),
    CONSTRAINT embedding_output_key_retirement_requests_output_fkey
        FOREIGN KEY(workspace_id,job_id,output_ordinal)
        REFERENCES embedding_job_material_intents(workspace_id,job_id,output_ordinal) ON DELETE RESTRICT,
    CONSTRAINT embedding_output_key_retirement_requests_intent_fkey
        FOREIGN KEY(intent_id,workspace_id)
        REFERENCES material_key_creation_intents(id,workspace_id) ON DELETE RESTRICT,
    CONSTRAINT embedding_output_key_retirement_requests_authority_fkey
        FOREIGN KEY(termination_receipt_id,workspace_id)
        REFERENCES embedding_job_pre_dispatch_retirement_authorities(id,workspace_id)
        ON DELETE RESTRICT
);

CREATE TABLE embedding_output_key_receipts (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK(output_ordinal>=0),
    intent_id UUID NOT NULL,
    vault_receipt UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(workspace_id,job_id,output_ordinal),
    CONSTRAINT embedding_output_key_receipts_intent_key UNIQUE(workspace_id,intent_id),
    CONSTRAINT embedding_output_key_receipts_output_fkey
        FOREIGN KEY(workspace_id,job_id,output_ordinal)
        REFERENCES embedding_job_material_intents(workspace_id,job_id,output_ordinal) ON DELETE RESTRICT,
    CONSTRAINT embedding_output_key_receipts_intent_fkey
        FOREIGN KEY(intent_id,workspace_id)
        REFERENCES material_key_creation_intents(id,workspace_id) ON DELETE RESTRICT
);

CREATE TABLE embedding_output_key_retirement_receipts (
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK(output_ordinal>=0),
    intent_id UUID NOT NULL,
    erasure_receipt UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(workspace_id,job_id,output_ordinal),
    CONSTRAINT embedding_output_key_retirement_receipts_intent_key
        UNIQUE(workspace_id,intent_id),
    CONSTRAINT embedding_output_key_retirement_receipts_receipt_key
        UNIQUE(erasure_receipt),
    CONSTRAINT embedding_output_key_retirement_receipts_output_fkey
        FOREIGN KEY(workspace_id,job_id,output_ordinal)
        REFERENCES embedding_job_material_intents(workspace_id,job_id,output_ordinal) ON DELETE RESTRICT,
    CONSTRAINT embedding_output_key_retirement_receipts_intent_fkey
        FOREIGN KEY(intent_id,workspace_id)
        REFERENCES material_key_creation_intents(id,workspace_id) ON DELETE RESTRICT,
    CONSTRAINT embedding_output_key_retirement_receipts_request_fkey
        FOREIGN KEY(workspace_id,job_id,output_ordinal)
        REFERENCES embedding_output_key_retirement_requests(workspace_id,job_id,output_ordinal)
        ON DELETE RESTRICT
);

DO $$ DECLARE target REGCLASS; BEGIN
    FOREACH target IN ARRAY ARRAY[
        'public.embedding_delivery_acceptance_receipts'::REGCLASS,
        'public.embedding_delivery_source_memberships'::REGCLASS,
        'public.embedding_job_pre_dispatch_retirement_authorities'::REGCLASS,
        'public.embedding_output_key_retirement_requests'::REGCLASS,
        'public.embedding_output_key_receipts'::REGCLASS,
        'public.embedding_output_key_retirement_receipts'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY',target);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY',target);
        EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC',target);
    END LOOP;
END $$;
CREATE POLICY embedding_delivery_acceptance_receipts_workspace_policy ON embedding_delivery_acceptance_receipts
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
CREATE POLICY embedding_delivery_source_memberships_workspace_policy ON embedding_delivery_source_memberships
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
CREATE POLICY embedding_job_pre_dispatch_retirement_authorities_workspace_policy ON embedding_job_pre_dispatch_retirement_authorities
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
CREATE POLICY embedding_output_key_retirement_requests_workspace_policy ON embedding_output_key_retirement_requests
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
CREATE POLICY embedding_output_key_receipts_workspace_policy ON embedding_output_key_receipts
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);
CREATE POLICY embedding_output_key_retirement_receipts_workspace_policy ON embedding_output_key_retirement_receipts
    USING(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID)
    WITH CHECK(workspace_id=NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID);

CREATE TRIGGER embedding_delivery_acceptance_receipts_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_delivery_acceptance_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_delivery_source_memberships_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_delivery_source_memberships
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_job_pre_dispatch_retirement_authorities_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_job_pre_dispatch_retirement_authorities
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_output_key_retirement_requests_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_output_key_retirement_requests
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_output_key_receipts_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_output_key_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_output_key_retirement_receipts_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_output_key_retirement_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

CREATE OR REPLACE FUNCTION vestrace_validate_delivery_source_membership()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE member embedding_delivery_source_memberships%ROWTYPE;
BEGIN
    IF TG_OP='DELETE' THEN member:=OLD; ELSE member:=NEW; END IF;
    IF NOT EXISTS(
        SELECT 1
          FROM embedding_delivery_acceptance_receipts receipt
          JOIN model_request_evidence_nodes node
            ON node.workspace_id=receipt.workspace_id
           AND node.evidence_root_id=receipt.model_request_evidence_id
           AND node.ordinal=member.source_ordinal
           AND node.reference_kind='governed_input_material'
           AND node.reference_id=member.source_material_id
          JOIN embedding_job_material_intents output
            ON output.workspace_id=member.workspace_id AND output.job_id=member.job_id
           AND output.output_ordinal=member.output_ordinal AND output.intent_id=member.intent_id
          JOIN material_erasure_blockers blocker
            ON blocker.workspace_id=member.workspace_id AND blocker.id=member.blocker_id
           AND blocker.target_kind='content'
           AND blocker.content_material_id=member.source_material_id
           AND blocker.blocker_kind='intent'
         WHERE receipt.workspace_id=member.workspace_id AND receipt.job_id=member.job_id
    ) THEN
        RAISE EXCEPTION 'delivery source membership requires its exact MRE, output and blocker tuple'
            USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER embedding_delivery_source_memberships_exact
    AFTER INSERT OR UPDATE OR DELETE ON embedding_delivery_source_memberships
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_delivery_source_membership();

-- This is the state-first gate.  A fresh immutable receipt is inserted before
-- effect/job/output/audit mutation.  Unique-key races block on that insertion;
-- the loser then reads and returns the installed receipt.
CREATE OR REPLACE FUNCTION vestrace_begin_delivery_embedding_outputs(
    target_receipt UUID,target_workspace UUID,target_principal UUID,target_idempotency TEXT,
    target_job UUID,target_space UUID,target_kind TEXT,target_snapshot UUID,target_effect UUID,
    target_mre UUID,target_predecessor UUID,target_expected_predecessor BIGINT,
    target_request JSONB,target_outputs JSONB
) RETURNS TABLE(receipt_id UUID,job_id UUID,accepted_outputs JSONB,created BOOLEAN)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE prior embedding_delivery_acceptance_receipts%ROWTYPE; source_tuple JSONB;
        inserted INTEGER; sources_unique BOOLEAN;
BEGIN
    IF target_receipt IS NULL OR target_workspace IS NULL OR target_principal IS NULL
       OR target_idempotency IS NULL OR btrim(target_idempotency)=''
       OR target_job IS NULL OR target_space IS NULL OR target_kind IS DISTINCT FROM 'delivery'
       OR target_snapshot IS NULL OR target_effect IS NULL OR target_mre IS NULL
       OR jsonb_typeof(target_request) IS DISTINCT FROM 'object'
       OR jsonb_typeof(target_outputs) IS DISTINCT FROM 'array'
       OR jsonb_array_length(target_outputs)=0
       OR ((target_predecessor IS NULL) <> (target_expected_predecessor IS NULL)) THEN
        RAISE EXCEPTION 'delivery acceptance arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);
    IF NULLIF(current_setting('vestrace.principal_id',true),'') IS DISTINCT FROM target_principal::TEXT THEN
        RAISE EXCEPTION 'delivery acceptance principal does not match the scoped request' USING ERRCODE='42501';
    END IF;
    SELECT * INTO prior FROM embedding_delivery_acceptance_receipts
     WHERE workspace_id=target_workspace AND idempotency_key=target_idempotency FOR UPDATE;
    IF FOUND THEN
        IF prior.id=target_receipt AND prior.principal_id=target_principal
           AND prior.job_id=target_job AND prior.space_registration_id=target_space
           AND prior.job_kind=target_kind AND prior.model_binding_snapshot_id=target_snapshot
           AND prior.external_effect_id=target_effect AND prior.model_request_evidence_id=target_mre
           AND prior.retries_unknown_embedding_job_id IS NOT DISTINCT FROM target_predecessor
           AND prior.expected_predecessor_version IS NOT DISTINCT FROM target_expected_predecessor
           AND prior.request_tuple=target_request AND prior.accepted_outputs=target_outputs THEN
            RETURN QUERY SELECT prior.id,prior.job_id,prior.accepted_outputs,FALSE; RETURN;
        END IF;
        RAISE EXCEPTION 'delivery acceptance idempotency key conflicts' USING ERRCODE='40001';
    END IF;
    IF EXISTS(SELECT 1 FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job) THEN
        RAISE EXCEPTION 'an existing embedding job cannot be backfilled with guessed outputs' USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS(
        SELECT 1 FROM model_request_evidence_roots root
         WHERE root.workspace_id=target_workspace AND root.id=target_mre
           AND root.external_effect_id=target_effect AND root.request_kind='embeddings'
           AND root.binding_snapshot_id=target_snapshot
           AND root.cause_kind='embedding_job' AND root.cause_id=target_job
    ) THEN
        RAISE EXCEPTION 'delivery acceptance requires its exact delivery MRE cause' USING ERRCODE='23514';
    END IF;
    SELECT jsonb_agg(jsonb_build_object(
               'source_ordinal',node.ordinal,'material_id',node.reference_id
           ) ORDER BY node.ordinal),
           COUNT(*)=COUNT(DISTINCT node.reference_id)
      INTO source_tuple,sources_unique
      FROM model_request_evidence_nodes node
     WHERE node.workspace_id=target_workspace AND node.evidence_root_id=target_mre
       AND node.reference_kind='governed_input_material';
    IF source_tuple IS NULL OR NOT sources_unique THEN
        RAISE EXCEPTION 'delivery acceptance requires nonempty unique ordered MRE sources' USING ERRCODE='23514';
    END IF;
    INSERT INTO embedding_delivery_acceptance_receipts(
        id,workspace_id,principal_id,idempotency_key,job_id,space_registration_id,job_kind,
        model_binding_snapshot_id,external_effect_id,model_request_evidence_id,
        retries_unknown_embedding_job_id,expected_predecessor_version,request_tuple,
        accepted_sources,accepted_outputs
    ) VALUES(
        target_receipt,target_workspace,target_principal,target_idempotency,target_job,target_space,target_kind,
        target_snapshot,target_effect,target_mre,target_predecessor,target_expected_predecessor,
        target_request,source_tuple,target_outputs
    ) ON CONFLICT DO NOTHING;
    GET DIAGNOSTICS inserted=ROW_COUNT;
    IF inserted=0 THEN
        SELECT * INTO prior FROM embedding_delivery_acceptance_receipts
         WHERE workspace_id=target_workspace AND idempotency_key=target_idempotency FOR UPDATE;
        IF NOT FOUND OR prior.id<>target_receipt OR prior.principal_id<>target_principal
           OR prior.request_tuple<>target_request OR prior.accepted_outputs<>target_outputs THEN
            RAISE EXCEPTION 'delivery acceptance receipt identity conflicts' USING ERRCODE='40001';
        END IF;
        RETURN QUERY SELECT prior.id,prior.job_id,prior.accepted_outputs,FALSE; RETURN;
    END IF;
    RETURN QUERY SELECT target_receipt,target_job,target_outputs,TRUE;
END $$;

CREATE OR REPLACE FUNCTION vestrace_finalize_delivery_embedding_outputs(
    target_workspace UUID,target_principal UUID,target_receipt UUID
) RETURNS JSONB LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE authority embedding_delivery_acceptance_receipts%ROWTYPE; output JSONB; output_position BIGINT;
        source JSONB; blocker UUID; existing_blocker UUID; accepted UUID;
BEGIN
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);
    IF NULLIF(current_setting('vestrace.principal_id',true),'') IS DISTINCT FROM target_principal::TEXT THEN
        RAISE EXCEPTION 'delivery acceptance principal does not match the scoped request' USING ERRCODE='42501';
    END IF;
    SELECT * INTO authority FROM embedding_delivery_acceptance_receipts
     WHERE workspace_id=target_workspace AND id=target_receipt FOR UPDATE;
    IF NOT FOUND OR authority.principal_id<>target_principal THEN
        RAISE EXCEPTION 'delivery acceptance receipt is absent' USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS(
        SELECT 1 FROM external_effect_intents effect
         WHERE effect.id=authority.external_effect_id AND effect.workspace_id=target_workspace
           AND effect.payload=authority.request_tuple->'external_effect_intent'
    ) THEN
        RAISE EXCEPTION 'delivery acceptance effect does not match its immutable request' USING ERRCODE='23514';
    END IF;
    accepted:=vestrace_accept_embedding_job(
        authority.job_id,authority.workspace_id,authority.space_registration_id,authority.job_kind,
        authority.model_binding_snapshot_id,authority.external_effect_id,authority.model_request_evidence_id,
        authority.retries_unknown_embedding_job_id,authority.expected_predecessor_version
    );
    IF accepted<>authority.job_id THEN
        RAISE EXCEPTION 'delivery acceptance converged on another job' USING ERRCODE='40001';
    END IF;
    IF EXISTS(
        SELECT 1 FROM jsonb_array_elements(authority.accepted_sources) item
        LEFT JOIN content_materials material
          ON material.workspace_id=target_workspace
         AND material.id=(item->>'material_id')::UUID
         AND material.state='live'
        WHERE material.id IS NULL
    ) THEN
        RAISE EXCEPTION 'delivery acceptance source is absent, foreign or non-live' USING ERRCODE='23514';
    END IF;
    -- Lock live sources in their evidence order before any blocker is added.
    PERFORM material.id FROM jsonb_array_elements(authority.accepted_sources) item
    JOIN content_materials material
      ON material.workspace_id=target_workspace AND material.id=(item->>'material_id')::UUID
    ORDER BY (item->>'source_ordinal')::INTEGER FOR UPDATE OF material;
    FOR output,output_position IN
        SELECT value,ordinality-1 FROM jsonb_array_elements(authority.accepted_outputs) WITH ORDINALITY
    LOOP
        IF jsonb_typeof(output)<>'object'
           OR (output->>'output_ordinal')::BIGINT<>output_position
           OR output->>'intent_id' IS NULL OR output->>'material_id' IS NULL
           OR output->>'key_id' IS NULL OR output->>'nonce' IS NULL THEN
            RAISE EXCEPTION 'delivery output identities must be exact and contiguous' USING ERRCODE='23514';
        END IF;
        PERFORM vestrace_reserve_embedding_job_output_intent(
            (output->>'intent_id')::UUID,target_workspace,authority.job_id,output_position,
            (output->>'material_id')::UUID,(output->>'key_id')::UUID,(output->>'nonce')::UUID
        );
        FOR source IN SELECT value FROM jsonb_array_elements(authority.accepted_sources) LOOP
            SELECT membership.blocker_id INTO existing_blocker
              FROM embedding_delivery_source_memberships membership
             WHERE membership.workspace_id=target_workspace AND membership.job_id=authority.job_id
               AND membership.source_ordinal=(source->>'source_ordinal')::INTEGER
               AND membership.output_ordinal=output_position;
            IF FOUND THEN CONTINUE; END IF;
            blocker:=gen_random_uuid();
            PERFORM vestrace_record_material_erasure_blocker(
                blocker,'content',(source->>'material_id')::UUID,NULL,'intent',NULL
            );
            INSERT INTO embedding_delivery_source_memberships(
                workspace_id,job_id,source_ordinal,source_material_id,output_ordinal,intent_id,blocker_id
            ) VALUES(
                target_workspace,authority.job_id,(source->>'source_ordinal')::INTEGER,
                (source->>'material_id')::UUID,output_position,(output->>'intent_id')::UUID,blocker
            );
        END LOOP;
    END LOOP;
    RETURN authority.accepted_outputs;
END $$;

-- The generic restart reconciler may observe an embedding output, but only an
-- embedding-owned durable retirement request may authorize its abandonment.
CREATE OR REPLACE FUNCTION vestrace_prepare_pre_prepared_material_abandon(target_intent_id UUID)
RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE intent_row material_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM material_key_creation_intents WHERE id=target_intent_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE='23514'; END IF;
    PERFORM vestrace_assert_material_intent_workspace(intent_row.workspace_id);
    IF intent_row.owner_kind='embedding_job_output' AND NOT EXISTS(
        SELECT 1 FROM embedding_output_key_retirement_requests request
         WHERE request.workspace_id=intent_row.workspace_id AND request.intent_id=intent_row.id
           AND request.job_id=intent_row.owner_id AND request.output_ordinal=intent_row.output_ordinal
    ) THEN
        RAISE EXCEPTION 'embedding output abandonment requires its exact job-owned retirement authority' USING ERRCODE='23514';
    END IF;
    IF intent_row.state='pre_prepared_abandon_prepared' THEN RETURN; END IF;
    IF intent_row.state NOT IN ('reserved','provisional_created','provisional_receipted')
       OR intent_row.prepared_marker IS NOT NULL OR intent_row.bound_receipt IS NOT NULL
       OR EXISTS(SELECT 1 FROM content_materials WHERE intent_id=intent_row.id)
       OR EXISTS(SELECT 1 FROM content_material_bytes WHERE intent_id=intent_row.id)
       OR EXISTS(SELECT 1 FROM prepared_material_attachments WHERE intent_id=intent_row.id)
       OR EXISTS(SELECT 1 FROM content_material_ordinary_references WHERE intent_id=intent_row.id) THEN
        RAISE EXCEPTION 'only an unprepared unbound intent may take the pre-prepared abort' USING ERRCODE='23514';
    END IF;
    UPDATE material_key_creation_intents SET state='pre_prepared_abandon_prepared',updated_at=NOW()
     WHERE id=intent_row.id;
END $$;

CREATE OR REPLACE FUNCTION vestrace_request_embedding_output_retirement(
    target_receipt_id UUID,target_workspace UUID,target_principal UUID,target_job UUID,
    target_expected_version BIGINT,target_idempotency_key TEXT,target_terminal_state TEXT,
    target_evidence_kind TEXT,target_evidence_id UUID,target_authorization_policy_version TEXT,
    target_authorization_capability TEXT,target_authorization_operation TEXT,
    target_authorization_resource_scope TEXT,target_authorization_risk TEXT
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE;
        prior embedding_job_pre_dispatch_retirement_authorities%ROWTYPE;
        member RECORD; member_count INTEGER:=0; target_now TIMESTAMPTZ;
BEGIN
    IF target_receipt_id IS NULL OR target_workspace IS NULL OR target_principal IS NULL
       OR target_job IS NULL OR target_expected_version IS NULL OR target_expected_version<1
       OR target_idempotency_key IS NULL OR btrim(target_idempotency_key)=''
       OR target_terminal_state NOT IN ('cancelled','failed_definite')
       OR target_evidence_kind NOT IN ('cancellation_authorization','external_effect_denied','admission_timeout')
       OR target_evidence_id IS NULL THEN
        RAISE EXCEPTION 'embedding output retirement authority is malformed' USING ERRCODE='22023';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);
    IF NULLIF(current_setting('vestrace.principal_id',true),'') IS DISTINCT FROM target_principal::TEXT THEN
        RAISE EXCEPTION 'embedding output retirement principal does not match request scope' USING ERRCODE='42501';
    END IF;
    IF (target_terminal_state='cancelled' AND target_evidence_kind<>'cancellation_authorization')
       OR (target_terminal_state='failed_definite' AND target_evidence_kind='cancellation_authorization') THEN
        RAISE EXCEPTION 'embedding output retirement state does not match evidence' USING ERRCODE='23514';
    END IF;
    IF target_evidence_kind='cancellation_authorization' AND (
       target_authorization_policy_version IS NULL OR btrim(target_authorization_policy_version)=''
       OR target_authorization_capability IS DISTINCT FROM 'execution.write'
       OR target_authorization_operation IS DISTINCT FROM 'embedding.job.cancel'
       OR target_authorization_resource_scope IS DISTINCT FROM 'workspace://'
       OR target_authorization_risk IS DISTINCT FROM 'low') THEN
        RAISE EXCEPTION 'embedding output retirement lacks exact cancellation authorization' USING ERRCODE='42501';
    END IF;
    SELECT job.model_binding_snapshot_id,job.space_registration_id,job.external_effect_id,
           snapshot.connection_id,snapshot.branch,snapshot.credential_slot_id,
           snapshot.credential_activation_guard_id
      INTO initial FROM embedding_jobs job JOIN model_binding_snapshots snapshot
        ON snapshot.workspace_id=job.workspace_id AND snapshot.id=job.model_binding_snapshot_id
     WHERE job.workspace_id=target_workspace AND job.id=target_job;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding output retirement requires exact job snapshot' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM connection_execution_guards WHERE workspace_id=target_workspace
      AND connection_id=initial.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding output retirement requires connection guard' USING ERRCODE='23514'; END IF;
    IF initial.branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards WHERE workspace_id=target_workspace
          AND connection_id=initial.connection_id AND credential_slot_id=initial.credential_slot_id
          AND id=initial.credential_activation_guard_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding output retirement requires credential guard' USING ERRCODE='23514'; END IF;
    ELSIF initial.branch<>'no_auth' OR initial.credential_slot_id IS NOT NULL
       OR initial.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding output retirement auth branch is not exact' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_registrations WHERE workspace_id=target_workspace
      AND id=initial.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding output retirement requires space guard' USING ERRCODE='23514'; END IF;
    SELECT * INTO job_row FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
    IF NOT FOUND OR job_row.version<>target_expected_version OR job_row.state NOT IN ('requested','running') THEN
        RAISE EXCEPTION 'embedding output retirement job version or state conflicts' USING ERRCODE='40001';
    END IF;
    target_now:=clock_timestamp();
    IF EXISTS(SELECT 1 FROM external_effect_lifecycle_transitions WHERE workspace_id=target_workspace
       AND effect_id=job_row.external_effect_id AND status NOT IN ('prepared','authorized'))
       OR EXISTS(SELECT 1 FROM external_effect_receipts WHERE workspace_id=target_workspace AND effect_id=job_row.external_effect_id)
       OR EXISTS(SELECT 1 FROM provider_result_preparations WHERE workspace_id=target_workspace AND external_effect_id=job_row.external_effect_id) THEN
        RAISE EXCEPTION 'embedding output retirement has provider dispatch evidence' USING ERRCODE='23514';
    END IF;
    IF target_evidence_kind='external_effect_denied' AND NOT EXISTS(
       SELECT 1 FROM external_effect_authorizations WHERE id=target_evidence_id
         AND workspace_id=target_workspace AND effect_id=job_row.external_effect_id AND result='deny') THEN
        RAISE EXCEPTION 'embedding output retirement requires exact denied authorization' USING ERRCODE='23514';
    END IF;
    IF target_evidence_kind='admission_timeout' AND NOT EXISTS(
       SELECT 1 FROM provider_admission_waits WHERE id=target_evidence_id
         AND workspace_id=target_workspace AND external_effect_id=job_row.external_effect_id
         AND connection_id=initial.connection_id AND deadline_at<=target_now
         AND (terminal_reason IS NULL OR terminal_reason='timeout')) THEN
        RAISE EXCEPTION 'embedding output retirement requires exact expired admission wait' USING ERRCODE='23514';
    END IF;
    SELECT * INTO prior FROM embedding_job_pre_dispatch_retirement_authorities
     WHERE (workspace_id=target_workspace AND job_id=target_job)
        OR (workspace_id=target_workspace AND idempotency_key=target_idempotency_key)
     FOR UPDATE;
    IF FOUND THEN
        IF prior.id<>target_receipt_id OR prior.principal_id<>target_principal
           OR prior.job_id<>target_job OR prior.expected_version<>target_expected_version
           OR prior.idempotency_key<>target_idempotency_key OR prior.terminal_state<>target_terminal_state
           OR prior.evidence_kind<>target_evidence_kind OR prior.evidence_id<>target_evidence_id
           OR prior.authorization_policy_version IS DISTINCT FROM target_authorization_policy_version
           OR prior.authorization_capability IS DISTINCT FROM target_authorization_capability
           OR prior.authorization_operation IS DISTINCT FROM target_authorization_operation
           OR prior.authorization_resource_scope IS DISTINCT FROM target_authorization_resource_scope
           OR prior.authorization_risk IS DISTINCT FROM target_authorization_risk THEN
            RAISE EXCEPTION 'embedding output retirement authority conflicts' USING ERRCODE='40001';
        END IF;
    ELSE
        INSERT INTO embedding_job_pre_dispatch_retirement_authorities(
            id,workspace_id,principal_id,job_id,expected_version,idempotency_key,terminal_state,
            evidence_kind,evidence_id,authorization_policy_version,authorization_capability,
            authorization_operation,authorization_resource_scope,authorization_risk
        ) VALUES(
            target_receipt_id,target_workspace,target_principal,target_job,target_expected_version,
            target_idempotency_key,target_terminal_state,target_evidence_kind,target_evidence_id,
            target_authorization_policy_version,target_authorization_capability,
            target_authorization_operation,target_authorization_resource_scope,target_authorization_risk
        );
    END IF;
    FOR member IN
        SELECT membership.output_ordinal,membership.intent_id,intent.owner_kind,intent.owner_id,
               intent.output_ordinal AS intent_output_ordinal,intent.state,intent.prepared_marker,
               intent.bound_receipt
          FROM embedding_job_material_intents membership
          JOIN material_key_creation_intents intent
            ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
         WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
         ORDER BY membership.output_ordinal,membership.intent_id
         FOR UPDATE OF membership,intent
    LOOP
        member_count:=member_count+1;
        IF member.owner_kind<>'embedding_job_output' OR member.owner_id<>target_job
           OR member.intent_output_ordinal<>member.output_ordinal
           OR member.state NOT IN ('reserved','provisional_created','provisional_receipted','pre_prepared_abandon_prepared','abandoned')
           OR member.prepared_marker IS NOT NULL OR member.bound_receipt IS NOT NULL THEN
            RAISE EXCEPTION 'embedding output retirement requires exact pre-prepared outputs' USING ERRCODE='23514';
        END IF;
        INSERT INTO embedding_output_key_retirement_requests(
            workspace_id,job_id,output_ordinal,intent_id,termination_receipt_id
        ) VALUES(target_workspace,target_job,member.output_ordinal,member.intent_id,target_receipt_id)
        ON CONFLICT(workspace_id,intent_id) DO NOTHING;
        IF NOT EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests request
                       WHERE request.workspace_id=target_workspace AND request.intent_id=member.intent_id
                         AND request.termination_receipt_id=target_receipt_id) THEN
            RAISE EXCEPTION 'embedding output retirement request conflicts' USING ERRCODE='40001';
        END IF;
        IF member.state NOT IN ('pre_prepared_abandon_prepared','abandoned') THEN
            PERFORM vestrace_prepare_pre_prepared_material_abandon(member.intent_id);
        END IF;
    END LOOP;
    IF member_count=0 THEN
        RAISE EXCEPTION 'embedding output retirement requires enrolled outputs' USING ERRCODE='23514';
    END IF;
    RETURN target_receipt_id;
END $$;

CREATE OR REPLACE FUNCTION vestrace_claim_embedding_output_key(target_workspace UUID)
RETURNS TABLE(workspace_id UUID,job_id UUID,intent_id UUID,material_id UUID,material_key_id UUID,
              nonce UUID,output_ordinal BIGINT,vault_receipt UUID,retirement_requested BOOLEAN)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);
    RETURN QUERY
    SELECT member.workspace_id,member.job_id,member.intent_id,intent.material_id,intent.material_key_id,
           intent.nonce,member.output_ordinal,receipt.vault_receipt,(retirement.intent_id IS NOT NULL)
      FROM embedding_job_material_intents member
      JOIN embedding_jobs job ON job.workspace_id=member.workspace_id AND job.id=member.job_id
      JOIN material_key_creation_intents intent
        ON intent.workspace_id=member.workspace_id AND intent.id=member.intent_id
      LEFT JOIN embedding_output_key_receipts receipt
        ON receipt.workspace_id=member.workspace_id AND receipt.intent_id=member.intent_id
      LEFT JOIN embedding_output_key_retirement_requests retirement
        ON retirement.workspace_id=member.workspace_id AND retirement.intent_id=member.intent_id
      LEFT JOIN embedding_output_key_retirement_receipts retired
        ON retired.workspace_id=member.workspace_id AND retired.intent_id=member.intent_id
     WHERE member.workspace_id=target_workspace AND job.state IN ('requested','running')
       AND intent.owner_kind='embedding_job_output' AND intent.owner_id=job.id
       AND intent.output_ordinal=member.output_ordinal
       AND ((retirement.intent_id IS NULL AND receipt.vault_receipt IS NULL
             AND intent.state IN ('reserved','provisional_created','provisional_receipted'))
            OR (retirement.intent_id IS NOT NULL AND retired.intent_id IS NULL
                AND intent.state='pre_prepared_abandon_prepared'))
     ORDER BY member.job_id,member.output_ordinal
     FOR UPDATE OF member,job,intent SKIP LOCKED LIMIT 1;
END $$;

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_output_termination_authority()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF EXISTS(SELECT 1 FROM embedding_job_material_intents member
               WHERE member.workspace_id=NEW.workspace_id AND member.job_id=NEW.job_id) THEN
        IF NOT EXISTS(
            SELECT 1 FROM embedding_job_pre_dispatch_retirement_authorities authority
             WHERE authority.id=NEW.id AND authority.workspace_id=NEW.workspace_id
               AND authority.principal_id=NEW.principal_id AND authority.job_id=NEW.job_id
               AND authority.expected_version=NEW.expected_version
               AND authority.idempotency_key=NEW.idempotency_key
               AND authority.terminal_state=NEW.terminal_state
               AND authority.evidence_kind=NEW.evidence_kind AND authority.evidence_id=NEW.evidence_id
               AND authority.authorization_policy_version IS NOT DISTINCT FROM NEW.authorization_policy_version
               AND authority.authorization_capability IS NOT DISTINCT FROM NEW.authorization_capability
               AND authority.authorization_operation IS NOT DISTINCT FROM NEW.authorization_operation
               AND authority.authorization_resource_scope IS NOT DISTINCT FROM NEW.authorization_resource_scope
               AND authority.authorization_risk IS NOT DISTINCT FROM NEW.authorization_risk
        ) OR EXISTS(
            SELECT 1 FROM embedding_job_material_intents member
             WHERE member.workspace_id=NEW.workspace_id AND member.job_id=NEW.job_id
               AND NOT EXISTS(
                   SELECT 1
                     FROM embedding_output_key_retirement_requests request
                     JOIN embedding_output_key_retirement_receipts retirement
                       ON retirement.workspace_id=request.workspace_id
                      AND retirement.job_id=request.job_id
                      AND retirement.output_ordinal=request.output_ordinal
                      AND retirement.intent_id=request.intent_id
                     JOIN material_key_creation_intents intent
                       ON intent.workspace_id=request.workspace_id
                      AND intent.id=request.intent_id
                    WHERE request.workspace_id=member.workspace_id
                      AND request.intent_id=member.intent_id
                      AND request.job_id=member.job_id
                      AND request.output_ordinal=member.output_ordinal
                      AND request.termination_receipt_id=NEW.id
                      AND intent.owner_kind='embedding_job_output'
                      AND intent.owner_id=member.job_id
                      AND intent.output_ordinal=member.output_ordinal
                      AND intent.state='abandoned'
               )
        ) THEN
            RAISE EXCEPTION 'embedding termination requires every output retired under its exact durable authority'
                USING ERRCODE='23514';
        END IF;

        -- The source remains protected while the job is active, even after the
        -- host erasure and its immutable output receipt exist.  Release only
        -- the blockers named by this job's immutable delivery memberships,
        -- in the same transaction that installs the validated terminal fact.
        UPDATE material_erasure_blockers blocker SET state='terminal'
          FROM embedding_delivery_source_memberships source
         WHERE source.workspace_id=NEW.workspace_id AND source.job_id=NEW.job_id
           AND blocker.workspace_id=source.workspace_id AND blocker.id=source.blocker_id
           AND blocker.target_kind='content'
           AND blocker.content_material_id=source.source_material_id
           AND blocker.blocker_kind='intent' AND blocker.state='nonterminal';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER embedding_job_termination_output_authority_exact
    BEFORE INSERT ON embedding_job_termination_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_output_termination_authority();

CREATE OR REPLACE FUNCTION vestrace_record_embedding_output_key_receipt(
    target_workspace UUID,target_intent UUID,target_receipt UUID
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE job_row embedding_jobs%ROWTYPE; member embedding_job_material_intents%ROWTYPE;
        intent_row material_key_creation_intents%ROWTYPE;
        locked_member RECORD; existing embedding_output_key_receipts%ROWTYPE;
BEGIN
    IF target_receipt IS NULL THEN RAISE EXCEPTION 'output key receipt is malformed' USING ERRCODE='22023'; END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);

    -- Every output-key lifecycle writer takes the parent job first, then the
    -- complete membership/intent set in ordinal order.  Aggregate progress
    -- re-enters the same job lock after this function returns, so it cannot
    -- invert against retirement's job -> outputs order.
    SELECT job.* INTO job_row
      FROM embedding_jobs job
      JOIN embedding_job_material_intents target_member
        ON target_member.workspace_id=job.workspace_id AND target_member.job_id=job.id
     WHERE job.workspace_id=target_workspace AND target_member.intent_id=target_intent
     FOR UPDATE OF job;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'output receipt requires its active exact enrolled owner tuple' USING ERRCODE='23514';
    END IF;
    FOR locked_member IN
        SELECT membership.output_ordinal,membership.intent_id
          FROM embedding_job_material_intents membership
          JOIN material_key_creation_intents intent
            ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
         WHERE membership.workspace_id=target_workspace AND membership.job_id=job_row.id
         ORDER BY membership.output_ordinal,membership.intent_id
         FOR UPDATE OF membership,intent
    LOOP
        NULL;
    END LOOP;
    SELECT * INTO member FROM embedding_job_material_intents
     WHERE workspace_id=target_workspace AND job_id=job_row.id AND intent_id=target_intent;
    SELECT * INTO intent_row FROM material_key_creation_intents
     WHERE workspace_id=target_workspace AND id=target_intent;
    IF NOT FOUND OR member.intent_id IS NULL OR intent_row.owner_kind<>'embedding_job_output'
       OR intent_row.owner_id<>member.job_id OR intent_row.output_ordinal<>member.output_ordinal
       OR EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests
                  WHERE workspace_id=target_workspace AND intent_id=target_intent) THEN
        RAISE EXCEPTION 'output receipt requires its active exact enrolled owner tuple' USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_output_key_receipts
     WHERE workspace_id=target_workspace AND intent_id=target_intent FOR UPDATE;
    IF existing.intent_id IS NOT NULL THEN
        IF existing.vault_receipt<>target_receipt THEN
            RAISE EXCEPTION 'output receipt conflicts' USING ERRCODE='40001';
        END IF;
        RETURN;
    END IF;
    PERFORM vestrace_record_material_key_provisional_created(target_intent);
    PERFORM vestrace_record_material_key_provisional_receipt(target_intent,target_receipt);
    INSERT INTO embedding_output_key_receipts(workspace_id,job_id,output_ordinal,intent_id,vault_receipt)
        VALUES(target_workspace,member.job_id,member.output_ordinal,target_intent,target_receipt);
END $$;

-- Readiness is derived from the complete immutable membership set.  A single
-- per-output receipt is never allowed to stand in for aggregate result-key
-- readiness, and a retirement request closes this phase permanently.
CREATE OR REPLACE FUNCTION vestrace_embedding_output_key_progress(
    target_workspace UUID,target_job UUID
) RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE job_row embedding_jobs%ROWTYPE; member_count BIGINT;
BEGIN
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);
    SELECT * INTO job_row FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
    IF NOT FOUND OR job_row.state NOT IN ('requested','running') THEN
        RAISE EXCEPTION 'output key progress requires its active exact job' USING ERRCODE='23514';
    END IF;
    SELECT COUNT(*) INTO member_count FROM embedding_job_material_intents
     WHERE workspace_id=target_workspace AND job_id=target_job;
    IF member_count=0 THEN
        RAISE EXCEPTION 'output key progress requires enrolled outputs' USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1
          FROM embedding_job_material_intents member
          JOIN material_key_creation_intents intent
            ON intent.workspace_id=member.workspace_id AND intent.id=member.intent_id
          LEFT JOIN embedding_output_key_receipts receipt
            ON receipt.workspace_id=member.workspace_id AND receipt.job_id=member.job_id
           AND receipt.output_ordinal=member.output_ordinal AND receipt.intent_id=member.intent_id
         WHERE member.workspace_id=target_workspace AND member.job_id=target_job
           AND (intent.owner_kind<>'embedding_job_output' OR intent.owner_id<>member.job_id
                OR intent.output_ordinal<>member.output_ordinal)
    ) OR EXISTS(
        SELECT 1 FROM embedding_output_key_retirement_requests request
         WHERE request.workspace_id=target_workspace AND request.job_id=target_job
    ) THEN
        RAISE EXCEPTION 'output key progress has a conflicting owner or retirement phase'
            USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1
          FROM embedding_job_material_intents member
          JOIN material_key_creation_intents intent
            ON intent.workspace_id=member.workspace_id AND intent.id=member.intent_id
          LEFT JOIN embedding_output_key_receipts receipt
            ON receipt.workspace_id=member.workspace_id AND receipt.job_id=member.job_id
           AND receipt.output_ordinal=member.output_ordinal AND receipt.intent_id=member.intent_id
         WHERE member.workspace_id=target_workspace AND member.job_id=target_job
           AND (receipt.intent_id IS NULL OR intent.state<>'provisional_receipted'
                OR intent.vault_receipt IS DISTINCT FROM receipt.vault_receipt)
    ) THEN
        RETURN 'waiting_for_result_keys';
    END IF;
    RETURN 'prepared';
END $$;

CREATE OR REPLACE FUNCTION vestrace_record_embedding_output_key_retirement(
    target_workspace UUID,target_intent UUID,target_receipt UUID
) RETURNS VOID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE job_row embedding_jobs%ROWTYPE; member embedding_job_material_intents%ROWTYPE;
        locked_member RECORD; existing embedding_output_key_retirement_receipts%ROWTYPE;
BEGIN
    IF target_receipt IS NULL THEN RAISE EXCEPTION 'output key retirement receipt is malformed' USING ERRCODE='22023'; END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace);

    SELECT job.* INTO job_row
      FROM embedding_jobs job
      JOIN embedding_job_material_intents target_member
        ON target_member.workspace_id=job.workspace_id AND target_member.job_id=job.id
     WHERE job.workspace_id=target_workspace AND target_member.intent_id=target_intent
     FOR UPDATE OF job;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'output key retirement requires its exact durable request' USING ERRCODE='23514';
    END IF;
    FOR locked_member IN
        SELECT membership.output_ordinal,membership.intent_id
          FROM embedding_job_material_intents membership
          JOIN material_key_creation_intents intent
            ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
         WHERE membership.workspace_id=target_workspace AND membership.job_id=job_row.id
         ORDER BY membership.output_ordinal,membership.intent_id
         FOR UPDATE OF membership,intent
    LOOP
        NULL;
    END LOOP;
    SELECT * INTO member FROM embedding_job_material_intents
     WHERE workspace_id=target_workspace AND job_id=job_row.id AND intent_id=target_intent;
    IF NOT FOUND OR NOT EXISTS(
        SELECT 1 FROM embedding_output_key_retirement_requests request
         WHERE request.workspace_id=target_workspace AND request.intent_id=target_intent
           AND request.job_id=member.job_id AND request.output_ordinal=member.output_ordinal
    ) THEN
        RAISE EXCEPTION 'output key retirement requires its exact durable request' USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_output_key_retirement_receipts
     WHERE workspace_id=target_workspace AND intent_id=target_intent FOR UPDATE;
    IF FOUND THEN
        IF existing.erasure_receipt<>target_receipt THEN
            RAISE EXCEPTION 'output key retirement receipt conflicts' USING ERRCODE='40001';
        END IF;
        RETURN;
    END IF;
    PERFORM vestrace_record_unbound_material_key_erasure(target_intent,target_receipt);
    PERFORM vestrace_finalize_material_key_abandon(target_intent);
    INSERT INTO embedding_output_key_retirement_receipts(
        workspace_id,job_id,output_ordinal,intent_id,erasure_receipt
    ) VALUES(target_workspace,member.job_id,member.output_ordinal,target_intent,target_receipt);
END $$;

-- Close the default PUBLIC execute privilege while the migrator still owns
-- each new function.  The guarded-owner provisioner repeats this operation,
-- while this block also secures the explicit fresh-superuser fallback below.
DO $$ DECLARE target REGPROCEDURE; BEGIN
    FOREACH target IN ARRAY ARRAY[
        'public.vestrace_validate_delivery_source_membership()'::REGPROCEDURE,
        'public.vestrace_begin_delivery_embedding_outputs(UUID,UUID,UUID,TEXT,UUID,UUID,TEXT,UUID,UUID,UUID,UUID,BIGINT,JSONB,JSONB)'::REGPROCEDURE,
        'public.vestrace_finalize_delivery_embedding_outputs(UUID,UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_prepare_pre_prepared_material_abandon(UUID)'::REGPROCEDURE,
        'public.vestrace_request_embedding_output_retirement(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT)'::REGPROCEDURE,
        'public.vestrace_claim_embedding_output_key(UUID)'::REGPROCEDURE,
        'public.vestrace_record_embedding_output_key_receipt(UUID,UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_embedding_output_key_progress(UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_record_embedding_output_key_retirement(UUID,UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_validate_embedding_output_termination_authority()'::REGPROCEDURE
    ] LOOP EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC',target); END LOOP;
END $$;
GRANT EXECUTE ON FUNCTION vestrace_begin_delivery_embedding_outputs(UUID,UUID,UUID,TEXT,UUID,UUID,TEXT,UUID,UUID,UUID,UUID,BIGINT,JSONB,JSONB) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_delivery_embedding_outputs(UUID,UUID,UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_request_embedding_output_retirement(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_claim_embedding_output_key(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_embedding_output_key_receipt(UUID,UUID,UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_embedding_output_key_progress(UUID,UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_embedding_output_key_retirement(UUID,UUID,UUID) TO vestrace;

DO $$ DECLARE target REGPROCEDURE; BEGIN
    PERFORM vestrace_assign_p02_function_owner(
        'public.vestrace_prepare_pre_prepared_material_abandon(UUID)'::REGPROCEDURE
    );
    FOREACH target IN ARRAY ARRAY[
        'public.vestrace_validate_delivery_source_membership()'::REGPROCEDURE,
        'public.vestrace_begin_delivery_embedding_outputs(UUID,UUID,UUID,TEXT,UUID,UUID,TEXT,UUID,UUID,UUID,UUID,BIGINT,JSONB,JSONB)'::REGPROCEDURE,
        'public.vestrace_finalize_delivery_embedding_outputs(UUID,UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_request_embedding_output_retirement(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT)'::REGPROCEDURE,
        'public.vestrace_claim_embedding_output_key(UUID)'::REGPROCEDURE,
        'public.vestrace_record_embedding_output_key_receipt(UUID,UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_embedding_output_key_progress(UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_record_embedding_output_key_retirement(UUID,UUID,UUID)'::REGPROCEDURE,
        'public.vestrace_validate_embedding_output_termination_authority()'::REGPROCEDURE
    ] LOOP
        BEGIN
            PERFORM vestrace_assign_p03_function_owner(target);
        EXCEPTION WHEN OTHERS THEN
            RAISE EXCEPTION '0193 could not hand guarded ownership to %: %',target,SQLERRM
                USING ERRCODE=SQLSTATE;
        END;
    END LOOP;
    PERFORM vestrace_assign_p02_table_owner('public.material_erasure_blockers'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_job_termination_receipts'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_delivery_acceptance_receipts'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_delivery_source_memberships'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_job_pre_dispatch_retirement_authorities'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_output_key_retirement_requests'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_output_key_receipts'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_output_key_retirement_receipts'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN RAISE; END IF;
    ALTER TABLE material_erasure_blockers OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_job_termination_receipts OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_delivery_acceptance_receipts OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_delivery_source_memberships OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_job_pre_dispatch_retirement_authorities OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_output_key_retirement_requests OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_output_key_receipts OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_output_key_retirement_receipts OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_validate_delivery_source_membership() OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_begin_delivery_embedding_outputs(UUID,UUID,UUID,TEXT,UUID,UUID,TEXT,UUID,UUID,UUID,UUID,BIGINT,JSONB,JSONB) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_finalize_delivery_embedding_outputs(UUID,UUID,UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_prepare_pre_prepared_material_abandon(UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_request_embedding_output_retirement(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_claim_embedding_output_key(UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_record_embedding_output_key_receipt(UUID,UUID,UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_embedding_output_key_progress(UUID,UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_record_embedding_output_key_retirement(UUID,UUID,UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_validate_embedding_output_termination_authority() OWNER TO vestrace_guarded_owner;
END $$;

DO $$
BEGIN
    PERFORM public.vestrace_finish_p04_output_key_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN
        RAISE EXCEPTION 'P04 output-key ownership hand-back is unavailable' USING ERRCODE='42501';
    END IF;
END $$;
