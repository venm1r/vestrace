# IDW-014 Shared Read Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the smallest fail-closed cross-workspace shared-memory read path and prove, under a real restricted PostgreSQL login, that it reads only one permitted source revision without weakening normal workspace RLS.

**Architecture:** An application service validates the authenticated target identity and current grant/mount/policy with one trusted clock sample, then consumes an opaque application-issued permit through a dedicated reader port. The PostgreSQL adapter derives transaction-local source scope only from that permit, requires active RLS on its actual connection, and selects the exact workspace/memory/revision triple. Offline conformance remains a truthful `IDW-014` skip until a later DB-backed qualification evidence path exists.

**Tech Stack:** Rust 1.85 / edition 2024, `async-trait`, `static_assertions`, `serde`, SQLx 0.8/PostgreSQL 17, PostgreSQL RLS, Cargo integration tests, Docker, PowerShell, Git Bash.

## Global Constraints

- Start implementation from the final reviewed commit containing this plan; design commit `aa5739a` is the authority and exact diff base.
- Treat `docs/superpowers/specs/2026-08-16-v1-idw-014-shared-read-adapter-design.md` as the authority for this slice.
- Use `superpowers:subagent-driven-development`: a fresh implementer per task, then independent spec-compliance and code-quality reviews before the next task.
- Maintain the ignored ledger and task briefs under `.superpowers/sdd/2026-08-16-v1-idw-014-shared-read-adapter/`; do not commit them.
- Do not add migrations or treat migration `0087` `cross_workspace_memory_grants` as current authority.
- Do not wire HTTP, MCP, worker, retrieval, context hydration, cache, index, provider, export, or any public sharing surface.
- Do not persist grants, mounts, policies, permits, or disclosures. Do not claim durable revocation, cross-process TOCTOU control, or `source_generation` database validation.
- Do not synthesize a source `RequestContext`, pass raw `SharedMemoryRef`/bare identifiers to storage, use a cross-workspace join, `SECURITY DEFINER`, disabled RLS, a superuser, or a `BYPASSRLS` role.
- Keep offline `IDW-014` at `Skip`; do not label a SQLx test `Executed`, `BuildVerified`, or attested proof.
- Keep TRUSTED exactly `199/196/0/3/0`, `passed_build_verified=1`, with skips exactly `IDW-014`, `QUAL-010`, and `REC-016`; the command must remain non-zero.
- Preserve the pre-existing cached rename `apps/console/nginx.conf -> apps/console/nginx.conf.template` and exclude it from every task commit.
- Preserve all existing `apps/console/dist`, `apps/console/node_modules`, `target`, `graphify-out`, and generated/conformance-cache state. Baseline: 43 generated/cache entries, SHA-1 `771c271c6fcc611a3405860235cc2ecdac411fb5`, 0 cached generated paths, 44 total status entries before this plan file.
- Use `git -c safe.directory=E:/Soft/vestrace` for Git commands and `apply_patch` for every intentional text edit, including mutation insertion and removal.
- Stage and commit only the exact paths named by the current task. Before and after each commit, require the cached diff to contain only that task's paths plus the preserved nginx rename; after `commit --only`, only the nginx rename may remain cached.
- Every implementation task follows RED -> minimal GREEN -> focused verification -> mutation where required -> exact-path commit -> spec-compliance review -> code-quality review. Resolve every Critical or Important finding before proceeding.
- Docker work uses a uniquely named standalone PostgreSQL container, a random host port, and guarded `try/finally` cleanup. Do not touch the default Compose project.

## Required Public Interfaces

Task 1 must establish these names and signatures for Task 2:

```rust
pub trait SharedMemoryReadClock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub struct SystemSharedMemoryReadClock;

pub struct SharedMemoryReadPermit {
    // private fields; no public constructor, Clone, Serialize, or Deserialize
}

impl SharedMemoryReadPermit {
    pub fn source_workspace_id(&self) -> WorkspaceId;
    pub fn source_memory_id(&self) -> MemoryId;
    pub fn memory_revision_id(&self) -> MemoryRevisionId;
    pub fn grant_revision_id(&self) -> MemoryShareGrantRevisionId;
    pub fn source_generation(&self) -> &str;
    pub fn target_workspace_id(&self) -> WorkspaceId;
    pub fn target_principal_id(&self) -> PrincipalId;
}

#[async_trait]
pub trait SharedMemoryRevisionReader: Send + Sync {
    async fn read_exact(
        &self,
        permit: SharedMemoryReadPermit,
    ) -> Result<Option<SharedMemoryRevisionRecord>, ApplicationError>;
}

pub struct SharedMemoryRevisionRecord {
    source_workspace_id: WorkspaceId,
    source_memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
    content: String,
}

impl SharedMemoryRevisionRecord {
    pub fn new(
        source_workspace_id: WorkspaceId,
        source_memory_id: MemoryId,
        memory_revision_id: MemoryRevisionId,
        content: String,
    ) -> Self;
    pub fn source_workspace_id(&self) -> WorkspaceId;
    pub fn source_memory_id(&self) -> MemoryId;
    pub fn memory_revision_id(&self) -> MemoryRevisionId;
    pub fn content(&self) -> &str;
}

pub struct SharedMemoryReadResult {
    // private SharedMemoryRef, content, and ShareDisclosure
}

impl SharedMemoryReadResult {
    pub fn shared_ref(&self) -> &SharedMemoryRef;
    pub fn content(&self) -> &str;
    pub fn disclosure(&self) -> &ShareDisclosure;
}

pub struct SharedMemoryReadService<R, C> {
    reader: R,
    clock: C,
}

impl<R, C> SharedMemoryReadService<R, C>
where
    R: SharedMemoryRevisionReader,
    C: SharedMemoryReadClock,
{
    pub fn new(reader: R, clock: C) -> Self;

    pub async fn read_shared(
        &self,
        context: &RequestContext,
        grant: &MemoryShareGrant,
        mount: &mut MemoryMount,
        target_policy: &TargetSharePolicy,
    ) -> Result<Option<SharedMemoryReadResult>, ApplicationError>;
}

pub struct PgSharedMemoryRevisionReader;

impl PgSharedMemoryRevisionReader {
    pub fn new(store: PgStore) -> Self;
}
```

