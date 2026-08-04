# Vestrace P1 State Engine Recovery Vertical Design

**Status:** Implemented in draft PR #28  
**Date:** 2026-08-05  
**Base:** `agent/p0-restore-main`  
**Integrated foundation:** R1.1–R1.3 from `agent/r1-projection-integration`

## 1. Objective

P1 / PR1 provides one recoverable State Engine path:

```text
CreateRun
→ append-only RunEvent stream
→ pure reducer
→ checkpoint
→ replay
→ projection rebuild
```

The event stream is authoritative. `agent_runs` and `run_checkpoints` are derived data. Deleting an `agent_runs` row must not delete or invalidate the event stream or its checkpoints.

The mandatory acceptance scenario is:

```text
create run
→ execute typed transitions
→ create an intermediate checkpoint
→ execute an event tail
→ delete the agent_runs projection
→ validate the checkpoint against authoritative events
→ restore checkpoint plus bounded event tail
→ rebuild the projection
→ obtain exactly the pre-deletion RunState and AgentRun projection
```

## 2. Integrated typed foundation

The vertical includes:

- typed run commands and events;
- versioned event envelopes;
- pure `decide`, `apply`, and `replay` functions;
- canonical command execution;
- atomic event and projection commits;
- optimistic stream concurrency;
- PostgreSQL recovery storage;
- typed checkpoint validation and projection rebuild.

HTTP command migration, persisted idempotency, retry policy, delegated subagents, compensation, capabilities, Artifact CAS, workers, schedulers, memory expansion, and new UI remain outside this PR.

## 3. Authority model

```text
run_streams                durable stream identity and optimistic head
  ├── run_events           canonical append-only facts
  ├── run_checkpoints      derived acceleration snapshots
  └── agent_runs           derived query projection
```

Authority rules:

1. `run_events` are the canonical business history.
2. `run_streams.current_version` is transactionally maintained stream-head metadata.
3. `run_checkpoints` accelerate restoration but never override events.
4. `agent_runs` may be deleted and rebuilt without changing authority.
5. Canonical command transitions append events through the State Engine boundary.
6. Optimistic concurrency locks `run_streams`, never the projection.
7. A projection-only legacy row is not promoted into invented event history.

## 4. PostgreSQL model

### 4.1 `run_streams`

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

- create one stream row for each existing `agent_runs` identity;
- use the maximum event sequence when authoritative events exist;
- use version `0` when no events exist, regardless of a legacy projection version;
- replace `run_events → agent_runs` ownership with `run_events → run_streams`;
- link derived projections and checkpoints to the stream without projection-to-stream cascade;
- enable and force workspace RLS;
- grant the runtime role only required operations.

A compatibility trigger remains during the legacy direct-create boundary. It may only seed a missing version-zero stream for a projection insert. It cannot create events or advance stream history. It is removed when all projection-only create surfaces are retired.

### 4.2 `run_checkpoints`

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

Checkpoint rows are immutable to the runtime role. A duplicate write at one sequence is idempotent only when format, state, hash, and creation timestamp are identical. Different content at the same sequence is a conflict.

## 5. Checkpoint format

`RunCheckpoint` contains:

- workspace ID;
- run ID;
- sequence/version;
- format version `1`;
- complete serialized `RunState`;
- SHA-256 hash of canonical serialized state;
- creation timestamp.

Canonical state bytes are produced by `serde_json::to_vec(&RunState)`. The format version and hash algorithm are explicit so future formats cannot silently reinterpret existing rows.

`created_at` is normalized to PostgreSQL microsecond precision before the checkpoint is persisted and returned. The value returned by `create_checkpoint` therefore equals the value read back from storage.

A checkpoint is locally valid only when:

- its format version is supported;
- JSON deserialization succeeds;
- workspace and run IDs match the row and request context;
- `state.version == checkpoint.sequence`;
- the recomputed SHA-256 equals `state_hash`.

A checkpoint is authoritatively valid only when replaying events from sequence `1` through the checkpoint sequence yields exactly the stored `RunState`.

## 6. Application boundaries

### 6.1 Shared projection mapper

Command execution and rebuild use the same mapper:

```rust
pub fn project_run(state: &RunState) -> AgentRun
```

### 6.2 Recovery storage port

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

    async fn load_checkpoint(
        &self,
        context: &RequestContext,
        run_id: AgentRunId,
        sequence: RunVersion,
    ) -> Result<Option<RunCheckpoint>, ApplicationError>;

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

Every PostgreSQL operation is explicitly scoped by workspace and also protected by forced RLS. `replace_projection` locks the stream, rechecks the expected head, and upserts the derived projection in one transaction.

## 7. Recovery snapshot semantics

A recovery call reconstructs one internally consistent stream snapshot.

Algorithm:

1. Read and capture `run_streams.current_version` as `observed_head`.
2. Read the latest available checkpoint.
3. Use the checkpoint only when `checkpoint.sequence <= observed_head`.
4. If the latest checkpoint is newer than `observed_head`, ignore it for this call and replay events through `observed_head`.
5. Validate a selected checkpoint against its authoritative event prefix.
6. Load events after the checkpoint and apply only events with `sequence <= observed_head`.
7. Ignore events committed after `observed_head`; they belong to a later recovery snapshot.
8. Require the restored state version to equal `observed_head`.

A newer event or checkpoint observed after the head read is not corruption. It is concurrent progress and is excluded from the captured snapshot.

A malformed checkpoint, invalid hash, mismatched identity, unsupported format, or state that differs from its authoritative prefix remains an explicit storage-corruption error. Recovery never hides such corruption behind the projection.

## 8. Recovery operations

### `create_checkpoint`

- capture the stream head;
- load events through that head;
- replay deterministically;
- require state version to equal the captured head;
- hash the typed state;
- normalize timestamp precision;
- save an immutable checkpoint.

### `validate_checkpoint`

- load the requested checkpoint;
- validate structure, identity, format, and hash;
- replay the authoritative event prefix;
- require exact state equality.

### `restore`

- capture one stream head;
- select only a checkpoint not newer than that head;
- validate the selected checkpoint;
- apply an event tail bounded by the captured head;
- otherwise replay the full stream through the captured head.

### `rebuild_projection`

- restore a typed state snapshot;
- read the stream head again;
- reject the rebuild if the stream advanced;
- map through `project_run`;
- replace the projection under a stream-head guard.

The service never reads `agent_runs` as input to state reconstruction.

## 9. Canonical commit semantics

`PgRunCommandCommitter` and `PgRunEventStore` lock `run_streams`.

Create path:

```text
insert or claim run_streams at version 0
→ validate expected version
→ insert events
→ advance stream head
→ insert/upsert agent_runs projection
→ commit
```

Existing-stream path:

```text
lock run_streams
→ verify current_version == expected_version
→ verify stored event head agrees with stream metadata
→ append events
→ advance run_streams.current_version
→ upsert agent_runs projection
→ commit
```

A missing projection does not prevent a valid command. The command service replays the stream, decides the transition, and the committer recreates the projection from the resulting state.

Direct `RunRepository::create` remains a temporary P0 compatibility path until HTTP command integration. It creates a matching version-zero stream atomically, creates no events, and does not claim canonical State Engine execution.

## 10. Failure semantics

- missing stream: domain not found;
- stale expected version: conflict;
- stream metadata/event-head divergence: storage corruption;
- event sequence gap or invalid envelope metadata: conflict or storage corruption;
- unsupported event/checkpoint format: storage corruption;
- checkpoint hash or authoritative-prefix mismatch: storage corruption;
- duplicate immutable checkpoint with different data: conflict;
- projection replacement after stream advancement: conflict with no stale write;
- cross-workspace access: hidden by RLS and reported without leaking existence;
- SQL failure: transaction rollback and sanitized application error.

## 11. Test coverage

Application and integration coverage includes:

- deterministic checkpoint hashing;
- checkpoint timestamp round-trip equality;
- checkpoint structural and authoritative validation;
- full replay without checkpoint;
- checkpoint plus bounded tail restoration;
- rejection of invalid hash and rehashed tampering;
- events committed after an observed head are ignored for that snapshot;
- checkpoints created after an observed head are ignored for that snapshot;
- projection mapping equality between command execution and rebuild;
- migration and recovery table constraints;
- projection deletion preserving stream, events, and checkpoints;
- command continuation after projection deletion;
- concurrent writers serialized by stream head;
- immutable checkpoint idempotency/conflict behavior;
- stale rebuild rejection;
- runtime privileges and workspace RLS;
- end-to-end checkpoint, tail, projection deletion, restore, and rebuild.

## 12. Scope boundaries

Included:

- R1.1–R1.3 integrated over P0;
- independent stream identity;
- typed checkpoints;
- authoritative validation;
- deterministic replay and restoration;
- projection rebuild;
- corruption and snapshot-race handling required by this vertical.

Deferred:

- HTTP command migration and persisted idempotency;
- pause/resume retry counters beyond existing typed transitions;
- delegated subagents;
- compensation;
- capability and risk policy;
- Artifact CAS;
- workers, schedulers, memory expansion, and new UI.

## 13. Exit conditions

P1 / PR1 satisfies its design gate when:

1. P0 and the integrated typed core pass together in CI.
2. Events and checkpoints survive projection deletion.
3. A checkpoint can be created and authoritatively validated.
4. Checkpoint plus bounded tail equals full replay at the captured head.
5. Concurrent newer events/checkpoints do not corrupt an older snapshot.
6. A deleted projection is rebuilt identically.
7. Invalid checkpoint or stream data fails explicitly.
8. Optimistic concurrency no longer depends on a projection row.
9. All writes remain workspace-scoped and transactional.
10. No P1 PR2+, P2, P3, or P4 scope is introduced.

Verification evidence is recorded in the corresponding P1 / PR1 exit-gate report and draft PR.