CREATE TABLE credential_activation_events (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    credential_slot_id UUID NOT NULL,
    credential_revision_id UUID NOT NULL,
    credential_intent_id UUID NOT NULL,
    connection_qualification_revision_id UUID,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('active', 'revoked')),
    expected_slot_version BIGINT NOT NULL CHECK (expected_slot_version >= 0),
    resulting_slot_version BIGINT NOT NULL CHECK (resulting_slot_version > expected_slot_version),
    audit_event_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_activation_events_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_activation_events_exact_slot_connection_fkey
        FOREIGN KEY (workspace_id, connection_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_activation_events_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_activation_events_exact_credential_fkey
        FOREIGN KEY (workspace_id, credential_slot_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, credential_slot_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_activation_events_exact_intent_fkey
        FOREIGN KEY (
            workspace_id, connection_id, credential_slot_id,
            credential_revision_id, credential_intent_id
        ) REFERENCES credential_key_creation_intents(
            workspace_id, connection_id, credential_slot_id,
            credential_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT credential_activation_events_qualification_fkey
        FOREIGN KEY (workspace_id, connection_qualification_revision_id)
        REFERENCES connection_qualification_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_activation_events_audit_fkey
        FOREIGN KEY (audit_event_id) REFERENCES audit_events(id)
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT credential_activation_events_revision_kind_key
        UNIQUE (credential_revision_id, event_kind),
    CONSTRAINT credential_activation_events_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE credential_rotation_events (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    credential_slot_id UUID NOT NULL,
    previous_credential_revision_id UUID NOT NULL,
    activated_credential_revision_id UUID NOT NULL,
    expected_slot_version BIGINT NOT NULL CHECK (expected_slot_version >= 1),
    resulting_slot_version BIGINT NOT NULL CHECK (resulting_slot_version > expected_slot_version),
    audit_event_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_rotation_events_distinct_revisions CHECK (
        previous_credential_revision_id <> activated_credential_revision_id
    ),
    CONSTRAINT credential_rotation_events_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_rotation_events_exact_slot_connection_fkey
        FOREIGN KEY (workspace_id, connection_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_rotation_events_previous_fkey
        FOREIGN KEY (workspace_id, previous_credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_rotation_events_exact_previous_fkey
        FOREIGN KEY (workspace_id, credential_slot_id, previous_credential_revision_id)
        REFERENCES credential_revisions(workspace_id, credential_slot_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_rotation_events_activated_fkey
        FOREIGN KEY (workspace_id, activated_credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_rotation_events_exact_activated_fkey
        FOREIGN KEY (workspace_id, credential_slot_id, activated_credential_revision_id)
        REFERENCES credential_revisions(workspace_id, credential_slot_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_rotation_events_audit_fkey
        FOREIGN KEY (audit_event_id) REFERENCES audit_events(id)
        DEFERRABLE INITIALLY DEFERRED,
    CONSTRAINT credential_rotation_events_one_activation UNIQUE (activated_credential_revision_id),
    CONSTRAINT credential_rotation_events_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TRIGGER credential_activation_events_immutable BEFORE INSERT OR UPDATE OR DELETE ON credential_activation_events
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER credential_rotation_events_immutable BEFORE INSERT OR UPDATE OR DELETE ON credential_rotation_events
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
ALTER TABLE credential_activation_events ENABLE ROW LEVEL SECURITY; ALTER TABLE credential_activation_events FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_rotation_events ENABLE ROW LEVEL SECURITY; ALTER TABLE credential_rotation_events FORCE ROW LEVEL SECURITY;
CREATE POLICY credential_activation_events_workspace_policy ON credential_activation_events USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_rotation_events_workspace_policy ON credential_rotation_events USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

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
        IF intent_row.state <> 'erasure_prepared'
           OR occupancy_row.state <> 'candidate'
           OR occupancy_row.association_version <> target_expected_association_version
           OR NOT EXISTS (
               SELECT 1 FROM credential_association_events
               WHERE intent_id = intent_row.id
                 AND occupancy_id = occupancy_row.id
                 AND event_kind = 'credential_association_cancelled'
                 AND resulting_version = target_expected_association_version
                 AND expected_version + 1 = resulting_version
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
            RAISE EXCEPTION 'Candidate erasure replay requires its exact cancellation evidence and current association version'
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

CREATE OR REPLACE FUNCTION vestrace_prepare_retired_or_revoked_credential_erasure(
    target_intent_id UUID
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
    preparation_row material_erasure_preparations%ROWTYPE;
    is_retired_or_revoked BOOLEAN;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO intent_row FROM credential_key_creation_intents
        WHERE id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_revisions WHERE id = intent_row.credential_revision_id FOR UPDATE;
    PERFORM vestrace_assert_material_erasure_workspace(intent_row.workspace_id);

    SELECT (
        EXISTS (
            SELECT 1 FROM credential_rotation_events
            WHERE workspace_id = intent_row.workspace_id
              AND previous_credential_revision_id = intent_row.credential_revision_id
        )
        OR EXISTS (
            SELECT 1 FROM credential_activation_events
            WHERE workspace_id = intent_row.workspace_id
              AND credential_revision_id = intent_row.credential_revision_id
              AND event_kind = 'revoked'
        )
    ) INTO is_retired_or_revoked;
    IF NOT is_retired_or_revoked OR EXISTS (
        SELECT 1 FROM credential_slots
        WHERE workspace_id = intent_row.workspace_id
          AND id = intent_row.credential_slot_id
          AND current_revision_id = intent_row.credential_revision_id
    ) THEN
        RAISE EXCEPTION 'only exact non-current Retired or Revoked credential material may prepare erasure'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO preparation_row FROM material_erasure_preparations
        WHERE credential_intent_id = intent_row.id FOR UPDATE;
    IF FOUND THEN
        IF intent_row.state <> 'erasure_prepared' THEN
            RAISE EXCEPTION 'ordinary credential erasure replay requires exact ErasurePrepared state'
                USING ERRCODE = '23514';
        END IF;
        preparation_id := preparation_row.id;
        material_key_id := preparation_row.material_key_id;
        finalized_erasure_receipt := preparation_row.erasure_receipt;
        RETURN NEXT;
        RETURN;
    END IF;
    IF vestrace_material_erasure_has_blocker('credential', NULL, intent_row.id) THEN
        RAISE EXCEPTION 'credential material erasure has a nonterminal blocker'
            USING ERRCODE = '23514';
    END IF;

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

-- This is the only creator for an empty slot. It moves a complete Candidate
-- association to Active and advances the slot pointer with one compare-and-set.
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
    IF NOT EXISTS (
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
    ) THEN
        RAISE EXCEPTION 'first activation requires the exact current connection qualification'
            USING ERRCODE = '23514';
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
    RETURN resulting_slot_version;
END
$$;

-- Rotation is one all-or-nothing pointer replacement: the old Active intent
-- becomes Retired only after the successor Candidate and exact qualification
-- have been proven under the same canonical locks.
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
        RAISE EXCEPTION 'rotation requires the exact current connection qualification'
            USING ERRCODE = '23514';
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
    RETURN resulting_slot_version;
END
$$;

-- Revocation is final for slot resolution but deliberately does not prepare
-- erasure. The retired state retains material for the ordinary erasure path.
CREATE OR REPLACE FUNCTION vestrace_revoke_credential(
    target_workspace_id UUID,
    target_connection_id UUID,
    target_execution_guard_id UUID,
    target_activation_guard_id UUID,
    target_slot_id UUID,
    target_credential_revision_id UUID,
    target_credential_intent_id UUID,
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
    resulting_slot_version BIGINT;
BEGIN
    IF target_workspace_id IS NULL OR target_connection_id IS NULL
       OR target_execution_guard_id IS NULL OR target_activation_guard_id IS NULL
       OR target_slot_id IS NULL OR target_credential_revision_id IS NULL
       OR target_credential_intent_id IS NULL OR target_audit_event_id IS NULL
       OR target_expected_slot_version IS NULL OR target_expected_slot_version < 1 THEN
        RAISE EXCEPTION 'credential revoke arguments are malformed'
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
        RAISE EXCEPTION 'credential revoke requires its exact canonical guards'
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
    IF slot_row.current_revision_id IS DISTINCT FROM target_credential_revision_id
       OR slot_row.tombstone_version IS NOT NULL THEN
        RAISE EXCEPTION 'revoke requires the exact current untombstoned credential revision'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO intent_row FROM credential_key_creation_intents
     WHERE id = target_credential_intent_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
       AND credential_revision_id = target_credential_revision_id
     FOR UPDATE;
    IF NOT FOUND OR intent_row.state <> 'active' THEN
        RAISE EXCEPTION 'revoke requires its exact Active credential intent'
            USING ERRCODE = '23514';
    END IF;
    UPDATE credential_slots
       SET current_revision_id = NULL,
           current_revision_version = current_revision_version + 1,
           tombstone_version = current_revision_version + 1,
           tombstoned_at = NOW(),
           updated_at = NOW()
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_slot_id
       AND current_revision_id = target_credential_revision_id
       AND current_revision_version = target_expected_slot_version
       AND tombstone_version IS NULL
     RETURNING current_revision_version INTO resulting_slot_version;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential slot version is stale' USING ERRCODE = '40001';
    END IF;
    INSERT INTO credential_activation_events (
        id, workspace_id, connection_id, credential_slot_id, credential_revision_id,
        credential_intent_id, event_kind, expected_slot_version, resulting_slot_version,
        audit_event_id
    ) VALUES (
        gen_random_uuid(), target_workspace_id, target_connection_id, target_slot_id,
        target_credential_revision_id, target_credential_intent_id, 'revoked',
        target_expected_slot_version, resulting_slot_version, target_audit_event_id
    );
    UPDATE credential_key_creation_intents
       SET state = 'retired', updated_at = NOW()
     WHERE id = intent_row.id;
    RETURN resulting_slot_version;
END
$$;

SELECT vestrace_assign_p03_table_owner('credential_activation_events'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('credential_rotation_events'::REGCLASS);
SELECT vestrace_assign_p03_function_owner('vestrace_prepare_candidate_abandon_and_erasure(UUID, BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_prepare_retired_or_revoked_credential_erasure(UUID)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_activate_first_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_rotate_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_revoke_credential(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, BIGINT)'::REGPROCEDURE);

DO $$
DECLARE
    caller_is_superuser BOOLEAN;
BEGIN
    IF to_regprocedure('public.vestrace_disable_superseded_p02_credential_erasure()') IS NOT NULL THEN
        PERFORM public.vestrace_disable_superseded_p02_credential_erasure();
    ELSE
        SELECT rolsuper INTO caller_is_superuser FROM pg_roles WHERE rolname = current_user;
        IF NOT caller_is_superuser THEN
            RAISE EXCEPTION 'credential-erasure supersession helper must be provisioned before runtime migration'
                USING ERRCODE = '42501';
        END IF;
        REVOKE EXECUTE ON FUNCTION public.vestrace_prepare_credential_material_erasure(UUID)
            FROM vestrace;
    END IF;
END
$$;
