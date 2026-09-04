-- Credential keys are fixed identities.  A pre-live cancellation may remove
-- ciphertext, but it cannot publish a Candidate or release its guard until a
-- host-vault erasure receipt has been witnessed.
ALTER TABLE credential_guard_occupancies
    ADD COLUMN association_version BIGINT NOT NULL DEFAULT 1
        CHECK (association_version >= 1),
    ADD COLUMN intent_id UUID UNIQUE;

DROP INDEX credential_guard_occupancies_one_live_candidate_key;
CREATE UNIQUE INDEX credential_guard_occupancies_one_nonterminal_key
    ON credential_guard_occupancies (activation_guard_id)
    WHERE state IN ('preparing', 'candidate', 'cancelled');

CREATE TABLE credential_key_creation_intents (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL REFERENCES connections(id) ON DELETE RESTRICT,
    credential_slot_id UUID NOT NULL,
    occupancy_id UUID NOT NULL UNIQUE REFERENCES credential_guard_occupancies(id) ON DELETE RESTRICT,
    credential_revision_id UUID NOT NULL UNIQUE,
    material_key_id UUID NOT NULL UNIQUE,
    nonce UUID NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN (
        'reserved',
        'provisional_created',
        'provisional_receipted',
        'credential_prepared',
        'credential_abandon_prepared',
        'bound',
        'candidate',
        'active',
        'retired',
        'abandoned'
    )),
    vault_receipt UUID,
    bound_receipt UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_key_creation_intents_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_key_creation_intents_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_key_creation_intents_workspace_id_id_key UNIQUE (workspace_id, id)
);

-- This index is the unconditional singleton authority: exactly one intent for
-- a slot can be Active even when a transaction does not update another Active
-- intent. The deferred invariant below is the consistency authority: it ties
-- that Active intent to the slot's current-revision CAS pointer and occupancy.
CREATE UNIQUE INDEX credential_key_creation_intents_one_active_slot_key
    ON credential_key_creation_intents (credential_slot_id)
    WHERE state = 'active';

ALTER TABLE credential_guard_occupancies
    ADD CONSTRAINT credential_guard_occupancies_intent_fkey
    FOREIGN KEY (intent_id) REFERENCES credential_key_creation_intents(id) ON DELETE RESTRICT;

CREATE TABLE credential_prepared_materials (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    credential_revision_id UUID NOT NULL UNIQUE,
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_prepared_materials_intent_fkey
        FOREIGN KEY (workspace_id, intent_id)
        REFERENCES credential_key_creation_intents(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_prepared_materials_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id)
        ON DELETE RESTRICT
);

CREATE TABLE credential_prepared_attachments (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    credential_revision_id UUID NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_prepared_attachments_intent_fkey
        FOREIGN KEY (workspace_id, intent_id)
        REFERENCES credential_key_creation_intents(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_prepared_attachments_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id)
        ON DELETE RESTRICT
);

