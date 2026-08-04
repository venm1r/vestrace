# Vestrace P0 — Restore Main Implementation Specification v0.1

**Status:** Approved implementation specification  
**Priority:** First mandatory phase under the P0–P4 amendment  
**Target branch:** `main`  
**Baseline commit at specification time:** `531bf73b9e3850de7fb09949647f6da402d1a467`  
**Scope:** truthfulness, reproducibility, architectural boundaries, and release gates  

---

## 1. Purpose

P0 restores `main` to a state that is not only buildable, but operationally truthful.

A P0-complete repository must satisfy all of the following:

- every successful command performs the operation it claims to perform;
- every unsupported command or endpoint fails explicitly;
- documentation matches the actual workspace, migrations, APIs, and runtime behavior;
- the clean-database path is reproducible;
- health and readiness have narrow, testable meanings;
- HTTP remains a transport adapter and does not own persistence logic;
- console behavior never substitutes mock success for backend failure;
- CI proves the same contract that the documentation claims.

P0 does **not** add State Engine execution, checkpoints, replay, capabilities, CAS, agents, workflows, or new product surfaces.

---

## 2. Normative priority

The project implementation order is:

```text
P0 Restore Main
→ P1 State Engine Vertical
→ P2 Capability and Policy
→ P3 Artifact CAS
→ P4 Truthful Readiness
```

All R1 implementation pull requests remain draft and frozen while P0 is incomplete:

- PR #22 — R1.1 domain kernel;
- PR #23 — R1.2 event store foundation;
- PR #24 — R1.3 projection integration;
- PR #25 — R1.4 command HTTP integration.

P0 work must not cherry-pick or merge these branches merely to reduce the open-PR count.

They are P1 inputs and require a separate integration decision after P0 passes its exit gate.

---

## 3. Current verified baseline

### 3.1. Existing strengths

At the baseline commit, `main` already has:

- a Rust workspace pinned to Rust 1.85;
- six workspace crates plus the root integration-test package;
- PostgreSQL persistence and SQLx forward-only migrations;
- transaction-scoped workspace and principal context;
- explicit run-record create/list/get endpoints;
- explicit `501 Not Implemented` responses for unsupported HTTP surfaces;
- liveness and database/migration readiness checks;
- console typecheck and production-build gates;
- Docker Compose acceptance;
- runtime RLS acceptance;
- a README that distinguishes run records from actual agent execution.

These foundations are retained.

### 3.2. Confirmed P0 defects

#### P0-D1 — Worker command reports false success

`vestrace worker` currently logs that a memory-extraction worker and loop were initialized, then exits successfully.

No worker loop is started.

This is a critical truthfulness defect.

Required result:

```text
vestrace worker
→ non-zero exit
→ explicit "worker command is not implemented"
```

until a real worker is implemented in a later phase.

#### P0-D2 — Migrate command reports false success

`vestrace migrate` currently:

- does not connect to PostgreSQL;
- does not run embedded migrations;
- does not inspect `_sqlx_migrations`;
- prints a hard-coded claim that migrations `0001–0016` are current;
- exits successfully.

This is a critical truthfulness defect.

Required result:

`vestrace migrate` must either:

1. perform real migration work through the same embedded SQLx migrator used by the server; or
2. fail explicitly as unimplemented.

The preferred P0 behavior is real migration execution because the server already contains a working migration path.

No maximum migration number may be hard-coded in source or documentation.

#### P0-D3 — Architecture document has stale workspace inventory

The architecture document says the workspace has five crates.

The root workspace currently includes six crates:

```text
vestrace-domain
vestrace-application
vestrace-infrastructure
vestrace-http
vestrace-cli
vestrace-rig-spike
```

Required result:

- document all six crates;
- classify `vestrace-rig-spike` as experimental/non-production;
- state whether it participates in all workspace CI gates;
- avoid describing experimental code as runtime functionality.

#### P0-D4 — Architecture document has a stale migration range

The architecture document describes migrations as `0001–0016`.

The database-schema reference correctly states that migration files are discovered from the directory and that documentation must not maintain a maximum migration number.

Required result:

- remove all authoritative maximum migration numbers;
- refer to the ordered `migrations/` directory;
- preserve representative ranges only when explicitly labelled historical examples.

#### P0-D5 — CLI surface mixes real and placeholder commands

Current command status:

| Command | Actual state | P0 required behavior |
|---|---|---|
| `server` | implemented | keep |
| `worker` | false success | explicit failure |
| `mcp` | explicit failure | keep |
| `migrate` | false success | implement real migration execution |
| `doctor` | explicit failure | keep |
| `rebuild` | explicit failure | keep |

The CLI help may list unimplemented commands only when their invocation fails explicitly and predictably.

