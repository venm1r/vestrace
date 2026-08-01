# PostgreSQL Schema & Migrations Reference

Vestrace uses PostgreSQL with `pgvector` and `pg_trgm` extensions as its authoritative relational store. All migrations are forward-only and applied sequentially.

## Migration History

| Migration Script | Description | Primary Tables Created |
| :--- | :--- | :--- |
| `0001_extensions.sql` | Enables PostgreSQL extensions | `vector`, `pg_trgm` |
| `0002_identity_and_workspaces.sql` | Tenant identity & RBAC schema | `workspaces`, `principals`, `roles`, `capabilities` |
| `0003_rls_baseline.sql` | Row-Level Security helper functions | RLS policies on identity tables |
| `0004_sessions_and_events.sql` | Append-only event ingestion | `sessions`, `events` |
| `0005_memories_and_revisions.sql` | Core memory & revision history | `memories`, `memory_revisions` |
| `0006_provenance_scopes_relations.sql` | Knowledge graph & derivations | `derivations`, `memory_sources`, `knowledge_relations` |
| `0007_idempotency_jobs_outbox.sql` | Worker queues & audit logs | `idempotency_keys`, `jobs`, `outbox`, `purge_audits` |
| `0008_search_documents.sql` | Full-text & trigram search index | `search_documents` |
| `0009_embedding_spaces.sql` | Vector search & embeddings | `embedding_spaces`, `memory_embeddings` |
| `0010_structured_search_indexes.sql` | Entity-attribute search index | `structured_search_indexes` |
| `0011_retrieval_journal_and_context_packs.sql` | Context pack caching & audit | `retrieval_runs`, `context_packs` |
| `0012_tokens_policies_and_approvals.sql` | Access tokens & approvals | `access_tokens`, `approval_records` |
| `0013_audit_and_redaction.sql` | Security audit & redaction | `audit_events`, `redaction_rules` |
| `0014_provider_and_model_registry.sql` | AI provider & model registry | `providers`, `models` |
| `0015_routing_executions_and_evaluations.sql` | Execution logs & routing | `routing_decisions`, `model_executions` |
| `0016_agents_skills_workflows.sql` | Cognitive asset registry | `agents`, `skills` |

## Row-Level Security (RLS) Isolation

Every multi-tenant table enables Row-Level Security:

```sql
ALTER TABLE <table_name> ENABLE ROW LEVEL SECURITY;

CREATE POLICY <table_name>_workspace_isolation ON <table_name>
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
```

Session variable `vestrace.workspace_id` is automatically populated by `PgTransactionManager` on every scoped transaction.
