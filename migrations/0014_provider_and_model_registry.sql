-- Migration: 0014_provider_and_model_registry.sql

CREATE TABLE IF NOT EXISTS providers (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    locality TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE providers ENABLE ROW LEVEL SECURITY;

CREATE POLICY providers_workspace_isolation ON providers
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

CREATE TABLE IF NOT EXISTS models (
    id UUID PRIMARY KEY,
    provider_id UUID NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    model_name TEXT NOT NULL,
    context_window INT NOT NULL,
    input_cost_per_mtoken REAL NOT NULL CHECK (input_cost_per_mtoken >= 0),
    output_cost_per_mtoken REAL NOT NULL CHECK (output_cost_per_mtoken >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE models ENABLE ROW LEVEL SECURITY;

CREATE POLICY models_workspace_isolation ON models
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
