-- P04 Task 14D: delivery result preparation. This is deliberately the first
-- half of completion: it seals and records non-live output only. Binding,
-- ordinary references, Live projections, corpus advancement and Succeeded are
-- deliberately absent.

-- The real runtime migrator is deliberately not a superuser. The provisioner
-- lends only the two existing guarded tables on which this migration must
-- install its terminal fences, then takes that capability back below.
DO $$
BEGIN
    PERFORM vestrace_prepare_p04_result_preparation_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN
        RAISE EXCEPTION 'P04 result-preparation ownership hand-back must be provisioned before runtime migration'
            USING ERRCODE='42501';
    END IF;
END $$;

-- 0192 deliberately treated every enrolled output as an incomplete protocol.
-- Task 14D still does not dispatch a job or compose a worker, but its result
-- preparation needs the existing dispatch path to recognize an already
-- complete, receipted output set.  These are forward replacements of the two
-- existing validators only; every non-embedding path and all of the original
-- connection, credential, space, job, and terminal checks stay intact.
CREATE OR REPLACE FUNCTION vestrace_lock_embedding_job_pre_dispatch_gate(
    target_workspace_id UUID,
    target_effect_id UUID,
    allow_terminal BOOLEAN
) RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE; member RECORD;
        output_position BIGINT := 0; receipt_vault_receipt UUID; retired BOOLEAN;
BEGIN
    IF target_workspace_id IS NULL OR target_effect_id IS NULL OR allow_terminal IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding pre-dispatch gate arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT job.id AS job_id, job.model_binding_snapshot_id, job.space_registration_id,
           snapshot.connection_id, snapshot.branch, snapshot.credential_slot_id,
           snapshot.credential_activation_guard_id
      INTO initial FROM embedding_jobs AS job JOIN model_binding_snapshots AS snapshot
        ON snapshot.workspace_id=job.workspace_id AND snapshot.id=job.model_binding_snapshot_id
     WHERE job.workspace_id=target_workspace_id AND job.external_effect_id=target_effect_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding pre-dispatch gate requires its exact job snapshot' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM connection_execution_guards WHERE workspace_id=target_workspace_id
      AND connection_id=initial.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch gate requires its connection execution guard' USING ERRCODE='23514'; END IF;
    IF initial.branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards WHERE workspace_id=target_workspace_id
          AND connection_id=initial.connection_id AND credential_slot_id=initial.credential_slot_id
          AND id=initial.credential_activation_guard_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch gate requires its exact credential guard' USING ERRCODE='23514'; END IF;
    ELSIF initial.branch <> 'no_auth' THEN
        RAISE EXCEPTION 'embedding pre-dispatch gate has an unknown auth branch' USING ERRCODE='23514';
    ELSIF initial.credential_slot_id IS NOT NULL OR initial.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding pre-dispatch gate no-auth branch is not exact' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_registrations WHERE workspace_id=target_workspace_id
      AND id=initial.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch gate requires its exact embedding space guard' USING ERRCODE='23514'; END IF;
    SELECT * INTO job_row FROM embedding_jobs WHERE workspace_id=target_workspace_id AND id=initial.job_id FOR UPDATE;
    IF job_row.id IS NULL THEN RAISE EXCEPTION 'embedding pre-dispatch gate job is absent' USING ERRCODE='23514'; END IF;
    IF job_row.state IN ('requested','running')
       AND NOT EXISTS(SELECT 1 FROM embedding_job_termination_receipts WHERE workspace_id=target_workspace_id AND job_id=job_row.id) THEN
        FOR member IN
            SELECT membership.output_ordinal,membership.intent_id,intent.owner_kind,intent.owner_id,
                   intent.output_ordinal AS intent_output_ordinal,intent.state,intent.vault_receipt
              FROM embedding_job_material_intents AS membership
              JOIN material_key_creation_intents AS intent
                ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
             WHERE membership.workspace_id=target_workspace_id AND membership.job_id=job_row.id
             ORDER BY membership.output_ordinal,membership.intent_id
             FOR UPDATE OF membership,intent
        LOOP
            SELECT receipt.vault_receipt INTO receipt_vault_receipt
              FROM embedding_output_key_receipts AS receipt
             WHERE receipt.workspace_id=target_workspace_id AND receipt.job_id=job_row.id
               AND receipt.output_ordinal=member.output_ordinal AND receipt.intent_id=member.intent_id
             FOR UPDATE;
            IF NOT FOUND THEN
                RAISE EXCEPTION 'embedding output execution requires every exact output receipt' USING ERRCODE='23514';
            END IF;
            PERFORM 1 FROM embedding_output_key_retirement_requests AS retirement
             WHERE retirement.workspace_id=target_workspace_id AND retirement.job_id=job_row.id
               AND retirement.output_ordinal=member.output_ordinal AND retirement.intent_id=member.intent_id
             FOR UPDATE;
            retired:=FOUND;
            IF member.output_ordinal<>output_position
               OR member.owner_kind<>'embedding_job_output' OR member.owner_id<>job_row.id
               OR member.intent_output_ordinal<>member.output_ordinal
               OR member.state<>'provisional_receipted' OR member.vault_receipt IS NULL
               OR receipt_vault_receipt IS DISTINCT FROM member.vault_receipt OR retired THEN
                RAISE EXCEPTION 'embedding output execution requires a complete exact receipted output set' USING ERRCODE='23514';
            END IF;
            output_position:=output_position+1;
        END LOOP;
        IF output_position=0 THEN
            RAISE EXCEPTION 'embedding output execution requires a nonempty exact receipted output set' USING ERRCODE='23514';
        END IF;
        RETURN job_row.state;
    END IF;
    IF allow_terminal AND job_row.state IN ('succeeded','inconclusive_unknown') THEN
        RETURN job_row.state;
    END IF;
    IF allow_terminal AND job_row.state IN ('cancelled','failed_definite')
       AND EXISTS(SELECT 1 FROM embedding_job_termination_receipts AS receipt
                   WHERE receipt.workspace_id=target_workspace_id AND receipt.job_id=job_row.id
                     AND receipt.external_effect_id=target_effect_id
                     AND receipt.terminal_state=job_row.state
                     AND receipt.terminal_version=job_row.version)
       AND NOT EXISTS(SELECT 1 FROM external_effect_lifecycle_transitions
                       WHERE workspace_id=target_workspace_id AND effect_id=target_effect_id AND status='dispatching')
       AND NOT EXISTS(SELECT 1 FROM external_effect_receipts
                       WHERE workspace_id=target_workspace_id AND effect_id=target_effect_id)
       AND NOT EXISTS(SELECT 1 FROM provider_result_preparations
                       WHERE workspace_id=target_workspace_id AND external_effect_id=target_effect_id)
       AND NOT EXISTS(
           SELECT 1 FROM embedding_job_material_intents AS output_member
           JOIN material_key_creation_intents AS intent ON intent.id=output_member.intent_id AND intent.workspace_id=output_member.workspace_id
           WHERE output_member.workspace_id=target_workspace_id AND output_member.job_id=job_row.id
             AND (intent.owner_kind <> 'embedding_job_output' OR intent.owner_id <> job_row.id
                  OR intent.output_ordinal <> output_member.output_ordinal OR intent.state <> 'abandoned'
                  OR NOT EXISTS(SELECT 1 FROM material_key_creation_intent_erasure_receipts WHERE intent_id=intent.id))
       ) THEN
        RETURN job_row.state;
    END IF;
    RAISE EXCEPTION 'embedding job is terminal before provider dispatch' USING ERRCODE='23514';
END $$;

CREATE OR REPLACE FUNCTION vestrace_fence_embedding_job_dispatching()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE; member RECORD;
        target_workspace_id UUID; target_effect_id UUID; output_position BIGINT := 0;
        receipt_vault_receipt UUID; retired BOOLEAN;
