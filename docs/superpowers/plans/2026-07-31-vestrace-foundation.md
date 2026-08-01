# Vestrace Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a compilable, testable Rust foundation with stable crate boundaries, configuration, PostgreSQL migrations, workspace-aware transactions, baseline RLS, health checks, observability, CI and a development Compose environment.

**Architecture:** Use a Cargo workspace with pure domain/application crates and adapter crates for PostgreSQL, HTTP and the executable. The repository root is also a non-publishable `vestrace-integration-tests` package so root-level `tests/` are valid Cargo integration tests. The binary exposes subcommands but contains no business rules. PostgreSQL is the only persistence dependency; infrastructure implements application ports without leaking SQLx types into domain code.

**Tech Stack:** Rust Edition 2024, Tokio, Axum, Tower, Serde, Schemars, SQLx, PostgreSQL, pgvector, pg_trgm, tracing, Clap, Docker Compose, GitHub Actions.

## Global Constraints

- PostgreSQL is the only source of truth.
- Domain code must not depend on Axum, SQLx, MCP SDKs or concrete AI providers.
- Rust Edition is 2024.
- All versioned identifiers use UUID values wrapped in domain newtypes.
- Applied SQL migrations are forward-only and never edited.
- Secrets are referenced, not stored in configuration files or database rows.
- Default authorization behavior is deny.
- Every database transaction that accesses tenant data must set workspace and principal context.
- CI must not require an external AI provider.
- Root-level integration tests belong to the non-publishable root package and may depend on workspace crates only through public interfaces.

---

## Locked file structure

```text
Cargo.toml
Cargo.lock
src/lib.rs
rust-toolchain.toml
rustfmt.toml
clippy.toml
.env.example
Dockerfile
docker-compose.yml
.github/workflows/ci.yml

crates/vestrace-domain/
  Cargo.toml
  src/lib.rs
  src/id.rs
  src/time.rs
  src/error.rs

crates/vestrace-application/
  Cargo.toml
  src/lib.rs
  src/error.rs
  src/context.rs
  src/ports.rs
  src/health.rs

crates/vestrace-infrastructure/
  Cargo.toml
  src/lib.rs
  src/config.rs
  src/error.rs
  src/postgres/mod.rs
  src/postgres/pool.rs
  src/postgres/transaction.rs
  src/postgres/health.rs

crates/vestrace-http/
  Cargo.toml
  src/lib.rs
  src/router.rs
  src/health.rs

crates/vestrace-cli/
  Cargo.toml
  src/main.rs
  src/commands/mod.rs
  src/commands/server.rs
  src/commands/worker.rs
  src/commands/mcp.rs
  src/commands/migrate.rs
  src/commands/doctor.rs
  src/commands/rebuild.rs

migrations/
  0001_extensions.sql
  0002_identity_and_workspaces.sql
  0003_rls_baseline.sql

tests/
  support/mod.rs
  migrations.rs
  rls.rs
```

---

### Task 1: Bootstrap the Cargo workspace, root test package and quality gates

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `rust-toolchain.toml`
- Create: `rustfmt.toml`
- Create: `clippy.toml`
- Create: `.gitignore`
- Create: `.github/workflows/ci.yml`
- Create: the five crate `Cargo.toml` files and minimal `src/lib.rs` or `src/main.rs`

**Interfaces:**
- Produces workspace crates named `vestrace-domain`, `vestrace-application`, `vestrace-infrastructure`, `vestrace-http`, and `vestrace-cli`.
- Produces root package `vestrace-integration-tests` for repository-level integration tests.
- Produces a binary named `vestrace` from `crates/vestrace-cli`.

- [ ] **Step 1: Write the workspace manifest**

```toml
[package]
name = "vestrace-integration-tests"
version = "0.0.0"
publish = false
edition.workspace = true
rust-version.workspace = true

[workspace]
resolver = "3"
members = [
  "crates/vestrace-domain",
  "crates/vestrace-application",
  "crates/vestrace-infrastructure",
  "crates/vestrace-http",
  "crates/vestrace-cli",
]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.85"

[workspace.dependencies]
anyhow = "1"
async-trait = "0.1"
axum = "0.8"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive", "env"] }
config = "0.15"
http = "1"
schemars = { version = "1", features = ["chrono04", "uuid1"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json", "migrate"] }
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "request-id"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid = { version = "1", features = ["v7", "serde"] }

[dev-dependencies]
serde_json.workspace = true
sqlx.workspace = true
tokio.workspace = true
tower.workspace = true
vestrace-application = { path = "crates/vestrace-application" }
vestrace-domain = { path = "crates/vestrace-domain" }
vestrace-http = { path = "crates/vestrace-http" }
vestrace-infrastructure = { path = "crates/vestrace-infrastructure" }
```

