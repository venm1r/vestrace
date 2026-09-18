-- Migration: 0145_force_rls_on_the_run_core.sql
--
-- Fifth batch: the run core, and the surfaces built on top of it.
--
-- `ENABLE` exempts the table owner and the runtime role owns every table here,
-- so an enabled-but-unforced policy is inert for the only role that runs
-- queries. See 0139, 0140 and 0143 for the batches before this one, and 0144
-- for the tables that had no policy at all.
--
-- # The run core
--
-- Three adapters were converted from a bare `PgPool` to a scoped transaction
-- before these tables could be forced:
--
--   run_leases        PgRunLeasePort
--   run_work_items    PgWorkQueuePort
--   agent_runs        PgStartupRecoverySource
--
-- The rest of the run write path — `PgRunStore`, `PgRunCommandCommitter`,
-- `PgRunEventStore`, `PgRunRecoveryStore`, `PgRunRepository` — already scoped.
--
-- # The lease is not a read leak
--
-- `PgRunLeasePort::heartbeat` and `release` took a `RequestContext`, ignored
-- it, and matched on `run_id` alone; `acquire`'s `ON CONFLICT (run_id) DO
-- UPDATE` carried no workspace predicate, so an expired lease on *any* tenant's
-- run was takeable by any worker. The row kept its own `workspace_id` while its
-- `worker_id` came to belong to somebody else.
--
-- The lease is what stops two workers advancing the same run. A cross-tenant
-- acquire is therefore not a disclosure — it is one workspace able to deny
-- another the ability to execute its runs, and a second writer on a run that is
-- supposed to have exactly one. All three statements carry the workspace now.
--
-- `PgWorkQueuePort` was the opposite case and worth naming as such: every
-- statement already carried `workspace_id = $n` and had done all along. What it
-- lacked was the connection scope, so the predicate was the only thing holding
-- the boundary, with nothing behind it if a later query forgot.
--
-- # Tables forced without an adapter change
--
-- `artifacts`, `artifact_revisions`, `connections`, `connectors`,
-- `external_triggers`, `ag_ui_endpoints` and `cognitive_mutations` are each
-- reached by exactly one adapter, and all of those already scope.
--
-- `run_exports` and `approval_records` have no adapter at all — no reader, no
-- writer, anywhere in the workspace. Forcing them costs nothing today and puts
-- the boundary in place before the first caller arrives rather than after.

ALTER TABLE agent_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE run_events FORCE ROW LEVEL SECURITY;
ALTER TABLE run_steps FORCE ROW LEVEL SECURITY;
ALTER TABLE run_work_items FORCE ROW LEVEL SECURITY;
ALTER TABLE run_leases FORCE ROW LEVEL SECURITY;
ALTER TABLE run_exports FORCE ROW LEVEL SECURITY;
ALTER TABLE approval_records FORCE ROW LEVEL SECURITY;

ALTER TABLE artifacts FORCE ROW LEVEL SECURITY;
ALTER TABLE artifact_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE connections FORCE ROW LEVEL SECURITY;
ALTER TABLE connectors FORCE ROW LEVEL SECURITY;
ALTER TABLE external_triggers FORCE ROW LEVEL SECURITY;
ALTER TABLE ag_ui_endpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE cognitive_mutations FORCE ROW LEVEL SECURITY;
