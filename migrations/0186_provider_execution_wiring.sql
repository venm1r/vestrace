-- Task 11 owns the durable identity of one provider attempt for one Run step.
-- The row is intentionally reserved before host-side vault work: a restart must
-- re-use these ids rather than manufacture a second governed input/effect/MRE.

-- Existing deployments run this migration as the restricted runtime role.  A
-- bootstrap-provisioned, no-argument bridge lends exactly the one forward-
-- replaced Candidate-abandon function to that role.  Fresh SQLx databases use
-- the deliberately narrow superuser fallback and never broaden production ACLs.
DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_task11_candidate_abandon_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_task11_candidate_abandon_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'Task 11 Candidate-abandon ownership hand-back must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
    END IF;
END
$$;

-- A crash after guarded preparation keeps the Candidate cancellation evidence
-- immutable.  Recovery supplies the *original* association version: the
-- resulting version must be exactly that version plus one, and the existing
-- preparation must still bind the same intent and material key.  Any stale or
-- substituted request is refused rather than converging onto another lifecycle.
CREATE OR REPLACE FUNCTION vestrace_prepare_candidate_abandon_and_erasure(
    target_intent_id UUID,
    target_expected_association_version BIGINT
)
RETURNS TABLE (
    preparation_id UUID,
    material_key_id UUID,
    finalized_erasure_receipt UUID
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
    preparation_row material_erasure_preparations%ROWTYPE;
BEGIN
    IF target_intent_id IS NULL OR target_expected_association_version IS NULL
       OR target_expected_association_version < 0 THEN
        RAISE EXCEPTION 'candidate abandon arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO occupancy_row FROM credential_guard_occupancies
        WHERE id = intent_row.occupancy_id FOR UPDATE;
    SELECT * INTO intent_row FROM credential_key_creation_intents
        WHERE id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_revisions WHERE id = intent_row.credential_revision_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_materials WHERE intent_id = intent_row.id FOR UPDATE;
    PERFORM vestrace_assert_material_erasure_workspace(intent_row.workspace_id);

    SELECT * INTO preparation_row FROM material_erasure_preparations
        WHERE credential_intent_id = intent_row.id FOR UPDATE;
    IF FOUND THEN
        IF NOT (
               (intent_row.state = 'erasure_prepared'
                AND occupancy_row.state = 'candidate'
                AND occupancy_row.association_version = target_expected_association_version + 1
                AND preparation_row.erasure_receipt IS NULL)
               OR
               (intent_row.state = 'destroyed'
                AND occupancy_row.state = 'destroyed'
                AND occupancy_row.association_version = target_expected_association_version + 2
                AND preparation_row.erasure_receipt IS NOT NULL)
           )
           OR preparation_row.workspace_id <> intent_row.workspace_id
           OR preparation_row.target_kind <> 'credential'
           OR preparation_row.credential_intent_id <> intent_row.id
           OR preparation_row.material_key_id <> intent_row.material_key_id
           OR NOT EXISTS (
               SELECT 1 FROM credential_association_events
               WHERE intent_id = intent_row.id
                 AND occupancy_id = occupancy_row.id
                 AND event_kind = 'credential_association_cancelled'
                 AND expected_version = target_expected_association_version
                 AND resulting_version = target_expected_association_version + 1
           )
           OR EXISTS (
               SELECT 1 FROM credential_slots
               WHERE workspace_id = intent_row.workspace_id
                 AND id = intent_row.credential_slot_id
                 AND current_revision_id = intent_row.credential_revision_id
           )
           OR EXISTS (
               SELECT 1 FROM credential_activation_events
               WHERE credential_revision_id = intent_row.credential_revision_id
                 AND event_kind = 'active'
           ) THEN
            RAISE EXCEPTION 'Candidate erasure replay requires its exact original cancellation evidence and preparation tuple'
                USING ERRCODE = '23514';
        END IF;
        preparation_id := preparation_row.id;
        material_key_id := preparation_row.material_key_id;
        finalized_erasure_receipt := preparation_row.erasure_receipt;
        RETURN NEXT;
        RETURN;
    END IF;

    IF intent_row.state <> 'candidate'
       OR occupancy_row.state <> 'candidate'
       OR occupancy_row.association_version <> target_expected_association_version
       OR EXISTS (
           SELECT 1 FROM credential_slots
           WHERE workspace_id = intent_row.workspace_id
             AND id = intent_row.credential_slot_id
             AND current_revision_id = intent_row.credential_revision_id
       )
       OR EXISTS (
           SELECT 1 FROM credential_activation_events
           WHERE credential_revision_id = intent_row.credential_revision_id
             AND event_kind = 'active'
       ) THEN
        RAISE EXCEPTION 'Candidate abandon requires the exact non-current Candidate association version'
            USING ERRCODE = '23514';
    END IF;
    IF vestrace_material_erasure_has_blocker('credential', NULL, intent_row.id) THEN
        RAISE EXCEPTION 'credential material erasure has a nonterminal blocker'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO credential_association_events (
        id, workspace_id, occupancy_id, intent_id, event_kind, expected_version, resulting_version
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, occupancy_row.id, intent_row.id,
        'credential_association_cancelled', occupancy_row.association_version,
        occupancy_row.association_version + 1
    );
    UPDATE credential_guard_occupancies
       SET association_version = association_version + 1, updated_at = NOW()
     WHERE id = occupancy_row.id;

    INSERT INTO material_erasure_preparations (
        id, workspace_id, target_kind, credential_intent_id, material_key_id
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, 'credential', intent_row.id,
        intent_row.material_key_id
    ) RETURNING * INTO preparation_row;
    INSERT INTO material_erasure_events (id, workspace_id, preparation_id, event_kind)
    VALUES (gen_random_uuid(), intent_row.workspace_id, preparation_row.id, 'erasure_prepared');
    INSERT INTO credential_lifecycle_events (
        id, workspace_id, intent_id, credential_revision_id, event_kind
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, intent_row.id,
        intent_row.credential_revision_id, 'erasure_prepared'
    );
    UPDATE credential_key_creation_intents SET state = 'erasure_prepared', updated_at = NOW()
        WHERE id = intent_row.id;

    preparation_id := preparation_row.id;
    material_key_id := preparation_row.material_key_id;
    finalized_erasure_receipt := NULL;
    RETURN NEXT;
END
$$;

-- Close the production ownership loan in the same transaction.  A fresh
-- superuser SQLx database has no bridge and already preserves its guarded
-- owner across CREATE OR REPLACE; a restricted runtime migration must prove
-- the bootstrap bridge was present for both calls.
DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_prepare_task11_candidate_abandon_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_task11_candidate_abandon_upgrade();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'Task 11 Candidate-abandon ownership hand-back is unavailable after replacement'
                USING ERRCODE = '42501';
        END IF;
    END IF;
END
$$;

CREATE TABLE run_step_execution_attempts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    run_id UUID NOT NULL,
    step_id UUID NOT NULL,
    model_binding_snapshot_id UUID NOT NULL,
    input_material_intent_id UUID NOT NULL,
    input_content_material_id UUID NOT NULL,
    input_material_key_id UUID NOT NULL,
    input_intent_nonce UUID NOT NULL,
    input_prepared_attachment_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    model_request_evidence_id UUID NOT NULL,
    phase TEXT NOT NULL DEFAULT 'reserved'
        CHECK (phase IN ('reserved', 'admitted', 'dispatching', 'result_prepared', 'published', 'unknown')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT run_step_execution_attempts_unique_step
        UNIQUE (workspace_id, run_id, step_id),
    CONSTRAINT run_step_execution_attempts_unique_input_material_intent
        UNIQUE (input_material_intent_id),
    CONSTRAINT run_step_execution_attempts_unique_input_content_material
        UNIQUE (input_content_material_id),
    CONSTRAINT run_step_execution_attempts_unique_input_material_key
        UNIQUE (input_material_key_id),
    CONSTRAINT run_step_execution_attempts_unique_input_intent_nonce
        UNIQUE (input_intent_nonce),
    CONSTRAINT run_step_execution_attempts_unique_input_prepared_attachment
        UNIQUE (input_prepared_attachment_id),
    CONSTRAINT run_step_execution_attempts_unique_external_effect
        UNIQUE (external_effect_id),
    CONSTRAINT run_step_execution_attempts_unique_model_request_evidence
        UNIQUE (model_request_evidence_id),
    CONSTRAINT run_step_execution_attempts_run_step_fkey
        FOREIGN KEY (workspace_id, run_id, step_id)
        REFERENCES run_steps(workspace_id, run_id, id) ON DELETE RESTRICT,
    -- `run_model_binding_snapshots` has independent UNIQUE keys for the Run
    -- and snapshot. The exact triple is checked by the guarded reserver below
    -- rather than by an invalid composite foreign key.
    CONSTRAINT run_step_execution_attempts_snapshot_fkey
        FOREIGN KEY (workspace_id, model_binding_snapshot_id)
        REFERENCES model_binding_snapshots(workspace_id, id) ON DELETE RESTRICT
);

ALTER TABLE run_step_execution_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE run_step_execution_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY run_step_execution_attempts_workspace_policy
    ON run_step_execution_attempts
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
REVOKE ALL ON run_step_execution_attempts FROM PUBLIC;
CREATE TRIGGER run_step_execution_attempts_guarded
    BEFORE INSERT OR UPDATE OR DELETE ON run_step_execution_attempts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

CREATE OR REPLACE FUNCTION vestrace_reserve_run_step_execution_attempt(
    target_attempt_id UUID,
    target_workspace_id UUID,
    target_run_id UUID,
    target_step_id UUID,
    target_snapshot_id UUID,
    target_input_material_intent_id UUID,
    target_input_content_material_id UUID,
    target_input_material_key_id UUID,
    target_input_intent_nonce UUID,
    target_input_attachment_id UUID,
    target_external_effect_id UUID,
    target_model_request_evidence_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing run_step_execution_attempts%ROWTYPE;
BEGIN
    IF target_attempt_id IS NULL OR target_workspace_id IS NULL OR target_run_id IS NULL
       OR target_step_id IS NULL OR target_snapshot_id IS NULL
       OR target_input_material_intent_id IS NULL OR target_input_content_material_id IS NULL
       OR target_input_material_key_id IS NULL OR target_input_intent_nonce IS NULL
       OR target_input_attachment_id IS NULL OR target_external_effect_id IS NULL
       OR target_model_request_evidence_id IS NULL THEN
        RAISE EXCEPTION 'run step execution attempt arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    -- Serialize the absent-tuple case before inspecting the canonical Run
    -- binding.  A hash collision can only serialize unrelated attempts; it
    -- cannot merge identities.  This avoids granting the guarded owner a Run
    -- write privilege merely to take a row lock.
    PERFORM pg_advisory_xact_lock(hashtextextended(
        target_workspace_id::TEXT || ':' || target_run_id::TEXT || ':' || target_step_id::TEXT,
        0
    ));

    -- The binding was created atomically at Run acceptance and is never looked
    -- up through a mutable/default model. The guarded owner needs only this
    -- immutable read; the advisory tuple lock above provides convergence.
    PERFORM 1 FROM run_steps
     WHERE workspace_id = target_workspace_id AND run_id = target_run_id AND id = target_step_id
    ;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM run_model_binding_snapshots
         WHERE workspace_id = target_workspace_id
           AND run_id = target_run_id
           AND snapshot_id = target_snapshot_id
    ) THEN
        RAISE EXCEPTION 'run step execution attempt requires its exact pinned binding'
            USING ERRCODE = '23514';
    END IF;

    SELECT * INTO existing
      FROM run_step_execution_attempts
     WHERE workspace_id = target_workspace_id
       AND run_id = target_run_id
       AND step_id = target_step_id
     FOR UPDATE;

    IF FOUND THEN
        IF existing.id = target_attempt_id
           AND existing.model_binding_snapshot_id = target_snapshot_id
           AND existing.input_material_intent_id = target_input_material_intent_id
           AND existing.input_content_material_id = target_input_content_material_id
           AND existing.input_material_key_id = target_input_material_key_id
           AND existing.input_intent_nonce = target_input_intent_nonce
           AND existing.input_prepared_attachment_id = target_input_attachment_id
           AND existing.external_effect_id = target_external_effect_id
           AND existing.model_request_evidence_id = target_model_request_evidence_id THEN
            RETURN existing.id;
        END IF;
        RAISE EXCEPTION 'run step execution attempt identity conflicts with its existing tuple'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO run_step_execution_attempts (
        id, workspace_id, run_id, step_id, model_binding_snapshot_id,
        input_material_intent_id, input_content_material_id, input_material_key_id,
        input_intent_nonce, input_prepared_attachment_id, external_effect_id,
        model_request_evidence_id
    ) VALUES (
        target_attempt_id, target_workspace_id, target_run_id, target_step_id, target_snapshot_id,
        target_input_material_intent_id, target_input_content_material_id,
        target_input_material_key_id, target_input_intent_nonce, target_input_attachment_id,
        target_external_effect_id, target_model_request_evidence_id
    );
    RETURN target_attempt_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_transition_run_step_execution_attempt(
    target_workspace_id UUID,
    target_run_id UUID,
    target_step_id UUID,
    target_phase TEXT
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    initial_attempt run_step_execution_attempts%ROWTYPE;
    attempt run_step_execution_attempts%ROWTYPE;
    root_connection_id UUID;
    canonical_connection_id UUID;
    canonical_connection_revision_id UUID;
    canonical_authorization_id UUID;
    canonical_concurrency_lease_id UUID;
    canonical_auth_mode TEXT;
    canonical_credential_slot_id UUID;
    canonical_credential_lease_id UUID;
    canonical_dispatch_transition_id UUID;
BEGIN
    IF target_workspace_id IS NULL OR target_run_id IS NULL OR target_step_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
       OR target_phase NOT IN ('admitted', 'dispatching', 'unknown', 'result_prepared', 'published') THEN
        RAISE EXCEPTION 'run step execution attempt transition arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    -- Dispatch holds the permanent connection guard before it reaches the
    -- attempt.  Read the immutable tuple first, then acquire that same root
    -- before taking any mutable attempt/effect locks.
    SELECT stored_attempt.* INTO initial_attempt
      FROM run_step_execution_attempts AS stored_attempt
     WHERE stored_attempt.workspace_id = target_workspace_id
       AND stored_attempt.run_id = target_run_id
       AND stored_attempt.step_id = target_step_id
    ;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step execution attempt is absent' USING ERRCODE = '23514';
    END IF;

    SELECT admission.connection_id
      INTO root_connection_id
      FROM provider_dispatch_causes AS cause
      JOIN external_effect_intents AS intent
        ON intent.workspace_id = cause.workspace_id
       AND intent.id = cause.external_effect_id
      JOIN connection_dispatch_admissions AS admission
        ON admission.workspace_id = cause.workspace_id
       AND admission.external_effect_id = cause.external_effect_id
       AND admission.decision = 'admitted'
       AND admission.model_binding_snapshot_id = initial_attempt.model_binding_snapshot_id
     WHERE cause.workspace_id = target_workspace_id
       AND cause.external_effect_id = initial_attempt.external_effect_id
       AND cause.cause_kind = 'run_step'
       AND cause.run_id = initial_attempt.run_id
       AND cause.step_id = initial_attempt.step_id
       AND cause.model_binding_snapshot_id = initial_attempt.model_binding_snapshot_id
       AND cause.model_request_evidence_id = initial_attempt.model_request_evidence_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step execution attempt requires its exact admitted dispatch authority'
            USING ERRCODE = '23514';
    END IF;

    PERFORM guard.id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = root_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step execution attempt requires its permanent connection guard'
            USING ERRCODE = '23514';
    END IF;

    SELECT stored_attempt.* INTO attempt
      FROM run_step_execution_attempts AS stored_attempt
     WHERE stored_attempt.id = initial_attempt.id
       AND stored_attempt.workspace_id = target_workspace_id
       AND stored_attempt.run_id = target_run_id
       AND stored_attempt.step_id = target_step_id
       AND stored_attempt.model_binding_snapshot_id = initial_attempt.model_binding_snapshot_id
       AND stored_attempt.external_effect_id = initial_attempt.external_effect_id
       AND stored_attempt.model_request_evidence_id = initial_attempt.model_request_evidence_id
     FOR UPDATE OF stored_attempt;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step execution attempt changed before its canonical guard lock'
            USING ERRCODE = '23514';
    END IF;

    -- The immutable authorization is intentionally read-validated only: the
    -- guarded owner has no row-lock privilege for it.  All mutable dispatch
    -- authority is locked before an attempt phase is advanced.
    SELECT admission.connection_id,
           admission.connection_revision_id,
           auth.id,
           lease.id,
           revision.auth_mode,
           revision.credential_slot_id
      INTO canonical_connection_id,
           canonical_connection_revision_id,
           canonical_authorization_id,
           canonical_concurrency_lease_id,
           canonical_auth_mode,
           canonical_credential_slot_id
      FROM provider_dispatch_causes AS cause
      JOIN external_effect_intents AS intent
        ON intent.workspace_id = cause.workspace_id
       AND intent.id = cause.external_effect_id
      JOIN connection_dispatch_admissions AS admission
        ON admission.workspace_id = cause.workspace_id
       AND admission.external_effect_id = cause.external_effect_id
       AND admission.decision = 'admitted'
       AND admission.model_binding_snapshot_id = attempt.model_binding_snapshot_id
       AND admission.connection_id = root_connection_id
      JOIN provider_concurrency_leases AS lease
        ON lease.workspace_id = admission.workspace_id
       AND lease.external_effect_id = admission.external_effect_id
       AND lease.connection_id = admission.connection_id
      JOIN external_effect_authorizations AS auth
        ON auth.workspace_id = cause.workspace_id
       AND auth.effect_id = cause.external_effect_id
       AND auth.result = 'allow'
      JOIN external_effect_lifecycle_transitions AS authorized
        ON authorized.workspace_id = auth.workspace_id
       AND authorized.effect_id = auth.effect_id
       AND authorized.status = 'authorized'
       AND authorized.cause = 'authorization_recorded'
       AND authorized.cause_ref = auth.id::TEXT
      JOIN connection_revisions AS revision
        ON revision.workspace_id = admission.workspace_id
       AND revision.connection_id = admission.connection_id
       AND revision.id = admission.connection_revision_id
     WHERE cause.workspace_id = target_workspace_id
       AND cause.external_effect_id = attempt.external_effect_id
       AND cause.cause_kind = 'run_step'
       AND cause.run_id = attempt.run_id
       AND cause.step_id = attempt.step_id
       AND cause.model_binding_snapshot_id = attempt.model_binding_snapshot_id
       AND cause.model_request_evidence_id = attempt.model_request_evidence_id
     ORDER BY auth.decided_at DESC, auth.id DESC
     LIMIT 1
     FOR KEY SHARE OF cause, admission, lease, authorized;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step execution attempt requires its exact admitted dispatch authority'
            USING ERRCODE = '23514';
    END IF;

    IF canonical_auth_mode = 'none' THEN
        IF canonical_credential_slot_id IS NOT NULL OR EXISTS (
            SELECT 1 FROM credential_dispatch_leases AS credential
             WHERE credential.workspace_id = target_workspace_id
               AND credential.external_effect_id = attempt.external_effect_id
        ) THEN
            RAISE EXCEPTION 'run step execution attempt no-auth authority is not exact'
                USING ERRCODE = '23514';
        END IF;
    ELSE
        IF canonical_credential_slot_id IS NULL THEN
            RAISE EXCEPTION 'run step execution attempt credential authority is malformed'
                USING ERRCODE = '23514';
        END IF;
        SELECT credential.id
          INTO canonical_credential_lease_id
          FROM credential_dispatch_leases AS credential
         WHERE credential.workspace_id = target_workspace_id
           AND credential.external_effect_id = attempt.external_effect_id
           AND credential.authorization_id = canonical_authorization_id
           AND credential.connection_id = canonical_connection_id
           AND credential.credential_slot_id = canonical_credential_slot_id
           AND credential.auth_mode = canonical_auth_mode
           AND credential.consumed_at IS NOT NULL
           AND credential.terminal_state = 'consumed_for_dispatch'
         FOR KEY SHARE OF credential;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step execution attempt credential authority is not exact'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF target_phase IN ('dispatching', 'unknown', 'result_prepared', 'published')
       OR attempt.phase IN ('dispatching', 'result_prepared', 'published', 'unknown') THEN
        SELECT dispatch.id
          INTO canonical_dispatch_transition_id
          FROM external_effect_lifecycle_transitions AS dispatch
         WHERE dispatch.workspace_id = target_workspace_id
           AND dispatch.effect_id = attempt.external_effect_id
           AND dispatch.status = 'dispatching'
           AND dispatch.cause = 'dispatch_started'
           AND dispatch.dispatch_expires_at IS NOT NULL
         ORDER BY dispatch.ordinal DESC
         LIMIT 1
         FOR KEY SHARE OF dispatch;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step execution attempt requires its original dispatch transition'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF target_phase = 'unknown' OR attempt.phase = 'unknown' THEN
        PERFORM 1
          FROM external_effect_lifecycle_transitions AS lost
         WHERE lost.workspace_id = target_workspace_id
           AND lost.effect_id = attempt.external_effect_id
           AND lost.status = 'unknown'
           AND lost.cause = 'dispatch_lost'
           AND lost.cause_ref = canonical_dispatch_transition_id::TEXT
         FOR KEY SHARE OF lost;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step execution attempt unknown requires its original dispatch loss transition'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF target_phase = 'result_prepared' THEN
        -- `result_prepared` is not a caller-named completion state.  It is
        -- reachable only after the original Run effect has one exact
        -- prepared result, a witnessed acknowledged receipt/lifecycle pair,
        -- and the original concurrency lease has been released by that same
        -- receipt.  These are all locked after the canonical connection root
        -- and dispatch authority have been revalidated above.
        PERFORM preparation.id
          FROM provider_result_preparations AS preparation
          JOIN external_effect_receipts AS receipt
            ON receipt.workspace_id = preparation.workspace_id
           AND receipt.id = preparation.external_effect_receipt_id
           AND receipt.effect_id = preparation.external_effect_id
           AND receipt.outcome_status = 'acknowledged'
          JOIN external_effect_lifecycle_transitions AS acknowledged
            ON acknowledged.workspace_id = receipt.workspace_id
           AND acknowledged.effect_id = receipt.effect_id
           AND acknowledged.status = 'acknowledged'
           AND acknowledged.cause = 'receipt_recorded'
           AND acknowledged.cause_ref = receipt.id::TEXT
          JOIN provider_concurrency_leases AS released_lease
            ON released_lease.workspace_id = preparation.workspace_id
           AND released_lease.id = canonical_concurrency_lease_id
           AND released_lease.external_effect_id = preparation.external_effect_id
           AND released_lease.released_receipt_id = receipt.id
           AND released_lease.released_at IS NOT NULL
         WHERE preparation.workspace_id = target_workspace_id
           AND preparation.external_effect_id = attempt.external_effect_id
           AND preparation.run_id = attempt.run_id
           AND preparation.step_id = attempt.step_id
           AND preparation.model_request_evidence_id = attempt.model_request_evidence_id
           AND preparation.state = 'result_prepared'
           AND preparation.receipt_witnessed_at IS NOT NULL
         -- Receipts and their lifecycle witnesses are immutable evidence and
         -- are only read-validated.  The guarded owner locks the mutable
         -- preparation and lease without requesting row-lock authority over
         -- those historical receipt rows.
         FOR KEY SHARE OF preparation, released_lease;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step execution attempt result preparation is not exact'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF target_phase = 'published' THEN
        -- Publication is the one terminal continuation of the exact prepared
        -- result.  Receipts, publication, run stream, step event, and work
        -- item are read-validated as immutable/canonical evidence; only the
        -- mutable preparation and released lease are locked here.
        PERFORM preparation.id
          FROM provider_result_preparations AS preparation
          JOIN external_effect_receipts AS receipt
            ON receipt.workspace_id = preparation.workspace_id
           AND receipt.id = preparation.external_effect_receipt_id
           AND receipt.effect_id = preparation.external_effect_id
           AND receipt.outcome_status = 'acknowledged'
          JOIN external_effect_lifecycle_transitions AS acknowledged
            ON acknowledged.workspace_id = receipt.workspace_id
           AND acknowledged.effect_id = receipt.effect_id
           AND acknowledged.status = 'acknowledged'
           AND acknowledged.cause = 'receipt_recorded'
           AND acknowledged.cause_ref = receipt.id::TEXT
          JOIN provider_concurrency_leases AS released_lease
            ON released_lease.workspace_id = preparation.workspace_id
           AND released_lease.id = canonical_concurrency_lease_id
           AND released_lease.external_effect_id = preparation.external_effect_id
           AND released_lease.released_receipt_id = receipt.id
           AND released_lease.released_at IS NOT NULL
          JOIN provider_result_publications AS publication
            ON publication.workspace_id = preparation.workspace_id
           AND publication.provider_result_preparation_id = preparation.id
           AND publication.external_effect_id = preparation.external_effect_id
           AND publication.run_id = preparation.run_id
           AND publication.step_id = preparation.step_id
           AND publication.artifact_id = preparation.artifact_id
           AND publication.artifact_revision_id = preparation.artifact_revision_id
           AND publication.model_execution_id = preparation.model_execution_id
           AND publication.material_intent_id = preparation.material_intent_id
           AND publication.prepared_attachment_id = preparation.prepared_attachment_id
           AND publication.size_class = preparation.size_class
         WHERE preparation.workspace_id = target_workspace_id
           AND preparation.external_effect_id = attempt.external_effect_id
           AND preparation.run_id = attempt.run_id
           AND preparation.step_id = attempt.step_id
           AND preparation.model_request_evidence_id = attempt.model_request_evidence_id
           AND preparation.state = 'published'
           AND preparation.published_at IS NOT NULL
           AND preparation.receipt_witnessed_at IS NOT NULL
           AND preparation.advance_work_item_id IS NOT NULL
           AND vestrace_run_step_publication_evidence_is_exact(
                preparation.workspace_id,
                preparation.run_id,
                preparation.step_id,
                preparation.expected_run_version,
                preparation.artifact_id,
                preparation.artifact_revision_id,
                preparation.model_execution_id,
                preparation.advance_work_item_id
           )
         FOR KEY SHARE OF preparation, released_lease;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step execution attempt publication is not exact'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF target_phase = 'admitted' THEN
        IF attempt.phase = 'reserved' THEN
            UPDATE run_step_execution_attempts
               SET phase = 'admitted', updated_at = NOW()
             WHERE id = attempt.id;
        ELSIF attempt.phase NOT IN ('admitted', 'dispatching') THEN
            RAISE EXCEPTION 'run step execution attempt admitted replay is not lawful'
                USING ERRCODE = '23514';
        END IF;
    ELSIF target_phase = 'dispatching' THEN
        IF attempt.phase = 'admitted' THEN
            UPDATE run_step_execution_attempts
               SET phase = 'dispatching', updated_at = NOW()
             WHERE id = attempt.id;
        ELSIF attempt.phase <> 'dispatching' THEN
            RAISE EXCEPTION 'run step execution attempt dispatching replay is not lawful'
                USING ERRCODE = '23514';
        END IF;
    ELSIF target_phase = 'result_prepared' THEN
        IF attempt.phase = 'dispatching' THEN
            UPDATE run_step_execution_attempts
               SET phase = 'result_prepared', updated_at = NOW()
             WHERE id = attempt.id;
        ELSIF attempt.phase <> 'result_prepared' THEN
            RAISE EXCEPTION 'run step execution attempt result-prepared replay is not lawful'
                USING ERRCODE = '23514';
        END IF;
    ELSIF target_phase = 'published' THEN
        IF attempt.phase = 'result_prepared' THEN
            UPDATE run_step_execution_attempts
               SET phase = 'published', updated_at = NOW()
             WHERE id = attempt.id;
        ELSIF attempt.phase <> 'published' THEN
            RAISE EXCEPTION 'run step execution attempt published replay is not lawful'
                USING ERRCODE = '23514';
        END IF;
    ELSIF attempt.phase = 'dispatching' THEN
        UPDATE run_step_execution_attempts
           SET phase = 'unknown', updated_at = NOW()
         WHERE id = attempt.id;
    ELSIF attempt.phase <> 'unknown' THEN
        RAISE EXCEPTION 'run step execution attempt unknown replay is not lawful'
            USING ERRCODE = '23514';
    END IF;
    RETURN attempt.id;
END
$$;

-- The guarded Run-attempt transition validates its mutable provider-result
-- tuple itself.  Stable Run publication evidence is deliberately read through
-- this RLS-aware owner boundary, rather than granting the guarded owner raw
-- access to the Run stream/event/work tables.
CREATE OR REPLACE FUNCTION vestrace_run_step_publication_evidence_is_exact(
    target_workspace_id UUID,
    target_run_id UUID,
    target_step_id UUID,
    target_expected_run_version BIGINT,
    target_artifact_id UUID,
    target_artifact_revision_id UUID,
    target_model_execution_id UUID,
    target_advance_work_item_id UUID
)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF target_workspace_id IS NULL OR target_run_id IS NULL OR target_step_id IS NULL
       OR target_expected_run_version IS NULL OR target_expected_run_version < 0
       OR target_artifact_id IS NULL OR target_artifact_revision_id IS NULL
       OR target_model_execution_id IS NULL OR target_advance_work_item_id IS NULL
       OR current_setting('vestrace.workspace_id', true) IS NULL
       OR target_workspace_id::TEXT <> current_setting('vestrace.workspace_id', true) THEN
        RETURN FALSE;
    END IF;

    RETURN EXISTS (
        SELECT 1
          FROM agent_runs AS published_run
          JOIN run_streams AS stream
            ON stream.workspace_id = published_run.workspace_id
           AND stream.run_id = published_run.id
           AND stream.current_version = target_expected_run_version + 1
          JOIN run_steps AS published_step
            ON published_step.workspace_id = published_run.workspace_id
           AND published_step.run_id = published_run.id
           AND published_step.id = target_step_id
           AND published_step.status = 'succeeded'
           AND published_step.output_references = jsonb_build_array(
                jsonb_build_object(
                    'id', target_artifact_id,
                    'kind', 'artifact',
                    'artifact_id', target_artifact_id,
                    'artifact_revision_id', target_artifact_revision_id,
                    'model_execution_id', target_model_execution_id
                )
           )
          JOIN run_events AS success_event
            ON success_event.workspace_id = published_run.workspace_id
           AND success_event.run_id = published_run.id
           AND success_event.run_version = target_expected_run_version + 1
           AND success_event.event_type = 'run.step_status_changed'
           AND success_event.payload_kind = 'run.step_status_changed'
           AND success_event.payload = jsonb_build_object(
                'type', 'step_status_changed',
                'step_id', target_step_id,
                'from', 'running',
                'to', 'succeeded',
                'attempt', 1
           )
          JOIN run_work_items AS continuation
            ON continuation.id = target_advance_work_item_id
           AND continuation.workspace_id = published_run.workspace_id
           AND continuation.run_id = published_run.id
           AND continuation.step_id = target_step_id
           AND continuation.kind = 'advance_run'
           AND continuation.expected_run_version = target_expected_run_version + 1
           AND continuation.idempotency_key = format(
                'provider-result:%s:%s',
                target_run_id,
                target_artifact_revision_id
           )
         WHERE published_run.workspace_id = target_workspace_id
           AND published_run.id = target_run_id
           AND published_run.status = 'running'
           AND published_run.run_version = target_expected_run_version + 1
    );
END
$$;
ALTER FUNCTION vestrace_run_step_publication_evidence_is_exact(
    UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID
) OWNER TO vestrace;
REVOKE ALL ON FUNCTION vestrace_run_step_publication_evidence_is_exact(
    UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_run_step_publication_evidence_is_exact(
    UUID, UUID, UUID, BIGINT, UUID, UUID, UUID, UUID
) TO vestrace_guarded_owner;

-- Recovery receives only one opaque Run-step tuple.  The guarded owner locks
-- that durable attempt and reconstructs its authority from the already
-- admitted original effect.  This forward migration adds and requires no new
-- direct runtime table grants over dispatch, lease, authorization, or lifecycle.
CREATE OR REPLACE FUNCTION vestrace_lock_run_step_attempt_recovery_authority(
    target_workspace_id UUID,
    target_run_id UUID,
    target_step_id UUID
)
RETURNS TABLE (
    attempt_id UUID,
    workspace_id UUID,
    run_id UUID,
    step_id UUID,
    model_binding_snapshot_id UUID,
    input_material_intent_id UUID,
    input_content_material_id UUID,
    input_material_key_id UUID,
    input_intent_nonce UUID,
    input_prepared_attachment_id UUID,
    external_effect_id UUID,
    model_request_evidence_id UUID,
    phase TEXT,
    authorization_id UUID,
    connection_id UUID,
    connection_revision_id UUID,
    concurrency_lease_id UUID,
    credential_lease_id UUID,
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
    initial_attempt run_step_execution_attempts%ROWTYPE;
    attempt run_step_execution_attempts%ROWTYPE;
    root_connection_id UUID := NULL;
    canonical_authorization_id UUID := NULL;
    canonical_connection_id UUID := NULL;
    canonical_connection_revision_id UUID := NULL;
    canonical_concurrency_lease_id UUID := NULL;
    canonical_credential_lease_id UUID := NULL;
    canonical_dispatch_transition_id UUID := NULL;
    canonical_dispatch_expires_at TIMESTAMPTZ := NULL;
BEGIN
    IF target_workspace_id IS NULL OR target_run_id IS NULL OR target_step_id IS NULL
       OR target_workspace_id IS DISTINCT FROM
          NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID THEN
        RAISE EXCEPTION 'run step attempt recovery authority arguments are malformed'
            USING ERRCODE = '22023';
    END IF;

    SELECT stored_attempt.* INTO initial_attempt
      FROM run_step_execution_attempts AS stored_attempt
     WHERE stored_attempt.workspace_id = target_workspace_id
       AND stored_attempt.run_id = target_run_id
       AND stored_attempt.step_id = target_step_id
    ;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step execution attempt is absent' USING ERRCODE = '23514';
    END IF;

    -- A truly pre-dispatch attempt has no canonical admission/root yet.  It
    -- may be locked directly, but must not subsequently wait for a connection
    -- guard if another worker advanced it while this call waited.
    IF initial_attempt.phase = 'reserved' THEN
        SELECT stored_attempt.* INTO attempt
          FROM run_step_execution_attempts AS stored_attempt
         WHERE stored_attempt.id = initial_attempt.id
           AND stored_attempt.workspace_id = target_workspace_id
           AND stored_attempt.run_id = target_run_id
           AND stored_attempt.step_id = target_step_id
         FOR UPDATE OF stored_attempt;
        IF attempt.phase <> 'reserved' THEN
            RAISE EXCEPTION 'run step attempt advanced while reserved recovery was acquiring its lock'
                USING ERRCODE = '40001';
        END IF;
        RETURN QUERY
        SELECT attempt.id,
               attempt.workspace_id,
               attempt.run_id,
               attempt.step_id,
               attempt.model_binding_snapshot_id,
               attempt.input_material_intent_id,
               attempt.input_content_material_id,
               attempt.input_material_key_id,
               attempt.input_intent_nonce,
               attempt.input_prepared_attachment_id,
               attempt.external_effect_id,
               attempt.model_request_evidence_id,
               attempt.phase,
               NULL::UUID, NULL::UUID, NULL::UUID, NULL::UUID, NULL::UUID,
               NULL::UUID, NULL::TIMESTAMPTZ,
               EXISTS (
                   SELECT 1 FROM external_effect_receipts AS receipt
                    WHERE receipt.workspace_id = attempt.workspace_id
                      AND receipt.effect_id = attempt.external_effect_id
               ),
               EXISTS (
                   SELECT 1 FROM provider_result_preparations AS preparation
                    WHERE preparation.workspace_id = attempt.workspace_id
                      AND preparation.external_effect_id = attempt.external_effect_id
               );
        RETURN;
    END IF;

    -- Once an admission exists, acquire the same permanent connection root as
    -- prepare_dispatch before locking the attempt or mutable dispatch evidence.
    SELECT admission.connection_id
      INTO root_connection_id
      FROM provider_dispatch_causes AS cause
      JOIN connection_dispatch_admissions AS admission
        ON admission.workspace_id = cause.workspace_id
       AND admission.external_effect_id = cause.external_effect_id
       AND admission.decision = 'admitted'
       AND admission.model_binding_snapshot_id = initial_attempt.model_binding_snapshot_id
     WHERE cause.workspace_id = target_workspace_id
       AND cause.external_effect_id = initial_attempt.external_effect_id
       AND cause.cause_kind = 'run_step'
       AND cause.run_id = initial_attempt.run_id
       AND cause.step_id = initial_attempt.step_id
       AND cause.model_binding_snapshot_id = initial_attempt.model_binding_snapshot_id
       AND cause.model_request_evidence_id = initial_attempt.model_request_evidence_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step attempt recovery requires its exact admitted dispatch authority'
            USING ERRCODE = '23514';
    END IF;
    PERFORM guard.id
      FROM connection_execution_guards AS guard
     WHERE guard.workspace_id = target_workspace_id
       AND guard.connection_id = root_connection_id
     FOR UPDATE OF guard;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'run step attempt recovery requires its permanent connection guard'
            USING ERRCODE = '23514';
    END IF;
    SELECT stored_attempt.* INTO attempt
      FROM run_step_execution_attempts AS stored_attempt
     WHERE stored_attempt.id = initial_attempt.id
       AND stored_attempt.workspace_id = target_workspace_id
       AND stored_attempt.run_id = target_run_id
       AND stored_attempt.step_id = target_step_id
       AND stored_attempt.model_binding_snapshot_id = initial_attempt.model_binding_snapshot_id
       AND stored_attempt.external_effect_id = initial_attempt.external_effect_id
       AND stored_attempt.model_request_evidence_id = initial_attempt.model_request_evidence_id
     FOR UPDATE OF stored_attempt;
    IF NOT FOUND OR attempt.phase = 'reserved' THEN
        RAISE EXCEPTION 'run step attempt recovery changed before its canonical guard lock'
            USING ERRCODE = '23514';
    END IF;

    IF attempt.phase <> 'reserved' THEN
        SELECT auth.id,
               admission.connection_id,
               admission.connection_revision_id,
               lease.id AS concurrency_lease_id,
               credential.id AS credential_lease_id
          INTO canonical_authorization_id,
               canonical_connection_id,
               canonical_connection_revision_id,
               canonical_concurrency_lease_id,
               canonical_credential_lease_id
          FROM provider_dispatch_causes AS cause
          JOIN external_effect_intents AS intent
            ON intent.workspace_id = cause.workspace_id
           AND intent.id = cause.external_effect_id
          JOIN connection_dispatch_admissions AS admission
            ON admission.workspace_id = cause.workspace_id
            AND admission.external_effect_id = cause.external_effect_id
            AND admission.decision = 'admitted'
            AND admission.model_binding_snapshot_id = attempt.model_binding_snapshot_id
            AND admission.connection_id = root_connection_id
          JOIN provider_concurrency_leases AS lease
            ON lease.workspace_id = admission.workspace_id
           AND lease.external_effect_id = admission.external_effect_id
          JOIN external_effect_authorizations AS auth
            ON auth.workspace_id = cause.workspace_id
           AND auth.effect_id = cause.external_effect_id
           AND auth.result = 'allow'
          JOIN external_effect_lifecycle_transitions AS authorized
            ON authorized.workspace_id = auth.workspace_id
           AND authorized.effect_id = auth.effect_id
           AND authorized.status = 'authorized'
           AND authorized.cause = 'authorization_recorded'
           AND authorized.cause_ref = auth.id::TEXT
          LEFT JOIN credential_dispatch_leases AS credential
            ON credential.workspace_id = auth.workspace_id
           AND credential.external_effect_id = auth.effect_id
           AND credential.authorization_id = auth.id
         WHERE cause.workspace_id = target_workspace_id
           AND cause.external_effect_id = attempt.external_effect_id
           AND cause.cause_kind = 'run_step'
           AND cause.run_id = attempt.run_id
           AND cause.step_id = attempt.step_id
           AND cause.model_binding_snapshot_id = attempt.model_binding_snapshot_id
           AND cause.model_request_evidence_id = attempt.model_request_evidence_id
         ORDER BY auth.decided_at DESC, auth.id DESC
         LIMIT 1
         -- The allowed authorization is immutable and exact-validated above.
         -- Do not take a row lock on it: the guarded owner has only the
         -- narrowly sufficient read authority for that historical evidence.
         FOR KEY SHARE OF cause, admission, lease, authorized;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step attempt recovery requires its exact admitted dispatch authority'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    IF attempt.phase IN ('dispatching', 'result_prepared', 'published', 'unknown') THEN
        SELECT lifecycle.id, lifecycle.dispatch_expires_at
          INTO canonical_dispatch_transition_id, canonical_dispatch_expires_at
          FROM external_effect_lifecycle_transitions AS lifecycle
         WHERE lifecycle.workspace_id = target_workspace_id
           AND lifecycle.effect_id = attempt.external_effect_id
           AND lifecycle.status = 'dispatching'
           AND lifecycle.cause = 'dispatch_started'
           AND lifecycle.dispatch_expires_at IS NOT NULL
         ORDER BY lifecycle.ordinal DESC
         LIMIT 1
         FOR KEY SHARE OF lifecycle;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'run step attempt recovery requires its original dispatch transition'
                USING ERRCODE = '23514';
        END IF;
    END IF;

    RETURN QUERY
    SELECT attempt.id,
           attempt.workspace_id,
           attempt.run_id,
           attempt.step_id,
           attempt.model_binding_snapshot_id,
           attempt.input_material_intent_id,
           attempt.input_content_material_id,
           attempt.input_material_key_id,
           attempt.input_intent_nonce,
           attempt.input_prepared_attachment_id,
           attempt.external_effect_id,
           attempt.model_request_evidence_id,
           attempt.phase,
            canonical_authorization_id,
            canonical_connection_id,
            canonical_connection_revision_id,
            canonical_concurrency_lease_id,
            canonical_credential_lease_id,
            canonical_dispatch_transition_id,
            canonical_dispatch_expires_at,
           EXISTS (
               SELECT 1 FROM external_effect_receipts AS receipt
                WHERE receipt.workspace_id = attempt.workspace_id
                  AND receipt.effect_id = attempt.external_effect_id
           ),
           EXISTS (
               SELECT 1 FROM provider_result_preparations AS preparation
                WHERE preparation.workspace_id = attempt.workspace_id
                  AND preparation.external_effect_id = attempt.external_effect_id
           );
END
$$;

-- The final scheduling leg is deliberately separate from reservation: it sees
-- only opaque identities and accepts a work item only after both governed
-- prerequisites are durably visible.
CREATE OR REPLACE FUNCTION vestrace_enqueue_run_step_after_input_ready(
    target_workspace_id UUID,
    target_run_id UUID,
    target_step_id UUID,
    target_attempt_id UUID,
    target_work_item_id UUID,
    target_expected_run_version BIGINT,
    target_idempotency_key TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    attempt run_step_execution_attempts%ROWTYPE;
BEGIN
    IF target_workspace_id IS NULL OR target_run_id IS NULL OR target_step_id IS NULL
       OR target_attempt_id IS NULL OR target_work_item_id IS NULL
       OR target_expected_run_version IS NULL OR target_expected_run_version < 0
       OR target_idempotency_key IS NULL OR length(btrim(target_idempotency_key)) = 0 THEN
        RAISE EXCEPTION 'governed Run-step enqueue arguments are malformed' USING ERRCODE = '22023';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
    SELECT * INTO attempt FROM run_step_execution_attempts
     WHERE id = target_attempt_id AND workspace_id = target_workspace_id
       AND run_id = target_run_id AND step_id = target_step_id FOR UPDATE;
    IF NOT FOUND
       OR NOT EXISTS (
           SELECT 1 FROM content_materials material
            JOIN material_key_creation_intents intent ON intent.id = material.intent_id
           WHERE material.workspace_id = target_workspace_id
             AND material.id = attempt.input_content_material_id
             AND material.intent_id = attempt.input_material_intent_id
             AND material.material_key_id = attempt.input_material_key_id
             AND material.state = 'live' AND intent.state = 'live'
             AND intent.owner_kind = 'model_request_input'
             AND intent.owner_id = target_step_id AND intent.output_ordinal = 0
       )
       OR NOT EXISTS (
           SELECT 1 FROM model_request_evidence_checks check_row
            JOIN model_request_evidence_nodes node
              ON node.evidence_root_id = check_row.evidence_root_id
           WHERE check_row.workspace_id = target_workspace_id
             AND check_row.evidence_root_id = attempt.model_request_evidence_id
             AND check_row.status = 'complete' AND check_row.missing_reference_count = 0
             AND node.reference_kind = 'governed_input_material'
             AND node.reference_id = attempt.input_content_material_id
       ) THEN
        RAISE EXCEPTION 'governed Run-step enqueue requires exact Live input material and Complete evidence'
            USING ERRCODE = '23514';
    END IF;
    INSERT INTO run_work_items (
        id, workspace_id, run_id, step_id, kind, status, expected_run_version,
        available_at, attempt, max_attempts, idempotency_key
    ) VALUES (
        target_work_item_id, target_workspace_id, target_run_id, target_step_id,
        'execute_step', 'ready', target_expected_run_version, NOW(), 1, 3, target_idempotency_key
    ) ON CONFLICT (workspace_id, idempotency_key) DO NOTHING;
END
$$;

-- Production upgrades run as the restricted runtime role and therefore use
-- the exact bootstrap allowlist extended for this migration.  SQLx fresh
-- databases are intentionally provisioned by a superuser and do not run the
-- Compose bootstrap first; retain that narrowly-scoped administrative fallback
-- here rather than rewriting migration 0176's historical fallback list.
DO $$
BEGIN
    PERFORM vestrace_assign_p03_table_owner('run_step_execution_attempts'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER TABLE run_step_execution_attempts OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON TABLE run_step_execution_attempts FROM PUBLIC;
    REVOKE ALL ON TABLE run_step_execution_attempts FROM vestrace;
    GRANT SELECT ON TABLE run_step_execution_attempts TO vestrace;
END
$$;

DO $$
BEGIN
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_reserve_run_step_execution_attempt(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_transition_run_step_execution_attempt(UUID, UUID, UUID, TEXT)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_lock_run_step_attempt_recovery_authority(UUID, UUID, UUID)'::REGPROCEDURE
    );
    PERFORM vestrace_assign_p03_function_owner(
        'vestrace_enqueue_run_step_after_input_ready(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT)'::REGPROCEDURE
    );
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER FUNCTION vestrace_reserve_run_step_execution_attempt(
        UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_transition_run_step_execution_attempt(
        UUID, UUID, UUID, TEXT
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_lock_run_step_attempt_recovery_authority(
        UUID, UUID, UUID
    ) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_enqueue_run_step_after_input_ready(
        UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT
    ) OWNER TO vestrace_guarded_owner;
END
$$;
REVOKE ALL ON FUNCTION vestrace_reserve_run_step_execution_attempt(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_transition_run_step_execution_attempt(UUID, UUID, UUID, TEXT) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_lock_run_step_attempt_recovery_authority(UUID, UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_enqueue_run_step_after_input_ready(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT) FROM PUBLIC;
GRANT SELECT ON run_step_execution_attempts TO vestrace;
-- The SECURITY DEFINER reserver reads the canonical P02 Run step only after
-- taking its transaction-scoped attempt key. The restricted runtime role
-- receives no direct Run-table authority.
--
-- The guarded owner receives exactly one Run write: INSERT on run_work_items,
-- which `vestrace_enqueue_run_step_after_input_ready` needs to schedule the
-- one deterministic ExecuteStep. It is the narrowest grant that lets the
-- guarded function be the only path to that row: the runtime role still holds
-- no direct authority over the table, and the function re-proves Live input
-- material and a Complete exact MRE before it inserts. The owner receives no
-- UPDATE or DELETE there, so it can create the item but never advance,
-- reschedule, or retire one.
GRANT SELECT ON run_steps TO vestrace_guarded_owner;
GRANT INSERT ON run_work_items TO vestrace_guarded_owner;
GRANT EXECUTE ON FUNCTION vestrace_reserve_run_step_execution_attempt(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_transition_run_step_execution_attempt(UUID, UUID, UUID, TEXT) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_lock_run_step_attempt_recovery_authority(UUID, UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_enqueue_run_step_after_input_ready(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT) TO vestrace;

-- Safe GET projections need stable catalog identity only to join it to the
-- current guarded head/qualification tuple.  The runtime may not read a
-- legacy wire name, cost profile, or provider display metadata, and receives
-- no catalog write privilege from this compatibility projection grant.
GRANT SELECT (id, workspace_id, provider_id, created_at) ON models TO vestrace;
GRANT SELECT (id, workspace_id) ON providers TO vestrace;
GRANT SELECT (id, workspace_id, created_at) ON connections TO vestrace;
