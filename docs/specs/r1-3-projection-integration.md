# R1.3 Projection Integration

## Status

Approved implementation slice stacked on R1.2 (`agent/r1-event-store-foundation`).

## Objective

Make command execution the first path that atomically persists both:

1. the authoritative append-only run events; and
2. the current `agent_runs` read projection derived from the resulting `RunState`.

The event stream remains authoritative. `agent_runs` is a rebuildable projection and must never advance independently of the event stream.

## Scope

R1.3 adds:

- an application command-execution use case around the existing domain `decide`, `replay`, and `apply` functions;
- deterministic conversion from `RunState` to the existing `AgentRun` projection shape;
- an application port that commits a validated event batch and the resulting projection atomically;
- a PostgreSQL adapter that supports both first-write creation and existing-stream updates;
- optimistic concurrency based on the command's `expected_version`;
- integration tests proving atomicity, projection correctness, stale-version rejection, and workspace isolation.

## Command flow

1. Load the run event stream for `(workspace_id, run_id)`.
2. Replay it into `Option<RunState>`.
3. Validate the command with domain `decide`.
4. Convert pending events into complete `RunEventEnvelope` values with contiguous sequence numbers.
5. Apply the new envelopes to derive the resulting `RunState`.
6. Convert the resulting state into the `AgentRun` projection.
7. Atomically commit the events and projection with an optimistic version check.
8. Return the resulting projection and committed envelopes.

## Application contracts

### `RunCommandCommitter`

The committer accepts:

- request context;
- run id;
- expected version;
- non-empty, contiguous event envelope batch;
- the exact projection derived from the resulting state.

It returns the committed run version or a conflict/storage error.

### `RunCommandExecutor`

The executor accepts a `RunCommandEnvelope` and returns a `RunCommandResult` containing:

- the resulting `AgentRun` projection;
- the committed event envelopes.

The executor contains no SQLx types and does not mutate the projection directly.

## PostgreSQL transaction rules

For `RunCommand::Create`:

- verify the expected version is zero;
- verify no owning run row exists in the workspace;
- insert the derived `agent_runs` projection;
- insert the first event batch;
- commit both or neither.

For an existing run:

- lock the owning `agent_runs` row with `FOR UPDATE`;
- compare `run_version` with the command expected version;
- insert the event batch;
- update the projection using a guarded `WHERE run_version = expected_version` condition;
- commit both or neither.

The adapter must reject a projection whose identity, final version, status, or timestamps do not match the supplied envelopes/resulting state contract.

## Invariants

- Every committed projection version equals the last committed event sequence.
- No projection update can commit without all corresponding events.
- No event batch can commit without the corresponding projection update.
- `created_at` is stable after the first event.
- `updated_at` equals the occurrence time of the final committed event.
- Workspace, run, and principal identities cannot cross tenant boundaries.
- Concurrent writers using the same expected version cannot both succeed.
- The existing P0 HTTP create/list/get adapter is unchanged in R1.3.

## Deliberately deferred

- HTTP migration from direct repository writes to command execution;
- persisted idempotency keys and command receipts;
- outbox messages in the same transaction;
- projection rebuild/checkpoints;
- additional projections beyond `agent_runs`;
- jobs, providers, approvals, capabilities, and agent loops.