CREATE TABLE credential_association_events (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    occupancy_id UUID NOT NULL,
    intent_id UUID NOT NULL,
    event_kind TEXT NOT NULL CHECK (event_kind = 'credential_association_cancelled'),
    expected_version BIGINT NOT NULL CHECK (expected_version >= 1),
    resulting_version BIGINT NOT NULL CHECK (resulting_version > expected_version),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_association_events_intent_fkey
        FOREIGN KEY (workspace_id, intent_id)
        REFERENCES credential_key_creation_intents(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_association_events_occupancy_fkey
        FOREIGN KEY (occupancy_id)
        REFERENCES credential_guard_occupancies(id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_association_events_one_cancellation UNIQUE (intent_id)
);

CREATE TABLE credential_lifecycle_events (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL,
    credential_revision_id UUID NOT NULL,
    event_kind TEXT NOT NULL CHECK (event_kind = 'candidate'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_lifecycle_events_intent_fkey
        FOREIGN KEY (workspace_id, intent_id)
        REFERENCES credential_key_creation_intents(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_lifecycle_events_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_lifecycle_events_one_candidate UNIQUE (intent_id)
);

CREATE TABLE credential_key_creation_intent_erasure_receipts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    erasure_receipt UUID NOT NULL UNIQUE,
    witnessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_key_creation_intent_erasure_receipts_intent_fkey
        FOREIGN KEY (workspace_id, intent_id)
        REFERENCES credential_key_creation_intents(workspace_id, id)
        ON DELETE RESTRICT
);

ALTER TABLE credential_key_creation_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_key_creation_intents FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_prepared_materials ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_prepared_materials FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_prepared_attachments ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_prepared_attachments FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_association_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_association_events FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_lifecycle_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_lifecycle_events FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_key_creation_intent_erasure_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_key_creation_intent_erasure_receipts FORCE ROW LEVEL SECURITY;

CREATE POLICY credential_key_creation_intents_workspace_policy
    ON credential_key_creation_intents
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_prepared_materials_workspace_policy
    ON credential_prepared_materials
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_prepared_attachments_workspace_policy
    ON credential_prepared_attachments
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_association_events_workspace_policy
    ON credential_association_events
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_lifecycle_events_workspace_policy
    ON credential_lifecycle_events
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_key_creation_intent_erasure_receipts_workspace_policy
    ON credential_key_creation_intent_erasure_receipts
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);


CREATE OR REPLACE FUNCTION vestrace_reject_raw_credential_intent_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' THEN
        RAISE EXCEPTION 'credential intent state changes require a guarded operation'
            USING ERRCODE = '42501';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER credential_key_creation_intents_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_key_creation_intents
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_intent_mutation();
CREATE TRIGGER credential_prepared_materials_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_prepared_materials
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_intent_mutation();
CREATE TRIGGER credential_prepared_attachments_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_prepared_attachments
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_intent_mutation();
CREATE TRIGGER credential_association_events_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_association_events
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_intent_mutation();
CREATE TRIGGER credential_lifecycle_events_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_lifecycle_events
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_intent_mutation();
CREATE TRIGGER credential_key_creation_intent_erasure_receipts_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_key_creation_intent_erasure_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_intent_mutation();

CREATE OR REPLACE FUNCTION vestrace_reserve_credential_key_creation_intent(
    target_intent_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_slot_id UUID,
    target_occupancy_id UUID,
    target_revision_id UUID,
    target_material_key_id UUID,
    target_nonce UUID,
    target_associated_data_profile TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    occupancy_row credential_guard_occupancies%ROWTYPE;
BEGIN
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id,
        target_connection_id,
        target_slot_id,
        ARRAY[
            'connection_execution_guard',
            'credential_activation_guard',
            'credential_slot',
            'revision_material'
        ]::TEXT[]
    );
    SELECT * INTO occupancy_row
      FROM credential_guard_occupancies
     WHERE id = target_occupancy_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
     FOR UPDATE;
    IF NOT FOUND OR occupancy_row.state <> 'preparing' THEN
        RAISE EXCEPTION 'credential intent requires a Preparing association'
            USING ERRCODE = '23514';
    END IF;
    IF occupancy_row.intent_id IS NOT NULL THEN
        RAISE EXCEPTION 'credential association already has an intent'
            USING ERRCODE = '23505';
    END IF;
    IF target_associated_data_profile NOT IN ('credential_v2', 'legacy_v1') THEN
        RAISE EXCEPTION 'credential associated-data profile is invalid' USING ERRCODE = '23514';
    END IF;

    -- Allocate this immutable identity before any caller can encrypt.
    INSERT INTO credential_revisions (
        id, workspace_id, credential_slot_id, material_key_id, associated_data_profile
    ) VALUES (
        target_revision_id, target_workspace_id, target_slot_id,
        target_material_key_id, target_associated_data_profile
    );
    INSERT INTO credential_key_creation_intents (
        id, workspace_id, connection_id, credential_slot_id, occupancy_id,
        credential_revision_id, material_key_id, nonce, state
    ) VALUES (
        target_intent_id, target_workspace_id, target_connection_id, target_slot_id,
        target_occupancy_id, target_revision_id, target_material_key_id, target_nonce, 'reserved'
    );
    UPDATE credential_guard_occupancies
       SET intent_id = target_intent_id, updated_at = NOW()
     WHERE id = target_occupancy_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_credential_key_provisional_created(
    target_intent_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'reserved' THEN
        RAISE EXCEPTION 'credential key creation intent must be Reserved' USING ERRCODE = '23514';
    END IF;
    UPDATE credential_key_creation_intents SET state = 'provisional_created', updated_at = NOW()
     WHERE id = target_intent_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_credential_key_provisional_receipt(
    target_intent_id UUID,
    target_vault_receipt UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'provisional_created' THEN
        RAISE EXCEPTION 'credential key creation intent must be ProvisionalCreated'
            USING ERRCODE = '23514';
    END IF;
    UPDATE credential_key_creation_intents
       SET state = 'provisional_receipted', vault_receipt = target_vault_receipt, updated_at = NOW()
     WHERE id = target_intent_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_create_credential_prepared_material(
    target_intent_id UUID,
    target_attachment_id UUID,
    target_ciphertext BYTEA
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'provisional_receipted' OR intent_row.vault_receipt IS NULL THEN
        RAISE EXCEPTION 'credential key creation intent must be ProvisionalReceipted'
            USING ERRCODE = '23514';
    END IF;
    IF target_ciphertext IS NULL OR octet_length(target_ciphertext) = 0 THEN
        RAISE EXCEPTION 'credential prepared ciphertext must be nonempty' USING ERRCODE = '23514';
    END IF;

    INSERT INTO credential_prepared_materials (
        id, workspace_id, intent_id, credential_revision_id, ciphertext
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, intent_row.id,
        intent_row.credential_revision_id, target_ciphertext
    );
    INSERT INTO credential_prepared_attachments (
        id, workspace_id, intent_id, credential_revision_id
    ) VALUES (
        target_attachment_id, intent_row.workspace_id, intent_row.id,
        intent_row.credential_revision_id
    );
    UPDATE credential_key_creation_intents
       SET state = 'credential_prepared', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_bind_credential_key_creation_intent(
    target_intent_id UUID,
    target_bound_receipt UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'credential_prepared' THEN
        RAISE EXCEPTION 'only CredentialPrepared may bind a credential key' USING ERRCODE = '23514';
    END IF;
    UPDATE credential_key_creation_intents
       SET state = 'bound', bound_receipt = target_bound_receipt, updated_at = NOW()
     WHERE id = target_intent_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_bound_credential_candidate(
    target_intent_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
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
    PERFORM 1 FROM credential_prepared_materials WHERE intent_id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_attachments WHERE intent_id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'bound' OR intent_row.bound_receipt IS NULL
       OR occupancy_row.state <> 'preparing'
       OR NOT EXISTS (SELECT 1 FROM credential_prepared_materials WHERE intent_id = target_intent_id)
       OR NOT EXISTS (SELECT 1 FROM credential_prepared_attachments WHERE intent_id = target_intent_id) THEN
        RAISE EXCEPTION 'only exact Bound prepared credential material may become Candidate'
            USING ERRCODE = '23514';
    END IF;

    DELETE FROM credential_prepared_attachments WHERE intent_id = target_intent_id;
    INSERT INTO credential_lifecycle_events (
        id, workspace_id, intent_id, credential_revision_id, event_kind
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, intent_row.id,
        intent_row.credential_revision_id, 'candidate'
    );
    UPDATE credential_guard_occupancies
       SET state = 'candidate', association_version = association_version + 1, updated_at = NOW()
     WHERE id = occupancy_row.id;
    UPDATE credential_key_creation_intents SET state = 'candidate', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_prepare_credential_pre_live_abandon(
    target_intent_id UUID,
    target_expected_association_version BIGINT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    -- Canonical permanent guards first; the association/version is then locked
    -- before the intent and its prepared-material rows.
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO occupancy_row FROM credential_guard_occupancies WHERE id = intent_row.occupancy_id FOR UPDATE;
    IF occupancy_row.state <> 'preparing'
       OR occupancy_row.association_version <> target_expected_association_version THEN
        RAISE EXCEPTION 'credential association is no longer the expected Preparing version'
            USING ERRCODE = '23514';
    END IF;
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_materials WHERE intent_id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_attachments WHERE intent_id = target_intent_id FOR UPDATE;
    IF intent_row.state NOT IN ('reserved', 'provisional_created', 'provisional_receipted', 'credential_prepared')
       OR intent_row.bound_receipt IS NOT NULL
       OR EXISTS (SELECT 1 FROM credential_lifecycle_events WHERE intent_id = target_intent_id)
       OR EXISTS (SELECT 1 FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = target_intent_id) THEN
        RAISE EXCEPTION 'only a pre-live credential intent without facts may prepare abandonment'
            USING ERRCODE = '23514';
    END IF;

    DELETE FROM credential_prepared_materials WHERE intent_id = target_intent_id;
    DELETE FROM credential_prepared_attachments WHERE intent_id = target_intent_id;
    INSERT INTO credential_association_events (
        id, workspace_id, occupancy_id, intent_id, event_kind, expected_version, resulting_version
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, occupancy_row.id, intent_row.id,
        'credential_association_cancelled', occupancy_row.association_version,
        occupancy_row.association_version + 1
    );
    UPDATE credential_guard_occupancies
       SET state = 'cancelled', association_version = association_version + 1, updated_at = NOW()
     WHERE id = occupancy_row.id;
    UPDATE credential_key_creation_intents
       SET state = 'credential_abandon_prepared', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_credential_unbound_key_erasure(
    target_intent_id UUID,
    target_erasure_receipt UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
BEGIN
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        intent_row.workspace_id, intent_row.connection_id, intent_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'credential_abandon_prepared'
       OR NOT EXISTS (SELECT 1 FROM credential_association_events WHERE intent_id = target_intent_id)
       OR EXISTS (SELECT 1 FROM credential_prepared_materials WHERE intent_id = target_intent_id)
       OR EXISTS (SELECT 1 FROM credential_prepared_attachments WHERE intent_id = target_intent_id) THEN
        RAISE EXCEPTION 'only a cancelled prepared credential intent may record unbound-key erasure'
            USING ERRCODE = '23514';
    END IF;
    INSERT INTO credential_key_creation_intent_erasure_receipts (
        id, workspace_id, intent_id, erasure_receipt
    ) VALUES (
        gen_random_uuid(), intent_row.workspace_id, intent_row.id, target_erasure_receipt
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_credential_key_abandon(
    target_intent_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
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
    PERFORM 1 FROM credential_prepared_materials WHERE intent_id = target_intent_id FOR UPDATE;
    PERFORM 1 FROM credential_prepared_attachments WHERE intent_id = target_intent_id FOR UPDATE;
    IF intent_row.state <> 'credential_abandon_prepared'
       OR occupancy_row.state <> 'cancelled'
       OR NOT EXISTS (SELECT 1 FROM credential_association_events WHERE intent_id = target_intent_id)
       OR NOT EXISTS (SELECT 1 FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = target_intent_id)
       OR EXISTS (SELECT 1 FROM credential_prepared_materials WHERE intent_id = target_intent_id)
       OR EXISTS (SELECT 1 FROM credential_prepared_attachments WHERE intent_id = target_intent_id)
       OR EXISTS (SELECT 1 FROM credential_lifecycle_events WHERE intent_id = target_intent_id) THEN
        RAISE EXCEPTION 'Abandoned requires cancellation, prepared-data removal, and witnessed erasure'
            USING ERRCODE = '23514';
    END IF;
    UPDATE credential_guard_occupancies
       SET state = 'destroyed', association_version = association_version + 1, updated_at = NOW()
     WHERE id = occupancy_row.id;
    UPDATE credential_key_creation_intents SET state = 'abandoned', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_credential_revision_is_candidate(
    target_revision_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1
          FROM credential_key_creation_intents AS intent
          JOIN credential_lifecycle_events AS event ON event.intent_id = intent.id
         WHERE intent.credential_revision_id = target_revision_id
           AND intent.state = 'candidate'
           AND event.event_kind = 'candidate'
           AND intent.workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
    );
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_credential_key_creation_intent()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_intent_id UUID;
    intent_row credential_key_creation_intents%ROWTYPE;
    occupancy_row credential_guard_occupancies%ROWTYPE;
    prepared_count BIGINT;
    attachment_count BIGINT;
    candidate_count BIGINT;
    cancellation_count BIGINT;
    erasure_receipt_count BIGINT;
BEGIN
    IF TG_TABLE_NAME = 'credential_guard_occupancies' THEN
        target_intent_id := COALESCE(NEW.intent_id, OLD.intent_id);
    ELSIF TG_TABLE_NAME = 'credential_key_creation_intents' THEN
        target_intent_id := COALESCE(NEW.id, OLD.id);
    ELSE
        target_intent_id := COALESCE(NEW.intent_id, OLD.intent_id);
    END IF;
    IF target_intent_id IS NULL THEN
        RETURN NULL;
    END IF;
    SELECT * INTO intent_row FROM credential_key_creation_intents WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    SELECT * INTO occupancy_row FROM credential_guard_occupancies WHERE id = intent_row.occupancy_id;
    SELECT COUNT(*) INTO prepared_count FROM credential_prepared_materials WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO attachment_count FROM credential_prepared_attachments WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO candidate_count FROM credential_lifecycle_events WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO cancellation_count FROM credential_association_events WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO erasure_receipt_count
      FROM credential_key_creation_intent_erasure_receipts WHERE intent_id = target_intent_id;

    IF occupancy_row.id IS NULL
       OR occupancy_row.workspace_id <> intent_row.workspace_id
       OR occupancy_row.connection_id <> intent_row.connection_id
       OR occupancy_row.credential_slot_id <> intent_row.credential_slot_id
       OR occupancy_row.intent_id <> intent_row.id THEN
        RAISE EXCEPTION 'credential intent must retain its exact Preparing association'
            USING ERRCODE = '23514';
    END IF;
    IF intent_row.state IN ('reserved', 'provisional_created')
       AND (intent_row.vault_receipt IS NOT NULL OR intent_row.bound_receipt IS NOT NULL
            OR prepared_count <> 0 OR attachment_count <> 0 OR candidate_count <> 0
            OR cancellation_count <> 0 OR erasure_receipt_count <> 0
            OR occupancy_row.state <> 'preparing') THEN
        RAISE EXCEPTION 'pre-provisional credential intent is inconsistent' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'provisional_receipted'
       AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NOT NULL
            OR prepared_count <> 0 OR attachment_count <> 0 OR candidate_count <> 0
            OR cancellation_count <> 0 OR erasure_receipt_count <> 0
            OR occupancy_row.state <> 'preparing') THEN
        RAISE EXCEPTION 'ProvisionalReceipted credential intent is inconsistent' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'credential_prepared'
       AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NOT NULL
            OR prepared_count <> 1 OR attachment_count <> 1 OR candidate_count <> 0
            OR cancellation_count <> 0 OR erasure_receipt_count <> 0
            OR occupancy_row.state <> 'preparing') THEN
        RAISE EXCEPTION 'CredentialPrepared intent is inconsistent' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'bound'
       AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NULL
            OR prepared_count <> 1 OR attachment_count <> 1 OR candidate_count <> 0
            OR cancellation_count <> 0 OR erasure_receipt_count <> 0
            OR occupancy_row.state <> 'preparing') THEN
        RAISE EXCEPTION 'Bound credential intent is inconsistent' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'candidate'
        AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NULL
             OR prepared_count <> 1 OR attachment_count <> 0 OR candidate_count <> 1
             OR cancellation_count <> 0 OR erasure_receipt_count <> 0
             OR occupancy_row.state <> 'candidate') THEN
        RAISE EXCEPTION 'Candidate requires exact Bound publication' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'active'
        AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NULL
             OR prepared_count <> 1 OR attachment_count <> 0 OR candidate_count <> 1
             OR cancellation_count <> 0 OR erasure_receipt_count <> 0
             OR occupancy_row.state <> 'activated'
             OR NOT EXISTS (
                 SELECT 1
                   FROM credential_slots AS slot
                  WHERE slot.workspace_id = intent_row.workspace_id
                    AND slot.connection_id = intent_row.connection_id
                    AND slot.id = intent_row.credential_slot_id
                    AND slot.current_revision_id = intent_row.credential_revision_id
                    AND slot.current_revision_version >= 1
             )) THEN
        RAISE EXCEPTION 'Active requires exact Candidate publication and current slot pointer'
            USING ERRCODE = '23514';
    END IF;
    -- Retired retains every Candidate-material fact and the completed activation
    -- association, but is no longer the slot pointer. It intentionally has no
    -- erasure receipt: ordinary erasure is a subsequent, separately guarded
    -- transition after the rotation/revocation evidence recorded by P03.
    IF intent_row.state = 'retired'
        AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NULL
             OR prepared_count <> 1 OR attachment_count <> 0 OR candidate_count <> 1
             OR cancellation_count <> 0 OR erasure_receipt_count <> 0
             OR occupancy_row.state <> 'activated'
             OR NOT EXISTS (
                 SELECT 1
                   FROM credential_slots AS slot
                  WHERE slot.workspace_id = intent_row.workspace_id
                    AND slot.connection_id = intent_row.connection_id
                    AND slot.id = intent_row.credential_slot_id
                    AND slot.current_revision_id IS DISTINCT FROM intent_row.credential_revision_id
                    AND slot.current_revision_version >= 1
             )) THEN
        RAISE EXCEPTION 'Retired requires intact Candidate material, activated occupancy, and a non-current slot pointer'
            USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'credential_abandon_prepared'
       AND (intent_row.bound_receipt IS NOT NULL OR prepared_count <> 0 OR attachment_count <> 0
            OR candidate_count <> 0 OR cancellation_count <> 1 OR erasure_receipt_count > 1
            OR occupancy_row.state <> 'cancelled') THEN
        RAISE EXCEPTION 'CredentialAbandonPrepared is inconsistent' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'abandoned'
       AND (intent_row.bound_receipt IS NOT NULL OR prepared_count <> 0 OR attachment_count <> 0
            OR candidate_count <> 0 OR cancellation_count <> 1 OR erasure_receipt_count <> 1
            OR occupancy_row.state <> 'destroyed') THEN
        RAISE EXCEPTION 'Abandoned requires cancellation and witnessed unbound-key erasure'
            USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER credential_guard_occupancies_deferred_intent_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_guard_occupancies
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
CREATE CONSTRAINT TRIGGER credential_key_creation_intents_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_key_creation_intents
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
CREATE CONSTRAINT TRIGGER credential_prepared_materials_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_prepared_materials
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
CREATE CONSTRAINT TRIGGER credential_prepared_attachments_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_prepared_attachments
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
CREATE CONSTRAINT TRIGGER credential_association_events_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_association_events
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
CREATE CONSTRAINT TRIGGER credential_lifecycle_events_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_lifecycle_events
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
CREATE CONSTRAINT TRIGGER credential_key_creation_intent_erasure_receipts_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON credential_key_creation_intent_erasure_receipts
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_credential_key_creation_intent();
