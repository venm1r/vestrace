-- Prepared material is deliberately not an ordinary content reference. The
-- only publication operation is the Bound finalizer below.
CREATE TABLE content_materials (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    material_key_id UUID NOT NULL,
    size_class BIGINT NOT NULL CHECK (
        size_class >= 4096 AND (size_class & (size_class - 1)) = 0
    ),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'abandon_prepared', 'live', 'abandoned')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (id, workspace_id),
    CONSTRAINT content_materials_intent_fkey
        FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT
);

CREATE TABLE content_material_bytes (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    material_id UUID NOT NULL UNIQUE,
    ciphertext BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT content_material_bytes_intent_fkey
        FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT content_material_bytes_material_fkey
        FOREIGN KEY (material_id, workspace_id)
        REFERENCES content_materials(id, workspace_id)
        ON DELETE RESTRICT
);

CREATE TABLE prepared_material_attachments (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    intent_id UUID NOT NULL UNIQUE,
    material_id UUID NOT NULL UNIQUE,
    marker TEXT NOT NULL CHECK (marker IN ('content_prepared', 'result_prepared')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT prepared_material_attachments_intent_fkey
        FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT prepared_material_attachments_material_fkey
        FOREIGN KEY (material_id, workspace_id)
        REFERENCES content_materials(id, workspace_id)
        ON DELETE RESTRICT
);

CREATE TABLE content_material_ordinary_references (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    material_id UUID NOT NULL UNIQUE,
    intent_id UUID NOT NULL UNIQUE,
    owner_kind TEXT NOT NULL CHECK (length(btrim(owner_kind)) > 0),
    owner_id UUID NOT NULL,
    output_ordinal BIGINT NOT NULL CHECK (output_ordinal >= 0),
    promoted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT content_material_ordinary_references_intent_fkey
        FOREIGN KEY (intent_id, workspace_id)
        REFERENCES material_key_creation_intents(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT content_material_ordinary_references_material_fkey
        FOREIGN KEY (material_id, workspace_id)
        REFERENCES content_materials(id, workspace_id)
        ON DELETE RESTRICT
);

ALTER TABLE content_materials ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_materials FORCE ROW LEVEL SECURITY;
ALTER TABLE content_material_bytes ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_material_bytes FORCE ROW LEVEL SECURITY;
ALTER TABLE prepared_material_attachments ENABLE ROW LEVEL SECURITY;
ALTER TABLE prepared_material_attachments FORCE ROW LEVEL SECURITY;
ALTER TABLE content_material_ordinary_references ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_material_ordinary_references FORCE ROW LEVEL SECURITY;

CREATE POLICY content_materials_workspace_policy ON content_materials
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY content_material_bytes_workspace_policy ON content_material_bytes
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY prepared_material_attachments_workspace_policy ON prepared_material_attachments
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY content_material_ordinary_references_workspace_policy
    ON content_material_ordinary_references
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);


CREATE TRIGGER content_materials_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON content_materials
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_intent_mutation();
CREATE TRIGGER content_material_bytes_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON content_material_bytes
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_intent_mutation();
CREATE TRIGGER prepared_material_attachments_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON prepared_material_attachments
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_intent_mutation();
CREATE TRIGGER content_material_ordinary_references_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON content_material_ordinary_references
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_material_intent_mutation();

CREATE OR REPLACE FUNCTION vestrace_prepare_material_key_content(
    target_intent_id UUID,
    target_attachment_id UUID,
    target_ciphertext BYTEA,
    target_size_class BIGINT,
    target_marker TEXT
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
    IF intent_row.state <> 'provisional_receipted' OR intent_row.vault_receipt IS NULL THEN
        RAISE EXCEPTION 'material key creation intent must be ProvisionalReceipted'
            USING ERRCODE = '23514';
    END IF;
    IF target_marker NOT IN ('content_prepared', 'result_prepared') THEN
        RAISE EXCEPTION 'material preparation marker is invalid' USING ERRCODE = '23514';
    END IF;
    IF target_size_class < 4096
       OR (target_size_class & (target_size_class - 1)) <> 0
       OR octet_length(target_ciphertext) <> target_size_class THEN
        RAISE EXCEPTION 'ciphertext must use its disclosed padded size class'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO content_materials (
        id, workspace_id, intent_id, material_key_id, size_class, state
    )
    VALUES (
        intent_row.material_id,
        intent_row.workspace_id,
        intent_row.id,
        intent_row.material_key_id,
        target_size_class,
        'prepared'
    );
    INSERT INTO content_material_bytes (
        id, workspace_id, intent_id, material_id, ciphertext
    )
    VALUES (
        gen_random_uuid(),
        intent_row.workspace_id,
        intent_row.id,
        intent_row.material_id,
        target_ciphertext
    );
    INSERT INTO prepared_material_attachments (
        id, workspace_id, intent_id, material_id, marker
    )
    VALUES (
        target_attachment_id,
        intent_row.workspace_id,
        intent_row.id,
        intent_row.material_id,
        target_marker
    );
    UPDATE material_key_creation_intents
       SET state = target_marker,
           prepared_marker = target_marker,
           updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_prepare_content_material(
    target_intent_id UUID,
    target_attachment_id UUID,
    target_ciphertext BYTEA,
    target_size_class BIGINT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM vestrace_prepare_material_key_content(
        target_intent_id,
        target_attachment_id,
        target_ciphertext,
        target_size_class,
        'content_prepared'
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_prepare_result_material(
    target_intent_id UUID,
    target_attachment_id UUID,
    target_ciphertext BYTEA,
    target_size_class BIGINT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM vestrace_prepare_material_key_content(
        target_intent_id,
        target_attachment_id,
        target_ciphertext,
        target_size_class,
        'result_prepared'
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_prepare_content_abandon(
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
    IF intent_row.state <> 'content_prepared'
       OR intent_row.prepared_marker <> 'content_prepared' THEN
        RAISE EXCEPTION 'only ContentPrepared may enter ContentAbandonPrepared'
            USING ERRCODE = '23514';
    END IF;

    DELETE FROM content_material_bytes WHERE intent_id = intent_row.id;
    DELETE FROM prepared_material_attachments WHERE intent_id = intent_row.id;
    UPDATE content_materials
       SET state = 'abandon_prepared', updated_at = NOW()
     WHERE intent_id = intent_row.id;
    UPDATE material_key_creation_intents
       SET state = 'content_abandon_prepared', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_bind_material_key_creation_intent(
    target_intent_id UUID,
    target_bound_receipt UUID
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
    IF intent_row.state NOT IN ('content_prepared', 'result_prepared') THEN
        RAISE EXCEPTION 'only prepared content may bind a material key'
            USING ERRCODE = '23514';
    END IF;

    UPDATE material_key_creation_intents
       SET state = 'bound', bound_receipt = target_bound_receipt, updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_bound_content_material(
    target_intent_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    intent_row material_key_creation_intents%ROWTYPE;
    material_state TEXT;
    attachment_marker TEXT;
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
    IF intent_row.state <> 'bound' OR intent_row.bound_receipt IS NULL THEN
        RAISE EXCEPTION 'only Bound may finalize content material publication'
            USING ERRCODE = '23514';
    END IF;

    SELECT state INTO material_state
      FROM content_materials
     WHERE intent_id = intent_row.id
     FOR UPDATE;
    SELECT marker INTO attachment_marker
      FROM prepared_material_attachments
     WHERE intent_id = intent_row.id
     FOR UPDATE;
    IF material_state <> 'prepared'
       OR attachment_marker <> intent_row.prepared_marker
       OR NOT EXISTS (SELECT 1 FROM content_material_bytes WHERE intent_id = intent_row.id) THEN
        RAISE EXCEPTION 'Bound finalizer requires the exact prepared attachment and ciphertext'
            USING ERRCODE = '23514';
    END IF;

    UPDATE content_materials
       SET state = 'live', updated_at = NOW()
     WHERE intent_id = intent_row.id;
    DELETE FROM prepared_material_attachments WHERE intent_id = intent_row.id;
    INSERT INTO content_material_ordinary_references (
        id, workspace_id, material_id, intent_id, owner_kind, owner_id, output_ordinal
    )
    VALUES (
        gen_random_uuid(),
        intent_row.workspace_id,
        intent_row.material_id,
        intent_row.id,
        intent_row.owner_kind,
        intent_row.owner_id,
        intent_row.output_ordinal
    );
    UPDATE material_key_creation_intents
       SET state = 'live', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_finalize_material_key_abandon(
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
    -- Abandoned terminalizes both abort branches. The ordinary one carries a
    -- ContentPrepared marker and a material row to retire; the pre-prepared one
    -- reached no marker at all, so it has neither, and demanding them would
    -- strand exactly the intents that branch exists for.
    IF NOT (
        (intent_row.state = 'content_abandon_prepared'
         AND intent_row.prepared_marker = 'content_prepared')
        OR (intent_row.state = 'pre_prepared_abandon_prepared'
            AND intent_row.prepared_marker IS NULL)
    ) THEN
        RAISE EXCEPTION
            'only a committed pre-prepared or ContentAbandonPrepared abort may finalize abandonment'
            USING ERRCODE = '23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM material_key_creation_intent_erasure_receipts
        WHERE intent_id = intent_row.id
    ) THEN
        RAISE EXCEPTION 'abandonment requires a witnessed unbound-key erase receipt'
            USING ERRCODE = '23514';
    END IF;
    IF EXISTS (SELECT 1 FROM content_material_bytes WHERE intent_id = intent_row.id)
       OR EXISTS (SELECT 1 FROM prepared_material_attachments WHERE intent_id = intent_row.id)
       OR EXISTS (SELECT 1 FROM content_material_ordinary_references WHERE intent_id = intent_row.id) THEN
        RAISE EXCEPTION 'abandonment requires removal of ciphertext and prepared attachment'
            USING ERRCODE = '23514';
    END IF;

    -- Absent on the pre-prepared branch, which never created one.
    UPDATE content_materials
       SET state = 'abandoned', updated_at = NOW()
     WHERE intent_id = intent_row.id;
    UPDATE material_key_creation_intents
       SET state = 'abandoned', updated_at = NOW()
     WHERE id = intent_row.id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_content_material_is_live(
    target_material_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM content_materials AS material
        JOIN material_key_creation_intents AS intent ON intent.id = material.intent_id
        JOIN content_material_ordinary_references AS reference ON reference.intent_id = intent.id
        WHERE material.id = target_material_id
          AND material.state = 'live'
          AND intent.state = 'live'
          AND material.workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID
    );
$$;

CREATE OR REPLACE FUNCTION vestrace_validate_material_key_creation_intent()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_intent_id UUID;
    intent_row material_key_creation_intents%ROWTYPE;
    material_row content_materials%ROWTYPE;
    attachment_marker TEXT;
    byte_count BIGINT;
    attachment_count BIGINT;
    ordinary_reference_count BIGINT;
    erasure_receipt_count BIGINT;
    ciphertext_size BIGINT;
BEGIN
    IF TG_TABLE_NAME = 'material_key_creation_intents' THEN
        target_intent_id := COALESCE(NEW.id, OLD.id);
    ELSE
        target_intent_id := COALESCE(NEW.intent_id, OLD.intent_id);
    END IF;

    SELECT * INTO intent_row
      FROM material_key_creation_intents
     WHERE id = target_intent_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;

    SELECT * INTO material_row
      FROM content_materials
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*), MAX(octet_length(ciphertext))
      INTO byte_count, ciphertext_size
      FROM content_material_bytes
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*), MAX(marker)
      INTO attachment_count, attachment_marker
      FROM prepared_material_attachments
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO ordinary_reference_count
      FROM content_material_ordinary_references
     WHERE intent_id = target_intent_id;
    SELECT COUNT(*) INTO erasure_receipt_count
      FROM material_key_creation_intent_erasure_receipts
     WHERE intent_id = target_intent_id;

    IF intent_row.state IN ('reserved', 'provisional_created')
       AND (intent_row.vault_receipt IS NOT NULL OR intent_row.bound_receipt IS NOT NULL
            OR material_row.id IS NOT NULL OR byte_count <> 0 OR attachment_count <> 0
            OR ordinary_reference_count <> 0 OR erasure_receipt_count <> 0
            OR intent_row.prepared_marker IS NOT NULL) THEN
        RAISE EXCEPTION 'pre-provisional-receipt intent has dependent material state'
            USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'provisional_receipted'
       AND (intent_row.vault_receipt IS NULL OR intent_row.bound_receipt IS NOT NULL
            OR material_row.id IS NOT NULL OR byte_count <> 0 OR attachment_count <> 0
            OR ordinary_reference_count <> 0 OR erasure_receipt_count <> 0
            OR intent_row.prepared_marker IS NOT NULL) THEN
        RAISE EXCEPTION 'ProvisionalReceipted intent is inconsistent' USING ERRCODE = '23514';
    END IF;

    IF intent_row.state IN ('content_prepared', 'result_prepared', 'bound')
       AND (
           intent_row.vault_receipt IS NULL
           OR material_row.id IS NULL
           OR material_row.id <> intent_row.material_id
           OR material_row.workspace_id <> intent_row.workspace_id
           OR material_row.material_key_id <> intent_row.material_key_id
           OR material_row.state <> 'prepared'
           OR byte_count <> 1
           OR attachment_count <> 1
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count <> 0
           OR ciphertext_size <> material_row.size_class
           OR attachment_marker <> intent_row.prepared_marker
           OR (intent_row.state = 'bound' AND intent_row.bound_receipt IS NULL)
           OR (intent_row.state <> 'bound' AND intent_row.bound_receipt IS NOT NULL)
       ) THEN
        RAISE EXCEPTION 'prepared or Bound material-key creation intent is inconsistent'
            USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'content_prepared'
       AND intent_row.prepared_marker <> 'content_prepared' THEN
        RAISE EXCEPTION 'ContentPrepared must retain its content marker' USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'result_prepared'
       AND intent_row.prepared_marker <> 'result_prepared' THEN
        RAISE EXCEPTION 'ResultPrepared must retain its result marker' USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'content_abandon_prepared'
       AND (
           intent_row.prepared_marker <> 'content_prepared'
           OR intent_row.bound_receipt IS NOT NULL
           OR material_row.id IS NULL
           OR material_row.state <> 'abandon_prepared'
           OR byte_count <> 0
           OR attachment_count <> 0
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count > 1
       ) THEN
        RAISE EXCEPTION 'ContentAbandonPrepared is inconsistent' USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'live'
       AND (
           intent_row.vault_receipt IS NULL
           OR intent_row.bound_receipt IS NULL
           OR material_row.id IS NULL
           OR material_row.state <> 'live'
           OR byte_count <> 1
           OR attachment_count <> 0
           OR ordinary_reference_count <> 1
           OR erasure_receipt_count <> 0
           OR ciphertext_size <> material_row.size_class
       ) THEN
        RAISE EXCEPTION 'Live material requires its exact Bound promotion' USING ERRCODE = '23514';
    END IF;

    -- The pre-prepared abort is taken before any marker exists, so it carries
    -- no prepared_marker and no material row. It is a distinct branch, not a
    -- ContentPrepared abort with pieces missing, and the spec refuses to let a
    -- ContentPrepared-only marker stand in for it.
    IF intent_row.state = 'pre_prepared_abandon_prepared'
       AND (
           intent_row.prepared_marker IS NOT NULL
           OR intent_row.bound_receipt IS NOT NULL
           OR material_row.id IS NOT NULL
           OR byte_count <> 0
           OR attachment_count <> 0
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count > 1
       ) THEN
        RAISE EXCEPTION 'a pre-prepared abort must carry no prepared, result or live material'
            USING ERRCODE = '23514';
    END IF;

    IF intent_row.state = 'abandoned'
       AND (
           intent_row.bound_receipt IS NOT NULL
           OR byte_count <> 0
           OR attachment_count <> 0
           OR ordinary_reference_count <> 0
           OR erasure_receipt_count <> 1
       ) THEN
        RAISE EXCEPTION 'Abandoned material requires a committed abort branch and its receipt'
            USING ERRCODE = '23514';
    END IF;

    -- Which branch produced the Abandoned decides what must remain: the
    -- ordinary one retires its material row, the pre-prepared one never had one.
    IF intent_row.state = 'abandoned'
       AND intent_row.prepared_marker IS NOT NULL
       AND (
           intent_row.prepared_marker <> 'content_prepared'
           OR material_row.id IS NULL
           OR material_row.state <> 'abandoned'
       ) THEN
        RAISE EXCEPTION 'an ordinary Abandoned requires its retired ContentPrepared material'
            USING ERRCODE = '23514';
    END IF;
    IF intent_row.state = 'abandoned'
       AND intent_row.prepared_marker IS NULL
       AND material_row.id IS NOT NULL THEN
        RAISE EXCEPTION 'a pre-prepared Abandoned must have no material row'
            USING ERRCODE = '23514';
    END IF;

    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER material_key_creation_intents_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON material_key_creation_intents
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_key_creation_intent();
CREATE CONSTRAINT TRIGGER material_key_creation_intent_erasure_receipts_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON material_key_creation_intent_erasure_receipts
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_key_creation_intent();
CREATE CONSTRAINT TRIGGER content_materials_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON content_materials
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_key_creation_intent();
CREATE CONSTRAINT TRIGGER content_material_bytes_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON content_material_bytes
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_key_creation_intent();
CREATE CONSTRAINT TRIGGER prepared_material_attachments_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON prepared_material_attachments
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_key_creation_intent();
CREATE CONSTRAINT TRIGGER content_material_ordinary_references_deferred_invariant
    AFTER INSERT OR UPDATE OR DELETE ON content_material_ordinary_references
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION vestrace_validate_material_key_creation_intent();
