-- P04 Task 14B: durable, pre-dispatch termination for governed embedding jobs.
--
-- This migration deliberately does not add an embedding output executor.  It
-- records the only lawful future owner tuple and fences every dispatch while
-- such a tuple exists, until a later contract supplies its vault protocol.

DO $$
BEGIN
    PERFORM vestrace_prepare_p04_termination_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE EXCEPTION 'P04 termination ownership hand-back must be provisioned before runtime migration'
            USING ERRCODE = '42501';
    END IF;
END
$$;

CREATE TABLE embedding_job_material_intents (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    job_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal >= 0),
    intent_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, job_id, output_ordinal),
    CONSTRAINT embedding_job_material_intents_identity_key UNIQUE (intent_id),
    CONSTRAINT embedding_job_material_intents_job_fkey
        FOREIGN KEY (workspace_id, job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_job_material_intents_intent_fkey
        FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id) ON DELETE RESTRICT
);

CREATE TABLE embedding_job_termination_receipts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    job_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    expected_version BIGINT NOT NULL CHECK (expected_version > 0),
    terminal_version BIGINT NOT NULL CHECK (terminal_version > expected_version),
    terminal_state TEXT NOT NULL CHECK (terminal_state IN ('cancelled', 'failed_definite')),
    evidence_kind TEXT NOT NULL CHECK (evidence_kind IN (
        'cancellation_authorization', 'external_effect_denied', 'admission_timeout'
    )),
    evidence_id UUID NOT NULL,
    authorization_policy_version TEXT,
    authorization_capability TEXT,
    authorization_operation TEXT,
    authorization_resource_scope TEXT,
    authorization_risk TEXT,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_job_termination_receipts_job_key UNIQUE (workspace_id, job_id),
    CONSTRAINT embedding_job_termination_receipts_idempotency_key
        UNIQUE (workspace_id, idempotency_key),
    CONSTRAINT embedding_job_termination_receipts_workspace_id_key UNIQUE (id, workspace_id),
    CONSTRAINT embedding_job_termination_receipts_effect_identity_key
        UNIQUE (id, workspace_id, external_effect_id),
    CONSTRAINT embedding_job_termination_receipts_job_fkey
        FOREIGN KEY (workspace_id, job_id)
        REFERENCES embedding_jobs(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_job_termination_receipts_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT embedding_job_termination_receipts_cancellation_tuple CHECK ((
        (evidence_kind = 'cancellation_authorization'
         AND authorization_policy_version IS NOT NULL AND btrim(authorization_policy_version) <> ''
         AND authorization_capability IS NOT DISTINCT FROM 'execution.write'
         AND authorization_operation IS NOT DISTINCT FROM 'embedding.job.cancel'
         AND authorization_resource_scope IS NOT DISTINCT FROM 'workspace://'
         AND authorization_risk IS NOT DISTINCT FROM 'low')
        OR
        (evidence_kind <> 'cancellation_authorization'
         AND authorization_policy_version IS NULL AND authorization_capability IS NULL
         AND authorization_operation IS NULL AND authorization_resource_scope IS NULL
         AND authorization_risk IS NULL)
    ) IS TRUE)
);

ALTER TABLE provider_concurrency_leases
    ADD COLUMN IF NOT EXISTS released_termination_id UUID;
ALTER TABLE provider_concurrency_leases
    DROP CONSTRAINT provider_concurrency_leases_release_receipt_pair;
ALTER TABLE provider_concurrency_leases
    ADD CONSTRAINT provider_concurrency_leases_release_evidence_pair CHECK (
        (released_at IS NULL AND released_receipt_id IS NULL AND released_termination_id IS NULL)
        OR (released_at IS NOT NULL AND (
            (released_receipt_id IS NOT NULL AND released_termination_id IS NULL)
            OR (released_receipt_id IS NULL AND released_termination_id IS NOT NULL)
            OR (released_receipt_id IS NULL AND released_termination_id IS NULL AND expires_at <= released_at)
        ))
    );
ALTER TABLE provider_concurrency_leases
    ADD CONSTRAINT provider_concurrency_leases_termination_fkey
    FOREIGN KEY (released_termination_id, workspace_id, external_effect_id)
    REFERENCES embedding_job_termination_receipts(id, workspace_id, external_effect_id) ON DELETE RESTRICT;

ALTER TABLE embedding_job_material_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_job_material_intents FORCE ROW LEVEL SECURITY;
ALTER TABLE embedding_job_termination_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_job_termination_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_job_material_intents_workspace_policy ON embedding_job_material_intents
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY embedding_job_termination_receipts_workspace_policy ON embedding_job_termination_receipts
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON embedding_job_material_intents FROM PUBLIC;
REVOKE ALL ON embedding_job_termination_receipts FROM PUBLIC;
CREATE TRIGGER embedding_job_material_intents_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_job_material_intents
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER embedding_job_termination_receipts_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON embedding_job_termination_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

-- The material table's generic owner columns are not authority.  This deferred
-- validator makes the embedding owner kind lawful only when its immutable,
-- typed membership proves the exact job and output ordinal in the same commit.
CREATE OR REPLACE FUNCTION vestrace_validate_embedding_job_output_membership()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE target_intent_id UUID; intent_row material_key_creation_intents%ROWTYPE;
BEGIN
    IF TG_TABLE_NAME = 'embedding_job_material_intents' THEN
        IF TG_OP = 'DELETE' THEN
            target_intent_id := OLD.intent_id;
        ELSE
            target_intent_id := NEW.intent_id;
        END IF;
    ELSIF TG_OP = 'DELETE' THEN
        target_intent_id := OLD.id;
    ELSE
        target_intent_id := NEW.id;
    END IF;
    SELECT * INTO intent_row FROM material_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN RETURN NULL; END IF;
    IF TG_TABLE_NAME <> 'embedding_job_material_intents'
       AND intent_row.owner_kind <> 'embedding_job_output' THEN RETURN NULL; END IF;
    IF NOT EXISTS (
        SELECT 1 FROM embedding_job_material_intents AS member
         WHERE member.workspace_id = intent_row.workspace_id
           AND member.intent_id = intent_row.id
           AND intent_row.owner_kind = 'embedding_job_output'
           AND member.job_id = intent_row.owner_id
           AND member.output_ordinal = intent_row.output_ordinal
    ) THEN
        RAISE EXCEPTION 'embedding output material intent requires its exact job membership'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END $$;

CREATE CONSTRAINT TRIGGER embedding_job_material_intents_deferred_membership
    AFTER INSERT OR UPDATE OR DELETE ON embedding_job_material_intents
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_embedding_job_output_membership();
CREATE CONSTRAINT TRIGGER material_intents_embedding_owner_deferred_membership
    AFTER INSERT OR UPDATE OR DELETE ON material_key_creation_intents
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION vestrace_validate_embedding_job_output_membership();

-- The shared routing, admission and lifecycle paths take this exact lock chain
-- before treating an embedding effect as dispatchable.  The output protocol is
-- deliberately not implemented by this task, so any enrolled output is a
-- durable refusal instead of a partial provider call.
CREATE OR REPLACE FUNCTION vestrace_lock_embedding_job_pre_dispatch_gate(
    target_workspace_id UUID,
    target_effect_id UUID,
    allow_terminal BOOLEAN
) RETURNS TEXT LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE;
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
        IF EXISTS(SELECT 1 FROM embedding_job_material_intents WHERE workspace_id=target_workspace_id AND job_id=job_row.id) THEN
            RAISE EXCEPTION 'embedding output execution is not configured' USING ERRCODE='23514';
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
           SELECT 1 FROM embedding_job_material_intents AS member
           JOIN material_key_creation_intents AS intent ON intent.id=member.intent_id AND intent.workspace_id=member.workspace_id
           WHERE member.workspace_id=target_workspace_id AND member.job_id=job_row.id
             AND (intent.owner_kind <> 'embedding_job_output' OR intent.owner_id <> job_row.id
                  OR intent.output_ordinal <> member.output_ordinal OR intent.state <> 'abandoned'
                  OR NOT EXISTS(SELECT 1 FROM material_key_creation_intent_erasure_receipts WHERE intent_id=intent.id))
       ) THEN
        RETURN job_row.state;
    END IF;
    RAISE EXCEPTION 'embedding job is terminal before provider dispatch' USING ERRCODE='23514';
END $$;

