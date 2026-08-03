# Vestrace P0 Stabilization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore a compilable and truthful Vestrace foundation with correctly composed Axum routes and one real PostgreSQL-backed `create/list/get run` vertical slice.

**Architecture:** Replace the current HTTP prototype with a thin adapter over an application-level run use-case interface. PostgreSQL remains authoritative through a scoped infrastructure repository; HTTP receives workspace and principal identifiers from request headers and never accesses SQLx or concrete database types. Unsupported product surfaces return explicit `501 Not Implemented`, and the console stops converting backend failures into successful mock responses.

**Tech Stack:** Rust 1.85, Rust 2024 edition, Axum 0.8, Tokio, SQLx 0.8, PostgreSQL 17/pgvector image, React 18, TypeScript 5.6, Vite 5, GitHub Actions.

## Global Constraints

- Keep Rust at `1.85` and edition `2024`.
- Keep Axum at `0.8`; dynamic captures must use `/{id}`, never `/:id`.
- The effective API prefix is `/v1` exactly once.
- HTTP must not import or call `sqlx`, `PgStore`, `PgPool`, or `as_any`.
- PostgreSQL is authoritative for run persistence.
- Every PostgreSQL run operation must execute inside `PgStore::begin_scoped(&RequestContext)`.
- Default API behavior must never replace an error with mock success data.
- P0 implements only `POST /v1/runs`, `GET /v1/runs`, and `GET /v1/runs/{id}` as real product behavior.
- Unsupported REST and AG-UI endpoints must return structured `501 Not Implemented` responses.
- State Engine reducers, replay, checkpoints, capabilities, risk policy, and artifact CAS remain outside P0.
- Each task must finish with formatting, focused tests, and one reviewable commit.

---

## File Map

### Create

- `crates/vestrace-application/src/runs/mod.rs` — public run application module.
- `crates/vestrace-application/src/runs/commands.rs` — run commands accepted by the application layer.
- `crates/vestrace-application/src/runs/ports.rs` — `RunRepository` and `RunUseCases` interfaces.
- `crates/vestrace-application/src/runs/service.rs` — validated run orchestration.
- `crates/vestrace-infrastructure/src/postgres/run_repository.rs` — scoped PostgreSQL implementation.
- `crates/vestrace-http/src/api/context.rs` — request-header to `RequestContext` conversion.
- `crates/vestrace-http/src/api/error.rs` — stable JSON API errors.
- `crates/vestrace-http/src/api/runs.rs` — run request/response DTOs and handlers.
- `crates/vestrace-http/tests/router_contract.rs` — route-prefix and unsupported-surface tests.
- `crates/vestrace-http/tests/run_routes.rs` — run HTTP contract tests.
- `crates/vestrace-infrastructure/tests/run_repository.rs` — PostgreSQL persistence and RLS tests.
- `apps/console/src/sdk/useApiResource.ts` — shared loading/error hook for API reads.

### Modify

- `crates/vestrace-application/src/lib.rs` — export `runs`.
- `crates/vestrace-infrastructure/src/postgres/mod.rs` — export `PgRunRepository`.
- `crates/vestrace-http/Cargo.toml` — declare only the dependencies actually used by the stabilized HTTP crate.
- `crates/vestrace-http/src/lib.rs` — retain public router exports.
- `crates/vestrace-http/src/router.rs` — store run use cases and compose routes once.
- `crates/vestrace-http/src/api/mod.rs` — register real run routes and explicit unsupported routes.
- `crates/vestrace-http/src/api/ag_ui.rs` — replace synthetic events with explicit `501` responses.
- `crates/vestrace-cli/src/commands/server.rs` — compose `PgRunRepository`, `RunService`, and `AppState`.
- `apps/console/src/sdk/client.ts` — remove fallback success values and align run DTOs.
- `apps/console/src/routes/HomePage.tsx` — display API failure state.
- `apps/console/src/routes/RunsPage.tsx` — display real run fields and create failures.
- `apps/console/src/routes/ArtifactsPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/AgentsPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/WorkflowsPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/TriggersPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/ConnectionsPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/ModelsPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/EvaluationsPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/AuditPage.tsx` — display unsupported/error state.
- `apps/console/src/routes/ProfilePage.tsx` — display unsupported/error state.
- `apps/console/package.json` — add type-check script and remove obsolete React Router v5 types.
- `apps/console/package-lock.json` — lock dependency changes.
- `.github/workflows/ci.yml` — build and type-check the console.
- `README.md` — describe the real P0 maturity level and required context headers.

