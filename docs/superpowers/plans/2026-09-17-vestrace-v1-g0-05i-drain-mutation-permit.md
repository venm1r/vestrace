# P05-I DrainMutationPermit / Quiescing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `DrainMutationPermit`/`Quiescing`, which does not exist anywhere in the repository today, and close G0 criteria g0-12, g0-13, and the remaining `DrainMutationPermit`/"no post-freeze write" conjunct of g0-10.

**Architecture:** Reuse the existing `InstallationMutationPermit::Exclusive` lock unchanged. Add one new domain/application/infrastructure module triple (`installation_drain`) parallel to `installation_safety`. Snapshot the pre-Quiescing intent set by stamping a nullable `drained_by` column on the existing `material_key_creation_intents`/`credential_key_creation_intents` tables at drain-request time — no new join table, no live recount. Add one guard clause to each existing `reserve` SQL function. Reconciliation only observes; it never drives a new per-intent transition — every one of those already exists and is evidenced under g0-08/g0-09.

**Tech Stack:** Rust 1.85, SQLx 0.8, PostgreSQL 17.

**Spec:** `docs/superpowers/specs/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit-design.md` (approved design, this plan's authority), which argues from `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` lines 944 (g0-12), 945 (g0-13), part of 942 (g0-10).

## Global Constraints

- `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test`, `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test`. Use `127.0.0.1`, never `localhost`.
- Do not modify migrations `0001`–`0215`, `scripts/verify-dirty-baseline.mjs`, or any path in `protectedAuthorityPaths`. The new migration is `0216`, numbered after the existing tip.
- Do not weaken migration `0209` or any P05 safety-bootstrap migration. `bounded_migrator`'s existing behavior and `HISTORICAL_MIGRATOR`'s existing bound (208) must not change — only a new function and a new static are added alongside them.
- No CLI surface. Domain + application + infrastructure only, confirmed with the operator.
- No new per-intent state or transition. Every material/credential intent transition this package touches already exists; this package only adds a precondition check to `reserve` and a read-only reconciliation.
- Never trust a combined `cargo test --test A --test B` run's positional output for per-suite counts (P05-F finding). Verify each new test standalone before citing it as evidence.

---

## File structure

| Path | Responsibility |
| --- | --- |
| `scripts/p05-scope.mjs` | Admits this plan's own path and every file below. |
| `migrations/0216_installation_drain_request.sql` | New table, new columns, guard clauses in both `reserve` functions. |
| `crates/vestrace-infrastructure/src/postgres/pool.rs` | `bounded_migrator_excluding`, `DRAIN_HISTORICAL_MIGRATOR`, guard test. |
| `crates/vestrace-infrastructure/src/postgres/mod.rs` | Re-exports `DRAIN_HISTORICAL_MIGRATOR` and `PgDrainMutationPermitRepository`. |
| `crates/vestrace-domain/src/installation_drain.rs` | `InstallationDrainRequestId`, `InstallationDrainRequest`, pre-Quiescing classification. |
| `crates/vestrace-domain/src/lib.rs` | `pub mod installation_drain;` + re-export. |
| `crates/vestrace-application/src/ports.rs` | `DrainMutationPermitRepository` trait. |
| `crates/vestrace-application/src/lib.rs` | Re-export. |
| `crates/vestrace-infrastructure/src/postgres/installation_drain.rs` | `PgDrainMutationPermitRepository`. |
| `crates/vestrace-infrastructure/tests/installation_drain_request.rs` | New PostgreSQL contract suite: concurrency, snapshot exactness, reserve refusal, reconcile-to-Frozen. |
| `crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs` | Extended with drain-aware crash-boundary tests, reusing existing `material_at`/`credential_at`/`resume_material`/`resume_credential`/`CountingVault` helpers. |
| `docs/development-evidence/v1-g0-05-gate.json` | g0-12, g0-13 moved to `pass`; g0-10 updated to close its `DrainMutationPermit` conjunct (still `blocked` on the browser conjunct alone). |
| `docs/development-evidence/v1-g0-05i-drain-mutation-permit.md` | Evidence doc. |

---

### Task 1: Admit the P05-I implementation scope

**Files:**
- Modify: `scripts/p05-scope.mjs`, `tests/p05_scope.test.mjs`, `docs/development-evidence/v1-g0-05-preflight.json`

- [ ] **Step 1: Write the failing assertion**

Append to `tests/p05_scope.test.mjs`:

```javascript
test('P05-I admits its own plan and implementation paths', () => {
  for (const path of [
    'docs/superpowers/plans/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit.md',
    'migrations/0216_installation_drain_request.sql',
    'crates/vestrace-domain/src/installation_drain.rs',
    'crates/vestrace-application/src/installation_drain.rs',
    'crates/vestrace-infrastructure/src/postgres/installation_drain.rs',
    'crates/vestrace-infrastructure/tests/installation_drain_request.rs',
    'docs/development-evidence/v1-g0-05i-drain-mutation-permit.md',
  ]) {
    assert.ok(changeScopePaths.includes(path), path);
    assert.ok(!protectedAuthorityPaths.includes(path), path);
  }
});
```

Note: `crates/vestrace-domain/src/lib.rs`, `crates/vestrace-application/src/lib.rs`, `crates/vestrace-application/src/ports.rs`, `crates/vestrace-infrastructure/src/postgres/pool.rs`, `crates/vestrace-infrastructure/src/postgres/mod.rs`, and `crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs` are already admitted (confirm with `node -e "import('./scripts/p05-scope.mjs').then(m=>console.log(m.changeScopePaths.includes('crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs')))"` before assuming — if any is missing, add it in Step 3 below instead of assuming).

- [ ] **Step 2: Run it and watch it fail**

Run: `node --test tests/p05_scope.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Add the seven new paths to `changeScopePaths`, sorted**

Plain ASCII `.sort()` order (`-` before `/`, digits before letters).

- [ ] **Step 4: Sync the preflight capture to the module, verbatim**

Disposable `sync-preflight.mjs` script (write at repo root, run, delete), same as every prior P05 sub-package. Confirm the printed count is `175 + 7 = 182`.

- [ ] **Step 5: Record the amendment**

One `scope_amendments` entry naming the seven paths and this plan.

- [ ] **Step 6: Update the count assertion and run the suite**

Change `assert.equal(changeScopePaths.length, 175);` to `182`.

Run: `node --test tests/p05_scope.test.mjs`
Expected: PASS, all tests.

- [ ] **Step 7: Verify the baseline is still clean**

Run: `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs`
Expected: exit 0.

---

### Task 2: Generalize `bounded_migrator` and add `DRAIN_HISTORICAL_MIGRATOR`

**Files:**
- Modify: `crates/vestrace-infrastructure/src/postgres/pool.rs`, `crates/vestrace-infrastructure/src/postgres/mod.rs`

**Interfaces:**
- Consumes: `MIGRATOR` (existing full embedded migrator), the seven existing `P05_*_ASSERTION_MIGRATION_VERSION` constants.
- Produces: `vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR`, a `LazyLock<Migrator>` bounded at `216`, excluding versions `209`–`215`. Task 4's tests and Task 7's new suite name it.

- [ ] **Step 1: Write the failing guard test**

Add to the existing `mod tests` block in `pool.rs` (extend its `use super::{...}` list with `DRAIN_HISTORICAL_MIGRATOR`, `P05_DRAIN_PREFIX_VERSION`):

```rust
#[test]
fn the_drain_historical_migrator_extends_the_historical_prefix_excluding_p05_assertions() {
    let versions: Vec<i64> = DRAIN_HISTORICAL_MIGRATOR.iter().map(|m| m.version).collect();

    assert!(!versions.is_empty(), "the drain historical migrator embedded nothing");
    assert_eq!(
        versions.iter().copied().max(),
        Some(P05_DRAIN_PREFIX_VERSION),
        "the drain historical migrator must end at its declared prefix",
    );
    for excluded in [
        P05_SAFETY_ASSERTION_MIGRATION_VERSION,
        P05_ARCHIVE_ASSERTION_MIGRATION_VERSION,
        P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
        P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
        P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
        P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
        P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION,
    ] {
        assert!(
            !versions.contains(&excluded),
            "the drain historical migrator must not carry P05 assertion migration {excluded}",
        );
    }
    // Every migration in [1, 216] except the seven excluded P05 assertions.
    let expected = MIGRATOR
        .iter()
        .filter(|m| {
            m.version <= P05_DRAIN_PREFIX_VERSION
                && ![
                    P05_SAFETY_ASSERTION_MIGRATION_VERSION,
                    P05_ARCHIVE_ASSERTION_MIGRATION_VERSION,
                    P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
                    P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
                    P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
                    P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
                    P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION,
                ]
                .contains(&m.version)
        })
        .count();
    assert_eq!(versions.len(), expected, "the drain historical migrator dropped or added a migration");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p vestrace-infrastructure --lib the_drain_historical_migrator_extends_the_historical_prefix_excluding_p05_assertions`
Expected: FAIL to compile — `cannot find value DRAIN_HISTORICAL_MIGRATOR` / `P05_DRAIN_PREFIX_VERSION`.

- [ ] **Step 3: Add the constant, the generalized function, and the static**

In `pool.rs`, immediately after the existing `P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION` constant:

```rust
const P05_DRAIN_PREFIX_VERSION: i64 = 216;
const P05_ASSERTION_MIGRATION_VERSIONS: [i64; 7] = [
    P05_SAFETY_ASSERTION_MIGRATION_VERSION,
    P05_ARCHIVE_ASSERTION_MIGRATION_VERSION,
    P05_BASE_CAPTURE_ASSERTION_MIGRATION_VERSION,
    P05_RESTORE_CUTOVER_ASSERTION_MIGRATION_VERSION,
    P05_RESTORE_REFUSAL_ASSERTION_MIGRATION_VERSION,
    P05_RESTORE_SAFETY_EVENT_ASSERTION_MIGRATION_VERSION,
    P05_SAFETY_READINESS_ASSERTION_MIGRATION_VERSION,
];
```

Immediately after the existing `fn bounded_migrator`:

```rust
/// Like `bounded_migrator`, but excludes specific versions inside the bound
/// rather than only cutting at the top. `bounded_migrator` cannot express "up
/// through 216, except the fixed P05 assertion migrations 209-215" -- a plain
/// `<= version` filter would re-admit them. This is that generalization; it
/// does not change `bounded_migrator`'s own behavior or any existing caller.
fn bounded_migrator_excluding(
    upper: i64,
    excluded: &[i64],
) -> Result<sqlx::migrate::Migrator, InfrastructureError> {
    if !MIGRATOR.version_exists(upper) {
        return Err(InfrastructureError::configuration(
            "requested bounded migration version is not embedded",
        ));
    }
    Ok(sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= upper && !excluded.contains(&migration.version))
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    })
}

