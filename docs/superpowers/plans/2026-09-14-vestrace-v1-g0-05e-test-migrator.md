# P05-E Historical Test Migrator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the database-backed regression suites, which have failed at migration 0209 since P05-A, by bounding the test schema at the historical 0001–0208 prefix without weakening any guard or changing production behaviour.

**Architecture:** One exported `HISTORICAL_MIGRATOR` static, built from the `bounded_migrator` helper that already exists in `pool.rs`, replaces `migrations = "../../migrations"` in every `#[sqlx::test]` that is not migration-sensitive. The twenty-nine migration-sensitive tests are audited individually and either kept on the historical migrator, or moved to a prepared database supplied through `VESTRACE_P05_TEST_DATABASE_URL`. Two guards keep the boundary from eroding.

**Tech Stack:** Rust, sqlx 0.8 (`migrator = "<rust path>"` test attribute), PostgreSQL 17, Node.js test runner.

**Spec:** `docs/superpowers/specs/2026-09-14-vestrace-v1-g0-05e-test-migrator-design.md`

## Global Constraints

- P05-E is a sub-package of P05. It extends `scripts/p05-scope.mjs` and `docs/development-evidence/v1-g0-05-preflight.json`; it does not create a new scope module, because `scripts/verify-dirty-baseline.mjs` hard-codes `^p(\d{2})-scope\.mjs$` and is protected authority.
- Preserve inherited dirty paths. Amend the allowlist and the captured preflight scope entry before any implementation write.
- Do not modify migrations `0001` through `0215`, any P01–P05 authority path, or `scripts/verify-dirty-baseline.mjs`.
- Do not weaken migration 0209. Do not change `MIGRATOR`, `migrate_through_version`, `migrate_only_version_after`, `p05_assertion_migration`, `migrations_are_compatible`, or `HealthRepository::check`.
- P05-E does not repair the failures it exposes. Every genuine failure is recorded and named as follow-up work.
- No test may be deleted to avoid classifying it.
- Do not commit, push, deploy, request secrets, or make provider calls.
- The `DATABASE_URL` for ordinary runs is `postgres://test:test@127.0.0.1:55432/vestrace_test`. Use `127.0.0.1`, never `localhost`: this host resolves `localhost` to `::1` first and the IPv6 attempt exceeds the one-second acquisition timeout.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | Admits the P05-E implementation paths. |
| `tests/p05_scope.test.mjs` | P05-E scope regression. |
| `docs/development-evidence/v1-g0-05-preflight.json` | Captured reviewed scope entry. |
| `crates/vestrace-infrastructure/src/postgres/pool.rs` | `HISTORICAL_MIGRATOR` static and its guard test. |
| `crates/vestrace-infrastructure/src/postgres/mod.rs` | Re-export of the static. |
| `tests/p05e_test_migrator.test.mjs` | Guard: no test applies the raw migrations directory. |
| `crates/vestrace-infrastructure/tests/*.rs`, `crates/vestrace-cli/tests/*.rs` | Attribute switched to the bounded migrator. |
| `crates/vestrace-infrastructure/tests/deployment_qualification.rs` | Prepared-database home for the moved `postgres.rs` tests. |
| `docs/development-evidence/v1-g0-05e-test-migrator.md` | P05-E observations and the measured red-test figure. |

---

### Task 1: Admit the P05-E implementation scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`

**Interfaces:**
- Consumes: nothing.
- Produces: `changeScopePaths` containing every path this plan writes, so the dirty-baseline verifier stays clean from Task 2 onward.

- [ ] **Step 1: Write the failing assertion**

Append to `tests/p05_scope.test.mjs`:

```javascript
test('P05-E admits its implementation and evidence paths', () => {
  for (const path of [
    'crates/vestrace-infrastructure/tests/deployment_qualification.rs',
    'docs/development-evidence/v1-g0-05e-test-migrator.md',
    'tests/p05e_test_migrator.test.mjs',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
  // Every test file whose attribute this package rewrites must be admitted.
  const rewritten = changeScopePaths.filter(
    (path) => /^crates\/vestrace-(infrastructure|cli)\/tests\/.*\.rs$/.test(path),
  );
  assert.ok(rewritten.length >= 60, `only ${rewritten.length} test files admitted`);
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `node --test tests/p05_scope.test.mjs`
Expected: FAIL on `crates/vestrace-infrastructure/tests/deployment_qualification.rs`.

- [ ] **Step 3: Generate the admitted path list**

The 63 files are not typed by hand. Write `list-paths.mjs` at the repo root, run it, then delete it:

```javascript
import { execSync } from 'node:child_process';
const files = execSync('grep -rl \'migrations = "../../migrations"\' crates/*/tests/', { encoding: 'utf8' })
  .trim().split('\n').map((p) => p.replace(/\\/g, '/'));
