# Vestrace

Vestrace is an evidence-first knowledge and execution platform. The current foundation provides a Rust service, PostgreSQL persistence with transaction-scoped workspace isolation, health checks, event-sourced run records with deterministic replay and checkpoint recovery, run worker lifecycle (AdvanceRun/ResumeRun/ExecuteStep handlers), HTTP run lifecycle endpoints (create/list/detail/pause/resume/cancel), memory lifecycle services, retrieval, security domain types, and an MCP server.

> **Documentation map:** start with the [documentation index](docs/README.md) for the distinction between the frozen architecture, source-pinned implementation snapshots, and proposed changes. The foundation descriptions below include historical setup and availability assumptions; they are not a new qualification of the current code. The normative product boundary is memory-first persistent cognition, as explained in the [Architecture Guide](docs/architecture.md).
>
> **Memory Workspace implementation design:** [API, Console, import/sync and portability](docs/implementation/memory-workspace/README.md), based on `6f6102536e9a535b7086db14573bf45fe750ad71`. Documentation integration does not make its proposed endpoints available or add MW-00–MW-07 to the frozen P01–P12 release program.

## Current maturity

Implemented:

- Rust workspace and PostgreSQL forward-only migrations.
- Transaction-scoped workspace and principal RLS context.
- HTTP liveness and readiness endpoints.
- PostgreSQL-backed event-sourced run records with deterministic replay, checkpoint recovery, and projection rebuild.
- Run worker lifecycle — `AdvanceRun` (Created→Preparing→Running, step dispatch, finalization), `ResumeRun` (Paused→Running), `ExecuteStep` (Pending→Succeeded) with work-queue leasing, lease heartbeats, and dead-letter routing.
- HTTP run lifecycle endpoints — `POST /v1/runs`, `GET /v1/runs`, `GET /v1/runs/{id}`, `POST /v1/runs/{id}/pause`, `/resume`, `/cancel` with `If-Match` optimistic concurrency.
- Memory lifecycle services with HTTP endpoints (event recording, memory creation, revision, knowledge-relation linking, all with idempotency and outbox).
- Retrieval service with HTTP endpoint (text channel FTS, RRF fusion, deterministic reranking, context pack building).
- Security domain types (capabilities, approvals, sensitivity, audit), policy engine, redaction service, audit repository, HTTP auth middleware.
- MCP server with `search_memories` and `get_memory` tools.
- Job enqueue, leasing via `FOR UPDATE SKIP LOCKED`, and completion.
- Worker process with graceful shutdown.
- Rust format, Clippy, test, and CI gates.

Not implemented:

- Approval execution with production authorization (deterministic test adapter only).
- Capability-policy enforcement in HTTP middleware (PolicyEngine port exists, not wired to routes).
- Artifact content-addressed storage.
- Real AG-UI execution or event streaming.
- Vector/structured/exact retrieval channels (ports defined, not wired).
- Memory extractor backed by an LLM (only a deterministic test double exists).

## HTTP contract

Health routes do not require identity headers:

```text
GET /health/live
GET /health/ready
```

Run routes:

```text
POST /v1/runs
GET  /v1/runs
GET  /v1/runs/{id}
POST /v1/runs/{id}/pause
POST /v1/runs/{id}/resume
POST /v1/runs/{id}/cancel
```

Every run request requires explicit UUID headers:

```text
x-workspace-id: UUID
x-principal-id: UUID
```

Pause, resume, and cancel require `If-Match: <version>` for optimistic concurrency.

Create payload:

```json
{
  "title": "Verify retention policy"
}
```

Creating a run persists a run record in `created` status. The worker activates it through `Created→Preparing→Running` and dispatches step execution via the work queue.

## Local container environment

Prerequisites: Docker Engine with Docker Compose v2, plus `curl`, Python 3, and Bash for the smoke checks. The Compose file uses distinct fixed credentials named `bootstrap-local-development-only` and `runtime-local-development-only`; they are exclusively for an isolated developer machine and must never be reused for production or an externally reachable database.

Build and start both services:

```bash
docker compose -p vestrace-foundation up --build -d
./scripts/foundation-smoke.sh
bash ./scripts/foundation-run-smoke.sh
```

The run smoke script explicitly seeds two local-only workspace/principal pairs, verifies create/list/get through HTTP, and confirms that the created run is hidden from the second workspace. It prints the identity values that can be supplied to the console. Identity creation remains outside the HTTP adapter; production systems must provision identities through an authenticated control plane.

The server listens inside the container on `0.0.0.0:8080`, while Compose publishes it only as `127.0.0.1:8080` by default. To use another loopback port:

```bash
VESTRACE_HTTP_PORT=18080 docker compose -p vestrace-foundation up --build -d
./scripts/foundation-smoke.sh http://127.0.0.1:18080
bash ./scripts/foundation-run-smoke.sh http://127.0.0.1:18080
```

Compose waits for PostgreSQL to report healthy before starting the server. The server connects, applies embedded migrations, and then begins serving. `/health/live` reports process liveness. `/health/ready` checks database access and exact migration compatibility.

The PostgreSQL image bootstraps with the local-only `vestrace_bootstrap` administrator and provisions a separate restricted `vestrace` runtime login. Verify HTTP health and runtime RLS behavior with:

```bash
./scripts/foundation-smoke.sh
bash ./scripts/foundation-run-smoke.sh
./scripts/foundation-runtime-rls.sh
```

Production deployments must use managed secrets and independently provisioned least-privilege migration and runtime identities. The fixed Compose credentials are not a production template.

Stop services while retaining data:

```bash
docker compose -p vestrace-foundation down --remove-orphans
```

Remove the development database volume as well:

```bash
docker compose -p vestrace-foundation down -v --remove-orphans
```

## Console

The console is located in `apps/console`. Configure the request identity explicitly. The local run smoke prints a usable development pair:

```text
VITE_VESTRACE_WORKSPACE_ID=<workspace UUID>
VITE_VESTRACE_PRINCIPAL_ID=<principal UUID>
```

An absent identity is not replaced with a default. The backend returns `400 Bad Request`, and the console displays that failure.

Run locally:

```bash
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
npm --prefix apps/console run dev
```

## Configuration

Configuration precedence is: built-in defaults, an optional non-secret TOML file selected with `--config`, `VESTRACE_` environment variables using `__` for nesting, then typed CLI overrides such as `--http-bind`. TOML rejects unknown fields. The secret-bearing database URL must come from `VESTRACE_DATABASE__URL` or another secret-management environment, never TOML or a CLI argument.

Logging defaults to text at `info`. Set `VESTRACE_OBSERVABILITY__FORMAT=json` for structured JSON or change `VESTRACE_OBSERVABILITY__LOG_FILTER` for application verbosity. Dependency SQL and connection details remain filtered even at verbose levels.

## Verification

The project pins Rust 1.85.0. With a pgvector-enabled PostgreSQL 17 database available through `DATABASE_URL`, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
docker compose config --quiet
```

## Documentation

Project references are under `docs/`:

- [Documentation Index and Status](docs/README.md)
- [Implementation Extensions](docs/implementation/README.md)
- [Memory Workspace Implementation Package](docs/implementation/memory-workspace/README.md)
- [Memory Workspace Preflight](docs/implementation/memory-workspace/plans/00-preflight.md)

- [Architecture Guide](docs/architecture.md)
- [Domain Model Reference](docs/domain-model.md)
- [Database Schema & Migrations](docs/database-schema.md)
- [Security & RLS Architecture](docs/security-and-rls.md)
- [Getting Started Guide](docs/getting-started.md)
