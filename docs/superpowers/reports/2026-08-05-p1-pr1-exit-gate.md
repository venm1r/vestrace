# P1 / PR1 State Engine Vertical — Exit Gate Report

**Status:** Implemented and verified on the code head  
**Date:** 2026-08-05  
**Draft PR:** #28  
**Base:** `agent/p0-restore-main`  
**Branch:** `agent/p1-state-engine-vertical`

## 1. Scope delivered

P1 / PR1 delivers the first complete recoverable State Engine vertical:

```text
CreateRun
→ typed RunCommand
→ pure decide
→ versioned RunEvent envelopes
→ append-only PostgreSQL stream
→ pure apply/replay
→ typed checkpoint
→ bounded recovery snapshot
→ guarded projection rebuild
```

The implemented authority direction is:

```text
run_streams
  ├── run_events       canonical history
  ├── run_checkpoints  derived recovery acceleration
  └── agent_runs       derived query projection
```

## 2. Exit-gate evidence

The full code matrix passed in GitHub Actions run `30961093444` on commit `ad8fb04760928844e925e28294cf90c5702b997f`.

Verified jobs:

- `lint` — success;
- `tests` — success;
- `console` — success;
- `compose-acceptance` — success.

Verified commands and behaviors include:

- `cargo fmt --all --check`;
- documentation truthfulness gate;
- HTTP/application boundary gate;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- complete Rust workspace and PostgreSQL integration suite;
- CLI binary build;
- standalone migration application and compatibility verification;
- CLI truthfulness checks;
- console typecheck and production build;
- clean Docker Compose build and startup;
- foundation smoke checks;
- run API isolation checks;
- restricted-runtime RLS checks;
- Compose shutdown and volume removal.

The final documentation-inclusive branch run is recorded in the draft PR after this report is committed.

## 3. TDD evidence

### Stream/recovery schema

RED established the absence of independent `run_streams`, the old checkpoint format, and projection-owned event foreign keys.

GREEN added migration `0112_run_streams_and_recovery_checkpoints.sql`, independent stream identity, converted checkpoint storage, RLS, privileges, and preservation of authoritative data when a projection is deleted.

### Application recovery service

RED established missing typed checkpoint, recovery port, recovery service, shared projection mapper, and corruption handling.

GREEN added deterministic state hashing, checkpoint validation, full replay, checkpoint-plus-tail restoration, and guarded rebuild.

### PostgreSQL recovery adapter

RED established missing checkpoint round-trip, event-range loading, immutable duplicate handling, guarded projection replacement, stale-head rejection, and tenant isolation.

GREEN added `PgRunRecoveryStore` with scoped transactions and typed serialization.

### Projection-independent concurrency

RED demonstrated that canonical commits failed after deleting `agent_runs` because concurrency still depended on the projection.

GREEN moved command and event append ownership to `run_streams`, allowed projection recreation, and retained rollback and RLS behavior.

### Checkpoint timestamp precision

The end-to-end acceptance test exposed nanosecond/microsecond divergence between an in-memory checkpoint and its PostgreSQL round trip.

GREEN normalized `created_at` to PostgreSQL microsecond precision before persistence and return.

### Snapshot races

RED run `30960767375` proved that a checkpoint created after an already observed stream head was incorrectly classified as corruption:

```text
Storage("run checkpoint is ahead of the stream head")
```

GREEN established the snapshot contract:

- capture one stream head;
- ignore events committed above that head;
- ignore checkpoints created above that head;
- replay only through the captured head;
- recheck the head before replacing a projection.

## 4. Requirement checklist

| Requirement | Evidence | Status |
|---|---|---|
| Typed commands and events | Domain kernel and command-service tests | Pass |
| Pure deterministic reducer | `decide`, `apply`, `replay` tests | Pass |
| Independent stream identity | `run_streams` migration and integration tests | Pass |
| Append-only canonical events | event-store, permission, and rollback tests | Pass |
| Stream-owned optimistic concurrency | concurrent writer tests | Pass |
| Projection deletion preserves authority | migration and projection-independence tests | Pass |
| Typed checkpoint creation | recovery-service and store tests | Pass |
| SHA-256 and format validation | checkpoint corruption tests | Pass |
| Authoritative prefix validation | rehashed-tampering test | Pass |
| Full replay | no-checkpoint recovery test | Pass |
| Checkpoint plus bounded tail | recovery and snapshot-boundary tests | Pass |
| Concurrent newer checkpoint handling | snapshot-boundary RED→GREEN | Pass |
| Exact projection rebuild | end-to-end recovery acceptance | Pass |
| Stale rebuild prevention | guarded stream-head test | Pass |
| Cross-workspace isolation | RLS and foreign-workspace tests | Pass |
| P0 regression protection | full P0 CI matrix | Pass |

## 5. Architectural decisions confirmed

### Projection-only compatibility runs

Legacy `RunRepository::create` creates a version-zero stream atomically but creates no events. A legacy projection version is not treated as canonical event history.

### Transitional trigger

The migration-level trigger that seeds a missing version-zero stream remains a defensive compatibility boundary until all projection-only create surfaces are removed. It cannot create domain events or advance stream history.

### Snapshot consistency

Recovery is not a continuously moving read. It reconstructs the state at one captured stream head. Concurrent progress belongs to a later recovery call.

### Checkpoints are never authority

A selected checkpoint must match replay of its authoritative event prefix. Invalid checkpoints cause an explicit failure; the implementation does not silently fall back to `agent_runs`.

## 6. Deferred work

The following items are intentionally not part of P1 / PR1:

- HTTP lifecycle command integration;
- persisted command idempotency R1.4B;
- pause/resume retry counters and timeout policy;
- delegated parent/child runs;
- compensation journal and actions;
- capability and risk-policy enforcement;
- Artifact CAS;
- production worker/scheduler runtime;
- memory expansion and new UI surfaces.

## 7. Exit decision

All ten P1 / PR1 design exit conditions are satisfied by the implementation and the full code-head CI evidence.

The PR remains a draft and is not merged automatically. Integration requires explicit review of the stacked base and an explicit merge decision.