The permit is a public nominal type only because a separate infrastructure crate implements its consuming port. Its fields and issuer remain private; its getters reveal identity from an already-issued permit but cannot reconstruct one. `SharedMemoryReadResult` never exposes a bare `MemoryRevision` or local `MemoryId`.

---

## Task 1: Implement the application permit and shared-read service

**Files:**

- Create: `crates/vestrace-application/src/memory/shared_read.rs`
- Create: `crates/vestrace-application/tests/shared_memory_read.rs`
- Modify: `crates/vestrace-application/src/memory/mod.rs`
- Modify: `crates/vestrace-application/Cargo.toml`
- Modify: `Cargo.lock`

**Interfaces:**

- Consumes: `RequestContext`, `MemoryShareGrant`, mutable `MemoryMount`, `TargetSharePolicy`, `evaluate_share_access`, and `ShareOperation::ReadContent`.
- Produces: every application interface in **Required Public Interfaces** except `PgSharedMemoryRevisionReader`.
- `crates/vestrace-application/src/lib.rs` already re-exports `memory::*`; do not modify it.
- `static_assertions` is already a workspace dependency and resolved in `Cargo.lock`; add `static_assertions.workspace = true` to application dev-dependencies. The only accepted lockfile change is adding the existing `static_assertions` package edge to the `vestrace-application` dependency list; no version, checksum, or new package may change.

### Step 1: Write the failing application integration test

- [ ] Create `crates/vestrace-application/tests/shared_memory_read.rs` with a fixed clock and a recording reader. The first committed test body must import the final public names, so RED is a compilation failure before production code exists:

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vestrace_application::{
    ApplicationError, RequestContext, SharedMemoryReadClock, SharedMemoryReadPermit,
    SharedMemoryReadService, SharedMemoryRevisionReader, SharedMemoryRevisionRecord,
};
use vestrace_domain::Timestamp;

#[derive(Clone)]
struct FixedClock {
    at: Timestamp,
    calls: Arc<Mutex<usize>>,
}

impl SharedMemoryReadClock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.calls.lock().unwrap() += 1;
        self.at
    }
}

#[derive(Clone, Default)]
struct RecordingReader {
    calls: Arc<Mutex<Vec<(String, String, String, String, String, String, String)>>>,
    answer: Arc<Mutex<Option<Result<Option<SharedMemoryRevisionRecord>, String>>>>,
}

#[async_trait]
impl SharedMemoryRevisionReader for RecordingReader {
    async fn read_exact(
        &self,
        permit: SharedMemoryReadPermit,
    ) -> Result<Option<SharedMemoryRevisionRecord>, ApplicationError> {
        self.calls.lock().unwrap().push((
            permit.source_workspace_id().to_string(),
            permit.source_memory_id().to_string(),
            permit.memory_revision_id().to_string(),
            permit.grant_revision_id().to_string(),
            permit.source_generation().to_owned(),
            permit.target_workspace_id().to_string(),
            permit.target_principal_id().to_string(),
        ));
        match self.answer.lock().unwrap().take().expect("configured answer") {
            Ok(value) => Ok(value),
            Err(message) => Err(ApplicationError::Storage(message)),
        }
    }
}
```

- [ ] Build one explicit fixture from `MemoryShareGrantRevisionSpec`, `MemoryShareGrant::issue`, `TargetSharePolicy::new`, and `MemoryMount::accept`. The fixture must grant both `ReadContent` and `IncludeContext`, pin one exact source workspace/memory/revision identity, construct the matching `SharedMemoryRevisionRecord`, return the genuine target `RequestContext`, and allow each test to narrow one side independently.
- [ ] Add the first test `successful_read_consumes_exact_permit_and_records_one_disclosure`. Configure the reader with the exact source record; assert one reader call containing all seven permit values: source workspace, memory, revision, grant revision, source generation, target workspace, and target principal. Also assert `result.shared_ref()` equals the grant's exact namespace, `result.content()` equals the source content, `result.disclosure().operation() == ShareOperation::ReadContent`, mount disclosures length is one, and clock calls equal one.

### Step 2: Run RED

- [ ] Run:

```powershell
cargo test -p vestrace-application --test shared_memory_read -- --nocapture
```

Expected: non-zero compile failure naming the absent shared-read interfaces. Record the exact diagnostic; do not weaken the test.

### Step 3: Implement the minimal application boundary

- [ ] Add `mod shared_read;` and `pub use shared_read::*;` to `memory/mod.rs`.
- [ ] Add `static_assertions.workspace = true` under `[dev-dependencies]` in the application manifest, run `cargo check -p vestrace-application --all-features`, and inspect `Cargo.lock`. Require the delta to add only `"static_assertions"` to the existing `vestrace-application` dependency list, with no dependency version/source/checksum change.
- [ ] Implement the clock port and `SystemSharedMemoryReadClock`, whose `now()` calls `vestrace_domain::time::now()`.
- [ ] Implement `SharedMemoryReadPermit` with exactly these private fields and only the read-only accessors from the public-interface block:

```rust
pub struct SharedMemoryReadPermit {
    source_workspace_id: WorkspaceId,
    source_memory_id: MemoryId,
    memory_revision_id: MemoryRevisionId,
    grant_revision_id: MemoryShareGrantRevisionId,
    source_generation: String,
    target_workspace_id: WorkspaceId,
    target_principal_id: PrincipalId,
}