/// The migrations an ordinary test database may hold when it needs
/// `installation_drain_requests` (migration 216): the historical 0001-0208
/// prefix, plus 216 itself, excluding the P05 safety-bootstrap assertions
/// 209-215 for the same reason `HISTORICAL_MIGRATOR` excludes them --
/// `#[sqlx::test]` cannot have provisioned the P05 safety catalog.
pub static DRAIN_HISTORICAL_MIGRATOR: std::sync::LazyLock<sqlx::migrate::Migrator> =
    std::sync::LazyLock::new(|| {
        bounded_migrator_excluding(P05_DRAIN_PREFIX_VERSION, &P05_ASSERTION_MIGRATION_VERSIONS)
            .expect("the drain historical prefix is embedded")
    });
```

In `crates/vestrace-infrastructure/src/postgres/mod.rs`, extend the existing re-export line that carries `HISTORICAL_MIGRATOR`:

```rust
pub use pool::{DRAIN_HISTORICAL_MIGRATOR, HISTORICAL_MIGRATOR, PgGovernedMutationRepository, PgStore};
```

- [ ] **Step 4: Run the guard**

Run each name separately, not combined — `DRAIN_HISTORICAL_MIGRATOR` is a `LazyLock` whose `.expect(...)` panics on first access when migration 216 is not embedded, so its own guard test cannot pass until Task 3 lands, exactly like Step 5's probe below (this plan originally said "both PASS" here, which is impossible before Task 3 exists — corrected during Task 2's review):

Run: `cargo test -p vestrace-infrastructure --lib the_historical_migrator_stops_at_the_declared_prefix`
Expected: PASS — confirms the existing static (`HISTORICAL_MIGRATOR`, bounded at 208) is unchanged.

Run: `cargo test -p vestrace-infrastructure --lib the_drain_historical_migrator_extends_the_historical_prefix_excluding_p05_assertions`
Expected: FAILS (panics) with `"requested bounded migration version is not embedded"` — the same expected-failure shape as Step 5, for the same reason. Task 3 makes this pass.

- [ ] **Step 5: Prove the new migrator drives `#[sqlx::test]` before migration 0216 exists**

This step's probe fails until Task 3 adds migration 0216 — that is expected and correct; it proves the plumbing before the schema exists, exactly as P05-E's Task 2 Step 5 did for `HISTORICAL_MIGRATOR`. Create `crates/vestrace-infrastructure/tests/installation_drain_request.rs` with a single temporary test:

```rust
#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn the_drain_migrator_reaches_216(pool: sqlx::PgPool) {
    let head: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(head, 216);
}
```

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test installation_drain_request`
Expected: FAILS with a migration error naming version 216 as not embedded (there is no `migrations/0216_*.sql` file yet). Task 3 makes this pass; Task 7 replaces this probe's contents with the real contract suite.

---

### Task 3: Migration 0216 — the drain table, snapshot columns, and reserve guards

**Files:**
- Create: `migrations/0216_installation_drain_request.sql`

**Interfaces:**
- Consumes: nothing (new schema).
- Produces: `installation_drain_requests` table; `drained_by` column on `material_key_creation_intents` and `credential_key_creation_intents`; a guard clause added to `vestrace_reserve_material_key_creation_intent` and `vestrace_reserve_credential_key_creation_intent` via `CREATE OR REPLACE FUNCTION` (their bodies are otherwise unchanged from `migrations/0169_material_key_creation_intents.sql` and `migrations/0173_credential_key_creation_intents.sql`).

- [ ] **Step 1: Write the migration**

```sql
-- DrainMutationPermit / Quiescing: an installation-wide request to stop
-- admitting new provisional-key work and wait until every intent that was
-- already in flight reaches a state that can survive indefinitely, using only
-- the resume/abort logic those intents already have. This table is
-- installation-wide, not workspace-scoped -- no RLS, matching the convention
-- `installation_fingerprint_continuity` (migration 0167) already established
-- for genuinely installation-wide state.
CREATE TABLE installation_drain_requests (
    id UUID PRIMARY KEY,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT installation_drain_requests_completed_after_requested
        CHECK (completed_at IS NULL OR completed_at >= requested_at)
);

