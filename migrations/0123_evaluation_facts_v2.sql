-- Migration: 0123_evaluation_facts_v2.sql
-- L1 typed raw evaluation evidence. Learned projections remain separate future state.

CREATE TABLE IF NOT EXISTS evaluation_facts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    target JSONB NOT NULL,
    evaluator JSONB NOT NULL,
    metric JSONB NOT NULL,
    result JSONB NOT NULL,
    evidence_refs JSONB NOT NULL,
    authority JSONB NOT NULL,
    policy_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT evaluation_facts_evidence_array CHECK (
        jsonb_typeof(evidence_refs) = 'array' AND jsonb_array_length(evidence_refs) > 0
    ),
    CONSTRAINT evaluation_facts_policy_version_not_blank CHECK (btrim(policy_version) <> '')
);

ALTER TABLE evaluation_facts ENABLE ROW LEVEL SECURITY;

CREATE POLICY evaluation_facts_workspace_isolation ON evaluation_facts
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_evaluation_facts_workspace_created
    ON evaluation_facts(workspace_id, created_at DESC);

CREATE INDEX idx_evaluation_facts_target
    ON evaluation_facts USING GIN (target);
