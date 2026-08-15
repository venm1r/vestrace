-- Migration: 0143_force_rls_on_execution_and_evaluation.sql
--
-- Third batch: workflow execution history, the workflow registry, and
-- evaluations.
--
-- `ENABLE` exempts the table owner and the runtime role owns every table here,
-- so a policy that is enabled but not forced is inert for the only role that
-- runs queries. See 0139 and 0140 for the two batches before this one, and
-- `crates/vestrace-infrastructure/tests/row_level_security.rs` for the guard
-- that runs under a role which cannot bypass the policy.
--
-- Four adapters were converted from a bare `PgPool` to a scoped transaction
-- before these tables could be forced:
--
--   workflow_executions   PgExecutionHistoryRepository
--   step_executions       PgExecutionHistoryRepository
--   execution_artifacts   PgExecutionHistoryRepository
--   execution_outcomes    PgExecutionHistoryRepository
--   workflow_definitions  PgWorkflowRepository
--   workflow_revisions    PgWorkflowRepository
--   evaluations           PgEvaluationRepository
--   evaluation_facts      PgEvaluationRepository
--
-- `PgDiagnosticsRepository` was converted in the same slice. It owns no table:
-- it counts rows in seven other tenants' tables, and it was the one adapter
-- that had the workspace predicate right and the connection wrong.
--
-- `search_documents` is forced here too, without an adapter change. Its three
-- callers — `PgMemoryRepository`, `PgTextRetriever` and
-- `vestrace rebuild search-documents` — all scope already, the first two since
-- the previous batch. The rebuild command did not, and could not have: it
-- named `memories.content_text` and `search_documents.search_tsv`, neither of
-- which exists, so it failed on its first statement in every deployment while
-- being the remediation two diagnostics print.
--
-- # Six unbounded queries closed on the way
--
-- `PgExecutionHistoryRepository` took a `RequestContext` on every method and
-- ignored all of them. `update_workflow_execution_status`,
-- `get_workflow_execution` and `update_step_status` selected on an id alone;
-- `list_steps` and `list_outcomes` selected on a `workflow_execution_id` alone.
-- `PgWorkflowRepository::find_by_id` and `get_revision` did the same, and the
-- revision read returns a workflow's entire definition body. Each now carries
-- the workspace in the statement as well as in the transaction, so the query
-- and the policy hold the boundary independently.
--
-- Two updates that matched no row previously reported success. A cross-tenant
-- update is now a refusal, deliberately indistinguishable from an update of a
-- row that does not exist.
--
-- # The finding that runs the other way
--
-- Migration `0124` forced `learned_projections` and `learning_proposals` while
-- their only adapter wrote through an unscoped pool. `vestrace_current_workspace_id()`
-- returned NULL for every statement it issued, so the policy admitted nothing:
-- **every write to those two tables has been refused and every read has
-- returned empty since 0124**, and `POST /v1/learning/projections` answered
-- `500 storage_failure` in every deployment running under the runtime role.
--
-- Nothing in this migration fixes that — the adapter change does. It is
-- recorded here because it is the same defect as the one these batches are
-- closing, seen from the other side: forcing a table and scoping its adapter
-- are two halves of one change, and doing either alone is silent. Enabling
-- without forcing leaves the policy inert; forcing without scoping kills the
-- surface. Neither shows up in a suite that connects as a superuser.

ALTER TABLE workflow_executions FORCE ROW LEVEL SECURITY;
ALTER TABLE step_executions FORCE ROW LEVEL SECURITY;
ALTER TABLE execution_artifacts FORCE ROW LEVEL SECURITY;
ALTER TABLE execution_outcomes FORCE ROW LEVEL SECURITY;
ALTER TABLE workflow_definitions FORCE ROW LEVEL SECURITY;
ALTER TABLE workflow_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE evaluations FORCE ROW LEVEL SECURITY;
ALTER TABLE evaluation_facts FORCE ROW LEVEL SECURITY;
ALTER TABLE search_documents FORCE ROW LEVEL SECURITY;
