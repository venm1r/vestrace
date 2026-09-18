-- Erasure is evidence-preserving but one way. Phase one records an immutable
-- preparation and commits before the host vault is touched; phase two accepts
-- only the witnessed vault receipt and removes ciphertext, never history.

ALTER TABLE material_key_creation_intents
    DROP CONSTRAINT material_key_creation_intents_state_check,
    ADD CONSTRAINT material_key_creation_intents_state_check CHECK (state IN (
        'reserved',
        'provisional_created',
        'provisional_receipted',
        'content_prepared',
        'result_prepared',
        'content_abandon_prepared',
        -- The pre-prepared abort branch, taken before any ContentPrepared,
        -- ResultPrepared or Bound marker exists. Distinct from the ordinary
        -- ContentPrepared abort by spec, which refuses to accept a
        -- ContentPrepared-only marker in its place.
        'pre_prepared_abandon_prepared',
        'bound',
        'live',
        'erasure_prepared',
        'tombstoned',
        'abandoned'
    ));

ALTER TABLE content_materials
    DROP CONSTRAINT content_materials_state_check,
    ADD CONSTRAINT content_materials_state_check CHECK (state IN (
        'prepared', 'abandon_prepared', 'live', 'erasure_prepared', 'tombstoned', 'abandoned'
    ));

ALTER TABLE credential_key_creation_intents
    DROP CONSTRAINT credential_key_creation_intents_state_check,
    ADD CONSTRAINT credential_key_creation_intents_state_check CHECK (state IN (
        'reserved',
        'provisional_created',
        'provisional_receipted',
        'credential_prepared',
        'credential_abandon_prepared',
        'bound',
        'candidate',
        'active',
        'retired',
        'erasure_prepared',
        'destroyed',
        'abandoned'
    ));

ALTER TABLE credential_lifecycle_events
    DROP CONSTRAINT credential_lifecycle_events_event_kind_check,
    DROP CONSTRAINT credential_lifecycle_events_one_candidate,
    ADD CONSTRAINT credential_lifecycle_events_event_kind_check CHECK (
        event_kind IN ('candidate', 'erasure_prepared', 'destroyed')
    ),
    ADD CONSTRAINT credential_lifecycle_events_one_event_per_kind UNIQUE (intent_id, event_kind);

CREATE TABLE material_erasure_preparations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('content', 'credential')),
    content_material_id UUID,
    credential_intent_id UUID,
    material_key_id UUID NOT NULL,
    fence_receipt UUID UNIQUE,
    erasure_receipt UUID UNIQUE,
    prepared_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fenced_at TIMESTAMPTZ,
    finalized_at TIMESTAMPTZ,
    UNIQUE (id, workspace_id),
    CONSTRAINT material_erasure_preparations_exact_target CHECK (
        (target_kind = 'content' AND content_material_id IS NOT NULL AND credential_intent_id IS NULL)
        OR (target_kind = 'credential' AND credential_intent_id IS NOT NULL AND content_material_id IS NULL)
    ),
    CONSTRAINT material_erasure_preparations_content_fkey
        FOREIGN KEY (content_material_id, workspace_id)
        REFERENCES content_materials(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT material_erasure_preparations_credential_fkey
        FOREIGN KEY (credential_intent_id, workspace_id)
        REFERENCES credential_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT material_erasure_preparations_one_content UNIQUE (content_material_id),
    CONSTRAINT material_erasure_preparations_one_credential UNIQUE (credential_intent_id)
);

CREATE TABLE material_erasure_events (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    preparation_id UUID NOT NULL,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('erasure_prepared', 'tombstoned', 'destroyed')),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_erasure_events_preparation_fkey
        FOREIGN KEY (preparation_id, workspace_id)
        REFERENCES material_erasure_preparations(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT material_erasure_events_one_kind UNIQUE (preparation_id, event_kind)
);

CREATE TABLE material_erasure_audit_tombstones (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    preparation_id UUID NOT NULL UNIQUE,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('content', 'credential')),
    target_id UUID NOT NULL,
    erasure_receipt UUID NOT NULL UNIQUE,
    audit_event_id UUID NOT NULL UNIQUE,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_erasure_audit_tombstones_preparation_fkey
        FOREIGN KEY (preparation_id, workspace_id)
        REFERENCES material_erasure_preparations(id, workspace_id)
        ON DELETE RESTRICT,
    -- The tombstone is evidence about an Audit entry, never a second audit
    -- authority beside audit_events. The deferred reference follows the
    -- governed_mutation_audit_marks precedent in 0165: an erasure that
    -- committed without its Audit entry is refused at commit time.
    CONSTRAINT material_erasure_audit_tombstones_audit_event_fkey
        FOREIGN KEY (audit_event_id)
        REFERENCES audit_events(id)
        DEFERRABLE INITIALLY DEFERRED
);

-- P02 records the three durable blocker classes. Later packages bind their
-- concrete effects, leases, and intents to these exact target identities.
CREATE TABLE material_erasure_blockers (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('content', 'credential')),
    content_material_id UUID,
    credential_intent_id UUID,
    blocker_kind TEXT NOT NULL CHECK (blocker_kind IN ('effect', 'lease', 'intent')),
    state TEXT NOT NULL CHECK (state IN ('nonterminal', 'terminal')),
    usable_until TIMESTAMPTZ,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_erasure_blockers_exact_target CHECK (
        (target_kind = 'content' AND content_material_id IS NOT NULL AND credential_intent_id IS NULL)
        OR (target_kind = 'credential' AND credential_intent_id IS NOT NULL AND content_material_id IS NULL)
    ),
    CONSTRAINT material_erasure_blockers_lease_expiry CHECK (
        (blocker_kind = 'lease' AND usable_until IS NOT NULL)
        OR (blocker_kind <> 'lease' AND usable_until IS NULL)
    ),
    CONSTRAINT material_erasure_blockers_content_fkey
        FOREIGN KEY (content_material_id, workspace_id)
        REFERENCES content_materials(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT material_erasure_blockers_credential_fkey
        FOREIGN KEY (credential_intent_id, workspace_id)
        REFERENCES credential_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT
);