-- Future output producers reserve both identities under the job's canonical
-- lock chain.  The generic intent reserve remains the sole material allocator;
-- this function adds typed enrolment, and refuses terminal or dispatched jobs.
CREATE OR REPLACE FUNCTION vestrace_reserve_embedding_job_output_intent(
    target_intent_id UUID, target_workspace_id UUID, target_job_id UUID,
    target_output_ordinal BIGINT, target_material_id UUID, target_material_key_id UUID,
    target_nonce UUID
) RETURNS UUID LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE; existing embedding_job_material_intents%ROWTYPE;
        reserved material_key_creation_intents%ROWTYPE;
BEGIN
    IF target_intent_id IS NULL OR target_workspace_id IS NULL OR target_job_id IS NULL
       OR target_output_ordinal IS NULL OR target_output_ordinal < 0
       OR target_material_id IS NULL OR target_material_key_id IS NULL OR target_nonce IS NULL THEN
        RAISE EXCEPTION 'embedding output intent reservation arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
    SELECT job.model_binding_snapshot_id, job.space_registration_id, snapshot.connection_id, snapshot.branch,
           snapshot.credential_slot_id, snapshot.credential_activation_guard_id
      INTO initial FROM embedding_jobs AS job JOIN model_binding_snapshots AS snapshot
        ON snapshot.workspace_id=job.workspace_id AND snapshot.id=job.model_binding_snapshot_id
     WHERE job.workspace_id=target_workspace_id AND job.id=target_job_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding output intent requires its exact job snapshot' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM connection_execution_guards WHERE workspace_id=target_workspace_id
      AND connection_id=initial.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding output intent requires its connection execution guard' USING ERRCODE='23514'; END IF;
    IF initial.branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards WHERE workspace_id=target_workspace_id
          AND connection_id=initial.connection_id AND credential_slot_id=initial.credential_slot_id
          AND id=initial.credential_activation_guard_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding output intent requires its exact credential guard' USING ERRCODE='23514'; END IF;
    ELSIF initial.branch <> 'no_auth' THEN
        RAISE EXCEPTION 'embedding output intent has an unknown auth branch' USING ERRCODE='23514';
    ELSIF initial.credential_slot_id IS NOT NULL OR initial.credential_activation_guard_id IS NOT NULL THEN
        RAISE EXCEPTION 'embedding output intent no-auth branch is not exact' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_registrations WHERE workspace_id=target_workspace_id
      AND id=initial.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding output intent requires its exact embedding space guard' USING ERRCODE='23514'; END IF;
    SELECT * INTO job_row FROM embedding_jobs WHERE workspace_id=target_workspace_id AND id=target_job_id FOR UPDATE;
    IF NOT FOUND OR job_row.state NOT IN ('requested','running')
       OR EXISTS(SELECT 1 FROM external_effect_lifecycle_transitions WHERE workspace_id=target_workspace_id AND effect_id=job_row.external_effect_id AND status='dispatching') THEN
        RAISE EXCEPTION 'embedding output intent requires a nonterminal pre-dispatch job' USING ERRCODE='23514';
    END IF;
    SELECT * INTO existing FROM embedding_job_material_intents WHERE workspace_id=target_workspace_id
      AND job_id=target_job_id AND output_ordinal=target_output_ordinal FOR UPDATE;
    IF FOUND THEN
        IF existing.intent_id=target_intent_id THEN
            SELECT * INTO reserved FROM material_key_creation_intents
             WHERE id=existing.intent_id AND workspace_id=target_workspace_id FOR UPDATE;
            IF NOT FOUND
               OR reserved.id IS DISTINCT FROM target_intent_id
               OR reserved.workspace_id IS DISTINCT FROM target_workspace_id
               OR reserved.material_id IS DISTINCT FROM target_material_id
               OR reserved.material_key_id IS DISTINCT FROM target_material_key_id
               OR reserved.nonce IS DISTINCT FROM target_nonce
               OR reserved.owner_kind IS DISTINCT FROM 'embedding_job_output'
               OR reserved.owner_id IS DISTINCT FROM target_job_id
               OR reserved.output_ordinal IS DISTINCT FROM target_output_ordinal THEN
                RAISE EXCEPTION 'embedding output material intent replay tuple mismatches its reservation'
                    USING ERRCODE='23514';
            END IF;
            RETURN existing.intent_id;
        END IF;
        RAISE EXCEPTION 'embedding output ordinal already names a different material intent' USING ERRCODE='23514';
    END IF;
    PERFORM vestrace_reserve_material_key_creation_intent(target_intent_id,target_workspace_id,
        target_material_id,target_material_key_id,target_nonce,'embedding_job_output',target_job_id,target_output_ordinal);
    INSERT INTO embedding_job_material_intents(workspace_id,job_id,output_ordinal,intent_id)
        VALUES(target_workspace_id,target_job_id,target_output_ordinal,target_intent_id);
    RETURN target_intent_id;
END $$;

-- One receipt is both the canonical command identity and the immutable terminal
-- fact.  It converges before touching a job and never delegates idempotency to
-- PgGovernedMutationRepository, whose same-hash path intentionally reapplies.
CREATE OR REPLACE FUNCTION vestrace_terminate_embedding_job_pre_dispatch(
    target_receipt_id UUID, target_workspace_id UUID, target_principal_id UUID,
    target_job_id UUID, target_expected_version BIGINT, target_idempotency_key TEXT,
    target_terminal_state TEXT, target_evidence_kind TEXT, target_evidence_id UUID,
    target_authorization_policy_version TEXT, target_authorization_capability TEXT,
    target_authorization_operation TEXT, target_authorization_resource_scope TEXT,
    target_authorization_risk TEXT
) RETURNS TABLE(receipt_id UUID, job_id UUID, version BIGINT, terminal_state TEXT,
                evidence_kind TEXT, evidence_id UUID, created BOOLEAN)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE; prior embedding_job_termination_receipts%ROWTYPE;
        member RECORD; target_now TIMESTAMPTZ;