---

### Task 1: Replace the broken HTTP prototype with a compilable truthful shell

**Files:**
- Modify: `crates/vestrace-http/Cargo.toml`
- Modify: `crates/vestrace-http/src/router.rs`
- Replace: `crates/vestrace-http/src/api/mod.rs`
- Replace: `crates/vestrace-http/src/api/ag_ui.rs`
- Create: `crates/vestrace-http/src/api/error.rs`
- Create: `crates/vestrace-http/tests/router_contract.rs`

**Interfaces:**
- Consumes: `AppState::new(Arc<dyn HealthRepository>)` during this transitional task.
- Produces: `ApiError::not_implemented(feature: &'static str)` and correctly mounted `/v1` and `/ag-ui` routers.

- [ ] **Step 1: Reproduce the current compile failure**

Run:

```bash
cargo check -p vestrace-http --all-features
```

Expected: failure containing unresolved `serde_json`, `sqlx`, `chrono`, `futures_util`, or `tokio_stream`; missing `api::ag_ui`; and/or missing `HealthRepository::as_any`.

- [ ] **Step 2: Write the route-contract test before changing the router**

Create `crates/vestrace-http/tests/router_contract.rs`:

```rust
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use vestrace_application::{ApplicationError, HealthRepository};
use vestrace_http::{AppState, build_router};

struct HealthyRepository;

#[async_trait::async_trait]
impl HealthRepository for HealthyRepository {
    async fn check(&self) -> Result<(), ApplicationError> {
        Ok(())
    }
}

fn app() -> axum::Router {
    build_router(AppState::new(Arc::new(HealthyRepository)))
}

#[tokio::test]
async fn v1_is_applied_exactly_once() {
    let response = app()
        .oneshot(Request::builder().uri("/v1/runs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

    let duplicated = app()
        .oneshot(Request::builder().uri("/v1/v1/runs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(duplicated.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ag_ui_is_explicitly_unavailable() {
    let response = app()
        .oneshot(Request::builder().uri("/ag-ui/run").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}
```

- [ ] **Step 3: Run the test to verify the current implementation cannot satisfy it**

Run:

```bash
cargo test -p vestrace-http --test router_contract
```

Expected: compile failure or route-construction failure.

- [ ] **Step 4: Introduce a structured unsupported response**

Create `crates/vestrace-http/src/api/error.rs`:

```rust
use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ErrorBody,
}

impl ApiError {
    pub fn not_implemented(feature: &'static str) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            body: ErrorBody {
                code: "not_implemented",
                message: format!("{feature} is not implemented in the P0 foundation"),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self.body)).into_response()
    }
}
```

Add `serde_json.workspace = true` to `crates/vestrace-http/Cargo.toml`. Do not add SQLx, Chrono, Futures, or Tokio Stream to the HTTP crate; P0 removes the code that required them.

- [ ] **Step 5: Replace prototype API registration with explicit shell routes**

Use this structure in `crates/vestrace-http/src/api/mod.rs`:

```rust
use axum::{Router, routing::{get, post}};

use crate::AppState;

pub mod ag_ui;
mod error;

pub use error::ApiError;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/runs", get(unsupported_runs).post(unsupported_runs))
        .route("/runs/{id}", get(unsupported_runs))
        .route("/runs/{id}/approve", post(unsupported_approval))
        .route("/artifacts", get(unsupported_artifacts))
        .route("/agents", get(unsupported_agents))
        .route("/workflows", get(unsupported_workflows))
        .route("/triggers", get(unsupported_triggers))
        .route("/connections", get(unsupported_connections))
        .route("/models", get(unsupported_models))
        .route("/evaluations", get(unsupported_evaluations))
        .route("/audit", get(unsupported_audit))
        .route("/metrics/summary", get(unsupported_metrics))
        .route("/system/health", get(unsupported_system_health))
        .route("/profile", get(unsupported_profile))
}

macro_rules! unsupported_handler {
    ($name:ident, $feature:literal) => {
        async fn $name() -> ApiError {
            ApiError::not_implemented($feature)
        }
    };
}

unsupported_handler!(unsupported_runs, "run API");
unsupported_handler!(unsupported_approval, "run approval API");
unsupported_handler!(unsupported_artifacts, "artifact API");
unsupported_handler!(unsupported_agents, "agent API");
unsupported_handler!(unsupported_workflows, "workflow API");
unsupported_handler!(unsupported_triggers, "trigger API");
unsupported_handler!(unsupported_connections, "connection API");
unsupported_handler!(unsupported_models, "model API");
unsupported_handler!(unsupported_evaluations, "evaluation API");
unsupported_handler!(unsupported_audit, "audit API");
unsupported_handler!(unsupported_metrics, "metrics API");
unsupported_handler!(unsupported_system_health, "system health API");
unsupported_handler!(unsupported_profile, "profile API");
```

