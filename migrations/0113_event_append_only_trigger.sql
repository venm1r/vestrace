-- Migration: 0113_event_append_only_trigger.sql

CREATE OR REPLACE FUNCTION vestrace_prevent_event_modification()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'events table is append-only' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER tr_events_no_update
    BEFORE UPDATE ON events
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_event_modification();

CREATE TRIGGER tr_events_no_delete
    BEFORE DELETE ON events
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_event_modification();