BEGIN
    IF target_receipt_id IS NULL OR target_workspace_id IS NULL OR target_principal_id IS NULL
       OR target_job_id IS NULL OR target_expected_version IS NULL OR target_expected_version < 1
       OR target_idempotency_key IS NULL OR btrim(target_idempotency_key)=''
       OR target_terminal_state NOT IN ('cancelled','failed_definite')
       OR target_evidence_kind NOT IN ('cancellation_authorization','external_effect_denied','admission_timeout')
       OR target_evidence_id IS NULL THEN
        RAISE EXCEPTION 'embedding pre-dispatch termination arguments are malformed' USING ERRCODE='22023';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
    IF NULLIF(current_setting('vestrace.principal_id',true),'') IS DISTINCT FROM target_principal_id::TEXT THEN
        RAISE EXCEPTION 'embedding pre-dispatch termination principal does not match the scoped request' USING ERRCODE='42501';
    END IF;
    SELECT job.model_binding_snapshot_id, job.space_registration_id, snapshot.connection_id, snapshot.branch,
           snapshot.credential_slot_id, snapshot.credential_activation_guard_id
      INTO initial FROM embedding_jobs AS job JOIN model_binding_snapshots AS snapshot
        ON snapshot.workspace_id=job.workspace_id AND snapshot.id=job.model_binding_snapshot_id
     WHERE job.workspace_id=target_workspace_id AND job.id=target_job_id;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch termination requires its exact job snapshot' USING ERRCODE='23514'; END IF;
    PERFORM 1 FROM connection_execution_guards WHERE workspace_id=target_workspace_id AND connection_id=initial.connection_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch termination requires its connection execution guard' USING ERRCODE='23514'; END IF;
    IF initial.branch='credential' THEN
        PERFORM 1 FROM credential_activation_guards WHERE workspace_id=target_workspace_id AND connection_id=initial.connection_id
          AND credential_slot_id=initial.credential_slot_id AND id=initial.credential_activation_guard_id FOR UPDATE;
        IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch termination requires its exact credential guard' USING ERRCODE='23514'; END IF;
    ELSIF initial.branch <> 'no_auth' THEN RAISE EXCEPTION 'embedding pre-dispatch termination has an unknown auth branch' USING ERRCODE='23514'; END IF;
    IF initial.branch='no_auth' AND (initial.credential_slot_id IS NOT NULL OR initial.credential_activation_guard_id IS NOT NULL) THEN
        RAISE EXCEPTION 'embedding pre-dispatch termination no-auth branch is not exact' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM embedding_space_registrations WHERE workspace_id=target_workspace_id AND id=initial.space_registration_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'embedding pre-dispatch termination requires its exact embedding space guard' USING ERRCODE='23514'; END IF;
    SELECT * INTO job_row FROM embedding_jobs WHERE workspace_id=target_workspace_id AND id=target_job_id FOR UPDATE;
    IF job_row.id IS NULL THEN RAISE EXCEPTION 'embedding job is absent' USING ERRCODE='23514'; END IF;
    -- `now()` is fixed at transaction start.  This timestamp follows every
    -- canonical lock, so a cancellation begun before a competing admission
    -- cannot try to release that newly-issued lease in its own past.
    target_now := clock_timestamp();
    IF (target_terminal_state='cancelled' AND target_evidence_kind <> 'cancellation_authorization')
       OR (target_terminal_state='failed_definite' AND target_evidence_kind='cancellation_authorization') THEN
        RAISE EXCEPTION 'embedding terminal state does not match its durable evidence kind' USING ERRCODE='23514';
    END IF;
    IF target_evidence_kind='cancellation_authorization' AND (
       target_authorization_policy_version IS NULL OR btrim(target_authorization_policy_version)=''
       OR target_authorization_capability IS DISTINCT FROM 'execution.write'
       OR target_authorization_operation IS DISTINCT FROM 'embedding.job.cancel'
       OR target_authorization_resource_scope IS DISTINCT FROM 'workspace://'
       OR target_authorization_risk IS DISTINCT FROM 'low') THEN
        RAISE EXCEPTION 'embedding cancellation authorization is not bound to its exact principal and request tuple' USING ERRCODE='42501';
    END IF;
    SELECT * INTO prior FROM embedding_job_termination_receipts WHERE workspace_id=target_workspace_id AND idempotency_key=target_idempotency_key FOR UPDATE;
    IF FOUND THEN
        IF prior.principal_id=target_principal_id AND prior.job_id=target_job_id AND prior.expected_version=target_expected_version
           AND prior.terminal_state=target_terminal_state AND prior.evidence_kind=target_evidence_kind
           AND (target_evidence_kind='cancellation_authorization' OR prior.evidence_id=target_evidence_id) THEN
            RETURN QUERY SELECT prior.id,prior.job_id,prior.terminal_version,prior.terminal_state,prior.evidence_kind,prior.evidence_id,FALSE; RETURN;
        END IF;
        RAISE EXCEPTION 'embedding pre-dispatch termination idempotency key conflicts' USING ERRCODE='40001';
    END IF;
    IF job_row.version<>target_expected_version OR job_row.state NOT IN ('requested','running') THEN
        RAISE EXCEPTION 'embedding job version or state conflicts with pre-dispatch termination' USING ERRCODE='40001';
    END IF;
    IF EXISTS(SELECT 1 FROM external_effect_lifecycle_transitions WHERE workspace_id=target_workspace_id
       AND effect_id=job_row.external_effect_id AND status NOT IN ('prepared','authorized'))
       OR EXISTS(SELECT 1 FROM external_effect_receipts WHERE workspace_id=target_workspace_id AND effect_id=job_row.external_effect_id)
       OR EXISTS(SELECT 1 FROM provider_result_preparations WHERE workspace_id=target_workspace_id AND external_effect_id=job_row.external_effect_id) THEN
        RAISE EXCEPTION 'embedding job has historical provider dispatch evidence' USING ERRCODE='23514';
    END IF;
    IF target_evidence_kind='external_effect_denied' AND NOT EXISTS(
       SELECT 1 FROM external_effect_authorizations WHERE id=target_evidence_id AND workspace_id=target_workspace_id
         AND effect_id=job_row.external_effect_id AND result='deny') THEN
        RAISE EXCEPTION 'embedding definite failure requires its exact denied effect authorization' USING ERRCODE='23514';
    END IF;
    IF target_evidence_kind='admission_timeout' AND NOT EXISTS(
       SELECT 1 FROM provider_admission_waits WHERE id=target_evidence_id AND workspace_id=target_workspace_id
         AND external_effect_id=job_row.external_effect_id AND connection_id=initial.connection_id
         AND deadline_at<=target_now AND (terminal_reason IS NULL OR terminal_reason='timeout')) THEN
        RAISE EXCEPTION 'embedding definite failure requires its exact expired admission wait' USING ERRCODE='23514';
    END IF;
    FOR member IN SELECT membership.output_ordinal,membership.intent_id,intent.owner_kind,intent.owner_id,
                          intent.output_ordinal AS intent_output_ordinal,intent.state
       FROM embedding_job_material_intents AS membership JOIN material_key_creation_intents AS intent
         ON intent.id=membership.intent_id AND intent.workspace_id=membership.workspace_id
       WHERE membership.workspace_id=target_workspace_id AND membership.job_id=target_job_id
       ORDER BY membership.output_ordinal,membership.intent_id FOR UPDATE OF membership,intent LOOP
        IF member.owner_kind <> 'embedding_job_output' OR member.owner_id <> target_job_id
           OR member.intent_output_ordinal <> member.output_ordinal
           OR member.state <> 'abandoned' OR NOT EXISTS(SELECT 1 FROM material_key_creation_intent_erasure_receipts WHERE intent_id=member.intent_id) THEN
            RAISE EXCEPTION 'embedding pre-dispatch termination requires every output material intent abandoned with its exact witness' USING ERRCODE='23514';
        END IF;
    END LOOP;
    INSERT INTO embedding_job_termination_receipts(id,workspace_id,job_id,external_effect_id,principal_id,expected_version,terminal_version,terminal_state,evidence_kind,evidence_id,authorization_policy_version,authorization_capability,authorization_operation,authorization_resource_scope,authorization_risk,idempotency_key)
      VALUES(target_receipt_id,target_workspace_id,target_job_id,job_row.external_effect_id,target_principal_id,target_expected_version,target_expected_version+1,target_terminal_state,target_evidence_kind,target_evidence_id,target_authorization_policy_version,target_authorization_capability,target_authorization_operation,target_authorization_resource_scope,target_authorization_risk,target_idempotency_key);
    UPDATE provider_admission_waits SET terminal_reason=CASE
          WHEN id=target_evidence_id AND target_evidence_kind='admission_timeout' THEN 'timeout'
          ELSE 'cancelled' END,
          terminal_at=target_now
      WHERE workspace_id=target_workspace_id AND external_effect_id=job_row.external_effect_id AND terminal_reason IS NULL;
    UPDATE provider_concurrency_leases SET released_at=target_now,released_termination_id=target_receipt_id
      WHERE workspace_id=target_workspace_id AND external_effect_id=job_row.external_effect_id AND released_at IS NULL;
    UPDATE credential_dispatch_leases AS credential SET terminal_state='revoked'
      WHERE credential.workspace_id=target_workspace_id AND credential.external_effect_id=job_row.external_effect_id
        AND credential.terminal_state IS NULL AND credential.consumed_at IS NULL;
    UPDATE connection_admission_states AS admission_state SET active_lease_count=(SELECT COUNT(*) FROM provider_concurrency_leases
      WHERE workspace_id=target_workspace_id AND connection_id=initial.connection_id AND released_at IS NULL AND expires_at>target_now),
      version=admission_state.version+1,updated_at=target_now
      WHERE admission_state.workspace_id=target_workspace_id AND admission_state.connection_id=initial.connection_id;
    UPDATE embedding_jobs AS job SET state=target_terminal_state,version=job.version+1
      WHERE job.workspace_id=target_workspace_id AND job.id=target_job_id;
    RETURN QUERY SELECT target_receipt_id,target_job_id,target_expected_version+1,target_terminal_state,target_evidence_kind,target_evidence_id,TRUE;
END $$;

-- The external effect repository's `record_dispatch_started_on` is an existing
-- direct writer.  Fence its persisted fact, rather than hoping callers first
-- pass through routing or admission.  Non-embedding effects retain their P03
-- semantics unchanged.
CREATE OR REPLACE FUNCTION vestrace_fence_embedding_job_dispatching()
RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS $$
DECLARE initial RECORD; job_row embedding_jobs%ROWTYPE;
        target_workspace_id UUID; target_effect_id UUID;
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
    -- An UPDATE may try to rename an embedding lifecycle row away from its
    -- owning effect. OLD remains authority for that mutation decision.
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
    -- Lifecycle rows for embedding effects become immutable once they exist.
    -- The narrow branch leaves the shared Run/Q1 lifecycle writers untouched.
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
    IF EXISTS(SELECT 1 FROM embedding_job_material_intents WHERE workspace_id=target_workspace_id AND job_id=job_row.id) THEN
      RAISE EXCEPTION 'embedding output execution is not configured' USING ERRCODE='23514';
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
    RETURN NEW;
END $$;

