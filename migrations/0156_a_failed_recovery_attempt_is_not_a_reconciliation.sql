-- The bounded reconciliation sweep could spend its entire batch on effects it
-- was unable to ask about. That failure lived only in `UnreconciledEffect`, an
-- in-memory report: no reconciliation existed, so the retry cutoff excluded
-- nothing, and oldest-first discovery returned the same unaskable effects on
-- every worker tick. Eight such effects could therefore starve everything
-- behind them forever.
--
-- This is an attempt record rather than an inconclusive reconciliation.
-- `Inconclusive` says the provider was asked and its answer settled nothing;
-- a missing route or failed provider call says nobody obtained an answer at
-- all. Migration 0151 made reconciliation recency mean "we asked" precisely so
-- an inconclusive answer would be retried without pretending it was settled.
-- Putting a call that never happened into that table would erase the boundary
-- again and turn a failure of this system into evidence about the outside
-- world.
--
-- Attempts are append-only. A later reconciliation supersedes the failure in
-- candidate discovery, while a settled reconciliation removes the effect from
-- recovery altogether; no mutable retry flag or cleanup write is needed. There
-- is deliberately no backfill because the old in-memory failures are gone and
-- inventing their times or reasons would create evidence that was never kept.

CREATE TABLE external_effect_recovery_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    effect_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL,
    failure_reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT external_effect_recovery_attempt_effect_workspace_fk
        FOREIGN KEY (effect_id, workspace_id)
        REFERENCES external_effect_intents (id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT external_effect_recovery_attempt_reason_not_blank
        CHECK (btrim(failure_reason) <> '')
);

-- Candidate discovery asks for the latest failed attempt for an effect and
-- compares its domain time with both the caller's cutoff and any later
-- reconciliation. The id makes equal-time ordering deterministic within this
-- evidence family without pretending it orders rows in another table.
CREATE INDEX idx_external_effect_recovery_attempts_latest
    ON external_effect_recovery_attempts
        (workspace_id, effect_id, attempted_at DESC, id DESC);

ALTER TABLE external_effect_recovery_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_effect_recovery_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY external_effect_recovery_attempts_workspace_isolation
ON external_effect_recovery_attempts
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
