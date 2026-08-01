# Quickstart & Local Development Guide

## Prerequisites

- Rust 1.85.0+
- Docker Engine with Docker Compose v2
- PostgreSQL 17 with `pgvector` extension (for non-Docker local dev)

## 1. Running with Docker Compose

Start PostgreSQL and the Vestrace HTTP server:

```bash
docker compose -p vestrace up --build -d
```

Verify service health and database RLS policies:

```bash
./scripts/foundation-smoke.sh
./scripts/foundation-runtime-rls.sh
```

## 2. CLI Usage

The `vestrace` binary provides subcommands for server administration and background processing:

```bash
# Run HTTP server
cargo run --bin vestrace -- server

# Run background extraction worker
cargo run --bin vestrace -- worker

# Run database migration check
cargo run --bin vestrace -- migrate

# Run system diagnostic checks
cargo run --bin vestrace -- doctor
```

## 3. Running the Test Suite

Run unit and integration tests across all workspace crates:

```bash
DATABASE_URL=postgres://vestrace:vestrace@localhost:5432/vestrace_test cargo test --workspace --all-features
```
