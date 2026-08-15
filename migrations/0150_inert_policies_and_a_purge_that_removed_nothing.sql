-- Twenty-eight tables carried `ENABLE ROW LEVEL SECURITY` without `FORCE`.
--
-- The runtime role owns every table in this schema, and ownership bypasses an
-- unforced policy. So each of these had a workspace-isolation policy that had
-- never once been consulted: the schema said the rows were isolated, and the
-- only thing keeping them apart was a `WHERE workspace_id = $1` that a caller
-- had to remember.
--
-- Most of them are reached by no code at all — they are schema for capabilities
-- that were designed and never built — which is why this could be missed for so
-- long. Forcing a table nothing queries costs nothing and removes the false
-- statement; forcing one an adapter queries needs that adapter to be scoped
-- first, and both of those are in this change.
--
-- Every table below already carries at least one policy. A table with RLS
-- forced and no policy denies everything, so that was checked before writing
-- this rather than discovered afterwards.

ALTER TABLE agent_packages FORCE ROW LEVEL SECURITY;
ALTER TABLE authorization_tickets FORCE ROW LEVEL SECURITY;
ALTER TABLE budget_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE claim_assessments FORCE ROW LEVEL SECURITY;
ALTER TABLE claim_evidence_links FORCE ROW LEVEL SECURITY;
ALTER TABLE claims FORCE ROW LEVEL SECURITY;
ALTER TABLE conflicts FORCE ROW LEVEL SECURITY;
ALTER TABLE conversation_threads FORCE ROW LEVEL SECURITY;
ALTER TABLE cross_workspace_memory_grants FORCE ROW LEVEL SECURITY;
ALTER TABLE derivations FORCE ROW LEVEL SECURITY;
ALTER TABLE execution_plan_revisions FORCE ROW LEVEL SECURITY;
ALTER TABLE execution_plans FORCE ROW LEVEL SECURITY;
ALTER TABLE idempotency_keys FORCE ROW LEVEL SECURITY;
ALTER TABLE interaction_sessions FORCE ROW LEVEL SECURITY;
ALTER TABLE jobs FORCE ROW LEVEL SECURITY;
ALTER TABLE metric_rollups FORCE ROW LEVEL SECURITY;
ALTER TABLE model_execution_attempts FORCE ROW LEVEL SECURITY;
ALTER TABLE policy_bundles FORCE ROW LEVEL SECURITY;
ALTER TABLE product_api_transfers FORCE ROW LEVEL SECURITY;
ALTER TABLE purge_audits FORCE ROW LEVEL SECURITY;
ALTER TABLE reconciliation_records FORCE ROW LEVEL SECURITY;
ALTER TABLE redaction_rules FORCE ROW LEVEL SECURITY;
ALTER TABLE remote_agent_invocations FORCE ROW LEVEL SECURITY;
ALTER TABLE sessions FORCE ROW LEVEL SECURITY;
ALTER TABLE structured_search_indexes FORCE ROW LEVEL SECURITY;
ALTER TABLE tool_definitions FORCE ROW LEVEL SECURITY;
ALTER TABLE tool_invocations FORCE ROW LEVEL SECURITY;
ALTER TABLE webhook_subscriptions FORCE ROW LEVEL SECURITY;

-- `purge_audits` records that a memory was destroyed. Two things it did not
-- record:
--
-- The approval that permitted it. `HardPurgeMemoryService` takes an
-- `approval_id`, passes it to the repository, and the repository ends with
-- `let _ = approval_id;` — the authorization reference was accepted and thrown
-- away. An audit of an irreversible act that cannot name what authorized it is
-- not an audit.
--
-- What was actually removed. The purge ran unscoped against tables whose policy
-- is forced, so every DELETE matched zero rows while the audit row inserted
-- successfully: the system would have reported a purge, deleted nothing, and
-- left a durable record saying it had. Counting the rows makes that
-- contradiction impossible to write down.
ALTER TABLE purge_audits
    ADD COLUMN approval_id text,
    ADD COLUMN removed_counts jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT purge_audits_removed_counts_object
        CHECK (jsonb_typeof(removed_counts) = 'object');