#### P0-D6 — No executable truthfulness contract for CLI commands

Current CI exercises the workspace, HTTP service, console, Compose, and RLS, but does not define command-level acceptance for placeholder CLI commands.

Required result:

Add tests that prove:

- `worker`, `mcp`, `doctor`, and `rebuild` exit non-zero;
- their stderr identifies the unavailable command;
- `migrate` performs a real migration/compatibility operation;
- no placeholder command returns exit code zero.

---

## 4. Explicit non-defects retained by P0

The following current behaviors are truthful and should not be changed merely for stylistic consistency.

### 4.1. Unsupported HTTP APIs

Unsupported APIs already return explicit `501 Not Implemented` errors.

This is acceptable P0 behavior.

P0 must not create fake empty successful responses for:

- approvals;
- artifacts;
- agents;
- workflows;
- triggers;
- connections;
- models;
- evaluations;
- audit;
- metrics;
- system health;
- profile;
- AG-UI execution.

### 4.2. Run record semantics

On baseline `main`, `POST /v1/runs` creates a run record.

It does not start an agent or State Engine execution.

This remains acceptable for P0 as long as:

- README and API documentation say exactly that;
- the response does not imply execution started;
- no execution status or generated output is fabricated.

Migration to canonical event-sourced command execution belongs to P1.

### 4.3. Readiness semantics

Current readiness verifies exact compatibility between applied and embedded SQLx migrations.

That is a valid P0 readiness definition for the current single-process foundation.

P0 may improve diagnostics, but must not broaden `/health/ready` to claim:

- worker readiness;
- State Engine readiness;
- provider readiness;
- agent readiness;
- overall product readiness.

Those dimensions are introduced under P4.

---

## 5. P0 architecture rules

### 5.1. Dependency direction

The allowed dependency direction remains:

```text
vestrace-cli → vestrace-http / vestrace-infrastructure / vestrace-application
vestrace-http → vestrace-application
vestrace-infrastructure → vestrace-application / vestrace-domain
vestrace-application → vestrace-domain
vestrace-domain → no project crate
```

P0 must verify that:

- HTTP does not import SQLx;
- HTTP handlers do not execute SQL;
- domain does not import infrastructure or transport types;
- application ports do not expose PostgreSQL types;
- server wiring constructs adapters at the composition root.

### 5.2. SQL ownership

All SQL remains inside infrastructure adapters and migrations.

Allowed exceptions:

- integration tests;
- operational shell acceptance scripts;
- migration SQL files.

### 5.3. Configuration and secrets

P0 retains these rules:

- database URLs are provided by environment/secret management;
- fixed Compose credentials are local-only;
- logs do not expose SQL statements or connection secrets;
- configuration rejects unknown TOML fields;
- request identity is never silently defaulted.

---

## 6. Implementation slices

## P0.1 — Baseline evidence

### Goal

Create a reproducible snapshot of what `main` actually passes before modifications.

### Work

- record baseline commit SHA;
- run or verify the complete CI matrix;
- record workspace members;
- record the embedded migration count dynamically;
- record public HTTP routes;
- record CLI commands and actual exit behavior;
- identify every successful placeholder path.

### Deliverable

```text
docs/superpowers/reports/<date>-p0-baseline-inventory.md
```

### Acceptance

The inventory must distinguish:

```text
implemented
explicitly unavailable
false success
experimental
```

No item may be classified solely from its name.

---

## P0.2 — CLI truthfulness

### Goal

Eliminate all successful no-op operational commands.

### Work

1. Change `worker` to fail explicitly.
2. Replace the fake `migrate` implementation with a real asynchronous migration command.
3. Keep `mcp`, `doctor`, and `rebuild` as explicit failures.
4. Ensure failures use stable messages and non-zero exits.
5. Add unit/integration tests for command outcomes.

### Migrate command contract

`vestrace migrate` must:

1. load configuration through the existing typed configuration path;
2. connect using the migration-capable database identity;
3. run the embedded SQLx migrator;
4. verify exact migration compatibility after execution;
5. return zero only when both migration and verification succeed;
6. return a sanitized error on connection, migration, or compatibility failure;
7. never print a hard-coded migration range.

The command should be asynchronous:

```rust
pub async fn run(config: &AppConfig) -> anyhow::Result<()>
```

and the CLI dispatch must await it.

### Tests

Required tests:

- worker fails;
- MCP fails;
- doctor fails;
- rebuild fails;
- migrate fails against an unavailable database;
- migrate succeeds against a clean supported database;
- migrate is idempotent on an already-current database;
- migrate rejects incompatible migration history;
- logs do not include database secrets.

---

## P0.3 — Migration reproducibility

### Goal

Prove that a clean database and an existing compatible database produce the same ready state.

### Work

