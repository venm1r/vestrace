# Vestrace v0.2 Implementation Gap Delta — C1-C4 Foundation

**Date:** 2026-08-11  
**Checkout:** `E:\Soft\vestrace`  
**Repository state:** dirty, implementation changes uncommitted  
**HEAD:** `568f3d5` (`docs: update architecture, domain-model, getting-started, README, and acceptance for run worker lifecycle`)  
**Normative sources:** `docs/specs/vestrace-*.md`, `docs/implementation-plan-v0.2.md`, and `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`

This is a source-based delta over the frozen v0.2 documentation baseline. It records only the bounded C1-C4 foundation work visible in the current checkout. It is not a qualification result and does not claim C5-C8, G/H/E/T phases, or a v0.2 profile.

## Implemented or materially advanced in this checkout

| Area | Evidence | Boundary |
|---|---|---|
| Generic Event time semantics | `Event` exposes optional `occurred_at` and authoritative `recorded_at`; `0120_generic_event_temporal_fields.sql` backfills `recorded_at` from legacy `created_at`; PostgreSQL event repository and HTTP response map both fields. | The legacy `created_at` field remains as a compatibility timestamp until a deliberate API/schema removal. |
| Memory aggregate concurrency | `Memory.state_revision` is incremented by lifecycle transitions and is now persisted/read through `PgMemoryRepository`; `0116_memory_revision_temporal_fields.sql` supplies the column. | No claim is made that every memory mutation path has a complete transactional compare-and-swap boundary. |
| Claim / Conflict domain core | Typed `Claim`, evidence links, assessments, `Conflict`, supersession links, explicit lifecycle states, and workspace-owned migrations `0118_claim_conflict_core.sql`. | No application repository/API for claims or conflicts is wired yet. |
| Mutation / reconciliation records | Typed `CognitiveMutation` and `ReconciliationRecord`, migration `0119_mutation_reconciliation.sql`, stale expected-state validation, explicit conflict resolution reference, and evidence-preservation tests. | This is domain-core behavior, not the complete application-level C4 service or policy/audit execution path. |

## Verification evidence

- `cargo check --workspace` — PASS, with pre-existing warnings in run checkpoint and repository code.
- `cargo test -p vestrace-domain claim` — PASS, 5 tests.
- `cargo test -p vestrace-domain` — PASS, 140 tests.
- `cargo test --test v01_acceptance v01_degraded_embeddings_and_reranker_disabled` — PASS.
- `npm run typecheck --prefix apps/console` — PASS.
- `cargo test --test migrations migrations_expose_temporal_and_state_revision_columns` — BLOCKED before test execution because `DATABASE_URL` is not set.
- Repository-wide `cargo fmt --all -- --check` remains a dirty-checkout gate and reports unrelated pre-existing formatting differences; scoped formatting was run on the files changed in this slice.

## Still pending

1. Run configured PostgreSQL integration evidence for the C4 application service; see [`documentation-gap-delta-2026-08-11-c4-service.md`](documentation-gap-delta-2026-08-11-c4-service.md).
2. Complete C1 temporal historical queries and C2 provenance closure at runtime.
3. Implement C5-C8 exact-revision retrieval, ContextPack 2.0, temporal/multi-channel retrieval, and the CORE+MEMORY gate.
4. Do not treat the current conformance registry/CLI or static evidence strings as a `QualificationBundle` until executable evidence and environment/build identity are recorded.

The next accepted implementation gate is the application-level C4 service after the database-backed migration tests are run in a configured PostgreSQL environment.