const extra = [
  'crates/vestrace-infrastructure/tests/deployment_qualification.rs',
  'crates/vestrace-infrastructure/src/postgres/mod.rs',
  'docs/development-evidence/v1-g0-05e-test-migrator.md',
  'tests/p05e_test_migrator.test.mjs',
];
console.log([...new Set([...files, ...extra])].sort().map((p) => `  ${JSON.stringify(p)},`).join('\n'));
```

Merge that output into `changeScopePaths` in `scripts/p05-scope.mjs`, keeping the whole array sorted with plain ASCII ordering (`.sort()` with no comparator — `-` sorts before `/`). `crates/vestrace-infrastructure/src/postgres/pool.rs` is already admitted.

- [ ] **Step 4: Sync the preflight capture to the module, verbatim**

The verifier compares the two element by element, so write the module's array into the capture rather than editing it by hand. Create `sync-preflight.mjs` at the repo root, run it, then delete it:

```javascript
import fs from 'node:fs';
import { changeScopePaths } from './scripts/p05-scope.mjs';
const path = 'docs/development-evidence/v1-g0-05-preflight.json';
const lines = fs.readFileSync(path, 'utf8').split('\n');
const start = lines.findIndex((l) => l === '  "change_scope_paths": [');
let end = start;
while (lines[end] !== '  ],') end += 1;
const block = ['  "change_scope_paths": [',
  ...changeScopePaths.map((p) => `    ${JSON.stringify(p)},`), '  ],'];
block[block.length - 2] = block[block.length - 2].replace(/,$/, '');
const out = [...lines.slice(0, start), ...block, ...lines.slice(end + 1)].join('\n');
JSON.parse(out);
fs.writeFileSync(path, out);
console.log(`preflight now carries ${changeScopePaths.length} scope paths`);
```

- [ ] **Step 5: Record the amendment**

Add one entry to `scope_amendments` in the preflight, before the existing entries:

```json
{
  "authorized_at_utc": "2026-09-14T00:00:00.000Z",
  "authorized_by": "user standing authorization",
  "reason": "P05-E bounds the test schema at the historical 0001-0208 prefix. Every #[sqlx::test] suite has failed at migration 0209 since P05-A, so the database-backed regression net has been down for three packages.",
  "paths": ["see change_scope_paths entries under crates/vestrace-infrastructure/tests and crates/vestrace-cli/tests"]
}
```

- [ ] **Step 6: Update the count assertion and run the suite**

`tests/p05_scope.test.mjs` asserts an exact `changeScopePaths.length`. Set it to the new length printed in Step 4.

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS, all tests.

- [ ] **Step 7: Verify the baseline is still clean**

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: exit 0, no output.

---

### Task 2: Export the historical migrator and guard its bound

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/pool.rs`, `crates/vestrace-infrastructure/src/postgres/mod.rs`

**Interfaces:**
- Consumes: `bounded_migrator(version: i64) -> Result<sqlx::migrate::Migrator, InfrastructureError>` and `P05_HISTORY_PREFIX_VERSION: i64 = 208`, both already private in `pool.rs`.
- Produces: `vestrace_infrastructure::HISTORICAL_MIGRATOR`, a `std::sync::LazyLock<sqlx::migrate::Migrator>`. Tasks 3 and 4 name it in `#[sqlx::test(migrator = "...")]`.

- [ ] **Step 1: Write the failing guard test**

Add to the existing `mod tests` block at the bottom of `pool.rs`, and add `HISTORICAL_MIGRATOR` to that module's `use super::{...}` list:

```rust
#[test]
fn the_historical_migrator_stops_at_the_declared_prefix() {
    let versions: Vec<i64> = HISTORICAL_MIGRATOR.iter().map(|m| m.version).collect();

    assert!(!versions.is_empty(), "the historical migrator embedded nothing");
    assert_eq!(
        versions.iter().copied().max(),
        Some(P05_HISTORY_PREFIX_VERSION),
        "the historical migrator must end at the declared historical prefix",
    );
    // P05 assertions are applied only through the fixed three-phase route. A
    // test database that carried one would be a deployment nobody provisioned.
    assert!(
        versions.iter().all(|version| *version <= P05_HISTORY_PREFIX_VERSION),
        "the historical migrator carries a P05 assertion migration",
    );
    // Guarding only the head would pass a migrator with holes in it.
    assert_eq!(
        versions.len(),
        MIGRATOR.iter().filter(|m| m.version <= P05_HISTORY_PREFIX_VERSION).count(),
        "the historical migrator dropped a migration inside the prefix",
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p vestrace-infrastructure --lib the_historical_migrator_stops_at_the_declared_prefix`
Expected: FAIL to compile — `cannot find value HISTORICAL_MIGRATOR in this scope`.

- [ ] **Step 3: Add the static**

In `pool.rs`, immediately above `fn bounded_migrator`:

```rust
/// The migrations an ordinary test database may hold: `0001` through `0208`.
///
/// Migrations `0209` and later are not ordinary migrations. They assert that a
/// bootstrap-custody stage has already provisioned the P05 safety catalog, and
/// they are applied only through the fixed three-phase route. `#[sqlx::test]`
/// creates each database fresh from `template1` and applies every file it is
/// given, so pointing it at the whole directory asks for a provisioning step
/// that cannot have happened. This is the prefix that can.
pub static HISTORICAL_MIGRATOR: std::sync::LazyLock<sqlx::migrate::Migrator> =
    std::sync::LazyLock::new(|| {
        bounded_migrator(P05_HISTORY_PREFIX_VERSION)
            .expect("the historical prefix is embedded")
    });
```

In `crates/vestrace-infrastructure/src/postgres/mod.rs`, extend the existing re-export:

```rust
pub use pool::{HISTORICAL_MIGRATOR, PgGovernedMutationRepository, PgStore};
```

`crates/vestrace-infrastructure/src/lib.rs` already has `pub use postgres::*;`, so no change there.

- [ ] **Step 4: Run the guard**

Run: `cargo test -p vestrace-infrastructure --lib the_historical_migrator_stops_at_the_declared_prefix`
Expected: PASS.

- [ ] **Step 5: Prove the attribute accepts it end to end**

`sqlx::test`'s `migrator` argument takes `&'static Migrator`; a borrow of a `LazyLock` static deref-coerces to one. Confirm against the real cluster before rewriting 60 files on the assumption.

Create `crates/vestrace-infrastructure/tests/deployment_qualification.rs` with a single temporary test:

```rust
#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]
async fn the_bounded_migrator_drives_sqlx_test(pool: sqlx::PgPool) {
    let head: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(head, 208);
}
```

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test deployment_qualification`
Expected: PASS. Task 4 replaces this file's contents; the probe is scaffolding for this step only.

---

### Task 3: Switch every non-sensitive suite to the bounded migrator

**Files:**
- Modify: all 63 files matching `migrations = "../../migrations"` under `crates/vestrace-infrastructure/tests/` and `crates/vestrace-cli/tests/`

**Interfaces:**
- Consumes: `vestrace_infrastructure::HISTORICAL_MIGRATOR` from Task 2.
- Produces: a workspace in which no test applies the raw migrations directory. Task 4 audits what this breaks.

- [ ] **Step 1: Write the guard that will hold the boundary**

Create `tests/p05e_test_migrator.test.mjs`:

```javascript
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

// The filesystem, not `git ls-files`. This package is developed against a
// preserved dirty baseline and pushed through a temporary index, so a file this
// package creates stays untracked in the working tree indefinitely -- and an
// untracked file is exactly the new suite this guard exists to catch.
function testFiles() {
  const roots = [join(repoRoot, 'tests')];
  for (const crate of readdirSync(join(repoRoot, 'crates'), { withFileTypes: true })) {
    if (crate.isDirectory()) roots.push(join(repoRoot, 'crates', crate.name, 'tests'));
  }
  const found = [];
  for (const root of roots) {
    let entries;
    try {
      entries = readdirSync(root, { withFileTypes: true, recursive: true });
    } catch {
      continue; // a crate with no tests directory
    }
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.rs')) continue;
      const dir = entry.parentPath ?? entry.path;
      found.push(relative(repoRoot, join(dir, entry.name)).split('\\').join('/'));
    }
  }
  return found.sort();
}

