-- A ConnectionExecutionGuard is a stable, permanent serialization authority.
-- It is created exactly once for a (workspace, connection) pair and is never
-- a mutable pointer that could be reassigned to another connection.
CREATE TABLE connection_execution_guards (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    connection_id UUID NOT NULL REFERENCES connections(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT connection_execution_guards_workspace_connection_key
        UNIQUE (workspace_id, connection_id),
    CONSTRAINT connection_execution_guards_id_workspace_key
        UNIQUE (id, workspace_id)
);

ALTER TABLE connection_execution_guards ENABLE ROW LEVEL SECURITY;
ALTER TABLE connection_execution_guards FORCE ROW LEVEL SECURITY;

CREATE POLICY connection_execution_guards_workspace_policy
    ON connection_execution_guards
    USING (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('vestrace.workspace_id', true), '')::UUID);

GRANT SELECT ON connections TO vestrace_guarded_owner;

CREATE OR REPLACE FUNCTION vestrace_reject_raw_connection_execution_guard_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SET search_path = public, pg_temp
AS $$
BEGIN
    IF current_user <> 'vestrace_guarded_owner' THEN
        RAISE EXCEPTION 'connection execution guards require a guarded operation'
            USING ERRCODE = '42501';
    END IF;

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER connection_execution_guards_reject_raw_mutation
    BEFORE INSERT OR UPDATE OR DELETE ON connection_execution_guards
    FOR EACH ROW EXECUTE FUNCTION vestrace_reject_raw_connection_execution_guard_mutation();

CREATE OR REPLACE FUNCTION vestrace_assert_credential_guard_workspace(
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
        RAISE EXCEPTION 'credential guard workspace context is required'
            USING ERRCODE = '42501';
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_ensure_connection_execution_guard(
    target_guard_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    existing_guard_id UUID;
BEGIN
    PERFORM vestrace_assert_credential_guard_workspace(target_workspace_id);

    PERFORM 1
     FROM connections
     WHERE id = target_connection_id
       AND workspace_id = target_workspace_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'connection is absent from the credential guard workspace'
            USING ERRCODE = '23514';
    END IF;

    SELECT id
      INTO existing_guard_id
      FROM connection_execution_guards
     WHERE workspace_id = target_workspace_id
       AND connection_id = target_connection_id
     FOR UPDATE;
    IF FOUND THEN
        RETURN existing_guard_id;
    END IF;

    INSERT INTO connection_execution_guards (id, workspace_id, connection_id)
    VALUES (target_guard_id, target_workspace_id, target_connection_id);
    RETURN target_guard_id;
END
$$;
