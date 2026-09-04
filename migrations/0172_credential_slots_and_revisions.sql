-- The stable slot contains only an identity, CAS current-revision fields, and
-- its tombstone. Ciphertext belongs to later guarded credential material rows.
CREATE TABLE credential_slots (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL REFERENCES connections(id) ON DELETE RESTRICT,
    purpose TEXT NOT NULL CHECK (length(btrim(purpose)) > 0),
    name TEXT NOT NULL CHECK (length(btrim(name)) > 0),
    current_revision_id UUID,
    current_revision_version BIGINT NOT NULL DEFAULT 0 CHECK (current_revision_version >= 0),
    tombstone_version BIGINT CHECK (tombstone_version >= 0),
    tombstoned_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_slots_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT credential_slots_workspace_connection_purpose_name_key
        UNIQUE (workspace_id, connection_id, purpose, name)
);

CREATE TABLE credential_activation_guards (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    connection_id UUID NOT NULL,
    credential_slot_id UUID NOT NULL,
    execution_guard_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_activation_guards_execution_guard_fkey
        FOREIGN KEY (execution_guard_id, workspace_id)
        REFERENCES connection_execution_guards(id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_activation_guards_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_activation_guards_workspace_connection_slot_key
        UNIQUE (workspace_id, connection_id, credential_slot_id),
    CONSTRAINT credential_activation_guards_id_workspace_connection_slot_key
        UNIQUE (id, workspace_id, connection_id, credential_slot_id)
);

CREATE TABLE credential_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    credential_slot_id UUID NOT NULL,
    material_key_id UUID NOT NULL UNIQUE,
    associated_data_profile TEXT NOT NULL
        CHECK (associated_data_profile IN ('credential_v2', 'legacy_v1')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_revisions_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id)
        ON DELETE RESTRICT,
    CONSTRAINT credential_revisions_workspace_id_id_key UNIQUE (workspace_id, id)
);

ALTER TABLE credential_slots
    ADD CONSTRAINT credential_slots_current_revision_fkey
    FOREIGN KEY (workspace_id, current_revision_id)
    REFERENCES credential_revisions(workspace_id, id)
    ON DELETE RESTRICT;

-- This is the durable occupancy authority. `activated` retains the completed
-- association without occupying the preparation guard; it is not destruction.
-- The later P03 intent invariant and singleton index define active liveness.
CREATE TABLE credential_guard_occupancies (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    connection_id UUID NOT NULL,
    credential_slot_id UUID NOT NULL,
    activation_guard_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('preparing', 'candidate', 'activated', 'cancelled', 'destroyed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credential_guard_occupancies_activation_guard_fkey
        FOREIGN KEY (activation_guard_id, workspace_id, connection_id, credential_slot_id)
        REFERENCES credential_activation_guards(
            id, workspace_id, connection_id, credential_slot_id
        )
        ON DELETE RESTRICT
);

CREATE UNIQUE INDEX credential_guard_occupancies_one_live_candidate_key
    ON credential_guard_occupancies (activation_guard_id)
    WHERE state IN ('preparing', 'candidate');

ALTER TABLE credential_slots ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_slots FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_activation_guards ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_activation_guards FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_guard_occupancies ENABLE ROW LEVEL SECURITY;
ALTER TABLE credential_guard_occupancies FORCE ROW LEVEL SECURITY;

CREATE POLICY credential_slots_workspace_policy
    ON credential_slots
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_activation_guards_workspace_policy
    ON credential_activation_guards
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_revisions_workspace_policy
    ON credential_revisions
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_guard_occupancies_workspace_policy
    ON credential_guard_occupancies
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);


CREATE OR REPLACE FUNCTION vestrace_reject_raw_credential_guard_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' THEN
        RAISE EXCEPTION 'credential guard state changes require a guarded operation'
            USING ERRCODE = '42501';
    END IF;

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reject_raw_credential_revision_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' OR TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'credential revisions are immutable guarded metadata'
            USING ERRCODE = '42501';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER credential_slots_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_slots
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_guard_mutation();
CREATE TRIGGER credential_activation_guards_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_activation_guards
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_guard_mutation();
CREATE TRIGGER credential_revisions_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_revisions
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_revision_mutation();
CREATE TRIGGER credential_guard_occupancies_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON credential_guard_occupancies
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_credential_guard_mutation();

CREATE OR REPLACE FUNCTION vestrace_reserve_credential_slot(
    target_slot_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_purpose TEXT,
    target_name TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    PERFORM vestrace_assert_credential_guard_workspace(target_workspace_id);
    PERFORM 1
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential slot requires its connection execution guard'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO credential_slots (
        id, workspace_id, connection_id, purpose, name
    )
    VALUES (
        target_slot_id, target_workspace_id, target_connection_id, target_purpose, target_name
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_ensure_credential_activation_guard(
    target_guard_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_slot_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_execution_guard_id UUID;
    existing_guard_id UUID;
BEGIN
    PERFORM vestrace_assert_credential_guard_workspace(target_workspace_id);
    SELECT id
      INTO target_execution_guard_id
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential activation guard requires its connection execution guard'
            USING ERRCODE = '23514';
    END IF;

    SELECT id
      INTO existing_guard_id
      FROM credential_activation_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
     FOR UPDATE;
    IF FOUND THEN
        RETURN existing_guard_id;
    END IF;

    PERFORM 1
      FROM credential_slots
     WHERE workspace_id = target_workspace_id
       AND id = target_slot_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential activation guard requires its credential slot'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO credential_activation_guards (
        id, workspace_id, connection_id, credential_slot_id, execution_guard_id
    )
    VALUES (
        target_guard_id,
        target_workspace_id,
        target_connection_id,
        target_slot_id,
        target_execution_guard_id
    );
    RETURN target_guard_id;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_acquire_credential_lock_chain(
    target_workspace_id UUID,
    target_connection_id UUID,
    target_slot_id UUID,
    requested_lock_order TEXT[]
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    activation_guard_id UUID;
BEGIN
    IF requested_lock_order IS DISTINCT FROM ARRAY[
        'connection_execution_guard',
        'credential_activation_guard',
        'credential_slot',
        'revision_material'
    ]::TEXT[] THEN
        RAISE EXCEPTION 'credential lock order is not canonical'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_assert_credential_guard_workspace(target_workspace_id);

    PERFORM 1
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'connection execution guard is absent' USING ERRCODE = '23514';
    END IF;

    SELECT id
      INTO activation_guard_id
      FROM credential_activation_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential activation guard is absent' USING ERRCODE = '23514';
    END IF;

    PERFORM 1
      FROM credential_slots
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND id = target_slot_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'credential slot is absent' USING ERRCODE = '23514';
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reserve_credential_preparing_occupancy(
    target_occupancy_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_slot_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    target_activation_guard_id UUID;
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

    SELECT id
      INTO target_activation_guard_id
      FROM credential_activation_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id;

    INSERT INTO credential_guard_occupancies (
        id,
        workspace_id,
        connection_id,
        credential_slot_id,
        activation_guard_id,
        state
    )
    VALUES (
        target_occupancy_id,
        target_workspace_id,
        target_connection_id,
        target_slot_id,
        target_activation_guard_id,
        'preparing'
    );
END
$$;
