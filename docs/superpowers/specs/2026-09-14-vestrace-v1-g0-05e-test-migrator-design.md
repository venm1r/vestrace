# V1 G0-05E — historical test migrator design

**Recorded:** 2026-09-14.
**Package:** P05-E, a sub-package of P05 sharing `scripts/p05-scope.mjs` and
`docs/development-evidence/v1-g0-05-preflight.json` with P05-A through P05-D.
P05-D's recorded evidence is frozen; P05-E records its own.

## The measured problem

Every test file declaring `#[sqlx::test(migrations = "../../migrations")]` — 63
of them, 58 in `vestrace-infrastructure` and 5 in `vestrace-cli` — fails before
reaching its first assertion:

```text
failed to apply migrations: ExecuteMigration(... code: "42501",
message: "P05 safety verifier extension must be provisioned before migration"), 209)
```

This has held since P05-A shipped migration 0209. It is not a P05-D regression.

**No cluster fixes it.** `#[sqlx::test]` creates a fresh database per test from
`template1` and applies every file in `migrations/`. Migration 0209 asserts that
the `vestrace_safety_verify` extension, the `vestrace_guarded_owner` and
`vestrace_safety_supervisor` roles, and the `vestrace_install_p05_*` functions
are already provisioned. The P05 image provisions them only in the `vestrace`
database, so each fresh test database fails identically. Verified directly
against a P05-provisioned cluster, not only the ordinary test cluster.

**Consequence.** The database-backed regression net has been down for three
packages. The previously recorded red-test figure — four suites, fourteen tests —
cannot be reproduced, because the suites it names now fail at 0209 before
reaching any of the assertions it describes.

## The design tension this exposes

The G0-05 design states that 0209 and later are deliberately **not** ordinary
migrations. Compose applies them through a fixed three-phase route: the ordinary
runner takes the database `--through-version 208`; a bootstrap-custody stage
then installs the P05 schema while `vestrace` is `NOLOGIN`; only then does
`--only-version` apply one P05 assertion at a time, each pinned to its exact
predecessor.

`#[sqlx::test]`'s model is "apply every file in this directory". The two are
incompatible by construction. This is therefore a boundary that was never
drawn, not a bug to patch.

## Decisions

1. **The historical prefix is the test schema.** Suites that exercise product
   behaviour need migrations 0001–0208 and nothing more. Zero of the 63
   reference any P05 table (`installation_safety_state`,
   `installation_safety_generations`, `managed_backup_sets`,
   `managed_restore_attempts`, or the archive tables), so nothing is lost by
   stopping at 0208.

2. **0209's guard is not weakened.** Making 0209 skip when it finds no
   provisioning would hide real misprovisioning in deployment as well as in
   tests. The guard is the fail-closed behaviour the design requires and it
   stays exactly as it is.

3. **Production migration semantics do not change.** `MIGRATOR`,
   `migrate_through_version`, `migrate_only_version_after`,
   `p05_assertion_migration`, and `migrations_are_compatible` are untouched.
   The fix lives entirely in the test harness. Moving 0209–0215 into a separate
   directory was considered and rejected: it would make `vestrace migrate` with
   no flags stop silently at 208, narrowing a production path to solve a test
   problem.

4. **Provisioning `template1` was rejected.** It would couple testing to a
   bespoke image, give every test database a guarded-owner `public` schema, and
   run 0209 and later outside the fixed three-phase route — making the guard
   pass by arrangement rather than by provisioning discipline, which is the
   condition the guard exists to detect.

## The boundary

`vestrace_infrastructure::HISTORICAL_MIGRATOR` is a
`LazyLock<sqlx::migrate::Migrator>` built from the existing
`bounded_migrator(P05_HISTORY_PREFIX_VERSION)` helper in
`crates/vestrace-infrastructure/src/postgres/pool.rs`. No new filtering logic is
introduced; that helper already produces a version-bounded migrator and is
already used by `migrate_through_version`.