CREATE TRIGGER external_effect_embedding_job_pre_dispatch_fence
    BEFORE INSERT OR UPDATE OR DELETE ON external_effect_lifecycle_transitions
    FOR EACH ROW EXECUTE FUNCTION vestrace_fence_embedding_job_dispatching();

-- Task 14B forward replacements preserve the current shared Run/Q1 bodies byte-for-byte outside the embedding gate calls below.
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
          OR
          (target_cause_kind = 'embedding_job'
           AND target_run_id IS NULL AND target_step_id IS NULL
           AND target_snapshot_id IS NOT NULL
           AND target_qualification_job_id IS NULL
           AND target_qualification_target_id IS NULL)
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

    IF target_cause_kind = 'embedding_job' THEN
        PERFORM vestrace_lock_embedding_job_pre_dispatch_gate(
            target_workspace_id, target_effect_id, FALSE
        );
    END IF;

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
    ELSIF target_cause_kind = 'embedding_job' THEN
        -- The same shape as the Run branch, with the job standing where the run
        -- and step stand. The job's own effect identifies it, its pinned
        -- snapshot must be the one the caller named, and the evidence root must
        -- name the job as its cause. Nothing here can select a Connection the
        -- job did not pin at acceptance.
        SELECT snapshot.branch, snapshot.credential_revision_id,
               snapshot.credential_slot_id,
               snapshot.credential_activation_guard_id,
               snapshot.expected_slot_version,
               snapshot.no_auth_binding_revision_id,
               revision.runtime_base_url, revision.auth_mode,
               revision.credential_slot_id AS revision_credential_slot_id
          INTO routing
          FROM embedding_jobs AS job
          JOIN model_binding_snapshots AS snapshot
            ON snapshot.workspace_id = job.workspace_id
           AND snapshot.id = job.model_binding_snapshot_id
          JOIN connection_revisions AS revision
            ON revision.workspace_id = snapshot.workspace_id
           AND revision.connection_id = snapshot.connection_id
           AND revision.id = snapshot.connection_revision_id
          JOIN model_request_evidence_roots AS evidence
            ON evidence.workspace_id = snapshot.workspace_id
           AND evidence.id = target_evidence_id
           AND evidence.external_effect_id = target_effect_id
           AND evidence.cause_kind = 'embedding_job'
           AND evidence.cause_id = job.id
           AND evidence.binding_snapshot_id = snapshot.id
         WHERE job.workspace_id = target_workspace_id
           AND job.external_effect_id = target_effect_id
           AND job.model_binding_snapshot_id = target_snapshot_id
           AND job.model_request_evidence_id = target_evidence_id
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

