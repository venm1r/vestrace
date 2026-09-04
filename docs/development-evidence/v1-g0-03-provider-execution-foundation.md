# P03 provider execution foundation — development evidence

This file records what was actually run and what it returned. It is not a
summary of intent: a command that was not run is recorded as UNRUN, and a
deviation from the plan is recorded as a deviation rather than folded into a
pass.

Scope of this entry: **Task 11** (wire provider execution, governed HTTP
commands, qualification worker, and recovery), executed against the completion
plan `docs/superpowers/plans/2026-09-01-vestrace-p03-task11-completion.md`.

Tasks 12 and 13 are recorded further down, each in its own section, separately
from the Task 11 record above. All three are complete to their last
self-executable step and all three stop at the same place: the independent
review that none of them can perform on itself.

## Environment

| Fact | Value |
| --- | --- |
| Host | Windows 11, PostgreSQL 17 in Docker (`vestrace-postgres-1`, 127.0.0.1:55432) |
| `DATABASE_URL` | `postgres://test:test@127.0.0.1:55432/vestrace_test` — Steps 1-3 and the first workspace run used `localhost`; see below |
| `VESTRACE_RUNTIME_DATABASE_URL` | the same host, user `vestrace`, password `runtime-local-development-only` |
| `RUST_TEST_THREADS` | `1` for every SQLx suite |
| Provider calls | none; no LM Studio, no external network |

### The connection URL that invalidated the first workspace run

Steps 1-3 and the first `cargo test --workspace` were invoked with
`localhost:55432`. The container publishes `5432/tcp -> 127.0.0.1:55432` and
nothing else, and this host resolves `localhost` to `::1` first: the IPv6
attempt stalls for roughly two seconds before falling back to IPv4. Measured
with `asyncpg` on 2026-09-03, ten connections each:

| Host in the URL | median connect |
| --- | --- |
| `localhost` | 2104 ms |
| `127.0.0.1` | 62 ms |

`PgStore::connect` sets `ACQUIRE_TIMEOUT` to one second, so every pool opened
that way returned `PoolTimedOut`, and every test asserting a sub-second
acquisition timed out. `#[sqlx::test]` was affected only in speed, because its
pool keeps the 30-second default — which is why Steps 2 and 3 passed over the
same URL while the first workspace run took five hours.

This is a property of how the run was invoked, not of the tree: the acquisition
constant is untouched by this task, and `git diff` on
`crates/vestrace-infrastructure/src/postgres/pool.rs` contains no change to
`connect`, `acquire_timeout`, `max_connections` or `PgPoolOptions`. It is
recorded because it produced 23 of the 25 failures in the first workspace run,
and reading those as product defects would have been wrong.

## Step 1 — formatting

| Command | Class | Result |
| --- | --- | --- |
| `cargo fmt --all --check` | PASS | exit 0 |

The plan anticipated that pre-existing dirty formatting drift might leave this
non-green. It did not: the whole workspace is formatted.

## Step 2 — focused application, HTTP and CLI suites

| Command | Class | Count |
| --- | --- | --- |
| `cargo test -p vestrace-application --test execute_step` | PASS | 25 passed, 0 failed |
| `cargo test -p vestrace-application --test run_coordinator` | PASS | 14 passed, 0 failed |
| `cargo test -p vestrace-http --test run_routes` | PASS | 6 passed, 0 failed |
| `cargo test -p vestrace-http --test provider_routes` | PASS | 10 passed, 0 failed |
| `cargo test -p vestrace-cli --test provider_runtime_wiring` | PASS | 4 passed, 0 failed |
| `cargo test -p vestrace-cli --test provider_openapi_contract` | PASS | 2 passed, 0 failed |
| `node --test apps/console/tests/providerClientContract.test.mjs` | PASS | 3 pass, 0 fail |

`provider_runtime_wiring` was 2 passed / 1 failed before composition, failing on
`server does not construct governed provider dispatch authority`. The plan
expected 3 passed at the end; the suite is 4 because a test was added asserting
that only the worker registers the governed step executor.

## Step 3 — serial SQLx qualification

Run as one `--no-fail-fast` invocation. Two suites failed on the first pass;
both causes were found, corrected, and the suites re-run. Both results are
recorded, because the first pass is what the qualification actually observed.

Command issued (single `--no-fail-fast` invocation, `RUST_TEST_THREADS=1`):

```
cargo test -p vestrace-infrastructure --no-fail-fast \
  --test provider_schema_contract \
  --test provider_dispatch_is_atomic \
  --test provider_result_binding \
  --test provider_runtime_role_refusals \
  --test run_acceptance_binding_race \
  --test connection_mutation_is_atomic \
  --test credential_activation \
  --test qualification_target_binding \
  --test p03_upgrade_provisioning
```

| Suite | First pass | After correction |
| --- | --- | --- |
| `provider_dispatch_is_atomic` | PASS — 44 passed | — |
| `run_acceptance_binding_race` | PASS — 23 passed | — |
| `credential_activation` | PASS — 13 passed | — |
| `connection_mutation_is_atomic` | PASS — 5 passed | — |
| `provider_result_binding` | PASS — 4 passed | — |
| `qualification_target_binding` | PASS — 3 passed | — |
| `provider_runtime_role_refusals` | PASS — 1 passed | — |
| `provider_schema_contract` | **FAIL** — 37 passed, 1 failed | PASS — 38 passed |
| `p03_upgrade_provisioning` | **FAIL** — 3 passed, 3 failed | PASS — 6 passed |
| `provider_effect_recovery` | **UNRUN** — file does not exist | see Deviations |

### Defect the qualification found: a fresh deployment could not migrate

`p03_upgrade_provisioning` is the only suite that runs the real Compose
provisioner and then migrates as the restricted runtime role; every other suite
is provisioned by a superuser through `#[sqlx::test]`. It failed with:

```
42501: only exact declared P03 function signatures may be handed to the guarded owner
  at vestrace_assign_p03_function_owner
  'vestrace_enqueue_run_step_after_input_ready(UUID, UUID, UUID, UUID, UUID, BIGINT, TEXT)'
```

Migration 0186 hands the guarded owner four Run-step functions. Three are
declared in the bootstrap allowlists in `docker/postgres/init-runtime-role.sh`;
the fourth was not. The migration catches `insufficient_privilege` and falls
back to a superuser path, which is why superuser-provisioned suites never saw
it — but a production upgrade runs as `vestrace`, is not a superuser, and
re-raises. **A fresh deployment would have failed to migrate.**

Fixed by declaring the signature in both arrays: the one that transfers
ownership and the one that grants the runtime its execute set.

### Stale assertion the qualification found

`provider_schema_contract` asserted the guarded owner holds only `SELECT` on
`run_work_items`. Migration 0186 deliberately grants it `INSERT`, with the
comment "The guarded owner receives exactly one Run write: INSERT on
run_work_items", so the guarded enqueue function can schedule an `ExecuteStep`
item once input material and MRE are complete. The assertion was not updated
with the migration.

It now asserts the intent rather than the fact: `run_work_items` gains `INSERT`
and nothing else, and the other six Run-adjacent tables stay read-only to the
owner. A later widening of the owner's write set fails the test.

### Earlier observations during implementation

These are recorded because they are how defects were found, not as substitutes
for the qualification run above.

