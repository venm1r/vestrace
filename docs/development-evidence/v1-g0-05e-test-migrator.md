# P05-E historical test migrator: measured result

## Scope and before state

P05-E replaced the raw migration-directory attribute in 63 database-backed
test files (592 attributes). Before the change, the affected regression net
contained 590 tests and each fresh `#[sqlx::test]` database stopped at
migration 0209 before reaching its own assertions.

The ordinary test migrator is intentionally bounded at migration 0208.
Production migrations, `MIGRATOR`, and the P05 provisioning route were not
changed by this package.

## Migration-sensitive audit

| Classification | Tests | Reason |
| --- | ---: | --- |
| safe | 2 | `runtime_schema_gate::runtime_role_can_read_administratively_applied_migration_history` only counts the already-applied ledger as the runtime role; `row_level_security::a_table_holding_a_workspace_id_has_row_level_security` mentions it only in a comment. |
| vacuous on the 0208 prefix | 4 | The four `health_check_rejects_*` tests in `postgres.rs` observe an already-incompatible ledger before making their intended damage. They were moved to `deployment_qualification.rs`. |
| prepared database required | 23 | The remaining `postgres.rs`, product-launching CLI, migration-ledger, and health-check candidates require a database taken through the complete P05 route. |

The prepared-database tests deliberately print `BLOCKED:` and return when
`VESTRACE_P05_TEST_DATABASE_URL` is absent. That is not a passing deployment
proof and is counted as blocked below.

## Workspace measurement

Command:

```text
DATABASE_URL=postgres://test:test@127.0.0.1:55432/vestrace_test cargo test --workspace --no-fail-fast
```

Result: exit 1; 60 Cargo test targets failed. Unit, contract, CLI-only,
integration-only, and documentation targets continued to run after those
failures.

| Result | Targets / suites | Measured cause |
| --- | --- | --- |
| passed | bounded-migrator guard; `deployment_qualification` (8/8); the non-database and many ordinary database suites that completed before resource exhaustion | The 0209 bootstrap guard was not encountered by migrated `#[sqlx::test]` attributes. |
| blocked | 3 CLI targets, numerous P03/P04 infrastructure targets, and runtime/RLS checks | `VESTRACE_RUNTIME_DATABASE_URL` was absent. These tests require a restricted runtime-role connection and cannot prove their assertions with only `DATABASE_URL`. |
| failed follow-up | canonical generation, erasure, retrieval, transition, text/vector retrieval, index, result, and upgrade suites | The real provisioner/runtime bridge fails with PostgreSQL `42601`: `syntax error at or near "SQL"` (reported at byte position 234595). |
| failed follow-up | `postgres::store_migrate_applies_embedded_migrations`, several library/integration migration tests | They deliberately invoke the complete embedded migrator against an unprovisioned fresh database and stop at 0209 with `P05 safety verifier extension must be provisioned before migration`. |
| blocked after resource exhaustion | late integration migration, RLS, run-event, and recovery-store suites | PostgreSQL first returned `53100` while resizing a 32 MiB shared-memory segment, then subsequent setup attempts returned `PoolTimedOut`. |

### Named follow-up work

1. Resolved on 2026-09-15: the shared and P03 provisioner-test helpers had
   selected the final `SQL` heredoc terminator after P05 added a later `psql`
   block. They now select the first terminator, so PostgreSQL receives only
   the runtime-bridge SQL. The focused helper regression passed (1/1). A
   rerun of `embedding_schema_contract` no longer reported `42601`; it reached
   its stated runtime-role requirement and produced 9 passed / 7 failures only
   because `VESTRACE_RUNTIME_DATABASE_URL` was absent. This does not qualify
   the full workspace.
2. Provide a disposable restricted runtime-role URL for the suites that require
   `VESTRACE_RUNTIME_DATABASE_URL`.
3. Run the three-phase P05 provisioner and supply its disposable URL through
   `VESTRACE_P05_TEST_DATABASE_URL` before qualifying the prepared tests.
4. Bound or isolate PostgreSQL test resource consumption before interpreting
   tests that started after `SQLSTATE 53100`/`PoolTimedOut` as product failures.