impl SharedMemoryReadPermit {
    fn issue(shared_ref: &SharedMemoryRef, context: &RequestContext) -> Self {
        Self {
            source_workspace_id: shared_ref.source_workspace_id(),
            source_memory_id: shared_ref.source_memory_id(),
            memory_revision_id: shared_ref.memory_revision_id(),
            grant_revision_id: shared_ref.grant_revision_id(),
            source_generation: shared_ref.source_generation().to_owned(),
            target_workspace_id: context.workspace_id,
            target_principal_id: context.principal_id,
        }
    }
}
```

- [ ] Implement the reader port, the public constructible `SharedMemoryRevisionRecord` data carrier from the interface block, the namespaced result, and service. The record is not authority and may be constructed by adapters/tests; only the permit must remain unforgeable. The service order is security-significant and must be exact:

```rust
pub async fn read_shared(
    &self,
    context: &RequestContext,
    grant: &MemoryShareGrant,
    mount: &mut MemoryMount,
    target_policy: &TargetSharePolicy,
) -> Result<Option<SharedMemoryReadResult>, ApplicationError> {
    if context.workspace_id != mount.target_workspace_id()
        || context.principal_id != mount.target_principal_id()
    {
        return Err(ApplicationError::Policy(
            "shared read target identity does not match the accepted mount".into(),
        ));
    }

    let at = self.clock.now();
    let decision = evaluate_share_access(
        grant,
        mount,
        target_policy,
        ShareOperation::ReadContent,
        at,
    );
    if !decision.is_allowed() {
        return Err(ApplicationError::Policy(format!(
            "shared read denied: {:?}",
            decision.reason()
        )));
    }

    let shared_ref = mount.shared_ref(grant, at)?;
    let permit = SharedMemoryReadPermit::issue(&shared_ref, context);
    let Some(record) = self.reader.read_exact(permit).await? else {
        return Ok(None);
    };
    if record.source_workspace_id() != shared_ref.source_workspace_id()
        || record.source_memory_id() != shared_ref.source_memory_id()
        || record.memory_revision_id() != shared_ref.memory_revision_id()
    {
        return Err(ApplicationError::Storage(
            "shared memory reader returned a revision outside the authorized namespace".into(),
        ));
    }

    let disclosure = mount.record_disclosure(
        grant,
        target_policy,
        ShareOperation::ReadContent,
        at,
    )?;
    Ok(Some(SharedMemoryReadResult {
        shared_ref,
        content: record.content().to_owned(),
        disclosure,
    }))
}
```

Do not expose the retained `MemoryRevision`, a local `MemoryId`, a permit constructor, or a separate authorization method.

### Step 4: Complete the fail-closed test matrix

- [ ] Add these focused tests, each with explicit reader-call, disclosure, and error/result assertions:

```text
wrong_target_workspace_denies_before_clock_and_reader
wrong_target_principal_denies_before_clock_and_reader
source_without_read_content_denies_before_reader
mount_without_read_content_denies_before_reader
current_target_policy_without_read_content_denies_before_reader
wrong_grant_revision_denies_before_reader
revoked_source_grant_denies_before_reader
expired_source_grant_denies_before_reader
stale_mount_denies_before_reader
expired_mount_denies_before_reader
trusted_clock_is_sampled_once_and_reused_for_disclosure
missing_revision_returns_none_without_disclosure
storage_failure_returns_no_content_or_disclosure
mismatched_record_workspace_is_storage_failure
mismatched_record_memory_is_storage_failure
mismatched_record_revision_is_storage_failure
```

- [ ] For source denial, issue grant/mount/policy with only `IncludeContext`. For mount denial, let grant and policy contain both operations but accept only `IncludeContext`. For target denial, accept with a read-capable policy and call the service with a same-identity current policy containing only `IncludeContext`.
- [ ] For stale mount, call `sync_with_source` using a different grant and then evaluate the original. Use distinct validity fixtures: source-expiry keeps mount/policy valid beyond the clock; mount-expiry keeps grant/policy valid beyond the mount's `valid_until`; both clocks sit exactly at the relevant boundary. Run workspace-only and principal-only identity mismatches separately. Construct three independent bad records so each identity field mismatch is observed separately. Do not add domain transitions merely to manufacture suspended/deleted enum variants.
- [ ] In every pre-reader denial assert `reader.calls.is_empty()` and `mount.disclosures().is_empty()`. For wrong target identity also assert the clock was never sampled. For missing/storage/mismatch assert no disclosure. For the trusted-clock test assert exactly one sample and that the disclosure timestamp equals that sample.

### Step 5: Compile-check permit unforgeability

- [ ] Add `#[cfg(test)]` negative assertions beside the permit. Split macro invocations if Rust 1.85 cannot infer a grouped assertion; do not remove any path:

```rust
use static_assertions::assert_not_impl_any;

assert_not_impl_any!(SharedMemoryReadPermit:
    Clone,
    Default,
    serde::Serialize,
    serde::de::DeserializeOwned,
    From<SharedMemoryRef>,
    From<WorkspaceId>,
    From<MemoryId>,
    From<MemoryRevisionId>,
    From<MemoryShareGrantRevisionId>,
    From<PrincipalId>,
    Into<SharedMemoryRef>,
    Into<WorkspaceId>,
    Into<MemoryId>,
    Into<MemoryRevisionId>,
    Into<MemoryShareGrantRevisionId>,
    Into<PrincipalId>
);
```

- [ ] Put two `compile_fail` rustdoc examples on `SharedMemoryReadPermit`: one external struct literal using `todo!()` for every private field, and one call to private `SharedMemoryReadPermit::issue(todo!(), todo!())`. These must compile-fail for privacy, not for a misspelled type.

### Step 6: Run GREEN and focused quality gates

- [ ] Run:

```powershell
cargo test -p vestrace-application --test shared_memory_read -- --nocapture
cargo test -p vestrace-application --lib --all-features -- --nocapture
cargo test -p vestrace-application --doc --all-features -- --nocapture
cargo clippy -p vestrace-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git -c safe.directory=E:/Soft/vestrace diff --check -- Cargo.lock crates/vestrace-application
```

Expected: every command exits 0, privacy doc tests fail-to-compile as intended, and `Cargo.lock` contains only the accepted application dependency edge.

### Step 7: Commit and review Task 1

- [ ] Commit only the five Task 1 paths:

```powershell
git -c safe.directory=E:/Soft/vestrace add -- Cargo.lock crates/vestrace-application/Cargo.toml crates/vestrace-application/src/memory/mod.rs crates/vestrace-application/src/memory/shared_read.rs crates/vestrace-application/tests/shared_memory_read.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "feat(application): authorize exact shared reads" -- Cargo.lock crates/vestrace-application/Cargo.toml crates/vestrace-application/src/memory/mod.rs crates/vestrace-application/src/memory/shared_read.rs crates/vestrace-application/tests/shared_memory_read.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
```

Expected after commit: cached state is only the preserved nginx rename. Dispatch spec-compliance review, resolve findings, then dispatch code-quality review and resolve findings before Task 2.