| Suite | Observed | When |
| --- | --- | --- |
| `provider_dispatch_is_atomic` | 44 passed | before and after the governed-commit change |
| `provider_dispatch_is_atomic` | **25 failed** | with a `SELECT ... FOR UPDATE` idempotency read — see Defects |
| `run_acceptance_binding_race` | 23 passed | after adding two tests (was 21) |
| `connection_mutation_is_atomic` | 5 passed | after adding three tests (was 2) |
| `governed_mutation_is_atomic` | 7 passed | with the reuse rule in place |
| `credential_activation` | 13 passed | |
| `credential_intent_lifecycle` | 25 passed | |
| `connection_revision_lifecycle` | 6 passed | |
| `qualification_target_binding` | 3 passed | see the correction of this diagnosis below |
| `provider_result_binding` | 4 passed | |

## Step 4 — workspace and governance gates

| Command | Class | Result |
| --- | --- | --- |
| `cargo test --workspace --no-fail-fast` | PASS | **1785 passed, 0 failed, 8 ignored** across 198 test binaries, exit 0, 28.9 minutes. Run twice green: once after the two stale-expectation corrections, and again on the exact tree recorded here, after the withdrawn diagnosis below was reverted. First pass was 1760 passed / 25 failed; see below |
| `git diff --check` | PASS | two `new blank line at EOF` findings in `docker-compose.yml` and `docs/getting-started.md`, both introduced by this task and both removed; clean afterwards |
| `git status --short` | RECORDED | 317 entries at the close of Task 13: 149 modified, 146 untracked, 22 deleted. It was 311 when Task 11 closed; Task 12 added one file and Task 13's two scope amendments account for the rest. The verifier is what constrains this, not the count |
| `node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs` | PASS | 10 pass, 0 fail |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-03-preflight.json --scope p03-scope.mjs` | PASS | exit 1 with exactly two findings, both the already-authorized cdx cohabitation paths |
| `cargo clippy --workspace --all-targets` | PASS | no errors; six warnings, none in a file this task touched |

### What the first workspace pass returned, and why it is not the recorded result

The first pass was **1760 passed, 25 failed** in seven suites, over the
`localhost` URL described in Environment. The counts reconcile exactly:
1760 + 25 = 1785, the same set of tests the clean pass ran.

| Suite | First pass | Cause | After |
| --- | --- | --- | --- |
| `installation_permit_excludes` | 2 passed, 3 failed | connection latency: three one-second `tokio::time::timeout` guards around a second pooled connection | 5 passed |
| `postgres` | 15 passed, 1 failed | `PgStore::connect` itself returned `PoolTimedOut` at its one-second acquire timeout | 16 passed |
| `recovery_qualification_cli_contract` | 3 passed, 1 failed | same, through a spawned CLI | 4 passed |
| `runtime_schema_gate` | 2 passed, 2 failed | same | 4 passed |
| `v1_release_gate_cli` | 12 passed, 14 failed | same; the 14 failures were exactly the suite's 14 `#[sqlx::test]` cases, and every case that opens no database passed | 26 passed, 1 ignored |
| `model_request_semantic_observation` | 3 passed, 2 failed | **real**: the test's fake provider omitted `finish_reason` | 5 passed |
| `runtime_role_cannot_write_directly` | 43 passed, 2 failed | **real**: the test held migration 0181's replay contract | 45 passed |

Twenty-three of the twenty-five were the connection URL. The two real ones were
both stale test expectations rather than product defects, and both are recorded
under Pre-existing failures corrected below. The clean pass was run after those
two corrections, so it is the result this task stands on; the first pass is kept
because it is what the qualification actually observed and because it is what
found the two stale expectations.

Sum of suite durations, clean pass: **29.8 minutes**. The first pass took five
hours on the same tree.

### Scope audit

`scripts/p03-scope.mjs` declares 139 change paths and 17 protected paths, with
no path in both lists. Six declared change paths did not exist when this audit
was taken at the close of Task 11; one of them has since been created:

| Path | Why |
| --- | --- |
| `crates/vestrace-application/tests/model_data_policy.rs` | deleted with the legacy executor under the recorded scope amendment |
| `crates/vestrace-infrastructure/tests/lm_studio_model_data_policy.rs` | same |
| `docs/development-evidence/v1-g0-03-provider-execution-foundation.md` | this file, created by this step |
| `crates/vestrace-infrastructure/tests/provider_effect_recovery.rs` | see Deviations |
| `crates/vestrace-infrastructure/tests/provider_branch_isolation.rs` | created by Task 12 after this audit was taken; it exists now |
| `tests/provider_effect_fault_scenario_e2e.rs` | package Task 12, unstarted |

### Scope amendments recorded in the preflight

Two entries were appended to `v1-g0-03-preflight.json` during this task. The
preflight's captured dirty baseline, raw porcelain, HEAD and protected digests
were not touched.

- 125 → 137 change paths, and protected 18 → 17. Eleven paths were named by the
  Task 11 plan; the twelfth, `crates/vestrace-http/tests/router_contract.rs`, is
  the contract test for the AG-UI refusal the same task introduced, and is
  recorded explicitly rather than left as an unexplained divergence.
  `crates/vestrace-http/src/api/runs.rs` left `protectedAuthorityPaths`; the
  digest it was protected at (36072 bytes, sha256
  `2a584808bdb21e5bfc057294626bfb90330e9ea1f1079fc45656c2375ad742b9`) is
  retained in `protected_authority_digests`.
- 137 → 139, admitting the two legacy model-data-policy suites so the legacy
  executor could be removed with them.

Before these entries the verifier reported two extra findings
(`preflight change_scope_paths differ`, `preflight protected_authority_paths
differ`), because the scope module had been amended without a matching preflight
record.

## Defects found and closed

Each was found by a test written against the contract rather than against the
code.

1. **`x-request-id` cannot be a governed idempotency key.** `add_request_context`
   inserts a normalised UUIDv7 into every request, so a handler never observes
   its absence and a caller that sends none gets a fresh key per attempt — every
   retry would publish a second immutable revision. All nine governed mutation
   routes key on the caller's `idempotency-key` instead, which is the remedy
   `api::memory` had already documented for the same trap.
2. **A reused idempotency key carrying a different request was undetected.** The
   shared governed commit wrote keys with `ON CONFLICT DO NOTHING`, discarding
   the second request's hash; only the mutation's own guard stood in the way.
   The commit now refuses with `IDEMPOTENCY_KEY_REUSE_CONFLICT` before anything
   is applied. `credential_activation` already enforced this rule.
3. **`StepModelOutcome::Conflict` conflated two different situations.** "Another
   owner holds a live dispatch" and "admission is saturated" both produced a
   work-item completion, which for the second case would have left a step nobody
   would return to. The outcome now carries the interval admission quoted and
   the handler reschedules.
4. **A dispatch plan whose intent and attempt named different effects.** Found by
   a fake-authority test: dispatch takes its effect id from the intent while
   everything downstream takes it from the attempt. The executor now proves they
   are the same effect before dispatching.

### A defect this qualification introduced and then closed

The first form of defect 2 used `SELECT ... FOR UPDATE`, which needs `UPDATE`
privilege on `idempotency_keys` — a privilege the restricted runtime role does
not have and must not be granted. It failed **25 of 44** tests in
`provider_dispatch_is_atomic` with `permission denied for table
idempotency_keys`. This is the same trap the package's Task 10C corrective
already recorded for `model_request_evidence`. The mutual exclusion is now a
`pg_advisory_xact_lock` on the workspace-and-key pair with a plain `SELECT`,
which needs no table privilege; the suite returned to 44 passed.

