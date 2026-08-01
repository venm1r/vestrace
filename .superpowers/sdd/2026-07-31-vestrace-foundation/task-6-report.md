# Foundation Task 6 Report

## Status

PASS — PostgreSQL pool creation, embedded migrations, opaque infrastructure errors, real scoped transactions, and the application transaction port are implemented on `feat/v01-foundation` from base `95dab541e366f852f2ae6abeba0d09000e286517`.

## Public and internal interfaces

- `InfrastructureError` is an opaque infrastructure-crate error. `InfrastructureErrorKind::{Configuration, Database, Migration}` is SQLx-free and stable enough for adapter-side classification.
- A private `InfrastructureErrorSource` retains configuration detail, `sqlx::Error`, and `sqlx::migrate::MigrateError`; SQLx types do not enter domain, application, wire, or public error fields.
- `PgStore::connect(&DatabaseConfig)` explicitly exposes `SecretString` only at `PgPoolOptions::connect`, uses the configured `max_connections`, rejects zero, and applies a one-second acquisition timeout.
- `PgStore::from_pool(PgPool)` is the infrastructure/testing constructor required by the task brief.
- `PgStore::migrate()` runs `sqlx::migrate!("../../migrations")` against the store pool.
- `PgStore::begin_scoped(&RequestContext)` starts a real PostgreSQL transaction, then executes two separate `SELECT set_config($1, $2, true)` statements for `vestrace.workspace_id` and `vestrace.principal_id` before returning.
- `PgScopedTransaction::connection()` exposes `&mut PgConnection` only from the infrastructure crate surface for infrastructure repository queries. Concrete `commit` and `rollback` return `InfrastructureError` without exposing SQLx in their signatures.
- `PgTransactionManager::new(PgStore)` implements the application `TransactionManager` port. `PgScopedTransaction` implements `UnitOfWork`; the object-safe path returns `Box<dyn UnitOfWork>`.

## Error mapping

- Pool acquisition/begin/query/commit/rollback failures convert from `sqlx::Error` to opaque `InfrastructureErrorKind::Database`.
- Migration failures convert from `MigrateError` to opaque `InfrastructureErrorKind::Migration`.
- Invalid zero-sized pool configuration becomes `InfrastructureErrorKind::Configuration` before the secret is exposed or SQLx is called.
- The application port maps infrastructure failures to `ApplicationError::Storage` using the sanitized infrastructure display text (`database operation failed`, `database migration failed`, or validated configuration text). Neither `InfrastructureError` nor SQLx enters application/domain signatures.

## Docker resources and cleanup

All resources used `vestrace.owner=vestrace-foundation-task6-019fbc74`:

- Network: `vestrace-foundation-task6-019fbc74`
- PostgreSQL: `vestrace-foundation-task6-019fbc74-postgres`
- PostgreSQL image: `pgvector/pgvector:pg17-bookworm`
- Rust runner: `vestrace-foundation-task6-019fbc74-rust-runner`
- Rust image: `rust:1.85-bookworm`
- Database URL inside the network: `postgres://vestrace:vestrace@vestrace-foundation-task6-019fbc74-postgres:5432/vestrace_test`

Readiness and final pre-cleanup ownership audit:

```text
POSTGRES_LABELS={"vestrace.owner":"vestrace-foundation-task6-019fbc74"}
POSTGRES_HEALTH=healthy
RUNNER_LABELS={"org.opencontainers.image.source":"https://github.com/rust-lang/docker-rust","vestrace.owner":"vestrace-foundation-task6-019fbc74"}
RUNNER_STATE=running
NETWORK_LABELS={"vestrace.owner":"vestrace-foundation-task6-019fbc74"}
```

The first cleanup command stopped before deletion because its Go-template label lookup was malformed. A JSON-based exact-label recheck then succeeded; only the exact task resources were removed:

```text
VERIFY_CONTAINER vestrace-foundation-task6-019fbc74-rust-runner owner=vestrace-foundation-task6-019fbc74
VERIFY_CONTAINER vestrace-foundation-task6-019fbc74-postgres owner=vestrace-foundation-task6-019fbc74
VERIFY_NETWORK vestrace-foundation-task6-019fbc74 owner=vestrace-foundation-task6-019fbc74
vestrace-foundation-task6-019fbc74-rust-runner
vestrace-foundation-task6-019fbc74-postgres
vestrace-foundation-task6-019fbc74
REMAINING_CONTAINERS=
REMAINING_NETWORKS=
```

## TDD evidence

### Prescribed scoped-transaction RED

Command:

```text
docker exec vestrace-foundation-task6-019fbc74-rust-runner cargo test -p vestrace-infrastructure --test postgres
```

Exit 101, expected missing adapter surface:

```text
error[E0432]: unresolved import `vestrace_infrastructure::PgStore`
 --> crates/vestrace-infrastructure/tests/postgres.rs:3:5
  |
3 | use vestrace_infrastructure::PgStore;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no `PgStore` in the root
```

The minimal workspace-setting implementation then passed `1 passed; 0 failed`.

### Principal setting RED/GREEN

Before the second parameterized `set_config` call, the principal test failed as intended:

```text
assertion `left == right` failed
  left: None
 right: Some("<principal UUID>")
test result: FAILED. 0 passed; 1 failed
```

After adding the separate principal statement, the combined scope target passed:

```text
running 2 tests
test scoped_transaction_sets_database_context ... ok
test scoped_transaction_sets_principal_database_context ... ok
test result: ok. 2 passed; 0 failed
```

### Commit and rollback non-leak RED/GREEN

