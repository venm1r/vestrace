-- A fault-suite row is an observation made at one point in time. The payload
-- may carry an evaluator verdict for historical inspection, but neither that
-- verdict nor the observations it accompanied may be rewritten after a newer
-- evaluator reaches a different conclusion.

CREATE OR REPLACE FUNCTION vestrace_prevent_fault_suite_evidence_modification()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'fault-suite evidence is append-only' USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER tr_fault_suite_evidence_no_update
    BEFORE UPDATE ON external_effect_fault_suite_evidence
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_fault_suite_evidence_modification();

CREATE TRIGGER tr_fault_suite_evidence_no_delete
    BEFORE DELETE ON external_effect_fault_suite_evidence
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_fault_suite_evidence_modification();
