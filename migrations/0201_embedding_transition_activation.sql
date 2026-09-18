-- P04 Task 7: atomic embedding transition activation and staged rotation.
--
-- Three inherited authorities advance a model qualification head:
-- vestrace_finalize_qualification_job, vestrace_activate_first_credential and
-- vestrace_rotate_credential.  Rather than fork all three, the invariant is
-- enforced where it actually lives: a guarded trigger on the head itself.
-- Once an embedding model revision has a head, that head may only move again
-- when this transaction has already written the exact activation receipt that
-- authorizes the move.  The receipt is a durable fact, not a session flag, so
-- no caller can assert its way past the guard.
--
-- The pre-existing blanket refusal in vestrace_rotate_credential is retained
-- deliberately.  It is not a defect: it is the gate that forces a credential
-- rotation touching embeddings through a proven transition, and
-- embedding_result_finalization.rs asserts it.  Activation is the one path
-- that may complete such a rotation.

DO $upgrade$
BEGIN
    IF to_regprocedure('public.vestrace_prepare_embedding_transition_activation_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_prepare_embedding_transition_activation_upgrade();
    ELSIF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
        RAISE EXCEPTION 'embedding transition activation upgrade must be provisioned'
            USING ERRCODE='42501';
    END IF;
END
$upgrade$;

-- An ordinary transition carries a qualification change alone.  A staged one
-- additionally carries the credential rotation that may only land with it.
ALTER TABLE embedding_transitions
    ADD COLUMN staging TEXT NOT NULL DEFAULT 'ordinary'
        CHECK(staging IN ('ordinary','rotation_staged'));

CREATE TABLE embedding_transition_activation_receipts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    transition_id UUID NOT NULL,
    transition_plan_id UUID NOT NULL,
    batch_id UUID NOT NULL,
    target_model_revision_id UUID NOT NULL,
    target_model_qualification_revision_id UUID NOT NULL,
    target_space_registration_id UUID NOT NULL,
    source_credential_revision_id UUID,
    target_credential_revision_id UUID,
    staging TEXT NOT NULL CHECK(staging IN ('ordinary','rotation_staged')),
    expected_qualification_head_version BIGINT NOT NULL
        CHECK(expected_qualification_head_version >= 0),
    resulting_qualification_head_version BIGINT NOT NULL
        CHECK(resulting_qualification_head_version >= 1),
    audit_event_id UUID NOT NULL REFERENCES audit_events(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT embedding_transition_activation_receipts_workspace_id_id_key
        UNIQUE (workspace_id, id),
    -- One transition activates at most once, for all time.
    CONSTRAINT embedding_transition_activation_receipts_transition_key
        UNIQUE (workspace_id, transition_id),
    CONSTRAINT embedding_transition_activation_receipts_transition_fkey
        FOREIGN KEY (workspace_id, transition_id)
        REFERENCES embedding_transitions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT embedding_transition_activation_receipts_space_fkey
        FOREIGN KEY (workspace_id, target_space_registration_id)
        REFERENCES embedding_space_registrations(workspace_id, id) ON DELETE RESTRICT,
    -- A rotation_staged activation names both ends of the credential lineage;
    -- an ordinary one names neither.
    CONSTRAINT embedding_transition_activation_receipts_rotation_xor CHECK(
        (staging = 'rotation_staged'
         AND source_credential_revision_id IS NOT NULL
         AND target_credential_revision_id IS NOT NULL
         AND source_credential_revision_id <> target_credential_revision_id)
        OR
        (staging = 'ordinary'
         AND source_credential_revision_id IS NULL
         AND target_credential_revision_id IS NULL)
    ),
    CONSTRAINT embedding_transition_activation_receipts_head_advances CHECK(
        resulting_qualification_head_version > expected_qualification_head_version
    )
);

ALTER TABLE embedding_transition_activation_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE embedding_transition_activation_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY embedding_transition_activation_receipts_workspace_policy
    ON embedding_transition_activation_receipts
    USING(workspace_id = NULLIF(current_setting('vestrace.workspace_id', TRUE), '')::UUID)
    WITH CHECK(workspace_id = NULLIF(current_setting('vestrace.workspace_id', TRUE), '')::UUID);

CREATE INDEX embedding_transition_activation_receipts_head_lookup
    ON embedding_transition_activation_receipts
    (workspace_id, target_model_revision_id, target_model_qualification_revision_id);

-- Receipts are immutable evidence.  Nothing rewrites or removes one.
CREATE OR REPLACE FUNCTION vestrace_guard_embedding_activation_receipt()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'embedding transition activation receipts are immutable'
        USING ERRCODE='23514';
END
$$;

CREATE TRIGGER embedding_transition_activation_receipts_immutable
    BEFORE UPDATE OR DELETE ON embedding_transition_activation_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_guard_embedding_activation_receipt();

CREATE TRIGGER embedding_transition_activation_receipts_guarded
    BEFORE INSERT ON embedding_transition_activation_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();

-- The invariant, enforced once for every path that can move a head.
--
-- A first embedding qualification is unconstrained: there is no live corpus to
-- carry, so there is nothing a transition could prove.  Every subsequent move
-- of that head must be authorized by an activation receipt written earlier in
-- the same transaction, naming the exact model revision, the exact incoming
-- qualification revision, and the exact version arithmetic being performed.
CREATE OR REPLACE FUNCTION vestrace_require_embedding_activation_receipt()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    revision_kind TEXT;
    previous_version BIGINT;
BEGIN
    SELECT kind INTO revision_kind FROM model_revisions
     WHERE workspace_id = NEW.workspace_id AND id = NEW.model_revision_id;
    IF revision_kind IS DISTINCT FROM 'embedding' THEN
        RETURN NEW;
    END IF;
    IF TG_OP = 'INSERT' THEN
        -- The first head for this embedding model revision. No live corpus
        -- precedes it, so no transition can exist to authorize it.
        RETURN NEW;
    END IF;
    IF NEW.current_qualification_revision_id IS NOT DISTINCT FROM OLD.current_qualification_revision_id
       AND NEW.active_space_registration_id IS NOT DISTINCT FROM OLD.active_space_registration_id THEN
        -- Nothing that requires a transition actually moved.
        RETURN NEW;
    END IF;
    IF OLD.active_space_registration_id IS NULL THEN
        -- This head does not yet point at a canonical corpus.  Nothing is live
        -- behind it, so moving it strands nothing and no transition could
        -- prove anything about it.  This covers both the initial active-space
        -- adoption and any qualification change made before a corpus exists.
        -- Once a head names an active space, every further move needs its
        -- receipt.
        RETURN NEW;
    END IF;
    previous_version := OLD.version;
    IF NOT EXISTS (
        SELECT 1 FROM embedding_transition_activation_receipts AS receipt
         WHERE receipt.workspace_id = NEW.workspace_id
           AND receipt.target_model_revision_id = NEW.model_revision_id
           AND receipt.target_model_qualification_revision_id = NEW.current_qualification_revision_id
           AND receipt.target_space_registration_id IS NOT DISTINCT FROM NEW.active_space_registration_id
           AND receipt.expected_qualification_head_version = previous_version
           AND receipt.resulting_qualification_head_version = NEW.version
    ) THEN
        RAISE EXCEPTION 'embedding qualification head advance requires its exact activation receipt'
            USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER model_qualification_heads_require_embedding_activation
    BEFORE INSERT OR UPDATE ON model_qualification_heads
    FOR EACH ROW EXECUTE FUNCTION vestrace_require_embedding_activation_receipt();

-- The one authority that may activate a proven transition.
--
-- Lock order follows the package invariant: connection execution guard, then
-- credential guards in canonical slot order, then source and target space
-- guards.  No provider lease is held; every fact is read from durable state.
CREATE OR REPLACE FUNCTION vestrace_activate_embedding_transition(
    target_workspace UUID,
    target_transition UUID,
    target_plan UUID,
    target_batch UUID,
    target_expected_transition_version BIGINT,
    target_expected_head_version BIGINT,
    target_audit_event UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    transition_row embedding_transitions%ROWTYPE;
    plan_row embedding_transition_plans%ROWTYPE;
    batch_state TEXT;
    guard_row embedding_index_generation_guards%ROWTYPE;
    generation_state TEXT;
    resulting_version BIGINT;
    receipt_id UUID;
    receipt_staging TEXT;
    source_credential UUID;
    target_credential UUID;
    slot_row credential_slots%ROWTYPE;
    unadopted BIGINT;
BEGIN
    IF target_workspace IS NULL OR target_transition IS NULL OR target_plan IS NULL
       OR target_batch IS NULL OR target_expected_transition_version IS NULL
       OR target_expected_head_version IS NULL OR target_audit_event IS NULL
       OR target_expected_transition_version < 1 OR target_expected_head_version < 0 THEN
        RAISE EXCEPTION 'embedding transition activation arguments are malformed'
            USING ERRCODE='22023';
    END IF;

    SELECT * INTO transition_row FROM embedding_transitions
     WHERE workspace_id = target_workspace AND id = target_transition FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition activation requires its exact transition'
            USING ERRCODE='23514';
    END IF;
    IF transition_row.state <> 'ready_to_activate' THEN
        RAISE EXCEPTION 'embedding transition activation requires a proven ready transition'
            USING ERRCODE='23514';
    END IF;
    IF transition_row.version <> target_expected_transition_version THEN
        RAISE EXCEPTION 'embedding transition version is stale' USING ERRCODE='40001';
    END IF;

    SELECT * INTO plan_row FROM embedding_transition_plans
     WHERE workspace_id = target_workspace AND id = target_plan
       AND transition_id = target_transition;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition activation requires its exact plan'
            USING ERRCODE='23514';
    END IF;
    IF plan_row.transition_batch_id <> target_batch THEN
        RAISE EXCEPTION 'embedding transition activation requires the exact planned batch'
            USING ERRCODE='23514';
    END IF;
    IF plan_row.target_space_registration_id <> transition_row.target_space_registration_id THEN
        RAISE EXCEPTION 'embedding transition activation requires one exact target space'
            USING ERRCODE='23514';
    END IF;

    SELECT state INTO batch_state FROM embedding_transition_batches
     WHERE workspace_id = target_workspace AND id = target_batch
       AND transition_id = target_transition AND transition_plan_id = target_plan
     FOR UPDATE;
    IF NOT FOUND OR batch_state <> 'ready_to_activate' THEN
        RAISE EXCEPTION 'embedding transition activation requires its complete batch'
            USING ERRCODE='23514';
    END IF;
    -- Re-prove the bijection under this transaction's locks rather than
    -- trusting the state flag alone.
    PERFORM vestrace_validate_embedding_transition_bijection(target_workspace, target_batch, TRUE);

    -- The package lock order names the ConnectionExecutionGuard, not the
    -- unguarded connections row: that is the guarded authority every other
    -- P03/P04 path serialises on, and the one this owner may lock.
    PERFORM 1 FROM connection_execution_guards
     WHERE workspace_id = target_workspace AND connection_id = plan_row.target_connection_id
     FOR UPDATE;

    -- The target qualification must still be valid at activation time.  A
    -- transition may have been proven long before anyone asked to activate it.
    IF NOT EXISTS (
        SELECT 1 FROM model_qualification_revisions
         WHERE workspace_id = target_workspace
           AND id = plan_row.target_model_qualification_revision_id
           AND model_revision_id = plan_row.target_model_revision_id
           AND valid_until > NOW()
    ) THEN
        RAISE EXCEPTION 'embedding transition activation requires an unexpired target qualification'
            USING ERRCODE='23514';
    END IF;

    IF plan_row.target_branch = 'credential' THEN
        IF plan_row.target_credential_revision_id IS NULL
           OR plan_row.target_credential_slot_id IS NULL
           OR plan_row.target_expected_slot_version IS NULL THEN
            RAISE EXCEPTION 'embedding transition activation requires a complete credential target'
                USING ERRCODE='23514';
        END IF;
        SELECT * INTO slot_row FROM credential_slots
         WHERE workspace_id = target_workspace AND id = plan_row.target_credential_slot_id
           AND connection_id = plan_row.target_connection_id
         FOR UPDATE;
        IF NOT FOUND OR slot_row.tombstone_version IS NOT NULL THEN
            RAISE EXCEPTION 'embedding transition activation requires an untombstoned credential slot'
                USING ERRCODE='23514';
        END IF;
        IF slot_row.current_revision_version <> plan_row.target_expected_slot_version THEN
            RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE='40001';
        END IF;
        -- An erasure-prepared candidate can never become active.
        IF EXISTS (
            SELECT 1 FROM credential_key_creation_intents
             WHERE workspace_id = target_workspace
               AND credential_revision_id = plan_row.target_credential_revision_id
               AND state <> 'candidate'
        ) THEN
            RAISE EXCEPTION 'embedding transition activation requires an exact Candidate credential target'
                USING ERRCODE='23514';
        END IF;
    END IF;

    IF plan_row.source_branch = 'credential' AND plan_row.target_branch = 'credential' THEN
        receipt_staging := 'rotation_staged';
        source_credential := plan_row.source_credential_revision_id;
        target_credential := plan_row.target_credential_revision_id;
        IF source_credential IS NULL OR target_credential IS NULL
           OR source_credential = target_credential THEN
            RAISE EXCEPTION 'staged rotation requires two distinct credential revisions'
                USING ERRCODE='23514';
        END IF;
    ELSE
        receipt_staging := 'ordinary';
        source_credential := NULL;
        target_credential := NULL;
    END IF;

    -- Source then target space guard, in that order.
    IF plan_row.target_space_registration_id IS DISTINCT FROM transition_row.target_space_registration_id THEN
        RAISE EXCEPTION 'embedding transition activation requires one exact target space'
            USING ERRCODE='23514';
    END IF;
    -- The head's deferred consistency trigger will demand exactly this at
    -- commit time.  Checking it here turns a late, opaque commit failure into
    -- an exact refusal naming the tuple that does not line up.
    IF NOT EXISTS (
        SELECT 1 FROM embedding_space_registrations
         WHERE workspace_id = target_workspace
           AND id = plan_row.target_space_registration_id
           AND registration_kind = 'canonical'
           AND model_revision_id = plan_row.target_model_revision_id
           AND model_qualification_revision_id = plan_row.target_model_qualification_revision_id
    ) THEN
        RAISE EXCEPTION 'embedding transition activation requires a canonical target space bound to its exact qualification'
            USING ERRCODE='23514';
    END IF;

    SELECT * INTO guard_row FROM embedding_index_generation_guards
     WHERE workspace_id = target_workspace
       AND space_registration_id = plan_row.target_space_registration_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding transition activation requires its target generation guard'
            USING ERRCODE='23514';
    END IF;
    IF guard_row.current_generation_id IS NULL THEN
        RAISE EXCEPTION 'embedding transition activation requires an exact Ready current generation'
            USING ERRCODE='23514';
    END IF;
    SELECT state INTO generation_state FROM embedding_corpus_generations
     WHERE workspace_id = target_workspace AND id = guard_row.current_generation_id
     FOR SHARE;
    IF NOT FOUND OR generation_state <> 'ready' THEN
        RAISE EXCEPTION 'embedding transition activation requires an exact Ready current generation'
            USING ERRCODE='23514';
    END IF;

    -- Every satisfying result must have adopted its completion blocker.
    SELECT COUNT(*) INTO unadopted
      FROM embedding_transition_recipe_satisfactions AS satisfaction
      JOIN embedding_job_result_preparations AS preparation
        ON preparation.workspace_id = satisfaction.workspace_id
       AND preparation.job_id = satisfaction.terminal_job_id
     WHERE satisfaction.workspace_id = target_workspace
       AND satisfaction.batch_id = target_batch
       AND satisfaction.satisfaction_kind = 'satisfied_result'
       AND NOT EXISTS (
           SELECT 1 FROM embedding_result_credential_blocker_adoptions AS adoption
            WHERE adoption.workspace_id = preparation.workspace_id
              AND adoption.preparation_id = preparation.id
       );
    IF unadopted > 0 THEN
        RAISE EXCEPTION 'embedding transition activation requires complete completion-blocker adoption'
            USING ERRCODE='23514';
    END IF;

    receipt_id := gen_random_uuid();
    resulting_version := target_expected_head_version + 1;

    -- The receipt is written first: the head guard reads it as the authority
    -- for the move that follows, in this same transaction.
    INSERT INTO embedding_transition_activation_receipts(
        id, workspace_id, transition_id, transition_plan_id, batch_id,
        target_model_revision_id, target_model_qualification_revision_id,
        target_space_registration_id, source_credential_revision_id,
        target_credential_revision_id, staging,
        expected_qualification_head_version, resulting_qualification_head_version,
        audit_event_id
    ) VALUES (
        receipt_id, target_workspace, target_transition, target_plan, target_batch,
        plan_row.target_model_revision_id, plan_row.target_model_qualification_revision_id,
        plan_row.target_space_registration_id, source_credential, target_credential,
        receipt_staging, target_expected_head_version, resulting_version,
        target_audit_event
    );

    UPDATE model_qualification_heads
       SET current_qualification_revision_id = plan_row.target_model_qualification_revision_id,
           active_space_registration_id = plan_row.target_space_registration_id,
           version = resulting_version,
           updated_at = NOW()
     WHERE workspace_id = target_workspace
       AND model_revision_id = plan_row.target_model_revision_id
       AND version = target_expected_head_version;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'embedding qualification head version is stale' USING ERRCODE='40001';
    END IF;

    IF receipt_staging = 'rotation_staged' THEN
        UPDATE credential_slots
           SET current_revision_id = target_credential,
               current_revision_version = current_revision_version + 1,
               updated_at = NOW()
         WHERE workspace_id = target_workspace
           AND connection_id = plan_row.target_connection_id
           AND id = plan_row.target_credential_slot_id
           AND current_revision_id = source_credential
           AND current_revision_version = plan_row.target_expected_slot_version
           AND tombstone_version IS NULL;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE='40001';
        END IF;
        UPDATE credential_key_creation_intents SET state = 'retired', updated_at = NOW()
         WHERE workspace_id = target_workspace
           AND credential_revision_id = source_credential AND state = 'active';
        UPDATE credential_key_creation_intents SET state = 'active', updated_at = NOW()
         WHERE workspace_id = target_workspace
           AND credential_revision_id = target_credential AND state = 'candidate';
    END IF;

    UPDATE embedding_transitions
       SET state = 'activated', version = version + 1
     WHERE workspace_id = target_workspace AND id = target_transition;

    INSERT INTO embedding_transition_observations(
        id, workspace_id, transition_id, transition_plan_id, batch_id,
        attempt_id, observation_kind
    ) VALUES (
        gen_random_uuid(), target_workspace, target_transition, target_plan,
        target_batch, NULL, 'ready_to_activate'
    );

    RETURN receipt_id;
END
$$;

DO $upgrade$
DECLARE target_function REGPROCEDURE;
BEGIN
    IF to_regprocedure('public.vestrace_finish_embedding_transition_activation_upgrade()') IS NOT NULL THEN
        PERFORM public.vestrace_finish_embedding_transition_activation_upgrade();
    ELSE
        IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user), FALSE) THEN
            RAISE EXCEPTION 'embedding transition activation ownership hand-back is unavailable'
                USING ERRCODE='42501';
        END IF;
        ALTER TABLE embedding_transition_activation_receipts OWNER TO vestrace_guarded_owner;
        GRANT ALL ON TABLE embedding_transition_activation_receipts TO vestrace_guarded_owner;
        REVOKE ALL ON TABLE embedding_transition_activation_receipts FROM PUBLIC, vestrace;
        GRANT SELECT, REFERENCES ON TABLE embedding_transition_activation_receipts TO vestrace;
        FOREACH target_function IN ARRAY ARRAY[
            'vestrace_guard_embedding_activation_receipt()'::REGPROCEDURE,
            'vestrace_require_embedding_activation_receipt()'::REGPROCEDURE,
            'vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid)'::REGPROCEDURE
        ] LOOP
            EXECUTE format('ALTER FUNCTION %s OWNER TO vestrace_guarded_owner', target_function);
            EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC,vestrace', target_function);
        END LOOP;
        GRANT EXECUTE ON FUNCTION
            vestrace_activate_embedding_transition(uuid,uuid,uuid,uuid,bigint,bigint,uuid) TO vestrace;
    END IF;
END
$upgrade$;
