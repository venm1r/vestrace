# C4 Mutation/Reconciliation Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the next C4 gate as an application service that authorizes and atomically persists cognitive mutations, conflict reconciliation, idempotency, audit, and outbox effects.

**Architecture:** Keep domain transition rules in `vestrace-domain`. Add one application-facing transactional cognitive-mutation repository port; the application service performs capability authorization and request validation, while the PostgreSQL adapter executes the aggregate update, mutation/reconciliation records, idempotency result, audit event, and outbox message in one scoped transaction. Do not add HTTP or retrieval behavior in this slice.

**Tech Stack:** Rust 2024, async-trait, Serde, SQLx/PostgreSQL, existing RLS context, existing policy/audit/idempotency/outbox foundations.

## Global Constraints

- `MUT-001..008` are the target requirements; C5 retrieval and later G/H/E/T phases remain out of scope.
- A mutation must retain actor, target, expected state, reason, provenance, and resulting state identity.
- Conflict resolution must retain the conflicting inputs and reconciliation basis; no timestamp-wins rule may be introduced.
- Human-required reconciliation is never selected by an automatic policy path.
- Authorization is checked before mutation; workspace identity must match the request context.
- Idempotency replay returns the original result for the same request hash and rejects key reuse with a different hash.
- Mutation, aggregate update, reconciliation record, idempotency record, audit event, and outbox message commit atomically.
- Preserve the existing dirty worktree, deleted documentation, and graphify artifacts; do not commit or reformat unrelated files.

---

### Task 1: Add mutation identity and conflict optimistic concurrency

**Files:**
- Modify: `crates/vestrace-domain/src/id.rs`
- Modify: `crates/vestrace-domain/src/claim/mutation.rs`
- Modify: `crates/vestrace-domain/src/claim/conflict.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Create: `migrations/0121_cognitive_mutation_concurrency.sql`
- Test: domain claim tests

**Interfaces:**
- Consumes: existing `CognitiveMutation`, `Conflict`, `ConflictStatus`, and `ConflictId`.
- Produces: `CognitiveMutationId`, `CognitiveMutation.mutation_id`, and `Conflict.state_revision` so stale writes are checkable for both claim and conflict targets.

- [x] **Step 1: Write failing domain tests**

Add tests showing a conflict starts at revision zero, successful lifecycle transitions increment it, and a mutation has a stable identity.

```rust
#[test]
fn conflict_lifecycle_increments_state_revision() {
    let conflict = open_conflict();
    assert_eq!(conflict.state_revision, 0);

    let proposed = conflict.propose_reconciliation("reconciliation-1".into()).unwrap();
    assert_eq!(proposed.state_revision, 1);
}
```

- [x] **Step 2: Run the tests and confirm RED**

Run `cargo test -p vestrace-domain conflict_lifecycle_increments_state_revision` and confirm compilation fails because the new identity/revision fields do not exist.

- [x] **Step 3: Implement the minimum domain changes**

Add `domain_id!(CognitiveMutationId)`, add `mutation_id` to `CognitiveMutation::new`, add `state_revision` to `Conflict`, and increment it on proposal, resolution, ambiguity acceptance, and obsolescence. Add migration `0121` for the conflict column and RLS policies for the C4 tables.

- [x] **Step 4: Run focused domain verification**

Run `cargo test -p vestrace-domain claim` and confirm all claim/conflict/mutation tests pass.

---

### Task 2: Add the application C4 command, transactional port, and service

**Files:**
- Create: `crates/vestrace-application/src/cognitive_mutation.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: `crates/vestrace-application/src/cognitive_mutation.rs`

**Interfaces:**
- Consumes: `PolicyEngine`, `RequestContext`, domain mutation/reconciliation constructors, and `Capability::MemoryWrite`.
- Produces: `CognitiveMutationCommand`, `CognitiveMutationResult`, `CognitiveMutationRepository`, and `CognitiveMutationService<R, P>`.

- [x] **Step 1: Write failing service tests**

Use a small in-memory repository that records the command and returns a result. Add tests that authorization is required, workspace mismatch is rejected, and the service forwards a deterministic request hash and idempotency key.

