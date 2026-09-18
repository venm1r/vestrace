-- Migration: 0147_capability_grants.sql
--
-- Somewhere to keep a capability grant.
--
-- # Why authority has been a configuration file
--
-- `CapabilityGrant` has existed since the G1 slice: a subject, an operation, a
-- resource scope, a validity window, a budget, a risk ceiling, conditions and a
-- revocation. CAP-002..CAP-014 verify all of it executably. Nothing ever issued
-- one, because there was no table, no repository and no route — so the deployed
-- authorization engine is `ConfiguredCapabilityPolicyEngine`, which allows a
-- static list of capability enum values with no subject, no expiry and no
-- revocation, and prints a warning at startup saying exactly that.
--
-- CAP-001, CAP-005 and every GOV requirement rest on this table existing.
--
-- # Shape
--
-- The JSONB payload is authoritative and the columns beside it are indexed
-- projections checked against it on read, as in `health_findings`,
-- `qualification_bundles` and the recovery tables.
--
-- The lookup that matters is "the active grants for this subject in this
-- workspace", which is what an authorization decision needs and the only query
-- on the hot path. Everything else is administration.
--
-- # Why status is a column and not only a payload field
--
-- Revocation has to be visible to a query. A revoked grant that can only be
-- recognised by decoding its payload would mean loading every grant a subject
-- has ever held in order to decide one request, and CAP-010 requires revocation
-- to take effect without a new grant cycle — which in practice means the next
-- decision, which means the query.

CREATE TABLE IF NOT EXISTS capability_grants (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    subject_id UUID NOT NULL,
    issuer_id UUID NOT NULL,
    capability TEXT NOT NULL,
    operation TEXT NOT NULL,
    resource_scope TEXT NOT NULL,
    status TEXT NOT NULL,
    risk_ceiling TEXT NOT NULL,
    budget_max_units BIGINT,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    CONSTRAINT capability_grants_payload_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT capability_grants_operation_not_blank CHECK (btrim(operation) <> ''),
    CONSTRAINT capability_grants_scope_not_blank CHECK (btrim(resource_scope) <> ''),
    CONSTRAINT capability_grants_status_known CHECK (status IN ('active', 'revoked')),
    -- A revoked grant must say when, and an active one must not claim to have
    -- been revoked. The status column and the timestamp are two halves of one
    -- fact and a row carrying only one of them is unreadable.
    CONSTRAINT capability_grants_revocation_is_complete CHECK (
        (status = 'revoked' AND revoked_at IS NOT NULL)
        OR (status = 'active' AND revoked_at IS NULL)
    ),
    CONSTRAINT capability_grants_validity_ordered CHECK (
        valid_until IS NULL OR valid_until > valid_from
    ),
    CONSTRAINT capability_grants_budget_positive CHECK (
        budget_max_units IS NULL OR budget_max_units > 0
    )
);

-- The authorization hot path: active grants for one subject in one workspace.
CREATE INDEX IF NOT EXISTS idx_capability_grants_subject_active
    ON capability_grants (workspace_id, subject_id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_capability_grants_administration
    ON capability_grants (workspace_id, created_at DESC, id DESC);

ALTER TABLE capability_grants ENABLE ROW LEVEL SECURITY;
ALTER TABLE capability_grants FORCE ROW LEVEL SECURITY;
CREATE POLICY capability_grants_workspace_isolation ON capability_grants
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