ALTER TABLE material_erasure_preparations ENABLE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_preparations FORCE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_events FORCE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_audit_tombstones ENABLE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_audit_tombstones FORCE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_blockers ENABLE ROW LEVEL SECURITY;
ALTER TABLE material_erasure_blockers FORCE ROW LEVEL SECURITY;

CREATE POLICY material_erasure_preparations_workspace_policy ON material_erasure_preparations
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY material_erasure_events_workspace_policy ON material_erasure_events
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY material_erasure_audit_tombstones_workspace_policy ON material_erasure_audit_tombstones
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY material_erasure_blockers_workspace_policy ON material_erasure_blockers
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);


CREATE OR REPLACE FUNCTION vestrace_reject_raw_material_erasure_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' THEN
        RAISE EXCEPTION 'material erasure state changes require a guarded operation'
            USING ERRCODE = '42501';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER material_erasure_preparations_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON material_erasure_preparations
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_erasure_mutation();
CREATE TRIGGER material_erasure_events_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON material_erasure_events
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_erasure_mutation();
CREATE TRIGGER material_erasure_audit_tombstones_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON material_erasure_audit_tombstones
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_erasure_mutation();
CREATE TRIGGER material_erasure_blockers_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON material_erasure_blockers
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_erasure_mutation();

-- One-wayness is structural rather than an absence-of-code argument. These
-- triggers refuse a reversal for every role, the guarded owner included, so a
-- guarded operation added by a later package cannot walk an erased identity
-- back to a usable state. Their names sort after the reject_raw triggers on the
-- same tables, so an unprivileged caller still observes 42501 first.
CREATE OR REPLACE FUNCTION vestrace_reject_content_material_state_reversal()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.state IN ('erasure_prepared', 'tombstoned') THEN
            RAISE EXCEPTION 'an erasure-prepared or tombstoned content material cannot be deleted'
                USING ERRCODE = '23514';
        END IF;
        RETURN OLD;
    END IF;
    IF OLD.state = 'tombstoned' AND NEW.state <> 'tombstoned' THEN
        RAISE EXCEPTION 'Tombstoned is terminal for content material' USING ERRCODE = '23514';
    END IF;
    IF OLD.state <> NEW.state
       AND NEW.state = 'tombstoned'
       AND OLD.state <> 'erasure_prepared' THEN
        RAISE EXCEPTION 'content material may enter Tombstoned only from ErasurePrepared'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state = 'erasure_prepared' AND NEW.state NOT IN ('erasure_prepared', 'tombstoned') THEN
        RAISE EXCEPTION 'ErasurePrepared content material may only advance to Tombstoned'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reject_credential_intent_state_reversal()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.state IN ('erasure_prepared', 'destroyed') THEN
            RAISE EXCEPTION 'an erasure-prepared or destroyed credential intent cannot be deleted'
                USING ERRCODE = '23514';
        END IF;
        RETURN OLD;
    END IF;
    IF OLD.state = 'destroyed' AND NEW.state <> 'destroyed' THEN
        RAISE EXCEPTION 'Destroyed is terminal for a credential key creation intent'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state <> NEW.state
       AND NEW.state = 'destroyed'
       AND OLD.state <> 'erasure_prepared' THEN
        RAISE EXCEPTION 'credential intent may enter Destroyed only from ErasurePrepared'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state = 'erasure_prepared' AND NEW.state NOT IN ('erasure_prepared', 'destroyed') THEN
        RAISE EXCEPTION 'ErasurePrepared credential intent may only advance to Destroyed'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reject_material_intent_state_reversal()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.state IN ('erasure_prepared', 'tombstoned') THEN
            RAISE EXCEPTION 'an erasure-prepared or tombstoned material key creation intent cannot be deleted'
                USING ERRCODE = '23514';
        END IF;
        RETURN OLD;
    END IF;
    IF OLD.state = 'tombstoned' AND NEW.state <> 'tombstoned' THEN
        RAISE EXCEPTION 'Tombstoned is terminal for material key creation intent'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state <> NEW.state
       AND NEW.state = 'tombstoned'
       AND OLD.state <> 'erasure_prepared' THEN
        RAISE EXCEPTION 'material key creation intent may enter Tombstoned only from ErasurePrepared'
            USING ERRCODE = '23514';
    END IF;
    IF OLD.state = 'erasure_prepared' AND NEW.state NOT IN ('erasure_prepared', 'tombstoned') THEN
        RAISE EXCEPTION 'ErasurePrepared material key creation intent may only advance to Tombstoned'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER content_materials_state_is_one_way
    BEFORE UPDATE OR DELETE ON content_materials
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_content_material_state_reversal();
CREATE TRIGGER credential_key_creation_intents_state_is_one_way
    BEFORE UPDATE OR DELETE ON credential_key_creation_intents
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_credential_intent_state_reversal();
CREATE TRIGGER material_key_creation_intents_state_is_one_way
    BEFORE UPDATE OR DELETE ON material_key_creation_intents
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_material_intent_state_reversal();