---

## Task 2: Implement the restricted-RLS PostgreSQL reader

**Files:**

- Create: `crates/vestrace-infrastructure/src/postgres/shared_memory_revision_reader.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `tests/support/mod.rs`
- Create: `tests/idw_014_shared_read_postgres.rs`
- Create ignored execution helper: `.superpowers/sdd/2026-08-16-v1-idw-014-shared-read-adapter/run-isolated-pg.ps1`

**Interfaces:**

- Consumes: Task 1 `SharedMemoryReadPermit`, `SharedMemoryRevisionReader`, `SharedMemoryRevisionRecord`, `SharedMemoryReadService`, and `SharedMemoryReadResult`.
- Produces: `PgSharedMemoryRevisionReader::new(PgStore)`, plus a panic-safe `with_restricted_runtime_role` test helper using a real restricted LOGIN pool.
- No Cargo manifest or lockfile changes are permitted.

### Step 1: Write failing restricted-login helper tests

- [ ] Keep the existing `create_restricted_role`, `with_restricted_role`, and NOLOGIN/`SET LOCAL ROLE` tests unchanged. In the new root test, import this still-absent helper contract so RED occurs before implementation:

```rust
pub async fn with_restricted_runtime_role<T, F, Fut>(
    admin_pool: &sqlx::PgPool,
    test: F,
) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool, RestrictedRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static;

pub(crate) async fn with_restricted_runtime_role_connector<T, F, Fut, C, CFut>(
    admin_pool: &sqlx::PgPool,
    fail_before: Option<RestrictedRuntimeRoleSetupStep>,
    connect: C,
    test: F,
) -> T
where
    T: Send + 'static,
    F: FnOnce(sqlx::PgPool, RestrictedRole) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    C: FnOnce(sqlx::postgres::PgConnectOptions, RestrictedRole) -> CFut + Send + 'static,
    CFut: Future<Output = Result<sqlx::PgPool, sqlx::Error>> + Send + 'static;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RestrictedRuntimeRoleSetupStep {
    GrantConnect,
    GrantSchemaUsage,
    GrantTableSelect,
}
```

- [ ] Add a callback-panic cleanup test, a table-driven partial-setup cleanup test for all three `RestrictedRuntimeRoleSetupStep` values, and an injected pool-connect-failure cleanup test before implementing the helper. Every case captures the exact generated role name and requires that `pg_roles` contains no such role after the helper propagates the failure. The setup-failure cases pass `Some(step)` and must fail immediately before that grant, so they exercise cleanup after role creation alone, after CONNECT, and after CONNECT plus USAGE. The pool-failure test passes `None`, records the `RestrictedRole` supplied to the connector, overrides the correct password with a known-wrong password, and calls `PgPoolOptions::new().max_connections(1).connect_with(options.password("known-wrong-password"))`.
- [ ] Record the required pool construction shape in the tests, but do not add it to `tests/support/mod.rs` until after RED:

```rust
let options = admin_pool
    .connect_options()
    .as_ref()
    .clone()
    .username(role.name())
    .password(&password);
let runtime_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(options)
    .await;
```

- [ ] The tests must require an explicit `Result` match rather than `.expect()`: a pool connection error is propagated only after grants are revoked and the generated role is dropped.

### Step 2: Write the failing PostgreSQL integration test

- [ ] Create root test `tests/idw_014_shared_read_postgres.rs` with `mod support;` and the main `#[sqlx::test(migrations = "./migrations")]` shared-read test.
- [ ] Seed through the admin pool these fixed identities: source/target/wrong-source workspaces ending `...001/...002/...003`, target principal `...004`, one source memory `...005`, and two source revisions `...006/...007`. Use the same memory for both revisions and distinct content.
- [ ] Snapshot before the read:

```sql
SELECT c.relrowsecurity,
       c.relforcerowsecurity,
       pg_get_userbyid(c.relowner)::text
FROM pg_class c
WHERE c.oid = 'memory_revisions'::regclass
```

and ordered policy rows:

```sql
SELECT polname::text,
       polcmd::text,
       pg_get_expr(polqual, polrelid),
       pg_get_expr(polwithcheck, polrelid)
FROM pg_policy
WHERE polrelid = 'memory_revisions'::regclass
ORDER BY polname
```

- [ ] Inside `with_restricted_runtime_role`, prove actual connection identity before invoking application code:

```sql
SELECT current_user::text,
       session_user::text,
       r.rolsuper,
       r.rolbypassrls,
       EXISTS (
           SELECT 1
           FROM pg_roles privileged
           WHERE (privileged.rolsuper OR privileged.rolbypassrls)
             AND pg_has_role(current_user, privileged.oid, 'MEMBER')
       ) AS has_privileged_membership
FROM pg_roles r
WHERE r.rolname = current_user
```

Assert `current_user == session_user == generated role`, both role flags false, and privileged membership false.
- [ ] Construct `PgStore` from the restricted pool. Assert ordinary target `PgMemoryRepository::find_revision_by_id(&target_context, source_revision_1)` returns `None`.
- [ ] Construct separate valid grants and mounts pinned to revision 1 and revision 2. Through the real service and adapter, require each permit to return its own pinned content. This two-direction assertion must fail if `id = $3` is omitted; a row-order success for revision 1 alone is insufficient. Add a third valid domain grant whose declared source workspace is the wrong-source workspace but pins revision 1; require `None` and no disclosure.
- [ ] After both reads, use the one-connection pool to assert transaction-local scope did not leak:

```sql
SELECT NULLIF(current_setting('vestrace.workspace_id', true), '') IS NULL
```

- [ ] Through the admin pool, repeat the class/policy snapshots and assert byte-for-byte tuple equality with the before values, with both RLS flags true.
- [ ] Add a second `#[sqlx::test(migrations = "./migrations")]` named `restricted_runtime_role_is_dropped_when_body_panics`. Capture the generated role name, make the helper callback panic inside an outer spawned task, assert the outer `JoinError` is a panic, then query `pg_roles` through the admin pool and require that role name to be absent. This proves cleanup on the failure path rather than only documenting it.
- [ ] Add `restricted_runtime_role_is_dropped_when_pool_creation_fails`. Invoke the connector-injection helper with a wrong-password pool connector, assert the outer task reports the connection failure, then require the captured exact role name to be absent from `pg_roles`. This test and the panic test must both be present before helper implementation.

