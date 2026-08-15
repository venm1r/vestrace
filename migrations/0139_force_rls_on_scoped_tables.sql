-- Migration: 0139_force_rls_on_scoped_tables.sql
--
-- Force row level security on the tables whose adapters now scope.
--
-- # The wider problem this is one step of
--
-- Sixty-six tables in this schema enable row level security without forcing it.
-- `ENABLE` exempts the table's owner, `FORCE` removes that exemption, and the
-- runtime role owns every table here — so on those sixty-six the workspace
-- policies are present, readable, and inert for the one role that runs queries.
-- Tenant separation rests entirely on each query remembering
-- `WHERE workspace_id = $1`.
--
-- Nothing caught it because every database test connects as a superuser, which
-- bypasses row level security outright. See
-- `crates/vestrace-infrastructure/tests/row_level_security.rs`, which runs under
-- a role that cannot.
--
-- # Why this migration is small
--
-- Forcing the policy makes an unscoped write a refusal and an unscoped read an
-- empty result. Twenty-four adapters did not scope at all, so forcing
-- everything at once would break most of the system — and the test suite would
-- stay green while it did, because those tests bypass the policy too.
--
-- The order has to be: scope the adapters, force their tables, verify, repeat.
-- This is the first batch. Seven adapters were converted from a bare `PgPool`
-- to a scoped transaction:
--
--   audit_events        PgAuditRepository
--   models              PgModelRepository
--   agents              PgAgentRepository
--   skills              PgSkillRepository
--   providers           PgProviderRepository
--   routing_decisions   PgRoutingDecisionRepository
--   model_executions    PgModelExecutionRepository
--
-- `PgAuditRepository` is the one worth naming: it took a `RequestContext` and
-- ignored it, writing `event.workspace_id` through an unscoped pool with
-- nothing checking the two agreed. An audit event written into another tenant's
-- trail is the least recoverable kind of leak, because afterwards it is
-- indistinguishable from a real one.

ALTER TABLE audit_events FORCE ROW LEVEL SECURITY;
ALTER TABLE models FORCE ROW LEVEL SECURITY;
ALTER TABLE agents FORCE ROW LEVEL SECURITY;
ALTER TABLE skills FORCE ROW LEVEL SECURITY;
ALTER TABLE providers FORCE ROW LEVEL SECURITY;
ALTER TABLE routing_decisions FORCE ROW LEVEL SECURITY;
ALTER TABLE model_executions FORCE ROW LEVEL SECURITY;
