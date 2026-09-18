-- `ExternalEffectService::authorize` obtained a fully modelled
-- `PolicyDecision` and immediately dropped it. The intent proved only that an
-- effect was prepared; no durable fact said whether authority permitted or
-- refused it, by which policy version, against which grant, or why. Migration
-- 0024 even named policy decisions while creating only authorization tickets.
-- Fault point 2 was therefore indistinguishable from fault point 1.
--
-- This table is deliberately narrower than that missing general store. It
-- records authorizations *of external effects*, with a composite foreign key
-- making the effect-to-workspace relationship a database invariant. Persisting
-- every policy decision made by run work or other boundaries has different
-- volume and query-design costs and is not implied here.
--
-- A refusal is evidence too. Recording only `allow` would preserve the answer
-- operators already see and discard the answer auditors ask about most. Both
-- results are stored; only an allowed decision appends `authorized`, because a
-- lifecycle transition claiming a refused effect was authorized would be a
-- contradiction rather than evidence.
--
-- The JSONB payload is authoritative and the columns beside it are projections
-- checked on read, following the external-effect and capability-grant stores.
-- The authorization row and its permitted lifecycle transition are inserted by
-- one repository transaction, before dispatch can continue.

CREATE TABLE external_effect_authorizations (
    id UUID PRIMARY KEY,
    effect_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    policy_id UUID,
    policy_version TEXT NOT NULL,
    subject_id UUID NOT NULL,
    capability TEXT NOT NULL,
    operation TEXT NOT NULL,
    resource_scope TEXT NOT NULL,
    result TEXT NOT NULL,
    reason TEXT NOT NULL,
    input_state JSONB NOT NULL,
    matched_grant_id UUID,
    decided_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT external_effect_authorizations_effect_workspace_fk
        FOREIGN KEY (effect_id, workspace_id)
        REFERENCES external_effect_intents (id, workspace_id)
        ON DELETE RESTRICT,
    CONSTRAINT external_effect_authorizations_policy_version_not_blank
        CHECK (btrim(policy_version) <> ''),
    CONSTRAINT external_effect_authorizations_capability_not_blank
        CHECK (btrim(capability) <> ''),
    CONSTRAINT external_effect_authorizations_operation_not_blank
        CHECK (btrim(operation) <> ''),
    CONSTRAINT external_effect_authorizations_scope_not_blank
        CHECK (btrim(resource_scope) <> ''),
    CONSTRAINT external_effect_authorizations_result_known CHECK (
        result IN ('deny', 'allow', 'prepare_only', 'require_approval')
    ),
    CONSTRAINT external_effect_authorizations_reason_known CHECK (
        reason IN (
            'default_deny', 'grant_matched', 'configured_allowance',
            'workspace_mismatch', 'subject_mismatch', 'capability_mismatch',
            'operation_mismatch', 'resource_mismatch', 'not_yet_valid',
            'expired', 'revoked', 'budget_exceeded',
            'risk_exceeds_ceiling', 'condition_not_satisfied'
        )
    ),
    CONSTRAINT external_effect_authorizations_input_state_object
        CHECK (jsonb_typeof(input_state) = 'object'),
    CONSTRAINT external_effect_authorizations_payload_object
        CHECK (jsonb_typeof(payload) = 'object')
);

CREATE INDEX idx_external_effect_authorizations_effect
    ON external_effect_authorizations
        (workspace_id, effect_id, decided_at DESC, id DESC);

ALTER TABLE external_effect_authorizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_effect_authorizations FORCE ROW LEVEL SECURITY;
CREATE POLICY external_effect_authorizations_workspace_isolation
ON external_effect_authorizations
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- Migration 0155 last defined this CHECK when lost-dispatch adoption was
-- introduced. Preserve every existing qualified pair and add the one fact this
-- migration makes durable: Authorized is inseparable from the authorization
-- record whose id it names.
ALTER TABLE external_effect_lifecycle_transitions
    DROP CONSTRAINT external_effect_lifecycle_cause_qualified;

ALTER TABLE external_effect_lifecycle_transitions
    ADD CONSTRAINT external_effect_lifecycle_cause_qualified CHECK (
           (status = 'prepared' AND cause = 'intent_recorded')
        OR (status = 'authorized' AND cause = 'authorization_recorded')
        OR (status = 'dispatching' AND cause = 'dispatch_started')
        OR (status IN ('acknowledged', 'failed', 'unknown') AND cause = 'receipt_recorded')
        OR (status = 'unknown' AND cause = 'dispatch_lost')
        OR (status = 'reconciling' AND cause = 'outcome_settled')
        OR (status IN ('confirmed', 'failed') AND cause = 'outcome_delivered')
    );
