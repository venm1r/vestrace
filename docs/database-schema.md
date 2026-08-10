# PostgreSQL Schema & Migrations Reference

## Documentation status

This document describes the **current implementation schema foundation** for the source snapshot behind `docs/architecture-v0.2`.

It is not the target v0.2 domain schema specification. The normative target model is defined in:

- [`specs/vestrace-domain-model-v0.2.md`](specs/vestrace-domain-model-v0.2.md)
- [`specs/vestrace-data-temporal-model-v0.2.md`](specs/vestrace-data-temporal-model-v0.2.md)
- [`specs/vestrace-crypto-data-governance-contract-v0.2.md`](specs/vestrace-crypto-data-governance-contract-v0.2.md)

No migrations are added or changed during this documentation phase.

## Migration source of truth

The ordered [`migrations/`](../migrations/) directory is the authoritative implementation migration history. Documentation must not maintain a claimed maximum migration number.

Runtime compatibility checks applied `_sqlx_migrations` entries against the embedded migration set by version, success state and checksum. Missing, extra, failed or modified migration records make the database not ready.

## Current storage role

PostgreSQL is the primary authoritative transactional store for the current implementation: workspaces/principals, Run state/history, Memory state/revisions/provenance, jobs/outbox/idempotency, retrieval journals, audit and related product metadata.

The target architecture preserves PostgreSQL as the authoritative transactional/domain-state store, while target artifact bytes may live in governed local content-addressed storage. A CAS hash identifies/integrity-checks bytes; it does not replace domain metadata or authorization.

## Current major schema areas

The migration history contains foundations for:

- extensions, identity and RLS;
- sessions/events/memory/revisions/provenance/relations;
- retrieval journals/context packs;
- policy/approval/audit/provider/model foundations;
- agents/skills/workflows and execution-related structures;
- Run event store/streams/checkpoints;
- jobs/outbox/idempotency;
- security/product/enterprise-oriented schema additions.

The source migration SQL, not this summary, is authoritative for exact table/column definitions.

## Run event store

Current Run persistence uses append-oriented event history plus projections and recovery checkpoints.

The current foundation includes:

- `run_events` — sequenced event envelopes;
- `run_streams` — current stream version / optimistic concurrency;
- `run_checkpoints` — serialized recovery state with integrity metadata;
- `agent_runs` — current/read projection metadata.

The application layer replays canonical Run history and can rebuild projections.

## Memory foundation

Current schema supports Memory identity/revisions, provenance/relations and retrieval-related projections/journals.

Database-level integrity includes append-only event enforcement and an invariant requiring active memories to have a source in the current implementation snapshot.

The v0.2 target Domain Model introduces additional semantics such as Claim, richer Evidence/Derivation, conflicts, sharing, health/repair, external effects and governance. Their appearance in target docs does **not** imply corresponding migration tables currently exist.

## Row-Level Security

Multi-tenant tables use workspace-bound RLS policies. Conceptually:

```sql
ALTER TABLE <table_name> ENABLE ROW LEVEL SECURITY;

CREATE POLICY <table_name>_workspace_isolation ON <table_name>
    FOR ALL
    USING (workspace_id = vestrace_current_workspace_id())
    WITH CHECK (workspace_id = vestrace_current_workspace_id());
```

Scoped transactions establish workspace/principal context. Application-level authorization remains required in addition to RLS.

## Derived-state rule

Target v0.2 requires derived indexes/caches/projections to be rebuildable from more authoritative state. Database presence does not automatically make a table authoritative; authority is defined by the Domain/Architecture Contract.

## Schema evolution rule

- migrations remain forward-only implementation artifacts;
- applied migrations are not silently rewritten;
- new target requirements require explicit migration planning only after documentation completion;
- migration qualification must later verify data/history/invariant preservation.
