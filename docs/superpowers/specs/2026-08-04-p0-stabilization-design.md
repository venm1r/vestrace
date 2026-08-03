# Vestrace P0 Stabilization Design

Date: 2026-08-04
Status: Approved direction, pending implementation plan
Baseline: `main` at `466b1d4f641225abf62bbde528ab534531432877`

## 1. Purpose

Restore a truthful, compilable, testable foundation before adding new product surfaces or State Engine capabilities.

P0 is complete only when the Rust workspace builds, the HTTP router starts without route-registration panics, the documented API paths resolve exactly once, health endpoints still work, and API failures are reported rather than replaced with successful mock responses.

## 2. Current root causes

### 2.1 Incomplete HTTP crate boundary

`vestrace-http` uses `serde_json`, `sqlx`, `chrono`, `futures-util`, and `tokio-stream`, but does not declare them as direct dependencies. The crate also references `crate::api::ag_ui` without declaring the module.

This is a compile-time boundary failure, not a database or deployment problem.

### 2.2 Invalid dependency inversion

`AppState` stores only `Arc<dyn HealthRepository>`, but API handlers try to recover a PostgreSQL pool with `as_any()` and a concrete downcast. `HealthRepository` exposes only `check()`, and the object supplied by the CLI is `PgStore`, not `PgPool`.

Even if an `as_any()` escape hatch were added, direct SQL in the HTTP adapter would violate the intended architecture: HTTP should call application services, and infrastructure should implement application ports.

### 2.3 Broken route composition

`build_router()` nests `api_routes()` at `/v1`, while `api_routes()` registers paths that already begin with `/v1`. The resulting effective paths are `/v1/v1/...`.

Dynamic paths use Axum 0.7-style `:id` captures while the workspace declares Axum 0.8. P0 will use `/{id}` and will not disable Axum's compatibility checks.

### 2.4 False-success behavior

Several handlers ignore SQL errors and return success or mock data. The console client also catches API failures and returns mock data. This makes an unavailable or broken backend look healthy.

P0 must distinguish demo fixtures from real API behavior. Production/default paths must fail explicitly.

### 2.5 Missing verification coverage

Rust CI exists, but the latest inspected commit has no associated status checks visible through the connected GitHub data. Frontend build/type-check coverage is not present in the workflow.

## 3. Considered approaches

### Approach A — Minimal compile patch only

Add missing dependencies, expose `ag_ui`, add an `as_any()` method, and repair route strings.

Advantages:
- Smallest diff.
- Fastest route to a compiling crate.

Disadvantages:
- Preserves direct SQL in HTTP.
- Preserves false-success behavior.
- Turns a temporary architectural violation into a supported interface.

Decision: rejected.

### Approach B — Remove all prototype API surfaces

Keep only health endpoints until the State Engine application layer exists.

Advantages:
- Very clean foundation.
- No misleading product behavior.

Disadvantages:
- Removes the current console contract entirely.
- Produces a larger visible regression than necessary.
- Makes subsequent vertical-slice work harder to demonstrate.

Decision: rejected as the default, retained as a fallback if the existing API cannot be stabilized without broad refactoring.

### Approach C — Stabilize the boundary and keep one truthful vertical slice

Repair compilation and routing, remove database downcasting from HTTP, introduce a minimal run application port/service, retain only endpoints that can be backed by real behavior, and mark or isolate remaining demo endpoints.

Advantages:
- Restores architecture instead of hiding the violation.
- Preserves a usable API path for the next State Engine slice.
- Gives tests a narrow, meaningful contract.

Disadvantages:
- Slightly larger than a dependency-only patch.
- Requires coordinated changes across application, infrastructure, HTTP, CLI, and tests.

Decision: selected.

## 4. Selected P0 design

### 4.1 State composition

`AppState` will contain application-facing services or ports only. It will not expose `PgStore`, `PgPool`, `sqlx`, or a generic downcast mechanism.

Initial shape:

- `health_repository: Arc<dyn HealthRepository>`
- `run_service: Arc<RunService>` or `Arc<dyn RunUseCases>`

The exact service form should follow existing application-layer conventions. The important invariant is that HTTP depends on `vestrace-application`, not `vestrace-infrastructure` or SQLx.

### 4.2 Minimal run contract

P0 supports only a narrow real contract:

- Create a run.
- List runs for the request workspace.
- Get one run by ID.

Approval, AG-UI execution, metrics, artifacts, agents, workflows, triggers, connections, models, evaluations, audit, and profile endpoints are outside the real P0 vertical slice.

They must either:

1. be removed from the default router;
2. return `501 Not Implemented`; or
3. be mounted only under an explicit demo feature/profile.

They must not silently return production-looking fixtures.

### 4.3 Request context

Workspace and principal IDs must enter through an explicit request context mechanism. P0 must not insert a default workspace or principal inside a GET handler.

