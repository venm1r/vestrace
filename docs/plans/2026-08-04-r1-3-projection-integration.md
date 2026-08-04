# R1.3 Projection Integration Implementation Plan

## Goal

Introduce a command execution path that derives `agent_runs` from the authoritative event stream and commits the new events plus projection atomically.

## Branch strategy

- Base: `agent/r1-event-store-foundation`
- Head: `agent/r1-projection-integration`
- Delivery: stacked draft PR
- `main` remains untouched

## Task 1 — Application contract RED

Create compile-time/application tests expecting:

- `RunCommandCommitter`;
- `RunCommandExecutor`;
- `RunCommandResult`;
- `RunCommandService` accepting an event store and committer.

Expected RED: compilation fails only because the new contracts do not exist.

## Task 2 — Minimal application execution GREEN

Implement:

- full envelope creation from `PendingRunEvent`;
- stream replay;
- domain decision dispatch;
- application error mapping;
- resulting state derivation;
- `RunState` to `AgentRun` projection conversion;
- committer invocation with expected version.

Unit tests:

- create command produces one `run.created` event and a version-1 projection;
- existing command replays prior events and advances status/version;
- invalid transitions do not call the committer;
- stale expected versions surface as conflicts;
- generated envelopes preserve actor, causation, correlation, occurrence, and recording time.

## Task 3 — PostgreSQL committer RED

Create integration tests expecting `PgRunCommandCommitter`.

Test cases:

- create commits projection and event together;
- existing command advances projection and appends event;
- stale version rejects both writes;
- event insertion failure rolls back projection creation/update;
- projection identity mismatch is rejected before SQL;
- concurrent writers at one expected version yield one success and one conflict;
- workspace isolation is preserved.

Expected RED: compilation fails only because the PostgreSQL adapter does not exist.

## Task 4 — PostgreSQL committer GREEN

Implement the smallest adapter that:

- opens a scoped transaction;
- validates batch/projection identity and final version;
- inserts a new projection before the first event batch;
- locks and guards existing projections;
- inserts all events;
- updates the projection with optimistic version protection;
- commits or rolls back as one unit.

Reuse the R1.2 envelope serialization and status mapping without weakening event-store constraints.

## Task 5 — Boundary hardening

Add tests for:

- projection version equals final sequence;
- `created_at` stability;
- `updated_at` equals final event occurrence time;
- no partial writes on duplicate event id;
- no cross-workspace run ownership;
- restricted runtime role has only required permissions.

## Task 6 — Verification and PR

Run the complete matrix:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features`
- console typecheck/build
- Docker Compose acceptance and existing smoke tests

Open a stacked draft PR against `agent/r1-event-store-foundation`, documenting RED/GREEN evidence and the R1.4 boundary.

## R1.4 boundary

R1.4 should migrate the HTTP create path to command execution, add persisted command idempotency, and begin treating direct `RunRepository::create` as a compatibility path rather than the canonical write path.