## Pre-existing failures corrected

| Test | Was | Now |
| --- | --- | --- |
| `command_contract::worker_fails_explicitly_when_database_is_unavailable` | red — the storage-root check preceded the database connect, so a worker pointed at a dead database reported a filesystem fault | green; both roots prove the database first |
| `provider_runtime_wiring::long_running_roots_verify_schema_before_constructing_governed_authorities` | red — neither root composed any governed authority; hidden behind the failure above | green |
| `runs::tests::adding_an_agent_step_reaches_the_coordinator_as_an_agent_actor` | red — sent an agent step without `input` after the task made it mandatory | green |
| `tests/openai_q1_loopback.rs` | `cargo clippy --workspace` failed on two `unused_io_amount` errors | green |
| `model_request_semantic_observation` (2 tests) | red — the fake provider's chat response carried no `finish_reason`, which the governed decoder requires as a closed value | green; the stub returns `"stop"` |
| `runtime_role_cannot_write_directly::runtime_role_may_execute_guarded_functions` | red — replayed the Candidate abandon with the resulting association version | green; replays the original one, and the refusal case now uses the resulting one |
| `runtime_role_cannot_write_directly::retired_revoked_active_erasure_and_independent_recovery_are_exact` | red — asserted migration 0181's wording of the replay refusal | green; asserts 0186's |

### The replay contract two suites disagreed about

Migration 0186 deliberately re-states `vestrace_prepare_candidate_abandon_and_erasure`:
recovery now supplies the **original** association version and the function
proves the resulting version is exactly that plus one. The migration says so in
its own header, and `credential_activation` asserts it in both directions —
`candidate_abandon_replay_returns_its_original_erasure_preparation` replays the
original version, and `candidate_abandon_refuses_stale_or_unequal_association_versions`
requires that the resulting version "must not masquerade as the original replay
key". The production caller passes `command.expected_association_version`.

`runtime_role_cannot_write_directly` was left on migration 0181's contract, and
asserted the exact inverse: that replaying with the resulting version succeeds
and replaying with the original one is stale. It was the test that was wrong,
not the function; the two cases have been swapped and the local names now say
which version they carry.

Both of these were invisible until the workspace run, because no focused suite
in Steps 2 or 3 covers them.

### A diagnosis this qualification made and then withdrew

Two tests in `qualification_target_binding` failed mid-implementation. They were
recorded as a pre-existing failure caused by "Docker port-forwarding latency"
exceeding the suite's two-second connect timeout, and the timeout was widened to
twenty seconds behind a named constant saying so.

That was wrong. The suite derives the restricted connection from the test pool's
own `connect_options()`, so it inherited `localhost` from `DATABASE_URL` and
paid the two-second IPv6 fallback described in Environment — just over its
two-second budget. Port forwarding itself costs 62 ms.

The constant and its comment have been removed and the original
`Duration::from_secs(2)` restored;
`crates/vestrace-infrastructure/tests/qualification_target_binding.rs` is now
byte-identical to what it was before that change, and the suite passes 3 of 3
over IPv4. The same false diagnosis had been carried into Known gaps as a claim
that `ACQUIRE_TIMEOUT` is too small on this host; that entry is withdrawn too.
One second is roughly sixteen times the measured connect cost.

This is recorded rather than quietly deleted because the wrong diagnosis was
itself published as evidence, and because it is the reason two later gates were
mis-read for several hours.

## Self-audit against the review points

Recorded so an independent reviewer can check these rather than take them on
faith. This is not the review.

**No vault or provider call inside a transaction.** The only transaction this
task opens directly is in `PgProviderDispatchRepository::load_run_step_dispatch_plan`:
it runs two reads, commits, and only then decodes the stored intent and derives
the destination authority, both of which are pure computation. The dispatch,
result-preparation and publication transactions are owned by repositories that
predate this task.

**Classification precedes every adapter call.** `GovernedProviderStepExecutor::execute`
calls `recover_run_step_attempt` first, and returns on `Published`,
`AdoptedUnknown`, `AlreadyUnknown`, `AwaitDispatchDeadline` and
`ResumeResultPrepared` without loading a dispatch plan at all. The adapter is
reached only from `ResumeReserved` and `ResumeAdmitted`, after
`prepare_dispatch` has committed.

**No adapter retry after `Dispatching`.** An expired original dispatch is
classified `AdoptedUnknown` and returns `RecoveredUnknown`; the handler fails the
step as non-retryable, because retrying would be a second effect for a call that
may already have happened. Proved by `an_adopted_unknown_never_calls_the_adapter_again`
with an adapter call counter, and end-to-end by the repeat run in
`the_governed_executor_dispatches_an_accepted_step_once_and_never_again`.

**No double Run completion.** `ExecuteStepHandler` returns `Completed` on
`StepModelOutcome::Published` without calling `complete_step`; the success is
written by the provider-result finalizer in the transaction that binds the
encrypted output. Proved at the unit level by
`a_published_agent_step_is_not_completed_a_second_time_by_the_handler` (exactly
one commit, still `Running`, no output references) and against the database by
the step reaching `succeeded` while the handler wrote no success of its own.

**Pinned routing.** The executor cannot name a Connection, revision, model,
credential or base URL. It reads the plan from the Run's pinned
`ModelBindingSnapshot`, and the dispatch transaction independently re-proves the
connection revision belongs to that snapshot. `a_governed_run_step_dispatch_plan_is_read_back_from_the_pinned_binding`
asserts the branch is read from the snapshot rather than defaulted.

**Server and worker parity.** Both compose `GovernedProviderRuntime::new`;
`provider_runtime_wiring` forbids either root from constructing
`PgProviderDispatchRepository::new` or `PgProviderResultRepository::new` beside
the shared graph, and asserts only the worker registers a step executor.

## Deviations from the plan

1. **`provider_effect_recovery.rs` was not created.** Its proofs were placed in
   `run_acceptance_binding_race.rs`, where the governed-input fixture lives. A
   separate file would have required duplicating roughly two thousand lines of
   fixture, or including another suite by `#[path]` and running it a second time
   in a third binary. Database-level recovery classification is already proved
   in `provider_dispatch_is_atomic`; the end-to-end executor proof
   (`the_governed_executor_dispatches_an_accepted_step_once_and_never_again`)
   drives the real `ExecuteStepHandler` over the real PostgreSQL authorities and
   asserts one adapter call, `published` phase, a `succeeded` step written by the
   finalizer rather than the handler, no sentinel in `run_events`,
   `audit_events` or `outbox`, and zero adapter calls on a repeat run. The same
   file is also named by package Task 12, which is unstarted.
2. **The governed idempotency key is `idempotency-key`, not `x-request-id`.** See
   defect 1.
3. **A twelfth scope path** was admitted beyond the eleven the plan named. See the
   scope amendment record above.

## Task 12 — complete to its last self-executable step

### Steps 1 and 2: runtime-role refusal coverage