- use one embedded migrator definition or a shared accessor;
- avoid duplicated migrator paths and compatibility algorithms;
- verify fresh migration from an empty PostgreSQL database;
- verify restart with an already-migrated database;
- verify failure for missing migration rows;
- verify failure for extra migration rows;
- verify failure for modified checksum;
- verify failure for failed migration state;
- verify runtime role cannot mutate migration metadata.

### Clean-database acceptance

```text
empty database
→ vestrace migrate
→ server start
→ /health/ready = 200
→ run create/list/get smoke passes
→ runtime RLS smoke passes
```

### Existing-database acceptance

```text
compatible database
→ vestrace migrate
→ no schema drift
→ server start
→ /health/ready = 200
```

---

## P0.4 — Documentation truth sync

### Goal

Make README and architecture references match the executable system.

### Work

Update at least:

- `README.md`;
- `docs/architecture.md`;
- `docs/database-schema.md` when necessary;
- `docs/getting-started.md`;
- CLI help/command descriptions;
- any feature inventory or roadmap links.

### Required corrections

- six workspace crates, not five;
- experimental classification of `vestrace-rig-spike`;
- no hard-coded maximum migration number;
- truthful command status;
- explicit distinction between run records and run execution;
- explicit distinction between database readiness and product readiness;
- explicit P0–P4 priority link.

### Documentation test

Add a lightweight CI check that fails when known false claims reappear.

At minimum, reject authoritative documentation patterns equivalent to:

```text
migrations (0001 - 0016)
worker loop initialized successfully
```

A more structured feature inventory belongs to P4.

---

## P0.5 — HTTP and application boundary audit

### Goal

Prove that the current P0 HTTP implementation remains a transport adapter.

### Work

- audit every HTTP handler;
- verify no direct SQL or PgPool/PgConnection dependency;
- verify identity extraction remains centralized;
- verify errors map through typed API errors;
- verify unsupported surfaces return `501`;
- verify create/list/get delegate through application use cases;
- verify server wiring remains at the CLI composition root.

### Tests

- compile-time dependency checks where practical;
- route contract tests;
- unsupported API response tests;
- missing identity tests;
- malformed identity tests;
- workspace-isolation tests;
- request/correlation ID propagation tests.

### P0 boundary

P0 does not replace `RunService` with the R1 command executor.

That transition is intentionally deferred to P1 so P0 does not mix truthfulness restoration with event-sourced behavior changes.

---

## P0.6 — Console truth audit

### Goal

Ensure the console exposes only the real P0 run-record behavior.

### Work

- inventory visible actions and navigation;
- remove or disable actions backed only by unsupported endpoints;
- preserve visible backend errors;
- verify no mock success fallback;
- identify the UI as a P0 foundation console;
- avoid labels implying agent execution.

### Tests

- missing identity is visible;
- backend `400`, `404`, `409`, `500`, `501`, and `503` are not rendered as success;
- unsupported features are disabled or clearly marked unavailable;
- create run text describes record creation rather than execution;
- production build contains no mock API fallback enabled by default.

---

## P0.7 — CI and acceptance gate

### Goal

Make P0 truthfulness part of the mandatory merge gate.

### Required CI jobs

#### Rust lint

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

#### Rust tests

```bash
cargo test --workspace --all-targets --all-features --no-fail-fast
```

#### CLI truthfulness

Run the built CLI and assert:

```text
worker  → non-zero
mcp     → non-zero
doctor  → non-zero
rebuild → non-zero
migrate → real database behavior
```

#### Console

```bash
npm --prefix apps/console ci
npm --prefix apps/console run typecheck
npm --prefix apps/console run build
```

#### Compose

```bash
docker compose config --quiet
docker compose up --build --detach --wait
./scripts/foundation-smoke.sh
bash ./scripts/foundation-run-smoke.sh
./scripts/foundation-runtime-rls.sh
```

#### Fresh migration

Use a database/volume not reused by any prior job.

#### Documentation truth

Run the P0 documentation check.

---

## 7. Issue breakdown

The implementation order is strict.

### P0-001 — Capture baseline inventory

**Depends on:** none  
**Produces:** verified inventory report  

Acceptance:

- records workspace, routes, commands, migrations, and CI;
- labels false success separately from unimplemented;
- contains baseline SHA.

### P0-002 — Add CLI command contract tests

**Depends on:** P0-001  
**Purpose:** RED tests for the current worker/migrate false-success behavior.

Acceptance:

- tests fail before production changes;
- failures identify worker and migrate semantics.

### P0-003 — Make worker explicitly unavailable

**Depends on:** P0-002

Acceptance:

- non-zero exit;
- stable sanitized message;
- no success log;
- test green.

### P0-004 — Implement real migrate command

**Depends on:** P0-002

Acceptance:

- connects and executes embedded migrations;
- verifies compatibility;
- clean DB succeeds;
- current DB succeeds idempotently;
- unavailable/incompatible DB fails;
- no migration range hard-coded.

### P0-005 — Consolidate migrator and compatibility logic

**Depends on:** P0-004

Acceptance:

- server readiness and CLI migrate use the same migration source of truth;
- no duplicated migration directory/path assumptions;
- tests cover missing/extra/failed/checksum mismatch.

### P0-006 — Add CLI truthfulness CI job

**Depends on:** P0-003, P0-004

Acceptance:

- successful no-op commands fail CI;
- migrate is exercised with PostgreSQL.

### P0-007 — Correct architecture and workspace documentation

**Depends on:** P0-001

Acceptance:

- six crates documented;
- rig spike marked experimental;
- stale migration range removed;
- current CLI status documented.

### P0-008 — Add documentation truth check

**Depends on:** P0-007

Acceptance:

- known false patterns fail CI;
- check is deterministic and lightweight.

### P0-009 — Audit HTTP/application boundary

**Depends on:** P0-001

Acceptance:

- no SQLx in HTTP;
- no direct SQL in handlers;
- unsupported endpoints remain explicit `501`;
- route contract tests green.

### P0-010 — Audit console truthfulness

**Depends on:** P0-001

Acceptance:

- no mock-success fallback;
- unsupported actions unavailable;
- run creation labelled accurately;
- typecheck/build green.

### P0-011 — Prove clean-database reproducibility

**Depends on:** P0-004, P0-005, P0-006

Acceptance:

- fresh volume migrates;
- server becomes ready;
- run smoke and RLS smoke pass;
- second startup is idempotent.

### P0-012 — Publish P0 completion report

**Depends on:** P0-007 through P0-011

Acceptance:

- maps every exit-gate item to evidence;
- records final commit and CI run;
- lists deferred P1 work;
- contains no unverified success claim.

---

## 8. Branch and pull-request strategy

P0 implementation should use small reviewable pull requests from `main`.

Recommended sequence:

```text
P0-PR1  baseline + RED CLI contract tests
P0-PR2  worker truthfulness + real migrate
P0-PR3  migration compatibility consolidation
P0-PR4  documentation + architecture sync
P0-PR5  HTTP/console audits + acceptance gates
P0-PR6  completion report
```

Rules:

- every PR starts from current `main` or the immediately preceding P0 branch;
- every PR stays draft until its own checks are green;
- no R1 branch is used as the base;
- no R1 code is merged into P0 PRs;
- no automatic merge;
- documentation PR #26 may land independently because it changes no runtime code.

---

## 9. P0 exit gate

P0 is complete only when all twenty conditions are true.

1. `main` builds with Rust 1.85.
2. Rust formatting passes.
3. Clippy passes with warnings denied.
4. Full Rust tests pass.
5. Console install, typecheck, and build pass.
6. Docker Compose configuration validates.
7. Compose starts from a clean volume.
8. Embedded migrations apply successfully to an empty database.
9. Reapplying migrations is idempotent.
10. Readiness rejects missing, extra, failed, or modified migrations.
11. `worker` fails explicitly.
12. `mcp`, `doctor`, and `rebuild` fail explicitly.
13. `migrate` performs real work and never reports a fake range.
14. No placeholder CLI command exits successfully.
15. HTTP handlers contain no direct SQL or SQLx dependency.
16. Unsupported APIs return explicit `501` responses.
17. Run-record API and documentation do not imply agent execution.
18. Console never substitutes mock success for backend failure.
19. Architecture and database documentation match the executable repository.
20. A completion report links every claim to test or CI evidence.

A green generic CI badge alone is insufficient if any truthfulness condition is unmet.

---

## 10. P0 completion output

The final completion report must contain:

```text
baseline SHA
final SHA
merged P0 PRs
CI run IDs
workspace inventory
migration verification evidence
CLI command matrix
HTTP route matrix
console surface matrix
known deferred items
P1 entry decision
```

The report must explicitly say that P0 does not implement:

- State Engine execution;
- event replay or checkpoint recovery;
- delegated subagents;
- compensation;
- capabilities and policy;
- Artifact CAS;
- production-wide readiness.

---

## 11. Transition to P1

After the P0 exit gate passes:

1. review PR #22 as the first P1 candidate;
2. rebase or reconstruct it on the final P0 `main`;
3. integrate R1.1–R1.3 in reviewable order;
4. prioritize checkpoints, replay, and rebuild before persisted idempotency;
5. treat PR #25 as provisional because its scope predates the P0–P4 amendment;
6. implement R1.4B only when the P1 command boundary is stable.

P0 completion authorizes P1 planning, not automatic merging of the existing R1 stack.