- [ ] **Step 2: Add the root integration-test harness and crate smoke test**

Create `src/lib.rs`:

```rust
#![forbid(unsafe_code)]

//! Non-publishable integration-test harness for the Vestrace workspace.
```

Create `crates/vestrace-domain/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub fn crate_name() -> &'static str {
    "vestrace-domain"
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_is_linkable() {
        assert_eq!(super::crate_name(), "vestrace-domain");
    }
}
```

- [ ] **Step 3: Run the quality gates**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: all commands exit `0`.

- [ ] **Step 4: Add CI with PostgreSQL service**

The workflow runs the three commands above and starts PostgreSQL 17 with database `vestrace_test`, user `vestrace`, and password `vestrace`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src rust-toolchain.toml rustfmt.toml clippy.toml .gitignore .github crates
git commit -m "build: bootstrap Rust workspace"
```

---

### Task 2: Add shared domain identifiers, time and errors

**Files:**
- Create: `crates/vestrace-domain/src/id.rs`
- Create: `crates/vestrace-domain/src/time.rs`
- Create: `crates/vestrace-domain/src/error.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test: inline unit tests in each module

**Interfaces:**
- Produces `WorkspaceId`, `PrincipalId`, `OperationId`, `RequestId`, and `CorrelationId` newtypes.
- Produces `Timestamp` as UTC `chrono::DateTime<Utc>`.
- Produces `DomainError` with stable machine-readable codes.

- [ ] **Step 1: Write the failing identifier test**

```rust
#[test]
fn workspace_id_round_trips_as_string() {
    let id = WorkspaceId::new();
    assert_eq!(id.to_string().parse::<WorkspaceId>().unwrap(), id);
}
```

Run:

```bash
cargo test -p vestrace-domain workspace_id_round_trips_as_string
```

Expected: FAIL because `WorkspaceId` is undefined.

- [ ] **Step 2: Implement the ID macro and concrete IDs**

```rust
macro_rules! domain_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);

        impl $name {
            pub fn new() -> Self { Self(uuid::Uuid::now_v7()) }
            pub const fn from_uuid(value: uuid::Uuid) -> Self { Self(value) }
            pub const fn as_uuid(self) -> uuid::Uuid { self.0 }
        }
    };
}
```

Implement `Display`, `FromStr`, and `Default` for each generated type.

- [ ] **Step 3: Add time helpers**

```rust
pub type Timestamp = chrono::DateTime<chrono::Utc>;

pub fn now() -> Timestamp {
    chrono::Utc::now()
}
```

- [ ] **Step 4: Add stable domain errors**

```rust
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("revision conflict: expected {expected}, current {current}")]
    RevisionConflict { expected: u32, current: u32 },
    #[error("policy violation: {0}")]
    PolicyViolation(String),
}

impl DomainError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "invalid_argument",
            Self::NotFound(_) => "not_found",
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::PolicyViolation(_) => "policy_violation",
        }
    }
}
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p vestrace-domain
git add crates/vestrace-domain
git commit -m "feat(domain): add shared identifiers and errors"
```

---

### Task 3: Define request context and application ports

**Files:**
- Create: `crates/vestrace-application/src/error.rs`
- Create: `crates/vestrace-application/src/context.rs`
- Create: `crates/vestrace-application/src/ports.rs`
- Create: `crates/vestrace-application/src/health.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Test: inline unit tests

**Interfaces:**
- Consumes IDs from `vestrace-domain`.
- Produces framework-free `ApplicationError` variants for domain, conflict, policy, unavailable, storage and internal failures without importing SQLx.
- Produces `RequestContext { request_id, correlation_id, workspace_id, principal_id }`.
- Produces `TransactionManager::begin(&RequestContext)` and `UnitOfWork::commit/rollback` ports.
- Produces `HealthRepository::check()`.

- [ ] **Step 1: Write a failing request-context test**

```rust
#[test]
fn request_context_is_workspace_bound() {
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    assert_ne!(ctx.workspace_id.as_uuid(), uuid::Uuid::nil());
}
```

- [ ] **Step 2: Implement `RequestContext`**

```rust
#[derive(Clone, Debug)]
pub struct RequestContext {
    pub request_id: RequestId,
    pub correlation_id: CorrelationId,
    pub workspace_id: WorkspaceId,
    pub principal_id: PrincipalId,
}