-- Exactly one undrained (completed_at IS NULL) request may exist at a time.
-- A drain request that reserve() must see and refuse against relies on this
-- being a real database constraint, not an application-level check that a
-- concurrent transaction could race past.
CREATE UNIQUE INDEX installation_drain_requests_one_active
    ON installation_drain_requests ((true))
    WHERE completed_at IS NULL;

ALTER TABLE material_key_creation_intents
    ADD COLUMN drained_by UUID REFERENCES installation_drain_requests(id);

ALTER TABLE credential_key_creation_intents
    ADD COLUMN drained_by UUID REFERENCES installation_drain_requests(id);

-- Pre-Quiescing states per docs/superpowers/specs/2026-09-17-vestrace-v1-g0-05i-drain-mutation-permit-design.md
-- section 2. Bound and every terminal state are never stamped: g0-13 --
-- "Bound never abandons" -- and a terminal intent needs no draining.
CREATE OR REPLACE FUNCTION vestrace_request_installation_drain(
    target_request_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM installation_drain_requests WHERE completed_at IS NULL) THEN
        RAISE EXCEPTION 'an installation drain is already active' USING ERRCODE = '55000';
    END IF;

    INSERT INTO installation_drain_requests (id) VALUES (target_request_id);

    UPDATE material_key_creation_intents
       SET drained_by = target_request_id
     WHERE state IN ('reserved', 'provisional_created', 'provisional_receipted',
                      'content_prepared', 'result_prepared')
       AND drained_by IS NULL;

    UPDATE credential_key_creation_intents
       SET drained_by = target_request_id
     WHERE state IN ('reserved', 'provisional_created', 'provisional_receipted',
                      'credential_prepared')
       AND drained_by IS NULL;
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reconcile_installation_drain(
    target_request_id UUID
)
RETURNS BOOLEAN
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    pending BIGINT;
BEGIN
    PERFORM 1 FROM installation_drain_requests WHERE id = target_request_id AND completed_at IS NULL
        FOR UPDATE;
    IF NOT FOUND THEN
        -- Already Frozen (or the id is unknown) is not an error: reconcile is
        -- idempotent and safe to call after a crash or a repeat call.
        RETURN EXISTS (
            SELECT 1 FROM installation_drain_requests
             WHERE id = target_request_id AND completed_at IS NOT NULL
        );
    END IF;

    SELECT
        (SELECT COUNT(*) FROM material_key_creation_intents
          WHERE drained_by = target_request_id
            AND state IN ('reserved', 'provisional_created', 'provisional_receipted',
                           'content_prepared', 'result_prepared'))
        +
        (SELECT COUNT(*) FROM credential_key_creation_intents
          WHERE drained_by = target_request_id
            AND state IN ('reserved', 'provisional_created', 'provisional_receipted',
                           'credential_prepared'))
      INTO pending;

    IF pending = 0 THEN
        UPDATE installation_drain_requests SET completed_at = NOW() WHERE id = target_request_id;
        RETURN TRUE;
    END IF;
    RETURN FALSE;
END
$$;

-- Widens the existing reserve functions with one precondition: refuse a new
-- Reserved intent while a drain is active. Every other line is copied
-- unchanged from migrations/0169 and 0173 respectively -- this is a
-- CREATE OR REPLACE, the established pattern this schema already uses to
-- widen a guarded function in a later migration (e.g. migration 0174 widened
-- vestrace_record_unbound_material_key_erasure the same way).
CREATE OR REPLACE FUNCTION vestrace_reserve_material_key_creation_intent(
    target_intent_id UUID,
    target_workspace_id UUID,
    target_material_id UUID,
    target_material_key_id UUID,
    target_nonce UUID,
    target_owner_kind TEXT,
    target_owner_id UUID,
    target_output_ordinal BIGINT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM installation_drain_requests WHERE completed_at IS NULL) THEN
        RAISE EXCEPTION 'the installation is draining; no new material key creation intent may be reserved'
            USING ERRCODE = '55000';
    END IF;
    PERFORM vestrace_assert_material_intent_workspace(target_workspace_id);

    INSERT INTO material_key_creation_intents (
        id,
        workspace_id,
        material_id,
        material_key_id,
        nonce,
        owner_kind,
        owner_id,
        output_ordinal,
        state
    )
    VALUES (
        target_intent_id,
        target_workspace_id,
        target_material_id,
        target_material_key_id,
        target_nonce,
        target_owner_kind,
        target_owner_id,
        target_output_ordinal,
        'reserved'
    );
END
$$;