### Step 3: Create the ignored isolated-PostgreSQL runner and run RED

- [ ] Create the ignored `run-isolated-pg.ps1` with `apply_patch`. It accepts `-Gate idw014|task2-green|full`, starts only `vestrace-idw014-pg-<32 hex>`, polls readiness for at most 60 seconds, and selects commands as follows:

```powershell
param(
  [Parameter(Mandatory)]
  [ValidateSet('idw014', 'task2-green', 'full')]
  [string]$Gate
)

$commands = switch ($Gate) {
  'idw014' { @({ cargo test -p vestrace-integration-tests --test idw_014_shared_read_postgres -- --nocapture }) }
  'task2-green' { @(
    { cargo test -p vestrace-integration-tests --test idw_014_shared_read_postgres -- --nocapture },
    { cargo test -p vestrace-integration-tests --test rls -- --nocapture }
  ) }
  'full' { @(
    { cargo test -p vestrace-integration-tests --test idw_014_shared_read_postgres -- --nocapture },
    { cargo test -p vestrace-integration-tests --test rls -- --nocapture },
    { cargo test --workspace --all-targets --all-features --no-fail-fast -- --nocapture }
  ) }
}
```

- [ ] The runner must implement this exact safety envelope around those commands:

```powershell
$pgName = "vestrace-idw014-pg-$([guid]::NewGuid().ToString('N'))"
$hadDatabaseUrl = Test-Path Env:DATABASE_URL
$previousDatabaseUrl = if ($hadDatabaseUrl) { $env:DATABASE_URL } else { $null }
$created = $false
$containerId = $null
$gateExit = 1
try {
  if ($pgName -notmatch '^vestrace-idw014-pg-[0-9a-f]{32}$') { throw 'Unsafe PostgreSQL container name.' }
  $containerId = (docker run --detach --name $pgName --env POSTGRES_DB=vestrace_test --env POSTGRES_USER=vestrace --env POSTGRES_PASSWORD=vestrace --publish 127.0.0.1::5432 pgvector/pgvector:pg17).Trim()
  $runExit = $LASTEXITCODE
  if ($runExit -ne 0) { throw 'Failed to start isolated PostgreSQL.' }
  if ($containerId -notmatch '^[0-9a-f]{64}$') { throw "Invalid created container ID: $containerId" }
  $created = $true
  $resolvedId = (docker container inspect --format '{{.Id}}' $pgName).Trim()
  if ($LASTEXITCODE -ne 0 -or $resolvedId -ne $containerId) { throw "Container name/ID mismatch for $pgName" }
  $ready = $false
  for ($attempt = 0; $attempt -lt 60; $attempt++) {
    docker exec $pgName pg_isready -U vestrace -d vestrace_test | Out-Null
    if ($LASTEXITCODE -eq 0) { $ready = $true; break }
    Start-Sleep -Seconds 1
  }
  if (-not $ready) { throw 'Isolated PostgreSQL was not ready within 60 seconds.' }
  $pgPort = ((docker port $pgName 5432/tcp) -split ':')[-1]
  if ($pgPort -notmatch '^\d+$') { throw "Invalid PostgreSQL port: $pgPort" }
  $imageId = (docker container inspect --format '{{.Image}}' $pgName).Trim()
  if ($LASTEXITCODE -ne 0 -or $imageId -notmatch '^sha256:[0-9a-f]{64}$') { throw "Invalid PostgreSQL image ID: $imageId" }
  Write-Output "IDW014_POSTGRES_CONTAINER=$pgName PORT=$pgPort IMAGE=$imageId"
  $env:DATABASE_URL = "postgres://vestrace:vestrace@127.0.0.1:$pgPort/vestrace_test"
  $gateExit = 0
  foreach ($command in $commands) {
    & $command
    if ($LASTEXITCODE -ne 0) { $gateExit = $LASTEXITCODE; break }
  }
} finally {
  if ($hadDatabaseUrl) { $env:DATABASE_URL = $previousDatabaseUrl }
  else { Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue }
  if ($created) {
    if ($containerId -notmatch '^[0-9a-f]{64}$') { throw 'Refusing cleanup without an owned container ID.' }
    docker rm --force $containerId | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to remove owned container $containerId" }
    docker container inspect $containerId 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { throw "Container still exists: $containerId" }
  }
}
exit $gateExit
```

- [ ] Run `& .\.superpowers\sdd\2026-08-16-v1-idw-014-shared-read-adapter\run-isolated-pg.ps1 -Gate idw014` and require a non-zero compile failure naming both absent helper and absent adapter. Require the runner to restore the prior environment and remove its container on RED.

### Step 4: Implement the restricted-login helper and adapter

- [ ] Keep existing NOLOGIN/`SET LOCAL ROLE` helpers unchanged. Add `with_restricted_runtime_role`, `with_restricted_runtime_role_connector`, and `RestrictedRuntimeRoleSetupStep` with the exact Step 1 signatures. The public helper delegates with `fail_before = None` and the real one-connection `connect_with`; only cleanup tests supply a failpoint or different connector.
- [ ] Generate a unique `vestrace_runtime_<uuid>` role and hex-only password; create it as `LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS`. Explicitly grant CONNECT on the isolated SQLx test database, USAGE on `public`, and SELECT on `memories, memory_revisions`. This is a role-specific grant; do not claim PostgreSQL's default `PUBLIC` CONNECT privilege was revoked or that the role has exclusive effective database access.
- [ ] Track setup with `RestrictedRuntimeRoleSetupProgress { connect_granted, schema_usage_granted, table_select_granted }`, initially all false. Before each grant, trigger the matching test failpoint when configured. Match every grant result explicitly; after each success set only its flag. Any failpoint or grant error must call the common cleanup in reverse order for exactly the completed grants, drop the generated role, and only then propagate/panic. This same cleanup function must also handle pool-connect and callback failures.
- [ ] Clone `admin_pool.connect_options().as_ref()`, override username/password, and open a one-connection pool. Match `connect_with(...).await` explicitly. On `Err`, invoke the common progress-aware cleanup before propagating/panicking; `.expect()` before cleanup is forbidden.
- [ ] Retain the owner runtime pool and pass a clone to the spawned callback. On callback success or panic, close the owner pool first, revoke SELECT/USAGE/role-specific CONNECT, drop only the generated role, assert cleanup, then return or resume the panic. Never log the password. All Step 1 cleanup tests must pass through the same cleanup function used by production test setup.
- [ ] Export the new module and `PgSharedMemoryRevisionReader` from `postgres/mod.rs`; `crates/vestrace-infrastructure/src/lib.rs` already re-exports `postgres::*` and must remain unchanged.
- [ ] Implement `PgSharedMemoryRevisionReader { store: PgStore }` and the consuming port. Copy expected values from permit getters before opening the transaction. Never construct `RequestContext` or call `begin_scoped`.
- [ ] Use this transaction order:

