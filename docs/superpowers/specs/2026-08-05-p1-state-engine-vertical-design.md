# Vestrace P1 State Engine Recovery Vertical Design

**Status:** Implementation-ready  
**Date:** 2026-08-05  
**Base:** `agent/p0-restore-main`  
**Integrated foundation:** R1.1–R1.3 from `agent/r1-projection-integration`

## 1. Objective

Complete P1 / PR1 as one recoverable vertical path:

```text
CreateRun
→ append-only RunEvent stream
→ pure reducer
→ checkpoint
→ replay
→ projection rebuild
```

The event stream is authoritative. `agent_runs` and checkpoints are derived recovery data. Deleting an `agent_runs` row must not delete or invalidate the event stream.

The primary acceptance scenario is:

```text
create run
→ execute several transitions
→ create checkpoint
→ execute additional transitions
→ delete agent_runs projection
→ validate checkpoint against authoritative events
→ restore checkpoint plus event tail
→ rebuild projection
→ obtain exactly the same RunState and AgentRun projection
```

## 2. Existing foundation

The integrated R1 stack already provides:

- typed commands and events;
- versioned event envelopes;
- pure `decide`, `apply`, and `replay` functions;
- PostgreSQL ordered event loading and optimistic append;
- canonical command execution;
- atomic event plus projection commits.

The existing storage coupling is not sufficient for recovery: `run_events` has a cascading foreign key to `agent_runs`. Deleting the projection therefore deletes the authoritative events. P1 must reverse that dependency before checkpoints or rebuild can be truthful.

## 3. Considered approaches

### A. Checkpoints linked directly to `agent_runs`

This is the smallest schema change, but it preserves the incorrect authority direction. Projection deletion would either cascade checkpoints/events or be blocked by foreign keys. Rejected.

### B. Opaque checkpoints without independent stream metadata

This separates checkpoints from the projection, but optimistic concurrency still locks `agent_runs`. A missing projection prevents safe continuation, and event ownership remains ambiguous. Rejected.

### C. Independent `run_streams` metadata plus derived projections

Create one durable stream identity row per run. Events and checkpoints belong to the stream. Command commits lock and advance the stream row, then upsert the derived projection. Projection deletion no longer affects authority. Selected.

This adds one small metadata table but removes the architectural inversion and gives recovery a stable concurrency boundary.

## 4. Authority model

```text
run_streams                durable stream identity and optimistic head
  ├── run_events           canonical append-only facts
  ├── run_checkpoints      derived acceleration snapshots
  └── agent_runs           derived query projection
```

Authority rules:

1. `run_events` are the canonical history.
2. `run_streams.current_version` is transactionally maintained stream-head metadata, not business state.
3. `run_checkpoints` may accelerate restoration but cannot override events.
4. `agent_runs` may be deleted and rebuilt without changing the stream.
5. Every command transition must append events through the canonical State Engine boundary.

## 5. PostgreSQL model

### 5.1 `run_streams`

```sql
run_streams (
    workspace_id UUID NOT NULL,
    run_id UUID NOT NULL,
    current_version BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (workspace_id, run_id),
    CHECK (current_version >= 0)
)
```

Migration behavior:

- backfill one stream row for every existing `agent_runs` row;
- set `current_version` to the maximum event sequence when events exist, otherwise the projection version;
- replace the `run_events → agent_runs` foreign key with `run_events → run_streams`;
- add `agent_runs → run_streams` as a derived-child relationship with no cascade from projection to stream;
- enable and force workspace RLS;
- runtime role receives only the operations required by command execution and recovery.

### 5.2 `run_checkpoints`

```sql
run_checkpoints (
    workspace_id UUID NOT NULL,
    run_id UUID NOT NULL,
    sequence BIGINT NOT NULL,
    format_version SMALLINT NOT NULL,
    state_hash TEXT NOT NULL,
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (workspace_id, run_id, sequence),
    FOREIGN KEY (workspace_id, run_id)
        REFERENCES run_streams (workspace_id, run_id)
        ON DELETE CASCADE,
    CHECK (sequence > 0),
    CHECK (format_version = 1),
    CHECK (state_hash ~ '^[0-9a-f]{64}$')
)
```

Checkpoint rows are immutable to the runtime role. Duplicate creation at the same sequence is idempotent only when the serialized state and hash are identical; a different snapshot at the same sequence is a conflict.

## 6. Checkpoint format

`RunCheckpoint` contains:

- workspace ID;
- run ID;
- sequence/version;
- format version `1`;
- complete serialized `RunState`;
- SHA-256 hash of canonical serialized state;
- creation timestamp.

Canonical state bytes are `serde_json::to_vec(&RunState)` using the current typed structure. The hash algorithm and format version are explicit so a future format can coexist without silently reinterpreting old data.

A checkpoint is locally valid only when:

- its format version is supported;
- JSON deserialization succeeds;
- state workspace and run IDs match the row;
- `state.version == checkpoint.sequence`;
- the recomputed SHA-256 equals `state_hash`.

A checkpoint is authoritatively valid only when replaying events from sequence 1 through the checkpoint sequence yields exactly the stored `RunState`.

## 7. Application boundaries

### 7.1 Shared projection mapper

Move `RunState → AgentRun` mapping out of `RunCommandService` into a focused reusable function:

```rust
pub fn project_run(state: &RunState) -> AgentRun
```

Command execution and rebuild must use the same mapper.

### 7.2 Recovery ports

