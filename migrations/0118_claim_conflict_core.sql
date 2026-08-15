CREATE TABLE IF NOT EXISTS claims (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    semantic_key TEXT NOT NULL,
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    value TEXT NOT NULL,
    lifecycle_status TEXT NOT NULL DEFAULT 'proposed',
    valid_from TIMESTAMPTZ,
    valid_until TIMESTAMPTZ,
    state_revision INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_claims_workspace ON claims(workspace_id);
CREATE INDEX IF NOT EXISTS idx_claims_semantic_key ON claims(workspace_id, semantic_key);
CREATE INDEX IF NOT EXISTS idx_claims_status ON claims(workspace_id, lifecycle_status);

CREATE TABLE IF NOT EXISTS claim_evidence_links (
    id UUID PRIMARY KEY,
    claim_id UUID NOT NULL REFERENCES claims(id),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    evidence_ref JSONB NOT NULL,
    role TEXT NOT NULL,
    evidence_weight_metadata JSONB,
    source_classification TEXT NOT NULL,
    added_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_claim_evidence_claim ON claim_evidence_links(claim_id);
CREATE INDEX IF NOT EXISTS idx_claim_evidence_workspace ON claim_evidence_links(workspace_id);

CREATE TABLE IF NOT EXISTS claim_assessments (
    id UUID PRIMARY KEY,
    claim_id UUID NOT NULL REFERENCES claims(id),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    assessment_kind TEXT NOT NULL,
    confidence REAL NOT NULL,
    basis_refs JSONB NOT NULL DEFAULT '[]',
    policy_version TEXT NOT NULL,
    assessor UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_claim_assessments_claim ON claim_assessments(claim_id);
CREATE INDEX IF NOT EXISTS idx_claim_assessments_workspace ON claim_assessments(workspace_id);

CREATE TABLE IF NOT EXISTS conflicts (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    conflict_kind TEXT NOT NULL,
    participant_refs JSONB NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'open',
    detected_by UUID NOT NULL,
    evidence_refs JSONB NOT NULL DEFAULT '[]',
    reconciliation_ref TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conflicts_workspace ON conflicts(workspace_id);
CREATE INDEX IF NOT EXISTS idx_conflicts_status ON conflicts(workspace_id, status);

CREATE TABLE IF NOT EXISTS supersession_links (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    target_kind TEXT NOT NULL,
    superseded_claim_id UUID REFERENCES claims(id),
    superseded_memory_id UUID,
    superseded_revision_id UUID,
    replacement_claim_id UUID REFERENCES claims(id),
    replacement_memory_id UUID,
    replacement_revision_id UUID,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_supersession_workspace ON supersession_links(workspace_id);
CREATE INDEX IF NOT EXISTS idx_supersession_superseded ON supersession_links(workspace_id, superseded_claim_id, superseded_memory_id);
CREATE INDEX IF NOT EXISTS idx_supersession_replacement ON supersession_links(workspace_id, replacement_claim_id, replacement_memory_id);