```rust
let mut transaction = self.store.pool().begin().await.map_err(storage_error)?;
sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
    .bind("vestrace.workspace_id")
    .bind(source_workspace_id.to_string())
    .fetch_one(&mut *transaction)
    .await
    .map_err(storage_error)?;
sqlx::query_scalar::<_, String>("SELECT set_config($1, $2, true)")
    .bind("vestrace.principal_id")
    .bind(target_principal_id.to_string())
    .fetch_one(&mut *transaction)
    .await
    .map_err(storage_error)?;

let active: bool = sqlx::query_scalar(
    "SELECT row_security_active('memory_revisions'::regclass)",
)
.fetch_one(&mut *transaction)
.await
.map_err(storage_error)?;
if !active {
    return Err(ApplicationError::Storage(
        "row level security is inactive for shared memory reads".into(),
    ));
}
```

- [ ] Query only the identity and content needed by the application record, with all three predicates:

```sql
SELECT workspace_id, memory_id, id AS memory_revision_id, content
FROM memory_revisions
WHERE workspace_id = $1
  AND memory_id = $2
  AND id = $3
```

Parse all four columns, revalidate `workspace_id`, `memory_id`, and `memory_revision_id`, and return `Storage` on an integrity mismatch. Commit and return `Ok(None)` for no row; construct `SharedMemoryRevisionRecord::new(...)`, commit, and return `Ok(Some(record))` only for an exact match. A dropped transaction on any error must roll back.

### Step 5: Run GREEN and existing RLS regression

- [ ] Run database-backed gates through a fresh runner-owned container, then database-independent gates:

```powershell
& .\.superpowers\sdd\2026-08-16-v1-idw-014-shared-read-adapter\run-isolated-pg.ps1 -Gate task2-green
cargo test -p vestrace-infrastructure --all-targets --all-features --no-run
cargo clippy -p vestrace-infrastructure --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all commands exit 0. The first test must report the actual LOGIN role identity, restricted flags, no privileged membership, direct target isolation, exact shared read, wrong-source denial, active-RLS check, unchanged catalog snapshots, no leaked GUC, and successful role cleanup.

### Step 6: Prove mutation sensitivity

- [ ] With `apply_patch`, temporarily change only the adapter's workspace GUC bind from `source_workspace_id` to `target_workspace_id`.
- [ ] Run the ignored runner with `-Gate idw014`. Require non-zero exit at the authorized source-read assertion after direct target isolation has passed. The runner must still restore the prior environment and remove its isolated container.
- [ ] Remove only the mutation with `apply_patch`. Run the same ignored runner again and require exit 0. Record mutated RED and restored GREEN; never stage the mutation or ignored runner.

### Step 7: Commit and review Task 2

- [ ] Commit only the four Task 2 paths:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- crates/vestrace-infrastructure/src/postgres/shared_memory_revision_reader.rs crates/vestrace-infrastructure/src/postgres/mod.rs tests/support/mod.rs tests/idw_014_shared_read_postgres.rs
git -c safe.directory=E:/Soft/vestrace add -- crates/vestrace-infrastructure/src/postgres/shared_memory_revision_reader.rs crates/vestrace-infrastructure/src/postgres/mod.rs tests/support/mod.rs tests/idw_014_shared_read_postgres.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "feat(storage): read shared revisions under forced RLS" -- crates/vestrace-infrastructure/src/postgres/shared_memory_revision_reader.rs crates/vestrace-infrastructure/src/postgres/mod.rs tests/support/mod.rs tests/idw_014_shared_read_postgres.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
```

Expected after commit: cached state is only the preserved nginx rename. Dispatch and resolve spec-compliance review, then code-quality review before Task 3.

---

## Task 3: Update the truthful offline IDW-014 boundary

**Files:**

- Modify: `crates/vestrace-cli/src/commands/conformance.rs`
- Create: `crates/vestrace-cli/tests/idw_014_shared_read_truth.rs`

**Interfaces:**

- Consumes: committed adapter/test evidence from Tasks 1-2.
- Produces: a more precise offline `IDW-014` skip message and black-box CLI regression test.
- Does not register an executable case, change `CaseOrigin`, or change any TRUSTED count.

### Step 1: Write the failing black-box CLI test

- [ ] Copy the child-process pattern from `idw_010_build_verified.rs`, run the built CLI with `conformance check trusted --json`, accept the required non-zero child exit, and parse stdout as JSON.
- [ ] Find `IDW-014` and assert:

```rust
assert_eq!(idw_014["status"], "skip");
assert_eq!(idw_014["origin"], "attested");
let message = idw_014["message"].as_str().unwrap();
assert!(message.contains("PgSharedMemoryRevisionReader"));
assert!(message.contains("restricted"));
assert!(message.contains("offline"));
assert!(message.contains("database-backed"));
assert_eq!(
    idw_014["evidence"],
    "tests/idw_014_shared_read_postgres.rs"
);
```

- [ ] Assert exact summary values `199/196/0/3/0`, `passed_build_verified == 1`, and sorted skips exactly `IDW-014`, `QUAL-010`, `REC-016`. Assert no IDW-014 result has `pass`, `executed`, or `build_verified`.

### Step 2: Run RED

- [ ] Run:

```powershell
cargo test -p vestrace-cli --test idw_014_shared_read_truth -- --nocapture
```

Expected: the new semantic message/evidence assertions fail against the historical domain-only fallback, while the existing counts remain unchanged.

