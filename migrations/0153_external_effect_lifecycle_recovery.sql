-- A dispatch could return and the process could die before its receipt was
-- committed. Recovery joined through receipts, so that effect then disappeared
-- permanently: the intent survived, the world had been touched, and no durable
-- fact said the dispatch had begun.
--
-- Lifecycle is append-only evidence rather than a mutable intent column. The
-- intent is the immutable claim made before execution; each later state names
-- the record or idempotency key that justifies it. Replays therefore append
-- nothing unless their own evidence row was inserted.
--
-- There is deliberately no backfill. Existing effects have no recorded
-- lifecycle transition, and `None` is the truthful answer for them. With no
-- UPDATE across existing tenant rows this migration needs no temporary
-- `NO FORCE ROW LEVEL SECURITY` dance like 0151.

CREATE TABLE external_effect_lifecycle_transitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    effect_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    status TEXT NOT NULL,
    cause TEXT NOT NULL,
    cause_ref TEXT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT external_effect_lifecycle_effect_workspace_fk
        FOREIGN KEY (effect_id, workspace_id)
        REFERENCES external_effect_intents (id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT external_effect_lifecycle_status_known CHECK (
        status IN (
            'prepared', 'authorized', 'dispatching', 'acknowledged',
            'failed', 'unknown', 'confirmed', 'reconciling'
        )
    ),
    CONSTRAINT external_effect_lifecycle_cause_qualified CHECK (
           (status = 'prepared' AND cause = 'intent_recorded')
        OR (status = 'dispatching' AND cause = 'dispatch_started')
        OR (status IN ('acknowledged', 'failed', 'unknown') AND cause = 'receipt_recorded')
        OR (status = 'reconciling' AND cause = 'outcome_settled')
        OR (status IN ('confirmed', 'failed') AND cause = 'outcome_delivered')
    ),
    CONSTRAINT external_effect_lifecycle_cause_ref_not_blank
        CHECK (btrim(cause_ref) <> '')
);

CREATE INDEX idx_external_effect_lifecycle_latest
    ON external_effect_lifecycle_transitions
        (workspace_id, effect_id, recorded_at DESC, created_at DESC, id DESC);

CREATE INDEX idx_external_effect_lifecycle_lost_dispatch
    ON external_effect_lifecycle_transitions (workspace_id, recorded_at)
    WHERE status = 'dispatching';

ALTER TABLE external_effect_lifecycle_transitions ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_effect_lifecycle_transitions FORCE ROW LEVEL SECURITY;
CREATE POLICY external_effect_lifecycle_workspace_isolation
ON external_effect_lifecycle_transitions
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- A reconciliation is about the dispatch. The receipt is optional evidence:
-- when the process died after dispatch and before receipt persistence, there is
-- still an effect to reconcile and no receipt id to name. Migration 0144
-- already installed the required `(effect_id, workspace_id)` parent key; its
-- receipt key remains valid under PostgreSQL's MATCH SIMPLE null semantics.
ALTER TABLE external_reconciliations
    ALTER COLUMN receipt_id DROP NOT NULL;