```rust
#[async_trait]
pub trait RunRecoveryStore: Send + Sync {
    async fn load_stream_head(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunVersion>, ApplicationError>;

    async fn load_events_through(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        through: RunVersion,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError>;

    async fn load_events_after(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        after: RunVersion,
    ) -> Result<Vec<RunEventEnvelope>, ApplicationError>;

    async fn load_latest_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
    ) -> Result<Option<RunCheckpoint>, ApplicationError>;

    async fn save_checkpoint(
        &self,
        context: &RequestContext,
        checkpoint: &RunCheckpoint,
    ) -> Result<(), ApplicationError>;

    async fn replace_projection(
        &self,
        context: &RequestContext,
        expected_stream_version: RunVersion,
        projection: &AgentRun,
    ) -> Result<(), ApplicationError>;
}
```

The PostgreSQL adapter scopes every operation by workspace and principal context. `replace_projection` locks the stream row, rechecks its version, and upserts `agent_runs` in one transaction.

### 7.3 Recovery service

```rust
pub struct RunRecoveryService {
    store: SharedRunRecoveryStore,
}
```

Operations:

- `create_checkpoint(context, run_id)`:
  - load stream head;
  - load events through the head;
  - replay deterministically;
  - verify state version equals stream head;
  - hash and save an immutable checkpoint.

- `validate_checkpoint(context, run_id, sequence)`:
  - load authoritative prefix;
  - replay it;
  - compare exact state and state hash;
  - return an explicit validation result or storage-corruption error.

- `restore(context, run_id)`:
  - load stream head and latest checkpoint;
  - validate checkpoint structure and hash;
  - load events after the checkpoint;
  - apply the tail to the checkpoint state;
  - require final state version to equal stream head;
  - when no checkpoint exists, replay the complete stream.

- `rebuild_projection(context, run_id)`:
  - authoritatively validate the selected checkpoint against its event prefix;
  - restore checkpoint plus tail;
  - map state through `project_run`;
  - replace the projection only if the stream head is unchanged.

The service never treats `agent_runs` as input to state reconstruction.

## 8. Canonical commit changes

`PgRunCommandCommitter` and `PgRunEventStore` must lock `run_streams`, not `agent_runs`.

Create path:

```text
insert run_streams
→ insert events
→ insert/upsert agent_runs projection
→ commit
```

Existing-stream path:

```text
lock run_streams
→ verify current_version == expected_version
→ validate stored events
→ append events
→ advance run_streams.current_version
→ upsert agent_runs projection
→ commit
```

A missing projection is recoverable and must not prevent an otherwise valid command commit. The command commit recreates the projection from the resulting state.

Direct `RunRepository::create` remains a legacy P0 path until HTTP command integration, but it must create the matching stream row atomically so database invariants remain valid. It must not create events or pretend to be canonical command execution.

## 9. Failure semantics

- missing stream: domain not found;
- stream head mismatch: conflict;
- event sequence gap or duplicate: storage corruption/conflict;
- unsupported event or checkpoint format: storage corruption;
- checkpoint hash mismatch: storage corruption;
- checkpoint state differs from authoritative prefix replay: storage corruption;
- projection replacement after stream advancement: conflict, no stale write;
- cross-workspace access: hidden by RLS and reported as not found/policy failure without leaking existence;
- any SQL failure: transaction rollback and sanitized application error.

Recovery never silently hides an invalid checkpoint or corrupted prefix behind an existing projection.

## 10. Testing strategy

### Domain/application tests

- deterministic state hashing;
- checkpoint structural validation;
- checkpoint creation from a complete stream;
- restore with no checkpoint;
- restore from checkpoint plus event tail;
- invalid hash rejection;
- checkpoint/prefix state mismatch rejection;
- stream-head mismatch rejection;
- projection mapping equality between command execution and rebuild.

### PostgreSQL integration tests

- migration backfills stream rows;
- deleting `agent_runs` preserves `run_streams`, `run_events`, and checkpoints;
- command commit recreates a missing projection;
- optimistic concurrency is enforced by `run_streams`;
- checkpoint save/load round-trips typed state;
- conflicting duplicate checkpoint is rejected;
- rebuild upserts an identical projection;
- concurrent stream advancement prevents stale rebuild;
- runtime role cannot update/delete events or checkpoints;
- workspace RLS isolates streams, events, checkpoints, and projections.

### End-to-end acceptance

The mandatory scenario executes multiple typed transitions, checkpoints at an intermediate version, executes a tail, deletes the projection, validates and restores the checkpoint, rebuilds the projection, and compares the rebuilt state and projection with the pre-deletion values.

## 11. Scope boundaries

Included:

- integration of R1.1–R1.3 over P0;
- independent stream identity;
- checkpoints;
- validation;
- replay and restoration;
- projection rebuild;
- corruption detection required for this vertical.

Excluded:

- HTTP command migration and persisted idempotency;
- pause/resume retry counters beyond existing typed waiting transitions;
- delegated subagents;
- compensation;
- capability policy;
- Artifact CAS;
- workers, schedulers, memory expansion, and new UI.

Those remain ordered after P1 / PR1 by the approved P0–P4 amendment.

## 12. Exit conditions

This design is complete only when:

1. P0 and the integrated typed core pass together in CI.
2. Events survive projection deletion.
3. A checkpoint can be created and authoritatively validated.
4. Checkpoint plus tail restoration produces the same `RunState` as full replay.
5. A deleted projection is rebuilt identically.
6. Stream corruption or invalid checkpoint data causes an explicit failure.
7. Optimistic concurrency no longer depends on the projection row.
8. All writes remain workspace-scoped and transactional.
9. No P1 PR2+, P2, P3, or P4 scope is introduced.
