-- A dispatch states which worker began it, but that identity had no durable
-- liveness record. `worker_id` existed only on a run lease, so a worker holding
-- no run lease was invisible and recovery had to wait for every dispatch's
-- deadline even after its owning process had stopped reporting.
--
-- Presence is workspace-scoped because workers serve configured workspaces and
-- every effect is recovered inside one. `started_at` distinguishes one
-- registration lifetime, `last_reported_at` is the renewable liveness fact, and
-- `stopped_at` is explicit evidence from an orderly shutdown.
--
-- # Why stopped workers keep a row
--
-- Deleting a row would make a clean shutdown indistinguishable from no evidence
-- at all. No row is also the truthful state for dispatches written before this
-- table existed and for a process that died before its first registration. Such
-- dispatches must continue to use their stated deadlines; treating absence as
-- death would silently change the meaning of historical evidence, the same
-- compatibility error migration 0154 avoided with its `NOT VALID` constraint.
-- A stopped tombstone therefore removes *active* presence without erasing the
-- evidence recovery needs.
--
-- This is liveness evidence for external-effect dispatch only. Run leases keep
-- their existing ownership, generation and heartbeat rules; this table adds no
-- fencing and makes no claim that a lapsed process cannot return.

-- # Why the workspace is not a foreign key here
--
-- No table in this subsystem references `workspaces`: not
-- `external_effect_intents`, not receipts, not reconciliations, not the
-- lifecycle transitions, not the recovery attempts. Adding the reference to
-- this one table alone would make presence stricter than the evidence it
-- describes, and the asymmetry has a failure mode: a worker registers presence
-- for **every** configured workspace, so a workspace whose row is missing would
-- stop the worker from starting while effects for that same workspace continue
-- to be recorded without complaint. A missing parent row would take down the
-- process over something nothing else in this subsystem requires.
--
-- Isolation is still enforced — by the forced row level security policy below,
-- which is how every other table here enforces it.
CREATE TABLE external_effect_worker_presence (
    workspace_id UUID NOT NULL,
    worker_id TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    last_reported_at TIMESTAMPTZ NOT NULL,
    stopped_at TIMESTAMPTZ,
    PRIMARY KEY (workspace_id, worker_id),
    CONSTRAINT external_effect_worker_presence_worker_not_blank
        CHECK (btrim(worker_id) <> ''),
    CONSTRAINT external_effect_worker_presence_report_not_before_start
        CHECK (last_reported_at >= started_at),
    CONSTRAINT external_effect_worker_presence_stop_not_before_start
        CHECK (stopped_at IS NULL OR stopped_at >= started_at)
);

-- Recovery joins an exact owner through the primary key. This second access
-- path supports workspace-scoped inspection and lapse discovery over active
-- workers without making stopped tombstones pay for an index they never use.
CREATE INDEX idx_external_effect_worker_presence_liveness
    ON external_effect_worker_presence
        (workspace_id, last_reported_at, worker_id)
    WHERE stopped_at IS NULL;

ALTER TABLE external_effect_worker_presence ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_effect_worker_presence FORCE ROW LEVEL SECURITY;
CREATE POLICY external_effect_worker_presence_workspace_isolation
ON external_effect_worker_presence
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