Replace `api/ag_ui.rs` with three explicit routes returning `ApiError::not_implemented("AG-UI")`. Keep `/endpoints`, `/events/stream`, and `/run` registered so clients receive `501`, not `404` or fake events.

- [ ] **Step 6: Correct router composition**

Keep the outer mounts in `router.rs`:

```rust
.nest("/v1", crate::api::api_routes())
.nest("/ag-ui", crate::api::ag_ui::ag_ui_routes())
```

No route inside `api_routes()` may begin with `/v1`.

- [ ] **Step 7: Verify the shell**

Run:

```bash
cargo fmt --all --check
cargo test -p vestrace-http --test router_contract
cargo check -p vestrace-http --all-features
rg "sqlx::|as_any|mock_runs_fallback|chrono::|futures_util|tokio_stream" crates/vestrace-http/src
```

Expected: all commands pass; `rg` returns no matches.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-http
git commit -m "fix(http): restore compilable truthful API shell"
```

---

### Task 2: Add the application-level run contract and service

**Files:**
- Create: `crates/vestrace-application/src/runs/mod.rs`
- Create: `crates/vestrace-application/src/runs/commands.rs`
- Create: `crates/vestrace-application/src/runs/ports.rs`
- Create: `crates/vestrace-application/src/runs/service.rs`
- Modify: `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Consumes: `RequestContext`, `ApplicationError`, `AgentRun`, `AgentRunId`, and `now()`.
- Produces:
  - `CreateRunCommand { pub title: String }`
  - `RunRepository::{create, list, find_by_id}`
  - `RunUseCases::{create_run, list_runs, get_run}`
  - `RunService::new(Arc<dyn RunRepository>)`

- [ ] **Step 1: Write service tests with an in-memory repository**

At the bottom of `service.rs`, add tests covering creation, blank-title rejection, list limits, and lookup. The central test must be equivalent to:

```rust
#[tokio::test]
async fn create_run_persists_a_workspace_bound_run() {
    let repository = Arc::new(InMemoryRunRepository::default());
    let service = RunService::new(repository.clone());
    let context = RequestContext::new(WorkspaceId::new(), PrincipalId::new());

    let run = service
        .create_run(
            &context,
            CreateRunCommand {
                title: "  Verify retention policy  ".to_owned(),
            },
        )
        .await
        .unwrap();

    assert_eq!(run.title, "Verify retention policy");
    assert_eq!(run.workspace_id, context.workspace_id);
    assert_eq!(run.principal_id, context.principal_id);
    assert_eq!(run.status, RunStatus::Created);
    assert_eq!(repository.saved.lock().unwrap().as_slice(), &[run]);
}
```

The blank-title test must assert `ApplicationError::Domain(DomainError::InvalidArgument(_))`. The list test must assert that `limit == 0` and `limit > 100` are rejected.

- [ ] **Step 2: Run the tests to verify the module is absent**

Run:

```bash
cargo test -p vestrace-application runs::service::tests
```

Expected: failure because the `runs` module and interfaces do not exist.

- [ ] **Step 3: Define the command and ports**

`commands.rs`:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateRunCommand {
    pub title: String,
}
```

`ports.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use vestrace_domain::{AgentRun, AgentRunId};

use crate::{ApplicationError, RequestContext};
use super::CreateRunCommand;

#[async_trait]
pub trait RunRepository: Send + Sync {
    async fn create(
        &self,
        context: &RequestContext,
        run: &AgentRun,
    ) -> Result<(), ApplicationError>;