### Step 3: Replace only the stale fallback wording

- [ ] In the `(F::Idw, 14, _)` branch of `evaluate_requirement`, keep `CaseStatus::Skip` and return a message with these exact claims:

```text
PgSharedMemoryRevisionReader and SharedMemoryReadService now execute an exact permit-bound source revision read under forced RLS, and tests/idw_014_shared_read_postgres.rs proves direct target isolation through a NOSUPERUSER NOBYPASSRLS login. The offline conformance report cannot execute or ingest that database-backed evidence, so IDW-014 remains skipped until a DB-backed qualification evidence path exists. This is not a production sharing surface: grants, mounts, policies, and disclosures remain in memory and source_generation is not stored with memory_revisions.
```

- [ ] Set evidence to `Some("tests/idw_014_shared_read_postgres.rs".to_string())`. Do not add an application mock case to the conformance runner and do not change origin or hard-gate behavior.

### Step 4: Run GREEN and regress IDW-010

- [ ] Run:

```powershell
cargo test -p vestrace-cli --test idw_014_shared_read_truth -- --nocapture
cargo test -p vestrace-cli --test idw_010_build_verified -- --nocapture
cargo clippy -p vestrace-cli --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all commands exit 0; both child TRUSTED commands remain non-zero as asserted; counts and skip IDs are unchanged.

### Step 5: Commit and review Task 3

- [ ] Commit only the two CLI paths:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check -- crates/vestrace-cli/src/commands/conformance.rs crates/vestrace-cli/tests/idw_014_shared_read_truth.rs
git -c safe.directory=E:/Soft/vestrace add -- crates/vestrace-cli/src/commands/conformance.rs crates/vestrace-cli/tests/idw_014_shared_read_truth.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "fix(conformance): explain IDW-014 runtime evidence" -- crates/vestrace-cli/src/commands/conformance.rs crates/vestrace-cli/tests/idw_014_shared_read_truth.rs
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
```

Expected after commit: cached state is only the preserved nginx rename. Dispatch and resolve spec-compliance review, then code-quality review before final gates.

---

## Task 4: Run final local and isolated runtime gates

**Files:**

- Create: `docs/superpowers/reports/2026-08-16-v1-idw-014-shared-read-adapter.md`
- Modify only if execution facts require a truthful correction: `docs/superpowers/specs/2026-08-16-v1-idw-014-shared-read-adapter-design.md`
- Modify only if execution diverges: `docs/superpowers/plans/2026-08-16-v1-idw-014-shared-read-adapter.md`

### Step 1: Capture exact preservation and range baselines

- [ ] Record:

```powershell
git -c safe.directory=E:/Soft/vestrace status --short
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace log --oneline aa5739a..HEAD
git -c safe.directory=E:/Soft/vestrace diff --name-status aa5739a..HEAD
```

- [ ] Snapshot the default Compose project without changing it:

```powershell
$defaultContainers = @(docker ps -a --filter 'label=com.docker.compose.project=vestrace' --format '{{.ID}} {{.Status}} {{.Names}}')
$defaultNetworks = @(docker network ls --filter 'label=com.docker.compose.project=vestrace' --format '{{.ID}} {{.Name}}')
$defaultVolumes = @(docker volume ls --filter 'label=com.docker.compose.project=vestrace' --format '{{.Name}}')
```

- [ ] Reuse the established generated/cache procedure exactly:

```powershell
$generatedStatus = git -c safe.directory=E:/Soft/vestrace status --short -- apps/console/dist apps/console/node_modules
$generatedDiff = git -c safe.directory=E:/Soft/vestrace diff --binary -- apps/console/dist apps/console/node_modules
$generatedHash = ($generatedDiff -join "`n") | git hash-object --stdin
$cachedGenerated = git -c safe.directory=E:/Soft/vestrace diff --cached --name-only -- apps/console/dist apps/console/node_modules target graphify-out crates/vestrace-domain/src/generated crates/vestrace-domain/src/conformance/generated
```

Require 43 entries, hash `771c271c6fcc611a3405860235cc2ecdac411fb5`, cached generated count 0, and only the nginx rename cached. Stop and investigate any drift; do not normalize it.

### Step 2: Run database-independent Rust gates

- [ ] Run each independently and record exit code and counts:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --no-run
cargo test -p vestrace-application --test shared_memory_read -- --nocapture
cargo test -p vestrace-application --doc --all-features -- --nocapture
cargo test -p vestrace-cli --test idw_014_shared_read_truth -- --nocapture
cargo test -p vestrace-cli --test idw_010_build_verified -- --nocapture
cargo test --test v1_release_evidence -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
```

Expected: every parent command exits 0; release evidence remains 4/4 and registry remains 3/3.

### Step 3: Run fresh isolated PostgreSQL acceptance

- [ ] Run the complete database-backed suite through Task 2's ignored runner:

```powershell
& .\.superpowers\sdd\2026-08-16-v1-idw-014-shared-read-adapter\run-isolated-pg.ps1 -Gate full
```

Expected: the runner exits 0 after creating a fresh uniquely named PostgreSQL 17 container, polling readiness for at most 60 seconds, running all three commands, restoring the prior `DATABASE_URL` state, and removing only its exact container. Record the image ID/digest and random port, total pass/fail/ignored counts, the generated restricted LOGIN role identity, `current_user == session_user`, `rolsuper=false`, `rolbypassrls=false`, no privileged membership, direct target read `None`, both exact permitted revision reads, wrong-source read `None`, `row_security_active=true` in the reader transaction, unchanged RLS/policy snapshot, no leaked GUC, and successful role cleanup.
- [ ] Verify the runner-owned container is absent. Capture the same three default-project arrays again and require all three `Compare-Object` results to be empty. No Compose `up` or `down` is required for this unwired adapter.

### Step 4: Run CLI truth and exact TRUSTED parse

- [ ] Run:

```powershell
& 'C:\Program Files\Git\bin\bash.exe' ./scripts/foundation-cli-truth.sh
cargo build -p vestrace-cli --bin vestrace
```

Expected: both exit 0.
- [ ] Run the Windows executable explicitly, capture stdout/stderr separately, require exit 1, and parse JSON rather than prose:

```powershell
$stdout = Join-Path $env:TEMP "idw014-trusted-$([guid]::NewGuid().ToString('N')).json"
$stderr = Join-Path $env:TEMP "idw014-trusted-$([guid]::NewGuid().ToString('N')).stderr"
try {
  & .\target\debug\vestrace.exe conformance check trusted --json 1> $stdout 2> $stderr
  $trustedExit = $LASTEXITCODE
  if ($trustedExit -ne 1) { throw "Expected TRUSTED exit 1, got $trustedExit" }
  $report = Get-Content -Raw -LiteralPath $stdout | ConvertFrom-Json
  if ($report.summary.total -ne 199 -or
      $report.summary.passed -ne 196 -or
      $report.summary.failed -ne 0 -or
      $report.summary.skipped -ne 3 -or
      $report.summary.not_applicable -ne 0 -or
      $report.summary.passed_build_verified -ne 1) {
    throw 'Unexpected TRUSTED summary.'
  }
  $idw014 = @($report.results | Where-Object {
    @($_.requirement_ids | Where-Object { $_.family -eq 'idw' -and $_.number -eq 14 }).Count -gt 0
  })
  if ($idw014.Count -ne 1 -or $idw014[0].status -ne 'skip' -or $idw014[0].origin -ne 'attested') {
    throw 'Unexpected IDW-014 result.'
  }
  $skipIds = @(
    $report.results |
      Where-Object status -eq 'skip' |
      ForEach-Object requirement_ids |
      ForEach-Object { '{0}-{1:000}' -f $_.family.ToUpperInvariant(), $_.number } |
      Sort-Object
  )
  if ((Compare-Object $skipIds @('IDW-014', 'QUAL-010', 'REC-016')).Count -ne 0) {
    throw 'Unexpected TRUSTED skip set.'
  }
} finally {
  Remove-Item -LiteralPath $stdout, $stderr -ErrorAction SilentlyContinue
}
```

Require exactly:

```text
total=199
passed=196
failed=0
skipped=3
not_applicable=0
passed_build_verified=1
IDW-014=skip/attested
skips=IDW-014,QUAL-010,REC-016
```

The IDW-014 message/evidence must match Task 3. Do not reinterpret the non-zero exit as a product failure; it is the required open-TRUSTED state.

### Step 5: Verify exact range and write the durable report

- [ ] Run:

```powershell
git -c safe.directory=E:/Soft/vestrace diff --check aa5739a..HEAD
git -c safe.directory=E:/Soft/vestrace log --oneline aa5739a..HEAD
git -c safe.directory=E:/Soft/vestrace diff --name-status aa5739a..HEAD
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace status --short
```

- [ ] Create the durable report with base/final commits, per-task exact paths, application RED/GREEN, permit compile/doc boundaries, PostgreSQL RED/GREEN, mutation RED/restored GREEN, actual restricted role/RLS evidence, all final gate counts, exact unchanged TRUSTED output, container cleanup/default-project preservation, generated/cache baseline, known limitations, and explicit no-migration/no-public-wiring claims.
- [ ] End the report with exactly:

```text
IDW-014 shared-read adapter and restricted-role PostgreSQL isolation evidence complete; offline TRUSTED remains open because database-backed conformance evidence is not yet ingestible.
```

### Step 6: Commit the report and rerun post-commit truth checks

- [ ] Commit only the report plus any explicitly justified truthful spec/plan correction:

```powershell
git -c safe.directory=E:/Soft/vestrace add -- docs/superpowers/reports/2026-08-16-v1-idw-014-shared-read-adapter.md
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
git -c safe.directory=E:/Soft/vestrace commit --only -m "docs: record IDW-014 shared-read evidence" -- docs/superpowers/reports/2026-08-16-v1-idw-014-shared-read-adapter.md
git -c safe.directory=E:/Soft/vestrace diff --check aa5739a..HEAD
git -c safe.directory=E:/Soft/vestrace diff --cached --name-status
```

If spec/plan corrections were necessary, add their exact paths to both staging and `commit --only`, explain them in the report, and rerun every affected gate. Expected cached state after commit: only the preserved nginx rename.

---

## Task 5: Independent broad security and scope review

**Files:** No planned tracked edits. Every accepted fix returns to the relevant task's RED/GREEN, exact-path commit, and full Task 4 verification discipline.

### Step 1: Review the exact complete range

- [ ] Give a fresh independent reviewer the design, this plan, durable report, SDD ledger/task reports, and exact range `aa5739a..HEAD`.
- [ ] Require line-by-line review of:

  - authenticated target workspace/principal check before clock and storage;
  - one trusted clock sample reused for evaluation and disclosure;
  - permit private issuance, non-cloneability/non-serialization, public getter minimum, and by-value consumption;
  - absence of raw-reference/bare-ID storage authorization and synthetic source `RequestContext`;
  - exact workspace/memory/revision SQL predicates and duplicate identity validation;
  - same-pool restricted LOGIN identity, privileged-role membership check, active RLS inside the adapter transaction, forced-RLS/policy preservation, and transaction-local GUC cleanup;
  - denial/no-reader/no-disclosure tests across source, mount, target, expiry, revocation, stale state, absence, storage failure, and identity mismatch;
  - mutation sensitivity and absence of the temporary mutation from HEAD;
  - unchanged offline skip/count/origin truth and no false conformance PASS;
  - no migration, public wiring, durable-state, source-generation, TOCTOU, or v1.0 overclaim;
  - exact committed scope and preservation of unrelated dirty/cached/generated/Docker state.

### Step 2: Resolve findings by severity

- [ ] Fix every Critical or Important finding through a focused RED test, minimal change, exact-path commit, and Task 4 full rerun. Re-review each fix.
- [ ] Defer a Minor only with a concrete reason and durable-report entry; do not use a Minor label to hide a security or evidence-truth defect.
- [ ] Do not declare completion until the reviewer reports no open Critical or Important findings.

### Step 3: Final handoff

- [ ] Report final HEAD and task commits, focused/full gate results, restricted-role/RLS proof, mutation proof, container cleanup, preservation values, and exact remaining TRUSTED skips.
- [ ] Use only the approved truthful completion statement. The next design slice is DB-backed qualification evidence ingestion for IDW-014; do not begin it within this implementation range.