BEGIN
    IF TG_OP='DELETE' THEN
      target_workspace_id := OLD.workspace_id;
      target_effect_id := OLD.effect_id;
    ELSE
      target_workspace_id := NEW.workspace_id;
      target_effect_id := NEW.effect_id;
    END IF;
    SELECT job.model_binding_snapshot_id,job.space_registration_id,snapshot.connection_id,snapshot.branch,snapshot.credential_slot_id,snapshot.credential_activation_guard_id
      INTO initial FROM embedding_jobs AS job JOIN model_binding_snapshots AS snapshot
        ON snapshot.workspace_id=job.workspace_id AND snapshot.id=job.model_binding_snapshot_id
     WHERE job.workspace_id=target_workspace_id AND job.external_effect_id=target_effect_id;
    IF NOT FOUND AND TG_OP='UPDATE' THEN
      target_workspace_id := OLD.workspace_id;
      target_effect_id := OLD.effect_id;
      SELECT job.model_binding_snapshot_id,job.space_registration_id,snapshot.connection_id,snapshot.branch,snapshot.credential_slot_id,snapshot.credential_activation_guard_id
        INTO initial FROM embedding_jobs AS job JOIN model_binding_snapshots AS snapshot
          ON snapshot.workspace_id=job.workspace_id AND snapshot.id=job.model_binding_snapshot_id
       WHERE job.workspace_id=target_workspace_id AND job.external_effect_id=target_effect_id;
    END IF;
    IF NOT FOUND THEN
      IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
    END IF;
    IF TG_OP <> 'INSERT' THEN
      RAISE EXCEPTION 'embedding lifecycle history is immutable' USING ERRCODE='23514';
    END IF;
    IF NEW.status <> 'dispatching' THEN RETURN NEW; END IF;
    PERFORM set_config('vestrace.workspace_id',target_workspace_id::TEXT,true);
    PERFORM 1 FROM connection_execution_guards WHERE workspace_id=target_workspace_id AND connection_id=initial.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding dispatch requires its connection execution guard' USING ERRCODE='23514'; END IF;
    IF initial.branch='credential' THEN
      PERFORM 1 FROM credential_activation_guards WHERE workspace_id=target_workspace_id AND connection_id=initial.connection_id
        AND credential_slot_id=initial.credential_slot_id AND id=initial.credential_activation_guard_id FOR UPDATE;
      IF NOT FOUND THEN RAISE EXCEPTION 'embedding dispatch requires its exact credential guard' USING ERRCODE='23514'; END IF;
    ELSIF initial.branch <> 'no_auth' THEN RAISE EXCEPTION 'embedding dispatch has an unknown auth branch' USING ERRCODE='23514';
    ELSIF initial.credential_slot_id IS NOT NULL OR initial.credential_activation_guard_id IS NOT NULL THEN RAISE EXCEPTION 'embedding dispatch no-auth branch is not exact' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM embedding_space_registrations WHERE workspace_id=target_workspace_id AND id=initial.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding dispatch requires its exact embedding space guard' USING ERRCODE='23514'; END IF;
    SELECT * INTO job_row FROM embedding_jobs WHERE workspace_id=target_workspace_id AND id=(SELECT id FROM embedding_jobs WHERE workspace_id=target_workspace_id AND external_effect_id=target_effect_id) FOR UPDATE;
    IF job_row.state NOT IN ('requested','running') OR EXISTS(SELECT 1 FROM embedding_job_termination_receipts WHERE workspace_id=NEW.workspace_id AND job_id=job_row.id) THEN
      RAISE EXCEPTION 'embedding job is terminal before provider dispatch' USING ERRCODE='23514';
    END IF;
    FOR member IN
        SELECT membership.output_ordinal,membership.intent_id,intent.owner_kind,intent.owner_id,
               intent.output_ordinal AS intent_output_ordinal,intent.state,intent.vault_receipt
          FROM embedding_job_material_intents AS membership
          JOIN material_key_creation_intents AS intent
            ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
         WHERE membership.workspace_id=target_workspace_id AND membership.job_id=job_row.id
         ORDER BY membership.output_ordinal,membership.intent_id
         FOR UPDATE OF membership,intent
    LOOP
        SELECT receipt.vault_receipt INTO receipt_vault_receipt
          FROM embedding_output_key_receipts AS receipt
         WHERE receipt.workspace_id=target_workspace_id AND receipt.job_id=job_row.id
           AND receipt.output_ordinal=member.output_ordinal AND receipt.intent_id=member.intent_id
         FOR UPDATE;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding output execution requires every exact output receipt' USING ERRCODE='23514';
        END IF;
        PERFORM 1 FROM embedding_output_key_retirement_requests AS retirement
         WHERE retirement.workspace_id=target_workspace_id AND retirement.job_id=job_row.id
           AND retirement.output_ordinal=member.output_ordinal AND retirement.intent_id=member.intent_id
         FOR UPDATE;
        retired:=FOUND;
        IF member.output_ordinal<>output_position
           OR member.owner_kind<>'embedding_job_output' OR member.owner_id<>job_row.id
           OR member.intent_output_ordinal<>member.output_ordinal
           OR member.state<>'provisional_receipted' OR member.vault_receipt IS NULL
           OR receipt_vault_receipt IS DISTINCT FROM member.vault_receipt OR retired THEN
            RAISE EXCEPTION 'embedding output execution requires a complete exact receipted output set' USING ERRCODE='23514';
        END IF;
        output_position:=output_position+1;
    END LOOP;
    IF output_position=0 THEN
      RAISE EXCEPTION 'embedding output execution requires a nonempty exact receipted output set' USING ERRCODE='23514';
    END IF;
    IF EXISTS(SELECT 1 FROM external_effect_lifecycle_transitions
      WHERE workspace_id=target_workspace_id AND effect_id=target_effect_id AND status='dispatching') THEN
      RAISE EXCEPTION 'embedding job has historical provider dispatch evidence' USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM connection_dispatch_admissions AS admission JOIN provider_concurrency_leases AS lease
      ON lease.workspace_id=admission.workspace_id AND lease.external_effect_id=admission.external_effect_id
      WHERE admission.workspace_id=target_workspace_id AND admission.external_effect_id=target_effect_id
        AND admission.decision='admitted' AND admission.connection_id=initial.connection_id
        AND admission.model_binding_snapshot_id=initial.model_binding_snapshot_id
        AND lease.released_at IS NULL AND lease.expires_at>NOW()) THEN
      RAISE EXCEPTION 'embedding dispatch requires its active admitted lease' USING ERRCODE='23514';
    END IF;
    IF job_row.state='requested' THEN
      UPDATE embedding_jobs AS job SET state='running',version=job.version+1
       WHERE job.workspace_id=target_workspace_id AND job.id=job_row.id AND job.state='requested';
      IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job changed before dispatch state transition' USING ERRCODE='40001';
      END IF;
    ELSIF job_row.state<>'running' THEN
      RAISE EXCEPTION 'embedding job is terminal before provider dispatch' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM embedding_data_policy_decisions
         WHERE purpose = 'delivery'
         GROUP BY causal_reference_id, delivery_attempt
        HAVING count(*) > 1
    ) THEN
        RAISE EXCEPTION 'delivery data-policy decisions must have one exact cause and attempt'
            USING ERRCODE = '23514';
    END IF;
END $$;

CREATE UNIQUE INDEX embedding_data_policy_delivery_cause_attempt_key
    ON embedding_data_policy_decisions (causal_reference_id, delivery_attempt)
    WHERE purpose = 'delivery';

CREATE TABLE embedding_space_corpus_states (
    workspace_id UUID NOT NULL,
    space_registration_id UUID NOT NULL,
    corpus_revision BIGINT NOT NULL DEFAULT 0 CHECK (corpus_revision >= 0),
    next_projection_ordinal BIGINT NOT NULL DEFAULT 1 CHECK (next_projection_ordinal > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, space_registration_id),
    FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT
);

CREATE TABLE embedding_index_generation_guards (
    workspace_id UUID NOT NULL,
    space_registration_id UUID NOT NULL,
    generation_epoch BIGINT NOT NULL DEFAULT 0 CHECK (generation_epoch >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, space_registration_id),
    FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT
);

INSERT INTO embedding_space_corpus_states (workspace_id, space_registration_id)
SELECT workspace_id, id FROM embedding_space_registrations
ON CONFLICT (workspace_id, space_registration_id) DO NOTHING;

INSERT INTO embedding_index_generation_guards (workspace_id, space_registration_id)
SELECT workspace_id, id FROM embedding_space_registrations
ON CONFLICT (workspace_id, space_registration_id) DO NOTHING;

-- A registration can be created long after this migration's upgrade backfill.
-- The guarded registration command must therefore create the two locks with
-- the registration instead of leaving a future space executable without them.
CREATE OR REPLACE FUNCTION vestrace_create_embedding_result_space_guards()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF NEW.workspace_id IS NULL OR NEW.id IS NULL THEN
        RAISE EXCEPTION 'embedding result space guard requires an exact registration identity'
            USING ERRCODE='22023';
    END IF;
    INSERT INTO embedding_space_corpus_states(workspace_id,space_registration_id)
    VALUES(NEW.workspace_id,NEW.id)
    ON CONFLICT(workspace_id,space_registration_id) DO NOTHING;
    INSERT INTO embedding_index_generation_guards(workspace_id,space_registration_id)
    VALUES(NEW.workspace_id,NEW.id)
    ON CONFLICT(workspace_id,space_registration_id) DO NOTHING;
    RETURN NEW;
END $$;

CREATE TRIGGER embedding_result_space_guard_on_registration
    AFTER INSERT ON embedding_space_registrations
    FOR EACH ROW EXECUTE FUNCTION vestrace_create_embedding_result_space_guards();

ALTER TABLE embedding_space_registrations
    ADD CONSTRAINT embedding_space_registrations_result_model_dimension_key
    UNIQUE (workspace_id,id,model,dimensions);

ALTER TABLE embedding_delivery_source_memberships
    ADD COLUMN source_intent_id UUID;

-- 0193's `intent_id` names the job-owned output intent. Result preparation
-- must additionally retain the intent that owns the Live source material.
UPDATE embedding_delivery_source_memberships membership
   SET source_intent_id=material.intent_id
  FROM content_materials material
 WHERE material.workspace_id=membership.workspace_id
   AND material.id=membership.source_material_id;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM embedding_delivery_source_memberships membership
          LEFT JOIN content_materials material
            ON material.workspace_id=membership.workspace_id
           AND material.id=membership.source_material_id
          LEFT JOIN material_key_creation_intents source_intent
            ON source_intent.workspace_id=material.workspace_id
           AND source_intent.id=material.intent_id
         WHERE membership.source_intent_id IS NULL
            OR material.intent_id IS DISTINCT FROM membership.source_intent_id
            OR material.state<>'live'
            OR source_intent.state<>'live'
    ) THEN
        RAISE EXCEPTION 'result preparation requires every legacy source membership to name its exact Live source intent'
            USING ERRCODE='23514';
    END IF;
END $$;

ALTER TABLE embedding_delivery_source_memberships
    ALTER COLUMN source_intent_id SET NOT NULL,
    ADD CONSTRAINT embedding_delivery_source_memberships_source_intent_exact_fkey
        FOREIGN KEY (source_material_id,workspace_id,source_intent_id)
        REFERENCES content_materials(id,workspace_id,intent_id) ON DELETE RESTRICT;

CREATE OR REPLACE FUNCTION vestrace_assign_embedding_delivery_source_intent()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE source_material content_materials%ROWTYPE;
        source_intent material_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO source_material FROM content_materials
     WHERE workspace_id=NEW.workspace_id AND id=NEW.source_material_id FOR KEY SHARE;
    IF NOT FOUND OR source_material.state<>'live' THEN
        RAISE EXCEPTION 'delivery source membership requires its exact Live source material'
            USING ERRCODE='23514';
    END IF;
    SELECT * INTO source_intent FROM material_key_creation_intents
     WHERE workspace_id=source_material.workspace_id AND id=source_material.intent_id FOR KEY SHARE;
    IF NOT FOUND OR source_intent.state<>'live' THEN
        RAISE EXCEPTION 'delivery source membership requires its exact Live source intent'
            USING ERRCODE='23514';
    END IF;
    NEW.source_intent_id:=source_intent.id;
    RETURN NEW;
END $$;

CREATE TRIGGER embedding_delivery_source_memberships_source_intent_assign
    BEFORE INSERT ON embedding_delivery_source_memberships
    FOR EACH ROW EXECUTE FUNCTION vestrace_assign_embedding_delivery_source_intent();

ALTER TABLE embedding_delivery_source_memberships
    ADD CONSTRAINT embedding_delivery_source_memberships_result_exact_key
    UNIQUE (workspace_id,job_id,output_ordinal,source_ordinal,source_material_id,
            intent_id,source_intent_id,blocker_id);