impl RequestContext {
    pub fn new(workspace_id: WorkspaceId, principal_id: PrincipalId) -> Self {
        Self {
            request_id: RequestId::new(),
            correlation_id: CorrelationId::new(),
            workspace_id,
            principal_id,
        }
    }
}
```

- [ ] **Step 3: Define the application error boundary**

Create `ApplicationError` in `crates/vestrace-application/src/error.rs` according to the normative cross-plan contract in `2026-07-31-vestrace-roadmap.md`. It maps `DomainError`, conflict, policy, unavailable, storage and internal failures without importing SQLx or exposing infrastructure error types.

- [ ] **Step 4: Define transaction and health ports**

```rust
#[async_trait::async_trait]
pub trait UnitOfWork: Send {
    async fn commit(self: Box<Self>) -> Result<(), ApplicationError>;
    async fn rollback(self: Box<Self>) -> Result<(), ApplicationError>;
}

#[async_trait::async_trait]
pub trait TransactionManager: Send + Sync {
    async fn begin(&self, context: &RequestContext) -> Result<Box<dyn UnitOfWork>, ApplicationError>;
}

#[async_trait::async_trait]
pub trait HealthRepository: Send + Sync {
    async fn check(&self) -> Result<(), ApplicationError>;
}
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test -p vestrace-application
git add crates/vestrace-application
git commit -m "feat(application): define request context and ports"
```

---

### Task 4: Implement configuration and CLI command parsing

**Files:**
- Create: `crates/vestrace-infrastructure/src/config.rs`
- Modify: `crates/vestrace-infrastructure/src/lib.rs`
- Create: `crates/vestrace-cli/src/commands/mod.rs`
- Create: `crates/vestrace-cli/src/commands/server.rs`
- Create: `crates/vestrace-cli/src/commands/worker.rs`
- Create: `crates/vestrace-cli/src/commands/mcp.rs`
- Create: `crates/vestrace-cli/src/commands/migrate.rs`
- Create: `crates/vestrace-cli/src/commands/doctor.rs`
- Create: `crates/vestrace-cli/src/commands/rebuild.rs`
- Modify: `crates/vestrace-cli/src/main.rs`
- Create: `.env.example`
- Test: `crates/vestrace-infrastructure/tests/config.rs`
- Test: `crates/vestrace-cli/tests/cli.rs`

**Interfaces:**
- Produces `AppConfig::load()` and `AppConfig::load_from_with_overrides(...)` with precedence typed non-secret CLI overrides > environment > file > safe defaults.
- Produces `ConfigOverrides { http_bind: Option<SocketAddr> }`; database URL and future secret fields are intentionally absent from CLI overrides and remain secret-typed environment/secret-source values.
- Produces subcommands `server`, `worker`, `mcp`, `migrate`, `doctor`, and `rebuild`.

- [ ] **Step 1: Write a failing config precedence test**

Use `temp_env::with_var` so the test does not call unsafe environment mutation APIs:

```rust
#[test]
fn environment_overrides_file_value() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), "[http]\nbind = '127.0.0.1:7000'\n").unwrap();
    temp_env::with_var("VESTRACE_HTTP__BIND", Some("127.0.0.1:8000"), || {
        let config = AppConfig::load_from(Some(file.path())).unwrap();
        assert_eq!(config.http.bind.to_string(), "127.0.0.1:8000");
    });
}
```

Add `temp-env` and `tempfile` as test dependencies.

- [ ] **Step 2: Implement typed configuration**

```rust
#[derive(Clone, serde::Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub observability: ObservabilityConfig,
}