// Migrations 0209 and later assert a provisioning step that #[sqlx::test]
// cannot perform, because it builds each database fresh from template1. A
// suite pointed at the raw directory fails at 0209 before its first assertion
// -- which is how the whole database-backed net stayed down for three packages
// without anyone noticing.
test('no test applies the raw migrations directory', () => {
  const offenders = testFiles().filter((file) =>
    readFileSync(join(repoRoot, file), 'utf8').includes('migrations = "../../migrations"'),
  );
  assert.deepEqual(offenders, [], `these suites would fail at migration 0209: ${offenders}`);
});

test('the bounded migrator is named by its exported path', () => {
  const users = testFiles().filter((file) =>
    readFileSync(join(repoRoot, file), 'utf8').includes('migrator = '),
  );
  assert.ok(users.length >= 60, `only ${users.length} suites use the bounded migrator`);
  for (const file of users) {
    assert.match(
      readFileSync(join(repoRoot, file), 'utf8'),
      /migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR"/,
      `${file} names a migrator this package does not export`,
    );
  }
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `node --test tests/p05e_test_migrator.test.mjs`
Expected: FAIL, listing 63 offending files.

- [ ] **Step 2a: Prove the guard sees an untracked file**

The guard would be worthless if it inspected only tracked files, because every
file this package creates stays untracked in the preserved dirty baseline.
Create `crates/vestrace-infrastructure/tests/zz_guard_probe.rs` containing the
single line `// migrations = "../../migrations"`, run the guard, and confirm the
probe appears in the offender list. Delete the probe afterwards.

- [ ] **Step 3: Rewrite the attribute everywhere**

592 occurrences across 63 files: script it, do not hand-edit. Create `rewrite.mjs` at the repo root, run it, then delete it:

```javascript
import fs from 'node:fs';
import { execSync } from 'node:child_process';
const files = execSync('grep -rl \'migrations = "../../migrations"\' crates/*/tests/', { encoding: 'utf8' })
  .trim().split('\n');
let total = 0;
for (const file of files) {
  const before = fs.readFileSync(file, 'utf8');
  const after = before.replaceAll(
    'migrations = "../../migrations"',
    'migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR"',
  );
  total += before.split('migrations = "../../migrations"').length - 1;
  fs.writeFileSync(file, after);
}
console.log(`rewrote ${total} attributes across ${files.length} files`);
```

Expected output: `rewrote 592 attributes across 63 files`. If either number differs, stop and find out why before continuing.

- [ ] **Step 4: Run the guard**

Run: `node --test tests/p05e_test_migrator.test.mjs`
Expected: PASS, both tests.

- [ ] **Step 5: Confirm the workspace still compiles**

`vestrace-cli` already depends on `vestrace-infrastructure`, so no manifest change is needed.

Run: `cargo test --workspace --no-run`
Expected: compiles. Any `unresolved import` means a crate lacks the dependency — add it rather than reverting the attribute.

- [ ] **Step 6: Prove a previously blocked suite now reaches its assertions**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test embedding_schema_contract --no-fail-fast`
Expected: the 0209 error is gone. Tests now pass or fail on their own assertions. Record the counts; do not fix failures here.

---

### Task 4: Audit the twenty-nine migration-sensitive tests

**Files:**
- Modify: `crates/vestrace-infrastructure/tests/postgres.rs`, `crates/vestrace-cli/tests/runtime_schema_gate.rs`, `crates/vestrace-cli/tests/v1_release_gate_cli.rs`, `crates/vestrace-cli/tests/recovery_qualification_cli_contract.rs`, `crates/vestrace-infrastructure/tests/fingerprint_continuity_is_fail_closed.rs`, `crates/vestrace-infrastructure/tests/row_level_security.rs`
- Create: `crates/vestrace-infrastructure/tests/deployment_qualification.rs` (replacing the Task 2 probe)

**Interfaces:**
- Consumes: the rewritten attributes from Task 3.
- Produces: a classification of every one of the twenty-nine as `safe`, `prepared`, or `vacuous`, recorded in Task 5's evidence.

**The candidate list**, from the spec. Bounding changes meaning for two reasons: the test reads or writes the ledger, or it launches a `vestrace` process whose startup gate requires the applied ledger to equal the complete migrator.

| File | Tests |
| --- | --- |
| `v1_release_gate_cli.rs` | 15, all launch the product |
| `postgres.rs` | 8, all ledger |
| `runtime_schema_gate.rs` | 3 |
| `recovery_qualification_cli_contract.rs` | 1, launches the product |
| `fingerprint_continuity_is_fail_closed.rs` | 1, `HealthRepository::check` |
| `row_level_security.rs` | 1, ledger named only in a comment |

- [ ] **Step 1: Classify empirically, not from the patterns**

Both properties were derived by pattern matching and the second exists only because the first proved insufficient. Run each file and read the failures:

```
DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test \
  -p vestrace-infrastructure --test postgres --test fingerprint_continuity_is_fail_closed \
  --test row_level_security --no-fail-fast
DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test \
  -p vestrace-cli --test runtime_schema_gate --test v1_release_gate_cli \
  --test recovery_qualification_cli_contract --no-fail-fast
```

A test that fails needs a complete deployment: classify `prepared`. A test that passes is not yet cleared — go to Step 2.

- [ ] **Step 2: Test the passing ones for vacuity**

A passing test in this set may be passing for the wrong reason. Four in `postgres.rs` are known to: `health_check_rejects_missing_migration`, `health_check_rejects_unsuccessful_migration`, `health_check_rejects_extra_migration`, and `health_check_rejects_same_count_version_mismatch` each damage the ledger and assert the health check refuses. `HealthRepository::check` compares the applied ledger against the complete migrator, so at 0208 it already refuses before the damage.

For each passing candidate, comment out the line that performs the damage and re-run it. If it still passes, it is `vacuous` — it asserts nothing. Restore the line and classify it `prepared`.

Expected: the four named above are vacuous. `row_level_security::a_table_holding_a_workspace_id_has_row_level_security` names `_sqlx_migrations` only inside a comment, and `runtime_schema_gate::runtime_role_can_read_administratively_applied_migration_history` only counts rows as the runtime role — both are `safe`.

- [ ] **Step 3: Move the `postgres.rs` tests to a prepared database**

Replace the Task 2 probe in `crates/vestrace-infrastructure/tests/deployment_qualification.rs` with the eight `postgres.rs` tests classified `prepared` or `vacuous`. Delete them from `postgres.rs`; its remaining tests keep the bounded migrator.

Use the pattern `installation_safety_authority.rs` already establishes:

```rust
//! Deployment qualification against a completely migrated database.
//!
//! These assert properties of a finished deployment: that the applied ledger
//! equals the embedded migrator, and that the health check refuses when it does
//! not. A database bounded at the historical prefix cannot answer either
//! question -- it is already incompatible, so a refusal proves nothing.

use sqlx::{PgPool, postgres::PgPoolOptions};

async fn deployment_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("VESTRACE_P05_TEST_DATABASE_URL") else {
        eprintln!(
            "BLOCKED: set VESTRACE_P05_TEST_DATABASE_URL to a disposable database \
             taken through the three-phase P05 route"
        );
        return None;
    };
    Some(PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap())
}

#[tokio::test]
async fn health_check_accepts_reachable_database_with_compatible_migrations() {
    let Some(pool) = deployment_pool().await else {
        return;
    };
    let store = vestrace_infrastructure::PgStore::from_pool(pool);
    assert!(vestrace_application::HealthRepository::check(&store).await.is_ok());
}
```

Carry the remaining seven across unchanged apart from the attribute and the pool acquisition. Each keeps its original name so the evidence can be matched to it.

- [ ] **Step 4: Move the product-launching CLI tests the same way**

The sixteen tests in `v1_release_gate_cli.rs` and `recovery_qualification_cli_contract.rs`, and the two in `runtime_schema_gate.rs`, launch `vestrace` against the test database. Convert each in place from `#[sqlx::test(migrator = ...)]` to `#[tokio::test]` taking its pool from `deployment_pool()` above, copied into each file rather than shared — these are separate test binaries and a shared helper would need a `common` module they do not have.

- [ ] **Step 5: Prove the moved rejection tests fail for their stated reason**

This is the point of the exercise. Provision a database through the three-phase route (Task 6 records how), point `VESTRACE_P05_TEST_DATABASE_URL` at it, and for each of the four `health_check_rejects_*` tests confirm that the health check passes *before* the damage and refuses *after* it.

Run: `VESTRACE_P05_TEST_DATABASE_URL=<prepared> cargo test -p vestrace-infrastructure --test deployment_qualification -- --nocapture`
Expected: PASS, with no `BLOCKED:` line in the output.

- [ ] **Step 6: Confirm the blocked path is honest**

Run the same command with `VESTRACE_P05_TEST_DATABASE_URL` unset.
Expected: PASS with a `BLOCKED:` line per test. Record that these report green while proving nothing — that is why Task 5 counts them separately.

---

### Task 5: Measure and record the real red-test figure

**Files:**
- Create: `docs/development-evidence/v1-g0-05e-test-migrator.md`

**Interfaces:**
- Consumes: everything above.
- Produces: the measured figure. No repairs.

- [ ] **Step 1: Run the workspace**

The run is slow — one suite has previously taken about forty-five minutes. Run it in the background and start no other cargo command against the same target directory while it runs.

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test --workspace --no-fail-fast`

- [ ] **Step 2: Record it, suite by suite**

Write `docs/development-evidence/v1-g0-05e-test-migrator.md` with: the before state (63 files, 590 tests, all failing at 0209); the classification of all twenty-nine migration-sensitive tests with the reason for each; the after state as a table of suite, passed, failed, blocked; and a named list of every genuine failure as follow-up work.

Record `postgres.rs::store_migrate_applies_embedded_migrations` as a pre-existing failure P05-E does not repair: it declares a bare `#[sqlx::test]`, applies nothing, and calls `store.migrate()`, which runs the complete migrator and fails at 0209 today.

Count blocked suites separately from passing ones and say so in the table, so a blocked run is never read as coverage.

- [ ] **Step 3: State what is not claimed**

Record explicitly: P05-E makes no G0 claim, repairs no failure it exposes, and changes no production migration behaviour. The aggregate G0 result remains whatever `scripts/p05-g0-gate.mjs` emits.

---

### Task 6: Qualify P05-E

**Files:**
- Modify: `docs/development-evidence/v1-g0-05e-test-migrator.md`

- [ ] **Step 1: Run the full gate set**

```
node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings
git diff --check
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs
```

Expected: every one exits 0.

- [ ] **Step 2: Record how the prepared database was provisioned**

Document the exact route, so the blocked tests can be run by anyone: build the image from `docker/postgres`, start it with `init-runtime-role.sh` mounted at `/docker-entrypoint-initdb.d/10-runtime-role.sh` (the Dockerfile does not bake it, and without the mount the container logs `ignoring /docker-entrypoint-initdb.d/*` and provisions nothing), then `migrate --through-version 208`, the bootstrap psql stage, then `migrate --only-version` for 209 through 215 one at a time.

- [ ] **Step 3: Confirm the G0 result is unchanged**

Run: `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`
Expected: exit 1, aggregate `blocked`, 1 pass / 1 blocked / 17 unknown — identical to P05-D. P05-E closes no G0 criterion, and a changed result here means something was claimed that should not have been.

- [ ] **Step 4: Mark P05-E complete only if every check above passed**

Record actual exits and counts. Report the red-test figure exactly as measured.

---

## Self-review

- Spec coverage: the historical prefix decision is Task 2; the attribute switch Task 3; the twenty-nine-test audit and the vacuity hazard Task 4; both guards Tasks 2 and 3; measurement Task 5; the evidence list Task 6.
- The four `health_check_rejects_*` tests have a dedicated step proving they fail for their stated reason, which is evidence item 5 in the spec.
- `store_migrate_applies_embedded_migrations` is named as pre-existing and out of scope, matching the spec.
- Type consistency: `HISTORICAL_MIGRATOR` and `deployment_pool()` are spelled identically in Tasks 2, 3 and 4.

## Execution handoff

Execute Task 1 first. Do not repair any failure Task 5 exposes; record it. Stop before any package beyond P05-E.