- Commit RED: `E0599`, no method named `commit` on `PgScopedTransaction`.
- Rollback RED: `E0599`, no method named `rollback` on `PgScopedTransaction`.
- The first commit reuse run correctly revealed a fixture defect: the default SQLx pool returned backend PID 3045 rather than scoped PID 3044. The test was tightened to a separately constructed max-one pool; production code was not changed to satisfy that fixture failure.
- Commit GREEN: `1 passed; 0 failed`; identical backend PID after commit and both `NULLIF(current_setting(..., true), '')` values were `NULL`.
- Rollback GREEN: `1 passed; 0 failed`; identical backend PID after rollback and both transaction-local values were `NULL`.

### Application port RED/GREEN

RED:

```text
error[E0432]: unresolved import `vestrace_infrastructure::PgTransactionManager`
```

GREEN: `Box<dyn TransactionManager>::begin` returned `Box<dyn UnitOfWork>` and committed successfully against PostgreSQL (`1 passed; 0 failed`).

### Migrate RED/GREEN

RED:

```text
error[E0599]: no method named `migrate` found for struct `PgStore`
```

GREEN: `PgStore::migrate()` created both `vector`/`pg_trgm` extensions and all six identity tables in an initially unmigrated SQLx test database (`1 passed; 0 failed`).

### Pool connection bounds RED/GREEN

- Connect RED: `E0599`, no associated item `PgStore::connect`.
- After implementation, the test fixture was corrected to avoid `unwrap_err` requiring an irrelevant `Debug` implementation on successful transactions.
- GREEN: with `max_connections=1`, a held transaction exhausted the pool; the next begin returned `InfrastructureErrorKind::Database` via the configured acquisition timeout. The test retained both the failure-kind and elapsed `< 10s` assertions; total test time was 1.15s.
- Zero-bound RED: `E0599`, no `InfrastructureError::Configuration` classification.
- Zero-bound GREEN: zero maximum connections were rejected before pool creation (`1 passed; 0 failed`).

### SQLx-free public error boundary RED/GREEN

Pre-gate review identified that a public enum payload still exposed `sqlx::Error`. The replacement contract was test-driven:

```text
error[E0432]: unresolved import `vestrace_infrastructure::InfrastructureErrorKind`
error[E0599]: no method named `kind` found for enum `InfrastructureError`
```

After making the error opaque with a private source enum, the complete focused target passed:

```text
running 8 tests
test connect_rejects_zero_max_connections ... ok
test scoped_transaction_sets_database_context ... ok
test application_transaction_manager_commits_through_object_safe_ports ... ok
test scoped_transaction_sets_principal_database_context ... ok
test store_migrate_applies_embedded_migrations ... ok
test committed_transaction_scope_does_not_leak_on_connection_reuse ... ok
test rolled_back_transaction_scope_does_not_leak_on_connection_reuse ... ok
test connect_honors_max_connections_and_acquisition_timeout ... ok
test result: ok. 8 passed; 0 failed
```

## Final verification

All final commands ran in `vestrace-foundation-task6-019fbc74-rust-runner` with the real PostgreSQL service healthy:

```text
cargo fmt --all --check
```

Exit 0; no formatting differences.

```text
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Exit 0; workspace Clippy completed without warnings or errors.

```text
cargo test --workspace --all-targets
```

Exit 0. Substantive results:

- application: 2 passed
- CLI: 3 passed
- domain: 4 passed
- infrastructure configuration: 4 passed
- Task 6 PostgreSQL: 8 passed
- immutable migration suite: 21 passed
- total substantive tests: 42 passed, 0 failed

`git diff --check` exited 0. Working-tree and HEAD blob hashes matched for both immutable migrations:

```text
0001 6aa6c6f71f59950abeac46f55253c40d84437900
0002 f152e14451a079c945bfcce8eddedcfcdb95e86b
```

## Files

Created:

- `crates/vestrace-infrastructure/src/error.rs`
- `crates/vestrace-infrastructure/src/postgres/mod.rs`
- `crates/vestrace-infrastructure/src/postgres/pool.rs`
- `crates/vestrace-infrastructure/src/postgres/transaction.rs`
- `crates/vestrace-infrastructure/tests/postgres.rs`
- `.superpowers/sdd/2026-07-31-vestrace-foundation/task-6-report.md`

Modified:

- `crates/vestrace-infrastructure/Cargo.toml`
- `crates/vestrace-infrastructure/src/lib.rs`
- `Cargo.lock`
- `docs/superpowers/plans/2026-07-31-vestrace-foundation.md` — preserved and included the controller's tracked clarification adding `src/error.rs` and its normative interface note.

Unchanged:

- `migrations/0001_extensions.sql`
- `migrations/0002_identity_and_workspaces.sql`
- all Task 5 schema and test files

## Self-review

- Confirmed the database secret is exposed exactly once and only at the SQLx connection boundary.
- Confirmed max connections are explicit, zero is rejected, and acquisition timeout is finite and behavior-tested.
- Confirmed both context settings use two separate parameterized statements with transaction-local `true`; neither ID is interpolated into SQL.
- Confirmed both commit and rollback clear workspace and principal values on the same reused backend connection.
- Confirmed SQLx and migration source types are retained only by a private infrastructure error source and never enter application/domain/public fields.
- Confirmed the application port is object-safe and its commit/rollback results contain only `ApplicationError`.
- Confirmed migrations are embedded from the existing directory; no `0003`, RLS, schema edit, or Task 5 test edit was introduced.
- Confirmed the controller's tracked plan clarification remains included and unrelated files are untouched.
- Confirmed the final diff passes `git diff --check`.

## Concerns

None.