CREATE TABLE embedding_job_result_preparations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    job_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    space_registration_id UUID NOT NULL,
    model_binding_snapshot_id UUID NOT NULL,
    model_request_evidence_id UUID NOT NULL,
    expected_job_version BIGINT NOT NULL CHECK (expected_job_version > 0),
    adapter TEXT NOT NULL CHECK (btrim(adapter) <> ''),
    auth_branch TEXT NOT NULL CHECK (auth_branch IN ('credential','no_auth')),
    credential_revision_id UUID,
    credential_intent_id UUID,
    credential_erasure_blocker_id UUID,
    response_model TEXT NOT NULL CHECK (btrim(response_model) <> ''),
    output_count BIGINT NOT NULL CHECK (output_count > 0),
    data_policy_decision_id UUID NOT NULL,
    receipt_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, job_id),
    UNIQUE (workspace_id, external_effect_id),
    UNIQUE (workspace_id, receipt_id),
    UNIQUE (id, workspace_id),
    UNIQUE (workspace_id,id,job_id,space_registration_id,model_binding_snapshot_id,
            data_policy_decision_id,response_model),
    FOREIGN KEY (workspace_id, job_id) REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, model_binding_snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, model_request_evidence_id)
        REFERENCES model_request_evidence_roots(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, credential_intent_id)
        REFERENCES credential_key_creation_intents(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (credential_erasure_blocker_id, workspace_id)
        REFERENCES material_erasure_blockers(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (receipt_id, workspace_id)
        REFERENCES external_effect_receipts(id, workspace_id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT embedding_result_preparation_policy_fkey FOREIGN KEY (data_policy_decision_id)
        REFERENCES embedding_data_policy_decisions(id) ON DELETE RESTRICT,
    CONSTRAINT embedding_result_preparation_auth_branch_exact CHECK (
        (auth_branch='credential' AND credential_revision_id IS NOT NULL
            AND credential_intent_id IS NOT NULL AND credential_erasure_blocker_id IS NOT NULL)
        OR (auth_branch='no_auth' AND credential_revision_id IS NULL
            AND credential_intent_id IS NULL AND credential_erasure_blocker_id IS NULL)
    )
);

CREATE TABLE embedding_projection_entries (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    preparation_id UUID NOT NULL,
    job_id UUID NOT NULL,
    input_ordinal BIGINT NOT NULL CHECK (input_ordinal >= 0),
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal >= 0),
    space_registration_id UUID NOT NULL,
    projection_ordinal BIGINT NOT NULL CHECK (projection_ordinal > 0),
    material_id UUID NOT NULL,
    intent_id UUID NOT NULL,
    material_key_id UUID NOT NULL,
    response_index BIGINT NOT NULL CHECK (response_index >= 0),
    response_model TEXT NOT NULL CHECK (btrim(response_model) <> ''),
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    model_binding_snapshot_id UUID NOT NULL,
    data_policy_decision_id UUID NOT NULL,
    sensitivity TEXT NOT NULL CHECK (sensitivity IN ('public','internal','confidential','restricted')),
    classification_labels TEXT[] NOT NULL CHECK (array_position(classification_labels, '') IS NULL),
    has_unclassified BOOLEAN NOT NULL,
    retention_eligibility_state TEXT NOT NULL
        CHECK (retention_eligibility_state = 'blocked_result_finalizing'),
    state TEXT NOT NULL CHECK (state = 'result_finalizing'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_projection_response_index_is_input_ordinal
        CHECK (response_index=input_ordinal),
    UNIQUE (workspace_id, job_id, input_ordinal),
    UNIQUE (workspace_id, space_registration_id, projection_ordinal),
    UNIQUE (id, workspace_id),
    FOREIGN KEY (preparation_id, workspace_id)
        REFERENCES embedding_job_result_preparations(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id,preparation_id,job_id,space_registration_id,
                 model_binding_snapshot_id,data_policy_decision_id,response_model)
        REFERENCES embedding_job_result_preparations(
            workspace_id,id,job_id,space_registration_id,model_binding_snapshot_id,
            data_policy_decision_id,response_model
        ) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, job_id, output_ordinal)
        REFERENCES embedding_job_material_intents(workspace_id, job_id, output_ordinal) ON DELETE RESTRICT,
    FOREIGN KEY (material_id, workspace_id) REFERENCES content_materials(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id,space_registration_id,response_model,dimensions)
        REFERENCES embedding_space_registrations(workspace_id,id,model,dimensions)
        ON DELETE RESTRICT,
    CONSTRAINT embedding_projection_policy_fkey FOREIGN KEY (data_policy_decision_id)
        REFERENCES embedding_data_policy_decisions(id) ON DELETE RESTRICT,
    UNIQUE (workspace_id,id,job_id,output_ordinal)
);

CREATE TABLE embedding_job_result_prepared_attachments (
    workspace_id UUID NOT NULL,
    preparation_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal >= 0),
    intent_id UUID NOT NULL,
    prepared_attachment_id UUID NOT NULL,
    projection_id UUID NOT NULL,
    PRIMARY KEY (workspace_id, preparation_id, output_ordinal),
    UNIQUE (workspace_id, intent_id),
    UNIQUE (workspace_id, prepared_attachment_id),
    FOREIGN KEY (preparation_id, workspace_id)
        REFERENCES embedding_job_result_preparations(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id, intent_id)
        REFERENCES material_key_creation_intents(workspace_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (projection_id, workspace_id)
        REFERENCES embedding_projection_entries(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (prepared_attachment_id)
        REFERENCES prepared_material_attachments(id) ON DELETE RESTRICT
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE embedding_projection_source_dependencies (
    workspace_id UUID NOT NULL,
    projection_id UUID NOT NULL,
    job_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal >= 0),
    source_ordinal INTEGER NOT NULL CHECK (source_ordinal >= 0),
    source_material_id UUID NOT NULL,
    output_intent_id UUID NOT NULL,
    source_intent_id UUID NOT NULL,
    erasure_blocker_id UUID NOT NULL,
    PRIMARY KEY (workspace_id, projection_id, source_ordinal),
    FOREIGN KEY (projection_id, workspace_id)
        REFERENCES embedding_projection_entries(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id,projection_id,job_id,output_ordinal)
        REFERENCES embedding_projection_entries(workspace_id,id,job_id,output_ordinal)
        ON DELETE RESTRICT,
    FOREIGN KEY (workspace_id,job_id,output_ordinal,source_ordinal,
                 source_material_id,output_intent_id,source_intent_id,erasure_blocker_id)
        REFERENCES embedding_delivery_source_memberships(
            workspace_id,job_id,output_ordinal,source_ordinal,
            source_material_id,intent_id,source_intent_id,blocker_id
        ) ON DELETE RESTRICT,
    FOREIGN KEY (source_material_id,workspace_id,source_intent_id)
        REFERENCES content_materials(id,workspace_id,intent_id) ON DELETE RESTRICT,
    FOREIGN KEY (output_intent_id,workspace_id)
        REFERENCES material_key_creation_intents(id,workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (source_intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (erasure_blocker_id, workspace_id)
        REFERENCES material_erasure_blockers(id, workspace_id) ON DELETE RESTRICT
);

DO $$
DECLARE target REGCLASS;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'embedding_space_corpus_states'::REGCLASS,
        'embedding_index_generation_guards'::REGCLASS,
        'embedding_job_result_preparations'::REGCLASS,
        'embedding_projection_entries'::REGCLASS,
        'embedding_job_result_prepared_attachments'::REGCLASS,
        'embedding_projection_source_dependencies'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', target);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', target);
        EXECUTE format('REVOKE ALL ON TABLE %s FROM PUBLIC', target);
        EXECUTE format('CREATE POLICY %I ON %s USING (workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',true),'''')::UUID) WITH CHECK (workspace_id=NULLIF(current_setting(''vestrace.workspace_id'',true),'''')::UUID)',
                       replace(target::TEXT, '.', '_') || '_workspace_policy', target);
    END LOOP;
END $$;

CREATE TRIGGER embedding_job_result_preparations_immutable
    BEFORE UPDATE OR DELETE ON embedding_job_result_preparations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_projection_entries_immutable
    BEFORE UPDATE OR DELETE ON embedding_projection_entries
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_job_result_prepared_attachments_immutable
    BEFORE UPDATE OR DELETE ON embedding_job_result_prepared_attachments
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_projection_source_dependencies_immutable
    BEFORE UPDATE OR DELETE ON embedding_projection_source_dependencies
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_result_preparation()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE preparation embedding_job_result_preparations%ROWTYPE;
        attachment_count BIGINT; projection_count BIGINT; output_count BIGINT;
        target_preparation_id UUID; target_workspace UUID;
BEGIN
    IF TG_TABLE_NAME='embedding_job_result_preparations' THEN
        target_preparation_id:=COALESCE(NEW.id,OLD.id);
    ELSE
        target_preparation_id:=COALESCE(NEW.preparation_id,OLD.preparation_id);
    END IF;
    target_workspace:=COALESCE(NEW.workspace_id,OLD.workspace_id);
    SELECT * INTO preparation FROM embedding_job_result_preparations
     WHERE id=target_preparation_id AND workspace_id=target_workspace;
    IF NOT FOUND THEN RETURN NULL; END IF;
    SELECT count(*) INTO output_count FROM embedding_job_material_intents
     WHERE workspace_id=preparation.workspace_id AND job_id=preparation.job_id;
    SELECT count(*) INTO attachment_count FROM embedding_job_result_prepared_attachments
     WHERE workspace_id=preparation.workspace_id AND preparation_id=preparation.id;
    SELECT count(*) INTO projection_count FROM embedding_projection_entries
     WHERE workspace_id=preparation.workspace_id AND preparation_id=preparation.id;
    IF output_count <> preparation.output_count OR attachment_count <> output_count OR projection_count <> output_count
       OR EXISTS (
           SELECT 1 FROM embedding_job_material_intents member
            LEFT JOIN embedding_job_result_prepared_attachments attachment
              ON attachment.workspace_id=member.workspace_id AND attachment.preparation_id=preparation.id
             AND attachment.output_ordinal=member.output_ordinal AND attachment.intent_id=member.intent_id
           LEFT JOIN embedding_projection_entries projection
              ON projection.workspace_id=member.workspace_id AND projection.preparation_id=preparation.id
             AND projection.output_ordinal=member.output_ordinal AND projection.intent_id=member.intent_id
           WHERE member.workspace_id=preparation.workspace_id AND member.job_id=preparation.job_id
             AND (attachment.intent_id IS NULL OR projection.id IS NULL)
       )
       OR EXISTS (
           SELECT 1 FROM embedding_job_result_prepared_attachments attachment
           JOIN embedding_projection_entries projection
             ON projection.workspace_id=attachment.workspace_id AND projection.id=attachment.projection_id
           JOIN prepared_material_attachments material_attachment
             ON material_attachment.id=attachment.prepared_attachment_id
      LEFT JOIN embedding_data_policy_decisions decision
             ON decision.id=projection.data_policy_decision_id
           WHERE attachment.workspace_id=preparation.workspace_id AND attachment.preparation_id=preparation.id
             AND (attachment.intent_id <> material_attachment.intent_id OR projection.intent_id <> attachment.intent_id
                  OR decision.id IS NULL
                  OR projection.response_model <> preparation.response_model
                  OR projection.data_policy_decision_id <> preparation.data_policy_decision_id
                  OR projection.sensitivity <> decision.classification
                  OR projection.classification_labels <> decision.classification_labels
                  OR projection.has_unclassified <> (decision.unclassified_count>0)
                  OR projection.state <> 'result_finalizing'
                  OR projection.retention_eligibility_state <> 'blocked_result_finalizing')
       )
       OR NOT EXISTS (
           SELECT 1 FROM external_effect_receipts receipt
            WHERE receipt.workspace_id=preparation.workspace_id AND receipt.effect_id=preparation.external_effect_id
              AND receipt.id=preparation.receipt_id AND receipt.outcome_status='acknowledged'
              AND receipt.payload @> jsonb_build_object(
                  'evidence_refs', jsonb_build_array(
                      format('embedding_job_result_preparation:%s', preparation.id)
                  )
              )
       )
       OR EXISTS (
           SELECT 1 FROM model_binding_snapshots snapshot
            WHERE snapshot.workspace_id=preparation.workspace_id AND snapshot.id=preparation.model_binding_snapshot_id
              AND (snapshot.branch<>preparation.auth_branch
                OR (snapshot.branch='credential' AND NOT EXISTS(
                    SELECT 1 FROM credential_key_creation_intents intent
                    JOIN material_erasure_blockers blocker
                      ON blocker.workspace_id=intent.workspace_id AND blocker.id=preparation.credential_erasure_blocker_id
                    WHERE intent.workspace_id=snapshot.workspace_id AND intent.id=preparation.credential_intent_id
                      AND intent.credential_revision_id=preparation.credential_revision_id
                      AND intent.credential_revision_id=snapshot.credential_revision_id
                      AND blocker.target_kind='credential' AND blocker.credential_intent_id=intent.id
                      AND blocker.state='nonterminal'
                )))
       )
       OR NOT EXISTS (
           SELECT 1 FROM model_binding_snapshots snapshot
            WHERE snapshot.workspace_id=preparation.workspace_id AND snapshot.id=preparation.model_binding_snapshot_id
       ) THEN
        RAISE EXCEPTION 'embedding result marker requires its complete exact output set and receipt'
            USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION vestrace_validate_embedding_projection_dependency()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE projection embedding_projection_entries%ROWTYPE; dependency_count BIGINT; source_count BIGINT;
        target_projection_id UUID; target_workspace UUID;
BEGIN
    IF TG_TABLE_NAME='embedding_projection_entries' THEN
        target_projection_id:=COALESCE(NEW.id,OLD.id);
        target_workspace:=COALESCE(NEW.workspace_id,OLD.workspace_id);
    ELSE
        target_projection_id:=COALESCE(NEW.projection_id,OLD.projection_id);
        target_workspace:=COALESCE(NEW.workspace_id,OLD.workspace_id);
    END IF;
    SELECT * INTO projection FROM embedding_projection_entries
     WHERE id=target_projection_id AND workspace_id=target_workspace;
    IF NOT FOUND THEN RETURN NULL; END IF;
    SELECT count(*) INTO dependency_count FROM embedding_projection_source_dependencies
     WHERE workspace_id=projection.workspace_id AND projection_id=projection.id;
    SELECT count(*) INTO source_count FROM embedding_delivery_source_memberships
     WHERE workspace_id=projection.workspace_id AND job_id=projection.job_id
       AND output_ordinal=projection.output_ordinal;
    IF dependency_count <> source_count OR EXISTS (
        SELECT 1 FROM embedding_delivery_source_memberships source
         LEFT JOIN embedding_projection_source_dependencies dependency
          ON dependency.workspace_id=source.workspace_id AND dependency.projection_id=projection.id
          AND dependency.source_ordinal=source.source_ordinal
          AND dependency.source_material_id=source.source_material_id
          AND dependency.output_intent_id=source.intent_id
          AND dependency.source_intent_id=source.source_intent_id
          AND dependency.erasure_blocker_id=source.blocker_id
        WHERE source.workspace_id=projection.workspace_id AND source.job_id=projection.job_id
          AND source.output_ordinal=projection.output_ordinal AND dependency.projection_id IS NULL
    ) OR EXISTS (
        SELECT 1 FROM embedding_projection_source_dependencies dependency
        JOIN content_materials material
          ON material.workspace_id=dependency.workspace_id AND material.id=dependency.source_material_id
         AND material.intent_id=dependency.source_intent_id
        JOIN material_key_creation_intents source_intent
          ON source_intent.workspace_id=dependency.workspace_id AND source_intent.id=dependency.source_intent_id
        WHERE dependency.workspace_id=projection.workspace_id AND dependency.projection_id=projection.id
          AND (material.state<>'live' OR source_intent.state<>'live')
    ) THEN
        RAISE EXCEPTION 'embedding projection requires every exact ordered delivery source dependency'
            USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER embedding_result_preparation_complete
    AFTER INSERT OR UPDATE ON embedding_job_result_preparations
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_result_preparation();
CREATE CONSTRAINT TRIGGER embedding_result_attachment_complete
    AFTER INSERT OR UPDATE ON embedding_job_result_prepared_attachments
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_result_preparation();
CREATE CONSTRAINT TRIGGER embedding_projection_dependency_complete
    AFTER INSERT OR UPDATE ON embedding_projection_source_dependencies
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_projection_dependency();
CREATE CONSTRAINT TRIGGER embedding_projection_entry_dependency_complete
    AFTER INSERT OR UPDATE ON embedding_projection_entries
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION vestrace_validate_embedding_projection_dependency();

CREATE OR REPLACE FUNCTION vestrace_lock_embedding_result_completion_authority(
    target_workspace UUID,target_job UUID,target_effect UUID,target_connection UUID,
    target_connection_revision UUID,target_dispatch_transition UUID,target_lease UUID
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE cause provider_dispatch_causes%ROWTYPE; job embedding_jobs%ROWTYPE;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding result completion authority arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM * FROM vestrace_lock_provider_dispatch_completion_authority(
        target_workspace,target_effect,target_connection,target_connection_revision,target_dispatch_transition,target_lease
    );
    -- The shared authority already owns the permanent connection guard and
    -- holds its immutable dispatch cause/admission/lease/transition tuple.
    -- Read the embedding identity without taking the job/effect lock: the
    -- guarded commit takes those only after credential, space, policy and
    -- source locks in its canonical embedding-specific order.
    SELECT * INTO cause FROM provider_dispatch_causes
     WHERE workspace_id=target_workspace AND external_effect_id=target_effect;
    SELECT * INTO job FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job;
    IF job.id IS NULL OR cause.cause_kind <> 'embedding_job' OR cause.embedding_job_id <> target_job
       OR job.external_effect_id <> target_effect OR job.kind <> 'delivery' OR job.state <> 'running'
       OR EXISTS (SELECT 1 FROM embedding_job_pre_dispatch_retirement_authorities termination
                   WHERE termination.workspace_id=target_workspace AND termination.job_id=target_job) THEN
        RAISE EXCEPTION 'embedding result completion authority is not its active delivery job'
            USING ERRCODE='23514';
    END IF;
    RETURN job.id;
END $$;

CREATE OR REPLACE FUNCTION vestrace_load_embedding_result_eligibility(
    target_workspace UUID,target_job UUID,target_effect UUID
) RETURNS TABLE(
    preparation_id UUID, preparation_output_count BIGINT, expected_job_version BIGINT, response_model TEXT, adapter TEXT,
    output_ordinal BIGINT, intent_id UUID, material_id UUID, material_key_id UUID,
    intent_nonce UUID, dimensions INTEGER, preparation_input_ordinal BIGINT,
    preparation_response_index BIGINT
) LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE job embedding_jobs%ROWTYPE; registration embedding_space_registrations%ROWTYPE;
        existing embedding_job_result_preparations%ROWTYPE;
        acceptance embedding_delivery_acceptance_receipts%ROWTYPE;
        policy_cause UUID; policy_attempt INTEGER; marker_complete BOOLEAN;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding result eligibility arguments are malformed' USING ERRCODE='22023';
    END IF;
    -- This loader is marker-first and performs no row lock.  Its caller has
    -- already taken the shared completion prefix; the commit command owns the
    -- canonical embedding-specific lock sequence if preparation is needed.
    SELECT * INTO job FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job;
    IF NOT FOUND OR job.kind <> 'delivery' OR job.external_effect_id <> target_effect THEN
        RAISE EXCEPTION 'embedding result requires its exact delivery job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO registration FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=job.space_registration_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding result eligibility registration is absent' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=job.space_registration_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding result eligibility corpus guard is absent' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=job.space_registration_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding result eligibility generation guard is absent' USING ERRCODE='23514';
    END IF;
    -- An existing marker is authority only when all of the facts it promises
    -- are still present and exact.  A row that was only partially persisted
    -- (or later found corrupted) must not suppress the normal guarded path or
    -- let a replay skip vault work.  Caller-generated ids and ciphertext are
    -- deliberately absent from this semantic check.
    SELECT * INTO existing FROM embedding_job_result_preparations marker
     WHERE marker.workspace_id=target_workspace AND marker.job_id=target_job;
    IF FOUND THEN
        SELECT * INTO acceptance FROM embedding_delivery_acceptance_receipts receipt
         WHERE receipt.workspace_id=target_workspace AND receipt.job_id=target_job;
        IF NOT FOUND
           OR jsonb_typeof(acceptance.request_tuple->'outbox')<>'array'
           OR jsonb_array_length(acceptance.request_tuple->'outbox')<1
           OR jsonb_typeof(acceptance.request_tuple #> '{outbox,0,id}')<>'string'
           OR jsonb_typeof(acceptance.request_tuple #> '{outbox,0,attempts}')<>'number'
           OR (acceptance.request_tuple #>> '{outbox,0,attempts}') !~ '^(0|[1-9][0-9]*)$'
           OR (acceptance.request_tuple #>> '{outbox,0,attempts}')::NUMERIC > 2147483646 THEN
            RAISE EXCEPTION 'embedding result marker has no exact delivery policy authority'
                USING ERRCODE='23514';
        END IF;
        policy_cause := (acceptance.request_tuple #>> '{outbox,0,id}')::UUID;
        policy_attempt := (acceptance.request_tuple #>> '{outbox,0,attempts}')::INTEGER+1;
        SELECT EXISTS(
            SELECT 1
              FROM embedding_job_result_preparations marker
              JOIN external_effect_intents effect
                ON effect.workspace_id=marker.workspace_id AND effect.id=marker.external_effect_id
              JOIN model_binding_snapshots snapshot
                ON snapshot.workspace_id=marker.workspace_id AND snapshot.id=marker.model_binding_snapshot_id
              JOIN embedding_data_policy_decisions decision ON decision.id=marker.data_policy_decision_id
              JOIN external_effect_receipts result_receipt
                ON result_receipt.workspace_id=marker.workspace_id AND result_receipt.id=marker.receipt_id
               AND result_receipt.effect_id=marker.external_effect_id AND result_receipt.outcome_status='acknowledged'
               AND result_receipt.payload @> jsonb_build_object(
                   'evidence_refs',jsonb_build_array(
                       format('embedding_job_result_preparation:%s',marker.id)
                   )
               )
             WHERE marker.workspace_id=target_workspace AND marker.id=existing.id
               AND marker.job_id=job.id AND marker.external_effect_id=target_effect
               AND marker.space_registration_id=job.space_registration_id
               AND marker.model_binding_snapshot_id=job.model_binding_snapshot_id
               AND marker.model_request_evidence_id=job.model_request_evidence_id
               AND marker.expected_job_version=job.version AND job.state='running'
               AND marker.adapter=effect.adapter AND marker.response_model=registration.model
               AND decision.purpose='delivery' AND decision.causal_reference_id=policy_cause
               AND decision.delivery_attempt=policy_attempt AND decision.input_count=marker.output_count
               AND decision.verdict='allowed'
               AND marker.output_count=(SELECT count(*) FROM embedding_job_material_intents member
                                          WHERE member.workspace_id=marker.workspace_id AND member.job_id=marker.job_id)
               AND marker.output_count=(SELECT count(*) FROM embedding_job_result_prepared_attachments attachment
                                          WHERE attachment.workspace_id=marker.workspace_id AND attachment.preparation_id=marker.id)
               AND marker.output_count=(SELECT count(*) FROM embedding_projection_entries projection
                                          WHERE projection.workspace_id=marker.workspace_id AND projection.preparation_id=marker.id)
               AND NOT EXISTS(
                   SELECT 1 FROM embedding_job_material_intents member
                   JOIN material_key_creation_intents output_intent
                     ON output_intent.workspace_id=member.workspace_id AND output_intent.id=member.intent_id
                   LEFT JOIN embedding_job_result_prepared_attachments attachment
                     ON attachment.workspace_id=member.workspace_id AND attachment.preparation_id=marker.id
                    AND attachment.output_ordinal=member.output_ordinal AND attachment.intent_id=member.intent_id
                   LEFT JOIN prepared_material_attachments prepared_attachment
                     ON prepared_attachment.id=attachment.prepared_attachment_id
                   LEFT JOIN embedding_projection_entries projection
                     ON projection.workspace_id=member.workspace_id AND projection.preparation_id=marker.id
                    AND projection.output_ordinal=member.output_ordinal
                   WHERE member.workspace_id=marker.workspace_id AND member.job_id=marker.job_id
                     AND (attachment.preparation_id IS NULL OR prepared_attachment.id IS NULL
                       OR prepared_attachment.intent_id<>member.intent_id
                       OR projection.id IS NULL OR projection.job_id<>marker.job_id
                       OR projection.input_ordinal<>member.output_ordinal
                       OR projection.response_index<>member.output_ordinal
                       OR projection.intent_id<>member.intent_id
                       OR projection.material_id<>output_intent.material_id
                       OR projection.material_key_id<>output_intent.material_key_id
                       OR projection.space_registration_id<>marker.space_registration_id
                       OR projection.model_binding_snapshot_id<>marker.model_binding_snapshot_id
                       OR projection.data_policy_decision_id<>marker.data_policy_decision_id
                       OR projection.response_model<>marker.response_model
                       OR projection.dimensions<>registration.dimensions
                       OR projection.sensitivity<>decision.classification
                       OR projection.classification_labels<>decision.classification_labels
                       OR projection.has_unclassified<>(decision.unclassified_count>0)
                       OR projection.retention_eligibility_state<>'blocked_result_finalizing'
                       OR projection.state<>'result_finalizing')
               )
               AND NOT EXISTS(
                   SELECT 1 FROM embedding_projection_entries projection
                   LEFT JOIN embedding_job_material_intents member
                     ON member.workspace_id=projection.workspace_id AND member.job_id=marker.job_id
                    AND member.output_ordinal=projection.output_ordinal AND member.intent_id=projection.intent_id
                   WHERE projection.workspace_id=marker.workspace_id AND projection.preparation_id=marker.id
                     AND member.intent_id IS NULL
               )
               AND NOT EXISTS(
                   SELECT 1 FROM embedding_delivery_source_memberships source
                   JOIN embedding_projection_entries projection
                     ON projection.workspace_id=source.workspace_id AND projection.preparation_id=marker.id
                    AND projection.job_id=source.job_id AND projection.output_ordinal=source.output_ordinal
                   LEFT JOIN embedding_projection_source_dependencies dependency
                     ON dependency.workspace_id=source.workspace_id AND dependency.projection_id=projection.id
                    AND dependency.job_id=source.job_id AND dependency.output_ordinal=source.output_ordinal
                    AND dependency.source_ordinal=source.source_ordinal
                    AND dependency.source_material_id=source.source_material_id
                    AND dependency.output_intent_id=source.intent_id
                    AND dependency.source_intent_id=source.source_intent_id
                    AND dependency.erasure_blocker_id=source.blocker_id
                   JOIN content_materials source_material
                     ON source_material.workspace_id=source.workspace_id AND source_material.id=source.source_material_id
                    AND source_material.intent_id=source.source_intent_id
                   JOIN material_key_creation_intents source_intent
                     ON source_intent.workspace_id=source.workspace_id AND source_intent.id=source.source_intent_id
                   JOIN material_erasure_blockers blocker
                     ON blocker.workspace_id=source.workspace_id AND blocker.id=source.blocker_id
                   WHERE source.workspace_id=marker.workspace_id AND source.job_id=marker.job_id
                     AND (dependency.projection_id IS NULL OR source_material.state<>'live'
                       OR source_intent.state<>'live' OR blocker.state<>'nonterminal')
               )
               AND NOT EXISTS(
                   SELECT 1 FROM embedding_projection_source_dependencies dependency
                   JOIN embedding_projection_entries projection
                     ON projection.workspace_id=dependency.workspace_id AND projection.id=dependency.projection_id
                   LEFT JOIN embedding_delivery_source_memberships source
                     ON source.workspace_id=dependency.workspace_id AND source.job_id=dependency.job_id
                    AND source.output_ordinal=dependency.output_ordinal
                    AND source.source_ordinal=dependency.source_ordinal
                    AND source.source_material_id=dependency.source_material_id
                    AND source.intent_id=dependency.output_intent_id
                    AND source.source_intent_id=dependency.source_intent_id
                    AND source.blocker_id=dependency.erasure_blocker_id
                   WHERE projection.workspace_id=marker.workspace_id AND projection.preparation_id=marker.id
                     AND source.job_id IS NULL
               )
               AND ( (snapshot.branch='no_auth' AND marker.auth_branch='no_auth'
                       AND marker.credential_revision_id IS NULL AND marker.credential_intent_id IS NULL
                       AND marker.credential_erasure_blocker_id IS NULL)
                  OR (snapshot.branch='credential' AND marker.auth_branch='credential'
                       AND marker.credential_revision_id=snapshot.credential_revision_id
                       AND EXISTS(
                           SELECT 1 FROM credential_key_creation_intents credential_intent
                           JOIN material_erasure_blockers credential_blocker
                             ON credential_blocker.workspace_id=credential_intent.workspace_id
                            AND credential_blocker.id=marker.credential_erasure_blocker_id
                           WHERE credential_intent.workspace_id=marker.workspace_id
                             AND credential_intent.id=marker.credential_intent_id
                             AND credential_intent.credential_revision_id=marker.credential_revision_id
                             AND credential_intent.state='active'
                             AND credential_blocker.target_kind='credential'
                             AND credential_blocker.credential_intent_id=credential_intent.id
                             AND credential_blocker.state='nonterminal'
                       )))
               AND NOT EXISTS(SELECT 1 FROM embedding_job_pre_dispatch_retirement_authorities retirement
                               WHERE retirement.workspace_id=marker.workspace_id AND retirement.job_id=marker.job_id)
               AND NOT EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests retirement
                               WHERE retirement.workspace_id=marker.workspace_id AND retirement.job_id=marker.job_id)
        ) INTO marker_complete;
        IF NOT marker_complete THEN
            RAISE EXCEPTION 'embedding result marker is incomplete or semantically inconsistent'
                USING ERRCODE='23514';
        END IF;
    END IF;
    RETURN QUERY
    SELECT prepared_marker.id, prepared_marker.output_count,
           COALESCE(prepared_marker.expected_job_version,job.version),
           COALESCE(prepared_marker.response_model,registration.model),COALESCE(prepared_marker.adapter,effect.adapter),
           member.output_ordinal, member.intent_id, intent.material_id, intent.material_key_id,
           intent.nonce, COALESCE(projection.dimensions,registration.dimensions),
           projection.input_ordinal,projection.response_index
      FROM embedding_job_material_intents member
      JOIN material_key_creation_intents intent ON intent.workspace_id=member.workspace_id AND intent.id=member.intent_id
      JOIN external_effect_intents effect ON effect.workspace_id=member.workspace_id AND effect.id=target_effect
      LEFT JOIN embedding_job_result_preparations prepared_marker
        ON prepared_marker.workspace_id=member.workspace_id AND prepared_marker.job_id=member.job_id
      LEFT JOIN embedding_projection_entries projection
        ON projection.workspace_id=member.workspace_id AND projection.preparation_id=prepared_marker.id
       AND projection.output_ordinal=member.output_ordinal
     WHERE member.workspace_id=target_workspace AND member.job_id=target_job
     ORDER BY member.output_ordinal;
END $$;

-- Keep the shared embedding recovery vocabulary, but derive
-- ResumeResultPrepared from this delivery-only marker. The generic Run result
-- preparation is neither a substitute marker nor a proof that every delivery
-- output attachment committed.
CREATE OR REPLACE FUNCTION vestrace_lock_embedding_job_recovery_authority(
    target_workspace_id UUID,
    target_job_id UUID
)
RETURNS TABLE (
    job_id UUID, workspace_id UUID, space_registration_id UUID, kind TEXT, state TEXT,
    model_binding_snapshot_id UUID, external_effect_id UUID, model_request_evidence_id UUID,
    phase TEXT, connection_id UUID, connection_revision_id UUID, dispatch_transition_id UUID,
    dispatch_expires_at TIMESTAMPTZ, has_receipt BOOLEAN, has_result_preparation BOOLEAN
)
LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE initial embedding_jobs%ROWTYPE; job embedding_jobs%ROWTYPE; admission RECORD;
        root_connection_id UUID := NULL; canonical_connection_id UUID := NULL;
        canonical_connection_revision_id UUID := NULL; canonical_dispatch_transition_id UUID := NULL;
        canonical_dispatch_expires_at TIMESTAMPTZ := NULL; prepared embedding_job_result_preparations%ROWTYPE;
        receipt_exists BOOLEAN; preparation_exists BOOLEAN := FALSE; attachment_count BIGINT;
        member_count BIGINT; complete_result_prepared BOOLEAN := FALSE; derived_phase TEXT;
BEGIN
    IF target_workspace_id IS NULL OR target_job_id IS NULL
       OR target_workspace_id IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding job recovery authority arguments are malformed' USING ERRCODE='22023';
    END IF;
    SELECT stored.* INTO initial FROM embedding_jobs stored
     WHERE stored.workspace_id=target_workspace_id AND stored.id=target_job_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job is absent' USING ERRCODE='23514'; END IF;
    -- ResultPrepared moves output intents beyond the pre-dispatch provisional
    -- state. Only a complete immutable marker, its exact acknowledged receipt,
    -- every output attachment/projection, and every source dependency may
    -- bypass that readiness gate during recovery.
    SELECT EXISTS(
        SELECT 1 FROM embedding_job_result_preparations marker
        JOIN external_effect_receipts receipt
          ON receipt.workspace_id=marker.workspace_id AND receipt.id=marker.receipt_id
         AND receipt.effect_id=marker.external_effect_id AND receipt.outcome_status='acknowledged'
         AND receipt.payload @> jsonb_build_object(
             'evidence_refs',jsonb_build_array(
                 format('embedding_job_result_preparation:%s',marker.id)
             )
         )
         WHERE marker.workspace_id=target_workspace_id AND marker.job_id=initial.id
           AND marker.external_effect_id=initial.external_effect_id
           AND marker.output_count=(SELECT count(*) FROM embedding_job_material_intents member
                                    WHERE member.workspace_id=marker.workspace_id AND member.job_id=marker.job_id)
           AND marker.output_count=(SELECT count(*) FROM embedding_job_result_prepared_attachments attachment
                                    WHERE attachment.workspace_id=marker.workspace_id AND attachment.preparation_id=marker.id)
           AND marker.output_count=(SELECT count(*) FROM embedding_projection_entries projection
                                    WHERE projection.workspace_id=marker.workspace_id AND projection.preparation_id=marker.id)
           AND NOT EXISTS(
               SELECT 1 FROM embedding_job_material_intents member
                LEFT JOIN embedding_job_result_prepared_attachments attachment
                  ON attachment.workspace_id=member.workspace_id AND attachment.preparation_id=marker.id
                 AND attachment.output_ordinal=member.output_ordinal AND attachment.intent_id=member.intent_id
                LEFT JOIN embedding_projection_entries projection
                  ON projection.workspace_id=member.workspace_id AND projection.preparation_id=marker.id
                 AND projection.output_ordinal=member.output_ordinal AND projection.intent_id=member.intent_id
               WHERE member.workspace_id=marker.workspace_id AND member.job_id=marker.job_id
                 AND (attachment.intent_id IS NULL OR projection.id IS NULL)
           )
           AND NOT EXISTS(
               SELECT 1 FROM embedding_delivery_source_memberships source
                LEFT JOIN embedding_projection_entries projection
                  ON projection.workspace_id=source.workspace_id AND projection.preparation_id=marker.id
                 AND projection.job_id=source.job_id AND projection.output_ordinal=source.output_ordinal
                LEFT JOIN embedding_projection_source_dependencies dependency
                  ON dependency.workspace_id=source.workspace_id AND dependency.projection_id=projection.id
                 AND dependency.job_id=source.job_id AND dependency.output_ordinal=source.output_ordinal
                 AND dependency.source_ordinal=source.source_ordinal
                 AND dependency.source_material_id=source.source_material_id
                 AND dependency.output_intent_id=source.intent_id
                 AND dependency.source_intent_id=source.source_intent_id
                 AND dependency.erasure_blocker_id=source.blocker_id
               WHERE source.workspace_id=marker.workspace_id AND source.job_id=marker.job_id
                 AND dependency.projection_id IS NULL
           )
    ) INTO complete_result_prepared;
    IF NOT complete_result_prepared THEN
        PERFORM vestrace_lock_embedding_job_pre_dispatch_gate(target_workspace_id,initial.external_effect_id,TRUE);
    END IF;
    SELECT stored.connection_id,stored.connection_revision_id INTO admission
      FROM connection_dispatch_admissions stored
     WHERE stored.workspace_id=target_workspace_id AND stored.external_effect_id=initial.external_effect_id
       AND stored.decision='admitted' AND stored.model_binding_snapshot_id=initial.model_binding_snapshot_id;
    IF FOUND THEN
        root_connection_id:=admission.connection_id; canonical_connection_id:=admission.connection_id;
        canonical_connection_revision_id:=admission.connection_revision_id;
        PERFORM guard.id FROM connection_execution_guards guard
         WHERE guard.workspace_id=target_workspace_id AND guard.connection_id=root_connection_id FOR UPDATE OF guard;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding job recovery requires its permanent connection guard' USING ERRCODE='23514'; END IF;
    END IF;
    SELECT stored.* INTO job FROM embedding_jobs stored
     WHERE stored.workspace_id=target_workspace_id AND stored.id=initial.id
       AND stored.model_binding_snapshot_id=initial.model_binding_snapshot_id
       AND stored.external_effect_id=initial.external_effect_id
       AND stored.model_request_evidence_id=initial.model_request_evidence_id FOR UPDATE OF stored;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding job identity changed before its recovery lock' USING ERRCODE='23514'; END IF;
    SELECT lifecycle.id,lifecycle.dispatch_expires_at INTO canonical_dispatch_transition_id,canonical_dispatch_expires_at
      FROM external_effect_lifecycle_transitions lifecycle
     WHERE lifecycle.workspace_id=target_workspace_id AND lifecycle.effect_id=job.external_effect_id
       AND lifecycle.status='dispatching' AND lifecycle.cause='dispatch_started'
       AND lifecycle.dispatch_expires_at IS NOT NULL
     ORDER BY lifecycle.ordinal DESC LIMIT 1 FOR KEY SHARE OF lifecycle;
    receipt_exists:=EXISTS(SELECT 1 FROM external_effect_receipts receipt
      WHERE receipt.workspace_id=job.workspace_id AND receipt.effect_id=job.external_effect_id);
    SELECT * INTO prepared FROM embedding_job_result_preparations marker
     WHERE marker.workspace_id=job.workspace_id AND marker.job_id=job.id
       AND marker.external_effect_id=job.external_effect_id FOR KEY SHARE OF marker;
    IF FOUND THEN
        receipt_exists:=EXISTS(SELECT 1 FROM external_effect_receipts receipt
          WHERE receipt.workspace_id=job.workspace_id AND receipt.effect_id=job.external_effect_id
            AND receipt.id=prepared.receipt_id AND receipt.outcome_status='acknowledged'
            AND receipt.payload @> jsonb_build_object(
                'evidence_refs', jsonb_build_array(
                    format('embedding_job_result_preparation:%s', prepared.id)
                )
            ));
        SELECT count(*) INTO attachment_count FROM embedding_job_result_prepared_attachments attachment
         WHERE attachment.workspace_id=job.workspace_id AND attachment.preparation_id=prepared.id;
        SELECT count(*) INTO member_count FROM embedding_job_material_intents member
         WHERE member.workspace_id=job.workspace_id AND member.job_id=job.id;
        preparation_exists:=receipt_exists AND prepared.output_count=attachment_count
          AND prepared.output_count=member_count
          AND NOT EXISTS(
             SELECT 1 FROM embedding_job_material_intents member
              LEFT JOIN embedding_job_result_prepared_attachments attachment
                ON attachment.workspace_id=member.workspace_id AND attachment.preparation_id=prepared.id
               AND attachment.output_ordinal=member.output_ordinal AND attachment.intent_id=member.intent_id
             WHERE member.workspace_id=job.workspace_id AND member.job_id=job.id AND attachment.intent_id IS NULL
          )
          AND NOT EXISTS(
             SELECT 1 FROM embedding_delivery_source_memberships source
              LEFT JOIN embedding_projection_entries projection
                ON projection.workspace_id=source.workspace_id AND projection.preparation_id=prepared.id
               AND projection.job_id=source.job_id AND projection.output_ordinal=source.output_ordinal
              LEFT JOIN embedding_projection_source_dependencies dependency
                ON dependency.workspace_id=source.workspace_id AND dependency.projection_id=projection.id
               AND dependency.job_id=source.job_id AND dependency.output_ordinal=source.output_ordinal
               AND dependency.source_ordinal=source.source_ordinal
               AND dependency.source_material_id=source.source_material_id
               AND dependency.output_intent_id=source.intent_id
               AND dependency.source_intent_id=source.source_intent_id
               AND dependency.erasure_blocker_id=source.blocker_id
             WHERE source.workspace_id=job.workspace_id AND source.job_id=job.id
               AND dependency.projection_id IS NULL
          );
    END IF;
    IF job.state='succeeded' THEN derived_phase:='succeeded';
    ELSIF job.state='inconclusive_unknown' THEN derived_phase:='unknown';
    ELSIF job.state IN ('cancelled','failed_definite') THEN derived_phase:=job.state;
    ELSIF receipt_exists AND preparation_exists THEN derived_phase:='result_prepared';
    ELSIF canonical_dispatch_transition_id IS NOT NULL THEN derived_phase:='dispatching';
    ELSIF root_connection_id IS NOT NULL THEN derived_phase:='admitted';
    ELSE derived_phase:='reserved'; END IF;
    RETURN QUERY SELECT job.id,job.workspace_id,job.space_registration_id,job.kind,job.state,
      job.model_binding_snapshot_id,job.external_effect_id,job.model_request_evidence_id,derived_phase,
      canonical_connection_id,canonical_connection_revision_id,canonical_dispatch_transition_id,
      canonical_dispatch_expires_at,receipt_exists,preparation_exists;
END $$;

CREATE OR REPLACE FUNCTION vestrace_commit_embedding_result_preparation(
    target_workspace UUID,target_job UUID,target_effect UUID,target_preparation UUID,target_receipt UUID,
    target_expected_version BIGINT,target_response_model TEXT,target_attachment_ids UUID[],
    target_ciphertexts BYTEA[],target_dimensions INTEGER[]
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
DECLARE initial_job embedding_jobs%ROWTYPE; job embedding_jobs%ROWTYPE; registration embedding_space_registrations%ROWTYPE;
        acceptance embedding_delivery_acceptance_receipts%ROWTYPE; existing embedding_job_result_preparations%ROWTYPE;
        decision embedding_data_policy_decisions%ROWTYPE; member RECORD; position INTEGER; projection_id UUID;
        projection_ordinal BIGINT; policy_cause UUID; policy_attempt INTEGER;
        snapshot model_binding_snapshots%ROWTYPE; credential_intent UUID := NULL; credential_blocker UUID := NULL;
        effect external_effect_intents%ROWTYPE; source RECORD; source_count BIGINT:=0;
        receipt_recorded_at TIMESTAMPTZ;
BEGIN
    IF target_workspace IS NULL OR target_job IS NULL OR target_effect IS NULL OR target_preparation IS NULL
       OR target_receipt IS NULL OR target_expected_version IS NULL OR target_response_model IS NULL
       OR cardinality(target_attachment_ids) IS NULL OR cardinality(target_attachment_ids)=0
       OR cardinality(target_attachment_ids) <> cardinality(target_ciphertexts)
       OR cardinality(target_attachment_ids) <> cardinality(target_dimensions)
       OR target_workspace IS DISTINCT FROM NULLIF(current_setting('vestrace.workspace_id',true),'')::UUID THEN
        RAISE EXCEPTION 'embedding result preparation arguments are malformed' USING ERRCODE='22023';
    END IF;
    -- The job/snapshot identities are append-only inputs only.  They are read
    -- without a row lock so the first mutable embedding row follows the
    -- shared completion prefix's permanent connection guard.
    SELECT * INTO initial_job FROM embedding_jobs
     WHERE workspace_id=target_workspace AND id=target_job;
    IF NOT FOUND OR initial_job.kind <> 'delivery' OR initial_job.external_effect_id <> target_effect THEN
        RAISE EXCEPTION 'embedding result requires its exact delivery job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO snapshot FROM model_binding_snapshots
     WHERE workspace_id=target_workspace AND id=initial_job.model_binding_snapshot_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result requires its model-binding snapshot' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM connection_execution_guards guard
     WHERE guard.workspace_id=target_workspace AND guard.connection_id=snapshot.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result connection guard is absent' USING ERRCODE='23514'; END IF;
    IF snapshot.branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards guard
         WHERE guard.id=snapshot.credential_activation_guard_id AND guard.workspace_id=target_workspace
           AND guard.connection_id=snapshot.connection_id AND guard.credential_slot_id=snapshot.credential_slot_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding result credential guard is absent' USING ERRCODE='23514'; END IF;
        SELECT intent.id INTO credential_intent FROM credential_key_creation_intents intent
         WHERE intent.workspace_id=target_workspace AND intent.connection_id=snapshot.connection_id
           AND intent.credential_slot_id=snapshot.credential_slot_id
           AND intent.credential_revision_id=snapshot.credential_revision_id AND intent.state='active' FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding result credential intent is not active' USING ERRCODE='23514'; END IF;
        SELECT blocker.id INTO credential_blocker FROM material_erasure_blockers blocker
         WHERE blocker.workspace_id=target_workspace AND blocker.target_kind='credential'
           AND blocker.credential_intent_id=credential_intent AND blocker.state='nonterminal'
         ORDER BY blocker.id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding result credential completion blocker is absent' USING ERRCODE='23514'; END IF;
    ELSIF snapshot.branch<>'no_auth' OR snapshot.credential_revision_id IS NOT NULL
       OR snapshot.credential_slot_id IS NOT NULL OR snapshot.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding result snapshot has an invalid no-auth branch' USING ERRCODE='23514';
    END IF;
    SELECT * INTO registration FROM embedding_space_registrations
     WHERE workspace_id=target_workspace AND id=initial_job.space_registration_id FOR UPDATE;
    IF NOT FOUND OR registration.model <> target_response_model THEN
        RAISE EXCEPTION 'embedding result response model is not its registered space model' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_corpus_states WHERE workspace_id=target_workspace AND space_registration_id=initial_job.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result corpus guard is absent' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM embedding_index_generation_guards WHERE workspace_id=target_workspace AND space_registration_id=initial_job.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result generation guard is absent' USING ERRCODE='23514'; END IF;
    SELECT * INTO acceptance FROM embedding_delivery_acceptance_receipts
     WHERE workspace_id=target_workspace AND job_id=target_job FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding result requires delivery acceptance output authority' USING ERRCODE='23514'; END IF;
    IF jsonb_typeof(acceptance.request_tuple->'outbox')<>'array'
       OR jsonb_array_length(acceptance.request_tuple->'outbox')<1
       OR jsonb_typeof(acceptance.request_tuple #> '{outbox,0,id}')<>'string'
       OR jsonb_typeof(acceptance.request_tuple #> '{outbox,0,attempts}')<>'number'
       OR (acceptance.request_tuple #>> '{outbox,0,attempts}') !~ '^(0|[1-9][0-9]*)$'
       OR (acceptance.request_tuple #>> '{outbox,0,attempts}')::NUMERIC > 2147483646 THEN
        RAISE EXCEPTION 'embedding result delivery policy attempt is absent or invalid' USING ERRCODE='23514';
    END IF;
    policy_cause := (acceptance.request_tuple #>> '{outbox,0,id}')::UUID;
    policy_attempt := (acceptance.request_tuple #>> '{outbox,0,attempts}')::INTEGER+1;
    SELECT * INTO decision FROM embedding_data_policy_decisions
     WHERE purpose='delivery' AND causal_reference_id=policy_cause AND delivery_attempt=policy_attempt FOR UPDATE;
    IF NOT FOUND OR decision.verdict <> 'allowed' OR decision.input_count <> cardinality(target_attachment_ids) THEN
        RAISE EXCEPTION 'embedding result requires its exact allowed delivery data-policy decision' USING ERRCODE='23514';
    END IF;
    -- Lock every persisted source fact before authoring any output.  The
    -- tuple is the 0193 membership identity, not a current corpus lookup.
    FOR source IN
        SELECT membership.job_id,membership.output_ordinal,membership.source_ordinal,
               membership.source_material_id,membership.intent_id AS output_intent_id,
               membership.source_intent_id,membership.blocker_id,
               material.state AS material_state,source_intent.state AS source_intent_state,
               blocker.state AS blocker_state
          FROM embedding_delivery_source_memberships membership
          JOIN content_materials material
            ON material.workspace_id=membership.workspace_id AND material.id=membership.source_material_id
          -- 0193's membership intent is the exact output intent. The source
          -- intent is independently persisted and bound to the source material.
          JOIN material_key_creation_intents source_intent
            ON source_intent.workspace_id=material.workspace_id
           AND source_intent.id=membership.source_intent_id
           AND source_intent.id=material.intent_id
          JOIN material_erasure_blockers blocker
            ON blocker.workspace_id=membership.workspace_id AND blocker.id=membership.blocker_id
         WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
         ORDER BY membership.output_ordinal,membership.source_ordinal,
                  membership.source_material_id,membership.intent_id,membership.blocker_id
         FOR UPDATE OF membership,material,source_intent,blocker
    LOOP
        source_count:=source_count+1;
        IF source.material_state<>'live' OR source.source_intent_state<>'live'
           OR source.blocker_state<>'nonterminal' THEN
            RAISE EXCEPTION 'embedding result source membership is no longer live and blocked'
                USING ERRCODE='23514';
        END IF;
    END LOOP;
    -- The source chain is now held.  Re-lock the initially read job and its
    -- effect only after the connection/auth/space/policy/source prefix, then
    -- reject any identity change before touching output material.
    SELECT * INTO job FROM embedding_jobs WHERE workspace_id=target_workspace AND id=target_job FOR UPDATE;
    SELECT * INTO effect FROM external_effect_intents
     WHERE workspace_id=target_workspace AND id=target_effect FOR UPDATE;
    IF job.id IS NULL OR effect.id IS NULL OR job.kind <> 'delivery' OR job.state <> 'running'
       OR job.external_effect_id <> target_effect OR job.version <> target_expected_version
       OR job.space_registration_id <> initial_job.space_registration_id
       OR job.model_binding_snapshot_id <> initial_job.model_binding_snapshot_id
       OR job.model_request_evidence_id <> initial_job.model_request_evidence_id THEN
        RAISE EXCEPTION 'embedding result requires its active expected-version delivery job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_job_result_preparations
     WHERE workspace_id=target_workspace AND job_id=target_job FOR KEY SHARE;
    IF FOUND THEN
        IF existing.external_effect_id=target_effect AND existing.expected_job_version=target_expected_version
           AND existing.response_model=target_response_model AND existing.output_count=cardinality(target_attachment_ids)
           AND (SELECT count(*) FROM embedding_job_result_prepared_attachments attachment
                 WHERE attachment.workspace_id=target_workspace AND attachment.preparation_id=existing.id)
               = cardinality(target_attachment_ids)
           AND (SELECT count(*) FROM embedding_projection_entries projection
                 WHERE projection.workspace_id=target_workspace AND projection.preparation_id=existing.id)
               = cardinality(target_attachment_ids)
           AND NOT EXISTS (
               SELECT 1 FROM embedding_projection_entries projection
                WHERE projection.workspace_id=target_workspace AND projection.preparation_id=existing.id
                  AND (projection.output_ordinal<0
                    OR projection.response_index<>projection.output_ordinal
                    OR projection.output_ordinal>=cardinality(target_dimensions)
                    OR projection.response_model<>target_response_model
                    OR projection.dimensions<>target_dimensions[projection.output_ordinal+1])
           )
           AND EXISTS (
               SELECT 1 FROM external_effect_receipts receipt
                WHERE receipt.workspace_id=target_workspace AND receipt.id=existing.receipt_id
                  AND receipt.effect_id=target_effect AND receipt.outcome_status='acknowledged'
                  AND receipt.payload @> jsonb_build_object(
                      'evidence_refs',jsonb_build_array(
                          format('embedding_job_result_preparation:%s',existing.id)
                      )
                  )
           )
        THEN RETURN existing.id; END IF;
        RAISE EXCEPTION 'embedding result preparation conflicts with its marker semantic tuple' USING ERRCODE='23514';
    END IF;
    -- Lock and validate the complete exact output set before authoring any
    -- receipt, marker, attachment, projection or ciphertext.  The later write
    -- loop only re-locks these already-held rows.
    position := 0;
    FOR member IN SELECT membership.output_ordinal,membership.intent_id,intent.material_id,intent.material_key_id,intent.state,intent.vault_receipt,
                         receipt.intent_id AS output_key_receipt_intent
                    FROM embedding_job_material_intents membership
                    JOIN material_key_creation_intents intent ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
                    JOIN embedding_output_key_receipts receipt ON receipt.workspace_id=membership.workspace_id AND receipt.intent_id=membership.intent_id
                    LEFT JOIN embedding_output_key_retirement_requests retired ON retired.workspace_id=membership.workspace_id AND retired.intent_id=membership.intent_id
                   WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
                   ORDER BY membership.output_ordinal,intent.id FOR UPDATE OF membership,intent,receipt
    LOOP
        IF member.output_ordinal <> position OR member.state <> 'provisional_receipted' OR member.vault_receipt IS NULL
           OR member.output_key_receipt_intent IS NULL
           OR EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests retired WHERE retired.workspace_id=target_workspace AND retired.intent_id=member.intent_id)
           OR target_attachment_ids[position+1] IS NULL
           OR target_dimensions[position+1] <> registration.dimensions
           OR target_ciphertexts[position+1] IS NULL THEN
            RAISE EXCEPTION 'embedding result output set is incomplete or not receipted' USING ERRCODE='23514';
        END IF;
        position := position+1;
    END LOOP;
    IF source_count=0 OR position <> cardinality(target_attachment_ids) OR EXISTS (
        SELECT 1 FROM embedding_job_material_intents output
         WHERE output.workspace_id=target_workspace AND output.job_id=target_job
           AND NOT EXISTS (
               SELECT 1 FROM embedding_delivery_source_memberships membership
                WHERE membership.workspace_id=output.workspace_id AND membership.job_id=output.job_id
                  AND membership.output_ordinal=output.output_ordinal
           )
    ) THEN
        RAISE EXCEPTION 'embedding result requires every complete exact source membership'
            USING ERRCODE='23514';
    END IF;
    -- The guarded command creates the receipt and its lifecycle transition;
    -- no runtime caller can leave an embedding-result receipt without the
    -- marker transaction that owns it.
    receipt_recorded_at:=clock_timestamp();
    INSERT INTO external_effect_receipts(id,effect_id,workspace_id,outcome_status,payload,created_at)
    VALUES(
        target_receipt,target_effect,target_workspace,'acknowledged',
        jsonb_build_object(
            'id',target_receipt,'effect_id',target_effect,'adapter',effect.adapter,
            'dispatched_at',receipt_recorded_at,'acknowledgement_at',receipt_recorded_at,
            'response_class','http_200','external_resource_id',NULL,
            'external_version',NULL,'response_digest',NULL,'outcome_status','acknowledged',
            'evidence_refs',jsonb_build_array(format('embedding_job_result_preparation:%s',target_preparation)),
            'recorded_at',receipt_recorded_at
        ),receipt_recorded_at
    );
    INSERT INTO external_effect_lifecycle_transitions(
        effect_id,workspace_id,status,cause,cause_ref,recorded_at
    ) VALUES(
        target_effect,target_workspace,'acknowledged','receipt_recorded',target_receipt::TEXT,receipt_recorded_at
    );
    -- Insert the marker before children. Its completeness trigger is deferred,
    -- while the children use ordinary foreign keys to the marker. Nothing is
    -- visible until this transaction also records every child and receipt.
    INSERT INTO embedding_job_result_preparations(
        id,workspace_id,job_id,external_effect_id,space_registration_id,model_binding_snapshot_id,model_request_evidence_id,
        expected_job_version,adapter,auth_branch,credential_revision_id,credential_intent_id,credential_erasure_blocker_id,
        response_model,output_count,data_policy_decision_id,receipt_id
    ) VALUES(
        target_preparation,target_workspace,job.id,job.external_effect_id,job.space_registration_id,
        job.model_binding_snapshot_id,job.model_request_evidence_id,job.version,effect.adapter,
        snapshot.branch,snapshot.credential_revision_id,credential_intent,credential_blocker,
        target_response_model,cardinality(target_attachment_ids),decision.id,target_receipt
    );
    position := 0;
    FOR member IN SELECT membership.output_ordinal,membership.intent_id,intent.material_id,intent.material_key_id,intent.state,intent.vault_receipt,
                         receipt.intent_id AS output_key_receipt_intent
                    FROM embedding_job_material_intents membership
                    JOIN material_key_creation_intents intent ON intent.workspace_id=membership.workspace_id AND intent.id=membership.intent_id
                    JOIN embedding_output_key_receipts receipt ON receipt.workspace_id=membership.workspace_id AND receipt.intent_id=membership.intent_id
                    LEFT JOIN embedding_output_key_retirement_requests retired ON retired.workspace_id=membership.workspace_id AND retired.intent_id=membership.intent_id
                   WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
                   ORDER BY membership.output_ordinal,intent.id FOR UPDATE OF membership,intent,receipt
    LOOP
        IF member.output_ordinal <> position OR member.state <> 'provisional_receipted' OR member.vault_receipt IS NULL
           OR member.output_key_receipt_intent IS NULL
           OR EXISTS(SELECT 1 FROM embedding_output_key_retirement_requests retired WHERE retired.workspace_id=target_workspace AND retired.intent_id=member.intent_id)
           OR target_dimensions[position+1] <> registration.dimensions
           OR target_ciphertexts[position+1] IS NULL THEN
            RAISE EXCEPTION 'embedding result output set is incomplete or not receipted' USING ERRCODE='23514';
        END IF;
        PERFORM vestrace_prepare_result_material(member.intent_id,target_attachment_ids[position+1],target_ciphertexts[position+1],octet_length(target_ciphertexts[position+1]));
        UPDATE embedding_space_corpus_states SET next_projection_ordinal=next_projection_ordinal+1
         WHERE workspace_id=target_workspace AND space_registration_id=job.space_registration_id
         RETURNING next_projection_ordinal-1 INTO projection_ordinal;
        projection_id := gen_random_uuid();
        INSERT INTO embedding_projection_entries(
            id,workspace_id,preparation_id,job_id,input_ordinal,output_ordinal,space_registration_id,projection_ordinal,
            material_id,intent_id,material_key_id,response_index,response_model,dimensions,model_binding_snapshot_id,
            data_policy_decision_id,sensitivity,classification_labels,has_unclassified,retention_eligibility_state,state
        ) VALUES (
            projection_id,target_workspace,target_preparation,target_job,member.output_ordinal,member.output_ordinal,job.space_registration_id,projection_ordinal,
            member.material_id,member.intent_id,member.material_key_id,member.output_ordinal,target_response_model,registration.dimensions,job.model_binding_snapshot_id,
            decision.id,decision.classification,decision.classification_labels,(decision.unclassified_count>0),'blocked_result_finalizing','result_finalizing'
        );
        INSERT INTO embedding_job_result_prepared_attachments(workspace_id,preparation_id,output_ordinal,intent_id,prepared_attachment_id,projection_id)
        VALUES(target_workspace,target_preparation,member.output_ordinal,member.intent_id,target_attachment_ids[position+1],projection_id);
        INSERT INTO embedding_projection_source_dependencies(
            workspace_id,projection_id,job_id,output_ordinal,source_ordinal,
            source_material_id,output_intent_id,source_intent_id,erasure_blocker_id
        )
        SELECT membership.workspace_id,projection_id,membership.job_id,membership.output_ordinal,membership.source_ordinal,
               membership.source_material_id,membership.intent_id,membership.source_intent_id,membership.blocker_id
          FROM embedding_delivery_source_memberships membership
         WHERE membership.workspace_id=target_workspace AND membership.job_id=target_job
           AND membership.output_ordinal=member.output_ordinal
         ORDER BY membership.source_ordinal;
        position := position+1;
    END LOOP;
    IF position <> cardinality(target_attachment_ids) THEN
        RAISE EXCEPTION 'embedding result output count does not match accepted outputs' USING ERRCODE='23514';
    END IF;
    PERFORM vestrace_release_provider_dispatch(target_workspace,target_effect,target_receipt);
    RETURN target_preparation;
END $$;

-- ResultPrepared is an immutable terminal fence in addition to 0193's full
-- pre-dispatch authority checker; it does not replace or weaken that checker.
CREATE OR REPLACE FUNCTION vestrace_reject_result_prepared_pre_dispatch_terminalization()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public,pg_temp AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM embedding_job_result_preparations prepared WHERE prepared.workspace_id=NEW.workspace_id AND prepared.job_id=NEW.job_id) THEN
        RAISE EXCEPTION 'result-prepared embedding outputs cannot be retired or terminalized pre-dispatch'
            USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER embedding_result_prepared_pre_dispatch_terminal_fence
    BEFORE INSERT ON embedding_job_pre_dispatch_retirement_authorities
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_result_prepared_pre_dispatch_terminalization();
CREATE TRIGGER embedding_result_prepared_output_retirement_fence
    BEFORE INSERT ON embedding_output_key_retirement_requests
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_result_prepared_pre_dispatch_terminalization();

REVOKE ALL ON FUNCTION vestrace_validate_embedding_result_preparation() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_validate_embedding_projection_dependency() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_create_embedding_result_space_guards() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_lock_embedding_result_completion_authority(UUID,UUID,UUID,UUID,UUID,UUID,UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_load_embedding_result_eligibility(UUID,UUID,UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_commit_embedding_result_preparation(UUID,UUID,UUID,UUID,UUID,BIGINT,TEXT,UUID[],BYTEA[],INTEGER[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_reject_result_prepared_pre_dispatch_terminalization() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_assign_embedding_delivery_source_intent() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_lock_embedding_result_completion_authority(UUID,UUID,UUID,UUID,UUID,UUID,UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_load_embedding_result_eligibility(UUID,UUID,UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_commit_embedding_result_preparation(UUID,UUID,UUID,UUID,UUID,BIGINT,TEXT,UUID[],BYTEA[],INTEGER[]) TO vestrace;
-- The guarded result command locks an immutable 0164 decision row.  A row
-- lock requires UPDATE even though this authority never changes the decision;
-- the append-only triggers remain the final mutation fence, and runtime gets
-- no new table privilege from this grant.
GRANT SELECT, UPDATE ON embedding_data_policy_decisions TO vestrace_guarded_owner;
-- Receipt and lifecycle evidence are authored only by the guarded atomic
-- command. These column grants cover exactly its two INSERT statements; the
-- runtime role receives no direct receipt or lifecycle write authority.
GRANT INSERT (id,effect_id,workspace_id,outcome_status,payload,created_at)
    ON external_effect_receipts TO vestrace_guarded_owner;
GRANT INSERT (effect_id,workspace_id,status,cause,cause_ref,recorded_at)
    ON external_effect_lifecycle_transitions TO vestrace_guarded_owner;
GRANT USAGE ON SEQUENCE external_effect_lifecycle_transitions_ordinal_seq
    TO vestrace_guarded_owner;
GRANT SELECT ON embedding_space_corpus_states,embedding_index_generation_guards,embedding_job_result_preparations,
                embedding_projection_entries,embedding_job_result_prepared_attachments,embedding_projection_source_dependencies TO vestrace;

DO $$
DECLARE target REGCLASS; target_function REGPROCEDURE;
BEGIN
    PERFORM public.vestrace_finish_p04_result_preparation_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN
        RAISE EXCEPTION 'P04 result-preparation ownership hand-back is unavailable' USING ERRCODE='42501';
    END IF;
    -- SQLx's privileged migration harness has no deployment provisioner. Keep
    -- its closed fallback equivalent to the provisioner's finish hand-back:
    -- SECURITY DEFINER result authorities must own every relation they read.
    FOREACH target IN ARRAY ARRAY[
        'embedding_space_corpus_states'::REGCLASS,
        'embedding_index_generation_guards'::REGCLASS,
        'embedding_job_result_preparations'::REGCLASS,
        'embedding_projection_entries'::REGCLASS,
        'embedding_job_result_prepared_attachments'::REGCLASS,
        'embedding_projection_source_dependencies'::REGCLASS,
        'embedding_space_registrations'::REGCLASS,
        'embedding_delivery_source_memberships'::REGCLASS
    ] LOOP
        EXECUTE format('ALTER TABLE %s OWNER TO vestrace_guarded_owner', target);
    END LOOP;
    FOREACH target_function IN ARRAY ARRAY[
        'vestrace_validate_embedding_result_preparation()'::REGPROCEDURE,
        'vestrace_validate_embedding_projection_dependency()'::REGPROCEDURE,
        'vestrace_create_embedding_result_space_guards()'::REGPROCEDURE,
        'vestrace_lock_embedding_result_completion_authority(uuid,uuid,uuid,uuid,uuid,uuid,uuid)'::REGPROCEDURE,
        'vestrace_load_embedding_result_eligibility(uuid,uuid,uuid)'::REGPROCEDURE,
        'vestrace_commit_embedding_result_preparation(uuid,uuid,uuid,uuid,uuid,bigint,text,uuid[],bytea[],integer[])'::REGPROCEDURE,
        'vestrace_reject_result_prepared_pre_dispatch_terminalization()'::REGPROCEDURE,
        'vestrace_assign_embedding_delivery_source_intent()'::REGPROCEDURE,
        'vestrace_lock_embedding_job_recovery_authority(uuid,uuid)'::REGPROCEDURE,
        'vestrace_lock_embedding_job_pre_dispatch_gate(uuid,uuid,boolean)'::REGPROCEDURE,
        'vestrace_fence_embedding_job_dispatching()'::REGPROCEDURE
    ] LOOP
        EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function);
    END LOOP;
END $$;
