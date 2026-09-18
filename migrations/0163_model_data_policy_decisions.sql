-- A model data-policy decision is deployment evidence captured at the exact
-- boundary where one configured model client is about to receive a run
-- objective. It records what the deployment decided from its policy and the
-- adapter-owned egress descriptor; it does not store the objective or model
-- response, so it is not tenant content. Run and step ids bind the observation
-- to the work it governed, but deliberately have no foreign keys: deployment
-- evidence must survive later cleanup of tenant run projections.
--
-- The table is therefore not workspace-scoped and has no RLS policy. Database
-- access is the evidence collection boundary, following recovery qualification
-- observations in 0162. Adding workspace_id would imply that a tenant owns and
-- may administer the deployment's proof that its configured boundary ran.
--
-- The row is committed before the provider is called. Rewriting an allowance
-- after disclosure, or deleting a denial observed during rollout, would make
-- the record agree with a later story rather than the event it witnessed.
-- UPDATE and DELETE are therefore trigger-refused, following migration 0160's
-- append-only evidence pattern.

CREATE TABLE model_data_policy_decisions (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL,
    step_id UUID NOT NULL,
    destination TEXT NOT NULL,
    classification TEXT NOT NULL,
    verdict TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    policy_version TEXT NOT NULL CHECK (btrim(policy_version) <> ''),
    mode TEXT NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT model_data_policy_destination_known CHECK (
        destination IN ('local_model', 'remote_provider')
    ),
    CONSTRAINT model_data_policy_classification_known CHECK (
        classification IN ('public', 'internal', 'confidential', 'restricted')
    ),
    CONSTRAINT model_data_policy_verdict_known CHECK (
        verdict IN ('allowed', 'denied')
    ),
    CONSTRAINT model_data_policy_mode_known CHECK (
        mode IN ('enforce', 'observe')
    )
);

CREATE INDEX idx_model_data_policy_decisions_run_step
    ON model_data_policy_decisions (run_id, step_id, decided_at, id);

CREATE OR REPLACE FUNCTION vestrace_prevent_model_data_policy_decision_modification()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'model data-policy decisions are append-only'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER tr_model_data_policy_decisions_no_update
    BEFORE UPDATE ON model_data_policy_decisions
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_model_data_policy_decision_modification();

CREATE TRIGGER tr_model_data_policy_decisions_no_delete
    BEFORE DELETE ON model_data_policy_decisions
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_model_data_policy_decision_modification();
