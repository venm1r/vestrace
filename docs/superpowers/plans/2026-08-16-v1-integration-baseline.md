# V1 Integration Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the approved dirty `main` checkout into a reproducible source baseline whose ordinary formatting, lint, compile, focused evidence, console, workspace, PostgreSQL, Docker, and conformance gates have fresh truthful results.

**Architecture:** Preserve the approved implementation as one explicit-path checkpoint, then layer behavior-neutral formatting and lint normalization on top. Verify from cheap local gates toward isolated runtime gates, record every result in one baseline report, and keep the four known TRUSTED skips open for later separately designed slices.

**Tech Stack:** Rust 1.85/Cargo, Clippy, PostgreSQL 17 with pgvector, Docker Compose, Bash verification scripts, React/TypeScript/Vite, npm, PowerShell, Git.

## Global Constraints

- Work in `E:\Soft\vestrace` on the existing `main` checkout. Do not create a worktree.
- Treat design commit `7f6a49a39d7af42a42eb6b52e1d9b00195c7584e` and the 588-path dirty tree observed on 2026-08-16 as the approved source starting point; the later plan commits are documentation-only control artifacts.
- Preserve the pre-existing staged rename `apps/console/nginx.conf -> apps/console/nginx.conf.template`; never include it in a commit in this plan.
- Do not reset, restore, rebase, stash, delete existing work, run `git add -A`, or use an implicit commit scope.
- Leave `apps/console/node_modules/`, `apps/console/dist/`, `target/`, and `graphify-out/` untouched and uncommitted.
- This stabilization slice must not implement `IDW-010`, `IDW-014`, `QUAL-010`, `REC-016`, a production evidence probe, or production crypto custody.
- No product behavior change is designed here. Every selected code edit is mechanical. If any diagnostic requires behavior to change, stop that edit, add a focused failing test first, and amend this plan before implementation.
- A gate blocked by Docker, PostgreSQL, ports, credentials, or sandbox policy remains explicitly open; another test may not stand in for it.
- Run Git commands with `git -c safe.directory=E:/Soft/vestrace -C E:/Soft/vestrace`.
- Before every commit, inspect `git diff --cached --name-status` and use `git commit --only` with the same explicit paths used for staging.

---

### Task 1: Checkpoint the approved implementation baseline

**Files:**

- Create: `docs/superpowers/reports/2026-08-16-v1-integration-baseline.md`
- Checkpoint: `Cargo.toml`
- Checkpoint: `Cargo.lock`
- Checkpoint: `Dockerfile`
- Checkpoint: `docker-compose.yml`
- Checkpoint: `docker/postgres/init-runtime-role.sh`
- Checkpoint: `docker/postgres/20-dev-seed.sql`
- Checkpoint: `apps/console/Dockerfile`
- Checkpoint: `apps/console/src/`
- Checkpoint: `crates/`
- Checkpoint: `migrations/`
- Checkpoint: `tests/`
- Checkpoint: `docs/`
- Exclude: `apps/console/nginx.conf`
- Exclude: `apps/console/nginx.conf.template`
- Exclude: `apps/console/node_modules/`
- Exclude: `apps/console/dist/`

- [ ] **Step 1: Capture the exact starting state**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo rev-parse HEAD
git -c safe.directory=E:/Soft/vestrace -C $repo status --short --branch
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo status --porcelain=v1 | Measure-Object
```

Expected: HEAD contains this implementation plan, `main` is ahead 28 and behind 3, the staged diff contains only the nginx rename, and the dirty count is 588 unless the operator has intentionally changed the tree after approval. Stop and reconcile any unexpected drift before staging.

- [ ] **Step 2: Create the verification report**

Create `docs/superpowers/reports/2026-08-16-v1-integration-baseline.md` with these sections and current facts:

```markdown
# V1 Integration Baseline Report

**Design:** `docs/superpowers/specs/2026-08-16-v1-integration-baseline-design.md`
**Plan:** `docs/superpowers/plans/2026-08-16-v1-integration-baseline.md`
**Approved source point:** design commit `7f6a49a39d7af42a42eb6b52e1d9b00195c7584e`
**Execution-start HEAD:** the commit captured by Task 1 Step 1, containing this plan
**Starting branch:** `main`, ahead 28 and behind 3 relative to `origin/main`

## Baseline inventory

- Approved dirty paths at plan time: 588.
- Pre-existing staged path: `R100 apps/console/nginx.conf apps/console/nginx.conf.template`.
- Generated/cache exclusions: `apps/console/node_modules`, `apps/console/dist`, `target`, `graphify-out`.

