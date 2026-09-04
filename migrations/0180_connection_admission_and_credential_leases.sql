CREATE TABLE connection_admission_policy_revisions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    max_in_flight SMALLINT NOT NULL CHECK (max_in_flight BETWEEN 1 AND 64),
    requests_per_60_seconds INTEGER NOT NULL CHECK (requests_per_60_seconds BETWEEN 1 AND 60000),
    queue_wait_timeout_seconds INTEGER NOT NULL CHECK (queue_wait_timeout_seconds BETWEEN 1 AND 300),
    provider_throttle_cap_seconds INTEGER NOT NULL CHECK (provider_throttle_cap_seconds BETWEEN 1 AND 900),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT connection_admission_policy_revisions_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_admission_policy_revisions_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT connection_admission_policy_revisions_connection_version_key
        UNIQUE (workspace_id, connection_id, version),
    CONSTRAINT connection_admission_policy_revisions_exact_identity_key
        UNIQUE (workspace_id, connection_id, id)
);

CREATE TABLE connection_admission_policy_heads (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    current_policy_revision_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, connection_id),
    CONSTRAINT connection_admission_policy_heads_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_admission_policy_heads_exact_policy_fkey
        FOREIGN KEY (workspace_id, connection_id, current_policy_revision_id)
        REFERENCES connection_admission_policy_revisions(workspace_id, connection_id, id)
        ON DELETE RESTRICT
);

CREATE TABLE connection_admission_states (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    window_started_at TIMESTAMPTZ NOT NULL,
    admitted_in_window INTEGER NOT NULL DEFAULT 0 CHECK (admitted_in_window >= 0),
    active_lease_count INTEGER NOT NULL DEFAULT 0 CHECK (active_lease_count >= 0),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, connection_id),
    CONSTRAINT connection_admission_states_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT
);