For tests and local development, deterministic IDs may be supplied by test middleware or an explicit development configuration, not created as a side effect of listing runs.

### 4.4 Database access

A new application port will describe run persistence. PostgreSQL infrastructure will implement it using scoped transactions and existing RLS conventions.

Minimum repository operations:

- `create_run(context, input)`
- `list_runs(context, limit)`
- `get_run(context, run_id)`

Every operation executes inside a transaction that sets workspace and principal context locally to that transaction.

### 4.5 Error behavior

- Invalid input: `400 Bad Request`.
- Missing run: `404 Not Found`.
- Authorization or RLS denial: `403 Forbidden` where safely distinguishable.
- Unavailable database/application dependency: `503 Service Unavailable`.
- Unimplemented prototype endpoint: `501 Not Implemented`.
- Unexpected internal error: `500 Internal Server Error` with no secret-bearing detail.

No handler may ignore a failed write and return `201 Created`.

### 4.6 Routing

The outer router owns the `/v1` prefix. The nested API router registers relative paths:

- `/runs`
- `/runs/{id}`

AG-UI remains under `/ag-ui` only if it compiles and is clearly marked as non-executing/demo behavior; otherwise it is removed from the default P0 router.

### 4.7 Frontend behavior

The console may keep explicitly labelled demo fixtures, but API client methods must not replace arbitrary network/server errors with successful mock responses.

Recommended separation:

- real client: throws/returns typed errors;
- demo data provider: selected explicitly through development configuration;
- UI: displays unavailable, empty, and error states honestly.

## 5. Implementation sequence

Each step is independently testable and should be committed separately.

1. **Baseline reproduction**
   - Run formatting, Clippy, and workspace tests.
   - Capture the first complete set of compiler errors before editing.

2. **HTTP crate compilation boundary**
   - Declare required direct dependencies or remove unused prototype code that requires them.
   - Declare the AG-UI module if retained.
   - Remove unused imports under `-D warnings`.

3. **Router correctness**
   - Make nested routes relative.
   - Convert dynamic captures to Axum 0.8 syntax.
   - Add router tests for exact paths and startup without panic.

4. **Application run port/service**
   - Define minimal input/output types and repository port.
   - Add unit tests with an in-memory fake.

5. **PostgreSQL run repository**
   - Implement scoped transactional operations.
   - Add integration tests for workspace isolation and missing records.

6. **HTTP run handlers**
   - Replace SQL/downcasting with application calls.
   - Map application errors to HTTP responses.
   - Add request/response tests.

7. **Remove false-success paths**
   - Stop swallowing database errors.
   - Return `501` or unmount unsupported endpoints.
   - Remove automatic mock fallback from the real console client.

8. **CI coverage**
   - Keep Rust format/Clippy/test gates.
   - Add console install, type-check, and build gates.
   - Ensure compose acceptance tests address the actual `/v1/runs` path.

## 6. Files expected to change

Likely P0 files:

- `crates/vestrace-http/Cargo.toml`
- `crates/vestrace-http/src/api/mod.rs`
- `crates/vestrace-http/src/api/ag_ui.rs`
- `crates/vestrace-http/src/router.rs`
- `crates/vestrace-http/src/lib.rs`
- `crates/vestrace-application/src/lib.rs`
- new application run service/port modules
- `crates/vestrace-infrastructure/src/postgres/mod.rs`
- new PostgreSQL run repository module
- `crates/vestrace-cli/src/commands/server.rs`
- HTTP/application/infrastructure tests
- `apps/console/src/sdk/client.ts`
- `.github/workflows/ci.yml`

Schema migrations should not be changed unless repository implementation reveals a concrete incompatibility with the existing run tables.

## 7. Verification criteria

P0 is accepted only when all of the following are evidenced:

- `cargo fmt --all --check` passes.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes.
- `cargo test --workspace --all-targets --all-features` passes.
- Constructing the router does not panic.
- `GET /health/live` and `GET /health/ready` retain their intended behavior.
- `POST /v1/runs`, `GET /v1/runs`, and `GET /v1/runs/{id}` resolve without duplicated prefixes.
- Failed persistence cannot produce a successful create response.
- A run in workspace A cannot be listed or fetched from workspace B.
- Unsupported endpoints do not return production-looking fixture data.
- Console type-check and production build pass.

## 8. Explicit non-goals

P0 does not implement:

- deterministic State Engine reducers;
- replay and checkpoint restoration;
- capability attenuation;
- risk policy evaluation;
- artifact CAS;
- real AG-UI agent execution;
- workflow scheduling;
- model/provider orchestration.

Those begin only after this stabilization gate is green.

## 9. Next stage after P0

The first P1 vertical slice is:

`CreateRun -> append RunCreated event -> reduce state -> persist checkpoint -> rebuild state by replay -> compare deterministic result`.

No additional horizontal product surface should be added before that slice passes integration tests.