CREATE OR REPLACE FUNCTION vestrace_assert_material_erasure_workspace(
    target_workspace_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
END
$$;

CREATE OR REPLACE FUNCTION vestrace_material_erasure_has_blocker(
    target_kind TEXT,
    target_content_material_id UUID,
    target_credential_intent_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1
          FROM material_erasure_blockers
         WHERE target_kind = $1
           AND content_material_id IS NOT DISTINCT FROM $2
           AND credential_intent_id IS NOT DISTINCT FROM $3
           AND state = 'nonterminal'
           AND (
               blocker_kind IN ('effect', 'intent')
               OR (blocker_kind = 'lease' AND usable_until > NOW())
           )
    );
$$;

CREATE OR REPLACE FUNCTION vestrace_prepare_content_material_erasure(
    target_material_id UUID
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
    material_row content_materials%ROWTYPE;
    intent_row material_key_creation_intents%ROWTYPE;
    preparation_row material_erasure_preparations%ROWTYPE;
BEGIN
    SELECT * INTO material_row FROM content_materials WHERE id = target_material_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'content material is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_erasure_workspace(material_row.workspace_id);
    SELECT * INTO intent_row FROM material_key_creation_intents WHERE id = material_row.intent_id FOR UPDATE;

    SELECT * INTO preparation_row
      FROM material_erasure_preparations
     WHERE content_material_id = material_row.id
     FOR UPDATE;
    IF FOUND THEN
        preparation_id := preparation_row.id;
        material_key_id := preparation_row.material_key_id;
        finalized_erasure_receipt := preparation_row.erasure_receipt;
        RETURN NEXT;
        RETURN;
    END IF;

    IF material_row.state <> 'live' OR intent_row.state <> 'live'
       OR NOT EXISTS (
           SELECT 1 FROM content_material_ordinary_references WHERE material_id = material_row.id
       )
       OR NOT EXISTS (SELECT 1 FROM content_material_bytes WHERE material_id = material_row.id) THEN
        RAISE EXCEPTION 'only exact Live content material may prepare erasure'
            USING ERRCODE = '23514';
    END IF;
    IF vestrace_material_erasure_has_blocker('content', material_row.id, NULL) THEN
        RAISE EXCEPTION 'content material erasure has a nonterminal blocker'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO material_erasure_preparations (
        id, workspace_id, target_kind, content_material_id, material_key_id
    ) VALUES (
        gen_random_uuid(), material_row.workspace_id, 'content', material_row.id, material_row.material_key_id
    ) RETURNING * INTO preparation_row;
    INSERT INTO material_erasure_events (id, workspace_id, preparation_id, event_kind)
    VALUES (gen_random_uuid(), material_row.workspace_id, preparation_row.id, 'erasure_prepared');
    UPDATE content_materials SET state = 'erasure_prepared', updated_at = NOW() WHERE id = material_row.id;
    UPDATE material_key_creation_intents
       SET state = 'erasure_prepared', updated_at = NOW()
     WHERE id = intent_row.id;

    preparation_id := preparation_row.id;
    material_key_id := preparation_row.material_key_id;
    finalized_erasure_receipt := NULL;
    RETURN NEXT;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_prepare_credential_material_erasure(
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
    SELECT * INTO occupancy_row FROM credential_guard_occupancies WHERE id = intent_row.occupancy_id FOR UPDATE;
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_revisions WHERE id = intent_row.credential_revision_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_materials WHERE intent_id = intent_row.id FOR UPDATE;
    PERFORM vestrace_assert_material_erasure_workspace(intent_row.workspace_id);

    SELECT * INTO preparation_row
      FROM material_erasure_preparations
     WHERE credential_intent_id = intent_row.id
     FOR UPDATE;
    IF FOUND THEN
        preparation_id := preparation_row.id;
        material_key_id := preparation_row.material_key_id;
        finalized_erasure_receipt := preparation_row.erasure_receipt;
        RETURN NEXT;
        RETURN;
    END IF;

    IF intent_row.state <> 'candidate'
       OR occupancy_row.state <> 'candidate'
       OR NOT EXISTS (
           SELECT 1 FROM credential_lifecycle_events
            WHERE intent_id = intent_row.id AND event_kind = 'candidate'
       )
       OR NOT EXISTS (SELECT 1 FROM credential_prepared_materials WHERE intent_id = intent_row.id) THEN
        RAISE EXCEPTION 'only exact Candidate credential material may prepare erasure'
            USING ERRCODE = '23514';
    END IF;
    IF vestrace_material_erasure_has_blocker('credential', NULL, intent_row.id) THEN
        RAISE EXCEPTION 'credential material erasure has a nonterminal blocker'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO material_erasure_preparations (
        id, workspace_id, target_kind, credential_intent_id, material_key_id
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, 'credential', intent_row.id, intent_row.material_key_id
    ) RETURNING * INTO preparation_row;
    INSERT INTO material_erasure_events (id, workspace_id, preparation_id, event_kind)
    VALUES (gen_random_uuid(), intent_row.workspace_id, preparation_row.id, 'erasure_prepared');
    INSERT INTO credential_lifecycle_events (
        id, workspace_id, intent_id, credential_revision_id, event_kind
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, intent_row.id, intent_row.credential_revision_id,
        'erasure_prepared'
    );
    UPDATE credential_key_creation_intents
       SET state = 'erasure_prepared', updated_at = NOW()
     WHERE id = intent_row.id;

    preparation_id := preparation_row.id;
    material_key_id := preparation_row.material_key_id;
    finalized_erasure_receipt := NULL;
    RETURN NEXT;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_material_erasure_fence(
    target_preparation_id UUID,
    target_fence_receipt UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    preparation_row material_erasure_preparations%ROWTYPE;
BEGIN
    SELECT * INTO preparation_row
      FROM material_erasure_preparations
     WHERE id = target_preparation_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material erasure preparation is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_erasure_workspace(preparation_row.workspace_id);
    IF preparation_row.fence_receipt IS NOT NULL
       AND preparation_row.fence_receipt <> target_fence_receipt THEN
        RAISE EXCEPTION 'material erasure preparation already has a different fence receipt'
            USING ERRCODE = '23505';
    END IF;
    IF preparation_row.erasure_receipt IS NOT NULL THEN
        RETURN;
    END IF;
    UPDATE material_erasure_preparations
       SET fence_receipt = target_fence_receipt,
           fenced_at = COALESCE(fenced_at, NOW())
     WHERE id = preparation_row.id;
END
$$;

-- The narrow pre-prepared abort. Spec 313 allows only `erase_unbound_provisional_key`
-- before any ContentPrepared, ResultPrepared or Bound marker, and requires the
-- committed Reserved or provisional intent plus database proof of no prepared,
-- no result and no live material. That proof is taken here, under the intent
-- lock, so the vault operation the supervisor performs afterwards is authorized
-- by a committed fact rather than by the caller's say-so.
CREATE OR REPLACE FUNCTION vestrace_prepare_pre_prepared_material_abandon(
    target_intent_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row material_key_creation_intents%ROWTYPE;
BEGIN
    SELECT *
      INTO intent_row
      FROM material_key_creation_intents
     WHERE id = target_intent_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(intent_row.workspace_id);

    -- Replay of a committed abort is the same abort, not a second one.
    IF intent_row.state = 'pre_prepared_abandon_prepared' THEN
        RETURN;
    END IF;
    IF intent_row.state NOT IN ('reserved', 'provisional_created', 'provisional_receipted') THEN
        RAISE EXCEPTION
            'only a pre-prepared intent may take the pre-prepared abort; a prepared one takes its own branch'
            USING ERRCODE = '23514';
    END IF;
    IF intent_row.prepared_marker IS NOT NULL OR intent_row.bound_receipt IS NOT NULL THEN
        RAISE EXCEPTION 'a pre-prepared abort requires no prepared marker and no bind receipt'
            USING ERRCODE = '23514';
    END IF;
    IF EXISTS (SELECT 1 FROM content_materials WHERE intent_id = intent_row.id)
       OR EXISTS (SELECT 1 FROM content_material_bytes WHERE intent_id = intent_row.id)
       OR EXISTS (SELECT 1 FROM prepared_material_attachments WHERE intent_id = intent_row.id)
       OR EXISTS (
           SELECT 1 FROM content_material_ordinary_references WHERE intent_id = intent_row.id
       ) THEN
        RAISE EXCEPTION 'a pre-prepared abort requires proof of no prepared, result or live material'
            USING ERRCODE = '23514';
    END IF;

    UPDATE material_key_creation_intents
       SET state = 'pre_prepared_abandon_prepared', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

-- Erasure writes into the repository's existing Audit authority. The payload
-- carries only the padded disclosure the spec already permits: never plaintext,
-- an exact byte count, or an unkeyed digest.
CREATE OR REPLACE FUNCTION vestrace_record_material_erasure_audit(
    target_workspace_id UUID,
    target_action TEXT,
    target_resource_type TEXT,
    target_resource_id UUID,
    target_material_key_id UUID,
    target_erasure_receipt UUID,
    target_payload JSONB
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    configured_principal_id TEXT;
    recorded_audit_event_id UUID;
BEGIN
    configured_principal_id := NULLIF(current_setting('vestrace.principal_id', true), '');
    IF configured_principal_id IS NULL THEN
        RAISE EXCEPTION 'material erasure audit requires a principal context'
            USING ERRCODE = '42501';
    END IF;
    recorded_audit_event_id := gen_random_uuid();
    INSERT INTO audit_events (
        id, workspace_id, principal_id, action, resource_type, resource_id, payload
    ) VALUES (
        recorded_audit_event_id,
        target_workspace_id,
        configured_principal_id::UUID,
        target_action,
        target_resource_type,
        target_resource_id,
        target_payload || jsonb_build_object(
            'material_key_id', target_material_key_id,
            'erasure_receipt', target_erasure_receipt
        )
    );
    RETURN recorded_audit_event_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_content_material_erasure(
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
    material_row content_materials%ROWTYPE;
    intent_row material_key_creation_intents%ROWTYPE;
    recorded_audit_event_id UUID;
BEGIN
    SELECT * INTO preparation_row
      FROM material_erasure_preparations
     WHERE id = target_preparation_id
     FOR UPDATE;
    IF NOT FOUND OR preparation_row.target_kind <> 'content' THEN
        RAISE EXCEPTION 'content material erasure preparation is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_erasure_workspace(preparation_row.workspace_id);
    IF preparation_row.erasure_receipt IS NOT NULL THEN
        IF preparation_row.erasure_receipt <> target_erasure_receipt THEN
            RAISE EXCEPTION 'content material erasure already has a different receipt'
                USING ERRCODE = '23505';
        END IF;
        RETURN preparation_row.erasure_receipt;
    END IF;
    SELECT * INTO material_row FROM content_materials
     WHERE id = preparation_row.content_material_id FOR UPDATE;
    SELECT * INTO intent_row FROM material_key_creation_intents
     WHERE id = material_row.intent_id FOR UPDATE;
    PERFORM 1 FROM content_material_bytes WHERE material_id = material_row.id FOR UPDATE;
    PERFORM 1 FROM content_material_ordinary_references WHERE material_id = material_row.id FOR UPDATE;
    IF preparation_row.fence_receipt IS NULL
       OR material_row.state <> 'erasure_prepared'
       OR intent_row.state <> 'erasure_prepared'
       OR NOT EXISTS (SELECT 1 FROM content_material_bytes WHERE material_id = material_row.id)
       OR NOT EXISTS (SELECT 1 FROM content_material_ordinary_references WHERE material_id = material_row.id)
       OR NOT EXISTS (
           SELECT 1 FROM material_erasure_events
            WHERE preparation_id = preparation_row.id AND event_kind = 'erasure_prepared'
       ) THEN
        RAISE EXCEPTION 'content material erasure requires its exact fenced preparation'
            USING ERRCODE = '23514';
    END IF;

    DELETE FROM content_material_bytes WHERE material_id = material_row.id;
    UPDATE content_materials SET state = 'tombstoned', updated_at = NOW() WHERE id = material_row.id;
    UPDATE material_key_creation_intents
       SET state = 'tombstoned', updated_at = NOW()
     WHERE id = intent_row.id;
    UPDATE material_erasure_preparations
       SET erasure_receipt = target_erasure_receipt, finalized_at = NOW()
     WHERE id = preparation_row.id;
    INSERT INTO material_erasure_events (id, workspace_id, preparation_id, event_kind)
    VALUES (gen_random_uuid(), preparation_row.workspace_id, preparation_row.id, 'tombstoned');
    recorded_audit_event_id := vestrace_record_material_erasure_audit(
        preparation_row.workspace_id,
        'material.content.tombstoned',
        'content_material',
        material_row.id,
        preparation_row.material_key_id,
        target_erasure_receipt,
        jsonb_build_object(
            'size_class', material_row.size_class,
            'padded_storage_bytes_reclaimed', material_row.size_class
        )
    );
    INSERT INTO material_erasure_audit_tombstones (
        id, workspace_id, preparation_id, target_kind, target_id, erasure_receipt,
        audit_event_id
    ) VALUES (
        gen_random_uuid(), preparation_row.workspace_id, preparation_row.id, 'content',
        material_row.id, target_erasure_receipt, recorded_audit_event_id
    );
    RETURN target_erasure_receipt;
END
$$;

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
       OR occupancy_row.state <> 'candidate'
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

CREATE OR REPLACE FUNCTION vestrace_record_material_erasure_blocker(
    target_blocker_id UUID,
    target_kind TEXT,
    target_content_material_id UUID,
    target_credential_intent_id UUID,
    target_blocker_kind TEXT,
    target_usable_until TIMESTAMPTZ
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_workspace_id UUID;
    target_state TEXT;
BEGIN
    IF target_kind = 'content' THEN
        SELECT workspace_id, state INTO target_workspace_id, target_state FROM content_materials
         WHERE id = target_content_material_id FOR UPDATE;
    ELSIF target_kind = 'credential' THEN
        SELECT workspace_id, state INTO target_workspace_id, target_state FROM credential_key_creation_intents
         WHERE id = target_credential_intent_id FOR UPDATE;
    ELSE
        RAISE EXCEPTION 'material erasure blocker target is invalid' USING ERRCODE = '23514';
    END IF;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material erasure blocker target is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_erasure_workspace(target_workspace_id);
    -- 55000 (object_not_in_prerequisite_state) distinguishes a target that has
    -- already crossed its erasure boundary from an invalid request or a direct
    -- DML refusal. A blocker is meaningful only before phase one commits.
    IF (target_kind = 'content' AND target_state <> 'live')
       OR (target_kind = 'credential' AND target_state <> 'candidate') THEN
        RAISE EXCEPTION 'material erasure blocker target is no longer Live or Candidate; phase-one erasure is already prepared or terminal'
            USING ERRCODE = '55000';
    END IF;
    INSERT INTO material_erasure_blockers (
        id, workspace_id, target_kind, content_material_id, credential_intent_id,
        blocker_kind, state, usable_until
    ) VALUES (
        target_blocker_id, target_workspace_id, target_kind, target_content_material_id,
        target_credential_intent_id, target_blocker_kind, 'nonterminal', target_usable_until
    );
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
           AND (intent_state <> 'erasure_prepared' OR occupancy_state <> 'candidate'
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

CREATE CONSTRAINT TRIGGER material_erasure_preparations_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON material_erasure_preparations
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_erasure();
CREATE CONSTRAINT TRIGGER material_erasure_events_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON material_erasure_events
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_erasure();
CREATE CONSTRAINT TRIGGER material_erasure_audit_tombstones_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON material_erasure_audit_tombstones
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_erasure();

GRANT INSERT ON TABLE audit_events TO vestrace_guarded_owner;

-- sqlx::test creates fresh databases by applying migrations, not by running
-- the Compose bootstrap script. Mirror the bootstrap-only bridge there when
-- (and only when) the migration executor is a superuser. A non-superuser
-- deployment without the bootstrap bridge fails closed rather than acquiring
-- any ownership capability itself.
DO $bootstrap_helper$
BEGIN
    IF to_regprocedure('public.vestrace_assign_p02_table_owner(regclass)') IS NULL
       OR to_regprocedure('public.vestrace_assign_p02_function_owner(regprocedure)') IS NULL THEN
        IF NOT (SELECT rolsuper FROM pg_roles WHERE rolname = current_user) THEN
            RAISE EXCEPTION 'bootstrap ownership helper is unavailable'
                USING ERRCODE = '42501';
        END IF;

        EXECUTE $table_helper$
            CREATE FUNCTION public.vestrace_assign_p02_table_owner(target REGCLASS)
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $function$
            DECLARE
                target_schema TEXT;
                target_name TEXT;
            BEGIN
                SELECT namespace.nspname, relation.relname
                  INTO target_schema, target_name
                  FROM pg_class AS relation
                  JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
                 WHERE relation.oid = target AND relation.relkind = 'r';

                IF target_schema <> 'public' OR target_name <> ALL (ARRAY[
                    'p02_guarded_operation_probe',
                    'governed_mutation_audit_marks',
                    'installation_fingerprint_continuity',
                    'installation_mutation_watermark',
                    'installation_mutation_watermark_advances',
                    'material_key_creation_intents',
                    'material_key_creation_intent_erasure_receipts',
                    'content_materials',
                    'content_material_bytes',
                    'prepared_material_attachments',
                    'content_material_ordinary_references',
                    'connection_execution_guards',
                    'credential_activation_guards',
                    'credential_slots',
                    'credential_revisions',
                    'credential_guard_occupancies',
                    'credential_key_creation_intents',
                    'credential_prepared_materials',
                    'credential_prepared_attachments',
                    'credential_association_events',
                    'credential_lifecycle_events',
                    'credential_key_creation_intent_erasure_receipts',
                    'material_erasure_preparations',
                    'material_erasure_events',
                    'material_erasure_audit_tombstones',
                    'material_erasure_blockers'
                ]::TEXT[]) THEN
                    RAISE EXCEPTION 'only declared P02 tables may be handed to the guarded owner'
                        USING ERRCODE = '42501';
                END IF;

                EXECUTE format(
                    'ALTER TABLE %I.%I OWNER TO vestrace_guarded_owner',
                    target_schema,
                    target_name
                );
                EXECUTE format(
                    'REVOKE ALL ON TABLE %I.%I FROM PUBLIC',
                    target_schema,
                    target_name
                );
                EXECUTE format(
                    'REVOKE ALL ON TABLE %I.%I FROM vestrace',
                    target_schema,
                    target_name
                );
            END
            $function$;
        $table_helper$;

        EXECUTE $function_helper$
            CREATE FUNCTION public.vestrace_assign_p02_function_owner(target REGPROCEDURE)
            RETURNS VOID
            LANGUAGE plpgsql
            SECURITY DEFINER
            SET search_path = pg_catalog
            AS $function$
            DECLARE
                target_schema TEXT;
                target_name TEXT;
                target_arguments TEXT;
                allowed_targets REGPROCEDURE[] := ARRAY[
                    to_regprocedure('public.vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])'),
                    to_regprocedure('public.vestrace_assert_credential_guard_workspace(UUID)'),
                    to_regprocedure('public.vestrace_assert_material_erasure_workspace(UUID)'),
                    to_regprocedure('public.vestrace_assert_material_intent_workspace(UUID)'),
                    to_regprocedure('public.vestrace_bind_credential_key_creation_intent(UUID, UUID)'),
                    to_regprocedure('public.vestrace_bind_material_key_creation_intent(UUID, UUID)'),
                    to_regprocedure('public.vestrace_content_material_is_live(UUID)'),
                    to_regprocedure('public.vestrace_create_credential_prepared_material(UUID, UUID, BYTEA)'),
                    to_regprocedure('public.vestrace_credential_revision_is_candidate(UUID)'),
                    to_regprocedure('public.vestrace_ensure_connection_execution_guard(UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_finalize_bound_content_material(UUID)'),
                    to_regprocedure('public.vestrace_finalize_bound_credential_candidate(UUID)'),
                    to_regprocedure('public.vestrace_finalize_content_material_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_finalize_credential_key_abandon(UUID)'),
                    to_regprocedure('public.vestrace_finalize_credential_material_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_finalize_material_key_abandon(UUID)'),
                    to_regprocedure('public.vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)'),
                    to_regprocedure('public.vestrace_material_erasure_has_blocker(TEXT, UUID, UUID)'),
                    to_regprocedure('public.vestrace_prepare_content_abandon(UUID)'),
                    to_regprocedure('public.vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)'),
                    to_regprocedure('public.vestrace_prepare_content_material_erasure(UUID)'),
                    to_regprocedure('public.vestrace_prepare_credential_material_erasure(UUID)'),
                    to_regprocedure('public.vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT)'),
                    to_regprocedure('public.vestrace_prepare_material_key_content(UUID, UUID, BYTEA, BIGINT, TEXT)'),
                    to_regprocedure('public.vestrace_prepare_pre_prepared_material_abandon(UUID)'),
                    to_regprocedure('public.vestrace_prepare_result_material(UUID, UUID, BYTEA, BIGINT)'),
                    to_regprocedure('public.vestrace_record_credential_key_provisional_created(UUID)'),
                    to_regprocedure('public.vestrace_record_credential_key_provisional_receipt(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_credential_unbound_key_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ)'),
                    to_regprocedure('public.vestrace_record_governed_mutation_audit_mark_and_advance(UUID, UUID, UUID, TIMESTAMPTZ)'),
                    to_regprocedure('public.vestrace_record_guarded_operation_probe(UUID)'),
                    to_regprocedure('public.vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)'),
                    to_regprocedure('public.vestrace_record_material_erasure_audit(UUID, TEXT, TEXT, UUID, UUID, UUID, JSONB)'),
                    to_regprocedure('public.vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)'),
                    to_regprocedure('public.vestrace_record_material_erasure_fence(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_material_key_provisional_created(UUID)'),
                    to_regprocedure('public.vestrace_record_material_key_provisional_receipt(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_unbound_material_key_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_reject_content_material_state_reversal()'),
                    to_regprocedure('public.vestrace_reject_credential_intent_state_reversal()'),
                    to_regprocedure('public.vestrace_reject_material_intent_state_reversal()'),
                    to_regprocedure('public.vestrace_reject_raw_connection_execution_guard_mutation()'),
                    to_regprocedure('public.vestrace_reject_raw_credential_guard_mutation()'),
                    to_regprocedure('public.vestrace_reject_raw_credential_intent_mutation()'),
                    to_regprocedure('public.vestrace_reject_raw_credential_revision_mutation()'),
                    to_regprocedure('public.vestrace_reject_raw_material_erasure_mutation()'),
                    to_regprocedure('public.vestrace_reject_raw_material_intent_mutation()'),
                    to_regprocedure('public.vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)'),
                    to_regprocedure('public.vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)'),
                    to_regprocedure('public.vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)'),
                    to_regprocedure('public.vestrace_validate_credential_key_creation_intent()'),
                    to_regprocedure('public.vestrace_validate_material_erasure()'),
                    to_regprocedure('public.vestrace_validate_material_key_creation_intent()')
                ];
                runtime_executable_targets REGPROCEDURE[] := ARRAY[
                    to_regprocedure('public.vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])'),
                    to_regprocedure('public.vestrace_bind_credential_key_creation_intent(UUID, UUID)'),
                    to_regprocedure('public.vestrace_bind_material_key_creation_intent(UUID, UUID)'),
                    to_regprocedure('public.vestrace_content_material_is_live(UUID)'),
                    to_regprocedure('public.vestrace_create_credential_prepared_material(UUID, UUID, BYTEA)'),
                    to_regprocedure('public.vestrace_credential_revision_is_candidate(UUID)'),
                    to_regprocedure('public.vestrace_ensure_connection_execution_guard(UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_finalize_bound_content_material(UUID)'),
                    to_regprocedure('public.vestrace_finalize_bound_credential_candidate(UUID)'),
                    to_regprocedure('public.vestrace_finalize_content_material_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_finalize_credential_key_abandon(UUID)'),
                    to_regprocedure('public.vestrace_finalize_credential_material_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_finalize_material_key_abandon(UUID)'),
                    to_regprocedure('public.vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)'),
                    to_regprocedure('public.vestrace_prepare_content_abandon(UUID)'),
                    to_regprocedure('public.vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)'),
                    to_regprocedure('public.vestrace_prepare_content_material_erasure(UUID)'),
                    to_regprocedure('public.vestrace_prepare_credential_material_erasure(UUID)'),
                    to_regprocedure('public.vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT)'),
                    to_regprocedure('public.vestrace_prepare_pre_prepared_material_abandon(UUID)'),
                    to_regprocedure('public.vestrace_record_credential_key_provisional_created(UUID)'),
                    to_regprocedure('public.vestrace_record_credential_key_provisional_receipt(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_credential_unbound_key_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ)'),
                    to_regprocedure('public.vestrace_record_governed_mutation_audit_mark_and_advance(UUID, UUID, UUID, TIMESTAMPTZ)'),
                    to_regprocedure('public.vestrace_record_guarded_operation_probe(UUID)'),
                    to_regprocedure('public.vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)'),
                    to_regprocedure('public.vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)'),
                    to_regprocedure('public.vestrace_record_material_erasure_fence(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_material_key_provisional_created(UUID)'),
                    to_regprocedure('public.vestrace_record_material_key_provisional_receipt(UUID, UUID)'),
                    to_regprocedure('public.vestrace_record_unbound_material_key_erasure(UUID, UUID)'),
                    to_regprocedure('public.vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)'),
                    to_regprocedure('public.vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)'),
                    to_regprocedure('public.vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)'),
                    to_regprocedure('public.vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)')
                ];
            BEGIN
                SELECT namespace.nspname, procedure.proname, pg_get_function_identity_arguments(procedure.oid)
                  INTO target_schema, target_name, target_arguments
                  FROM pg_proc AS procedure
                  JOIN pg_namespace AS namespace ON namespace.oid = procedure.pronamespace
                 WHERE procedure.oid = target;

                IF target_schema <> 'public' OR NOT COALESCE(target = ANY (allowed_targets), FALSE) THEN
                    RAISE EXCEPTION 'only exact declared P02 function signatures may be handed to the guarded owner'
                        USING ERRCODE = '42501';
                END IF;

                EXECUTE format(
                    'ALTER FUNCTION %I.%I(%s) OWNER TO vestrace_guarded_owner',
                    target_schema,
                    target_name,
                    target_arguments
                );
                EXECUTE format(
                    'REVOKE ALL ON FUNCTION %I.%I(%s) FROM PUBLIC',
                    target_schema,
                    target_name,
                    target_arguments
                );
                IF COALESCE(target = ANY (runtime_executable_targets), FALSE) THEN
                    EXECUTE format(
                        'GRANT EXECUTE ON FUNCTION %I.%I(%s) TO vestrace',
                        target_schema,
                        target_name,
                        target_arguments
                    );
                END IF;
            END
            $function$;
        $function_helper$;

        REVOKE ALL ON FUNCTION public.vestrace_assign_p02_table_owner(REGCLASS) FROM PUBLIC;
        REVOKE ALL ON FUNCTION public.vestrace_assign_p02_function_owner(REGPROCEDURE) FROM PUBLIC;
        GRANT EXECUTE ON FUNCTION public.vestrace_assign_p02_table_owner(REGCLASS) TO vestrace;
        GRANT EXECUTE ON FUNCTION public.vestrace_assign_p02_function_owner(REGPROCEDURE) TO vestrace;
    END IF;
END
$bootstrap_helper$;

SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_raw_material_erasure_mutation()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_assert_material_erasure_workspace(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_material_erasure_has_blocker(TEXT, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_content_material_erasure(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_credential_material_erasure(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_material_erasure_fence(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_pre_prepared_material_abandon(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_material_erasure_audit(UUID, TEXT, TEXT, UUID, UUID, UUID, JSONB)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_content_material_state_reversal()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_credential_intent_state_reversal()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_material_intent_state_reversal()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_finalize_content_material_erasure(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_finalize_credential_material_erasure(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner('vestrace_validate_material_erasure()'::REGPROCEDURE);

-- P02 ownership handoff deliberately occurs only after every P02 migration
-- has performed its DDL. The runtime migrator must retain ownership until this
-- point because later migrations redefine and alter earlier P02 objects.
SELECT vestrace_assign_p02_table_owner('p02_guarded_operation_probe'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('governed_mutation_audit_marks'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('installation_fingerprint_continuity'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('installation_mutation_watermark'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('installation_mutation_watermark_advances'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('material_key_creation_intents'::REGCLASS);
SELECT vestrace_assign_p02_table_owner(
    'material_key_creation_intent_erasure_receipts'::REGCLASS
);
SELECT vestrace_assign_p02_table_owner('content_materials'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('content_material_bytes'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('prepared_material_attachments'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('content_material_ordinary_references'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('connection_execution_guards'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_slots'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_activation_guards'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_revisions'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_guard_occupancies'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_key_creation_intents'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_prepared_materials'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_prepared_attachments'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_association_events'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('credential_lifecycle_events'::REGCLASS);
SELECT vestrace_assign_p02_table_owner(
    'credential_key_creation_intent_erasure_receipts'::REGCLASS
);
SELECT vestrace_assign_p02_table_owner('material_erasure_preparations'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('material_erasure_events'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('material_erasure_audit_tombstones'::REGCLASS);
SELECT vestrace_assign_p02_table_owner('material_erasure_blockers'::REGCLASS);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_guarded_operation_probe(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_governed_mutation_audit_mark_and_advance(UUID, UUID, UUID, TIMESTAMPTZ)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_raw_material_intent_mutation()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_assert_material_intent_workspace(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reserve_material_key_creation_intent(UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_material_key_provisional_created(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_material_key_provisional_receipt(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_unbound_material_key_erasure(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_material_key_content(UUID, UUID, BYTEA, BIGINT, TEXT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_result_material(UUID, UUID, BYTEA, BIGINT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner('vestrace_prepare_content_abandon(UUID)'::REGPROCEDURE);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_bind_material_key_creation_intent(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_finalize_bound_content_material(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner('vestrace_finalize_material_key_abandon(UUID)'::REGPROCEDURE);
SELECT vestrace_assign_p02_function_owner('vestrace_content_material_is_live(UUID)'::REGPROCEDURE);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_validate_material_key_creation_intent()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_raw_connection_execution_guard_mutation()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_assert_credential_guard_workspace(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_ensure_connection_execution_guard(UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_raw_credential_guard_mutation()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_raw_credential_revision_mutation()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reject_raw_credential_intent_mutation()'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_reserve_credential_key_creation_intent(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_credential_key_provisional_created(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_credential_key_provisional_receipt(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_create_credential_prepared_material(UUID, UUID, BYTEA)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_bind_credential_key_creation_intent(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_finalize_bound_credential_candidate(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_record_credential_unbound_key_erasure(UUID, UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_finalize_credential_key_abandon(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_credential_revision_is_candidate(UUID)'::REGPROCEDURE
);
SELECT vestrace_assign_p02_function_owner(
    'vestrace_validate_credential_key_creation_intent()'::REGPROCEDURE
);

-- The bootstrap-owned ownership bridge applies the following original ACL
-- intent while it owns each function. Runtime SQL cannot alter ACLs after the
-- handoff, so this reference block is deliberately non-executing.
/*
REVOKE ALL ON FUNCTION vestrace_record_guarded_operation_probe(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_record_guarded_operation_probe(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_governed_mutation_audit_mark(UUID, UUID, UUID, TIMESTAMPTZ) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)
    FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_record_installation_fingerprint_continuity(UUID, UUID, INTEGER, BYTEA)
    TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_installation_fingerprint_continuity_matches(UUID, UUID, INTEGER, BYTEA)
    TO vestrace;

REVOKE ALL ON FUNCTION vestrace_record_governed_mutation_audit_mark_and_advance(
    UUID, UUID, UUID, TIMESTAMPTZ
) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_governed_mutation_audit_mark(
    UUID, UUID, UUID, TIMESTAMPTZ
) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_record_governed_mutation_audit_mark_and_advance(
    UUID, UUID, UUID, TIMESTAMPTZ
) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_governed_mutation_audit_mark(
    UUID, UUID, UUID, TIMESTAMPTZ
) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_assert_material_intent_workspace(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_reserve_material_key_creation_intent(
    UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT
) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_material_key_provisional_created(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_material_key_provisional_receipt(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_unbound_material_key_erasure(UUID, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_reserve_material_key_creation_intent(
    UUID, UUID, UUID, UUID, UUID, TEXT, UUID, BIGINT
) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_material_key_provisional_created(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_material_key_provisional_receipt(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_unbound_material_key_erasure(UUID, UUID) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_prepare_material_key_content(UUID, UUID, BYTEA, BIGINT, TEXT)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_prepare_result_material(UUID, UUID, BYTEA, BIGINT)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_prepare_content_abandon(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_bind_material_key_creation_intent(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finalize_bound_content_material(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finalize_material_key_abandon(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_content_material_is_live(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_prepare_content_material(UUID, UUID, BYTEA, BIGINT)
    TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_prepare_content_abandon(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_bind_material_key_creation_intent(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_bound_content_material(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_material_key_abandon(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_content_material_is_live(UUID) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_reject_raw_connection_execution_guard_mutation() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_assert_credential_guard_workspace(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_ensure_connection_execution_guard(UUID, UUID, UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_ensure_connection_execution_guard(UUID, UUID, UUID)
    TO vestrace;

REVOKE ALL ON FUNCTION vestrace_reject_raw_credential_guard_mutation() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_reject_raw_credential_revision_mutation() FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)
    FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_reserve_credential_slot(UUID, UUID, UUID, TEXT, TEXT)
    TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_ensure_credential_activation_guard(UUID, UUID, UUID, UUID)
    TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_acquire_credential_lock_chain(UUID, UUID, UUID, TEXT[])
    TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_reserve_credential_preparing_occupancy(UUID, UUID, UUID, UUID)
    TO vestrace;

REVOKE ALL ON FUNCTION vestrace_reserve_credential_key_creation_intent(
    UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT
) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_credential_key_provisional_created(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_credential_key_provisional_receipt(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_create_credential_prepared_material(UUID, UUID, BYTEA) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_bind_credential_key_creation_intent(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finalize_bound_credential_candidate(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_credential_unbound_key_erasure(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finalize_credential_key_abandon(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_credential_revision_is_candidate(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_reserve_credential_key_creation_intent(
    UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT
) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_credential_key_provisional_created(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_credential_key_provisional_receipt(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_create_credential_prepared_material(UUID, UUID, BYTEA) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_bind_credential_key_creation_intent(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_bound_credential_candidate(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_prepare_credential_pre_live_abandon(UUID, BIGINT) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_credential_unbound_key_erasure(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_credential_key_abandon(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_credential_revision_is_candidate(UUID) TO vestrace;

REVOKE ALL ON FUNCTION vestrace_prepare_pre_prepared_material_abandon(UUID) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_prepare_pre_prepared_material_abandon(UUID) TO vestrace;
REVOKE ALL ON FUNCTION vestrace_assert_material_erasure_workspace(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_material_erasure_has_blocker(TEXT, UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_prepare_content_material_erasure(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_prepare_credential_material_erasure(UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_material_erasure_fence(UUID, UUID) FROM PUBLIC;
-- Only the guarded owner writes the erasure Audit entry, and only from inside
-- the two finalizers. The runtime role never receives EXECUTE on it.
REVOKE ALL ON FUNCTION vestrace_record_material_erasure_audit(UUID, TEXT, TEXT, UUID, UUID, UUID, JSONB)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finalize_content_material_erasure(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_finalize_credential_material_erasure(UUID, UUID) FROM PUBLIC;
REVOKE ALL ON FUNCTION vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)
    FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vestrace_prepare_content_material_erasure(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_prepare_credential_material_erasure(UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_material_erasure_fence(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_content_material_erasure(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_finalize_credential_material_erasure(UUID, UUID) TO vestrace;
GRANT EXECUTE ON FUNCTION vestrace_record_material_erasure_blocker(UUID, TEXT, UUID, UUID, TEXT, TIMESTAMPTZ)
    TO vestrace;
*/
