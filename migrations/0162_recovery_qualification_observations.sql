-- StartupRecoveryService already classified every interrupted run and returned
-- a StartupRecoveryRecord carrying its target, classification, and outcome.
-- The server and worker logged those records and then discarded them, so the
-- release gate could only report recovery_qualification_missing even after a
-- deployment had exercised real startup recovery.
--
-- This table preserves the observation, not an expected answer. In particular,
-- `action` is written from StartupRecoveryOutcome. It is intentionally stored
-- beside `classification`: deriving action from classification.expected_action()
-- would make the evaluator agree by construction and hide the unsafe disagreement
-- this evidence exists to reveal.
--
-- Every row is kept and list operations return all rows. There is no uniqueness
-- constraint on target and no query-time deduplication: zero observations and
-- two observations for one target are both facts the existing evaluator rejects.
-- A run id identifies what was recovered; there is deliberately no foreign key,
-- because qualification evidence must survive later cleanup of the run projection
-- it observed.
--
-- These are deployment qualification observations, like fault-suite evidence,
-- rather than tenant-operated records. The table therefore has no workspace_id,
-- RLS enablement, or workspace policy. Database access is the collection boundary.
--
-- An observation that can be rewritten after the fact is not evidence. UPDATE
-- and DELETE are both refused by triggers following migration 0160's pattern.

CREATE TABLE recovery_qualification_observations (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL,
    target TEXT NOT NULL,
    classification TEXT NOT NULL,
    action TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT recovery_qualification_target_known CHECK (
        target IN (
            'running_execution', 'dispatching_external_effect', 'verifying_repair',
            'stale_lease', 'unfinished_workflow', 'orphan_temporary_state',
            'unknown_outcome', 'divergent_history'
        )
    ),
    CONSTRAINT recovery_qualification_classification_known CHECK (
        classification IN (
            'safe_to_resume', 'safe_to_retry', 'must_reconcile', 'must_abort',
            'human_required'
        )
    ),
    CONSTRAINT recovery_qualification_action_known CHECK (
        action IN ('resume', 'retry', 'reconcile', 'abort', 'human_review')
    )
);

CREATE INDEX idx_recovery_qualification_observations_target
    ON recovery_qualification_observations (target, observed_at, id);

CREATE OR REPLACE FUNCTION vestrace_prevent_recovery_qualification_observation_modification()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'recovery qualification observations are append-only'
        USING ERRCODE = '55000';
END;
$$;

CREATE TRIGGER tr_recovery_qualification_observations_no_update
    BEFORE UPDATE ON recovery_qualification_observations
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_recovery_qualification_observation_modification();

CREATE TRIGGER tr_recovery_qualification_observations_no_delete
    BEFORE DELETE ON recovery_qualification_observations
    FOR EACH ROW
    EXECUTE FUNCTION vestrace_prevent_recovery_qualification_observation_modification();
