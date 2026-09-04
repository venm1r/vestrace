-- Every content material begins with an opaque, fixed identity and a
-- provisional host-vault key. PostgreSQL records only the identity, nonce, and
-- receipts; it never records a data-encryption key.
CREATE TABLE material_key_creation_intents (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    material_id UUID NOT NULL UNIQUE,
    material_key_id UUID NOT NULL UNIQUE,
    nonce UUID NOT NULL UNIQUE,
    owner_kind TEXT NOT NULL CHECK (length(btrim(owner_kind)) > 0),
    owner_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal >= 0),
    state TEXT NOT NULL CHECK (state IN (
        'reserved',
        'provisional_created',
        'provisional_receipted',
        'content_prepared',
        'result_prepared',
        'content_abandon_prepared',
        'bound',
        'live',
        'abandoned'
    )),
    prepared_marker TEXT CHECK (prepared_marker IN ('content_prepared', 'result_prepared')),
    vault_receipt UUID,
    bound_receipt UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (id, workspace_id)
);

CREATE TABLE material_key_creation_intent_erasure_receipts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    erasure_receipt UUID NOT NULL UNIQUE,
    witnessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT material_key_creation_intent_erasure_receipts_intent_fkey
        FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT
);

ALTER TABLE material_key_creation_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE material_key_creation_intents FORCE ROW LEVEL SECURITY;
ALTER TABLE material_key_creation_intent_erasure_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE material_key_creation_intent_erasure_receipts FORCE ROW LEVEL SECURITY;

CREATE POLICY material_key_creation_intents_workspace_policy
    ON material_key_creation_intents
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

CREATE POLICY material_key_creation_intent_erasure_receipts_workspace_policy
    ON material_key_creation_intent_erasure_receipts
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);


CREATE OR REPLACE FUNCTION vestrace_reject_raw_material_intent_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' THEN
        RAISE EXCEPTION 'material intent state changes require a guarded operation'
            USING ERRCODE = '42501';
    END IF;

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER material_key_creation_intents_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON material_key_creation_intents
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_intent_mutation();

CREATE TRIGGER material_key_creation_intent_erasure_receipts_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON material_key_creation_intent_erasure_receipts
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_intent_mutation();

CREATE OR REPLACE FUNCTION vestrace_assert_material_intent_workspace(
    target_workspace_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    configured_workspace_id TEXT;
BEGIN
    configured_workspace_id := NULLIF(current_setting('vestrace.workspace_id', true), '');
    IF configured_workspace_id IS NULL
       OR configured_workspace_id::UUID <> target_workspace_id THEN
        RAISE EXCEPTION 'material intent workspace context is required'
            USING ERRCODE = '42501';
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reserve_material_key_creation_intent(
    target_intent_id UUID,
    target_workspace_id UUID,
    target_material_id UUID,
    target_material_key_id UUID,
    target_nonce UUID,
    target_owner_kind TEXT,
    target_owner_id UUID,
    target_output_ordinal BIGINT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);

    INSERT INTO material_key_creation_intents (
        id,
        workspace_id,
        material_id,
        material_key_id,
        nonce,
        owner_kind,
        owner_id,
        output_ordinal,
        state
    )
    VALUES (
        target_intent_id,
        target_workspace_id,
        target_material_id,
        target_material_key_id,
        target_nonce,
        target_owner_kind,
        target_owner_id,
        target_output_ordinal,
        'reserved'
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_material_key_provisional_created(
    target_intent_id UUID
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
    SELECT workspace_id, state
      INTO target_workspace_id, target_state
      FROM material_key_creation_intents
     WHERE id = target_intent_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
    IF target_state <> 'reserved' THEN
        RAISE EXCEPTION 'material key creation intent must be Reserved' USING ERRCODE = '23514';
    END IF;

    UPDATE material_key_creation_intents
       SET state = 'provisional_created', updated_at = NOW()
     WHERE id = target_intent_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_material_key_provisional_receipt(
    target_intent_id UUID,
    target_vault_receipt UUID
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
    SELECT workspace_id, state
      INTO target_workspace_id, target_state
      FROM material_key_creation_intents
     WHERE id = target_intent_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
    IF target_state <> 'provisional_created' THEN
        RAISE EXCEPTION 'material key creation intent must be ProvisionalCreated'
            USING ERRCODE = '23514';
    END IF;

    UPDATE material_key_creation_intents
       SET state = 'provisional_receipted',
           vault_receipt = target_vault_receipt,
           updated_at = NOW()
     WHERE id = target_intent_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_unbound_material_key_erasure(
    target_intent_id UUID,
    target_erasure_receipt UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_workspace_id UUID;
    target_state TEXT;
    existing_receipt UUID;
BEGIN
    SELECT workspace_id, state
      INTO target_workspace_id, target_state
      FROM material_key_creation_intents
     WHERE id = target_intent_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'material key creation intent is absent' USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);
    -- Two committed abort branches admit an unbound-key erase receipt, and the
    -- spec keeps them distinct: the ordinary ContentPrepared abort, and the
    -- pre-prepared abort taken before any ContentPrepared, ResultPrepared or
    -- Bound marker exists. Widened in 0174, where the pre-prepared branch and
    -- its state are introduced.
    IF target_state NOT IN ('content_abandon_prepared', 'pre_prepared_abandon_prepared') THEN
        RAISE EXCEPTION
            'only a committed pre-prepared or ContentPrepared abort may record an unbound-key erase receipt'
            USING ERRCODE = '23514';
    END IF;

    SELECT erasure_receipt
      INTO existing_receipt
      FROM material_key_creation_intent_erasure_receipts
     WHERE intent_id = target_intent_id;
    IF FOUND AND existing_receipt <> target_erasure_receipt THEN
        RAISE EXCEPTION 'an unbound-key erase receipt is already recorded'
            USING ERRCODE = '23505';
    END IF;

    INSERT INTO material_key_creation_intent_erasure_receipts (
        id, workspace_id, intent_id, erasure_receipt
    )
    VALUES (
        gen_random_uuid(), target_workspace_id, target_intent_id, target_erasure_receipt
    )
    ON CONFLICT (intent_id) DO NOTHING;
END
$$;
