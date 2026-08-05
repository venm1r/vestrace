# Quickstart & Local Development Guide

## Prerequisites

- Rust 1.85.0+
- Docker Engine with Docker Compose v2
- PostgreSQL 17 with `pgvector` for non-Docker development
- Bash, `curl`, and Python 3 for smoke checks

## 1. Run with Docker Compose

Start PostgreSQL and the Vestrace HTTP server:

```bash
docker compose -p vestrace up --build -d
```

Verify health, the run-record API, workspace isolation, and runtime RLS:

```bash
./scripts/foundation-smoke.sh
bash ./scripts/foundation-run-smoke.sh
./scripts/foundation-runtime-rls.sh
```

Stop the environment:

```bash
docker compose -p vestrace down --remove-orphans
```

Add `--volumes` to remove the development database as well.

## 2. Supported CLI commands

The database URL must be supplied through `VESTRACE_DATABASE__URL` or an equivalent secret environment.

### Start the HTTP server

```bash
VESTRACE_DATABASE__URL='postgres://...' \
  cargo run --bin vestrace -- server
```

The server connects to PostgreSQL, applies the embedded SQLx migrations, verifies the complete migration history, and then begins serving.

### Apply and verify migrations

```bash
VESTRACE_DATABASE__URL='postgres://...' \
  cargo run --bin vestrace -- migrate
```

The command performs real database work. It exits successfully only after embedded migrations have been applied and the applied migration records exactly match the embedded set.

## 3. Explicitly unavailable CLI commands

The following commands are reserved but not implemented in P0:

```text
worker
mcp
doctor
rebuild
```

They exit non-zero with an explicit `command is not implemented` error. They must not be used as operational health checks or background processes.

## 4. Run the test suite

With PostgreSQL available through `DATABASE_URL`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

## 5. Current product boundary

P0 provides health endpoints and PostgreSQL-backed create/list/get run records. Creating a run does not start an agent or workflow. Unsupported REST and AG-UI surfaces return `501 Not Implemented`.
