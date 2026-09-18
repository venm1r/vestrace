# Getting started

**Scope:** A prepared, isolated local development environment. These runtime commands were not executed in this documentation refactor. Run them from a complete repository checkout, not a documentation-only archive.

## 1. Inspect the checkout

Use the versions declared in rust-toolchain.toml, Cargo.toml, Console package files, and Compose. This snapshot pins Rust 1.85.0 and declares Node 22 and PostgreSQL 17/pgvector. Do not update lockfiles as an implicit setup step.

```bash
# Read-only discovery: no services or credentials are created.
git rev-parse HEAD
cargo --version
node --version
docker compose version
docker compose config --quiet
```

A ZIP checkout has no `.git`; initialize or clone a working Git repository before using Git commands. Do not publish full expanded `docker compose config` output because it can contain credentials. Use `--quiet` for validation without printing values.

## 2. Prepare persistent resources and authority

Compose uses PostgreSQL data, an installation-fingerprint vault, a provider-material vault, and an external read-only bootstrap-secret volume. Neither the image nor Compose generates the bootstrap credential. An empty correctly named volume does not satisfy the prerequisite. Verify the mounted-store layout required by the selected build.

The provisioner establishes roles; the migration service applies migrations; development initialization creates local identities. Do not substitute a bootstrap administrator for the restricted runtime identity to bypass errors. Development passwords are not deployment credentials.

**Stop until the bootstrap layout and disclosure policy are prepared.** A verified clean-machine installer is [F004](roadmap/p0-foundation.md#f004), not an assumption of this guide. For valuable data, establish and test recovery before use.

## 3. Start the prepared environment

These commands change the local environment:

```bash
docker compose -p vestrace up --build --detach
curl --fail --silent --show-error http://127.0.0.1:8080/health/live
curl --fail --silent --show-error http://127.0.0.1:8080/health/ready
```

Keep the same Compose project name throughout. Liveness means process availability. Readiness is not qualification of a complete embedding or import workflow. On failure, use [troubleshooting](operations/troubleshooting.md), not volume deletion.

## 4. Authenticate

Health endpoints are bounded public probes. `/v1/*` requires a valid Bearer token and the relevant capabilities, issued through the installation's authorized procedure. `x-workspace-id` and `x-principal-id` do not provide client-controlled identity; middleware replaces them with token-derived values.

Use an authorized read or the [Memory walkthrough](guides/memory-api-exercise.md). Do not enable shell tracing around credentials or publish sensitive command history. No working credential is supplied in the documentation.

## 5. Stop without erasing data

```bash
# Keep named volumes.
docker compose -p vestrace down --remove-orphans
```

Do not routinely add `--volumes`. Removing a database/vault may make knowledge unrecoverable. Retained volumes are not a backup.

Read [Console](guides/console.md), [HTTP](reference/http.md), and [Memory](reference/memory.md). MW folder-import commands remain proposed, not current CLI instructions.

**Sources:** [Compose](../docker-compose.yml), [toolchain](../rust-toolchain.toml), [authentication](../crates/vestrace-http/src/auth.rs).
