CREATE TABLE IF NOT EXISTS cognitive_mutations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    actor UUID NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    expected_state_revision INTEGER NOT NULL,
    mutation_kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    provenance_refs JSONB NOT NULL DEFAULT '[]',
    resulting_revision_id TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mutations_workspace ON cognitive_mutations(workspace_id);
CREATE INDEX IF NOT EXISTS idx_mutations_target ON cognitive_mutations(workspace_id, target_kind, target_id);

CREATE TABLE IF NOT EXISTS reconciliation_records (
    id TEXT PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    conflict_id UUID NOT NULL REFERENCES conflicts(id),
    reconciliation_class TEXT NOT NULL,
    outcome TEXT NOT NULL,
    input_evidence_refs JSONB NOT NULL DEFAULT '[]',
    basis_refs JSONB NOT NULL DEFAULT '[]',
    policy_version TEXT,
    human_decision TEXT,
    resolved_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reconciliation_workspace ON reconciliation_records(workspace_id);
CREATE INDEX IF NOT EXISTS idx_reconciliation_conflict ON reconciliation_records(conflict_id);
