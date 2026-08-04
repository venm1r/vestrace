# Vestrace R1 — Event-Sourced State Engine

Status: Approved

## Goal

Replace direct mutation of `agent_runs` with an append-only run event stream, deterministic reduction, optimistic concurrency, and rebuildable projections. R1.1 implements the domain kernel only; PostgreSQL persistence and HTTP migration follow in later R1 slices.

## Current compatibility constraints

- Existing `AgentRun`, `RunStatus`, and `RunVersion` remain available to current application and infrastructure code during R1.1.
- Current create/list/get HTTP behavior must not change during the domain-kernel slice.
- Domain code must not depend on SQLx, Axum, filesystem APIs, model providers, or system clocks.
- Rust 1.85 and edition 2024 remain the version floor.
- `#![forbid(unsafe_code)]` remains in force.

## Source of truth

Once the persistence slice lands, `run_events` is the source of truth. `agent_runs` becomes a projection. In R1.1 the same rule is represented in memory: `RunState` is obtained only by applying an ordered `RunEventEnvelope` stream.

## Version semantics

- A non-existent aggregate has `RunVersion::ZERO`.
- The first event has sequence/version 1.
- The aggregate version equals the sequence of the last applied event.
- Applying a non-contiguous sequence is an error.

## Domain modules

`crates/vestrace-domain/src/run/` contains focused modules:

- `command.rs`: command envelope and supported R1 commands.
- `event.rs`: event envelope, payloads, stable event names, event schema version.
- `state.rs`: aggregate state, waiting state, completion state, step state, invariants.
- `decision.rs`: pure `decide` function from state plus command to pending events.
- `reducer.rs`: pure `apply` and `replay` functions.
- `error.rs`: machine-distinguishable decision, reduction, and replay errors.
- `mod.rs`: compatibility types and public re-exports.

## Aggregate state

`RunState` includes immutable identity fields, title, status, version, optional active step, optional wait descriptor, optional terminal completion descriptor, and timestamps supplied by events.

Supported statuses in R1.1:

- `created`
- `ready`
- `running`
- `waiting_for_input`
- `waiting_for_approval`
- `completed`
- `failed`
- `cancelled`
- `stalled`

Reserved statuses for later slices may exist in the enum but cannot be produced by R1.1 commands.

## Commands

The R1.1 command set is:

- `Create`
- `MarkReady`
- `Start`
- `StartStep`
- `CompleteStep`
- `FailStep`
- `WaitForInput`
- `WaitForApproval`
- `Resume`
- `Complete`
- `Fail`
- `Cancel`
- `MarkStalled`

Every command is wrapped in `RunCommandEnvelope` containing command ID, workspace, run ID, actor, expected version, correlation ID, issue time, and optional idempotency key. The domain kernel validates expected version against the supplied state before deciding events.

## Events

The R1.1 event set is:

- `run.created`
- `run.marked_ready`
- `run.started`
- `step.started`
- `step.completed`
- `step.failed`
- `run.waiting_for_input`
- `run.waiting_for_approval`
- `run.resumed`
- `run.completed`
- `run.failed`
- `run.cancelled`
- `run.stalled`

Each `RunEventEnvelope` includes event ID, workspace, run ID, sequence, stable event type, per-event schema version, actor, causation ID, correlation ID, payload, occurrence time, and record time.

## Decision rules

`decide(state, command)` is pure and returns pending events.

- `Create` is valid only when state is absent and expected version is zero.
- `MarkReady` is valid only from `created`.
- `Start` is valid only from `ready`.
- Only `running` may start a step.
- Only the active step may complete or fail.
- Waiting commands are valid only from `running` without an active step.
- `Resume` is valid only from a waiting status.
- `Complete` requires `running` without an active step.
- Terminal states reject every later command.
- `Cancel` is accepted from any non-terminal state.
- Titles and user-visible reason strings are trimmed and validated as non-empty where required.

## Reduction rules

`apply(state, event)` is pure.

- The first event must be `run.created` with sequence 1.
- Subsequent event sequence must equal current version plus one.
- Workspace and run IDs must match the aggregate.
- Envelope event type must match the payload's stable event name.
- Event schema version must be supported.
- Events after a terminal state are rejected.
- Waiting statuses always have a matching wait descriptor.
- Terminal statuses always have a completion descriptor and finish timestamp.

`replay(events)` applies events in input order and returns `None` for an empty stream.

## Compatibility model

The existing `AgentRun` remains a projection DTO during R1.1. Shared `RunStatus` and `RunVersion` types are extended rather than replaced. No application or infrastructure repository is migrated in this slice.

## Error model

The kernel exposes typed errors rather than string inspection:

- missing aggregate
- aggregate already exists
- expected-version conflict
- invalid transition
- invalid command data
- sequence gap or duplicate
- aggregate identity mismatch
- event type mismatch
- unsupported event version
- event after terminal state

## Testing

R1.1 requires:

- test-first public API tests under `crates/vestrace-domain/tests/`;
- table-driven transition coverage;
- create/apply/replay happy paths;
- blank title rejection;
- duplicate create rejection;
- expected-version conflict;
- wrong run/workspace rejection;
- sequence-gap rejection;
- active-step invariants;
- waiting/resume invariants;
- terminal-state rejection;
- serialization round trips for commands, events, and state;
- deterministic replay equality.

## Definition of done

R1.1 is complete when the domain crate exports the new kernel, existing workspace consumers still compile, the new tests pass, all existing Rust tests pass, formatting is clean, and Clippy passes with warnings denied.
