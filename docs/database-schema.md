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
| Durable runs and later capabilities | Later ordered migration files | run records, events, checkpoints, policies, artifacts, product and integration registries |

When documentation and migration SQL differ, migration SQL is authoritative.

## P0 run records

The P0 API persists run metadata in `agent_runs`:

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

P0 exposes create, list, and get operations for these records. The existence of run-event and checkpoint tables does not imply that deterministic reduction, replay, or checkpoint restoration is implemented in the application layer.

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