Every `#[sqlx::test]` that is not migration-sensitive changes one line, from
`#[sqlx::test(migrations = "../../migrations")]` to
`#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]`.
Both crates already depend on `vestrace-infrastructure`, so no manifest changes
are required. `sqlx::test`'s `migrator` argument takes a `&'static Migrator`,
and a borrow of a `LazyLock` static deref-coerces to one; this was proved by a
throwaway probe that applied through 208 and passed against the ordinary test
cluster.

## The migration-sensitive tests

Writing the plan against this spec exposed two errors in the paragraph this
section replaces. It named whole files when the real unit is the individual
test, and it missed a second hazard entirely.

**Bounding can make an assertion vacuous, not merely wrong.** `postgres.rs`
contains four tests of the form "damage the ledger, then assert the health check
refuses" -- `health_check_rejects_missing_migration`,
`..._unsuccessful_migration`, `..._extra_migration`, and
`..._same_count_version_mismatch`. `HealthRepository::check` calls
`migrations_are_compatible`, which compares the applied ledger against the
complete embedded migrator. On a database stopped at 0208 that comparison
already fails, so the health check is unavailable *before* the test damages
anything. All four would pass without testing what they name. A test that
passes for the wrong reason is worse than one that fails.

**One test drives the full migrator.** `store_migrate_applies_embedded_migrations`
calls `store.migrate()`, which runs every embedded migration including 0209, and
would panic on its `unwrap` rather than fail an assertion.

**A second class the first rule missed: tests that start the product.** The
runtime's own startup gate calls `migrations_are_compatible`, so a database
bounded at 0208 is one the product refuses to serve. A test that launches
`vestrace` and asserts it starts therefore depends on a complete deployment
without naming any ledger symbol at all. This class is invisible to a grep for
ledger APIs and has to be found by looking for process launches.

**The rule, stated as two properties rather than a file list.** A test is
migration-sensitive when either it references `migration_history_compatible`,
`HealthRepository::check`, `deployment_qualification_evidence`,
`runtime_evidence`, `store.migrate()`, or `_sqlx_migrations`; or it launches a
`vestrace` process against the test database. Twenty-nine tests across six files
match, out of 590 migration-applying tests:

| File | Migration-sensitive tests |
| --- | --- |
| `crates/vestrace-cli/tests/v1_release_gate_cli.rs` | 15 |
| `crates/vestrace-infrastructure/tests/postgres.rs` | 8 |
| `crates/vestrace-cli/tests/runtime_schema_gate.rs` | 3 |
| `crates/vestrace-cli/tests/recovery_qualification_cli_contract.rs` | 1 |
| `crates/vestrace-infrastructure/tests/fingerprint_continuity_is_fail_closed.rs` | 1 |
| `crates/vestrace-infrastructure/tests/row_level_security.rs` | 1 |

**Neither property is trusted as complete.** Both were derived by pattern
matching, and the second exists only because the first proved insufficient part
way through planning. The classification below is therefore confirmed
empirically — by running the suites and examining what actually fails — rather
than accepted from the patterns that produced the candidate list.

**One test is already failing and is out of scope.**
`postgres.rs::store_migrate_applies_embedded_migrations` declares a bare
`#[sqlx::test]`, applies no migrations through the harness, and calls
`store.migrate()` itself, which runs the complete embedded migrator and fails at
0209 today. P05-E does not repair it; it is recorded in the measurement as a
pre-existing failure needing a provisioned cluster.

Each of the twenty-nine is audited individually and classified exactly once:

* **safe** -- the test only reads `_sqlx_migrations` incidentally, such as a
  `WHERE version <= 207` filter, and its meaning is unchanged at 0208. It takes
  the historical migrator with the other suites.
* **prepared** -- the test asserts a property of a complete deployment. It moves
  to `VESTRACE_P05_TEST_DATABASE_URL` against a database taken through the real
  three-phase route.
* **vacuous** -- the test would pass without exercising its subject. It moves to
  the prepared database, because its subject is the complete deployment's
  ledger; it is never left bounded and never simply deleted.

The classification of every one of the twenty-nine is recorded in the evidence
with the reason, so a reader can check the judgement rather than trust it. The
remaining tests in those six files are ordinary historical-schema tests and
stay where they are: the files are not moved wholesale.

## Guards

* A Rust unit test asserts `HISTORICAL_MIGRATOR` is non-empty, has head 208, and
  contains no version above 208. It reads the static's actual contents rather
  than any description of them.
* A Node test asserts that no test file anywhere applies the raw migrations
  directory. After this change that count is zero, so a new suite cannot
  silently reintroduce the break — which is how this went unnoticed for three
  packages.

## Measurement

Once the suites run, `cargo test --workspace --no-fail-fast` is run against the
test cluster and the real red-test figure recorded suite by suite, with counts
and causes. The run is slow — a single suite has previously taken about
forty-five minutes — so it runs in the background, and no other cargo command
contends for the same target-directory lock while it does.

Blocked suites are recorded by name and counted separately from passing ones,
so a blocked run is never read as coverage. Whatever genuinely fails is named
as follow-up work. P05-E does not fix it.

## Non-goals

P05-E does not repair the failures it exposes, does not alter production
migration behaviour, does not weaken migration 0209, does not touch migrations
0001–0208 or any P01–P05 authority path, and makes no G0 claim. The aggregate
G0 result remains whatever `scripts/p05-g0-gate.mjs` emits.

## Evidence required before P05-E acceptance

1. The Rust guard proves the bounded migrator's head is 208 with nothing above
   it; observed RED before the static exists.
2. The Node guard proves no test file applies the raw migrations directory;
   observed RED while any still does.
3. A previously blocked suite runs to its assertions against the ordinary test
   cluster, demonstrated on at least one suite that failed at 0209 before.
4. All twenty-nine migration-sensitive tests are classified safe, prepared, or
   vacuous, with the reason recorded for each. Every prepared test reports
   blocked without `VESTRACE_P05_TEST_DATABASE_URL` and passes against a
   database taken through the three-phase route. No test is left in the vacuous
   state, and none is deleted to avoid classifying it.
5. The four `health_check_rejects_*` tests are shown to fail for their stated
   reason and not merely because the ledger is short: each is observed failing
   the health check only after its own damage, on a complete deployment.
6. `cargo fmt --all -- --check`, strict clippy on every touched crate, and
   `git diff --check` all exit 0.
7. The dirty-baseline verifier exits 0 with no findings for the amended P05
   scope.
8. The workspace red-test figure is recorded as measured, including suites that
   still fail and the reason each fails.

## External corpus impact

| Decision | Registered external corpus impact | Rationale |
| --- | --- | --- |
| historical test migrator | No new corpus entry. | The change is confined to test harness wiring; no external protocol, schema, or product contract is altered. |
