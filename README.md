# Vestrace

Vestrace is an evidence-first knowledge and execution platform. This repository currently contains the foundation: a Rust workspace, PostgreSQL identity schema and row-level isolation, scoped transactions, and HTTP liveness/readiness endpoints. Product workflows and the Web UI are not implemented in this slice.

## Local container environment

Prerequisites: Docker Engine with Docker Compose v2, plus `curl` and Bash for the smoke checks. The Compose file uses distinct fixed credentials named `bootstrap-local-development-only` and `runtime-local-development-only`; they are exclusively for an isolated developer machine and must never be reused for production or an externally reachable database.

Build and start both services:

```bash
docker compose -p vestrace-foundation up --build -d
./scripts/foundation-smoke.sh
```

The server listens inside the container on `0.0.0.0:8080`, but Compose publishes it only as `127.0.0.1:8080` by default. Set `VESTRACE_HTTP_PORT` before starting Compose to choose another loopback host port, then pass the matching base URL to the smoke script:

```bash
VESTRACE_HTTP_PORT=18080 docker compose -p vestrace-foundation up --build -d
./scripts/foundation-smoke.sh http://127.0.0.1:18080
```

Compose waits for PostgreSQL to report healthy before it starts the server container. Once started, the server connects to PostgreSQL, applies the embedded forward-only migrations, and only then begins serving. `/health/live` reports process liveness. `/health/ready` also checks database access and exact migration compatibility. The smoke script retries for a bounded startup window and requires HTTP 200 with the exact safe JSON body from both endpoints.

The PostgreSQL image bootstraps with the local-only `vestrace_bootstrap` administrator, then provisions a separate `vestrace` login for the server. The server receives only the restricted runtime URL. That runtime role owns the local development database and schema so it can apply migrations, but it is explicitly `NOSUPERUSER`, `NOBYPASSRLS`, and has no membership in the bootstrap role. Verify both the HTTP endpoints and real runtime row-level isolation with:

```bash
./scripts/foundation-smoke.sh
./scripts/foundation-runtime-rls.sh
```

Production deployments must supply managed secrets and independently provisioned least-privilege bootstrap/migration and runtime identities; the fixed Compose credentials are not a production template.

Stop the services while keeping database data:

```bash
docker compose -p vestrace-foundation down --remove-orphans
```

To also remove the Compose-owned development database volume:

```bash
docker compose -p vestrace-foundation down -v --remove-orphans
```

## Configuration

Configuration precedence is: built-in defaults, an optional non-secret TOML file selected with `--config`, `VESTRACE_` environment variables using `__` for nesting, then typed CLI overrides such as `--http-bind`. TOML accepts only non-secret settings and rejects unknown fields. The database URL is secret-bearing and must come from `VESTRACE_DATABASE__URL` or a secret-management environment, never from TOML or a CLI argument. See `.env.example` for variable names; do not commit populated `.env` files.

Logging defaults to text at `info`. Set `VESTRACE_OBSERVABILITY__FORMAT=json` for structured JSON or change `VESTRACE_OBSERVABILITY__LOG_FILTER` for application verbosity. Dependency SQL and connection details remain filtered even at verbose levels.

## Rust verification

The project pins Rust 1.85.0. With a pgvector-enabled PostgreSQL 17 database available through `DATABASE_URL`, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```