The plan asks this task to assert INSERT/UPDATE/DELETE refusal for every P03
guarded table, successful invocation of the runtime-callable guarded functions,
non-executability of internal helpers, exact `42501` for the generic P02
`vestrace_prepare_result_material` and the superseded Candidate-only credential
erasure function, and exact runtime execute-set equality with the
migration-declared grants.

All of those already existed when this task opened, in two files rather than
the one the plan names. Recorded so a reviewer can find each obligation rather
than assume it:

| Obligation | Proved by |
| --- | --- |
| INSERT/UPDATE/DELETE on every P03 table | `runtime_role_cannot_write_directly::runtime_role_direct_dml_on_every_p03_table_is_exact_42501` |
| runtime-callable guarded functions succeed | `runtime_role_cannot_write_directly::runtime_role_may_execute_guarded_functions` |
| internal helpers stay non-executable | `runtime_role_cannot_write_directly::runtime_cannot_execute_internal_provider_result_live_guard` |
| generic `vestrace_prepare_result_material` refused | `runtime_role_cannot_write_directly::runtime_role_cannot_execute_result_material_preparation` |
| superseded Candidate-only credential erasure refused | `runtime_role_cannot_write_directly::superseded_p02_candidate_erasure_is_runtime_exact_42501` |
| exact runtime execute set | `provider_schema_contract::p03_guarded_function_runtime_execute_set_is_exact` |
| narrow P03 result prepare/finalize accept a valid tuple | `provider_schema_contract::provider_result_entrypoints_enforce_replay_and_commit_complete_publication` |

Nothing was duplicated into `provider_runtime_role_refusals.rs` to satisfy the
plan's file layout; re-proving 45 passing tests in a second binary would add
runtime and no coverage.

### The hole those tests could not see

Both refusal matrices name their tables — 26 P02 names and 38 P03 names, hard
coded — and `provider_schema_contract::every_p03_table_is_guarded_forced_rls_and_has_a_nonempty_acl`
filters the catalog *through* the same 38-name list. So all three agree with
each other about a set none of them derives. A guarded table introduced by a
later migration and forgotten in the list would escape the ACL assertion and
the refusal matrix together, silently.

`provider_runtime_role_refusals::every_guarded_owner_table_refuses_runtime_dml_including_undeclared_ones`
declares nothing. It asks `pg_class` which tables `vestrace_guarded_owner` owns,
takes each one's first column for a no-op `UPDATE`, and requires exact `42501`
**and** the exact message `permission denied for table <name>` for all three
verbs. A floor of 64 tables guards against the query silently matching nothing.

### Proof by breaking, Step 4 method applied early

`GRANT INSERT ON public.provider_result_publications TO vestrace;` was appended
to migration 0186 and the suite re-run:

```
assertion `left == right` failed: INSERT on provider_result_publications must be
refused by the ACL rather than by RLS or a check
  left: "P03 immutable evidence accepts guarded inserts only"
 right: "permission denied for table provider_result_publications"
```

The probe was reverted and the migration verified byte-identical by SHA-256
(`c4d293e4…0f77d2`, 58618 bytes); the suite returned to 2 passed.

**What the break showed that the design did not predict.** The SQLSTATE did not
change. With the ACL removed the insert was still refused — by the table's
immutability trigger, which also raises `42501`. The code assertion passed
through the broken grant, and only the message assertion caught it. A refusal
test that observes `42501` alone cannot tell the table ACL from the trigger
behind it, so it cannot notice the ACL being taken away. That is why this test
asserts the message and not only the code, and it is worth knowing for the
refusal tests that already exist.

### Step 3, part one: branch isolation

`crates/vestrace-infrastructure/tests/provider_branch_isolation.rs`, 3 passed.

The existing mixed-identity test in `provider_schema_contract` builds two
`no_auth` Connections, so the credential branch had no two-Connection coverage.
This file builds two Connections in one workspace, each with its own execution
guard, credential slot and activation guard, and asks which edge keeps them
apart.

**The activation guard is that edge.**
`vestrace_ensure_credential_activation_guard` refuses a slot belonging to
another Connection with `23514` and `credential activation guard requires its
credential slot`. Every binding-snapshot path to a foreign slot runs through a
guard row that cannot exist, which is why the snapshot's composite edges hold.

**One trigger carries the Connection-revision case, and only one.** A
credential-auth revision names a slot, and:

- `connection_revisions_credential_slot_fkey` references
  `credential_slots(workspace_id, id)` and omits `connection_id`, so the slot
  need only exist somewhere in the workspace;
- `vestrace_create_connection_revision_and_advance_head` validates the execution
  guard and the head version, then passes `target_credential_slot_id` straight
  into its INSERT;
- `PgConnectionRevisionRepository` requires an activation guard for the exact
  `(connection, slot, execution guard)` triple — but that is a caller's check,
  in Rust.

The database's own refusal comes from the row trigger
`connection_revisions_validate_identity`, and from nowhere else.

**Proof by breaking.** `DROP TRIGGER connection_revisions_validate_identity ON
connection_revisions;` was appended to migration 0186 and the test re-run:

```
the guarded creator accepted a foreign credential slot: revision 01a06610-4e7b-… has
connection_id=01a06610-4dee-… and credential_slot_id=01a06610-4e0b-…7f715,
while that slot belongs to connection 01a06610-4e0b-…e80f35
```

Every foreign key was satisfied by that row. The probe was reverted and the
migration verified byte-identical by SHA-256 (`c4d293e4…0f77d2`, 58618 bytes);
both suites returned to green.

The test asserts the trigger's exact message rather than only `23514`, for the
reason the Step 2 break established: several checks on this path share that
SQLSTATE, and a bare code cannot say which one spoke.

**The same wire model id on both Connections.** The third test is the one the
plan names: two Connections whose model revisions advertise the identical
provider-side string, `shared-wire-model-v1`. Read a model id off a provider
and there is nothing in it to tell the two apart, so the binding chain must
never carry that string in a key. It does not:
`model_qualification_revisions` is pinned by
`(workspace, model_revision, connection_revision)` and by
`(workspace, connection_revision, connection_qualification_revision)`, and a
qualification claiming one Connection's model against the other Connection's
evidence is refused with `23503` on
`model_qualification_revisions_model_revision_fkey`. The test asserts the
constraint name, not just the code, and asserts the shared string first so it
cannot pass for the wrong reason.

That last precaution earned itself immediately. Extending the fixture with
revisions and heads made the slot test start failing at `40001 connection
revision head version conflict` — it was being refused before it ever reached
the slot check. A test asserting only "some error" would have stayed green
while proving nothing.

### A claim made during this task and corrected

While reading `vestrace_create_connection_revision_and_advance_head` this task
recorded, in conversation, that the slot-ownership rule "lives in Rust, not in
the database". That was wrong, and the test above is what showed it: the rule
is in the database, in a trigger rather than in the creator or a foreign key.
The correction is kept because the wrong reading is an easy one to repeat —
neither the function body nor the schema's foreign keys reveal the constraint.

### Step 3, part two: the crash boundary after dispatch

Added to `run_acceptance_binding_race.rs`, where the governed-input fixture and
the real executor wiring already live:
`a_crash_between_dispatch_and_receipt_recovers_without_a_second_provider_call`.
The suite is 24 passed.

