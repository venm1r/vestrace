-- L2 learned projections and proposal-only mutation boundary.
-- These tables reference raw evaluation ids in JSON arrays and never replace
-- or delete the source evaluation_facts rows.

CREATE TABLE IF NOT EXISTS learned_projections (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind JSONB NOT NULL,
    target JSONB NOT NULL,
    generator JSONB NOT NULL,
    authority JSONB NOT NULL,
    source_generation INTEGER NOT NULL,
    source_evaluation_fact_ids JSONB NOT NULL,
    source_evidence_refs JSONB NOT NULL,
    content JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT learned_projections_advisory_only CHECK (authority = '"advisory"'::jsonb),
    CONSTRAINT learned_projections_generation_positive CHECK (source_generation > 0),
    CONSTRAINT learned_projections_fact_ids_array CHECK (
        jsonb_typeof(source_evaluation_fact_ids) = 'array'
        AND jsonb_array_length(source_evaluation_fact_ids) > 0
    ),
    CONSTRAINT learned_projections_evidence_array CHECK (
        jsonb_typeof(source_evidence_refs) = 'array'
        AND jsonb_array_length(source_evidence_refs) > 0
    )
);

CREATE TABLE IF NOT EXISTS learning_proposals (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_projection_ids JSONB NOT NULL,
    source_evaluation_fact_ids JSONB NOT NULL,
    target JSONB NOT NULL,
    change JSONB NOT NULL,
    expected_target_revision INTEGER NOT NULL,
    rationale TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    created_by UUID NOT NULL,
    status JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT learning_proposals_workspace_created_by_fkey
        FOREIGN KEY (workspace_id, created_by)
        REFERENCES principals (workspace_id, id),
    CONSTRAINT learning_proposals_projection_ids_array CHECK (
        jsonb_typeof(source_projection_ids) = 'array'
        AND jsonb_array_length(source_projection_ids) > 0
    ),
    CONSTRAINT learning_proposals_fact_ids_array CHECK (
        jsonb_typeof(source_evaluation_fact_ids) = 'array'
        AND jsonb_array_length(source_evaluation_fact_ids) > 0
    ),
    CONSTRAINT learning_proposals_revision_positive CHECK (expected_target_revision > 0),
    CONSTRAINT learning_proposals_rationale_not_blank CHECK (btrim(rationale) <> ''),
    CONSTRAINT learning_proposals_policy_version_not_blank CHECK (btrim(policy_version) <> ''),
    CONSTRAINT learning_proposals_status_is_known CHECK (
        status IN ('"draft"'::jsonb, '"submitted"'::jsonb, '"rejected"'::jsonb, '"withdrawn"'::jsonb)
    )
);

ALTER TABLE learned_projections ENABLE ROW LEVEL SECURITY;
ALTER TABLE learning_proposals ENABLE ROW LEVEL SECURITY;
ALTER TABLE learned_projections FORCE ROW LEVEL SECURITY;
ALTER TABLE learning_proposals FORCE ROW LEVEL SECURITY;

CREATE POLICY learned_projections_workspace_isolation ON learned_projections
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE POLICY learning_proposals_workspace_isolation ON learning_proposals
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE INDEX idx_learned_projections_workspace_created
    ON learned_projections(workspace_id, created_at DESC);

CREATE INDEX idx_learned_projections_target
    ON learned_projections USING GIN (target);

CREATE INDEX idx_learning_proposals_workspace_created
    ON learning_proposals(workspace_id, created_at DESC);

CREATE INDEX idx_learning_proposals_target
    ON learning_proposals USING GIN (target);