CREATE TABLE connection_dispatch_admissions (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    connection_revision_id UUID NOT NULL,
    policy_revision_id UUID NOT NULL,
    external_effect_id UUID NOT NULL UNIQUE,
    model_binding_snapshot_id UUID,
    qualification_target_binding_id UUID,
    decision TEXT NOT NULL CHECK (decision IN ('admitted', 'throttled', 'conflict', 'denied')),
    admitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT connection_dispatch_admissions_revision_fkey
        FOREIGN KEY (workspace_id, connection_id, connection_revision_id)
        REFERENCES connection_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_dispatch_admissions_exact_policy_fkey
        FOREIGN KEY (workspace_id, connection_id, policy_revision_id)
        REFERENCES connection_admission_policy_revisions(workspace_id, connection_id, id) ON DELETE RESTRICT,
    CONSTRAINT connection_dispatch_admissions_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT connection_dispatch_admissions_exact_snapshot_fkey
        FOREIGN KEY (
            workspace_id, connection_id, connection_revision_id, model_binding_snapshot_id
        ) REFERENCES model_binding_snapshots(
            workspace_id, connection_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT connection_dispatch_admissions_exact_target_fkey
        FOREIGN KEY (
            workspace_id, connection_id, connection_revision_id,
            qualification_target_binding_id
        ) REFERENCES qualification_target_bindings(
            workspace_id, connection_id, connection_revision_id, id
        ) ON DELETE RESTRICT,
    CONSTRAINT connection_dispatch_admissions_cause_xor CHECK (
        (model_binding_snapshot_id IS NOT NULL AND qualification_target_binding_id IS NULL)
        OR (model_binding_snapshot_id IS NULL AND qualification_target_binding_id IS NOT NULL)
    ),
    CONSTRAINT connection_dispatch_admissions_workspace_id_id_key UNIQUE (workspace_id, id)
);

CREATE TABLE provider_admission_waits (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL,
    deadline_at TIMESTAMPTZ NOT NULL,
    terminal_reason TEXT CHECK (terminal_reason IN ('admitted', 'timeout', 'cancelled')),
    terminal_at TIMESTAMPTZ,
    CONSTRAINT provider_admission_waits_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_admission_waits_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_admission_waits_terminal_pair CHECK (
        (terminal_reason IS NULL AND terminal_at IS NULL)
        OR (terminal_reason IS NOT NULL AND terminal_at IS NOT NULL)
    ),
    CONSTRAINT provider_admission_waits_effect_key UNIQUE (external_effect_id)
);

CREATE TABLE provider_concurrency_leases (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    external_effect_id UUID NOT NULL UNIQUE,
    slot_ordinal SMALLINT NOT NULL CHECK (slot_ordinal BETWEEN 0 AND 63),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ,
    CONSTRAINT provider_concurrency_leases_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_concurrency_leases_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_concurrency_leases_expiry CHECK (expires_at > issued_at),
    CONSTRAINT provider_concurrency_leases_release CHECK (released_at IS NULL OR released_at >= issued_at)
);
CREATE UNIQUE INDEX provider_concurrency_leases_one_live_declared_slot
    ON provider_concurrency_leases(workspace_id, connection_id, slot_ordinal)
    WHERE released_at IS NULL;

CREATE TABLE provider_throttle_observations (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    external_effect_id UUID NOT NULL,
    retry_after_seconds INTEGER NOT NULL CHECK (retry_after_seconds BETWEEN 1 AND 900),
    observed_at TIMESTAMPTZ NOT NULL,
    effective_until TIMESTAMPTZ NOT NULL,
    CONSTRAINT provider_throttle_observations_connection_fkey
        FOREIGN KEY (workspace_id, connection_id)
        REFERENCES connections(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT provider_throttle_observations_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT provider_throttle_observations_effect_key UNIQUE (external_effect_id),
    CONSTRAINT provider_throttle_observations_window CHECK (effective_until > observed_at)
);

CREATE TABLE credential_dispatch_leases (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL,
    external_effect_id UUID NOT NULL UNIQUE,
    authorization_id UUID NOT NULL UNIQUE,
    credential_revision_id UUID NOT NULL,
    credential_slot_id UUID NOT NULL,
    credential_activation_guard_id UUID NOT NULL,
    destination_authority TEXT NOT NULL CHECK (length(btrim(destination_authority)) > 0),
    auth_mode TEXT NOT NULL CHECK (auth_mode IN ('bearer', 'api_key', 'x_api_key')),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    terminal_state TEXT CHECK (terminal_state IN ('consumed_for_dispatch', 'expired', 'revoked')),
    CONSTRAINT credential_dispatch_leases_effect_fkey
        FOREIGN KEY (external_effect_id, workspace_id)
        REFERENCES external_effect_intents(id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT credential_dispatch_leases_exact_authorization_fkey
        FOREIGN KEY (authorization_id, external_effect_id, workspace_id)
        REFERENCES external_effect_authorizations(id, effect_id, workspace_id) ON DELETE RESTRICT,
    CONSTRAINT credential_dispatch_leases_revision_fkey
        FOREIGN KEY (workspace_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_dispatch_leases_slot_fkey
        FOREIGN KEY (workspace_id, credential_slot_id)
        REFERENCES credential_slots(workspace_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_dispatch_leases_exact_credential_fkey
        FOREIGN KEY (workspace_id, credential_slot_id, credential_revision_id)
        REFERENCES credential_revisions(workspace_id, credential_slot_id, id) ON DELETE RESTRICT,
    CONSTRAINT credential_dispatch_leases_guard_fkey
        FOREIGN KEY (
            credential_activation_guard_id, workspace_id, connection_id, credential_slot_id
        ) REFERENCES credential_activation_guards(
            id, workspace_id, connection_id, credential_slot_id
        ) ON DELETE RESTRICT,
    CONSTRAINT credential_dispatch_leases_expiry CHECK (expires_at > issued_at),
    CONSTRAINT credential_dispatch_leases_terminal_pair CHECK (
        (terminal_state IS NULL AND consumed_at IS NULL)
        OR (terminal_state = 'consumed_for_dispatch' AND consumed_at IS NOT NULL)
        OR (terminal_state IN ('expired', 'revoked') AND consumed_at IS NULL)
    )
);

-- This trigger makes terminal_state append-only: a live lease may advance once
-- from NULL to one terminal value, and no later UPDATE is accepted. The
-- terminal_pair CHECK separately guarantees the consumed_at shape for each
-- terminal value; it does not prevent a guarded writer from rewriting states.
CREATE OR REPLACE FUNCTION vestrace_reject_credential_dispatch_lease_state_rewrite()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF OLD.terminal_state IS NOT NULL OR NEW.terminal_state IS NULL THEN
        RAISE EXCEPTION 'credential dispatch lease terminal state is append-only'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_issue_credential_dispatch_lease(
    target_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_external_effect_id UUID,
    target_authorization_id UUID,
    target_credential_slot_id UUID,
    target_credential_revision_id UUID,
    target_activation_guard_id UUID,
    target_destination_authority TEXT,
    target_auth_mode TEXT,
    target_expires_at TIMESTAMPTZ
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    latest_effect_state TEXT;
    latest_effect_cause_ref TEXT;
BEGIN
    IF target_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_external_effect_id IS NULL
       OR target_authorization_id IS NULL OR target_credential_slot_id IS NULL
       OR target_credential_revision_id IS NULL OR target_activation_guard_id IS NULL
       OR target_destination_authority IS NULL OR target_auth_mode IS NULL
       OR target_expires_at IS NULL
       OR target_destination_authority <> lower(btrim(target_destination_authority))
       OR target_destination_authority = ''
       OR target_auth_mode NOT IN ('bearer', 'api_key', 'x_api_key') THEN
        RAISE EXCEPTION 'credential dispatch lease arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    IF target_expires_at <= NOW() THEN
        RAISE EXCEPTION 'credential dispatch lease must be issued live'
            USING ERRCODE = '23514';
    END IF;

    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, target_credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    IF NOT EXISTS (
        SELECT 1 FROM credential_activation_guards
         WHERE id = target_activation_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = target_credential_slot_id
    ) OR NOT EXISTS (
        SELECT 1 FROM credential_slots
         WHERE workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND id = target_credential_slot_id
           AND current_revision_id = target_credential_revision_id
           AND tombstone_version IS NULL
    ) OR NOT EXISTS (
        SELECT 1 FROM credential_key_creation_intents
         WHERE workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = target_credential_slot_id
           AND credential_revision_id = target_credential_revision_id
           AND state = 'active'
    ) THEN
        RAISE EXCEPTION 'credential dispatch lease requires the exact current Active credential'
            USING ERRCODE = '23514';
    END IF;

    PERFORM 1 FROM external_effect_intents
     WHERE id = target_external_effect_id AND workspace_id = target_workspace_id
     FOR UPDATE;
    IF NOT FOUND OR NOT EXISTS (
        SELECT 1 FROM external_effect_authorizations
         WHERE id = target_authorization_id
           AND effect_id = target_external_effect_id
           AND workspace_id = target_workspace_id
           AND result = 'allow'
    ) THEN
        RAISE EXCEPTION 'credential dispatch lease requires its exact allowed authorization'
            USING ERRCODE = '23514';
    END IF;
    SELECT status, cause_ref
      INTO latest_effect_state, latest_effect_cause_ref
      FROM external_effect_lifecycle_transitions
     WHERE effect_id = target_external_effect_id AND workspace_id = target_workspace_id
     ORDER BY ordinal DESC
     LIMIT 1
     FOR KEY SHARE;
    IF NOT FOUND OR latest_effect_state <> 'authorized'
       OR latest_effect_cause_ref <> target_authorization_id::TEXT THEN
        RAISE EXCEPTION 'credential dispatch lease requires an effect currently authorized by that decision'
            USING ERRCODE = '23514';
    END IF;

    INSERT INTO credential_dispatch_leases (
        id, workspace_id, connection_id, external_effect_id, authorization_id,
        credential_revision_id, credential_slot_id, credential_activation_guard_id,
        destination_authority, auth_mode, issued_at, expires_at
    ) VALUES (
        target_lease_id, target_workspace_id, target_connection_id,
        target_external_effect_id, target_authorization_id,
        target_credential_revision_id, target_credential_slot_id, target_activation_guard_id,
        target_destination_authority, target_auth_mode, NOW(), target_expires_at
    );
    RETURN target_lease_id;
END
$$;

-- Expiry is evaluated lazily here at consumption. That makes a stale lease
-- unusable without a sweeper or a second mutable state transition.
CREATE OR REPLACE FUNCTION vestrace_consume_credential_dispatch_lease(
    target_lease_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_external_effect_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    lease_row credential_dispatch_leases%ROWTYPE;
    latest_effect_state TEXT;
    latest_effect_cause_ref TEXT;
BEGIN
    IF target_lease_id IS NULL OR target_workspace_id IS NULL
       OR target_connection_id IS NULL OR target_external_effect_id IS NULL THEN
        RAISE EXCEPTION 'credential dispatch lease consumption arguments are malformed'
            USING ERRCODE = '22023';
    END IF;
    SELECT * INTO lease_row FROM credential_dispatch_leases
     WHERE id = target_lease_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND external_effect_id = target_external_effect_id
     FOR UPDATE;
    IF NOT FOUND OR lease_row.terminal_state IS NOT NULL THEN
        RAISE EXCEPTION 'credential dispatch lease is already terminal or absent'
            USING ERRCODE = '23514';
    END IF;
    IF lease_row.expires_at <= NOW() THEN
        RAISE EXCEPTION 'credential dispatch lease is expired'
            USING ERRCODE = '23514';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id, target_connection_id, lease_row.credential_slot_id,
        ARRAY['connection_execution_guard', 'credential_activation_guard', 'credential_slot', 'revision_material']::TEXT[]
    );
    IF NOT EXISTS (
        SELECT 1 FROM credential_activation_guards
         WHERE id = lease_row.credential_activation_guard_id
           AND workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = lease_row.credential_slot_id
    ) OR NOT EXISTS (
        SELECT 1 FROM credential_slots
         WHERE workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND id = lease_row.credential_slot_id
           AND current_revision_id = lease_row.credential_revision_id
           AND tombstone_version IS NULL
    ) OR NOT EXISTS (
        SELECT 1 FROM credential_key_creation_intents
         WHERE workspace_id = target_workspace_id
           AND connection_id = target_connection_id
           AND credential_slot_id = lease_row.credential_slot_id
           AND credential_revision_id = lease_row.credential_revision_id
           AND state = 'active'
    ) THEN
        RAISE EXCEPTION 'credential dispatch lease no longer names the current Active credential'
            USING ERRCODE = '23514';
    END IF;
    SELECT status, cause_ref
      INTO latest_effect_state, latest_effect_cause_ref
      FROM external_effect_lifecycle_transitions
     WHERE effect_id = target_external_effect_id AND workspace_id = target_workspace_id
     ORDER BY ordinal DESC
     LIMIT 1
     FOR KEY SHARE;
    IF NOT FOUND OR latest_effect_state <> 'authorized'
       OR latest_effect_cause_ref <> lease_row.authorization_id::TEXT THEN
        RAISE EXCEPTION 'credential dispatch lease effect is no longer authorized for dispatch'
            USING ERRCODE = '23514';
    END IF;

    UPDATE credential_dispatch_leases
       SET terminal_state = 'consumed_for_dispatch', consumed_at = NOW()
     WHERE id = lease_row.id;
    RETURN lease_row.id;
END
$$;

CREATE TRIGGER connection_admission_policy_heads_guarded BEFORE INSERT OR UPDATE OR DELETE ON connection_admission_policy_heads FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER connection_admission_states_guarded BEFORE INSERT OR UPDATE OR DELETE ON connection_admission_states FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER provider_admission_waits_guarded BEFORE INSERT OR UPDATE OR DELETE ON provider_admission_waits FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER provider_concurrency_leases_guarded BEFORE INSERT OR UPDATE OR DELETE ON provider_concurrency_leases FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER credential_dispatch_leases_guarded BEFORE INSERT OR UPDATE OR DELETE ON credential_dispatch_leases FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_p03_mutation();
CREATE TRIGGER credential_dispatch_leases_terminal_state_is_one_way BEFORE UPDATE ON credential_dispatch_leases FOR EACH ROW EXECUTE FUNCTION vestrace_reject_credential_dispatch_lease_state_rewrite();
CREATE TRIGGER connection_admission_policy_revisions_immutable BEFORE INSERT OR UPDATE OR DELETE ON connection_admission_policy_revisions FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER connection_dispatch_admissions_immutable BEFORE INSERT OR UPDATE OR DELETE ON connection_dispatch_admissions FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();
CREATE TRIGGER provider_throttle_observations_immutable BEFORE INSERT OR UPDATE OR DELETE ON provider_throttle_observations FOR EACH ROW EXECUTE FUNCTION vestrace_reject_p03_immutable_mutation();

ALTER TABLE connection_admission_policy_heads ENABLE ROW LEVEL SECURITY; ALTER TABLE connection_admission_policy_heads FORCE ROW LEVEL SECURITY;
ALTER TABLE connection_admission_policy_revisions ENABLE ROW LEVEL SECURITY; ALTER TABLE connection_admission_policy_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE connection_admission_states ENABLE ROW LEVEL SECURITY; ALTER TABLE connection_admission_states FORCE ROW LEVEL SECURITY;
ALTER TABLE connection_dispatch_admissions ENABLE ROW LEVEL SECURITY; ALTER TABLE connection_dispatch_admissions FORCE ROW LEVEL SECURITY;
ALTER TABLE provider_admission_waits ENABLE ROW LEVEL SECURITY; ALTER TABLE provider_admission_waits FORCE ROW LEVEL SECURITY;
ALTER TABLE provider_concurrency_leases ENABLE ROW LEVEL SECURITY; ALTER TABLE provider_concurrency_leases FORCE ROW LEVEL SECURITY;
ALTER TABLE provider_throttle_observations ENABLE ROW LEVEL SECURITY; ALTER TABLE provider_throttle_observations FORCE ROW LEVEL SECURITY;
ALTER TABLE credential_dispatch_leases ENABLE ROW LEVEL SECURITY; ALTER TABLE credential_dispatch_leases FORCE ROW LEVEL SECURITY;

CREATE POLICY connection_admission_policy_heads_workspace_policy ON connection_admission_policy_heads USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY connection_admission_policy_revisions_workspace_policy ON connection_admission_policy_revisions USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY connection_admission_states_workspace_policy ON connection_admission_states USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY connection_dispatch_admissions_workspace_policy ON connection_dispatch_admissions USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY provider_admission_waits_workspace_policy ON provider_admission_waits USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY provider_concurrency_leases_workspace_policy ON provider_concurrency_leases USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY provider_throttle_observations_workspace_policy ON provider_throttle_observations USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);
CREATE POLICY credential_dispatch_leases_workspace_policy ON credential_dispatch_leases USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID) WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

-- Lease issue/consume lock the exact effect while they inspect its latest
-- authorization state. This is authority for the guarded-owner functions,
-- never a direct runtime-table grant.
GRANT SELECT, UPDATE ON TABLE external_effect_intents TO vestrace_guarded_owner;
GRANT SELECT ON TABLE external_effect_authorizations TO vestrace_guarded_owner;
GRANT SELECT, UPDATE ON TABLE external_effect_lifecycle_transitions
    TO vestrace_guarded_owner;

SELECT vestrace_assign_p03_table_owner('connection_admission_policy_heads'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_admission_policy_revisions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_admission_states'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('connection_dispatch_admissions'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_admission_waits'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_concurrency_leases'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('provider_throttle_observations'::REGCLASS);
SELECT vestrace_assign_p03_table_owner('credential_dispatch_leases'::REGCLASS);
SELECT vestrace_assign_p03_function_owner('vestrace_reject_credential_dispatch_lease_state_rewrite()'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_issue_credential_dispatch_lease(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, TEXT, TIMESTAMPTZ)'::REGPROCEDURE);
SELECT vestrace_assign_p03_function_owner('vestrace_consume_credential_dispatch_lease(UUID, UUID, UUID, UUID)'::REGPROCEDURE);