**Why it was needed.** All seven `RunStepAttemptRecovery` variants were already
covered, but not in the same way. The database-level classification is proved in
`provider_dispatch_is_atomic`; the "never call the provider twice" property for
the intermediate states is proved in `vestrace-application`'s `execute_step`
against `ScriptedDispatch` and `ScriptedResults` — in-memory doubles, which the
plan's Step 5 forbids citing for a crash-boundary invariant. End to end over
real PostgreSQL, only the `Published` path was covered.

**The crash is performed, not described.** `prepare_dispatch` commits before the
network call, so an adapter that panics leaves precisely what a killed worker
leaves. The panic is caught by `tokio::spawn` returning a join error. The test
asserts the wreckage first — phase `dispatching`, one `dispatching` transition,
zero receipts — so it cannot pass without having actually crashed. Then a second
executor with a fresh adapter and a fresh vault resumes through the real
`ExecuteStepHandler`: zero adapter calls, `RunWorkOutcome::Retry`, still one
dispatch, still no receipt, and the step not failed. No double is involved
anywhere; the adapter is the process boundary the crash happens at.

**What the attempt to age the deadline found.** The first version of this test
tried to bring the dispatch deadline forward so the same run would classify as
adopted-unknown. PostgreSQL refused:

```
23514: Dispatching deadline must equal its admitted lease expiry
  at vestrace_validate_task10_deferred_contract
```

The deadline lives in `connection_dispatch_admissions`, which
`vestrace_reject_p03_immutable_mutation` restricts to guarded **inserts** —
`UPDATE` is refused for every role including the guarded owner — and a deferred
contract requires the lifecycle transition to carry the identical value. The
deadline is therefore unforgeable, which is the property rather than an
obstacle: a test that could age it would be testing a system that a compromised
worker could also lie to. The expired side of the boundary stays proved at the
database level, where the recovery clock is an argument rather than a wall
clock.

**A testability limit worth naming.** `GovernedProviderStepExecutor` calls
`Utc::now()` at three points and takes no clock. Its two time-dependent
branches, `AwaitDispatchDeadline` and `AdoptedUnknown`, therefore cannot both be
driven end to end without waiting out the sixty-second TTL. This is not a
product defect and nothing here depends on changing it, but it is why the
adopted-unknown case is cited from a different suite rather than proved here.

### Step 4: a break the plan predicted, which did not happen

The plan asks this task to "temporarily remove the Connection guard lock in the
test migration/function and observe the acceptance/rotation race admit a mixed
tuple". The lock is the `FOR UPDATE` on `connection_execution_guards` in
migration 0178's binding-acceptance function, under the comment "Canonical lock
order: connection execution guard first".

It was removed and the two race tests were run **eight times**:

```
without the guard lock: 8 green runs, 0 red runs
```

`run_acceptance_races_connection_head_advance_on_two_independent_pools` and
`run_acceptance_races_credential_rotation_on_two_independent_pools` did not
notice. The probe was reverted and migration 0178 verified byte-identical by
SHA-256 (`0994695a…4ae61333`, 38776 bytes); the suite is 24 passed with the lock
back.

The tests do intend to race: two independent pools, a `Barrier(2)` both sides
wait on, and `tokio::join!`, with the assertion that the persisted snapshot is
the complete old tuple or the complete new one and never a mixture. What the
probe shows is that the assertion survives without the row lock — either the
window is not reached at this timing, or something else serialises the two
paths. This evidence does not say which, and it should not: eight samples
establish that the tests fail to detect the removal, not why.

**The consequence is what matters.** These two tests cannot be cited as proof
that the connection-guard row lock is load-bearing, because they pass without
it. The lock may still be necessary — a break that does not fire is not a
licence to remove anything — but nothing in this package currently demonstrates
that it is. Recorded as an unproven safety claim rather than resolved, because
resolving it means either building a test that reaches the window or showing the
lock is redundant, and both are product work beyond a qualification task.

Two other breaks in this task did fire, and are recorded above: the runtime ACL
grant on `provider_result_publications`, and dropping
`connection_revisions_validate_identity`.

### Steps 3 (remainder) to 5

`provider_effect_recovery.rs` remains uncreated, for the reason Task 11
recorded: the proofs need the governed-input fixture, and a second binary would
either duplicate it or re-run the suite that owns it. The crash boundary above
was added beside that fixture instead.

### `tests/provider_effect_fault_scenario_e2e.rs` — deliberately not built

The plan asks for a provider twin of the external-effect fault suite: the five
points of `EffectFaultPoint::required_points()`, driven through the real
`vestrace-fault-scenario` program, which `abort()`s a child process at each one.
It was not built, on the operator's decision, and this records what that costs.

**What it would take.** `crates/vestrace-fault-scenario/src/child.rs` drives a
generic webhook effect today: `HttpWebhookEffectAdapter`,
`PerformExternalEffectService`, one intent, one adapter stub. A governed
provider dispatch has no equivalent shortcut — it needs the whole chain standing
before the first fault point is reachable: connector, Connection, credential
slot and activation, connection and model qualifications, model revision,
binding snapshot, Run, step, material vault, governed input, MRE, and an
admission policy. That is the fixture `run_acceptance_binding_race.rs` spends
most of its two thousand lines on, rebuilt as production-path calls inside a
standalone binary.

**What it would add over what exists.** One thing, precisely: a real `abort()`
rather than a panic. The crash test added above unwinds, so destructors run and
sqlx rolls back anything still open; `abort()` runs nothing and leaves the
server to reap the connection.

**Why that difference is small at the boundary now covered.** The executor
commits `prepare_dispatch` before it calls the adapter and holds no transaction
across the call — the property the Task 11 self-audit records and
`erasure_holds_no_lock_during_vault_call` enforces for the sibling path. With no
transaction open at the crash point, an aborted process and an unwound one leave
identical durable state, which is why the panic-based test can assert the
wreckage exactly. The difference would matter at a boundary that crashes *inside*
a transaction, and the P03 provider path has none across a provider call.

**What is genuinely left uncovered.** The other four required points against the
governed provider path under a hard kill: after intent persistence, after
authorization before dispatch, after receipt before outcome confirmation, and
after outcome before Run commit. Each is proved at the database level in
`provider_dispatch_is_atomic` and, for the executor's own branching, in
`execute_step` — but not by a process that actually died there.

**One more reason the trade is not close.** The sibling harness,
`tests/effect_fault_scenario_e2e.rs`, is `#[ignore]`d and its own header states
that its final assertion fails today, deliberately. A provider twin built to the
same pattern would be ignored by `cargo test` and red when run, so it would add
no gate coverage in this package — only a second red harness to explain.

### Step 5: scoped diff review

Three checks, each run rather than asserted.

**Fault point names and count are unchanged.** `EffectFaultPoint::required_points()`
still returns exactly five, and `git diff` on
`crates/vestrace-domain/src/external_effects.rs` shows the function body only as
context: the additions in the dirty tree are to `intent_points`, its doc, and a
name mapping, none of which touch the external-effect five. Nothing in Task 12
edited that file.

**Every refusal names its observed SQLSTATE.** The P03 suites contain twelve
`is_err()` assertions; each was read. None is a refusal standing on `is_err()`
alone:

| Where | What the `is_err()` means |
| --- | --- |
| `provider_schema_contract`, 7 sites | a `tokio::time::timeout` elapsed — the assertion is that the operation *waited* behind a lock |
| `provider_schema_contract`, 2 sites | an injected commit or finalize fault rolled back |
| `provider_dispatch_is_atomic`, 1 site | a domain replay refusal, paired with an adapter call count of 1 |
| `run_acceptance_binding_race`, 1 site | a `JoinHandle` carrying the simulated crash's panic |

