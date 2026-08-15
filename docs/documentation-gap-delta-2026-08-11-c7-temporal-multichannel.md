# Documentation Gap Delta — C7 Temporal and Multi-channel Retrieval

**Date:** 2026-08-11  
**Scope:** bounded C7 implementation slice in the dirty `E:\Soft\vestrace` checkout  
**Authority:** v0.2 Data & Temporal Model, Retrieval/ContextPack contract, and C7 execution plan

This source-based delta records the temporal and optional-channel retrieval
behavior now present in the checkout. It does not replace the frozen v0.2
architecture baseline and does not qualify the v0.2 profile.

## Implemented in this slice

- `Current` retrieval remains active-only and joins the exact active revision.
- `AsOf(t)` is an explicit validity-window query (`VALID_AS_OF` semantics):
  the selected revision satisfies `valid_from <= t < valid_until`, with open
  interval ends supported. `KNOWN_AS_OF` and `RECONSTRUCTED_AS_OF` are not
  conflated with this mode.
- `Timeline` and `AllHistory` query revision history with explicit temporal
  ordering and return lifecycle/status metadata so historical knowledge is not
  presented as current truth.
- Historical FTS ranking uses the selected revision content rather than the
  current search-document projection.
- Candidates and ContextItems retain status, revision number, validity range,
  revision creation time, and source generation. ContextPacks retain the
  requested temporal perspective.
- HTTP retrieval accepts `current`, `as_of` plus an RFC3339 timestamp,
  `timeline`, and `all_history`; responses expose perspective and degraded
  channel metadata.
- `RetrievalService` supports optional vector, exact, and structured ports.
  Channel failures are named in warnings and `degraded_channels`; if every
  configured channel fails, retrieval returns `ApplicationError::Unavailable`
  instead of an empty successful result.
- PostgreSQL memory revision persistence now writes and reads the temporal
  columns introduced by migration `0116`; migration `0122` adds temporal
  lookup/search workspace indexes.

## Evidence

- Application retrieval focused suite: 26 tests passed.
- Infrastructure temporal query-plan suite: 3 tests passed.
- HTTP temporal parser suite: 2 tests passed.
- Domain retrieval suite: 3 tests passed.
- `cargo check --workspace` and `cargo test --workspace --no-run` pass.
- `cargo test --test v01_acceptance` passes 9/9.
- PostgreSQL migration execution was attempted but blocked before test setup by
  missing `DATABASE_URL`; Docker runtime access remains an environment gate.

## Remaining boundary

This slice does not claim:

- `RET-003` unresolved conflict projection;
- `RET-007` classification/model-destination policy;
- `RET-012` actual retrieval-cache generation invalidation (no runtime cache
  exists in the inspected implementation; source generation is only preserved);
- `RET-014` mounted cross-workspace retrieval;
- configured production vector/exact/structured provider implementations — the
  application wiring is optional and text-only construction remains the current
  default;
- C8 CORE+MEMORY qualification or any later L/G/H/E/T gate.
