-- The role is created by the Compose bootstrap before ordinary migrations run.
-- Keeping this idempotent also makes an independently provisioned database
-- converge when its migration runner is privileged enough to create roles.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vestrace_guarded_owner') THEN
        BEGIN
            CREATE ROLE vestrace_guarded_owner
                NOLOGIN
                NOSUPERUSER
                NOCREATEDB
                NOCREATEROLE
                NOINHERIT
                NOREPLICATION
                NOBYPASSRLS;
        EXCEPTION
            WHEN duplicate_object OR unique_violation THEN NULL;
        END;
    END IF;
END
$$;

-- The runtime role never assumes or inherits the guarded owner's privileges.
-- Bootstrap provisions the bounded SECURITY DEFINER ownership bridge instead.
DO $$
BEGIN
    IF NOT pg_has_role('vestrace', 'vestrace_guarded_owner', 'MEMBER') THEN
        BEGIN
            GRANT vestrace_guarded_owner TO vestrace
                WITH ADMIN FALSE, INHERIT FALSE, SET FALSE;
        EXCEPTION
            WHEN duplicate_object OR unique_violation THEN NULL;
        END;
    END IF;
END
$$;

CREATE TABLE p02_guarded_operation_probe (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE OR REPLACE FUNCTION vestrace_record_guarded_operation_probe(probe_id UUID)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    INSERT INTO p02_guarded_operation_probe (id) VALUES (probe_id);
END
$$;

CREATE OR REPLACE FUNCTION vestrace_record_governed_mutation_audit_mark(
    marker_id UUID,
    marker_workspace_id UUID,
    marker_audit_event_id UUID,
    marker_created_at TIMESTAMPTZ
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    INSERT INTO governed_mutation_audit_marks
        (id, workspace_id, audit_event_id, created_at)
    VALUES
        (marker_id, marker_workspace_id, marker_audit_event_id, marker_created_at);
END
$$;
