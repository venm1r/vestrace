# PostgreSQL Schema & Migrations Reference

Vestrace uses PostgreSQL with `pgvector` and `pg_trgm` extensions as its authoritative relational store. Migrations are forward-only and applied sequentially by SQLx.

## Migration source of truth

The ordered [`migrations/`](../migrations/) directory is the authoritative migration history. Do not maintain a maximum migration number in this document: new migrations must be discovered from that directory and validated against the `_sqlx_migrations` table at runtime.

The early foundation migrations establish:

| Area | Representative migrations | Primary objects |
| :--- | :--- | :--- |
| Extensions and identity | `0001_extensions.sql`–`0003_rls_baseline.sql` | `vector`, `pg_trgm`, workspaces, principals, roles, RLS helpers |
| Evidence and memory | `0004_sessions_and_events.sql`–`0011_retrieval_journal_and_context_packs.sql` | sessions, events, memories, revisions, provenance, relations, retrieval records |
| Policy and providers | `0012_tokens_policies_and_approvals.sql`–`0016_agents_skills_workflows.sql` | tokens, approvals, audit, providers, models, agents, skills |
| Durable runs | `0019_agent_runs_and_steps.sql`–`0021_run_leases_and_work_items.sql` | run records, steps, events, checkpoints, leases, work items |
| Policy, budget, and tools | `0023_policy_bundles_and_snapshots.sql`–`0032_tool_definitions_and_invocations.sql` | policy bundles, decisions, tickets, budget accounts, tool definitions |
| Execution and artifacts | `0037_execution_plans_revisions_steps_validation.sql`–`0042_artifacts_revisions_blobs_and_provenance.sql` | execution plans, revisions, steps, artifacts, blobs |
| Conversations and connectors | `0048_conversations_channels_and_triggers.sql`–`0062_a2a_interoperability_gateway.sql` | conversations, channels, triggers, connectors, agent packages, A2A gateway |
| Observability and product | `0063_observability_and_metric_rollups.sql`–`0090_release_orchestration_and_manifests.sql` | metrics, product surface, AG-UI gateway, transfers, releases, webhooks |
| Enterprise and extensions | `0087_workspace_envelope_encryption_and_cross_sharing.sql`–`0088_event_schemas_exports_and_capture_profiles.sql` | envelope encryption, cross-workspace sharing, event schemas, capture profiles |
| Run event store foundation | `0111_run_event_store_foundation.sql` | run events table, stream registry |
| Run streams and recovery | `0112_run_streams_and_recovery_checkpoints.sql` | run streams, recovery checkpoints |

When documentation and migration SQL differ, migration SQL is authoritative.

## Run records

The API persists run metadata in `agent_runs`:

```text
id
workspace_id
principal_id
title
status
run_version
created_at
updated_at
```

The canonical write path appends events to the run event store and upserts the `agent_runs` projection atomically via `PgRunCommandCommitter`. Run-event and checkpoint tables support deterministic reduction, replay, and checkpoint restoration in the application layer.

## Run event store

Migrations `0111` and `0112` establish the event-sourcing infrastructure:

- `run_events` — append-only event envelopes with `sequence`, `event_type`, `event_version`, `payload`, `occurred_at`, `recorded_at`.
- `run_streams` — stream registry with `workspace_id`, `run_id`, `current_version` for optimistic concurrency.
- `run_checkpoints` — serialized state snapshots with `state_hash` (SHA-256) and `format_version`.

## Row-Level Security isolation

Multi-tenant tables enable Row-Level Security using workspace-bound policies such as:

```sql
ALTER TABLE <table_name> ENABLE ROW LEVEL SECURITY;

CREATE POLICY <table_name>_workspace_isolation ON <table_name>
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
```

`PgStore::begin_scoped` sets `vestrace.workspace_id` and `vestrace.principal_id` with transaction-local `set_config(..., true)` calls. Run persistence uses that scoped transaction boundary and also includes explicit workspace predicates for reads.

## Runtime compatibility

Readiness requires the applied `_sqlx_migrations` rows to match the embedded migration set by version, success state, and checksum. A database with missing, extra, failed, or modified migration records is not considered ready.