#[derive(Clone, serde::Deserialize)]
pub struct DatabaseConfig {
    pub url: secrecy::SecretString,
    pub max_connections: u32,
}
```

Implement redacted `Debug`; never print the database URL value.

Add a failing test proving an explicit `ConfigOverrides::http_bind` wins over `VESTRACE_HTTP__BIND`, while the existing test proves environment wins over file. `AppConfig::load()` and `load_from(...)` delegate to the same loader with empty CLI overrides.

- [ ] **Step 3: Implement CLI parsing**

```rust
#[derive(clap::Parser)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<std::path::PathBuf>,
    #[arg(long, global = true)]
    http_bind: Option<std::net::SocketAddr>,
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Server,
    Worker,
    Mcp,
    Migrate,
    Doctor,
    Rebuild,
}
```

Every command returns `anyhow::Result<()>`; unimplemented runtime behavior returns a clear command-specific error only after successful parsing.
The CLI maps `--http-bind` into `ConfigOverrides`. It has no `--database-url` flag and never accepts a secret value through process arguments.

- [ ] **Step 4: Verify and commit**

```bash
cargo run -p vestrace-cli -- --help
cargo test -p vestrace-infrastructure -p vestrace-cli
git add .env.example Cargo.toml Cargo.lock crates/vestrace-infrastructure crates/vestrace-cli
git commit -m "feat: add configuration and CLI surface"
```

Expected: all six subcommands are listed and tests pass.

---

### Task 5: Add PostgreSQL extensions and identity migrations

**Files:**
- Create: `migrations/0001_extensions.sql`
- Create: `migrations/0002_identity_and_workspaces.sql`
- Create: `tests/support/mod.rs`
- Create: `tests/migrations.rs`
- Modify: `Cargo.toml` dev-dependencies when test support requires them

**Interfaces:**
- Produces PostgreSQL extensions `vector` and `pg_trgm`.
- Produces tables `workspaces`, `principals`, `roles`, `capabilities`, `principal_roles`, and `role_capabilities`.
- Every tenant table includes `workspace_id UUID NOT NULL`.

- [ ] **Step 1: Write a failing migration integration test**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn migrations_create_required_extensions(pool: sqlx::PgPool) {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT extname FROM pg_extension WHERE extname IN ('vector', 'pg_trgm') ORDER BY extname"
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(names, vec!["pg_trgm", "vector"]);
}
```

- [ ] **Step 2: Create extensions migration**

```sql
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;
```

- [ ] **Step 3: Create identity tables**

Use UUID primary keys, `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`, case-insensitive unique workspace slug, and explicit foreign keys with deliberate delete behavior. Do not seed product roles in this migration.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test migrations
git add migrations tests Cargo.toml Cargo.lock
git commit -m "feat(storage): add PostgreSQL identity schema"
```

---

### Task 6: Implement PostgreSQL pool, migrations and scoped transactions

**Files:**
- Create: `crates/vestrace-infrastructure/src/error.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/pool.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/transaction.rs`
- Modify: `crates/vestrace-infrastructure/src/lib.rs`
- Test: `crates/vestrace-infrastructure/tests/postgres.rs`

**Interfaces:**
- Produces `InfrastructureError` in the infrastructure crate according to the normative cross-plan error contract; it may wrap SQLx and migration failures but never enters domain or public wire types.
- Produces `PgStore::connect(&DatabaseConfig) -> Result<PgStore, InfrastructureError>`.
- Produces `PgStore::migrate() -> Result<(), InfrastructureError>`.
- Produces `PgTransactionManager` implementing `TransactionManager`.
- A scoped transaction executes `set_config('vestrace.workspace_id', ..., true)` and `set_config('vestrace.principal_id', ..., true)` before repository queries.

- [ ] **Step 1: Write a failing scoped-transaction test**

```rust
#[sqlx::test(migrations = "../../migrations")]
async fn scoped_transaction_sets_database_context(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let ctx = RequestContext::new(WorkspaceId::new(), PrincipalId::new());
    let mut tx = store.begin_scoped(&ctx).await.unwrap();
    let workspace: String = sqlx::query_scalar("SELECT current_setting('vestrace.workspace_id')")
        .fetch_one(tx.connection())
        .await
        .unwrap();
    assert_eq!(workspace, ctx.workspace_id.to_string());
}
```

- [ ] **Step 2: Implement pool creation and migrations**

Use `PgPoolOptions`, bounded connection count, acquisition timeout, and `sqlx::migrate!("../../migrations")`.

- [ ] **Step 3: Implement scoped transaction setup**

Execute both settings using parameterized `SELECT set_config($1, $2, true)` statements; never interpolate IDs into SQL strings.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test -p vestrace-infrastructure --test postgres
git add crates/vestrace-infrastructure
git commit -m "feat(storage): add scoped PostgreSQL transactions"
```

---

### Task 7: Add baseline RLS and prove workspace isolation

**Files:**
- Create: `migrations/0003_rls_baseline.sql`
- Create: `tests/rls.rs`
- Modify: `tests/support/mod.rs`

**Interfaces:**
- Produces SQL helpers `vestrace_current_workspace_id()` and `vestrace_current_principal_id()`.
- Enables and forces RLS on tenant identity tables.

- [ ] **Step 1: Write a failing cross-workspace isolation test**

Create two workspaces and scoped transactions. Insert a principal in workspace A. Query from workspace B and assert zero rows:

```rust
assert_eq!(visible_count, 0);
```

- [ ] **Step 2: Add context helper functions**