Every database refusal in the two suites this task wrote asserts the exact
SQLSTATE, and — after the Step 2 break showed several mechanisms sharing
`42501` — the exact message or constraint name as well.

**No in-memory double is cited for a database or crash-boundary invariant.**
The crash boundary added here runs the production repositories, vault, Run store
and handler over real PostgreSQL; the only substitute is the adapter, which is
the process boundary the crash is staged at. Where doubles *are* used —
`ScriptedDispatch` and `ScriptedResults` in `execute_step` — this file says so
and says what covers the same ground against the database instead.

The credential-lease and header halves of branch isolation are **not** planned
as new tests here. The snapshot creator does not accept a credential: it reads
`credential_slots.current_revision_id` for the Connection's own slot and
refuses a qualification target naming anything else
(`credential model binding requires the exact active guarded credential`), so
the header a dispatch carries is derived from the Connection rather than
supplied alongside it. Dispatch-time pinning is already covered by
`provider_dispatch_is_atomic`'s `pinned_credential_and_request_auth_branches_must_match`,
`credential_metadata_must_match_the_pinned_authority` and
`pinned_credential_must_remain_current_and_usable`. What no test covers is a
*direct* guarded-owner insert of a snapshot pairing one Connection's slot with
another's credential revision: `model_binding_snapshots` carries an
immutability trigger but no identity trigger, and its credential foreign key
omits `connection_id`. The runtime role cannot reach that path — it cannot
assume the guarded owner — so this is recorded as a boundary a future migration
could cross, not as a reachable defect.

Recovery classification is not uncovered in the meantime: all seven
`RunStepAttemptRecovery` variants are exercised at the database level in
`provider_dispatch_is_atomic` and at the executor level in
`vestrace-application`'s `execute_step`. What `provider_effect_recovery.rs`
still owes is the crash-boundary matrix around them.

## Task 13 — integrated verification

### Step 1: fresh focused and workspace gates

| Command | Class | Result |
| --- | --- | --- |
| `node scripts/verify-dirty-baseline.mjs --check . … --scope p03-scope.mjs` | PASS | exit 1 with exactly two findings, both the already-authorized cdx cohabitation paths |
| `node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs` | PASS | 10 pass, 0 fail |
| `node scripts/protocol-lock.mjs --check .` | PASS | exit 0 |
| `node scripts/verify-p01-text-hygiene.mjs --check .` | PASS | exit 0 |
| `cargo fmt --all -- --check` | PASS | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | exit 0, after seven corrections below |
| `node --test apps/console/tests/providerClientContract.test.mjs` | PASS | 3 pass, 0 fail |
| `npm --prefix apps/console run typecheck` | PASS | exit 0, after the scope amendment below |
| `git diff --check` | PASS | exit 0 |
| `cargo test --workspace --locked` | PASS | **1790 passed, 0 failed, 8 ignored** across 199 test binaries, exit 0, 31.5 minutes. Task 12 accounts for the delta from Task 11's 1785 in 198: one new binary (`provider_branch_isolation`, 3 tests) plus the closed-world refusal test and the crash-boundary test |
| `cargo test --test compose_smoke -- --ignored --test-threads=1` | PASS | **4 passed, 0 failed**, ~115 s, three consecutive clean runs. Reached only after four separate obstacles, three of them real deployment defects; an intermittency was also observed and is characterised below |

### The strict clippy gate had never passed on this tree

`-D warnings` with `--all-features` is stricter than anything this package had
run. It failed with seven findings, all pre-existing and none introduced by
Task 12:

| Finding | Where | Resolution |
| --- | --- | --- |
| `too_many_arguments` (12/7) | `model_request_evidence.rs` | `#[allow]` with a reason: the argument count **is** the assertion. That function is a signature twin whose only job is to stop compiling if an input parameter is ever added, and collapsing it into a struct would let a new field slip in unnoticed |
| `format_collect` | `api/artifacts.rs` | hex built with `fold` and `write!` instead of `format!` per byte |
| `needless_borrow` | `provider_dispatch_is_atomic.rs` | borrow removed |
| `clone_on_copy` | `provider_dispatch_is_atomic.rs` | `clone()` removed from a `Copy` receipt |
| `items_after_test_module` | `crypto/content_material_codec.rs` | the production `GovernedInputSealer` impl was sitting after `#[cfg(test)] mod`; moved above it |
| `then_some(..).unwrap_or(..)` | `tests/openai_q1_loopback.rs` | rewritten as `if`/`else` |
| `needless_lifetimes` | `provider_branch_isolation.rs` | this task's own; elided |

### The typecheck gate found three broken console routes

`GET /models`, `GET /connections` and `GET /providers` were cut over to governed
projections during this package, and `createProvider` was dropped from the SDK
because `POST /providers` now answers `legacy_provider_registry_retired` by
design. Three console routes still read the retired fields, and none of the
three was in scope or had been touched by any task: the already-scoped
`apps/console/src/sdk/client.ts` broke them.

This was not only a type error. The fields are absent from the responses, so the
pages would have rendered `undefined`, and `ModelsPage` would have thrown inside
`Intl.NumberFormat.format`.

The builder stopped at the plan's stop condition and asked. Under operator
decision the three routes were admitted to change scope — the fifteenth
`scope_amendments` entry, 139 to 142 paths — and rewritten to render what the
governed API serves. `ModelsPage` no longer offers to create a provider, and
after registering a model it re-reads the governed list rather than splicing the
legacy create response into it. No console screen was added; P06 is not claimed.

### The second deployment-blocking defect: no image can be built

All four Compose smoke tests failed identically, at `Dockerfile:20`, with
`cargo build --release --package vestrace-cli --bin vestrace` exiting 101. The
same release build succeeds on the host, so the difference is the build context
and not the profile. The literal cause, from the builder stage:

```
error: couldn't read `crates/vestrace-infrastructure/src/../../../schemas/openai-compatible/openai-chat-completions-v1-q1.json`:
       No such file or directory (os error 2)
   = note: this error originates in the macro `include_str`
error: couldn't read `crates/vestrace-infrastructure/src/../../../tests/fixtures/openai-q1/marker.png`:
       No such file or directory (os error 2)
   = note: this error originates in the macro `include_bytes`
error: could not compile `vestrace-infrastructure` (lib) due to 2 previous errors
```

Task 7 added `crates/vestrace-infrastructure/src/openai_q1.rs`, which compiles
the q1 manifest and the q1 marker fixture *into* the binary. The Dockerfile
copies `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src`, `crates` and
`migrations`, and nothing else. Neither embedded file reaches the builder.

**No vestrace image can be built from this tree** — not the server, not the
worker, not the migrator. That is worse than the first deployment defect this
qualification found: a fresh deployment could not migrate, but here there is
nothing to deploy at all.

Fixed under the sixteenth scope amendment by copying `schemas` and
`tests/fixtures` into the builder stage. The files are copied rather than
duplicated into the crate: both are protected authority paths whose bytes this
package pinned, and a second copy under `crates/` would create a rival source of
truth for the q1 manifest that nothing keeps in step.

### The deployment gate found four obstacles, one behind another

