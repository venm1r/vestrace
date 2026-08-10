-- Migration: 0021_run_leases_and_work_items.sql
-- Operational leases and work items for restart-safe run execution.

-- ─── run_leases ──────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS run_leases (
    run_id          UUID PRIMARY KEY REFERENCES agent_runs(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    worker_id       TEXT NOT NULL,
    generation      BIGINT NOT NULL DEFAULT 1,
    acquired_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    heartbeat_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lease_until     TIMESTAMPTZ NOT NULL,

    CONSTRAINT chk_run_leases_generation_gte1
        CHECK (generation >= 1)
);

ALTER TABLE run_leases ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS run_leases_workspace_isolation ON run_leases;
CREATE POLICY run_leases_workspace_isolation ON run_leases
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());

-- ─── run_work_items ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS run_work_items (
    id                      UUID PRIMARY KEY,
    workspace_id            UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id                  UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,

    step_id                 UUID,
    expected_run_version    BIGINT NOT NULL DEFAULT 1,
    kind                    TEXT NOT NULL,
    payload                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    status                  TEXT NOT NULL DEFAULT 'ready',

    priority                INT NOT NULL DEFAULT 0,
    available_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deadline                TIMESTAMPTZ,

    attempt                 INT NOT NULL DEFAULT 0,
    max_attempts            INT NOT NULL DEFAULT 3,
    idempotency_key         TEXT NOT NULL,

    required_capabilities   JSONB NOT NULL DEFAULT '[]'::jsonb,

    lease_owner             TEXT,
    lease_until             TIMESTAMPTZ,

    last_error              JSONB,

    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- status must be one of the known enum values
    CONSTRAINT chk_run_work_items_status
        CHECK (status IN ('ready','leased','completed','failed','cancelled','dead_letter')),
    -- kind must be one of the known enum values
    CONSTRAINT chk_run_work_items_kind
        CHECK (kind IN ('advance_run','resume_run','create_checkpoint','expire_run','execute_step')),
    -- max_attempts must be >= 1
    CONSTRAINT chk_run_work_items_max_attempts_gte1
        CHECK (max_attempts >= 1),
    -- attempt must be >= 0
    CONSTRAINT chk_run_work_items_attempt_gte0
        CHECK (attempt >= 0),
    -- idempotency_key must not be blank
    CONSTRAINT chk_run_work_items_key_not_blank
        CHECK (octet_length(idempotency_key) >= 1)
);

-- unique idempotency key per workspace
CREATE UNIQUE INDEX IF NOT EXISTS uq_run_work_items_workspace_key
    ON run_work_items (workspace_id, idempotency_key);

-- index for lease_next: ordered by priority DESC, available_at, created_at, id
CREATE INDEX IF NOT EXISTS ix_run_work_items_lease_order
    ON run_work_items (status, available_at, priority DESC, created_at, id)
    WHERE status = 'ready';

-- index for cancel_for_run
CREATE INDEX IF NOT EXISTS ix_run_work_items_run_id_status
    ON run_work_items (run_id, status);

ALTER TABLE run_work_items ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS run_work_items_workspace_isolation ON run_work_items;
CREATE POLICY run_work_items_workspace_isolation ON run_work_items
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