    async fn list(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError>;

    async fn find_by_id(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError>;
}

#[async_trait]
pub trait RunUseCases: Send + Sync {
    async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError>;

    async fn list_runs(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError>;

    async fn get_run(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError>;
}

pub type SharedRunRepository = Arc<dyn RunRepository>;
pub type SharedRunUseCases = Arc<dyn RunUseCases>;
```

- [ ] **Step 4: Implement the minimal service**

`service.rs` must trim titles, reject empty titles, accept list limits from `1..=100`, create UUIDv7 run IDs through `AgentRunId::new()`, and delegate persistence:

```rust
pub struct RunService {
    repository: SharedRunRepository,
}

impl RunService {
    pub fn new(repository: SharedRunRepository) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl RunUseCases for RunService {
    async fn create_run(
        &self,
        context: &RequestContext,
        command: CreateRunCommand,
    ) -> Result<AgentRun, ApplicationError> {
        let title = command.title.trim();
        if title.is_empty() {
            return Err(DomainError::InvalidArgument("run title is required".to_owned()).into());
        }

        let run = AgentRun::new(
            AgentRunId::new(),
            context.workspace_id,
            context.principal_id,
            title,
            now(),
        );
        self.repository.create(context, &run).await?;
        Ok(run)
    }

    async fn list_runs(
        &self,
        context: &RequestContext,
        limit: u32,
    ) -> Result<Vec<AgentRun>, ApplicationError> {
        if !(1..=100).contains(&limit) {
            return Err(DomainError::InvalidArgument(
                "run list limit must be between 1 and 100".to_owned(),
            )
            .into());
        }
        self.repository.list(context, limit).await
    }

    async fn get_run(
        &self,
        context: &RequestContext,
        id: AgentRunId,
    ) -> Result<Option<AgentRun>, ApplicationError> {
        self.repository.find_by_id(context, id).await
    }
}
```

- [ ] **Step 5: Export the module and verify**

Add `pub mod runs;` and `pub use runs::*;` to `src/lib.rs`.

Run:

```bash
cargo fmt --all --check
cargo test -p vestrace-application runs::service::tests
cargo clippy -p vestrace-application --all-targets --all-features -- -D warnings
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/vestrace-application
git commit -m "feat(application): add run use cases"
```

---

### Task 3: Implement the scoped PostgreSQL run repository

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/run_repository.rs`
- Create: `crates/vestrace-infrastructure/tests/run_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`

**Interfaces:**
- Consumes: `RunRepository`, `RequestContext`, `PgStore::begin_scoped`, and the existing `agent_runs` table.
- Produces: `PgRunRepository::new(PgStore)` implementing all three repository operations.

- [ ] **Step 1: Write the integration test first**

Use `#[sqlx::test(migrations = "../../migrations")]`. Seed two workspaces and principals with direct fixture SQL, then create a run through the repository under workspace A. Assert that workspace A can list and fetch it while workspace B sees neither.

The fixture inserts must use the existing required columns:

```sql
INSERT INTO workspaces (id, name, slug)
VALUES ($1, 'Workspace A', 'workspace-a'), ($2, 'Workspace B', 'workspace-b');

INSERT INTO principals (id, workspace_id, name, principal_type)
VALUES
  ($1, $2, 'Principal A', 'user'),
  ($3, $4, 'Principal B', 'user');
```

The core assertions must be:

```rust
repository.create(&context_a, &run).await.unwrap();
assert_eq!(repository.list(&context_a, 50).await.unwrap(), vec![run.clone()]);
assert_eq!(repository.find_by_id(&context_a, run.id).await.unwrap(), Some(run.clone()));
assert!(repository.list(&context_b, 50).await.unwrap().is_empty());
assert_eq!(repository.find_by_id(&context_b, run.id).await.unwrap(), None);
```

- [ ] **Step 2: Run the test to verify the repository is absent**

Run:

```bash
cargo test -p vestrace-infrastructure --test run_repository
```

Expected: compile failure because `PgRunRepository` does not exist.

- [ ] **Step 3: Implement row decoding**

Create a private `StoredRun` with `sqlx::FromRow` fields matching:

```rust
struct StoredRun {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    principal_id: uuid::Uuid,
    title: String,
    status: String,
    run_version: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}
```

Implement `TryFrom<StoredRun> for AgentRun`. Map only these status strings:

```text
created
running
waiting_for_input
waiting_for_approval
completed
failed
cancelled
```

Unknown status values and non-positive versions must return `ApplicationError::Storage` with a non-secret diagnostic message.

- [ ] **Step 4: Implement scoped create/list/find operations**

Every method must:

1. call `self.store.begin_scoped(context)`;
2. execute SQL through `transaction.connection()`;
3. commit on success;
4. return `ApplicationError::Storage` on infrastructure or SQLx failure.

Create SQL:

```sql
INSERT INTO agent_runs (
    id,
    workspace_id,
    principal_id,
    title,
    status,
    run_version,
    created_at,
    updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
```

List SQL:

```sql
SELECT id, workspace_id, principal_id, title, status, run_version, created_at, updated_at
FROM agent_runs
ORDER BY created_at DESC, id DESC
LIMIT $1
```

Find SQL:

```sql
SELECT id, workspace_id, principal_id, title, status, run_version, created_at, updated_at
FROM agent_runs
WHERE id = $1
```

Bind `limit` as `i64`. Do not set session variables outside the transaction.

- [ ] **Step 5: Export and verify**

Add:

```rust
pub mod run_repository;
pub use run_repository::PgRunRepository;
```

to `postgres/mod.rs`.

Run:

```bash
cargo fmt --all --check
cargo test -p vestrace-infrastructure --test run_repository
cargo clippy -p vestrace-infrastructure --all-targets --all-features -- -D warnings
```

Expected: all pass against the test database created by `sqlx::test`.

- [ ] **Step 6: Commit**

```bash
git add crates/vestrace-infrastructure
git commit -m "feat(postgres): persist runs through scoped repository"
```

---

### Task 4: Expose the real run vertical slice through HTTP

**Files:**
- Create: `crates/vestrace-http/src/api/context.rs`
- Create: `crates/vestrace-http/src/api/runs.rs`
- Modify: `crates/vestrace-http/src/api/error.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Create: `crates/vestrace-http/tests/run_routes.rs`

**Interfaces:**
- Consumes: `SharedRunUseCases`, `CreateRunCommand`, `AgentRunId`, and `RequestContext`.
- Produces:
  - request headers `x-workspace-id` and `x-principal-id`;
  - `CreateRunRequest { title: String }`;
  - `RunResponse { id, title, status, version, created_at, updated_at }`;
  - real handlers for create/list/get.

- [ ] **Step 1: Write HTTP contract tests with a fake use-case object**

Create a `FakeRunUseCases` implementing `RunUseCases` with an `Arc<Mutex<Vec<AgentRun>>>`. Build state with both a healthy repository and the fake use case.

Required tests:

```rust
#[tokio::test]
async fn create_run_requires_workspace_and_principal_headers() { /* POST without headers -> 400 */ }

#[tokio::test]
async fn create_run_returns_201_and_the_persisted_contract() { /* POST valid JSON -> 201 */ }

#[tokio::test]
async fn list_runs_returns_200_without_mock_fields() { /* GET -> JSON array */ }

#[tokio::test]
async fn get_unknown_run_returns_404() { /* GET UUIDv7 -> 404 */ }

#[tokio::test]
async fn malformed_run_id_returns_400() { /* GET /v1/runs/not-a-uuid -> 400 */ }
```

Every successful request must include:

```rust
.header("x-workspace-id", workspace_id.to_string())
.header("x-principal-id", principal_id.to_string())
```

- [ ] **Step 2: Run the tests to verify the routes are still unsupported**

Run:

```bash
cargo test -p vestrace-http --test run_routes
```

Expected: tests fail because run routes return `501` and `AppState` has no run use-case field.

- [ ] **Step 3: Add context parsing**

`context.rs` must expose:

```rust
pub fn request_context(headers: &HeaderMap) -> Result<RequestContext, ApiError>
```

Parse `x-workspace-id` and `x-principal-id` as UUIDs, convert them with `WorkspaceId::from_uuid` and `PrincipalId::from_uuid`, and return `ApiError::bad_request` for missing, non-UTF-8, or invalid UUID values. Do not silently substitute default identities.

- [ ] **Step 4: Extend `ApiError` without leaking internals**

Add constructors:

```rust
pub fn bad_request(message: impl Into<String>) -> Self;
pub fn not_found(resource: &'static str) -> Self;
pub fn from_application(error: ApplicationError) -> Self;
```

Map errors exactly:

```text
Domain      -> 400 invalid_request
Conflict    -> 409 conflict
Policy      -> 403 forbidden
Unavailable -> 503 unavailable
Storage     -> 500 storage_failure
Internal    -> 500 internal_failure
```

For `Storage` and `Internal`, return generic client messages and log the original error with `tracing::error!`; do not serialize SQL, connection strings, or internal messages.

- [ ] **Step 5: Define the stable P0 run DTO**

`runs.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub id: uuid::Uuid,
    pub title: String,
    pub status: RunStatus,
    pub version: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl From<AgentRun> for RunResponse {
    fn from(run: AgentRun) -> Self {
        Self {
            id: run.id.as_uuid(),
            title: run.title,
            status: run.status,
            version: run.version.value(),
            created_at: run.created_at,
            updated_at: run.updated_at,
        }
    }
}
```

Handlers:

```rust
pub async fn create_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError>;

pub async fn list_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<RunResponse>>, ApiError>;

pub async fn get_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, ApiError>;
```

`list_runs` must call `list_runs(&context, 50)`. `get_run` must parse `AgentRunId` with `FromStr` and return `404` when the use case returns `None`.

- [ ] **Step 6: Extend `AppState` and register the real routes**

`AppState` becomes:

```rust
#[derive(Clone)]
pub struct AppState {
    health_repository: Arc<dyn HealthRepository>,
    run_use_cases: SharedRunUseCases,
}

impl AppState {
    pub fn new(
        health_repository: Arc<dyn HealthRepository>,
        run_use_cases: SharedRunUseCases,
    ) -> Self {
        Self {
            health_repository,
            run_use_cases,
        }
    }

    pub(crate) fn health_repository(&self) -> &dyn HealthRepository {
        self.health_repository.as_ref()
    }

    pub(crate) fn run_use_cases(&self) -> &dyn RunUseCases {
        self.run_use_cases.as_ref()
    }
}
```

Replace only these shell routes:

```rust
.route("/runs", get(runs::list_runs).post(runs::create_run))
.route("/runs/{id}", get(runs::get_run))
```

Leave approval and every non-run product surface as explicit `501`.

Update `router_contract.rs` to construct a no-op fake `RunUseCases` and keep its original assertions.

- [ ] **Step 7: Verify the HTTP contract**

Run:

```bash
cargo fmt --all --check
cargo test -p vestrace-http --test router_contract --test run_routes
cargo clippy -p vestrace-http --all-targets --all-features -- -D warnings
rg "sqlx::|PgStore|PgPool|as_any" crates/vestrace-http
```

Expected: tests and clippy pass; `rg` returns no matches.

- [ ] **Step 8: Commit**

```bash
git add crates/vestrace-http
git commit -m "feat(http): expose real run create list and get routes"
```

---

### Task 5: Wire the PostgreSQL run service into the server

**Files:**
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: affected HTTP unit-test constructors if compilation identifies any remaining one-argument `AppState::new` calls.

**Interfaces:**
- Consumes: `PgRunRepository::new(PgStore)`, `RunService::new(SharedRunRepository)`, and two-argument `AppState::new`.
- Produces: the production composition root for the P0 run slice.

- [ ] **Step 1: Run the CLI check to expose the composition break**

Run:

```bash
cargo check -p vestrace-cli --all-features
```

Expected: failure because `AppState::new` now requires run use cases.

- [ ] **Step 2: Compose the application service**

After migration succeeds in `server.rs`, construct dependencies exactly once:

```rust
let run_repository = Arc::new(PgRunRepository::new(store.clone()));
let run_service = Arc::new(RunService::new(run_repository));
let router = build_router(AppState::new(Arc::new(store), run_service));
```

Import `RunService` from `vestrace_application` and `PgRunRepository` from `vestrace_infrastructure`. Do not expose the pool and do not add a database getter to `PgStore`.

- [ ] **Step 3: Verify production wiring**

Run:

```bash
cargo fmt --all --check
cargo check -p vestrace-cli --all-features
cargo test -p vestrace-cli --all-targets --all-features
cargo clippy -p vestrace-cli --all-targets --all-features -- -D warnings
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vestrace-cli crates/vestrace-http
git commit -m "feat(server): wire postgres run service"
```

---

### Task 6: Remove false-success behavior from the console

**Files:**
- Create: `apps/console/src/sdk/useApiResource.ts`
- Modify: `apps/console/src/sdk/client.ts`
- Modify: `apps/console/src/routes/HomePage.tsx`
- Modify: `apps/console/src/routes/RunsPage.tsx`
- Modify: `apps/console/src/routes/ArtifactsPage.tsx`
- Modify: `apps/console/src/routes/AgentsPage.tsx`
- Modify: `apps/console/src/routes/WorkflowsPage.tsx`
- Modify: `apps/console/src/routes/TriggersPage.tsx`
- Modify: `apps/console/src/routes/ConnectionsPage.tsx`
- Modify: `apps/console/src/routes/ModelsPage.tsx`
- Modify: `apps/console/src/routes/EvaluationsPage.tsx`
- Modify: `apps/console/src/routes/AuditPage.tsx`
- Modify: `apps/console/src/routes/ProfilePage.tsx`

**Interfaces:**
- Consumes: the P0 JSON error body and run DTO.
- Produces: rejected promises on API failure, explicit UI error states, and no automatic mock data.

- [ ] **Step 1: Record the current false-success behavior**

Run:

```bash
rg "\.catch\(\(\) =>|returning fallback data|0194f4a0-7b3c|Compliance-Bot|claude-3-5-sonnet" apps/console/src/sdk/client.ts
```

Expected: multiple fallback blocks and synthetic records are found.

- [ ] **Step 2: Replace the request helper with typed errors**

Define:

```ts
export interface ApiErrorBody {
  code: string;
  message: string;
}

export class ApiRequestError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: ApiErrorBody,
  ) {
    super(body.message);
  }
}

async function request<T>(endpoint: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      'x-workspace-id': import.meta.env.VITE_VESTRACE_WORKSPACE_ID ?? '',
      'x-principal-id': import.meta.env.VITE_VESTRACE_PRINCIPAL_ID ?? '',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    const body = (await response.json()) as ApiErrorBody;
    throw new ApiRequestError(response.status, body);
  }

  return (await response.json()) as T;
}
```

Do not invent identity defaults. An unset environment variable must produce a backend `400`, which the UI displays.

- [ ] **Step 3: Align the run contract**

Replace `RunItem` with:

```ts
export type RunStatus =
  | 'created'
  | 'running'
  | 'waiting_for_input'
  | 'waiting_for_approval'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface RunItem {
  id: string;
  title: string;
  status: RunStatus;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface CreateRunPayload {
  title: string;
}
```

All client methods must return `request(...)` directly. Remove every `.catch(() => syntheticValue)` block.

- [ ] **Step 4: Add one reusable read hook**

Create `useApiResource.ts`:

```ts
import { useEffect, useState } from 'react';

export function useApiResource<T>(load: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    load()
      .then((value) => {
        if (active) setData(value);
      })
      .catch((reason: unknown) => {
        if (active) {
          setError(reason instanceof Error ? reason.message : 'API request failed');
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [load]);

  return { data, error, loading };
}
```

Callers must pass a stable imported client function, not a new inline function on every render.

- [ ] **Step 5: Update every read page to show an error instead of empty success**

Use the exact client-method mapping:

```text
HomePage        -> vestraceClient.getMetricsSummary
RunsPage        -> vestraceClient.listRuns
ArtifactsPage   -> vestraceClient.listArtifacts
AgentsPage      -> vestraceClient.listAgents
WorkflowsPage   -> vestraceClient.listWorkflows
TriggersPage    -> vestraceClient.listTriggers
ConnectionsPage -> vestraceClient.listConnections
ModelsPage      -> vestraceClient.listModels
EvaluationsPage -> vestraceClient.listEvaluations
AuditPage       -> vestraceClient.listAuditEvents
ProfilePage     -> vestraceClient.getProfile
```

For each page, render this before the normal content:

```tsx
if (error) {
  return (
    <div role="alert" style={{ padding: '24px' }}>
      Backend data is unavailable: {error}
    </div>
  );
}
```

The unsupported endpoints will therefore visibly show the server's `501` message.

- [ ] **Step 6: Make `RunsPage` honest**

Change create to:

```ts
const newRun = await vestraceClient.createRun({ title: 'Manual Operator Execution Trigger' });
```

Track a `mutationError` state and show it with `role="alert"`; never append a fabricated run after a failed request.

Replace the table columns with only:

```text
Title & ID | Status | Version | Created | Updated
```

Remove Agent, Tokens, Cost, Duration, progress language, and the claim that runs are already event-sourced. Replace the subtitle with:

```text
Persisted run records available in the P0 foundation.
```

- [ ] **Step 7: Verify no fallback remains**

Run:

```bash
rg "\.catch\(\(\) =>|returning fallback data|Compliance-Bot|DB-Optimizer|Memory-Extractor|claude-3-5-sonnet" apps/console/src
```

Expected: no matches.

Then run:

```bash
npm --prefix apps/console run build
```

Expected: successful TypeScript and Vite build.

- [ ] **Step 8: Commit**

```bash
git add apps/console/src
git commit -m "fix(console): surface backend failures instead of mock success"
```

---

### Task 7: Add frontend verification to CI

**Files:**
- Modify: `apps/console/package.json`
- Modify: `apps/console/package-lock.json`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: existing npm lockfile.
- Produces: deterministic console type-check and production build in CI.

- [ ] **Step 1: Add a local type-check command and remove obsolete types**

Update scripts:

```json
{
  "dev": "vite",
  "typecheck": "tsc --noEmit",
  "build": "tsc && vite build",
  "preview": "vite preview"
}
```

Remove `@types/react-router-dom` because React Router 7 ships its own types and the installed package describes React Router 5.

Run:

```bash
npm --prefix apps/console install --package-lock-only
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

Expected: all pass.

- [ ] **Step 2: Add a separate console CI job**

Append:

```yaml
  console:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: apps/console/package-lock.json
      - run: npm ci
        working-directory: apps/console
      - run: npm run typecheck
        working-directory: apps/console
      - run: npm run build
        working-directory: apps/console
```

Do not require backend environment variables for a static build.

- [ ] **Step 3: Validate workflow syntax and local commands**

Run:

```bash
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
cargo fmt --all --check
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add apps/console/package.json apps/console/package-lock.json .github/workflows/ci.yml
git commit -m "ci: verify console typecheck and build"
```

---

### Task 8: Verify the full P0 contract and update maturity documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/database-schema.md` only where it incorrectly claims migrations stop at `0016`.

**Interfaces:**
- Consumes: completed P0 implementation.
- Produces: accurate setup, API contract, and maturity statements.

- [ ] **Step 1: Update the README current-state section**

State exactly:

```text
Implemented:
- Rust workspace and PostgreSQL migrations
- transaction-scoped workspace/principal RLS context
- health endpoints
- PostgreSQL-backed create/list/get run API
- explicit 501 responses for unsupported REST and AG-UI surfaces
- React console that surfaces API failures

Not implemented:
- run execution
- run events, reducers, replay, and checkpoints as application behavior
- approvals
- agents, workflows, triggers, model routing, evaluations, and metrics APIs
- capability attenuation and risk policy enforcement
- artifact CAS
```

Document required headers:

```text
x-workspace-id: UUID
x-principal-id: UUID
```

Document real routes:

```text
POST /v1/runs
GET  /v1/runs
GET  /v1/runs/{id}
```

Do not claim the console or State Engine is complete.

- [ ] **Step 2: Correct migration-count drift**

Replace the fixed `0001–0016` statement with wording that directs readers to the ordered `migrations/` directory as the source of truth. Do not manually maintain a maximum migration number in prose.

- [ ] **Step 3: Run the complete verification matrix**

With PostgreSQL available at `DATABASE_URL`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
docker compose config --quiet
docker compose up --build --detach --wait --wait-timeout 180
./scripts/foundation-smoke.sh
./scripts/foundation-runtime-rls.sh
docker compose down --volumes --remove-orphans
```

Expected: every command succeeds.

- [ ] **Step 4: Perform negative contract checks**

Run against the started server:

```bash
curl -i http://127.0.0.1:8080/v1/runs
curl -i http://127.0.0.1:8080/v1/v1/runs
curl -i http://127.0.0.1:8080/ag-ui/run
```

Expected:

```text
/v1/runs without identity headers -> 400
/v1/v1/runs -> 404
/ag-ui/run -> 501
```

Search for forbidden implementation patterns:

```bash
rg "sqlx::|PgStore|PgPool|as_any" crates/vestrace-http
rg "mock_runs_fallback|returning fallback data|\.catch\(\(\) =>" crates/vestrace-http apps/console/src
rg '\.route\("/v1|/:id' crates/vestrace-http/src
```

Expected: no matches.

- [ ] **Step 5: Commit the documentation**

```bash
git add README.md docs/database-schema.md
git commit -m "docs: describe truthful P0 foundation"
```

- [ ] **Step 6: Review the final commit range**

Run:

```bash
git log --oneline --decorate main..HEAD
git diff --stat main...HEAD
git diff --check main...HEAD
```

Expected: eight focused commits, no whitespace errors, and no unrelated feature additions.

---

## Completion Criteria

P0 is complete only when all of the following are simultaneously true:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- `cargo test --workspace --all-targets --all-features` passes.
- the console type-check and production build pass in CI.
- the router starts without Axum path-syntax panic.
- `/v1/runs` is not duplicated as `/v1/v1/runs`.
- HTTP contains no direct SQL or database downcast.
- run create/list/get operations pass through application interfaces and scoped PostgreSQL transactions.
- unsupported endpoints return `501`, not synthetic success data.
- missing identity headers return `400`, not fixed default identities.
- the console visibly reports backend errors and contains no automatic fallback fixtures.
- README claims match actual behavior.
