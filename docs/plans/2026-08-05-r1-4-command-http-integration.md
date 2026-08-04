# R1.4 Command HTTP Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route `POST /v1/runs` through the event-sourced command pipeline and add persisted create-run idempotency without changing the existing read API.

**Architecture:** HTTP builds a typed `RunCommandEnvelope` and delegates to a new idempotent application service. PostgreSQL owns the atomic boundary: receipt claim, event append, projection update, and receipt completion occur in one workspace-scoped transaction. Reads continue through `RunUseCases` and `agent_runs`.

**Tech Stack:** Rust 1.85, Axum 0.8, SQLx 0.8, PostgreSQL, Tokio, SHA-256.

## Global Constraints

- Keep PR stacked on `agent/r1-projection-integration`.
- Do not merge or modify `main`.
- Preserve current GET/list response schema and behavior.
- Do not introduce asynchronous command execution, outbox, jobs, or generic workflow commands.
- Use RED → GREEN for every task.

---

### Task 1: HTTP command contract RED

**Files:**
- Modify: `tests/http_contract.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `crates/vestrace-http/src/api/runs.rs`

**Interfaces:**
- Consumes: `RunCommandExecutor::execute(&RequestContext, RunCommandEnvelope)`
- Produces: `AppState::new(health_repository, run_use_cases, run_command_executor)` and command-driven `create_run`

- [ ] Add an HTTP test whose direct repository writer panics and whose command executor records the received create command.
- [ ] Assert `POST /v1/runs` returns `201`, forwards workspace/principal/correlation identity, trims the title through domain execution, and never calls `RunRepository::create`.
- [ ] Run the focused test and confirm compilation fails because `AppState` has no command executor.
- [ ] Add `SharedRunCommandExecutor` to `AppState`, expose a crate-private accessor, and construct `RunCommandEnvelope` in the handler.
- [ ] Return the `RunCommandResult.run` projection as the existing `RunResponse`.
- [ ] Run the focused test and the existing HTTP suite.

### Task 2: Persisted receipt model RED

**Files:**
- Create: `migrations/0005_run_command_receipts.sql`
- Modify: `crates/vestrace-application/src/runs/commands.rs`
- Modify: `crates/vestrace-application/src/runs/ports.rs`
- Create: `crates/vestrace-application/src/runs/idempotent_service.rs`
- Test: `tests/run_command_idempotency.rs`

**Interfaces:**
- Produces: `RunCommandReceipt`, `RunCommandExecution { result, replayed }`, `RunCommandReceiptStore`, and `IdempotentRunCommandService`

- [ ] Add failing application tests for no-key passthrough, completed replay, and fingerprint mismatch.
- [ ] Define a receipt lookup/execute port that keeps storage atomicity outside HTTP.
- [ ] Canonicalize create title exactly as the domain decision does and compute a SHA-256 fingerprint over canonical JSON.
- [ ] Implement the minimal application orchestration and prove no second command execution occurs on replay.

### Task 3: PostgreSQL atomic idempotency RED

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/run_command_receipt_store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_command_committer.rs`
- Test: `tests/run_command_receipts_postgres.rs`

**Interfaces:**
- Produces: `PgIdempotentRunCommandExecutor`

- [ ] Add SQLx tests for first execution, exact replay, mismatched fingerprint, cross-workspace isolation, cross-principal isolation, and two concurrent requests.
- [ ] Create `run_command_receipts` with a composite primary key and RLS policies matching existing workspace scoping.
- [ ] In one `PgScopedTransaction`, claim or lock the receipt key, replay when completed, reject fingerprint mismatch, execute the same validation/replay/projection logic as R1.3, and complete the receipt before commit.
- [ ] Ensure all early returns roll back both receipt and command writes.
- [ ] Run PostgreSQL tests and verify exactly one event exists after concurrent identical requests.

### Task 4: Server wiring and HTTP idempotency semantics

**Files:**
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-http/src/api/runs.rs`
- Modify: `crates/vestrace-http/src/api/error.rs`
- Modify: `scripts/foundation-run-smoke.sh`

**Interfaces:**
- Consumes: `PgIdempotentRunCommandExecutor`
- Produces: `201` for first execution and `200` for completed replay

- [ ] Wire `PgRunEventStore`, `PgRunCommandCommitter`, and the idempotent executor into `AppState`.
- [ ] Parse `Idempotency-Key` as optional visible ASCII, length 1–128 bytes.
- [ ] Map replayed success to `200 OK`, first execution to `201 Created`, mismatch to `409 Conflict`.
- [ ] Extend smoke coverage to repeat a create with the same key and verify the same run id and a single list entry.

### Task 5: Full verification and PR evidence

**Files:**
- Modify: PR description only

- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace --all-targets --all-features` with PostgreSQL.
- [ ] Run console typecheck and production build.
- [ ] Run Docker Compose acceptance and RLS smoke.
- [ ] Review the diff for direct HTTP repository writes, non-atomic receipt writes, secret logging, and tenant leaks.
- [ ] Update the draft PR with RED/GREEN evidence and verified deferrals; do not merge.
