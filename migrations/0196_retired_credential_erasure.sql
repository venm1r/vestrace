-- Forward repair for the existing ordinary retired/revoked credential route.
-- Only the two0174 functions change; Candidate/content and completed replay stay.
DO $$ BEGIN
    PERFORM vestrace_prepare_retired_credential_erasure_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
        RAISE EXCEPTION 'credential erasure upgrade must be provisioned' USING ERRCODE='42501';
    END IF;
END $$;

CREATE OR REPLACE FUNCTION vestrace_finalize_credential_material_erasure(
    target_preparation_id UUID,
    target_erasure_receipt UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preparation_row material_erasure_preparations%ROWTYPE;
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
    recorded_audit_event_id UUID;
BEGIN
    SELECT * INTO preparation_row
      FROM material_erasure_preparations
     WHERE id = target_preparation_id;
    IF NOT FOUND OR preparation_row.target_kind <> 'credential' THEN
        RAISE EXCEPTION 'credential material erasure preparation is absent' USING ERRCODE = '23514';
    END IF;
    SELECT * INTO intent_row FROM credential_key_creation_intents
     WHERE id = preparation_row.credential_intent_id;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO preparation_row FROM material_erasure_preparations
     WHERE id = target_preparation_id FOR UPDATE;
    SELECT * INTO occupancy_row FROM credential_guard_occupancies WHERE id = intent_row.occupancy_id FOR UPDATE;
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = intent_row.id FOR UPDATE;
    PERFORM 1 FROM credential_revisions WHERE id = intent_row.credential_revision_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_materials WHERE intent_id = intent_row.id FOR UPDATE;
    PERFORM vestrace_assert_material_erasure_workspace(intent_row.workspace_id);
    IF preparation_row.erasure_receipt IS NOT NULL THEN
        IF preparation_row.erasure_receipt <> target_erasure_receipt THEN
            RAISE EXCEPTION 'credential material erasure already has a different receipt'
                USING ERRCODE = '23505';
        END IF;
        RETURN preparation_row.erasure_receipt;
    END IF;
    IF preparation_row.fence_receipt IS NULL
       OR intent_row.state <> 'erasure_prepared'
       -- Candidate remains the original branch. Activated occupancy needs
       -- exact immutable non-current Retired/Revoked evidence.
       OR (occupancy_row.state = 'candidate' OR EXISTS (
               SELECT 1
                 FROM credential_key_creation_intents AS exact_intent
                 JOIN credential_guard_occupancies AS exact_occupancy
                   ON exact_occupancy.id = exact_intent.occupancy_id
                  AND exact_occupancy.intent_id = exact_intent.id
                  AND exact_occupancy.workspace_id = exact_intent.workspace_id
                  AND exact_occupancy.connection_id = exact_intent.connection_id
                  AND exact_occupancy.credential_slot_id = exact_intent.credential_slot_id
                 JOIN credential_slots AS exact_slot
                   ON exact_slot.workspace_id = exact_intent.workspace_id
                  AND exact_slot.connection_id = exact_intent.connection_id
                  AND exact_slot.id = exact_intent.credential_slot_id
                 JOIN credential_revisions AS exact_revision
                   ON exact_revision.workspace_id = exact_intent.workspace_id
                  AND exact_revision.credential_slot_id = exact_intent.credential_slot_id
                  AND exact_revision.id = exact_intent.credential_revision_id
                  AND exact_revision.material_key_id = exact_intent.material_key_id
                WHERE exact_intent.id = preparation_row.credential_intent_id
                  AND exact_intent.workspace_id = preparation_row.workspace_id
                  AND exact_intent.material_key_id = preparation_row.material_key_id
                  AND exact_occupancy.state = 'activated'
                  AND exact_slot.current_revision_id IS DISTINCT FROM exact_intent.credential_revision_id
                  AND (
                      EXISTS (
                          SELECT 1 FROM credential_activation_events AS revoked
                           WHERE revoked.workspace_id = exact_intent.workspace_id
                             AND revoked.connection_id = exact_intent.connection_id
                             AND revoked.credential_slot_id = exact_intent.credential_slot_id
                             AND revoked.credential_revision_id = exact_intent.credential_revision_id
                             AND revoked.credential_intent_id = exact_intent.id
                             AND revoked.event_kind = 'revoked'
                      )
                      OR EXISTS (
                          SELECT 1 FROM credential_rotation_events AS retired
                           WHERE retired.workspace_id = exact_intent.workspace_id
                             AND retired.connection_id = exact_intent.connection_id
                             AND retired.credential_slot_id = exact_intent.credential_slot_id
                             AND retired.previous_credential_revision_id = exact_intent.credential_revision_id
                      )
                  )
           )) IS NOT TRUE
       OR NOT EXISTS (SELECT 1 FROM credential_prepared_materials WHERE intent_id = intent_row.id)
       OR NOT EXISTS (
           SELECT 1 FROM credential_lifecycle_events
            WHERE intent_id = intent_row.id AND event_kind = 'candidate'
       )
       OR NOT EXISTS (
           SELECT 1 FROM credential_lifecycle_events
            WHERE intent_id = intent_row.id AND event_kind = 'erasure_prepared'
       )
       OR NOT EXISTS (
           SELECT 1 FROM material_erasure_events
            WHERE preparation_id = preparation_row.id AND event_kind = 'erasure_prepared'
       ) THEN
        RAISE EXCEPTION 'credential material erasure requires its exact fenced preparation'
            USING ERRCODE = '23514';
    END IF;

    DELETE FROM credential_prepared_materials WHERE intent_id = intent_row.id;
    UPDATE credential_guard_occupancies
       SET state = 'destroyed', association_version = association_version + 1, updated_at = NOW()
     WHERE id = occupancy_row.id;
    UPDATE credential_key_creation_intents
       SET state = 'destroyed', updated_at = NOW()
     WHERE id = intent_row.id;
    UPDATE material_erasure_preparations
       SET erasure_receipt = target_erasure_receipt, finalized_at = NOW()
     WHERE id = preparation_row.id;
    INSERT INTO credential_lifecycle_events (
        id, workspace_id, intent_id, credential_revision_id, event_kind
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, intent_row.id, intent_row.credential_revision_id,
        'destroyed'
    );
    INSERT INTO material_erasure_events (id, workspace_id, preparation_id, event_kind)
    VALUES (gen_random_uuid(), preparation_row.workspace_id, preparation_row.id, 'destroyed');
    recorded_audit_event_id := vestrace_record_material_erasure_audit(
        preparation_row.workspace_id,
        'material.credential.destroyed',
        'credential_revision',
        intent_row.credential_revision_id,
        preparation_row.material_key_id,
        target_erasure_receipt,
        jsonb_build_object('credential_slot_id', intent_row.credential_slot_id)
    );
    INSERT INTO material_erasure_audit_tombstones (
        id, workspace_id, preparation_id, target_kind, target_id, erasure_receipt,
        audit_event_id
    ) VALUES (
        gen_random_uuid(), preparation_row.workspace_id, preparation_row.id, 'credential',
        intent_row.credential_revision_id, target_erasure_receipt, recorded_audit_event_id
    );
    RETURN target_erasure_receipt;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_material_erasure()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preparation_id UUID;
    preparation_row material_erasure_preparations%ROWTYPE;
    material_state TEXT;
    intent_state TEXT;
    occupancy_state TEXT;
    byte_count BIGINT;
    prepared_event_count BIGINT;
    terminal_event_count BIGINT;
    audit_count BIGINT;
    credential_ciphertext_count BIGINT;
BEGIN
    IF TG_TABLE_NAME = 'material_erasure_preparations' THEN
        preparation_id := COALESCE(NEW.id, OLD.id);
    ELSE
        preparation_id := COALESCE(NEW.preparation_id, OLD.preparation_id);
    END IF;
    SELECT * INTO preparation_row FROM material_erasure_preparations WHERE id = preparation_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    SELECT COUNT(*) INTO prepared_event_count FROM material_erasure_events AS event
     WHERE event.preparation_id = preparation_row.id AND event.event_kind = 'erasure_prepared';
    SELECT COUNT(*) INTO audit_count FROM material_erasure_audit_tombstones AS tombstone
     WHERE tombstone.preparation_id = preparation_row.id;
    IF preparation_row.target_kind = 'content' THEN
        SELECT state INTO material_state FROM content_materials WHERE id = preparation_row.content_material_id;
        SELECT intent.state INTO intent_state
          FROM material_key_creation_intents AS intent
          JOIN content_materials AS material ON material.intent_id = intent.id
         WHERE material.id = preparation_row.content_material_id;
        SELECT COUNT(*) INTO byte_count FROM content_material_bytes WHERE material_id = preparation_row.content_material_id;
        SELECT COUNT(*) INTO terminal_event_count FROM material_erasure_events AS event
         WHERE event.preparation_id = preparation_row.id AND event.event_kind = 'tombstoned';
        IF preparation_row.erasure_receipt IS NULL
           AND (material_state <> 'erasure_prepared' OR intent_state <> 'erasure_prepared'
                OR byte_count <> 1 OR prepared_event_count <> 1 OR terminal_event_count <> 0
                OR audit_count <> 0) THEN
            RAISE EXCEPTION 'content erasure preparation is inconsistent' USING ERRCODE = '23514';
        END IF;
        IF preparation_row.erasure_receipt IS NOT NULL
           AND (preparation_row.fence_receipt IS NULL OR material_state <> 'tombstoned'
                OR intent_state <> 'tombstoned' OR byte_count <> 0
                OR prepared_event_count <> 1 OR terminal_event_count <> 1 OR audit_count <> 1) THEN
            RAISE EXCEPTION 'content tombstone requires fenced ciphertext removal and audit evidence'
                USING ERRCODE = '23514';
        END IF;
    ELSE
        SELECT state INTO intent_state FROM credential_key_creation_intents
         WHERE id = preparation_row.credential_intent_id;
        SELECT occupancy.state INTO occupancy_state
          FROM credential_guard_occupancies AS occupancy
          JOIN credential_key_creation_intents AS intent ON intent.occupancy_id = occupancy.id
         WHERE intent.id = preparation_row.credential_intent_id;
        SELECT COUNT(*) INTO credential_ciphertext_count FROM credential_prepared_materials
         WHERE intent_id = preparation_row.credential_intent_id;
        SELECT COUNT(*) INTO terminal_event_count FROM material_erasure_events AS event
         WHERE event.preparation_id = preparation_row.id AND event.event_kind = 'destroyed';
        IF preparation_row.erasure_receipt IS NULL
           AND (intent_state <> 'erasure_prepared' OR (occupancy_state = 'candidate' OR EXISTS (
               SELECT 1
                 FROM credential_key_creation_intents AS exact_intent
                 JOIN credential_guard_occupancies AS exact_occupancy
                   ON exact_occupancy.id = exact_intent.occupancy_id
                  AND exact_occupancy.intent_id = exact_intent.id
                  AND exact_occupancy.workspace_id = exact_intent.workspace_id
                  AND exact_occupancy.connection_id = exact_intent.connection_id
                  AND exact_occupancy.credential_slot_id = exact_intent.credential_slot_id
                 JOIN credential_slots AS exact_slot
                   ON exact_slot.workspace_id = exact_intent.workspace_id
                  AND exact_slot.connection_id = exact_intent.connection_id
                  AND exact_slot.id = exact_intent.credential_slot_id
                 JOIN credential_revisions AS exact_revision
                   ON exact_revision.workspace_id = exact_intent.workspace_id
                  AND exact_revision.credential_slot_id = exact_intent.credential_slot_id
                  AND exact_revision.id = exact_intent.credential_revision_id
                  AND exact_revision.material_key_id = exact_intent.material_key_id
                WHERE exact_intent.id = preparation_row.credential_intent_id
                  AND exact_intent.workspace_id = preparation_row.workspace_id
                  AND exact_intent.material_key_id = preparation_row.material_key_id
                  AND exact_occupancy.state = 'activated'
                  AND exact_slot.current_revision_id IS DISTINCT FROM exact_intent.credential_revision_id
                  AND (
                      EXISTS (
                          SELECT 1 FROM credential_activation_events AS revoked
                           WHERE revoked.workspace_id = exact_intent.workspace_id
                             AND revoked.connection_id = exact_intent.connection_id
                             AND revoked.credential_slot_id = exact_intent.credential_slot_id
                             AND revoked.credential_revision_id = exact_intent.credential_revision_id
                             AND revoked.credential_intent_id = exact_intent.id
                             AND revoked.event_kind = 'revoked'
                      )
                      OR EXISTS (
                          SELECT 1 FROM credential_rotation_events AS retired
                           WHERE retired.workspace_id = exact_intent.workspace_id
                             AND retired.connection_id = exact_intent.connection_id
                             AND retired.credential_slot_id = exact_intent.credential_slot_id
                             AND retired.previous_credential_revision_id = exact_intent.credential_revision_id
                      )
                  )
           )) IS NOT TRUE
                OR credential_ciphertext_count <> 1 OR prepared_event_count <> 1
                OR terminal_event_count <> 0 OR audit_count <> 0) THEN
            RAISE EXCEPTION 'credential erasure preparation is inconsistent' USING ERRCODE = '23514';
        END IF;
        IF preparation_row.erasure_receipt IS NOT NULL
           AND (preparation_row.fence_receipt IS NULL OR intent_state <> 'destroyed'
                OR occupancy_state <> 'destroyed' OR credential_ciphertext_count <> 0
                OR prepared_event_count <> 1 OR terminal_event_count <> 1 OR audit_count <> 1) THEN
            RAISE EXCEPTION 'credential destruction requires fenced ciphertext removal and audit evidence'
                USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NULL;
END
$$;

DO $$ BEGIN
    PERFORM vestrace_finish_retired_credential_erasure_upgrade();
EXCEPTION WHEN undefined_function THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname=current_user),false) THEN
        RAISE EXCEPTION 'credential erasure finish must be provisioned' USING ERRCODE='42501';
    END IF;
    ALTER FUNCTION vestrace_validate_material_erasure() OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_finalize_credential_material_erasure(UUID,UUID) OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON FUNCTION vestrace_validate_material_erasure() FROM PUBLIC,vestrace;
    REVOKE ALL ON FUNCTION vestrace_finalize_credential_material_erasure(UUID,UUID) FROM PUBLIC,vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_finalize_credential_material_erasure(UUID,UUID) TO vestrace;
END $$;
