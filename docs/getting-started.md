# Quickstart & Local Development Guide

## Documentation status

This guide describes how to run/test the **current implementation snapshot**. It does not imply that every target v0.2 architecture capability is implemented.

Target architecture starts at [`specs/vestrace-architecture-contract-v0.2.md`](specs/vestrace-architecture-contract-v0.2.md). Current implementation boundaries are summarized in [`current-implementation.md`](current-implementation.md).

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

Verify health, the Run API, workspace isolation, and runtime RLS using the repository's smoke scripts:

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

## 2. Current CLI process modes

The database URL must be supplied through `VESTRACE_DATABASE__URL` or an equivalent secret environment.

### HTTP server

```bash
VESTRACE_DATABASE__URL='postgres://...' \
  cargo run --bin vestrace -- server
```

### Apply/verify migrations

```bash
VESTRACE_DATABASE__URL='postgres://...' \
  cargo run --bin vestrace -- migrate
```

The authoritative list of current commands is the source at the snapshot being run. Documentation must not infer availability from future target command examples.

## 3. `doctor`, `repair`, `rebuild`, `qualify`

The v0.2 target documentation defines **semantic contracts** for diagnostics, repair/rebuild and qualification, for example conceptually:

```text
vestrace doctor
vestrace doctor --plan
vestrace repair <plan>
vestrace qualify <profile>
```

These examples are target architecture, not a promise that those exact CLI commands are wired in the current snapshot.

Before using any of them operationally, check the actual CLI/source/acceptance status of the build being run.

## 4. Run the current test suite

With required dependencies available:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

These tests verify current implementation behavior. They are not equivalent to the future v0.2+ conformance/qualification profiles defined in [`specs/vestrace-qualification-conformance-spec-v0.2.md`](specs/vestrace-qualification-conformance-spec-v0.2.md).

## 5. Current product/runtime boundary

The inspected implementation snapshot provides a foundation including:

- liveness/readiness;
- PostgreSQL-backed event-sourced Run command state;
- deterministic replay/checkpoint recovery/projection rebuild;
- Memory creation/revision/provenance/relations;
- idempotency/outbox foundations;
- retrieval with PostgreSQL FTS, RRF, deterministic reranking and token-bounded ContextPack;
- policy/redaction/audit/security domain foundations;
- worker/job leasing infrastructure;
- memory-oriented MCP tools.

See [`current-implementation.md`](current-implementation.md) for the fuller boundary and known gaps.

## 6. What target docs do not make available today

The v0.2 target specs must not be interpreted as current runtime support for:

- full Claim/Evidence persistent cognition;
- complete temporal/reconciliation semantics;
- full capability attenuation and federation;
- findings-first repair engine;
- governed external side effects/reconciliation;
- incident/revalidation trust restoration;
- full crypto/data governance;
- qualification profiles.

Those capabilities remain future implementation work until executable implementation and conformance evidence exists.

## 7. Branching rule for future implementation

The v0.2 documentation baseline is already integrated into `main`. Future implementation work should use dedicated implementation branches derived from a reviewed current `main` baseline.

If implementation code has moved materially since the inspected snapshot, re-run the documented gap delta before treating the 36-PR plan as current without modification.