CREATE OR REPLACE FUNCTION vestrace_reserve_credential_key_creation_intent(
    target_intent_id UUID,
    target_workspace_id UUID,
    target_connection_id UUID,
    target_slot_id UUID,
    target_occupancy_id UUID,
    target_revision_id UUID,
    target_material_key_id UUID,
    target_nonce UUID,
    target_associated_data_profile TEXT
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    occupancy_row credential_guard_occupancies%ROWTYPE;
BEGIN
    IF EXISTS (SELECT 1 FROM installation_drain_requests WHERE completed_at IS NULL) THEN
        RAISE EXCEPTION 'the installation is draining; no new credential key creation intent may be reserved'
            USING ERRCODE = '55000';
    END IF;
    PERFORM vestrace_acquire_credential_lock_chain(
        target_workspace_id,
        target_connection_id,
        target_slot_id,
        ARRAY[
            'connection_execution_guard',
            'credential_activation_guard',
            'credential_slot',
            'revision_material'
        ]::TEXT[]
    );
    SELECT * INTO occupancy_row
      FROM credential_guard_occupancies
     WHERE id = target_occupancy_id
       AND workspace_id = target_workspace_id
       AND connection_id = target_connection_id
       AND credential_slot_id = target_slot_id
     FOR UPDATE;
    IF NOT FOUND OR occupancy_row.state <> 'preparing' THEN
        RAISE EXCEPTION 'credential intent requires a Preparing association'
            USING ERRCODE = '23514';
    END IF;
    IF occupancy_row.intent_id IS NOT NULL THEN
        RAISE EXCEPTION 'credential association already has an intent'
            USING ERRCODE = '23505';
    END IF;
    IF target_associated_data_profile NOT IN ('credential_v2', 'legacy_v1') THEN
        RAISE EXCEPTION 'credential associated-data profile is invalid' USING ERRCODE = '23514';
    END IF;

    -- Allocate this immutable identity before any caller can encrypt.
    INSERT INTO credential_revisions (
        id, workspace_id, credential_slot_id, material_key_id, associated_data_profile
    ) VALUES (
        target_revision_id, target_workspace_id, target_slot_id,
        target_material_key_id, target_associated_data_profile
    );
    INSERT INTO credential_key_creation_intents (
        id, workspace_id, connection_id, credential_slot_id, occupancy_id,
        credential_revision_id, material_key_id, nonce, state
    ) VALUES (
        target_intent_id, target_workspace_id, target_connection_id, target_slot_id,
        target_occupancy_id, target_revision_id, target_material_key_id, target_nonce, 'reserved'
    );
    UPDATE credential_guard_occupancies
       SET intent_id = target_intent_id, updated_at = NOW()
     WHERE id = target_occupancy_id;
END
$$;

-- Hand installation_drain_requests and the two new functions to the guarded
-- owner, exactly as every migration since 0176 has for its own new guarded
-- objects: try the standing (but necessarily pre-0216) allowlisted helper
-- first, and fall back to a direct grant only when running as the SQLx
-- fresh-database superuser, which never runs the Compose provisioner that
-- would otherwise extend the real allowlist. Without this, the drain-guard
-- check the two widened reserve functions above just added would fail with
-- "permission denied for table installation_drain_requests" for every
-- caller, drain active or not -- confirmed by Task 7's own PostgreSQL suite.
--
-- Production upgrades run as the restricted runtime role and use the exact
-- bootstrap allowlist, which docker/postgres/init-runtime-role.sh extends for
-- this migration. The provisioner re-runs before the migrator on every start,
-- so an existing deployment acquires the extended allowlist before this
-- executes. SQLx fresh databases are provisioned by a superuser and never run
-- the Compose bootstrap, which is what the narrow fallback below is for.
DO $$
BEGIN
    PERFORM vestrace_assign_p03_table_owner('installation_drain_requests'::REGCLASS);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER TABLE installation_drain_requests OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON TABLE installation_drain_requests FROM PUBLIC;
    REVOKE ALL ON TABLE installation_drain_requests FROM vestrace;
    GRANT SELECT ON TABLE installation_drain_requests TO vestrace;
END
$$;

DO $$
BEGIN
    PERFORM vestrace_assign_p03_function_owner('vestrace_request_installation_drain(UUID)'::REGPROCEDURE);
    PERFORM vestrace_assign_p03_function_owner('vestrace_reconcile_installation_drain(UUID)'::REGPROCEDURE);
EXCEPTION WHEN insufficient_privilege THEN
    IF NOT COALESCE((SELECT rolsuper FROM pg_roles WHERE rolname = current_user), FALSE) THEN
        RAISE;
    END IF;
    ALTER FUNCTION vestrace_request_installation_drain(UUID) OWNER TO vestrace_guarded_owner;
    ALTER FUNCTION vestrace_reconcile_installation_drain(UUID) OWNER TO vestrace_guarded_owner;
    REVOKE ALL ON FUNCTION vestrace_request_installation_drain(UUID) FROM PUBLIC;
    REVOKE ALL ON FUNCTION vestrace_reconcile_installation_drain(UUID) FROM PUBLIC;
    GRANT EXECUTE ON FUNCTION vestrace_request_installation_drain(UUID) TO vestrace;
    GRANT EXECUTE ON FUNCTION vestrace_reconcile_installation_drain(UUID) TO vestrace;
END
$$;
```

**Correction made during Task 3's review:** the block above originally dropped the `-- Allocate this immutable identity before any caller can encrypt.` comment that migration 0173's original function carries immediately before `INSERT INTO credential_revisions` — a plan-authoring transcription slip, not a logic change. Restored above so the widened function is genuinely byte-for-byte identical to the original beyond the one added guard block.

**Second correction made during Task 7's execution:** the migration as originally planned created `installation_drain_requests` and the two new functions with no ownership/grant statements at all, leaving them owned by whichever role runs the migration rather than `vestrace_guarded_owner`. Since the two widened reserve functions (owned by `vestrace_guarded_owner`, unchanged) now read `installation_drain_requests` inside a `SECURITY DEFINER` body, every `reserve()` call failed with `permission denied for table installation_drain_requests`, drain active or not. Root cause and fix (the ownership-assignment DO blocks above, following the exact pattern every migration since 0176 uses for its own new guarded objects) confirmed via `crates/vestrace-infrastructure/tests/installation_drain_request.rs`.

- [ ] **Step 2: Confirm the probe from Task 2 Step 5 now passes**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test installation_drain_request`
Expected: PASS (`the_drain_migrator_reaches_216`).

- [ ] **Step 3: Confirm the existing intent suites still pass unchanged**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test material_intent_lifecycle --test credential_intent_lifecycle --test intent_crash_boundaries`

Run each target name individually (not combined — see Global Constraints), and expect the exact same counts P05-F/G already recorded: `material_intent_lifecycle` 9/9, `credential_intent_lifecycle` 25/25, `intent_crash_boundaries` (record the count observed; this suite was not individually cited by an earlier package, so there is no prior figure to compare against — record whatever passes as the new baseline).

---

### Task 4: Domain layer — `InstallationDrainRequest`

**Files:**
- Create: `crates/vestrace-domain/src/installation_drain.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`

**Interfaces:**
- Consumes: `MaterialKeyCreationIntentState`, `CredentialKeyCreationIntentState` (existing, `crates/vestrace-domain/src/material/identity.rs`).
- Produces: `InstallationDrainRequestId`, `InstallationDrainRequest`, `is_material_pre_quiescing(MaterialKeyCreationIntentState) -> bool`, `is_credential_pre_quiescing(CredentialKeyCreationIntentState) -> bool`. Tasks 5/6 consume these; Task 7's tests assert their exhaustiveness.

- [ ] **Step 1: Write the failing test**

```rust
// crates/vestrace-domain/tests/installation_drain_contract.rs
use vestrace_domain::{
    CredentialKeyCreationIntentState, InstallationDrainRequest, InstallationDrainRequestId,
    MaterialKeyCreationIntentState, is_credential_pre_quiescing, is_material_pre_quiescing,
};

#[test]
fn a_fresh_request_is_draining() {
    let request = InstallationDrainRequest::request(InstallationDrainRequestId::new());
    assert!(!request.is_frozen());
}

#[test]
fn completing_a_request_freezes_it() {
    let request = InstallationDrainRequest::request(InstallationDrainRequestId::new());
    let frozen = request.complete();
    assert!(frozen.is_frozen());
}

#[test]
fn every_material_state_has_an_exact_pre_quiescing_answer() {
    use MaterialKeyCreationIntentState::*;
    let pre_quiescing = [Reserved, ProvisionalCreated, ProvisionalReceipted, ContentPrepared, ResultPrepared];
    let not_pre_quiescing = [
        ContentAbandonPrepared, PrePreparedAbandonPrepared, Bound, Live, ErasurePrepared, Tombstoned, Abandoned,
    ];
    for state in pre_quiescing {
        assert!(is_material_pre_quiescing(state), "{state:?} must be pre-Quiescing");
    }
    for state in not_pre_quiescing {
        assert!(!is_material_pre_quiescing(state), "{state:?} must not be pre-Quiescing");
    }
}

#[test]
fn every_credential_state_has_an_exact_pre_quiescing_answer() {
    use CredentialKeyCreationIntentState::*;
    let pre_quiescing = [Reserved, ProvisionalCreated, ProvisionalReceipted, CredentialPrepared];
    let not_pre_quiescing = [
        CredentialAbandonPrepared, Bound, Candidate, ErasurePrepared, Destroyed, Abandoned,
    ];
    for state in pre_quiescing {
        assert!(is_credential_pre_quiescing(state), "{state:?} must be pre-Quiescing");
    }
    for state in not_pre_quiescing {
        assert!(!is_credential_pre_quiescing(state), "{state:?} must not be pre-Quiescing");
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p vestrace-domain --test installation_drain_contract`
Expected: FAIL to compile — the module does not exist.

- [ ] **Step 3: Write the domain module**

```rust
// crates/vestrace-domain/src/installation_drain.rs
//! DrainMutationPermit / Quiescing: an installation-wide request that new
//! MaterialKeyCreationIntent/CredentialKeyCreationIntent work stop, and that
//! every intent already in flight reach a state that can survive
//! indefinitely -- using only the resume/abort logic those intents already
//! have. This module adds no new per-intent transition.

use uuid::Uuid;

use crate::{CredentialKeyCreationIntentState, MaterialKeyCreationIntentState};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct InstallationDrainRequestId(Uuid);

impl InstallationDrainRequestId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for InstallationDrainRequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// `Draining` while intents drained-by this request remain pre-Quiescing;
/// `Frozen` once none do. Reconciliation, not this type, decides which --
/// this type only carries the identity and the two states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallationDrainRequest {
    Draining(InstallationDrainRequestId),
    Frozen(InstallationDrainRequestId),
}

impl InstallationDrainRequest {
    pub const fn request(id: InstallationDrainRequestId) -> Self {
        Self::Draining(id)
    }

    pub const fn id(self) -> InstallationDrainRequestId {
        match self {
            Self::Draining(id) | Self::Frozen(id) => id,
        }
    }

    pub const fn is_frozen(self) -> bool {
        matches!(self, Self::Frozen(_))
    }

    pub const fn complete(self) -> Self {
        Self::Frozen(self.id())
    }
}

/// Exhaustive by construction: a state added to `MaterialKeyCreationIntentState`
/// later fails to compile here rather than being silently treated as
/// not-pre-Quiescing.
pub const fn is_material_pre_quiescing(state: MaterialKeyCreationIntentState) -> bool {
    use MaterialKeyCreationIntentState::*;
    match state {
        Reserved | ProvisionalCreated | ProvisionalReceipted | ContentPrepared | ResultPrepared => true,
        ContentAbandonPrepared | PrePreparedAbandonPrepared | Bound | Live | ErasurePrepared
        | Tombstoned | Abandoned => false,
    }
}

pub const fn is_credential_pre_quiescing(state: CredentialKeyCreationIntentState) -> bool {
    use CredentialKeyCreationIntentState::*;
    match state {
        Reserved | ProvisionalCreated | ProvisionalReceipted | CredentialPrepared => true,
        CredentialAbandonPrepared | Bound | Candidate | ErasurePrepared | Destroyed | Abandoned => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_request_is_draining() {
        assert!(!InstallationDrainRequest::request(InstallationDrainRequestId::new()).is_frozen());
    }
}
```

Add to `crates/vestrace-domain/src/lib.rs`, alongside the existing `pub mod installation_safety;`:

```rust
pub mod installation_drain;
```

and alongside the existing `pub use installation_safety::{...};`:

```rust
pub use installation_drain::{
    InstallationDrainRequest, InstallationDrainRequestId, is_credential_pre_quiescing,
    is_material_pre_quiescing,
};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p vestrace-domain --test installation_drain_contract`
Expected: PASS, 4/4.

Run: `cargo test -p vestrace-domain --lib a_fresh_request_is_draining`
Expected: PASS.

---

### Task 5: Application port — `DrainMutationPermitRepository`

**Files:**
- Modify: `crates/vestrace-application/src/ports.rs`, `crates/vestrace-application/src/lib.rs`

**Interfaces:**
- Consumes: `InstallationDrainRequest`, `InstallationDrainRequestId` (Task 4), `RequestContext` (existing, `crates/vestrace-application/src/context.rs`), `ApplicationError` (existing).
- Produces: the `DrainMutationPermitRepository` trait. Task 6 implements it.

- [ ] **Step 1: Add the trait**

In `crates/vestrace-application/src/ports.rs`, alongside `CredentialIntentRepository`:

```rust
/// DrainMutationPermit: acquires InstallationMutationPermit::Exclusive
/// internally for `request`; `current_state` and `reconcile` are plain reads
/// plus (for `reconcile`) a conditional completion write, safe to call at any
/// time including after a crash.
#[async_trait::async_trait]
pub trait DrainMutationPermitRepository: Send + Sync {
    async fn request(
        &self,
        context: &RequestContext,
    ) -> Result<InstallationDrainRequest, ApplicationError>;

    async fn current_state(
        &self,
        context: &RequestContext,
    ) -> Result<Option<InstallationDrainRequest>, ApplicationError>;

    async fn reconcile(
        &self,
        context: &RequestContext,
        id: InstallationDrainRequestId,
    ) -> Result<InstallationDrainRequest, ApplicationError>;
}
```

Add `InstallationDrainRequest, InstallationDrainRequestId` to `ports.rs`'s existing `use vestrace_domain::{...}` import line.

- [ ] **Step 2: Export it**

In `crates/vestrace-application/src/lib.rs`, add `DrainMutationPermitRepository` to the existing `pub use ports::{...};` line.

- [ ] **Step 3: Compile-check**

Run: `cargo check -p vestrace-application`
Expected: succeeds (no test yet — this is a trait with no implementation; Task 6 provides one, Task 7 tests it).

---

### Task 6: Infrastructure — `PgDrainMutationPermitRepository`

**Files:**
- Create: `crates/vestrace-infrastructure/src/postgres/installation_drain.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`

**Interfaces:**
- Consumes: `DrainMutationPermitRepository` (Task 5), `PgStore::begin_scoped` (existing, `crates/vestrace-infrastructure/src/postgres/transaction.rs`).
- Produces: `PgDrainMutationPermitRepository`. Task 7 tests it directly; Task 8 uses it from `intent_crash_boundaries.rs`.

- [ ] **Step 1: Write the implementation**

```rust
// crates/vestrace-infrastructure/src/postgres/installation_drain.rs
use sqlx::Row;
use vestrace_application::{ApplicationError, DrainMutationPermitRepository, RequestContext};
use vestrace_domain::{InstallationDrainRequest, InstallationDrainRequestId};

use super::PgStore;

#[derive(Clone, Debug)]
pub struct PgDrainMutationPermitRepository {
    store: PgStore,
}

impl PgDrainMutationPermitRepository {
    pub fn new(store: PgStore) -> Self {
        Self { store }
    }
}

fn sqlstate(error: &sqlx::Error) -> Option<String> {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .map(|code| code.into_owned())
}

fn storage_error(error: impl std::fmt::Display) -> ApplicationError {
    ApplicationError::Storage(error.to_string())
}

#[async_trait::async_trait]
impl DrainMutationPermitRepository for PgDrainMutationPermitRepository {
    async fn request(
        &self,
        context: &RequestContext,
    ) -> Result<InstallationDrainRequest, ApplicationError> {
        let id = InstallationDrainRequestId::new();
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let result = sqlx::query("SELECT vestrace_request_installation_drain($1)")
            .bind(id.as_uuid())
            .execute(transaction.connection())
            .await;
        match result {
            Ok(_) => {
                transaction.commit().await.map_err(storage_error)?;
                Ok(InstallationDrainRequest::request(id))
            }
            Err(error) if sqlstate(&error).as_deref() == Some("55000") => {
                Err(ApplicationError::Conflict("INSTALLATION_DRAIN_ALREADY_ACTIVE".to_owned()))
            }
            Err(error) => Err(storage_error(error)),
        }
    }

    async fn current_state(
        &self,
        context: &RequestContext,
    ) -> Result<Option<InstallationDrainRequest>, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let row = sqlx::query(
            "SELECT id, completed_at FROM installation_drain_requests \
             ORDER BY requested_at DESC LIMIT 1",
        )
        .fetch_optional(transaction.connection())
        .await
        .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(row.map(|row| {
            let id = InstallationDrainRequestId::from_uuid(row.get("id"));
            let completed_at: Option<chrono::DateTime<chrono::Utc>> = row.get("completed_at");
            if completed_at.is_some() {
                InstallationDrainRequest::Frozen(id)
            } else {
                InstallationDrainRequest::Draining(id)
            }
        }))
    }

    async fn reconcile(
        &self,
        context: &RequestContext,
        id: InstallationDrainRequestId,
    ) -> Result<InstallationDrainRequest, ApplicationError> {
        let mut transaction = self
            .store
            .begin_scoped(context)
            .await
            .map_err(storage_error)?;
        let frozen: bool = sqlx::query_scalar("SELECT vestrace_reconcile_installation_drain($1)")
            .bind(id.as_uuid())
            .fetch_one(transaction.connection())
            .await
            .map_err(storage_error)?;
        transaction.commit().await.map_err(storage_error)?;
        Ok(if frozen {
            InstallationDrainRequest::Frozen(id)
        } else {
            InstallationDrainRequest::Draining(id)
        })
    }
}
```

Check `chrono` is already a dependency of `vestrace-infrastructure` (grep its `Cargo.toml`); if the existing codebase instead uses `sqlx::types::time::OffsetDateTime` for `TIMESTAMPTZ` columns elsewhere in this crate, use that type instead to match convention — confirm against an existing `TIMESTAMPTZ`-reading query in this crate (e.g. `credential_intent.rs` or `material_intent.rs`) before writing this step for real, and use whichever type they already use.

In `crates/vestrace-infrastructure/src/postgres/mod.rs`, add `pub mod installation_drain;` alongside `pub mod credential_intent;`, and add `pub use installation_drain::PgDrainMutationPermitRepository;` alongside `pub use credential_intent::PgCredentialIntentRepository;`.

- [ ] **Step 2: Compile-check**

Run: `cargo check -p vestrace-infrastructure`
Expected: succeeds.

---

### Task 7: PostgreSQL contract suite

**Files:**
- Modify: `crates/vestrace-infrastructure/tests/installation_drain_request.rs` (replace Task 2's probe)

**Interfaces:**
- Consumes: `PgDrainMutationPermitRepository`, `PgMaterialIntentRepository`, `PgCredentialIntentRepository` (all existing/Task 6), `DRAIN_HISTORICAL_MIGRATOR`.
- Produces: the evidence Task 9 cites for g0-12/g0-13's concurrency and snapshot-exactness claims.

- [ ] **Step 1: Write the failing tests**

Replace the probe content with:

```rust
//! DrainMutationPermit's PostgreSQL contract: concurrent-request refusal,
//! exact pre-Quiescing snapshot stamping, reserve refusal during a drain, and
//! reconcile reaching Frozen only at zero pending.

use sqlx::PgPool;
use vestrace_application::{DrainMutationPermitRepository, MaterialIntentRepository, RequestContext};
use vestrace_domain::{
    ContentMaterialId, InstallationDrainRequest, IntentNonce, MaterialKeyCreationIntent,
    MaterialKeyCreationIntentId, MaterialKeyId, PrincipalId, WorkspaceId,
};
use vestrace_infrastructure::{PgDrainMutationPermitRepository, PgMaterialIntentRepository, PgStore};

/// Seeds one workspace and one principal via the raw superuser pool
/// connection (bypassing RLS, exactly as `material_intent_lifecycle.rs`'s own
/// `reserve()` fixture does), then returns a `RequestContext` for them. Every
/// write this suite makes needs a real `workspaces` row: the
/// `material_key_creation_intents.workspace_id` foreign key requires it, and
/// `PgStore::begin_scoped` sets the `vestrace.workspace_id` RLS GUC to it.
async fn seed_context(pool: &PgPool) -> RequestContext {
    let workspace_id = WorkspaceId::new();
    let principal_id = PrincipalId::new();
    sqlx::query("INSERT INTO workspaces (id, slug) VALUES ($1, $2)")
        .bind(workspace_id.as_uuid())
        .bind(format!("drain-test-{}", workspace_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO principals (id, workspace_id, identifier) VALUES ($1, $2, $3)")
        .bind(principal_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(format!("drain-test-principal-{}", principal_id.as_uuid()))
        .execute(pool)
        .await
        .unwrap();
    RequestContext::new(workspace_id, principal_id)
}

fn fresh_material_intent(workspace_id: WorkspaceId) -> MaterialKeyCreationIntent {
    MaterialKeyCreationIntent::reserve(
        MaterialKeyCreationIntentId::new(),
        workspace_id,
        ContentMaterialId::new(),
        MaterialKeyId::new(),
        IntentNonce::new(),
        "content",
        uuid::Uuid::now_v7(),
        0,
    )
}

// A workspace fixture is required by both repositories' RLS context; confirm
// the exact existing test-fixture helper this crate already uses for that
// (e.g. `crates/vestrace-infrastructure/tests/common/mod.rs::workspace_scoped_intent`
// or the `tenancy`/`Fixture` helpers in `intent_crash_boundaries.rs`) and use
// it here instead of inventing a second one -- this plan's placeholder
// `context()`/`fresh_material_intent()` above must be reconciled with that
// helper's actual signature before this step is implemented for real.

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn a_second_concurrent_drain_request_is_refused(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let repo = PgDrainMutationPermitRepository::new(store);
    let ctx = context(WorkspaceId::new());

    let first = repo.request(&ctx).await.unwrap();
    assert!(matches!(first, InstallationDrainRequest::Draining(_)));

    let second = repo.request(&ctx).await;
    assert!(matches!(
        second,
        Err(vestrace_application::ApplicationError::Conflict(code))
            if code == "INSTALLATION_DRAIN_ALREADY_ACTIVE"
    ));
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn drain_stamps_exactly_the_pre_quiescing_intents_present_at_request_time(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let material = PgMaterialIntentRepository::new(store.clone());
    let drain = PgDrainMutationPermitRepository::new(store);
    let workspace_id = WorkspaceId::new();
    let ctx = context(workspace_id);

    let before = fresh_material_intent(workspace_id);
    material.reserve(&ctx, &before).await.unwrap();

    let request = drain.request(&ctx).await.unwrap();

    // reserve() after the drain is refused (proven in the next test); confirm
    // here only that reconcile still finds the pre-existing intent pending.
    let state = drain.reconcile(&ctx, request.id()).await.unwrap();
    assert!(matches!(state, InstallationDrainRequest::Draining(_)));
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn reserve_is_refused_while_draining(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let material = PgMaterialIntentRepository::new(store.clone());
    let drain = PgDrainMutationPermitRepository::new(store);
    let workspace_id = WorkspaceId::new();
    let ctx = context(workspace_id);

    drain.request(&ctx).await.unwrap();

    let after = fresh_material_intent(workspace_id);
    let result = material.reserve(&ctx, &after).await;
    assert!(result.is_err(), "reserve must be refused while draining");
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn reconcile_reaches_frozen_only_when_nothing_remains_pending(pool: sqlx::PgPool) {
    let store = PgStore::from_pool(pool);
    let drain = PgDrainMutationPermitRepository::new(store);
    let ctx = context(WorkspaceId::new());

    // No pre-existing intents: the drain should reconcile to Frozen immediately.
    let request = drain.request(&ctx).await.unwrap();
    let state = drain.reconcile(&ctx, request.id()).await.unwrap();
    assert!(matches!(state, InstallationDrainRequest::Frozen(_)));

    // Idempotent: reconciling an already-Frozen request stays Frozen.
    let state_again = drain.reconcile(&ctx, request.id()).await.unwrap();
    assert!(matches!(state_again, InstallationDrainRequest::Frozen(_)));
}
```

- [ ] **Step 2: Reconcile the fixture helpers against the real ones**

Before running, replace this task's placeholder `context()`/`fresh_material_intent()` with whatever `crates/vestrace-infrastructure/tests/common/mod.rs` or `intent_crash_boundaries.rs`'s own `Fixture`/`tenancy` helpers actually provide, so a workspace row genuinely exists for the RLS-scoped `begin_scoped` call to reference (the domain constructors above compile but a real `MaterialKeyCreationIntent::reserve` call also needs a `workspaces` row present, matching `intent_crash_boundaries.rs`'s own `tenancy(pool, &fixture, "...")` call in every one of its existing tests).

- [ ] **Step 3: Run each test standalone**

```
DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test installation_drain_request -- --test-threads=1
```

Expected: 4/4 pass. Record the exact count; do not assume it matches this plan's guess if the workspace-fixture reconciliation in Step 2 changed anything about the test count.

---

### Task 8: Fault-injection crash-boundary tests

**Files:**
- Modify: `crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs`

**Interfaces:**
- Consumes: the file's own existing `material_at`, `credential_at`, `resume_material`, `resume_credential`, `CountingVault`, `Fixture`, `tenancy`, `guarded` helpers (all already defined in this file — do not duplicate them).
- Produces: proof that an active drain does not change what `resume_material`/`resume_credential` do at any boundary, and that `reconcile` correctly tracks pending-to-zero as boundaries resolve.

**Note:** these new tests need `DRAIN_HISTORICAL_MIGRATOR`, but the file's existing ~16 tests use `HISTORICAL_MIGRATOR` and must keep doing so unchanged — different functions in the same file may carry different `#[sqlx::test(migrator = ...)]` attributes.

- [ ] **Step 1: Write the failing tests**

Append near the end of the file, after the last existing `credential_crash_after_erase_receipt_before_terminal_append_is_resumable`:

```rust
// ─── DrainMutationPermit: existing resumption is undisturbed by a drain ────

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn draining_does_not_change_how_a_reserved_material_intent_resumes(pool: PgPool) {
    let vault = CountingVault::default();
    let fixture = material_at(&pool, "after_reserved", &vault).await;

    let store = vestrace_infrastructure::PgStore::from_pool(pool.clone());
    let drain = vestrace_infrastructure::PgDrainMutationPermitRepository::new(store);
    let ctx = vestrace_application::RequestContext::new(
        vestrace_domain::WorkspaceId::from_uuid(fixture.workspace_id),
        vestrace_domain::PrincipalId::from_uuid(fixture.principal_id),
    );
    let request = drain.request(&ctx).await.unwrap();

    // Confirm the drain snapshot picked up this pre-existing Reserved intent.
    let mid_reconcile = drain.reconcile(&ctx, request.id()).await.unwrap();
    assert!(matches!(mid_reconcile, vestrace_domain::InstallationDrainRequest::Draining(_)));

    // Same resumption outcome as the non-drain sibling test
    // `material_crash_after_reserved_is_resumable` above -- the drain adds no
    // new per-intent behavior.
    let outcome = resume_material(&pool, &fixture, &vault).await;
    material_boundary(&pool, &fixture, &outcome, /* expected outcome matching the sibling test */);

    // Now that the intent reached a terminal/parked resting state, reconcile
    // must reach Frozen.
    let final_reconcile = drain.reconcile(&ctx, request.id()).await.unwrap();
    assert!(matches!(final_reconcile, vestrace_domain::InstallationDrainRequest::Frozen(_)));
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn draining_does_not_change_how_a_content_prepared_material_intent_resumes(pool: PgPool) {
    // Same shape as the test above, at boundary "after_prepared_before_bound"
    // -- proves the ContentPrepared conjunct of g0-12's enumerated boundary
    // list, mirroring `material_crash_after_prepared_before_bound_is_resumable`.
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn draining_does_not_change_how_result_prepared_finalizes(pool: PgPool) {
    // Uses `material_result_prepared_at_boundary`, proves g0-13's "ResultPrepared
    // always bind/finalizes" is unchanged by an active drain.
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn draining_does_not_change_how_a_reserved_credential_intent_resumes(pool: PgPool) {
    // Credential-side mirror of the first test, at boundary "after_reserved",
    // using `credential_at`/`resume_credential`/`credential_boundary`.
}

#[sqlx::test(migrator = "vestrace_infrastructure::DRAIN_HISTORICAL_MIGRATOR")]
async fn a_bound_intent_present_before_a_drain_is_not_stamped_and_never_abandons(pool: PgPool) {
    // Drives a material intent to "after_prepared_before_bound", then one step
    // further to Bound (bind directly, matching what material_at's caller
    // would do next), *then* requests a drain, and asserts: reconcile finds
    // zero pending immediately (Bound was never stamped, because
    // vestrace_request_installation_drain's snapshot excludes it), and the
    // intent's own bind-only-forward invariant (g0-13: "Bound never abandons")
    // is unaffected -- proven by asserting no abandon path is reachable from
    // Bound, exactly as the file's existing Bound-boundary tests already do.
}
```

**This task's four fully-sketched-but-not-yet-concrete tests are deliberately left with the exact boundary/outcome comment markers shown** (`/* expected outcome matching the sibling test */` and the three `// ...` bodies) because completing them requires reading each named sibling test's exact assertions in this same file (`material_crash_after_reserved_is_resumable` at the line this plan's investigation found, `material_crash_after_prepared_before_bound_is_resumable`, `owner_result_prepared_crash_after_prepared_before_bound_is_parked`, `credential_crash_after_reserved_is_resumable`) and copying their assertion shape exactly — a step done at implementation time by reading those ~20-line functions directly, not guessed here. Do this before running Step 2; do not leave a body unfinished and call the step done.

**Correction made before Task 8's dispatch:** the sketch above assumed a `material_boundary(&pool, &fixture, &outcome, ...)` call shape that does not exist. The file's real `material_boundary`/`credential_boundary` helpers each create their own fixture internally and immediately resume it, with no seam to insert a drain request in between — the wrong assumption was caught by reading the real helper signatures directly (`resume_material`/`resume_credential` take `vault: Arc<CountingVault>`; `material_at`/`credential_at`/`material_result_prepared_at_boundary` take `vault: &CountingVault`; `Fixture::context()` already builds `RequestContext` directly) before dispatch, not discovered mid-implementation. The five tests were rewritten to call `material_at`/`credential_at`/`material_result_prepared_at_boundary` and `resume_material`/`resume_credential` directly (inserting the drain request between fixture creation and resumption), asserting the same `ResumptionOutcome`/`CredentialResumptionOutcome` values their non-drain siblings assert. One resulting detail worth flagging: `draining_does_not_change_how_result_prepared_finalizes`'s final `reconcile` call asserts `Draining`, not `Frozen` — a `Parked` `ResultPrepared` intent never leaves the pre-Quiescing state set, so the drain correctly stays open, unlike the other four tests which each resolve to a terminal state and then `Frozen`. The corrected, complete code lives in this package's execution workspace as `task-8-brief.md`, not reproduced a second time here to avoid duplicating a large block — the implemented file is the authoritative record going forward.

- [ ] **Step 2: Run each new test standalone**

```
DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test intent_crash_boundaries draining_ -- --test-threads=1
```

Expected: 5/5 new tests pass (filtered by the `draining_` prefix all five share).

- [ ] **Step 3: Confirm the file's pre-existing tests are unaffected**

Run: `DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test intent_crash_boundaries`
Expected: every test passes, count unchanged plus the 5 new ones.

---

### Task 9: Qualify P05-I

**Files:**
- Modify: `docs/development-evidence/v1-g0-05-gate.json`, `tests/p05_g0_gate.test.mjs`
- Create: `docs/development-evidence/v1-g0-05i-drain-mutation-permit.md`
- Create: `docs/development-evidence/v1-g0-05-gate/drain-mutation-permit-test.txt`

- [x] **Step 1: Capture the evidence file**

```bash
{
  printf '$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-domain --test installation_drain_contract\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-domain --test installation_drain_contract
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test installation_drain_request\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test installation_drain_request
  printf 'exit: %s\n' "$?"
  printf '\n$ DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test intent_crash_boundaries\n'
  DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@127.0.0.1:55432/vestrace_test cargo test -p vestrace-infrastructure --test intent_crash_boundaries
  printf 'exit: %s\n' "$?"
} > docs/development-evidence/v1-g0-05-gate/drain-mutation-permit-test.txt
```

Expected: all three exit 0. Add this new evidence file's path to `scripts/p05-scope.mjs` first (repeat Task 1's Steps 3-7 for this one additional path, or fold it into Task 1 originally if this plan is revised before execution — do not let this file go dirty before it is admitted).

- [x] **Step 2: Wire g0-12, g0-13, and g0-10 in the manifest**

Compute the digest, then set `g0-12` and `g0-13` to `claim: "pass"` with one `sources` entry each (or one shared entry citing the same file — either is acceptable since both criteria are proven by the same evidence; follow whichever the collector's schema makes cleaner, which is one `sources` entry per criterion citing the same file and digest twice).

Update `g0-10`'s existing (P05-H) entry: change its reason to state the `DrainMutationPermit`/"no post-freeze write" conjunct is now proven (cite the new evidence file as an additional source alongside P05-H's `embedding-transition-test.txt`), but **keep `claim: "blocked"`** — the browser oracle conjunct is still open and unrelated to this package.

- [x] **Step 3: Run the collector**

Run: `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json`
Expected: exit 1 (aggregate still `blocked`), `counts.pass` risen from 12 to 14, `counts.blocked` unchanged at 4, `counts.unknown` fallen from 3 to 1 (only `g0-16` remains).

- [x] **Step 4: Extend the pinned manifest assertion**

In `tests/p05_g0_gate.test.mjs`, add `g0-12` and `g0-13` to the `pass` assertion loop, raise `passed.length` to `14`, and add a note that `g0-10`'s `blocked` reason now names two sources.

- [x] **Step 5: Run the full gate set**

```
node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings
git diff --check
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs
node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json
```

Expected: every command exits 0 except the last, which exits 1 with `counts: { pass: 14, blocked: 4, unknown: 1 }`.

- [x] **Step 6: Write the evidence doc**

Record: the migration-numbering prerequisite and its fix; the schema and guard placement; every new suite and its count; the qualification-check table; and what remains open (`g0-16` unknown; `g0-10`, `g0-14`, `g0-15`, `g0-17` blocked on their named conjuncts — P05-J's scope).

---

## Self-review

- Spec coverage: g0-12's full sentence (exact pre-Quiescing reconciliation, freeze evidence across crash boundaries, no post-freeze write) maps to Tasks 3 (snapshot + guard), 7 (concurrency/snapshot proof), 8 (crash-boundary proof). g0-13's sentence (retained vs. lost input routes, no synthetic bytes, ResultPrepared always finalizes, Bound never abandons) maps to Task 8's four named tests, each citing the exact existing per-intent logic it does not disturb.
- The migration-numbering prerequisite found during design (Task 2) is sequenced before the migration that needs it (Task 3), and its guard test proves the new static's exact membership before any code depends on it.
- Type consistency: `InstallationDrainRequestId`/`InstallationDrainRequest` are spelled identically in Tasks 4, 5, 6, 7, 8.
- No task invents a new per-intent transition; every task that touches `material_key_creation_intents`/`credential_key_creation_intents` either reads them or adds exactly one guard clause to `reserve`.
- Task 8 is honest about what it could not finish inline (four test bodies need their sibling tests' exact assertions copied at implementation time) rather than inventing plausible-looking assertions that might not match — this is flagged as a required step, not silently deferred.

## Execution handoff

Execute Task 1 first. Tasks 2 and 3 are sequential (3 depends on 2's new static existing to name in Task 2 Step 5's probe, which Task 3 then makes pass). Tasks 4 and 5 may run in parallel with each other after Task 3. Task 6 depends on 4 and 5. Tasks 7 and 8 depend on 6. Task 9 is last. Do not start P05-J until this plan's Task 9 evidence is recorded and pushed.
