-- Migration: 0114_active_memory_source_invariant.sql

CREATE OR REPLACE FUNCTION vestrace_check_active_memory_source()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    source_count INT;
BEGIN
    IF NEW.status = 'active' THEN
        SELECT COUNT(*) INTO source_count
        FROM memory_sources
        WHERE memory_id = NEW.id
          AND workspace_id = NEW.workspace_id;

        IF source_count = 0 THEN
            RAISE EXCEPTION 'active memory must have at least one source'
                USING ERRCODE = '23001';
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER tr_active_memory_has_source
    AFTER INSERT OR UPDATE OF status, active_revision_id ON memories
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_check_active_memory_source();
