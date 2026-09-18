-- Migration: 0132_workspace_settings.sql
--
-- Operator-changeable workspace settings.
--
-- Only values the runtime consults and an operator can genuinely change are
-- stored. Environment facts (RLS enforcement, connection pool size, process log
-- filter) are deliberately absent: they are observed at request time, because a
-- stored copy would let an interface present controls that change nothing.

CREATE TABLE IF NOT EXISTS workspace_settings (
    workspace_id            UUID PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    max_concurrent_runs     INTEGER NOT NULL,
    run_budget_cap_micros   BIGINT NOT NULL,
    log_level               TEXT NOT NULL,
    version                 BIGINT NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_workspace_settings_concurrency
        CHECK (max_concurrent_runs BETWEEN 1 AND 10000),
    CONSTRAINT chk_workspace_settings_budget_non_negative
        CHECK (run_budget_cap_micros >= 0),
    CONSTRAINT chk_workspace_settings_version_positive
        CHECK (version >= 1),
    CONSTRAINT chk_workspace_settings_log_level
        CHECK (log_level IN ('error', 'warn', 'info', 'debug', 'trace'))
);

ALTER TABLE workspace_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_settings FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS workspace_settings_workspace_isolation ON workspace_settings;
CREATE POLICY workspace_settings_workspace_isolation ON workspace_settings
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