```rust
#[tokio::test]
async fn mutation_service_requires_memory_write_capability() {
    let repository = RecordingMutationRepository::default();
    let service = CognitiveMutationService::new(
        repository,
        CapabilitySetPolicyEngine::new([]),
    );

    let result = service.apply(&context(), command()).await;

    assert!(matches!(result, Err(ApplicationError::Policy(_))));
}
```

- [x] **Step 2: Run the tests and confirm RED**

Run `cargo test -p vestrace-application mutation_service_requires_memory_write_capability` and confirm the module/service is missing.

- [x] **Step 3: Implement the application service**

Define a serializable command containing mutation identity, target, expected revision, mutation kind, reason, provenance, optional reconciliation input, and idempotency key. Compute a SHA-256 request hash from the command, reject blank keys/reasons, call policy before the repository, and pass the context, command, hash, and current timestamp to `apply_transactional`.

- [x] **Step 4: Add mutation-sensitive service tests and run them**

Cover same-key replay, different-hash conflict, deterministic basis preservation, policy-guided exact version, semantic ambiguity, and human-required rejection of automatic execution. Run `cargo test -p vestrace-application cognitive_mutation` and confirm PASS.

---

### Task 3: Implement the PostgreSQL transactional repository

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/cognitive_mutation_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Test: `tests/migrations.rs`

**Interfaces:**
- Consumes: `PgStore::begin_scoped`, `CognitiveMutationRepository`, and the existing `claims`, `conflicts`, `cognitive_mutations`, `reconciliation_records`, `idempotency_keys`, `audit_events`, and `outbox` tables.
- Produces: `PgCognitiveMutationRepository` with one transaction covering load, expected-state check, domain transition, persistence, idempotency, audit, and outbox.

- [x] **Step 1: Write the migration/schema test**

Add a test that checks `conflicts.state_revision`, the C4 tables, and RLS policies exist after migrations. The test must report a missing `DATABASE_URL` as an environment block, not as evidence of implementation failure.

- [x] **Step 2: Run the schema test and confirm the expected RED/BLOCKED state**

Run `cargo test --test migrations cognitive_mutation_schema` and record either the missing-column failure or the configured-database result before writing the adapter.

- [x] **Step 3: Implement the repository transaction**

Use the scoped PostgreSQL transaction to select the workspace-owned claim/conflict, validate `expected_state_revision`, apply the domain transition, update with the old revision in the `WHERE` clause, insert the mutation and optional reconciliation record, then insert idempotency, audit, and outbox rows before commit. On duplicate idempotency key, compare request hashes and replay or return `ApplicationError::Conflict`.

- [x] **Step 4: Run compile and adapter verification**

Run `cargo check --workspace`, `cargo test --workspace --no-run`, and the configured migration test if `DATABASE_URL` is available. Keep any database-unavailable result explicitly separated from Rust compile/test evidence.

---

### Task 4: Update the source-based gap delta and inspect the exact slice

**Files:**
- Create: `docs/documentation-gap-delta-2026-08-11-c4-service.md`
- Modify: `docs/documentation-status-v0.2.md`
- Modify: `docs/documentation-gap-delta-2026-08-11-c1-c4.md`

**Interfaces:**
- Consumes: focused service tests, workspace compile output, migration/schema output, and scoped diff evidence.
- Produces: an honest C4 status that distinguishes transactional runtime evidence from static documentation/conformance registry coverage.

- [x] **Step 1: Record implemented C4 evidence**

Document the application service, transactional PostgreSQL adapter, authorization, idempotency, audit, outbox, stale-write, and reconciliation evidence. State which requirements remain partial because no configured PostgreSQL runtime was available.

- [x] **Step 2: Link the delta without rewriting frozen authority**

Link the dated C4 delta from `documentation-status-v0.2.md`; do not change the pinned architecture baseline or claim v0.2 qualification.

- [x] **Step 3: Run the final scoped checks**

Run `git diff --check`, `cargo test -p vestrace-domain claim`, `cargo test -p vestrace-application cognitive_mutation`, `cargo check --workspace`, and `git diff --stat -- <C4 files>`. Leave unrelated dirty files untouched.
