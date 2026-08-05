# Vestrace P0 Baseline Inventory

**Baseline commit:** `531bf73b9e3850de7fb09949647f6da402d1a467`  
**Captured:** 2026-08-05  
**Purpose:** executable truthfulness baseline before P0 runtime changes

## Classification legend

- **implemented** — the surface performs the operation it claims to perform;
- **explicitly unavailable** — the surface fails clearly and non-zero, or returns a stable `501`;
- **false success** — the surface reports success without performing the claimed operation;
- **experimental** — compiled and tested code that is not part of the supported runtime contract.

## Workspace

| Package | Classification | Notes |
|---|---|---|
| `vestrace-domain` | implemented foundation | domain types and invariants |
| `vestrace-application` | implemented foundation | use cases and ports |
| `vestrace-infrastructure` | implemented foundation | PostgreSQL and provider adapters |
| `vestrace-http` | implemented P0 transport | health and run-record HTTP APIs |
| `vestrace-cli` | mixed | real server plus placeholder operational commands |
| `vestrace-rig-spike` | experimental | participates in workspace lint/tests but is not supported runtime functionality |
| root `vestrace-integration-tests` | implemented test harness | PostgreSQL and boundary tests |

## CLI commands

| Command | Classification | Baseline behavior |
|---|---|---|
| `server` | implemented | connects, migrates, serves HTTP |
| `worker` | **false success** | logs successful loop initialization and returns zero without starting a loop |
| `mcp` | explicitly unavailable | returns non-zero with an unimplemented error |
| `migrate` | **false success** | prints a hard-coded migration range, performs no database work, returns zero |
| `doctor` | explicitly unavailable | returns non-zero with an unimplemented error |
| `rebuild` | explicitly unavailable | returns non-zero with an unimplemented error |

## HTTP routes

### Implemented

- `GET /health/live`
- `GET /health/ready`
- `POST /v1/runs`
- `GET /v1/runs`
- `GET /v1/runs/{id}`

The run API persists run records only. It does not execute agents or workflows.

### Explicitly unavailable

The following surfaces return `501 Not Implemented` rather than fake success:

- run approval;
- artifacts;
- agents;
- workflows;
- triggers;
- connections;
- models;
- evaluations;
- audit;
- metrics summary;
- system health;
- profile;
- AG-UI execution.

## Database and migrations

- authoritative migration source: ordered files in `migrations/` embedded by SQLx;
- server startup applies embedded migrations;
- readiness compares applied rows with the embedded migrator by version, success state, and checksum;
- documentation must not maintain a hard-coded maximum migration number;
- runtime RLS uses transaction-scoped workspace and principal settings.

## CI baseline

The main CI definition includes:

- Rust formatting;
- Clippy across workspace, targets, and features;
- full Rust tests with PostgreSQL 17 + pgvector;
- console install, typecheck, and production build;
- Docker Compose configuration and startup;
- health smoke;
- run create/list/get and workspace-isolation smoke;
- runtime RLS acceptance.

Missing P0 gates at baseline:

- built CLI truthfulness acceptance;
- fresh standalone `vestrace migrate` acceptance;
- documentation false-claim regression check.

## Confirmed documentation drift

- `docs/architecture.md` says the workspace has five crates although Cargo lists six;
- `docs/architecture.md` contains an authoritative `0001–0016` migration range;
- the migration reference correctly says the directory and embedded migrator are authoritative.

## P0 implementation order

1. add failing CLI contract tests;
2. make `worker` explicitly unavailable;
3. implement real `migrate` behavior through shared infrastructure migration APIs;
4. consolidate compatibility logic;
5. synchronize documentation;
6. add CLI and documentation truthfulness CI;
7. run clean-database and full acceptance gates.