Each became visible only once the one before it was cleared. None is visible to
`cargo test --workspace`, which passed 1790 tests on the same tree.

| # | Symptom | Cause | Real defect? |
| --- | --- | --- | --- |
| 1 | no image builds; `Dockerfile:20` exits 101 | `openai_q1.rs` embeds two files the build context excludes | **yes** |
| 2 | `external volume "vestrace-provider-bootstrap-secrets" not found` | the bootstrap mount is deliberately external and never populated by Compose, and no procedure for provisioning it is written down | **yes** |
| 3 | `vestrace-server` exits 1 | `policy.data.*` was made required and never declared for the server | **yes** |
| 4 | `vestrace-worker` exits 1 | the machine's local `.env` still enabled retired config-routed model execution | no — see below |

**Obstacle 2, the undocumented operator step.** `docker-compose.yml` declares
`provider-bootstrap-secrets` as `external: true`, with the comment that it "is
never populated by Compose, so a development image cannot smuggle an embedded
bootstrap key into production composition". That is the right design and this
package's own `getting-started` section states the rule — but nothing says how
to satisfy it, so the gate was unrunnable for anyone who did not already know
the mounted-secret-store layout.

The volume was provisioned by hand for this run, which is the operator act the
design requires rather than a way around it:
`material-vault-bootstrap/{scope,purpose,algorithm}` and `v1/{state,private.pkcs8}`,
carrying exactly the identity `docker-compose.yml` names. The key material is
the fixed development constant `vec![0x5A; 32]` that
`crates/vestrace-fault-scenario/src/main.rs` already uses — not generated, not
random, and not a secret. It exists only as a local Docker volume.

**Obstacle 3, and what it says about the split.** The four `policy.data.*`
settings were declared for the **worker** and not for the **server**. Both
compose the same governed authorities, and Task 11's own refusal message names
all four, so a deployment that started its worker would still lose its server.
That the two roots had drifted apart was not noticed until a duplicate insertion
made the YAML unparseable and exposed which service already had them.

**Obstacle 4 was a guarantee working, not a fault.** The worker refused with
`config-only model execution is retired; governed provider dispatch authority is
required` because the machine's gitignored `.env`, dated before this package,
still set `VESTRACE_MODEL_ENABLED=true`. The repository's own `.env.example`
does not advertise that switch, so this was local state rather than a repository
defect. Recorded as positive evidence: a deployment asked for the retired
configuration-routed path, and the worker refused to start instead of quietly
ignoring the request and reaching a provider by an unpinned URL. The local file
was corrected on the operator's instruction and the suite re-run with no
environment override.

### An intermittency observed, and not explained

Six suite-level observations were taken. Four were clean at 4 passed; two failed
during stack startup with

```
service "vestrace-role-provision" didn't complete successfully: exit 2
```

The two failures were consecutive and both fell immediately after a heavy
sequence of image builds and `down -v` teardowns. Afterwards the service was
cycled in isolation — `down -v`, then `up postgres vestrace-role-provision` —
**five times with five clean exits**, and the full suite then ran **three
consecutive times at 4 passed**, at 117, 114 and 115 seconds.

The first hypothesis was the familiar PostgreSQL entrypoint race, where a
healthcheck passes during `docker-entrypoint-initdb.d` before the server
restarts. Five clean isolated cycles do not support it: that race would show
there too. The failures only ever appeared while the whole stack was starting
and while Docker was still reclaiming volumes from the previous teardown, which
points at host resource pressure rather than an ordering defect — but that is
where the evidence stops. **The cause was not established, and this entry does
not claim one.**

Recorded rather than dismissed because the gate is the deployment gate: a
startup that fails two times in six is not something a package should hand over
as "passing" without saying so.

**A near-miss in this task's own method.** The run that first showed the
failure was filtered through `grep -E "^test |test result"`, which drops
libtest's `failures:` block. The visible line was
`compose_smoke_no_sensitive_labels_in_metrics ... FAILED`, and had it been
recorded from that alone this file would now claim a sensitive-label leak in the
metrics endpoint. There was no leak; the stack had not started. The test was
re-run unfiltered before anything was written down.

### Why a whole release profile went unexercised

Neither of these two facts is an accident of this session, and both are worth
more than the defect itself.

`cargo test --workspace` builds the **debug** profile. The repository's CI builds
the CLI with `cargo build -p vestrace-cli --bin vestrace` — also debug. The only
gate anywhere that compiles the release profile is `docker compose up --build`
inside the `compose-acceptance` job. So the release build of this package had
exactly one observer, and it lives behind Docker.

`include_str!` and `include_bytes!` make a *source-tree* dependency that no
manifest declares: `Cargo.toml` cannot see it, `cargo test` cannot miss it
because the whole tree is present, and only a build with a narrowed context can.
The container build is the narrowed context, which is why it was the only thing
that could notice.

### Step 2: repository and authority hygiene

The porcelain comparison is the verifier's, and it reports only the two
authorized cdx paths.

Protected-authority digests were recomputed from the tree rather than trusted:

| Outcome | Count | Detail |
| --- | --- | --- |
| byte-identical to capture | 15 | includes the q1 manifest (19181 bytes), the q1 fixture (509 bytes), `schemas/protocol-lock.json`, and both the P01 and P02 preflights |
| changed under recorded authorization | 2 | `v1-g0-02-security-material-foundation.md` and this package's own plan; each matches its `protected_authority_revisions` entry exactly, in both digest and byte count |
| stale digest for an unprotected path | 1 | `crates/vestrace-http/src/api/runs.rs` was moved from protected to change scope by an earlier amendment. Its `protected_authority_digests` record is history, not a live constraint, and it is absent from `protected_authority_paths` |

No protected authority changed without an authorization that names it.

### Step 3: what this package established, and what it did not

Claims are separated by how they are supported. A claim this session checked
names the thing that shows it; a claim carried from the package's own record is
labelled as carried, because Task 13 did not re-derive ten tasks of work.

**Established, and checked here:**

| Claim | Shown by |
| --- | --- |
| A Run pins exactly one `ModelBindingSnapshot` | `run_model_binding_snapshots.run_id` is `UNIQUE`; a second snapshot for a Run cannot exist |
| The auth branch is exclusive | `model_binding_snapshots_auth_xor` and `connection_revisions_auth_xor`, exercised by `provider_schema_contract::database_constraints_pin_auth_xor_and_cross_workspace_identity` |
| Two Connections sharing a wire model id stay separate | `provider_branch_isolation`, 3 passed, including the shared-string case |
| The superseded P02 Candidate-only credential erasure entrypoint is closed to the runtime | `runtime_role_cannot_write_directly::superseded_p02_candidate_erasure_is_runtime_exact_42501` |
| Retired/Revoked and Candidate-abandon are distinct paths | `retired_revoked_active_erasure_and_independent_recovery_are_exact`, whose replay contract this task corrected |
| The generic P02 result-material prepare stays runtime-denied | `runtime_role_cannot_execute_result_material_preparation` |
| No guarded table escapes the refusal matrix | `provider_runtime_role_refusals::every_guarded_owner_table_refuses_runtime_dml_including_undeclared_ones`, which derives its set from `pg_class` |
| Fresh and existing-P02 databases both run the real administrative bootstrap before runtime migration | `p03_upgrade_provisioning`, 6 passed, holding both `fresh_database_runs_real_provisioner_before_runtime_migrator` and `accepted_p02_database_acquires_p03_helpers_before_runtime_upgrade` |
| The Compose result is executed evidence | the deployment gate above, run to 4 passed three times, not inferred from SQLx suites |
| A crash after dispatch never calls the provider twice | the crash-boundary test, over real PostgreSQL with production repositories |