## Starting gate evidence

- `cargo fmt --all -- --check`: FAIL; formatting drift exists.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: FAIL; domain lint errors and workspace warnings exist.
- `cargo check --workspace --all-targets --all-features`: PASS with warnings.
- `cargo test --test v1_release_evidence -- --nocapture`: PASS, 4 tests.
- `cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture`: PASS, 3 tests.
- `npm run typecheck`: PASS.
- TRUSTED conformance: expected-open result, 199 total / 195 passed / 0 failed / 4 skipped / 0 not-applicable.
- Fresh PostgreSQL and Docker acceptance: not yet run in this slice.

## Final gate evidence

Results are appended task-by-task with command, UTC timestamp, exit status, counts, and environment.

## Remaining v1 gates and non-claims

- Open: `IDW-010`, `IDW-014`, `QUAL-010`, `REC-016`.
- This report does not claim TRUSTED, v1.0, production evidence collection, production crypto custody, or exact-environment qualification.
```

- [ ] **Step 3: Stage only approved source categories**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo add -- Cargo.toml Cargo.lock Dockerfile docker-compose.yml docker/postgres/init-runtime-role.sh docker/postgres/20-dev-seed.sql apps/console/Dockerfile apps/console/src crates migrations tests docs
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
```

Expected: the staged list contains the pre-existing nginx rename plus only source, tests, migrations, and documentation named above. It contains no `node_modules`, `dist`, `target`, or `graphify-out` path.

- [ ] **Step 4: Commit the approved baseline without the nginx rename**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "chore: checkpoint v1 integration baseline" -- Cargo.toml Cargo.lock Dockerfile docker-compose.yml docker/postgres/init-runtime-role.sh docker/postgres/20-dev-seed.sql apps/console/Dockerfile apps/console/src crates migrations tests docs
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
```

Expected: the commit succeeds and the cached diff again contains only the nginx rename.

---

### Task 2: Normalize Rust formatting without changing behavior

**Files:**

- Modify mechanically: Rust files under `crates/`
- Modify mechanically: Rust files under `tests/`
- Preserve: all non-Rust paths

- [ ] **Step 1: Record the pre-format Rust diff and run the formatter**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo diff --name-only -- '*.rs'
cargo fmt --all
git -c safe.directory=E:/Soft/vestrace -C $repo diff --name-only
```

Expected: every newly modified path ends in `.rs` and is under `crates/` or `tests/`. Stop if formatting touches another category.

- [ ] **Step 2: Verify formatting and compilation**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
```

Expected: both commands exit 0. Compiler warnings are allowed only until Tasks 3-5.

- [ ] **Step 3: Review and commit only formatter output**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo diff --check -- crates tests
git -c safe.directory=E:/Soft/vestrace -C $repo diff --stat -- crates tests
git -c safe.directory=E:/Soft/vestrace -C $repo add -- crates tests
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "style: normalize Rust formatting" -- crates tests
```

Expected: only Rust formatting is committed; the nginx rename remains staged afterward.

---

### Task 3: Resolve domain Clippy diagnostics mechanically

**Files:**

- Modify: `crates/vestrace-domain/src/conformance/cases.rs`
- Modify: `crates/vestrace-domain/src/conformance/runner.rs`
- Modify: `crates/vestrace-domain/src/run/checkpoint.rs`
- Modify: `crates/vestrace-domain/src/claim/assessment.rs`
- Modify: `crates/vestrace-domain/src/claim/evidence.rs`
- Modify: `crates/vestrace-domain/src/claim/mutation.rs`
- Modify: `crates/vestrace-domain/src/claim/supersession.rs`
- Modify: `crates/vestrace-domain/src/claim/mod.rs`
- Modify: `crates/vestrace-domain/src/enterprise/federation.rs`

- [ ] **Step 1: Reproduce the domain failure**

Run:

```powershell
cargo clippy -p vestrace-domain --all-targets --all-features -- -D warnings
```

Expected: non-zero with the known unused binding, irrefutable pattern, argument-count, module-name, nested-format, and test-module-order diagnostics. If a new diagnostic appears, add it to the report before editing.

- [ ] **Step 2: Apply the behavior-neutral fixes**

Make these exact changes:

- In `conformance/cases.rs`, rename the shadowed initial `plan` binding to `_plan`.
- In `run/checkpoint.rs`, replace the irrefutable `if let` with a direct `let RunCheckpointPayload::V1(v1) = &mut payload;` destructure.
- In `conformance/cases.rs`, remove nested `format!` and pass `first.version` and `first.status` directly to the outer format string.
- In `conformance/runner.rs`, move `group_by_family` above the `#[cfg(test)] mod tests` block without altering the function.
- Add narrowly scoped `#[allow(clippy::too_many_arguments)]` to the explicit domain constructors `ClaimAssessment::new`, `ClaimEvidenceLink::new`, `CognitiveMutation::new`, `SupersessionLink::for_memory_revision`, and `evaluate_federated_memory_access`.
- Add narrowly scoped `#[allow(clippy::module_inception)]` immediately above `mod claim;`; retain the public domain type name and module API.

Do not introduce parameter bags, change signatures, reorder validation, or alter returned values in this stabilization slice.

- [ ] **Step 3: Verify the domain layer**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy -p vestrace-domain --all-targets --all-features -- -D warnings
cargo test -p vestrace-domain --all-targets --all-features -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
```

Expected: all four commands exit 0; the registry suite reports 3 passed.

- [ ] **Step 4: Review and commit the domain lint normalization**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo diff --check -- crates/vestrace-domain
git -c safe.directory=E:/Soft/vestrace -C $repo add -- crates/vestrace-domain/src/conformance/cases.rs crates/vestrace-domain/src/conformance/runner.rs crates/vestrace-domain/src/run/checkpoint.rs crates/vestrace-domain/src/claim/assessment.rs crates/vestrace-domain/src/claim/evidence.rs crates/vestrace-domain/src/claim/mutation.rs crates/vestrace-domain/src/claim/supersession.rs crates/vestrace-domain/src/claim/mod.rs crates/vestrace-domain/src/enterprise/federation.rs
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "chore(domain): resolve baseline lint diagnostics" -- crates/vestrace-domain/src/conformance/cases.rs crates/vestrace-domain/src/conformance/runner.rs crates/vestrace-domain/src/run/checkpoint.rs crates/vestrace-domain/src/claim/assessment.rs crates/vestrace-domain/src/claim/evidence.rs crates/vestrace-domain/src/claim/mutation.rs crates/vestrace-domain/src/claim/supersession.rs crates/vestrace-domain/src/claim/mod.rs crates/vestrace-domain/src/enterprise/federation.rs
```

---

### Task 4: Resolve application warnings and dead test helpers

**Files:**

- Modify: `crates/vestrace-application/src/diagnostics.rs`
- Modify: `crates/vestrace-application/src/memory/services.rs`
- Modify: `crates/vestrace-application/tests/run_worker.rs`

- [ ] **Step 1: Reproduce application diagnostics**

Run:

```powershell
cargo clippy -p vestrace-application --all-targets --all-features -- -D warnings
```

Expected: non-zero for the unused `Arc` import, unread `provenance_repo` field, and unused `advance`, `set_snapshot`, `set_acquire_err`, and `set_outcome` test helpers.

- [ ] **Step 2: Apply the mechanical cleanup**

- Remove the unused `std::sync::Arc` import from `diagnostics.rs`.
- Rename the stored field to `_provenance_repo` while preserving the existing `provenance_repo: P` constructor parameter and assigning `_provenance_repo: provenance_repo`.
- Delete only the four unreferenced helper methods from `tests/run_worker.rs`; remove imports made unused by those deletions.

- [ ] **Step 3: Verify the application layer**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy -p vestrace-application --all-targets --all-features -- -D warnings
cargo test -p vestrace-application --all-targets --all-features -- --nocapture
```

Expected: all commands exit 0.

- [ ] **Step 4: Review and commit**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo diff --check -- crates/vestrace-application
git -c safe.directory=E:/Soft/vestrace -C $repo add -- crates/vestrace-application/src/diagnostics.rs crates/vestrace-application/src/memory/services.rs crates/vestrace-application/tests/run_worker.rs
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "chore(application): remove baseline warnings" -- crates/vestrace-application/src/diagnostics.rs crates/vestrace-application/src/memory/services.rs crates/vestrace-application/tests/run_worker.rs
```

---

### Task 5: Resolve HTTP, infrastructure, CLI, and integration-test warnings

**Files:**

- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `crates/vestrace-http/src/api/runs.rs`
- Modify: `crates/vestrace-http/tests/run_routes.rs`
- Modify: `crates/vestrace-http/tests/router_contract.rs`
- Modify: `crates/vestrace-http/tests/health.rs`
- Modify: `crates/vestrace-http/tests/request_span.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run/store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_repository.rs`
- Modify: `crates/vestrace-cli/src/commands/mcp.rs`
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-cli/src/commands/rebuild.rs`
- Modify: `tests/http_command_contract.rs`

- [ ] **Step 1: Reproduce the remaining workspace diagnostics**

Run:

```powershell
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: non-zero only for the known unused imports, bindings, mock executors, and unread row field outside the domain and application layers. Record any new diagnostic before editing.

- [ ] **Step 2: Remove imports, bindings, and test doubles proven unused**

- Remove `RunCommandExecutor` and `SharedRunCommandExecutor` imports from `http/src/router.rs` when not referenced.
- Remove the unused `PrincipalId` and `RunActor` imports and the unconstructed `RecordingCommands` test double from `http/src/api/runs.rs`.
- Remove unused run-command imports from `http/tests/run_routes.rs` while retaining the used `FakeRunCommands` implementation.
- Delete unconstructed `UnusedRunCommands`, both unconstructed `StubRunCommandExecutor` definitions, and the unconstructed integration-test `Commands` type together with imports used only by them.
- Remove the unused `PgPool` import from `postgres/run/store.rs`.
- Keep `AgentRunRow.title` in the SQL row shape and add a narrowly scoped `#[allow(dead_code)]` with a comment that the field preserves the canonical lossless row projection.
- Rename the unused `pool` bindings in CLI MCP and server commands to `_pool` if construction side effects or ownership are required; otherwise remove the binding while preserving the call and error propagation.
- Remove the unused `sqlx::Row` import from the rebuild command.

- [ ] **Step 3: Verify each affected layer before the workspace gate**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy -p vestrace-http --all-targets --all-features -- -D warnings
cargo test -p vestrace-http --all-targets --all-features -- --nocapture
cargo clippy -p vestrace-infrastructure --all-targets --all-features -- -D warnings
cargo test -p vestrace-infrastructure --all-targets --all-features --no-run
cargo clippy -p vestrace-cli --all-targets --all-features -- -D warnings
cargo test -p vestrace-cli --all-targets --all-features -- --nocapture
cargo test --test http_command_contract -- --nocapture
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: all commands exit 0. Infrastructure database tests are compiled here but executed against isolated PostgreSQL in Task 7.