CREATE OR REPLACE FUNCTION vestrace_try_admit_provider_dispatch(
    target_admission_id UUID,
    target_wait_id UUID,
    target_concurrency_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_connection_revision_id UUID,
    target_effect_id UUID,
    target_model_request_evidence_id UUID,
    target_cause_kind TEXT,
    target_run_id UUID,
    target_step_id UUID,
    target_snapshot_id UUID,
    target_qualification_job_id UUID,
    target_qualification_target_id UUID,
    target_qualification_probe_ordinal TEXT,
    target_dispatch_ttl_seconds INTEGER
)
RETURNS TABLE(
    decision TEXT,
    retry_after_seconds INTEGER,
    concurrency_lease_id UUID,
    wait_deadline_at TIMESTAMPTZ,
    dispatch_expires_at TIMESTAMPTZ
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_now TIMESTAMPTZ := NOW();
    target_dispatch_expiry TIMESTAMPTZ;
    evidence_root model_request_evidence_roots%ROWTYPE;
    latest_check_id UUID;
    policy_row connection_admission_policy_revisions%ROWTYPE;
    state_row connection_admission_states%ROWTYPE;
    existing_cause provider_dispatch_causes%ROWTYPE;
    existing_admission connection_dispatch_admissions%ROWTYPE;
    existing_wait provider_admission_waits%ROWTYPE;
    existing_lease provider_concurrency_leases%ROWTYPE;
    active_throttle provider_throttle_observations%ROWTYPE;
    target_slot SMALLINT;
    target_retry INTEGER;
    evidence_node_count BIGINT;
    governed_input_count BIGINT;
    governed_framed_bytes BIGINT;
    -- Derived, never supplied. `embedding_jobs.external_effect_id` is unique, so
    -- the job is a consequence of the effect the caller already named rather
    -- than a second thing the caller gets to choose.
    resolved_embedding_job_id UUID := NULL;
BEGIN
    IF target_admission_id IS NULL OR target_wait_id IS NULL
       OR target_concurrency_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_connection_revision_id IS NULL
       OR target_effect_id IS NULL OR target_model_request_evidence_id IS NULL
       OR target_dispatch_ttl_seconds IS NULL
       OR target_dispatch_ttl_seconds NOT BETWEEN 1 AND 900
       OR target_cause_kind IS NULL
       OR target_cause_kind NOT IN ('run_step', 'qualification_probe', 'embedding_job') THEN
        RAISE EXCEPTION 'provider dispatch admission arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF NOT (
        (target_cause_kind = 'run_step'
         AND target_run_id IS NOT NULL
         AND target_step_id IS NOT NULL
         AND target_snapshot_id IS NOT NULL
         AND target_qualification_job_id IS NULL
         AND target_qualification_target_id IS NULL
         AND target_qualification_probe_ordinal IS NULL)
        OR
        (target_cause_kind = 'qualification_probe'
         AND target_run_id IS NULL
         AND target_step_id IS NULL
         AND target_snapshot_id IS NULL
         AND target_qualification_job_id IS NOT NULL
         AND target_qualification_target_id IS NOT NULL
         AND target_qualification_probe_ordinal IN (
             '00','10','15','20','30','35','40','50','60','70','80','90'
         ))
        OR
        (target_cause_kind = 'embedding_job'
         AND target_run_id IS NULL
         AND target_step_id IS NULL
         AND target_snapshot_id IS NOT NULL
         AND target_qualification_job_id IS NULL
         AND target_qualification_target_id IS NULL
         AND target_qualification_probe_ordinal IS NULL)
    ) THEN
        RAISE EXCEPTION 'provider dispatch cause arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_workspace_id IS DISTINCT FROM
       NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'provider dispatch workspace context is required'
            USING ERRCODE = '42501';
    END IF;
    target_dispatch_expiry := target_now + make_interval(secs => target_dispatch_ttl_seconds);

    IF target_cause_kind = 'run_step' AND NOT EXISTS (
        SELECT 1 FROM run_model_binding_snapshots AS run_snapshot
         WHERE run_snapshot.workspace_id = target_workspace_id
           AND run_snapshot.run_id = target_run_id
           AND run_snapshot.snapshot_id = target_snapshot_id
    ) THEN
        RAISE EXCEPTION 'provider dispatch run and snapshot mapping is not exact'
            USING ERRCODE = '23514';
    END IF;

    -- The embedding equivalent of the run/snapshot mapping check above. The job
    -- is found by its own effect, and the snapshot it pinned at acceptance must
    -- be the one the caller named: line 219 says an ordinary job "owns its
    -- snapshot resolved from the current tuple at acceptance and never changed
    -- thereafter", so a dispatch naming a different snapshot is not a late
    -- resolution but a contradiction.
    IF target_cause_kind = 'embedding_job' THEN
        SELECT job.id INTO resolved_embedding_job_id
          FROM embedding_jobs AS job
         WHERE job.workspace_id = target_workspace_id
           AND job.external_effect_id = target_effect_id
           AND job.model_binding_snapshot_id = target_snapshot_id
           AND job.model_request_evidence_id = target_model_request_evidence_id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'provider dispatch embedding job and snapshot mapping is not exact'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    -- Permanent ConnectionExecutionGuard is the outer admission serializer.
    PERFORM guard.id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = target_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM connection_revisions AS revision
         WHERE revision.workspace_id = target_workspace_id
           AND revision.connection_id = target_connection_id
           AND revision.id = target_connection_revision_id
    ) THEN
        RAISE EXCEPTION 'provider dispatch requires its permanent connection guard and revision'
            USING ERRCODE = '23514';
    END IF;

    IF target_cause_kind = 'embedding_job' THEN
        PERFORM vestrace_lock_embedding_job_pre_dispatch_gate(
            target_workspace_id, target_effect_id, FALSE
        );
    END IF;

    -- Revalidate the named evidence while every lower source is held in the
    -- same canonical order used by Task 9 reconstruction.
    SELECT * INTO evidence_root
      FROM model_request_evidence_roots AS root
     WHERE root.id = target_model_request_evidence_id
       AND root.workspace_id = target_workspace_id
     FOR UPDATE OF root;
    IF NOT FOUND OR evidence_root.external_effect_id IS DISTINCT FROM target_effect_id
       OR evidence_root.cause_kind IS DISTINCT FROM target_cause_kind
       OR (
           target_cause_kind = 'run_step'
           AND (
               evidence_root.cause_id IS DISTINCT FROM target_step_id
               OR evidence_root.binding_snapshot_id IS DISTINCT FROM target_snapshot_id
               OR evidence_root.qualification_target_binding_id IS NOT NULL
               OR NOT EXISTS (
                   SELECT 1 FROM run_steps
                    WHERE workspace_id = target_workspace_id
                      AND run_id = target_run_id AND id = target_step_id
               )
               OR NOT EXISTS (
                   SELECT 1 FROM model_request_evidence_nodes
                    WHERE workspace_id = target_workspace_id
                      AND evidence_root_id = target_model_request_evidence_id
                      AND reference_kind = 'binding_snapshot'
                      AND reference_id = target_snapshot_id
               )
           )
       )
       OR (
           target_cause_kind = 'embedding_job'
           AND (
               evidence_root.cause_id IS DISTINCT FROM resolved_embedding_job_id
               OR evidence_root.binding_snapshot_id IS DISTINCT FROM target_snapshot_id
               OR evidence_root.qualification_target_binding_id IS NOT NULL
               OR NOT EXISTS (
                   SELECT 1 FROM model_request_evidence_nodes
                    WHERE workspace_id = target_workspace_id
                      AND evidence_root_id = target_model_request_evidence_id
                      AND reference_kind = 'binding_snapshot'
                      AND reference_id = target_snapshot_id
               )
           )
       )
       OR (
           target_cause_kind = 'qualification_probe'
           AND (
               evidence_root.cause_id IS DISTINCT FROM target_qualification_job_id
               OR evidence_root.binding_snapshot_id IS NOT NULL
               OR evidence_root.qualification_target_binding_id
                    IS DISTINCT FROM target_qualification_target_id
               OR NOT EXISTS (
                   SELECT 1 FROM qualification_target_bindings
                    WHERE workspace_id = target_workspace_id
                      AND qualification_job_id = target_qualification_job_id
                      AND id = target_qualification_target_id
                      AND connection_id = target_connection_id
                      AND connection_revision_id = target_connection_revision_id
               )
               OR NOT EXISTS (
                   SELECT 1 FROM model_request_evidence_nodes
                    WHERE workspace_id = target_workspace_id
                      AND evidence_root_id = target_model_request_evidence_id
                      AND reference_kind = 'qualification_probe'
                      AND reference_id = target_qualification_job_id
                      AND safe_ordinal = target_qualification_probe_ordinal
               )
           )
       )
       OR NOT EXISTS (
           SELECT 1 FROM model_request_evidence_nodes
            WHERE workspace_id = target_workspace_id
              AND evidence_root_id = target_model_request_evidence_id
              AND reference_kind = 'external_effect'
              AND reference_id = target_effect_id
       )
       OR NOT EXISTS (
           SELECT 1 FROM model_request_evidence_nodes
            WHERE workspace_id = target_workspace_id
              AND evidence_root_id = target_model_request_evidence_id
              AND reference_kind = 'connection_revision'
              AND reference_id = target_connection_revision_id
       ) THEN
        RAISE EXCEPTION 'provider dispatch evidence cause tuple is not current and exact'
            USING ERRCODE = '23514';
    END IF;

    PERFORM node.id
      FROM model_request_evidence_nodes AS node
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
     ORDER BY node.ordinal
     FOR UPDATE OF node;
    SELECT COUNT(*),
           COUNT(*) FILTER (WHERE node.reference_kind = 'governed_input_material')
      INTO evidence_node_count, governed_input_count
      FROM model_request_evidence_nodes AS node
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id;
    IF evidence_node_count NOT BETWEEN 1 AND 8200
       OR governed_input_count > 4096 THEN
        RAISE EXCEPTION 'provider dispatch evidence node cardinality exceeds its bound'
            USING ERRCODE = '23514';
    END IF;

    -- Lock every retained source in a fixed table/id order, then apply the
    -- same type-dispatched existence and exact-version test as Task 9.
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN external_effect_intents AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'external_effect'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN connection_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'connection_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN connection_qualification_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'connection_qualification_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'model_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_qualification_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'model_qualification_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_binding_snapshots AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'binding_snapshot'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN qualification_target_bindings AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'qualification_target'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_request_shape_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'request_shape_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_sampling_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'sampling_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_limits_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'limits_revision'
     ORDER BY source.id
     FOR SHARE OF source;
    PERFORM source.id
      FROM model_request_evidence_nodes AS node
      JOIN model_tool_schema_revisions AS source
        ON source.workspace_id = node.workspace_id
       AND source.id = node.reference_id
       AND source.version = node.reference_version
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'tool_schema_revision'
     ORDER BY source.id
     FOR SHARE OF source;

    IF EXISTS (
        SELECT 1
          FROM model_request_evidence_nodes AS node
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_model_request_evidence_id
           AND (
               (node.reference_kind IN (
                   'request_shape_revision', 'sampling_revision',
                   'limits_revision', 'tool_schema_revision'
                ) AND (node.reference_version IS NULL OR node.reference_version < 1))
               OR (node.reference_kind NOT IN (
                   'request_shape_revision', 'sampling_revision',
                   'limits_revision', 'tool_schema_revision'
               ) AND node.reference_version IS NOT NULL)
               OR (node.reference_kind = 'external_effect' AND NOT EXISTS (
                   SELECT 1 FROM external_effect_intents AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'connection_revision' AND NOT EXISTS (
                   SELECT 1 FROM connection_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'connection_qualification_revision' AND NOT EXISTS (
                   SELECT 1 FROM connection_qualification_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'model_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'model_qualification_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_qualification_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'binding_snapshot' AND NOT EXISTS (
                   SELECT 1 FROM model_binding_snapshots AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'qualification_target' AND NOT EXISTS (
                   SELECT 1 FROM qualification_target_bindings AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
               ))
               OR (node.reference_kind = 'request_shape_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_request_shape_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
               OR (node.reference_kind = 'sampling_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_sampling_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
               OR (node.reference_kind = 'limits_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_limits_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
               OR (node.reference_kind = 'tool_schema_revision' AND NOT EXISTS (
                   SELECT 1 FROM model_tool_schema_revisions AS source
                    WHERE source.workspace_id = node.workspace_id
                      AND source.id = node.reference_id
                      AND source.version = node.reference_version
               ))
           )
    ) THEN
        RAISE EXCEPTION 'provider dispatch evidence retained source is absent or wrong-version'
            USING ERRCODE = '23514';
    END IF;
    PERFORM material.id
      FROM model_request_evidence_nodes AS node
      JOIN content_materials AS material
        ON material.workspace_id = node.workspace_id
       AND material.id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY material.id
     FOR UPDATE OF material;
    PERFORM intent.id
      FROM model_request_evidence_nodes AS node
      JOIN content_materials AS material
        ON material.workspace_id = node.workspace_id
       AND material.id = node.reference_id
      JOIN material_key_creation_intents AS intent
        ON intent.id = material.intent_id
       AND intent.workspace_id = material.workspace_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY intent.id
     FOR UPDATE OF intent;
    PERFORM bytes.material_id
      FROM model_request_evidence_nodes AS node
      JOIN content_material_bytes AS bytes
        ON bytes.workspace_id = node.workspace_id
       AND bytes.material_id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material'
     ORDER BY bytes.material_id
     FOR UPDATE OF bytes;
    IF EXISTS (
        SELECT 1
          FROM model_request_evidence_nodes AS node
          LEFT JOIN content_materials AS material
            ON material.workspace_id = node.workspace_id
           AND material.id = node.reference_id
          LEFT JOIN material_key_creation_intents AS intent
            ON intent.id = material.intent_id
           AND intent.workspace_id = material.workspace_id
          LEFT JOIN content_material_bytes AS bytes
            ON bytes.workspace_id = node.workspace_id
           AND bytes.material_id = node.reference_id
         WHERE node.workspace_id = target_workspace_id
           AND node.evidence_root_id = target_model_request_evidence_id
           AND node.reference_kind = 'governed_input_material'
           AND (
               material.id IS NULL OR material.state <> 'live'
               OR intent.id IS NULL OR intent.state <> 'live'
               OR bytes.material_id IS NULL
               OR octet_length(bytes.ciphertext) NOT BETWEEN 4096 AND 1048576
               OR (octet_length(bytes.ciphertext)
                   & (octet_length(bytes.ciphertext) - 1)) <> 0
               OR substring(bytes.ciphertext FROM 1 FOR 4) <> decode('564d5246','hex')
               OR substring(bytes.ciphertext FROM 5 FOR 1) <> decode('01','hex')
               OR EXISTS (
                   SELECT 1 FROM material_erasure_preparations AS erasure
                    WHERE erasure.workspace_id = target_workspace_id
                      AND erasure.content_material_id = node.reference_id
                      AND erasure.finalized_at IS NOT NULL
               )
           )
    ) THEN
        RAISE EXCEPTION 'provider dispatch evidence input is not live and reconstructible'
            USING ERRCODE = '23514';
    END IF;
    SELECT COUNT(*), COALESCE(SUM(octet_length(bytes.ciphertext)::BIGINT), 0)
      INTO governed_input_count, governed_framed_bytes
      FROM model_request_evidence_nodes AS node
      JOIN content_material_bytes AS bytes
        ON bytes.workspace_id = node.workspace_id
       AND bytes.material_id = node.reference_id
     WHERE node.workspace_id = target_workspace_id
       AND node.evidence_root_id = target_model_request_evidence_id
       AND node.reference_kind = 'governed_input_material';
    IF governed_framed_bytes
       > governed_input_count * 4096::BIGINT + 8388608::BIGINT THEN
        RAISE EXCEPTION 'provider dispatch governed input aggregate exceeds derived framed ceiling'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO existing_cause FROM provider_dispatch_causes
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND THEN
        latest_check_id := existing_cause.model_request_evidence_check_id;
        PERFORM 1 FROM model_request_evidence_checks AS check_row
         WHERE check_row.id = latest_check_id
           AND check_row.workspace_id = target_workspace_id
           AND check_row.evidence_root_id = target_model_request_evidence_id
         FOR UPDATE OF check_row;
    ELSE
        SELECT check_row.id INTO latest_check_id
          FROM model_request_evidence_checks AS check_row
         WHERE check_row.workspace_id = target_workspace_id
           AND check_row.evidence_root_id = target_model_request_evidence_id
         ORDER BY check_row.checked_at DESC, check_row.id DESC
         LIMIT 1
         FOR UPDATE OF check_row;
    END IF;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM model_request_evidence_checks
         WHERE id = latest_check_id
           AND workspace_id = target_workspace_id
           AND evidence_root_id = target_model_request_evidence_id
           AND status = 'complete'
    ) THEN
        RAISE EXCEPTION 'provider dispatch requires the latest complete evidence check'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO provider_dispatch_causes (
        external_effect_id, workspace_id, model_request_evidence_id,
        model_request_evidence_check_id, cause_kind, run_id, step_id,
        model_binding_snapshot_id, qualification_job_id,
        qualification_target_binding_id, qualification_probe_ordinal,
        embedding_job_id
    ) VALUES (
        target_effect_id, target_workspace_id, target_model_request_evidence_id,
        latest_check_id, target_cause_kind, target_run_id, target_step_id,
        target_snapshot_id, target_qualification_job_id,
        target_qualification_target_id, target_qualification_probe_ordinal,
        resolved_embedding_job_id
    ) ON CONFLICT (external_effect_id) DO NOTHING;
    SELECT * INTO existing_cause FROM provider_dispatch_causes
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF existing_cause.workspace_id IS DISTINCT FROM target_workspace_id
       OR existing_cause.model_request_evidence_id IS DISTINCT FROM target_model_request_evidence_id
       OR existing_cause.model_request_evidence_check_id IS DISTINCT FROM latest_check_id
       OR existing_cause.cause_kind IS DISTINCT FROM target_cause_kind
       OR existing_cause.run_id IS DISTINCT FROM target_run_id
       OR existing_cause.step_id IS DISTINCT FROM target_step_id
       OR existing_cause.model_binding_snapshot_id IS DISTINCT FROM target_snapshot_id
       OR existing_cause.qualification_job_id IS DISTINCT FROM target_qualification_job_id
       OR existing_cause.qualification_target_binding_id IS DISTINCT FROM target_qualification_target_id
       OR existing_cause.qualification_probe_ordinal IS DISTINCT FROM target_qualification_probe_ordinal
       OR existing_cause.embedding_job_id IS DISTINCT FROM resolved_embedding_job_id THEN
        RAISE EXCEPTION 'provider dispatch cause replay tuple mismatch'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO existing_admission FROM connection_dispatch_admissions
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND THEN
        IF existing_admission.id IS DISTINCT FROM target_admission_id
           OR existing_admission.workspace_id IS DISTINCT FROM target_workspace_id
           OR existing_admission.connection_id IS DISTINCT FROM target_connection_id
           OR existing_admission.connection_revision_id IS DISTINCT FROM target_connection_revision_id
           OR existing_admission.requested_wait_id IS DISTINCT FROM target_wait_id
           OR existing_admission.requested_concurrency_lease_id
                IS DISTINCT FROM target_concurrency_lease_id
           OR existing_admission.requested_dispatch_ttl_seconds
                IS DISTINCT FROM target_dispatch_ttl_seconds
           OR existing_admission.model_binding_snapshot_id IS DISTINCT FROM target_snapshot_id
           OR existing_admission.qualification_target_binding_id
                IS DISTINCT FROM target_qualification_target_id THEN
            RAISE EXCEPTION 'provider dispatch admission replay tuple mismatch'
                USING ERRCODE = '23514';
        END IF;
        IF existing_admission.decision = 'admitted' THEN
            SELECT * INTO existing_lease FROM provider_concurrency_leases
             WHERE id = target_concurrency_lease_id
               AND external_effect_id = target_effect_id;
            IF NOT FOUND OR existing_lease.expires_at
                IS DISTINCT FROM existing_admission.dispatch_expires_at THEN
                RAISE EXCEPTION 'provider dispatch admitted replay lacks its exact lease'
                    USING ERRCODE = '23514';
            END IF;
            decision := 'admitted';
            retry_after_seconds := NULL;
            concurrency_lease_id := existing_lease.id;
            wait_deadline_at := NULL;
            dispatch_expires_at := existing_lease.expires_at;
        ELSIF existing_admission.decision = 'throttled' THEN
            decision := 'throttled';
            retry_after_seconds := existing_admission.retry_after_seconds;
            concurrency_lease_id := NULL;
            SELECT deadline_at INTO wait_deadline_at FROM provider_admission_waits
             WHERE external_effect_id = target_effect_id;
            dispatch_expires_at := NULL;
        ELSE
            decision := existing_admission.decision;
            retry_after_seconds := NULL;
            concurrency_lease_id := NULL;
            SELECT deadline_at INTO wait_deadline_at FROM provider_admission_waits
             WHERE external_effect_id = target_effect_id;
            dispatch_expires_at := NULL;
        END IF;
        RETURN NEXT;
        RETURN;
    END IF;

    SELECT policy.* INTO policy_row
      FROM connection_admission_policy_heads AS head
      JOIN connection_admission_policy_revisions AS policy
        ON policy.workspace_id = head.workspace_id
       AND policy.connection_id = head.connection_id
       AND policy.id = head.current_policy_revision_id
     WHERE head.workspace_id = target_workspace_id
       AND head.connection_id = target_connection_id
     FOR UPDATE OF head, policy;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'provider dispatch admission policy is absent'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO connection_admission_states (
        workspace_id, connection_id, window_started_at,
        admitted_in_window, active_lease_count
    ) VALUES (target_workspace_id, target_connection_id, target_now, 0, 0)
    ON CONFLICT (workspace_id, connection_id) DO NOTHING;
    SELECT * INTO state_row FROM connection_admission_states
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id FOR UPDATE;

    UPDATE provider_concurrency_leases
       SET released_at = target_now,
           released_receipt_id = NULL
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND released_at IS NULL
       AND expires_at <= target_now;
    IF target_now >= state_row.window_started_at + INTERVAL '60 seconds' THEN
        UPDATE connection_admission_states
           SET window_started_at = target_now, admitted_in_window = 0,
               active_lease_count = 0, version = version + 1, updated_at = target_now
         WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id;
        state_row.window_started_at := target_now;
        state_row.admitted_in_window := 0;
    END IF;
    SELECT COUNT(*)::INTEGER INTO state_row.active_lease_count
      FROM provider_concurrency_leases
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND released_at IS NULL AND expires_at > target_now;
    UPDATE connection_admission_states
       SET active_lease_count = state_row.active_lease_count,
           updated_at = target_now
     WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id;

    SELECT * INTO active_throttle FROM provider_throttle_observations
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND effective_until > target_now
     ORDER BY effective_until DESC, id DESC LIMIT 1 FOR UPDATE;
    SELECT * INTO existing_wait FROM provider_admission_waits
     WHERE external_effect_id = target_effect_id FOR UPDATE;
    IF FOUND AND (
        existing_wait.id IS DISTINCT FROM target_wait_id
        OR existing_wait.workspace_id IS DISTINCT FROM target_workspace_id
        OR existing_wait.connection_id IS DISTINCT FROM target_connection_id
        OR existing_wait.requested_admission_id IS DISTINCT FROM target_admission_id
        OR existing_wait.requested_concurrency_lease_id IS DISTINCT FROM target_concurrency_lease_id
        OR existing_wait.requested_dispatch_ttl_seconds IS DISTINCT FROM target_dispatch_ttl_seconds
    ) THEN
        RAISE EXCEPTION 'provider dispatch wait replay tuple mismatch'
            USING ERRCODE = '23514';
    END IF;

    IF active_throttle.id IS NOT NULL THEN
        target_retry := LEAST(
            policy_row.provider_throttle_cap_seconds,
            GREATEST(1, CEIL(EXTRACT(EPOCH FROM active_throttle.effective_until - target_now))::INTEGER)
        );
        IF existing_wait.id IS NOT NULL AND existing_wait.terminal_reason IS NULL THEN
            UPDATE provider_admission_waits
               SET terminal_reason = 'throttled', terminal_at = target_now
             WHERE id = existing_wait.id;
        END IF;
        INSERT INTO connection_dispatch_admissions (
            id, workspace_id, connection_id, connection_revision_id,
            policy_revision_id, external_effect_id, model_binding_snapshot_id,
            qualification_target_binding_id, decision, admitted_at,
            requested_wait_id, requested_concurrency_lease_id,
            requested_dispatch_ttl_seconds, retry_after_seconds, dispatch_expires_at
        ) VALUES (
            target_admission_id, target_workspace_id, target_connection_id,
            target_connection_revision_id, policy_row.id, target_effect_id,
            target_snapshot_id, target_qualification_target_id, 'throttled', target_now,
            target_wait_id, target_concurrency_lease_id, target_dispatch_ttl_seconds,
            target_retry, NULL
        );
        decision := 'throttled';
        retry_after_seconds := target_retry;
        concurrency_lease_id := NULL;
        wait_deadline_at := existing_wait.deadline_at;
        dispatch_expires_at := NULL;
        RETURN NEXT;
        RETURN;
    END IF;

    IF existing_wait.id IS NOT NULL
       AND existing_wait.terminal_reason IS NULL
       AND existing_wait.deadline_at <= target_now THEN
        UPDATE provider_admission_waits
           SET terminal_reason = 'timeout', terminal_at = target_now
         WHERE id = existing_wait.id;
        INSERT INTO connection_dispatch_admissions (
            id, workspace_id, connection_id, connection_revision_id,
            policy_revision_id, external_effect_id, model_binding_snapshot_id,
            qualification_target_binding_id, decision, admitted_at,
            requested_wait_id, requested_concurrency_lease_id,
            requested_dispatch_ttl_seconds, retry_after_seconds, dispatch_expires_at
        ) VALUES (
            target_admission_id, target_workspace_id, target_connection_id,
            target_connection_revision_id, policy_row.id, target_effect_id,
            target_snapshot_id, target_qualification_target_id, 'conflict', target_now,
            target_wait_id, target_concurrency_lease_id, target_dispatch_ttl_seconds,
            NULL, NULL
        );
        decision := 'conflict';
        retry_after_seconds := NULL;
        concurrency_lease_id := NULL;
        wait_deadline_at := existing_wait.deadline_at;
        dispatch_expires_at := NULL;
        RETURN NEXT;
        RETURN;
    END IF;

    IF state_row.active_lease_count >= policy_row.max_in_flight
       OR state_row.admitted_in_window >= policy_row.requests_per_60_seconds THEN
        IF existing_wait.id IS NULL THEN
            INSERT INTO provider_admission_waits (
                id, workspace_id, connection_id, external_effect_id,
                requested_at, deadline_at, requested_admission_id,
                requested_concurrency_lease_id, requested_dispatch_ttl_seconds
            ) VALUES (
                target_wait_id, target_workspace_id, target_connection_id,
                target_effect_id, target_now,
                target_now + make_interval(secs => policy_row.queue_wait_timeout_seconds),
                target_admission_id, target_concurrency_lease_id,
                target_dispatch_ttl_seconds
            ) RETURNING * INTO existing_wait;
        ELSIF existing_wait.terminal_reason IS NOT NULL THEN
            RAISE EXCEPTION 'provider dispatch wait is already terminal'
                USING ERRCODE = '23514';
        END IF;
        decision := 'conflict';
        retry_after_seconds := NULL;
        concurrency_lease_id := NULL;
        wait_deadline_at := existing_wait.deadline_at;
        dispatch_expires_at := NULL;
        RETURN NEXT;
        RETURN;
    END IF;

    SELECT slot::SMALLINT INTO target_slot
      FROM generate_series(0, policy_row.max_in_flight - 1) AS slot
     WHERE NOT EXISTS (
         SELECT 1 FROM provider_concurrency_leases AS lease
          WHERE lease.workspace_id = target_workspace_id
            AND lease.connection_id = target_connection_id
            AND lease.slot_ordinal = slot
            AND lease.released_at IS NULL
     )
     ORDER BY slot LIMIT 1;
    IF target_slot IS NULL THEN
        RAISE EXCEPTION 'provider dispatch slot accounting is inconsistent'
            USING ERRCODE = '23514';
    END IF;
    IF existing_wait.id IS NOT NULL THEN
        IF existing_wait.terminal_reason IS NOT NULL THEN
            RAISE EXCEPTION 'provider dispatch wait is already terminal'
                USING ERRCODE = '23514';
        END IF;
        UPDATE provider_admission_waits
           SET terminal_reason = 'admitted', terminal_at = target_now
         WHERE id = existing_wait.id;
    END IF;
    INSERT INTO connection_dispatch_admissions (
        id, workspace_id, connection_id, connection_revision_id,
        policy_revision_id, external_effect_id, model_binding_snapshot_id,
        qualification_target_binding_id, decision, admitted_at,
        requested_wait_id, requested_concurrency_lease_id,
        requested_dispatch_ttl_seconds, retry_after_seconds, dispatch_expires_at
    ) VALUES (
        target_admission_id, target_workspace_id, target_connection_id,
        target_connection_revision_id, policy_row.id, target_effect_id,
        target_snapshot_id, target_qualification_target_id, 'admitted', target_now,
        target_wait_id, target_concurrency_lease_id, target_dispatch_ttl_seconds,
        NULL, target_dispatch_expiry
    );
    INSERT INTO provider_concurrency_leases (
        id, workspace_id, connection_id, external_effect_id, slot_ordinal,
        issued_at, expires_at
    ) VALUES (
        target_concurrency_lease_id, target_workspace_id, target_connection_id,
        target_effect_id, target_slot, target_now, target_dispatch_expiry
    );
    UPDATE connection_admission_states
       SET admitted_in_window = admitted_in_window + 1,
           active_lease_count = active_lease_count + 1,
           version = version + 1, updated_at = target_now
     WHERE workspace_id = target_workspace_id AND connection_id = target_connection_id;
    decision := 'admitted';
    retry_after_seconds := NULL;
    concurrency_lease_id := target_concurrency_lease_id;
    wait_deadline_at := NULL;
    dispatch_expires_at := target_dispatch_expiry;
    RETURN NEXT;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_lock_embedding_job_recovery_authority(
    target_workspace_id UUID,
    target_job_id UUID
)
RETURNS TABLE (
    job_id UUID,
    workspace_id UUID,
    space_registration_id UUID,
    kind TEXT,
    state TEXT,
    model_binding_snapshot_id UUID,
    external_effect_id UUID,
    model_request_evidence_id UUID,
    phase TEXT,
    connection_id UUID,
    connection_revision_id UUID,
    dispatch_transition_id UUID,
    dispatch_expires_at TIMESTAMPTZ,
    has_receipt BOOLEAN,
    has_result_preparation BOOLEAN
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    initial embedding_jobs%ROWTYPE;
    job embedding_jobs%ROWTYPE;
    admission RECORD;
    root_connection_id UUID := NULL;
    canonical_connection_id UUID := NULL;
    canonical_connection_revision_id UUID := NULL;
    canonical_dispatch_transition_id UUID := NULL;
    canonical_dispatch_expires_at TIMESTAMPTZ := NULL;
    receipt_exists BOOLEAN;
    preparation_exists BOOLEAN;
    derived_phase TEXT;
BEGIN
    IF target_workspace_id IS NULL OR target_job_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'embedding job recovery authority arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT stored.* INTO initial
      FROM embedding_jobs AS stored
     WHERE stored.workspace_id = target_workspace_id AND stored.id = target_job_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job is absent' USING ERRCODE = '23514';
    END IF;

    PERFORM vestrace_lock_embedding_job_pre_dispatch_gate(
        target_workspace_id, initial.external_effect_id, TRUE
    );

    -- The admitted admission is what makes a job post-admission. Its absence is
    -- the pre-dispatch case and is locked directly, exactly as the Run-step twin
    -- locks a `reserved` attempt without first waiting on a connection guard it
    -- has no admission for.
    SELECT stored.connection_id, stored.connection_revision_id
      INTO admission
      FROM connection_dispatch_admissions AS stored
     WHERE stored.workspace_id = target_workspace_id
       AND stored.external_effect_id = initial.external_effect_id
       AND stored.decision = 'admitted'
       AND stored.model_binding_snapshot_id = initial.model_binding_snapshot_id;
    IF FOUND THEN
        root_connection_id := admission.connection_id;
        canonical_connection_id := admission.connection_id;
        canonical_connection_revision_id := admission.connection_revision_id;
        -- Canonical lock order: the permanent connection guard before the job.
        PERFORM guard.id
          FROM connection_execution_guards AS guard
         WHERE guard.workspace_id = target_workspace_id
           AND guard.connection_id = root_connection_id
         FOR UPDATE OF guard;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'embedding job recovery requires its permanent connection guard'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    SELECT stored.* INTO job
      FROM embedding_jobs AS stored
     WHERE stored.workspace_id = target_workspace_id
       AND stored.id = initial.id
       AND stored.model_binding_snapshot_id = initial.model_binding_snapshot_id
       AND stored.external_effect_id = initial.external_effect_id
       AND stored.model_request_evidence_id = initial.model_request_evidence_id
     FOR UPDATE OF stored;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding job identity changed before its recovery lock'
            USING ERRCODE = '23514';
    END IF;

    SELECT lifecycle.id, lifecycle.dispatch_expires_at
      INTO canonical_dispatch_transition_id, canonical_dispatch_expires_at
      FROM external_effect_lifecycle_transitions AS lifecycle
     WHERE lifecycle.workspace_id = target_workspace_id
       AND lifecycle.effect_id = job.external_effect_id
       AND lifecycle.status = 'dispatching'
       AND lifecycle.cause = 'dispatch_started'
       AND lifecycle.dispatch_expires_at IS NOT NULL
     ORDER BY lifecycle.ordinal DESC
     LIMIT 1
     FOR KEY SHARE OF lifecycle;

    receipt_exists := EXISTS (
        SELECT 1 FROM external_effect_receipts AS receipt
         WHERE receipt.workspace_id = job.workspace_id
           AND receipt.effect_id = job.external_effect_id
    );
    preparation_exists := EXISTS (
        SELECT 1 FROM provider_result_preparations AS preparation
         WHERE preparation.workspace_id = job.workspace_id
           AND preparation.external_effect_id = job.external_effect_id
    );

    -- The order of these tests is the order of the evidence, most durable
    -- first. A job that is terminal says so before anything else, because no
    -- scheduler may advance it whatever the effect looks like.
    IF job.state = 'succeeded' THEN
        derived_phase := 'succeeded';
    ELSIF job.state = 'inconclusive_unknown' THEN
        derived_phase := 'unknown';
    ELSIF job.state IN ('cancelled','failed_definite') THEN
        derived_phase := job.state;
    ELSIF receipt_exists AND preparation_exists THEN
        derived_phase := 'result_prepared';
    ELSIF canonical_dispatch_transition_id IS NOT NULL THEN
        derived_phase := 'dispatching';
    ELSIF root_connection_id IS NOT NULL THEN
        derived_phase := 'admitted';
    ELSE
        derived_phase := 'reserved';
    END IF;

    RETURN QUERY
    SELECT job.id,
           job.workspace_id,
           job.space_registration_id,
           job.kind,
           job.state,
           job.model_binding_snapshot_id,
           job.external_effect_id,
           job.model_request_evidence_id,
           derived_phase,
           canonical_connection_id,
           canonical_connection_revision_id,
           canonical_dispatch_transition_id,
           canonical_dispatch_expires_at,
           receipt_exists,
           preparation_exists;
END
$$;

DO $$
DECLARE target REGPROCEDURE;
BEGIN
    FOREACH target IN ARRAY ARRAY[
      'public.vestrace_validate_embedding_job_output_membership()'::REGPROCEDURE,
      'public.vestrace_lock_embedding_job_pre_dispatch_gate(UUID, UUID, BOOLEAN)'::REGPROCEDURE,
      'public.vestrace_reserve_embedding_job_output_intent(UUID, UUID, UUID, BIGINT, UUID, UUID, UUID)'::REGPROCEDURE,
      'public.vestrace_terminate_embedding_job_pre_dispatch(UUID, UUID, UUID, UUID, BIGINT, TEXT, TEXT, TEXT, UUID, TEXT, TEXT, TEXT, TEXT, TEXT)'::REGPROCEDURE,
      'public.vestrace_fence_embedding_job_dispatching()'::REGPROCEDURE
      ,'public.vestrace_lock_provider_dispatch_routing(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
      ,'public.vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)'::REGPROCEDURE
      ,'public.vestrace_lock_embedding_job_recovery_authority(UUID, UUID)'::REGPROCEDURE
    ] LOOP PERFORM vestrace_assign_p03_function_owner(target); END LOOP;
    PERFORM vestrace_assign_p03_table_owner('public.embedding_job_material_intents'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.embedding_job_termination_receipts'::REGCLASS);
    PERFORM vestrace_assign_p03_table_owner('public.provider_concurrency_leases'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN RAISE; END IF;
    ALTER TABLE embedding_job_material_intents OWNER TO vestrace_guarded_owner;
    ALTER TABLE embedding_job_termination_receipts OWNER TO vestrace_guarded_owner;
    ALTER TABLE provider_concurrency_leases OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_validate_embedding_job_output_membership() OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_lock_embedding_job_pre_dispatch_gate(UUID,UUID,BOOLEAN) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_reserve_embedding_job_output_intent(UUID,UUID,UUID,BIGINT,UUID,UUID,UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_terminate_embedding_job_pre_dispatch(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_fence_embedding_job_dispatching() OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_lock_provider_dispatch_routing(UUID,UUID,UUID,UUID,UUID,TEXT,UUID,UUID,UUID,UUID,UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_try_admit_provider_dispatch(UUID,UUID,UUID,UUID,UUID,UUID,UUID,UUID,TEXT,UUID,UUID,UUID,UUID,UUID,TEXT,INTEGER) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_lock_embedding_job_recovery_authority(UUID,UUID) OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON FUNCTION vestrace_validate_embedding_job_output_membership() FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_lock_embedding_job_pre_dispatch_gate(UUID,UUID,BOOLEAN) FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_reserve_embedding_job_output_intent(UUID,UUID,UUID,BIGINT,UUID,UUID,UUID) FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_terminate_embedding_job_pre_dispatch(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT) FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_fence_embedding_job_dispatching() FROM PUBLIC;
    GRANT SELECT ON embedding_job_material_intents,embedding_job_termination_receipts TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_reserve_embedding_job_output_intent(UUID,UUID,UUID,BIGINT,UUID,UUID,UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_terminate_embedding_job_pre_dispatch(UUID,UUID,UUID,UUID,BIGINT,TEXT,TEXT,TEXT,UUID,TEXT,TEXT,TEXT,TEXT,TEXT) TO vestrace;
END $$;
DO $$
BEGIN
    PERFORM public.vestrace_finish_p04_termination_upgrade();
EXCEPTION WHEN undefined_function THEN
    -- sqlx fresh schemas run as a superuser without the production bootstrap;
    -- real runtime migrations require the narrowly provisioned finalizer.
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),FALSE) THEN
        RAISE EXCEPTION 'P04 termination trigger hand-back must be provisioned before runtime migration'
            USING ERRCODE='42501';
    END IF;
END $$;