**Established, carried from the package record and not re-derived here:**
immutable Connection and Model revisions, the q1 qualification lifecycle, the
ordinary credential activation and rotation boundary, `ModelRequestEvidence`
non-duplication, the hardened provider transport, and shared effect dispatch and
recovery. Tasks 2 through 10 own these; this task verified the gates, not the
derivations.

**One claim in the plan's list that was false when this task started.** The plan
records that "the Axum route inventory, CLI OpenAPI and console SDK transport
contract were cut over together". The route inventory and the OpenAPI contract
were. The console was not: `GET /models`, `GET /connections` and `GET /providers`
had moved to governed projections while three console routes still read the
retired fields, so the console could not compile against its own API. That was
found by this task's typecheck gate and corrected under the fifteenth scope
amendment. The claim is true now; it was not true when it was written.

**Not done by this package, and not claimed:**

- **No production embedding path.** q1 ordinal 90 is qualified and present in the
  manifest, but `EmbeddingJob` appears in no production source file in any crate.
  P03 created no embedding transition, corpus generation, carry, barrier or
  retrieval fence. *(checked: the identifier is absent from `crates/*/src`)*
- **Rotation with live embedding dependencies remains a typed refusal** until P04
  supplies exact transition evidence. *(carried)*
- **No restore, backup or TargetActivationPlan; no console workflow; no real
  Agent publication; no AG-UI or A2A change.** The console work in this package
  was repair of an existing screen against a changed API, not a new surface, and
  P06 is not claimed. *(the console part checked; the rest carried)*
- **No LM Studio call and no remote provider was contacted.** No test in this
  session reached a non-loopback endpoint, and the only live LM Studio suite was
  deleted under a recorded amendment. Loopback success is semantic development
  evidence: it shows the governed request and response shapes are what the code
  believes, and it is not evidence about any real provider and not release
  evidence. *(checked)*
- **P02's outstanding pre-existing-table ownership and the fingerprint
  backup/witness obligations remain P05's.** *(carried)*
- **G0 is not complete and v1.0 is not ready.** Three packages of five are behind
  this point; P04 and P05 are entirely unstarted.

**What the package cannot yet do at all.** A governed step on a freshly created
Connection still cannot be dispatched: `vestrace_try_admit_provider_dispatch`
refuses with `provider dispatch admission policy is absent`, and no production
route publishes one. Every end-to-end proof in this package seeds that policy
directly. This is the largest single gap between what the package proves and
what an operator could do with it.

### Step 4: independent adversarial review

**UNRUN, and it cannot be run by the party that wrote the work.** The plan
requires an independent reviewer to trace every acceptance criterion to live
code, migration ownership and ACLs, exact q1 manifest fields, real PostgreSQL
evidence and fresh command output, and to attack specifically: the auth XOR,
snapshot-at-acceptance, MRE non-duplication, transport DNS and peer policy, the
atomic pre-dispatch transaction, no-retry-after-`Dispatching`, and the
P03/P04/P05 boundaries.

Tasks 11, 12 and 13 all close on that verdict and none of them can close without
it. The self-audit above is offered as material for that review, not as a
substitute for it.

Suggested places to push hardest, chosen because they are where this task's own
confidence is thinnest rather than where it is strongest:

- the connection-guard row lock, which eight runs could not show to be
  load-bearing;
- the `credential_activation` path that still takes `SELECT ... FOR UPDATE` on
  `idempotency_keys`, carrying the same latent `42501` that broke the dispatch
  suite once already;
- the two-place declaration of the four run-step guarded functions;
- the direct guarded-owner insert into `model_binding_snapshots`, which no
  identity trigger constrains;
- whether the intermittent `vestrace-role-provision` exit 2 is really host
  pressure, which this task asserted only as the limit of its evidence.

### Step 5: operator handoff

**Scoped outcome.** Tasks 11 and 12 are complete to their last self-executable
step. Task 13 Steps 1 to 3 are complete. Every gate the plan names has been run
and recorded, with two classifications that are not plain passes: the dirty
baseline verifier exits 1 on exactly the two authorized cdx paths, and the
deployment gate is green after being red twice in six observations for a reason
this package did not establish.

**Package count.** P03 is the third of five in G0. P04 and P05 are entirely
unstarted, and P03 remains operator-unaccepted: nothing here should be read as
G0-complete or v1.0-ready.

**What this qualification found that the implementation did not.** Three
deployment-blocking defects, none visible to a green 1790-test workspace run: a
guarded-function signature missing from the bootstrap allowlist, so a fresh
deployment could not migrate; two compile-time embedded files absent from the
container build context, so no image could be built at all; and the required
model disclosure boundary declared for the worker but not the server, so the
server could not start. Also two stale test expectations that had drifted from
the product, and one diagnosis this task made and then withdrew.

**Standing note for P04.** `scripts/verify-dirty-baseline.mjs` must be added to
`protectedAuthorityPaths` from P04 onward. P03 was permitted to edit it, and the
next package must not be.

## Known gaps this task did not close

- **The deployment gate is not reliably green.** Two of six observed startups
  failed at `vestrace-role-provision` with exit 2, and the cause was not
  established. Five isolated cycles and three consecutive suite runs were clean
  afterwards.
- **`model_binding_snapshots` has no identity trigger.** Its credential foreign
  key omits `connection_id`, so a direct guarded-owner insert could pair one
  Connection's slot with another's credential revision. The runtime role cannot
  reach that path — it cannot assume the guarded owner — so this is a boundary a
  future migration could cross rather than a reachable defect today.
- **`GovernedProviderStepExecutor` has no clock seam.** It reads `Utc::now()` at
  three points, so its two time-dependent recovery branches cannot both be driven
  end to end without waiting out the sixty-second dispatch TTL.
- **The connection-guard row lock is unproven.** Removing the `FOR UPDATE` on
  `connection_execution_guards` left the two acceptance-race tests green eight
  times out of eight. The lock may still be necessary; nothing in this package
  demonstrates that it is.

- **The four run-step guarded functions are declared in two places.** Migration
  0186 names them and `docker/postgres/init-runtime-role.sh` declares them, and
  nothing keeps the two lists in step. The omission above was invisible to every
  suite except the one that runs the real provisioner.
- **No production path publishes a connection admission policy.** A governed
  step on a freshly created Connection cannot be dispatched:
  `vestrace_try_admit_provider_dispatch` refuses with `provider dispatch
  admission policy is absent`, and only test fixtures seed
  `connection_admission_policy_revisions` / `_heads`.
- **Candidate credential abandon exposes phase one only.** The route calls
  `prepare_candidate_abandon` and returns the erasure preparation; the host-vault
  fence lives in `CandidateCredentialAbandonService`, which needs an erasure
  authority and vault this surface does not hold.
- **`credential_activation` still uses `SELECT ... FOR UPDATE`** on
  `idempotency_keys` after taking its advisory lock. That path is not exercised
  under the restricted runtime role today, so the same `42501` that broke the
  dispatch suite is latent there.