5. Keep `postgres::store_migrate_applies_embedded_migrations` as an explicit
   out-of-scope complete-migrator failure; P05-E does not weaken or redirect it.

### Runtime-role fixture follow-up

The documented local-development runtime login was supplied only to the test
process through `VESTRACE_RUNTIME_DATABASE_URL`; it was not written to a file.
With that role, `embedding_schema_contract` first reached 15/16 passing tests.
Its remaining failure exposed a custom `migrations = false` P04 fixture that
still selected the full migrator and therefore attempted protected migration
0209. The fixture now defaults to the historical 0208 prefix. The previously
failing `result_finalization_runtime_inventory_preserves_prior_acl_and_closes_new_authority`
test then passed in isolation. A repeated complete target exceeded the command
window, so this is not recorded as a 16/16 target qualification.

`embedding_canonical_generations` exposed two analogous P04 historical-upgrade
calls after 0196. Both now use `HISTORICAL_MIGRATOR`, because their asserted
transition ends at 0208. With the same transient runtime-role configuration,
the complete target passed 21/21 in 37.37 seconds.

Three `external_effect_repository` compatibility upgrades similarly end at
historical migrations 0153–0155. They now complete through the historical
prefix rather than invoking the protected P05 suffix; the complete target
passed 42/42 in 44.23 seconds.

The P03 upgrade target retained two full-migrator routes and an outdated
Compose service name. The P02-to-P03 route is now explicitly bounded through
0208, the 0207 retrieval upgrade uses `HISTORICAL_MIGRATOR`, and the Compose
contract follows `vestrace-migrate-history`, which is the runtime migration
stage after role provisioning. With the same transient runtime-role
configuration, the complete `p03_upgrade_provisioning` target passed 11/11 in
18.76 seconds.

The repeated complete `embedding_schema_contract` target also passed 16/16 in
12.02 seconds with that transient runtime-role configuration. This confirms
the result-finalization fixture change across the whole target.

The user then authorized a narrow P05-E scope amendment for three P04 test
files. They contained six explicit historical-upgrade calls that bypassed the
test attribute guard and reached migration 0209 after starting at migration
0194 or 0199. All six now use `HISTORICAL_MIGRATOR`. The four affected
result-finalization scenarios passed, as did the exact result-preparation and
transition-activation upgrade scenarios. Complete target reruns then passed:
`embedding_result_finalization` 17/17 in 24.71 seconds,
`embedding_result_preparation` 14/14 in 27.85 seconds, and
`embedding_transition_activation` 14/14 in 18.85 seconds.

## Non-claims

P05-E makes no G0 claim, repairs none of the failures above, and changes no
production migration behaviour. The aggregate G0 result remains whatever
`scripts/p05-g0-gate.mjs` emits.

## Prepared database route

To exercise a prepared test, build the image from `docker/postgres`, mount
`init-runtime-role.sh` at
`/docker-entrypoint-initdb.d/10-runtime-role.sh`, migrate through 0208, run
the bootstrap `psql` stage, then apply migrations 0209 through 0215 one at a
time with `migrate --only-version`. The Dockerfile does not bake this init
script; without the mount, PostgreSQL logs that it ignored
`/docker-entrypoint-initdb.d/*` and provisions nothing.

## Qualification checks

| Command | Result |
| --- | --- |
| `node --test tests/p05_scope.test.mjs tests/p05e_test_migrator.test.mjs tests/p05_g0_gate.test.mjs` | exit 0, 29/29 passed |
| `cargo fmt --all -- --check` | exit 0 after formatting the admitted P05-E Rust paths |
| `cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli --all-targets -- -D warnings` | exit 0 |
| `git -c safe.directory=E:/Soft/vestrace diff --check` | exit 0; no whitespace error |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-05-preflight.json --scope p05-scope.mjs` | exit 0 |
| `node scripts/p05-g0-gate.mjs --evidence docs/development-evidence/v1-g0-05-gate.json` | exit 1 as expected: aggregate `blocked`, 1 pass / 1 blocked / 17 unknown |

P05-E is complete as a test-migrator package: its scope, bounded-migrator
guard, raw-directory guard, formatting, lint, and baseline gates pass. The
workspace result above remains a measured red-test figure, not a G0 claim.
