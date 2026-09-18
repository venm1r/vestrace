-- Migration: 0011_retrieval_journal_and_context_packs.sql

CREATE TABLE IF NOT EXISTS retrieval_runs (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    query_text TEXT NOT NULL,
    intent TEXT NOT NULL,
    candidate_count INT NOT NULL,
    execution_time_ms INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE retrieval_runs ENABLE ROW LEVEL SECURITY;

CREATE POLICY retrieval_runs_workspace_isolation ON retrieval_runs
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS context_packs (
    id UUID PRIMARY KEY,
    retrieval_run_id UUID NOT NULL REFERENCES retrieval_runs(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    token_budget INT NOT NULL,
    used_tokens INT NOT NULL,
    items JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE context_packs ENABLE ROW LEVEL SECURITY;

CREATE POLICY context_packs_workspace_isolation ON context_packs
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
