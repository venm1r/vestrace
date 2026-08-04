# R1.2 PostgreSQL Run Event Store Foundation Design

**Status:** Approved

## Goal

Persist the R1.1 `RunEventEnvelope` in PostgreSQL as an append-only, workspace-scoped stream with deterministic decoding, ordered loading, and optimistic append. This slice does not change HTTP behavior or make `agent_runs` event-derived yet.

## Existing constraints

- The branch is stacked on `agent/r1-domain-kernel`.
- Rust 1.85 and edition 2024 remain the version floor.
- `#![forbid(unsafe_code)]` remains in force.
- Current P0 create/list/get behavior must remain unchanged.
- `run_events` already exists from migration `0020_run_events_and_checkpoints.sql`; it must be evolved in place.
- Workspace isolation continues to use transaction-local `vestrace.workspace_id` and `vestrace.principal_id`.

## Chosen approach

Extend the existing `run_events` table rather than creating a competing table or dropping data. Add the event envelope fields required by R1.1, backfill legacy rows deterministically, then enforce non-null and integrity constraints.

## Migration contract

Create `migrations/0111_run_event_store_foundation.sql`.

Add:

- `event_version SMALLINT`
- `actor JSONB`
- `causation_id UUID`
- `correlation_id UUID`
- `occurred_at TIMESTAMPTZ`

Retain `created_at` as the stored recording timestamp for compatibility. In Rust it maps to `RunEventEnvelope.recorded_at`.

Legacy rows are backfilled as follows:

- `event_version = 1`
- `actor = {"system":{"component":"legacy_migration"}}`
- `causation_id = id`
- `correlation_id = run_id`
- `occurred_at = created_at`

Add or replace integrity constraints:

- `sequence > 0`
- `event_version > 0`
- trimmed `event_type` is non-empty
- unique `(workspace_id, run_id, sequence)`
- composite foreign key `(workspace_id, run_id)` to `agent_runs(workspace_id, id)`

Add index `(workspace_id, run_id, sequence)` for ordered stream reads. Do not add a GIN index on payload.

## Application boundary

Add a dedicated `RunEventStore` port under `vestrace-application::runs`. It is separate from `RunRepository`, which remains the P0 projection repository.

```rust
#[async_trait]
pub trait RunEventStore: Send + Sync {
    async fn load_stream(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError>;

    async fn append(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        expected_version: RunVersion,
        events: &[RunEventEnvelope],
    ) -> Result<RunVersion, ApplicationError>;
}
```

`append` returns the final stored version. An empty event batch is rejected.

## PostgreSQL adapter

Add `PgRunEventStore` under `crates/vestrace-infrastructure/src/postgres/run_event_store.rs`.

### Loading

- Open a scoped transaction.
- Select rows using explicit `workspace_id` and `run_id` filters.
- Order by `sequence ASC`.
- Decode actor and payload through Serde.
- Validate stored `event_type` and `event_version` through the R1.1 reducer contract when replayed; storage decoding must not silently rewrite values.
- Commit the read transaction after successful decoding.

### Optimistic append

Within one scoped transaction:

1. Validate the batch identity and contiguous sequence in memory.
2. Read `MAX(sequence)` for `(workspace_id, run_id)` while locking the owning `agent_runs` row with `FOR UPDATE`.
3. Treat no events as `RunVersion::ZERO`.
4. Compare the current version with `expected_version`.
5. Insert every event row.
6. Commit and return the last sequence.

The `agent_runs` row lock serializes concurrent appends before R1.3 introduces projection compare-and-set updates. Unique constraints remain the final database guard.

## Error behavior

- Stale expected version returns `ApplicationError::Conflict` with expected and actual versions.
- Empty batches, mixed run/workspace identities, non-contiguous sequence, or a first sequence not equal to `expected_version + 1` return `ApplicationError::Domain` or `ApplicationError::Conflict` without writing rows.
- JSON decode failures, unsupported persisted representations, and SQL failures return `ApplicationError::Storage`.
- A missing owning `agent_runs` row returns a storage/not-found-compatible failure; a typed NotFound variant is deferred to the shared error-taxonomy slice.

## RLS and permissions

`run_events` remains RLS-protected by workspace. Restricted runtime roles may `SELECT` and `INSERT` but must not be granted `UPDATE`, `DELETE`, or `TRUNCATE`. Tests verify cross-workspace invisibility and insert rejection.

## Testing strategy

TDD order:

1. Migration shape and backfill tests.
2. Application port compile contract.
3. Stream round-trip and ordering tests.
4. Stale-version and sequence validation tests.
5. Atomic batch rollback test.
6. Concurrent append test.
7. Restricted-role RLS tests.
8. Full workspace, console, Compose, smoke, and RLS regression checks.

## Explicitly deferred

- Event-derived updates to `agent_runs`.
- Command idempotency storage.
- Checkpoint persistence changes.
- Rebuild.
- HTTP migration to command execution.
- Outbox, jobs, providers, capabilities, approvals, and agent loops.
