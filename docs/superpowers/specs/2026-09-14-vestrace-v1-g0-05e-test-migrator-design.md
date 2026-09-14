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

Sixty suites change one line each, from
`#[sqlx::test(migrations = "../../migrations")]` to
`#[sqlx::test(migrator = "vestrace_infrastructure::HISTORICAL_MIGRATOR")]`.
Both crates already depend on `vestrace-infrastructure`, so no manifest changes
are required. `sqlx::test`'s `migrator` argument takes a `&'static Migrator`,
and a borrow of a `LazyLock` static deref-coerces to one; this was proved by a
throwaway probe that applied through 208 and passed against the ordinary test
cluster.

## The three suites that need a complete deployment

`crates/vestrace-infrastructure/tests/postgres.rs`,
`crates/vestrace-infrastructure/tests/fingerprint_continuity_is_fail_closed.rs`,
and `crates/vestrace-cli/tests/v1_release_gate_cli.rs` assert full-deployment
properties: `migration_history_compatible`, `HealthRepository::check`, and
`deployment_qualification_evidence`. Each compares the applied ledger against
the **complete** embedded migrator, so a database stopped at 0208 is correctly
reported incompatible.

Bounding these suites would be lying to them: they would assert a health
property against a database that is, by construction, not the deployment whose
health they describe. They instead adopt the prepared-database pattern this
repository already uses for P05's own authority suites: the database URL is read
from `VESTRACE_P05_TEST_DATABASE_URL` — the same variable
`installation_safety_authority.rs` already reads — and must name a database
taken through the real three-phase route. When the variable is absent the suite
prints a `BLOCKED:` line and returns without asserting, exactly as the existing
P05 suites do.

That blocked path is a deliberate, and imperfect, trade. A suite that returns
without asserting reports green while proving nothing, which is how a Windows
fsync refusal once hid seven P05-D readiness cases. It is accepted here because
the alternative — failing when no prepared database exists — would make the
default workspace run red for an environmental reason and train everyone to
ignore it. The measurement step below therefore records blocked suites by name,
separately from passing ones, so the count is never mistaken for coverage.

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
4. The three full-deployment suites report blocked without a prepared database
   and run against one taken through the three-phase route.
5. `cargo fmt --all -- --check`, strict clippy on every touched crate, and
   `git diff --check` all exit 0.
6. The dirty-baseline verifier exits 0 with no findings for the amended P05
   scope.
7. The workspace red-test figure is recorded as measured, including suites that
   still fail and the reason each fails.

## External corpus impact

| Decision | Registered external corpus impact | Rationale |
| --- | --- | --- |
| historical test migrator | No new corpus entry. | The change is confined to test harness wiring; no external protocol, schema, or product contract is altered. |
