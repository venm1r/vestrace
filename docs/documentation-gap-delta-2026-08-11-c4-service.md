# Vestrace v0.2 Implementation Gap Delta - C4 Service

**Date:** 2026-08-11  
**Checkout:** `E:\Soft\vestrace`  
**Repository state:** dirty, implementation changes uncommitted  
**Normative sources:** `docs/specs/vestrace-normative-invariants-v0.2.md`, `docs/requirement-coverage-v0.2.md`, `docs/implementation-plan-v0.2.md`

This is a bounded C4 implementation delta. It does not replace the frozen v0.2 documentation baseline and does not qualify a CORE, MEMORY, or v0.2 profile.

## Implemented in this checkout

| Area | Evidence | Boundary |
|---|---|---|
| Application mutation boundary | `CognitiveMutationService` validates target/reason/idempotency key, checks `MemoryWrite` capability, binds actor/workspace from `RequestContext`, hashes the command, and constructs typed mutation/reconciliation records. | No HTTP endpoint is added in C4; callers must use the application service rather than mutate repositories directly. |
| Stale-write protection | `Claim.state_revision` and `Conflict.state_revision` are checked in the domain and again in PostgreSQL `UPDATE ... WHERE state_revision = expected`. | Full database-backed concurrent execution remains environment-gated until PostgreSQL tests run. |
| Reconciliation semantics | Deterministic requires basis evidence; policy-guided requires an exact policy version; semantic outcomes preserve ambiguity/defer states; human-required requires a recorded human decision. | No timestamp-wins resolution is introduced. |
| Atomic PostgreSQL persistence | `PgCognitiveMutationRepository` uses `PgStore::begin_scoped` and persists the aggregate transition, cognitive mutation, reconciliation record, audit event, outbox event, and idempotency response before commit. Same-key replay returns the stored result; a different request hash is rejected. | Runtime SQL behavior is compile-verified but not database-executed in this environment. |
| Workspace isolation | Migration `0121_cognitive_mutation_concurrency.sql` adds `Conflict.state_revision` and workspace RLS policies for C4 tables. | RLS acceptance still requires the restricted runtime PostgreSQL login and configured database. |

## Verification evidence

- `cargo test -p vestrace-domain claim` - PASS, 6 focused tests.
- `cargo test -p vestrace-application cognitive_mutation` - PASS, 5 focused tests.
- `cargo check -p vestrace-infrastructure` - PASS, with the pre-existing unused `AgentRunRow.title` warning.
- `cargo test --workspace --no-run` - PASS; all workspace test binaries compiled.
- `cargo test --test migrations cognitive_mutation_schema -- --nocapture` - BLOCKED before test execution because `DATABASE_URL` is not set.
- Repository-wide `cargo fmt --all -- --check` remains a dirty-checkout gate because unrelated pre-existing files are unformatted; the two new C4 implementation files were formatted separately.

## Remaining C4 limitations

1. The PostgreSQL adapter currently supports Claim lifecycle mutations and Conflict reconciliation; Memory/MemoryRevision mutation persistence remains a later integration slice.
2. No configured PostgreSQL runtime evidence exists for commit atomicity, RLS enforcement, JSONB round trips, or idempotency replay.
3. C5 exact-revision retrieval and authorization, C6 ContextPack, C7 temporal retrieval, C8 qualification, and later L/G/H/E/T phases remain pending.

The next verification gate is a configured restricted PostgreSQL runtime run for `cognitive_mutation_schema` plus mutation transaction/replay/RLS integration tests.