```sql
CREATE FUNCTION vestrace_current_workspace_id()
RETURNS uuid
LANGUAGE sql
STABLE
AS $$ SELECT nullif(current_setting('vestrace.workspace_id', true), '')::uuid $$;
```

Create the equivalent principal helper.

- [ ] **Step 3: Enable and force RLS**

```sql
ALTER TABLE principals ENABLE ROW LEVEL SECURITY;
ALTER TABLE principals FORCE ROW LEVEL SECURITY;
CREATE POLICY principals_workspace_isolation ON principals
USING (workspace_id = vestrace_current_workspace_id())
WITH CHECK (workspace_id = vestrace_current_workspace_id());
```

Apply the same convention to every tenant identity table.

- [ ] **Step 4: Run and commit**

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --test rls
git add migrations/0003_rls_baseline.sql tests/rls.rs tests/support/mod.rs
git commit -m "feat(security): enforce workspace RLS baseline"
```

---

### Task 8: Add health endpoints and structured observability

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/health.rs`
- Create: `crates/vestrace-http/src/router.rs`
- Create: `crates/vestrace-http/src/health.rs`
- Modify: `crates/vestrace-http/src/lib.rs`
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Test: `crates/vestrace-http/tests/health.rs`

**Interfaces:**
- Produces `build_router(AppState) -> axum::Router`.
- Produces `GET /health/live` returning `200` without external dependency checks.
- Produces `GET /health/ready` returning `200` only when PostgreSQL is reachable and migrations are compatible.
- Adds request and correlation IDs to tracing spans without logging request bodies.

- [ ] **Step 1: Write failing router tests**

```rust
#[tokio::test]
async fn live_endpoint_returns_ok() {
    let response = build_test_router().oneshot(
        http::Request::get("/health/live").body(axum::body::Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
}
```

Add a readiness test using a fake `HealthRepository` that returns an error and expect `503`.

- [ ] **Step 2: Implement health DTOs and handlers**

```rust
#[derive(serde::Serialize)]
struct HealthResponse {
    status: &'static str,
}
```

Use `{"status":"ok"}` and `{"status":"not_ready"}`; do not expose connection strings or SQL errors.

- [ ] **Step 3: Add tracing middleware**

Initialize `tracing_subscriber` once in the CLI, support text or JSON output from configuration, and include request IDs in spans.

- [ ] **Step 4: Run and commit**

```bash
cargo test -p vestrace-http
git add crates/vestrace-http crates/vestrace-infrastructure crates/vestrace-cli
git commit -m "feat(http): add health checks and tracing"
```

---

### Task 9: Add development container environment and foundation acceptance test

**Files:**
- Create: `Dockerfile`
- Create: `docker-compose.yml`
- Create: `.dockerignore`
- Create: `scripts/foundation-smoke.sh`
- Modify: `.github/workflows/ci.yml`
- Create: `README.md`

**Interfaces:**
- Produces Compose services `postgres` and `vestrace-server`.
- Exposes HTTP on `127.0.0.1:8080` by default.
- Provides documented migration and startup commands.

- [ ] **Step 1: Write the smoke script before Docker wiring**

```bash
#!/usr/bin/env bash
set -euo pipefail
curl --fail --silent http://127.0.0.1:8080/health/live | grep '"status":"ok"'
curl --fail --silent http://127.0.0.1:8080/health/ready | grep '"status":"ok"'
```

Run it before services start and verify it fails.

- [ ] **Step 2: Add a multi-stage Dockerfile**

Build the release binary in a Rust image and copy it into a minimal non-root runtime image. The final image must not contain Cargo registry caches or source code.

- [ ] **Step 3: Add Compose**

Use a PostgreSQL image that includes pgvector. Add health checks and make the server depend on healthy PostgreSQL. Do not embed production secrets.

- [ ] **Step 4: Run acceptance commands**

```bash
docker compose up --build -d
./scripts/foundation-smoke.sh
docker compose down -v
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: every command exits `0`.

- [ ] **Step 5: Commit**

```bash
git add Dockerfile docker-compose.yml .dockerignore scripts README.md .github/workflows/ci.yml
git commit -m "chore: add foundation development environment"
```

---

## Foundation completion gate

Before opening the plan pull request, verify with fresh output:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --workspace --all-features
docker compose up --build -d
./scripts/foundation-smoke.sh
docker compose down -v
```

Review the diff against the global constraints. Confirm that the root package exists only as an integration-test harness, no domain crate imports Axum or SQLx, no secret value is logged, all tenant queries execute inside a scoped transaction, and the cross-workspace RLS test passes.
