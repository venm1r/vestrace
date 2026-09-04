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

The `installation-fingerprint-vault` named volume must survive restarts and upgrades because it holds the create-only installation fingerprint record.

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

For a fresh install or upgrade outside Compose, an administrator must first run
`docker/postgres/init-runtime-role.sh` against the target database with
`POSTGRES_DB`, `POSTGRES_USER`, and `VESTRACE_RUNTIME_PASSWORD` set. Only after
that idempotent provisioning step succeeds should the restricted `vestrace`
runtime URL run migrations. This ordering installs the exact P02/P03 ownership
bridges on existing volumes without giving the migration, server, worker, or
MCP processes the bootstrap credential.

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

## 5a. Governed provider execution

An agent step reaches a provider through one governed path. Nothing about that
path is chosen by process configuration: the route comes from the Run's pinned
`ModelBindingSnapshot`, the input is durable encrypted material, and both the
server and the worker compose the same authorities from one shared bundle.

### Two storage roots, deliberately separate

The runtime needs two directories, and they must not contain one another:

| Setting | Mount | What it holds |
| --- | --- | --- |
| `provider_execution.material_vault_root` | writable | this installation's wrapped material keys |
| `provider_execution.bootstrap_secret_root` | **read-only** | the mounted key that unwraps them |

Startup canonicalises both, refuses overlap, and probes the vault root for
writability. The bootstrap mount is never written to. Neither Compose nor the
application generates a bootstrap key: a deployment that has not placed one
does not start.

### Naming the bootstrap key

```
VESTRACE_PROVIDER_EXECUTION__BOOTSTRAP_KEY_ID: material-vault-bootstrap
VESTRACE_PROVIDER_EXECUTION__BOOTSTRAP_KEY_VERSION: v1
```

Only the identity is configured. The mount declares the key's scope, purpose
and algorithm, and the reference is read back from that declaration, so a
configuration cannot present one key as another. Rotating the key is a mount
and configuration change, not a code change.

### Policy is required, not defaulted

Governed execution refuses to start without a declared disclosure boundary:
`policy.data.mode`, `policy.data.classification`,
`policy.data.maximum_sensitivity` and `policy.data.allowed_destinations` are all
mandatory. There is no default, because a default here would be this software
deciding what a deployment may send to a provider.

Authorization is separate and equally required: the capability policy engine
answers the exact effect tuple before a dispatch is admitted, and a principal
holding no grant is denied.

### The order things must exist in

A Run step cannot execute until the objects it names exist. They are created
through explicit HTTP mutations, each one naming every immutable field and
carrying an `Idempotency-Key`:

1. `POST /v1/connections` — the stable Connection and its first immutable revision.
2. `POST /v1/models/{id}/revisions` — the Model revision bound to that Connection revision.
3. `POST /v1/connections/{id}/credentials` — first credential activation, if the
   revision's auth branch is not `no_auth`.
4. `POST /v1/connections/{id}/qualifications` — the q1 job that qualifies the
   Connection and both model revisions.
5. `POST /v1/runs` then `POST /v1/runs/{id}/steps` with an agent step carrying
   `input`.

A connection admission policy must also exist for the Connection before a
dispatch can be admitted; no HTTP route publishes one yet, so today it is
seeded directly.

### Fail-closed startup

Both long-running roots refuse rather than degrade. In order, each proves the
database is reachable, then the two storage roots, then the migration history,
and only afterwards composes the vault, the policy engine and the declared
boundary. The database comes first on purpose: a worker pointed at a database
that is not there should say so rather than send an operator to the filesystem.
A missing authority is a refusal to start, never a process that runs and
reports success having called nothing — which is exactly what the earlier
configuration-routed executor did.

### A local provider endpoint

An OpenAI-compatible endpoint on loopback may be configured explicitly through
the Connection revision's `runtime_base_url`. The transport policy is not a
free choice: the domain classifies an `http://` URL whose host is `localhost`
or a loopback address as `loopback_only`, an `https://` URL as `remote_https`,
and refuses anything else — plain HTTP to a non-loopback host is not
expressible. Nothing in this repository's test suites calls such an endpoint,
and no qualification evidence in this package was produced by one.

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