- [ ] **Step 4: Review and commit**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo diff --check -- crates/vestrace-http crates/vestrace-infrastructure crates/vestrace-cli tests/http_command_contract.rs
git -c safe.directory=E:/Soft/vestrace -C $repo add -- crates/vestrace-http/src/router.rs crates/vestrace-http/src/api/runs.rs crates/vestrace-http/tests/run_routes.rs crates/vestrace-http/tests/router_contract.rs crates/vestrace-http/tests/health.rs crates/vestrace-http/tests/request_span.rs crates/vestrace-infrastructure/src/postgres/run/store.rs crates/vestrace-infrastructure/src/postgres/run_repository.rs crates/vestrace-cli/src/commands/mcp.rs crates/vestrace-cli/src/commands/server.rs crates/vestrace-cli/src/commands/rebuild.rs tests/http_command_contract.rs
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "chore: clear remaining baseline warnings" -- crates/vestrace-http/src/router.rs crates/vestrace-http/src/api/runs.rs crates/vestrace-http/tests/run_routes.rs crates/vestrace-http/tests/router_contract.rs crates/vestrace-http/tests/health.rs crates/vestrace-http/tests/request_span.rs crates/vestrace-infrastructure/src/postgres/run/store.rs crates/vestrace-infrastructure/src/postgres/run_repository.rs crates/vestrace-cli/src/commands/mcp.rs crates/vestrace-cli/src/commands/server.rs crates/vestrace-cli/src/commands/rebuild.rs tests/http_command_contract.rs
```

---

### Task 6: Run local CI gates and the console build without touching generated output

**Files:**

- Modify with evidence only: `docs/superpowers/reports/2026-08-16-v1-integration-baseline.md`
- Preserve byte-for-byte status: `apps/console/dist/`
- Preserve byte-for-byte status: `apps/console/node_modules/`

- [ ] **Step 1: Run the source and compile gates**

Run:

```powershell
cargo fmt --all -- --check
bash ./scripts/foundation-doc-truth.sh
bash ./scripts/foundation-boundary-truth.sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --no-run
cargo build -p vestrace-cli --bin vestrace
bash ./scripts/foundation-cli-truth.sh
```

Expected: every command exits 0. Append each result and timestamp to the report.

- [ ] **Step 2: Run focused release-evidence and catalogue tests**

Run:

```powershell
cargo test --test v1_release_evidence -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
cargo test -p vestrace-integration-tests --test v01_acceptance -- --nocapture
```

Expected: 4, 3, and the full v0.1 acceptance suite pass respectively.

- [ ] **Step 3: Snapshot generated-path status and build the console into validated temporary output**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
$console = Join-Path $repo 'apps\console'
$beforeGeneratedStatus = git -c safe.directory=E:/Soft/vestrace -C $repo status --short -- apps/console/dist apps/console/node_modules
$beforeGeneratedDiff = git -c safe.directory=E:/Soft/vestrace -C $repo diff --binary -- apps/console/dist apps/console/node_modules
$beforeGeneratedHash = ($beforeGeneratedDiff -join "`n") | git hash-object --stdin
$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempOutput = Join-Path $tempRoot ("vestrace-console-v1-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -LiteralPath $tempOutput | Out-Null
$resolvedOutput = (Resolve-Path -LiteralPath $tempOutput).Path
if (-not $resolvedOutput.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Disposable console output escaped the system temp directory.' }
Push-Location $console
try {
  npm run typecheck
  npm exec -- vite build --outDir $resolvedOutput --emptyOutDir
} finally {
  Pop-Location
}
$afterGeneratedStatus = git -c safe.directory=E:/Soft/vestrace -C $repo status --short -- apps/console/dist apps/console/node_modules
$afterGeneratedDiff = git -c safe.directory=E:/Soft/vestrace -C $repo diff --binary -- apps/console/dist apps/console/node_modules
$afterGeneratedHash = ($afterGeneratedDiff -join "`n") | git hash-object --stdin
if (($beforeGeneratedStatus -join "`n") -ne ($afterGeneratedStatus -join "`n") -or $beforeGeneratedHash -ne $afterGeneratedHash) { throw 'Console verification changed preserved generated paths.' }
```

Expected: typecheck and build exit 0, the output resolves beneath the system temp directory, and generated-path status is identical before and after. Leave the temporary output for the OS cleanup policy; do not delete an unvalidated path.

- [ ] **Step 4: Run TRUSTED conformance and assert the expected-open result**

Run:

```powershell
cargo run -q -p vestrace-cli -- conformance check trusted --json
```

Expected: process exit is non-zero because the gate remains open, while JSON reports exactly 199 total, 195 passed, 0 failed, 4 skipped, 0 not-applicable. The skip IDs must be exactly `IDW-010`, `IDW-014`, `QUAL-010`, and `REC-016`. Any other result fails this slice.

- [ ] **Step 5: Append local evidence and commit the report update**

Record command, timestamp, exit code, counts, and preserved generated-path result. Then run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo add -- docs/superpowers/reports/2026-08-16-v1-integration-baseline.md
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "docs: record v1 baseline local evidence" -- docs/superpowers/reports/2026-08-16-v1-integration-baseline.md
```

---

### Task 7: Run isolated PostgreSQL and Docker acceptance gates

**Files:**

- Modify with evidence only: `docs/superpowers/reports/2026-08-16-v1-integration-baseline.md`
- Do not modify source files unless a failing gate is traced to this slice and receives a separate RED/GREEN task.

- [ ] **Step 1: Start a uniquely named PostgreSQL 17 container**

Run with Docker permission:

```powershell
$pgName = "vestrace-v1-pg-$([guid]::NewGuid().ToString('N'))"
if (-not $pgName.StartsWith('vestrace-v1-pg-')) { throw 'Unsafe PostgreSQL container name.' }
docker run --detach --name $pgName --env POSTGRES_DB=vestrace_test --env POSTGRES_USER=vestrace --env POSTGRES_PASSWORD=vestrace --publish 127.0.0.1::5432 pgvector/pgvector:pg17
$pgPort = docker port $pgName 5432/tcp
$pgPortNumber = ($pgPort -split ':')[-1]
$databaseUrl = "postgres://vestrace:vestrace@127.0.0.1:$pgPortNumber/vestrace_test"
```

Poll `docker exec $pgName pg_isready -U vestrace -d vestrace_test` for at most 60 seconds, reporting progress at least once per minute.

- [ ] **Step 2: Run the database-backed workspace suite**

Run:

```powershell
$env:DATABASE_URL = $databaseUrl
cargo test --workspace --all-targets --all-features --no-fail-fast -- --nocapture
cargo build -p vestrace-cli --bin vestrace
$env:VESTRACE_DATABASE__URL = $databaseUrl
target/debug/vestrace migrate
target/debug/vestrace migrate
Remove-Item Env:VESTRACE_DATABASE__URL
Remove-Item Env:DATABASE_URL
```

Expected: the full suite and both idempotent migration runs exit 0. Append total counts and the image tag to the report.

- [ ] **Step 3: Remove only the validated isolated PostgreSQL container**

Run with Docker permission:

```powershell
if (-not $pgName.StartsWith('vestrace-v1-pg-')) { throw 'Refusing to remove an unvalidated container.' }
docker rm --force $pgName
```

Expected: exactly the uniquely named test container is removed.

- [ ] **Step 4: Run Compose acceptance under an isolated project name**

Run with Docker permission:

```powershell
$repo = 'E:\Soft\vestrace'
$env:COMPOSE_PROJECT_NAME = "vestrace-v1-baseline-$([guid]::NewGuid().ToString('N'))"
if (-not $env:COMPOSE_PROJECT_NAME.StartsWith('vestrace-v1-baseline-')) { throw 'Unsafe Compose project name.' }
Push-Location $repo
try {
  docker compose config --quiet
  docker compose up --build --detach --wait --wait-timeout 180
  bash ./scripts/foundation-smoke.sh
  bash ./scripts/foundation-run-smoke.sh
  bash ./scripts/foundation-runtime-rls.sh
  cargo test -p vestrace-integration-tests --test compose_smoke -- --ignored --nocapture
} finally {
  if ($env:COMPOSE_PROJECT_NAME.StartsWith('vestrace-v1-baseline-')) {
    docker compose down --volumes --remove-orphans
  }
  Pop-Location
  Remove-Item Env:COMPOSE_PROJECT_NAME
}
```

Expected: config, service health, both smoke scripts, runtime-role/RLS verification, and all ignored compose tests pass. Cleanup removes only the isolated project's containers, networks, and volumes.

- [ ] **Step 5: Record runtime evidence or exact environment blockers**

Append every fresh result to the report. If Docker or PostgreSQL cannot run, record the exact command, error, date, environment, and open gate; do not mark the task complete by inference.

- [ ] **Step 6: Commit the runtime evidence**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo add -- docs/superpowers/reports/2026-08-16-v1-integration-baseline.md
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo commit --only -m "docs: record v1 baseline runtime evidence" -- docs/superpowers/reports/2026-08-16-v1-integration-baseline.md
```

---

### Task 8: Perform the final baseline review

**Files:**

- Verify: `docs/superpowers/specs/2026-08-16-v1-integration-baseline-design.md`
- Verify: `docs/superpowers/plans/2026-08-16-v1-integration-baseline.md`
- Verify: `docs/superpowers/reports/2026-08-16-v1-integration-baseline.md`
- Verify: all commits created by Tasks 1-7

- [ ] **Step 1: Re-run the deterministic final gates**

Run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --no-run
cargo test --test v1_release_evidence -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
```

Expected: all exit 0, with 4 release-evidence tests and 3 registry tests passed.

- [ ] **Step 2: Inspect scope, staged state, and non-claims**

Run:

```powershell
$repo = 'E:\Soft\vestrace'
git -c safe.directory=E:/Soft/vestrace -C $repo log --oneline --decorate -10
git -c safe.directory=E:/Soft/vestrace -C $repo diff --check HEAD~7..HEAD
git -c safe.directory=E:/Soft/vestrace -C $repo diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace -C $repo status --short --branch
```

Expected: plan commits are atomic; the cached diff contains only the pre-existing nginx rename; preserved generated/cache changes remain uncommitted; the report does not claim TRUSTED or v1.0.

- [ ] **Step 3: Obtain independent code review before calling the slice complete**

Use `superpowers:requesting-code-review` against the design, plan, report, and exact commit range. Address only verified in-scope findings. If review asks for a behavior change, create a focused RED/GREEN amendment instead of folding it into mechanical cleanup.

- [ ] **Step 4: State the truthful outcome**

If every available gate passes and any environment-blocked gate is explicitly recorded, report: “V1 Integration Baseline complete; v1.0 remains open on `IDW-010`, `IDW-014`, `QUAL-010`, `REC-016`, production release evidence/crypto custody, and exact-environment qualification.” Do not state that Vestrace v1.0 is complete.
