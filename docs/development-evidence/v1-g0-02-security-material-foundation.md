# V1 G0-02 security and material foundation — scoped development evidence

**Status:** operator-accepted on 2026-08-28. Tasks 1-14 are complete.
After the operator-authorized exact `two` -> `three` correction in the protected
preflight's 130 -> 131 scope amendment, the repeated independent review found no
material finding and returned `VERDICT: APPROVE`. The operator accepted the Step 5
handoff by directing work to continue. This remains scoped P02 development
evidence and makes no G0-complete or release claim.

**Scope amendment:** Task 13 Step 2 required `sqlx` and `uuid` in
`crates/vestrace-fault-scenario/Cargo.toml`, which was absent from the captured
scope. The builder stopped and reported; the operator authorized the amendment
on 2026-08-27, and it is recorded in the preflight's `scope_amendments` (112 →
113 paths). The plan document was deliberately not edited — see that entry.

**Source revision:** `6aba953`, with P02 as uncommitted working-tree changes.
**Plan:** `docs/superpowers/plans/2026-08-27-vestrace-v1-g0-02-security-material-foundation.md`.

**Operator handoff — mechanism-language corrections:** Rounds 3 and 4 found
four instances of the same description failure: “sharing a path” where the
mechanism required literal-duplicate equality; “405 for a wrong method” where
the outer inventory authorization refuses `403` before axum routing; “console
code calls it” for an unexercised client method; and the two value-read comments
in `access_tokens.rs` and `secrets.rs`. The fourth claim predated P02 (it is on
the `6aba953` side of the diff), but P02 reflowed those lines while its new outer
authorization layer made the 405 explanation false. The routes still provide
the real benefit: an authorized value-read request reaches a specific
`*_value_not_readable` refusal rather than the generic inventory refusal. All
four descriptions lagged behind a mechanism the package itself was changing;
this is an operator-handoff pattern, not a behavior or release claim.

## Commands observed

Run 2026-08-28 against real PostgreSQL 17 (`#[sqlx::test(migrations = "../../migrations")]`,
per-test databases) with `DATABASE_URL` and `VESTRACE_RUNTIME_DATABASE_URL` both set,
the latter authenticating as the restricted runtime role (`rolsuper=f`, `rolbypassrls=f`).

```text
node scripts/verify-dirty-baseline.mjs --check . \
  docs/development-evidence/v1-g0-02-preflight.json --scope p02-scope.mjs   exit 1 (20 P03-only paths outside P02 scope)
node --test tests/p02_scope.test.mjs                              3 passed, 0 failed
node scripts/protocol-lock.mjs --check .                                   exit 0
cargo check --workspace --all-targets                                       exit 0
cargo clippy --workspace --all-targets --all-features -- -D warnings        exit 101 (P03 `admission.rs`: `from_persisted` and `build` have 8 arguments)
cargo fmt --all -- --check                                                  exit 0
cargo test --workspace --locked                                             exit 101 (P03 deliberate `BLOCKED finding 1`)
npm --prefix apps/console run typecheck                                     exit 0

governed_mutation_is_atomic                       7 passed, 0 failed
intent_crash_boundaries                          15 passed, 0 failed
material_intent_lifecycle                         9 passed, 0 failed
credential_intent_lifecycle                      16 passed, 0 failed
credential_guards                                 6 passed, 0 failed
erasure_is_one_way                               16 passed, 0 failed
erasure_holds_no_lock_during_vault_call           3 passed, 0 failed
runtime_role_cannot_write_directly               37 passed, 0 failed
cargo test -p vestrace-http                      59 passed, 0 failed
```

Task 13 Step 2, under the real harness — a child process that genuinely
`abort()`s, one ephemeral database per boundary, `VESTRACE_FAULT_ISOLATION=ephemeral`
and the URL passed as a file rather than in argv:

```text
14 boundaries executed, 0 failures

material   after_reserved                              reserved                  -> terminal (abandoned)
material   after_vault_create_before_receipt           provisional_created       -> terminal (abandoned)
material   after_receipt_before_prepared               provisional_receipted     -> terminal (abandoned)
material   after_prepared_before_bound                 content_prepared          -> terminal (abandoned)
material   after_bound_before_promotion                bound                     -> terminal (live)
material   after_abort_before_witnessed_erase          content_abandon_prepared  -> terminal (abandoned)
material   after_erase_receipt_before_terminal_append  content_abandon_prepared  -> terminal (abandoned)
credential after_reserved                              reserved                  -> terminal (abandoned)
credential after_vault_create_before_receipt           provisional_created       -> terminal (abandoned)
credential after_receipt_before_prepared               provisional_receipted     -> terminal (abandoned)
credential after_prepared_before_bound                 credential_prepared       -> terminal (abandoned)
credential after_bound_before_promotion                bound                     -> terminal (candidate)
credential after_abort_before_witnessed_erase          credential_abandon_prepared -> terminal (abandoned)
credential after_erase_receipt_before_terminal_append  credential_abandon_prepared -> terminal (abandoned)
```

Every run reported `intent_identities: 1`, so no resumption minted a second
identity under a reserved key. The surviving and resolved states match what
`intent_crash_boundaries` asserts in-process — a genuinely crashing process
reaches the same verdict. The first three material rows were `parked` before the
pre-prepared abort branch existed; they were re-run after it landed.

The harness refusals were re-checked and are unweakened, each exiting `2`:
a non-ephemeral `VESTRACE_FAULT_ISOLATION`, an effect boundary name given to an
intent scenario, and a missing `--database-url-file`.

## Fault-point vocabulary merge (Task 14 review finding E1)

The protected plan requires one shared `EffectFaultPoint` vocabulary. The
operator authorized replacing the separate `IntentFaultPoint` after the full
cost was disclosed. The five-item `EffectFaultPoint::required_points()` set is
unchanged and remains the external-effect suite's exact input. The seven intent
boundaries now live in the same enum, but in the separately ordered
`intent_points()` set: `after_reserved`,
`after_vault_create_before_receipt`, `after_receipt_before_prepared`,
`after_prepared_before_bound`, `after_bound_before_promotion`,
`after_abort_before_witnessed_erase`, and
`after_erase_receipt_before_terminal_append`.

This honours the plan's letter at a known cost: a type-level separation is now
a tested runtime refusal in release-critical code. `ScenarioSettings` keeps
the two scenario vocabularies closed; both cross-vocabulary invocations exit
`2`. The external-effect runtime, child stage mapping, report naming, and E2E
name helper each explicitly refuse all seven intent points. Tests pin the five
external points, the seven intent names and lifecycle order, process exit code
`2` in both directions, and the fact that the application suite and release
fixture still execute/evaluate exactly five observations. A deliberate
five-point-set break made the exact-set test fail before the original set was
restored. This is not presented as an unqualified improvement; it is the
operator-authorized trade from a compile-time guarantee to a runtime guard.

The merged-harness verification ran `cargo test -p vestrace-fault-scenario`
with **42 passed, 0 failed**; the deliberate real-process suite
`cargo test --test effect_fault_scenario_e2e -- --ignored --nocapture` with
**1 passed, 0 failed**, observing all five external-effect points; and
`cargo test -p vestrace-cli --test v1_release_gate_cli` with **26 passed,
0 failed, 1 ignored**. The ignored CLI case remains a separately-built
fault-scenario qualification path; the direct E2E suite above supplied the
real-process five-point proof for this change.

Task 14 Step 1, all six gates re-run from a fresh process on 2026-08-28,
after the pre-prepared abort branch and the fault-point vocabulary merge. This
supersedes the earlier run; no figure below is carried over from it. The test
gate ran against real PostgreSQL 17 with `DATABASE_URL` and
`VESTRACE_RUNTIME_DATABASE_URL` both set, the latter as the restricted runtime
role:

```text
node scripts/verify-dirty-baseline.mjs --check . ... --scope p02-scope.mjs   exit 1 (20 P03-only paths outside P02 scope)
node scripts/protocol-lock.mjs --check .                                     exit 0
cargo test --workspace --locked  exit 101 — one P03 deliberate `BLOCKED finding 1` failure
cargo clippy --workspace --all-targets -- -D warnings                        exit 101 — P03 `admission.rs`: `from_persisted` and `build` have 8 arguments
cargo fmt --all --check                                                      exit 0
npm --prefix apps/console run typecheck                                      exit 0
```

Task 14 Step 1 is **not satisfied**: the plan requires every command to exit
`0`, and only three of these six commands do. The dirty-baseline verifier exits
`1` for 20 P03-only paths outside P02 scope, the workspace test gate exits
`101` for P03's deliberate red-phase marker at
`crates/vestrace-infrastructure/tests/provider_schema_contract.rs:291`:
`BLOCKED finding 1: a random UUID is not the required DEK-derived HMAC
commitment`, and clippy exits `101` because P03's
`crates/vestrace-domain/src/models/admission.rs` defines both
`from_persisted` and `build` with eight arguments, tripping
`clippy::too_many_arguments` (8/7) under `-D warnings`. None is a P02
regression, but none is an exit `0` either.
The plan already explained the analogous P01 boundary: its verifier necessarily
exits `1` after P02 creates its first file. The same boundary now holds for the
P02 verifier against live P03; unlike Task 14's removal of the P01 invocation,
P02 cannot drop its own gate.

The 20 verifier findings are P03-only: `crates/vestrace-domain/src/connection/mod.rs`,
`crates/vestrace-domain/src/models/mod.rs`,
`crates/vestrace-domain/src/connection/revision.rs`,
`crates/vestrace-domain/src/models/admission.rs`,
`crates/vestrace-domain/src/models/binding.rs`,
`crates/vestrace-domain/src/models/evidence.rs`,
`crates/vestrace-domain/src/models/qualification.rs`,
`crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs`,
`crates/vestrace-infrastructure/tests/provider_schema_contract.rs`,
`docs/development-evidence/v1-g0-03-preflight.json`,
`docs/superpowers/plans/2026-08-28-vestrace-v1-g0-03-provider-execution-foundation.md`,
`migrations/0176_connection_revisions_and_no_auth_bindings.sql`,
`migrations/0177_qualification_jobs_and_revisions.sql`,
`migrations/0178_model_revisions_and_binding_snapshots.sql`,
`migrations/0179_model_request_evidence.sql`,
`migrations/0180_connection_admission_and_credential_leases.sql`,
`migrations/0181_credential_activation_and_rotation.sql`,
`migrations/0182_provider_effect_atomic_dispatch.sql`, `scripts/p03-scope.mjs`,
and `tests/p03_scope.test.mjs`.

`protocol-lock --check` matters independently: it proves the P01 protocol
baseline still reconstructs byte for byte after everything P02 added.

Task 14 Step 2, repository hygiene, is **not satisfied**: `git diff --check`
exits `2`, reporting `docker-compose.yml:322: new blank line at EOF.` and
`docs/getting-started.md:136: new blank line at EOF.` Both are P03 edits to
shared paths, not P02 changes, and must not be fixed here. The working tree
carries more paths than the preflight's dirty set. The difference includes the
files P02 created, every one inside `change_scope_paths`, and the 20 P03-only
paths listed above outside P02 scope — which is
not a digest guarantee. The verifier checks the four protected-authority
digests; byte identity (status, size, and SHA-256) for baseline dirty paths
outside `change_scope_paths`; and membership-only for new dirty paths. A
recorded `baseline_amendments` exception is the only stated exception to that
baseline-byte comparison. It deliberately does not digest-compare in-scope
paths. Therefore, the narrowness of in-scope edits to pre-existing dirty files
is held by review and by each amendment's recorded intent, not by the tool.
P02 changed 18 of the 136 pre-existing dirty files, so criterion 16 does not
literally hold for those files. Three have byte-exact reconstruction controls:
`.env.example` is `611 -> 657` bytes and reconstructs by removing the one added
`VESTRACE_INSTALLATION_FINGERPRINT_VAULT_ROOT` line; `crates/vestrace-http/tests/request_span.rs`
is `13727 -> 13727` bytes and reconstructs by restoring its one
`StatusCode::NOT_FOUND` token; and `tests/effect_fault_runtime.rs` is
`16049 -> 16052` bytes and reconstructs by returning the three functional
deadlines on lines 157, 179, and 217 from ten seconds to two. At P02's own run,
the remaining fifteen could not be reconstructed from their recorded digests
alone, so their conformance to their amendments remains unproved rather than
implied by this scoped verifier:

```text
Cargo.lock                                             97975 -> 98005
crates/vestrace-cli/src/commands/conformance.rs       141656 -> 141733
crates/vestrace-cli/src/commands/mcp.rs                 3842 -> 4101
crates/vestrace-cli/src/commands/server.rs             39627 -> 47090
crates/vestrace-cli/tests/v1_release_gate_cli.rs       61903 -> 62340
crates/vestrace-domain/src/external_effects.rs         46414 -> 50581
crates/vestrace-fault-scenario/src/main.rs             16314 -> 30522
crates/vestrace-fault-scenario/tests/child_points.rs    5768 -> 7619
crates/vestrace-http/tests/router_contract.rs          16559 -> 17363
crates/vestrace-infrastructure/src/postgres/outbox_repository.rs  8727 -> 9562
docker-compose.yml                                     15981 -> 16598
scripts/verify-dirty-baseline.mjs                       8238 -> 10655
tests/external_corpus_manifest.rs                       7843 -> 9125
tests/protocol_a2a_lock.rs                              2741 -> 3326
tests/protocol_q1_manifest.rs                          17854 -> 19580
```

These are P02-era measurements, not a claim about the current cohabiting tree.
All but `docker-compose.yml` and `scripts/verify-dirty-baseline.mjs` still
match their right-hand P02 byte counts; P03 later advanced those two shared
paths. The current values and deltas are recorded in the Round 4 disclosure
below.

`scripts/verify-dirty-baseline.mjs` is itself one of those fifteen: it is
baseline-dirty, changed inside `change_scope_paths`, and therefore exempt from
the verifier's own byte comparison. Its successful in-scope comparison is not
an independent witness for that tool; `protocol-lock.mjs --check` is, and the
narrowness tests under `tests/` exercise the verifier's logic.

Criterion 1's forward direction is closed structurally for inventory paths and
registration. The runtime suite asserts that each descriptor's own method does
not answer 405, which detects a handler mounted under a different verb. A method
absent from the inventory is refused 403 before routing reaches axum; axum 0.8.9
provides no sound public `MethodRouter` introspection that could prove the
descriptor method directly.

The 187 targets are 179 test binaries and 8 doc-test targets. The ten ignored
cases are named and unchanged: five need LM Studio or a separately built
fault-scenario program, four are the Compose smoke, and one is the real-process
fault-scenario E2E. None is a P02 assertion that was skipped.

**Resolved Task 14 gate finding:** the fresh default-parallel command initially
reached `tests/effect_fault_runtime.rs` with **12 passed, 2 failed**. Both
failures came from concurrent `powershell.exe` startup exceeding a two-second
scaffolding deadline; each test passed alone and the whole binary passed 14/14
with `--test-threads=1`. The operator authorized scope amendment 130 -> 131.
Only the three functional child-process deadlines changed from 2 to 10 seconds;
the dedicated hard-timeout assertion remains 20 milliseconds and passes.
Afterward the default-parallel binary passed 14/14 five consecutive times, and
`cargo test --workspace --locked` completed with exit 0 against real
PostgreSQL.

The baseline verifier was checked against a control in the same session: run
with the P01 preflight and no P02 scope it exits `1` and names the P02-modified
paths, so its `exit 0` above is a comparison that happened, not a silent pass.

## Governed non-read inventory and P02 migration boundary

This inventory was freshly enumerated from `ROUTE_INVENTORY` by selecting every
`governed!(POST|PUT|PATCH|DELETE, ...)` descriptor. It contains **38** governed
non-read HTTP paths:

```text
POST /ag-ui/run
POST /v1/runs
POST /v1/runs/{id}/steps
POST /v1/runs/{id}/pause
POST /v1/runs/{id}/resume
POST /v1/runs/{id}/cancel
POST /v1/runs/{id}/approve
POST /v1/events
POST /v1/memories
DELETE /v1/memories/{id}
POST /v1/memories/{id}/revisions
POST /v1/retrieval/search
POST /v1/agents
POST /v1/models
POST /v1/providers
POST /v1/skills
POST /v1/routing/decisions
POST /v1/executions
POST /v1/workflow-executions
POST /v1/workflow-executions/{id}/steps
POST /v1/workflow-executions/{id}/complete
POST /v1/workflow-executions/{id}/outcomes
POST /v1/workflows
POST /v1/evaluations
POST /v1/evaluation-facts
POST /v1/learning/projections
POST /v1/learning/proposals
POST /v1/learning/proposals/{id}/submit
POST /v1/capability-grants
POST /v1/capability-grants/{id}/revoke
POST /v1/access-tokens
DELETE /v1/access-tokens/{id}
POST /v1/secrets
DELETE /v1/secrets/{id}
POST /v1/secrets/{id}/value
PUT /v1/settings
POST /v1/effects
POST /v1/system/health/findings/{id}/disposition
```

P02 migrated exactly **one** of these paths end-to-end: `POST /v1/access-tokens`.
Its governed mutation, audit, idempotency, and outbox writes share one
transaction. The other **37 are explicit outstanding obligations**; P02 does
not claim that spec 11.3 holds system-wide.

`POST /v1/retrieval/search` appears in the governed non-read list because it is
a governed non-read method, but it is query-like; P02 does not force mutation
authority onto it.

The current scoped P02 file list was mechanically read from
`scripts/p02-scope.mjs` (`changeScopePaths`, 132 paths):

```text
.env.example
Cargo.lock
Cargo.toml
crates/vestrace-application/src/credential/commands.rs
crates/vestrace-application/src/fault_evidence.rs
crates/vestrace-application/src/fault_runtime.rs
crates/vestrace-application/src/fault_suite.rs
crates/vestrace-application/src/governed_mutation.rs
crates/vestrace-application/src/idempotency.rs
crates/vestrace-application/src/identity.rs
crates/vestrace-application/src/installation/permit.rs
crates/vestrace-application/src/lib.rs
crates/vestrace-application/src/material/commands.rs
crates/vestrace-application/src/material/erasure.rs
crates/vestrace-application/src/material/vault.rs
crates/vestrace-application/src/outbox.rs
crates/vestrace-application/src/ports.rs
crates/vestrace-application/src/security/audit.rs
crates/vestrace-cli/src/commands/conformance.rs
crates/vestrace-cli/src/commands/mcp.rs
crates/vestrace-cli/src/commands/server.rs
crates/vestrace-cli/src/commands/worker.rs
crates/vestrace-cli/tests/runtime_schema_gate.rs
crates/vestrace-cli/tests/v1_release_gate_cli.rs
crates/vestrace-domain/Cargo.toml
crates/vestrace-domain/src/credential/guard.rs
crates/vestrace-domain/src/credential/intent.rs
crates/vestrace-domain/src/credential/revision.rs
crates/vestrace-domain/src/credential/slot.rs
crates/vestrace-domain/src/external_effects.rs
crates/vestrace-domain/src/installation/fingerprint.rs
crates/vestrace-domain/src/lib.rs
crates/vestrace-domain/src/material/content.rs
crates/vestrace-domain/src/material/identity.rs
crates/vestrace-domain/src/material/intent.rs
crates/vestrace-domain/src/material/mod.rs
crates/vestrace-domain/src/material/size_class.rs
crates/vestrace-domain/tests/material_contract.rs
crates/vestrace-fault-scenario/Cargo.toml
crates/vestrace-fault-scenario/src/child.rs
crates/vestrace-fault-scenario/src/main.rs
crates/vestrace-fault-scenario/src/report.rs
crates/vestrace-fault-scenario/src/scenarios/credential_intent_crash.rs
crates/vestrace-fault-scenario/src/scenarios/material_intent_crash.rs
crates/vestrace-fault-scenario/src/settings.rs
crates/vestrace-fault-scenario/tests/child_points.rs
crates/vestrace-fault-scenario/tests/output_contract.rs
crates/vestrace-http/src/api/access_tokens.rs
crates/vestrace-http/src/api/ag_ui.rs
crates/vestrace-http/src/api/agents.rs
crates/vestrace-http/src/api/artifacts.rs
crates/vestrace-http/src/api/audit.rs
crates/vestrace-http/src/api/capability_grants.rs
crates/vestrace-http/src/api/connections.rs
crates/vestrace-http/src/api/context.rs
crates/vestrace-http/src/api/effects.rs
crates/vestrace-http/src/api/error.rs
crates/vestrace-http/src/api/evaluations.rs
crates/vestrace-http/src/api/executions.rs
crates/vestrace-http/src/api/learning.rs
crates/vestrace-http/src/api/memory.rs
crates/vestrace-http/src/api/metrics_summary.rs
crates/vestrace-http/src/api/mod.rs
crates/vestrace-http/src/api/models.rs
crates/vestrace-http/src/api/profile.rs
crates/vestrace-http/src/api/retrieval.rs
crates/vestrace-http/src/api/routing.rs
crates/vestrace-http/src/api/runs.rs
crates/vestrace-http/src/api/secrets.rs
crates/vestrace-http/src/api/settings.rs
crates/vestrace-http/src/api/skills.rs
crates/vestrace-http/src/api/system.rs
crates/vestrace-http/src/api/triggers.rs
crates/vestrace-http/src/api/workflow_executions.rs
crates/vestrace-http/src/api/workflows.rs
crates/vestrace-http/src/auth.rs
crates/vestrace-http/src/lib.rs
crates/vestrace-http/src/route_inventory.rs
crates/vestrace-http/src/router.rs
crates/vestrace-http/tests/request_span.rs
crates/vestrace-http/tests/route_inventory_is_exhaustive.rs
crates/vestrace-http/tests/router_contract.rs
crates/vestrace-infrastructure/src/crypto/material_vault.rs
crates/vestrace-infrastructure/src/crypto/mod.rs
crates/vestrace-infrastructure/src/postgres/access_token_repository.rs
crates/vestrace-infrastructure/src/postgres/audit_repository.rs
crates/vestrace-infrastructure/src/postgres/credential_guard.rs
crates/vestrace-infrastructure/src/postgres/credential_intent.rs
crates/vestrace-infrastructure/src/postgres/erasure.rs
crates/vestrace-infrastructure/src/postgres/idempotency_repository.rs
crates/vestrace-infrastructure/src/postgres/installation_fingerprint.rs
crates/vestrace-infrastructure/src/postgres/installation_permit.rs
crates/vestrace-infrastructure/src/postgres/material_intent.rs
crates/vestrace-infrastructure/src/postgres/mod.rs
crates/vestrace-infrastructure/src/postgres/outbox_repository.rs
crates/vestrace-infrastructure/src/postgres/pool.rs
crates/vestrace-infrastructure/src/postgres/transaction.rs
crates/vestrace-infrastructure/tests/credential_guards.rs
crates/vestrace-infrastructure/tests/credential_intent_lifecycle.rs
crates/vestrace-infrastructure/tests/erasure_holds_no_lock_during_vault_call.rs
crates/vestrace-infrastructure/tests/erasure_is_one_way.rs
crates/vestrace-infrastructure/tests/fingerprint_continuity_is_fail_closed.rs
crates/vestrace-infrastructure/tests/governed_mutation_is_atomic.rs
crates/vestrace-infrastructure/tests/installation_permit_excludes.rs
crates/vestrace-infrastructure/tests/intent_crash_boundaries.rs
crates/vestrace-infrastructure/tests/material_intent_lifecycle.rs
crates/vestrace-infrastructure/tests/material_vault_contract.rs
crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs
docker-compose.yml
docker/postgres/init-runtime-role.sh
docs/development-evidence/v1-g0-02-preflight.json
docs/development-evidence/v1-g0-02-security-material-foundation.md
docs/getting-started.md
migrations/0165_governed_mutation_audit_atomicity.sql
migrations/0166_guarded_operation_owner_role.sql
migrations/0167_installation_fingerprint_continuity.sql
migrations/0168_installation_mutation_permit.sql
migrations/0169_material_key_creation_intents.sql
migrations/0170_content_material_guards.sql
migrations/0171_connection_execution_guards.sql
migrations/0172_credential_slots_and_revisions.sql
migrations/0173_credential_key_creation_intents.sql
migrations/0174_material_erasure_primitives.sql
migrations/0175_runtime_migration_history_read.sql
scripts/p02-scope.mjs
scripts/verify-dirty-baseline.mjs
tests/effect_fault_runtime.rs
tests/effect_fault_scenario_e2e.rs
tests/external_corpus_manifest.rs
tests/p02_scope.test.mjs
tests/protocol_a2a_lock.rs
tests/protocol_q1_manifest.rs
```

## A correction, and the defect it was hiding

An earlier revision of this note recorded, as an unfixable defect, that a
material intent crashed before `ContentPrepared` had no lawful terminal and its
reserved vault key could never be erased. It justified not fixing it by saying
the spec mandates a "ContentPrepared-only" abort branch, so the plan's
requirement of a terminal at every boundary and the spec could not both hold.

**That reading was wrong, and the independent review caught it.** Spec line 1045
provides a *distinct* pre-prepared abort: a pre-Prepared unbound intent whose
input is unavailable "completes only its existing guarded pre-prepared
abort/unbound-erase/witnessed-receipt path", and "no synthetic ciphertext or
ContentPrepared-only abort marker is accepted". Spec 313 names the operation —
`erase_unbound_provisional_key`, admitted before any `ContentPrepared`,
`ResultPrepared` or `Bound` marker on proof of no prepared, result or live
material. Spec 728 states plainly that `Abandoned` follows "a committed guarded
pre-prepared **or** ordinary prepared-abort branch". The "ContentPrepared-only"
phrase in spec 813 describes the *ordinary* branch, not the only one.

There was no contradiction between plan and spec. The material lifecycle was
simply missing a branch the spec requires, and the ordinary ContentPrepared
abort had been mistaken for the whole of it. The branch is now implemented:
state `pre_prepared_abandon_prepared`, guarded creator
`vestrace_prepare_pre_prepared_material_abandon`, admitted to the unbound-key
erase and the abandon finalizer alongside the ordinary branch, and kept distinct
from it by the deferred invariant — a pre-prepared abort carries no
`prepared_marker` and no material row.

The deferred invariant is not the sole distinction. The database enforces the
pre-prepared branch independently through the creator's state check and
no-marker/no-material proof, the deferred invariant, and the exact `(state,
marker)` pairs admitted by `vestrace_finalize_material_key_abandon`.

All seven material boundaries now reach a lawful terminal, under the real
harness as well as in-process.

## ResultPrepared authority boundary

`vestrace_prepare_result_material(UUID, UUID, BYTEA, BIGINT)` is now unreachable
by the restricted `vestrace` runtime role: the runtime EXECUTE entry was removed
from both the migration bridge and the Compose runtime-role provisioner. The
restricted-role regression invokes the function and observes exact SQLSTATE
`42501`, in addition to verifying the catalog privilege is absent. The
owner-issued transition remains available and the in-process
`after_prepared_before_bound` boundary deliberately resumes as `Parked` in
`ResultPrepared`; it is not converted to `Terminal`. The package that first
introduces result material must introduce the runtime grant and a bind-receipt
recovery authority together.

## Review-repair record

### Credential pre-live-abandon state guard isolation limit

`pre_live_abort_state_clause_is_independently_reachable` now exercises the
state-list refusal through guarded functions in one open transaction. It builds
a valid `credential_prepared` intent, calls
`vestrace_bind_credential_key_creation_intent` with a NULL receipt, and proves
the intermediate `bound` state has no bound receipt while its occupancy remains
at the exact `preparing` version with no lifecycle event or erasure receipt.
The following guarded pre-live-abandon call then returns SQLSTATE `23514` with
the state-list composite's refusal message. The deferred invariant would reject
that malformed `bound` state at commit, so the test rolls the transaction back
after the assertion. This is an independently reachable case: the state-list
disjunct alone is true, while the receipt and both fact-existence disjuncts are
false. It replaces the earlier, false claim that every guarded `bound` state
necessarily carries a bound receipt.

The mechanism was proved by breaking and restoration. Temporarily changing only
the state-list predicate in
`migrations/0173_credential_key_creation_intents.sql` line 495 to `IF FALSE`,
while retaining the receipt and both fact-existence disjuncts and the `RAISE`,
made `pre_live_abort_state_clause_is_independently_reachable` fail at exit
`101`: the guarded call returned `PgQueryResult { rows_affected: 1 }` instead
of the expected SQLSTATE `23514`. Restoring the exact predicate restored the
migration SHA-256 `31F94774F1701A2EF5EE28F511B2D86BE9C9AFF4C6B32647F9966F1F3DC73BBD`;
the focused test then passed at exit `0`, and the full
`credential_intent_lifecycle` binary passed **16, 0 failed** at exit `0`.

This repair isolates the audit foreign key by name; removes the runtime
`EXECUTE` grant on `vestrace_prepare_result_material` from both grant mechanisms
so `result_prepared` is unreachable by the runtime role; adds the narrow
erasure-tail trigger to `material_key_creation_intents`; derives the
authentication exemption from the route inventory and enforces at router
construction that a governed route cannot duplicate a public path pattern. The
check deliberately compares literal patterns rather than path overlap, so this
narrower statement must not be widened to a shared-path claim; and
proves all four `/v1/access-tokens` routes with real requests rather than a
static table. These are scoped repair facts, not a repeated independent review
or a release claim.

## Deployment migration correction

Until this correction, **no P02 migration had ever been applied through the
real Compose deployment path**. The lead engineer's first Compose smoke,
`cargo test --test compose_smoke compose_smoke_health_ready -- --ignored
--test-threads=1`, stopped at migration 0166 with `must be able to SET ROLE
"vestrace_guarded_owner"`; a fresh volume therefore stopped at 0165. The
per-test `sqlx::test` databases hid that failure because `test` is a superuser.

The initial response pointed `vestrace-migrate` at `vestrace_bootstrap`. That
second Compose smoke completed migrations and `dev-seed`, but server and worker
then exited 1 because their runtime composition roots still called
`store.migrate()`. Those roots now perform only the existing fail-closed schema
compatibility check; `vestrace migrate` remains the administrative DDL command.
MCP had no stale-schema gate before this work, so it now uses the same three-arm
compatibility refusal as server and worker.

Removing runtime DDL exposed the third layer: when an administrative migrator
owned `_sqlx_migrations`, the restricted runtime role could not read it and the
compatibility gate correctly failed closed with SQLSTATE `42501`. Migration 0175
grants only `SELECT` on that history table. It is retained even though the
replacement design again makes `vestrace` its owner: it is harmless in that
case and preserves the compatibility gate for an already-admin-migrated
database instead of making it depend on table ownership.

The same runtime-DDL defect reached conformance and qualification tooling: four
`conformance.rs` entry points called `store.migrate()` through runtime database
configuration. They now use compatibility verification only; the two newly
gated paths use the same fail-closed three-arm handling as the two that already
had it.

The bootstrap-migrator change itself then exposed the fifth, distinct failure:
it silently reassigned 107 pre-existing runtime-owned tables to
`vestrace_bootstrap`, and runtime recovery failed to read `agent_runs`. That
ownership model belongs to P05 and must not be changed by P02. Compose now runs
the migrator as restricted `vestrace` again. Bootstrap installs a
`SECURITY DEFINER` bridge that accepts only the declared 26 P02 table names and
56 exact, schema-qualified P02 `regprocedure` identities, hardcodes
`vestrace_guarded_owner`, and grants runtime `EXECUTE` only to the exact 37
signatures selected by the migrations; `SET FALSE` membership is unchanged.
Ownership and both table ACL revocations occur only after the final P02 DDL,
because earlier handoff would prevent later P02 foreign-key and constraint
changes.

The first Task 14 independent adversarial review returned `VERDICT: REVISE` on
this bridge. Although the target OID was used for the ownership operation, both
the ownership allowlist and runtime-EXECUTE selection admitted it by `proname`
alone. Because production deliberately grants runtime `CREATE` on `public`, a
restricted caller could create an overload of an allowlisted name, pass that
new overload's `REGPROCEDURE` to the bridge, and have attacker-controlled
`SECURITY DEFINER` code transferred to `vestrace_guarded_owner`. The prior
evidence statement that the bridge admitted exact function identities was
therefore false.

The regression first failed against both bridge definitions: the restricted
runtime role successfully transferred
`public.vestrace_record_guarded_operation_probe(text)`. The production
provisioner and the superuser-only `sqlx::test` fallback now compare the target
OID against explicit `to_regprocedure('public.name(types)')` arrays. The same
exact-identity rule selects runtime EXECUTE grants. Both restricted-runtime
paths now refuse that unlisted overload with SQLSTATE `42501`; the production
migration-path test also verifies all 56 ownership identities and exact
agreement between the 37 runtime-executable identities and the migrations'
written grant intent.

The post-fix verification run on 2026-08-28 observed
`runtime_role_cannot_write_directly` **37 passed, 0 failed** and
`cargo test --workspace --locked` exit 0 against real PostgreSQL. `cargo check
--workspace --all-targets`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo fmt --all -- --check`, the protocol lock,
P01 text hygiene and P02 scope tests (3/3) also exited 0. In the present
P03-sharing round, `git diff --check` exits `2` for the P03 shared-file blank
lines in `docker-compose.yml:322` and `docs/getting-started.md:136`.
The protected `memory.rs`, `retrieval.rs`, and `runs.rs` hashes remained exactly
the preflight values.

The final P02 dirty-baseline verifier exits 0 in this repair slice. That is a
scoped baseline check only. The repeated independent review found that the
protected preflight amendment for `tests/effect_fault_runtime.rs` authorized
three functional deadline changes in its `reason`, while its
`preservation_note` said only two may change. Reconstructing the pinned
pre-amendment file requires reverting all three changes. The operator explicitly
authorized the exact wording correction, and `preservation_note` now says
`three`; no other field in that protected record changed. The repeated
independent review reconstructed the pinned 16,049-byte pre-P02 file by
reverting exactly those three deadline changes, confirmed its SHA-256, confirmed
that the 20-millisecond hard-timeout was unchanged, found no material finding,
and returned `VERDICT: APPROVE`. Task 14 Step 4 is complete.

The bridge cannot transfer `agent_runs`, choose a different target owner, or
touch an undeclared P02 object. Its residual risk is deliberate and bounded:
a runtime caller can invoke the fixed bridge to hand a declared P02 object to
the fixed guarded owner, including if that P02 table is nonempty. An emptiness
bound would be incorrect because migration 0168 deliberately inserts the
durable watermark before the final handoff.

The real-PostgreSQL regression applies the full P02 set as restricted runtime
`vestrace` against a per-test database with a bootstrap-equivalent bounded
bridge, verifies 107 runtime-owned legacy tables, exactly 26 guarded P02 tables,
and runtime `INSERT`, `UPDATE`, and `DELETE` on `agent_runs`. It does not execute
the actual Compose bootstrap identity or its initialization script; the previous
administrator regression likewise did not exercise Compose's
`vestrace_bootstrap` identity. The base `vestrace_test` development database
already records migrations 0166--0174, so their edited checksums require that
local database to be recreated before it can be used as a migration target.

The fourth Compose smoke reached readiness for the first time: migrations ran as
`vestrace`, the durable fingerprint-vault volume contained
`installation-fingerprint-v1.json` under UID 10001 in a `0700` directory, and
the server stayed running. It then remained unhealthy with the retained cause
`create_only_record_rejected`: the runtime lacked `EXECUTE` on
`vestrace_record_installation_fingerprint_continuity`. This was the sixth,
distinct Compose failure. Deferring P02 function ownership handoff had caused
the original runtime self-grants to be removed when each function changed owner.
The bounded bootstrap bridge now revokes `PUBLIC` and reapplies exactly the
original per-function runtime grant intent as guarded owner during that handoff;
internal helpers remain ungranted.

The fifth Compose smoke ran the complete readiness/observability set
(`health_ready`, `metrics_endpoint`, `doctor_in_container`, and
`no_sensitive_labels_in_metrics`). All four exposed the same seventh failure:
runtime `EXECUTE` on the guarded fingerprint function succeeded, but its
`SECURITY DEFINER` insert was refused for
`installation_fingerprint_continuity`. The pre-handoff table revokes had
materialized a runtime-owner ACL; the later owner handoff removed that entry and
left every one of the 26 P02 table ACLs as `{}`. This is the first Compose defect
the existing PostgreSQL suite actively concealed, because its separately
written test bridge differed from deployment provisioning. The runtime
migration regression now applies the bounded bridge extracted from
`init-runtime-role.sh`, asserts no guarded P02 table has an empty ACL, and
proves an actual runtime call through the guarded fingerprint writer persists a
row. Table ACL revocation is now performed by that bridge after ownership
handoff.

The sixth clean-volume Compose run exercised the full readiness/observability
suite. `health_ready` and `doctor_in_container` passed: the seven-link
deployment readiness chain above was closed and the deployed stack reached
ready. `metrics_endpoint` and `no_sensitive_labels_in_metrics` both failed for
the eighth, distinct authorization-scope gap: `/metrics` is correctly governed,
but the bootstrap `audit.read` grant was scoped to `/v1` and could not authorize
it. Bootstrap seeding now creates one additional exact low-risk `audit.read` /
`http.get` / `/metrics` grant; it does not widen `/v1` or make metrics public.

The lead engineer ran every test in the seventh clean-volume Compose suite:
`health_ready`, `metrics_endpoint`, `doctor_in_container`, and
`no_sensitive_labels_in_metrics` all passed. That confirmed the metrics repair
on the real deployment. The same probe then found the ninth and final gap:
`/ag-ui/endpoints` was governed and has a console client method, but that
method is currently unexercised; the shipped console calls `/ag-ui/run` and
`/ag-ui/events/stream`. The bootstrap `execution.read` and `execution.write`
grants remained scoped to `/v1`. Bootstrap seeding therefore now creates
exactly the three inventory
grants `execution.read` / `http.get` / `/ag-ui/endpoints` / low,
`execution.write` / `http.post` / `/ag-ui/run` / high, and `execution.read` /
`http.get` / `/ag-ui/events/stream` / low. They are exact resource scopes, not
wildcards or prefixes, and none makes an AG-UI route public. The endpoint grant
is therefore an explicitly unexercised exact low-risk default grant, supported
by the governed endpoint and available client API rather than a nonexistent
shipped call path.

This seeds a high-risk `execution.write` grant by default. The judgement is
deliberate and explicit: the bootstrap principal already held that same
capability at high risk for `/v1`, including the high-risk `POST /v1/runs`; this
extends only its resource scope to the shipped AG-UI equivalent, introducing no
new capability or risk category. The nine-finding Compose chain is closed: the
seven deployment-readiness links, the metrics scope gap, and the AG-UI scope
gap. It was invisible to 1,377 workspace tests, clippy, and an independent
reviewer because the deciding Compose smoke is `#[ignore]` and Docker was
unavailable to all three of us.

The lead engineer later re-ran the suite on 2026-08-28, rather than leaving the
result inferred from the earlier reports:
`cargo test --test compose_smoke -- --ignored --test-threads=1` reported
**4 passed, 0 failed** in 122.72s — `compose_smoke_health_ready`,
`compose_smoke_metrics_endpoint`, `compose_smoke_doctor_in_container`, and
`compose_smoke_no_sensitive_labels_in_metrics`. Each case begins and ends with
`docker compose down -v`, so all four migrated a fresh volume through the real
deployment path; the readiness case alone had already passed the same way in a
separate 29.15s run. The nine repairs are now confirmed by a second independent
execution. The project and its volumes were removed afterwards, and the working
tree path count is unchanged.

## Proof by breaking

Assertions verified to fail when the mechanism they describe is removed, then
restored and re-run green.

- Setting `prepared_marker = 'content_prepared'` in the pre-prepared abort — that
  is, letting a ContentPrepared-only marker stand in for the distinct branch, the
  exact substitution spec 1045 refuses — fails all three
  `material_crash_after_{reserved,vault_create_before_receipt,receipt_before_prepared}`
  tests.

- Changing the erasure Audit action string fails `metadata_and_audit_survive_erasure`.
- Converting the one-way trigger to `BEFORE INSERT` so it cannot see an update
  fails `state_reversal_is_refused_for_the_guarded_owner`, and the failure mode is
  the significant part: **without the trigger the guarded owner successfully
  returns a tombstoned content material to `live`.** The trigger closes a
  reachable reversal, not a hypothetical one.

## Outstanding obligations

These are stated as obligations because nothing in this package discharges them.
None is a defect in the code as written; each is a limit on what the passing
tests mean.

**1. The erasure blocker registry has no production writer.**
`material_erasure_blockers` records the three blocker classes the spec names —
`effect`, `lease`, `intent` — and both erasure preparers consult it and refuse
while a nonterminal one exists. But no production path writes a row into it.
`external_effect_intents` carries no material reference, and the
reference/hold/lease tables the spec's retention migrations describe belong to
P03–P05. So `blocker_refuses_preparation` proves the *gate* works against rows a
test inserted through the guarded writer; it does not prove any real effect or
lease can block an erasure today. Later packages must bind their concrete
effects, leases, and intents to these exact target identities, and until they do
the blocker leg has never fired on production data.

Once written, an erasure blocker for `effect` or `intent` cannot be cleared:
`vestrace_record_material_erasure_blocker` always writes `nonterminal`, nothing
terminalizes one, and direct DML is refused. Those kinds are permanent; only
`lease` kinds age out through `usable_until`.

**2. Credential destruction is keyed on `Candidate`, not on `Retired`/`Revoked`.**
The spec assigns two-phase credential material destruction to `Retired` or
`Revoked` revisions. Those states do not exist in P02 — credential activation and
rotation are P03 — so `vestrace_prepare_credential_material_erasure` admits an
exact `Candidate` whose occupancy is `candidate`. P03 must revisit this
precondition when it introduces the revision lifecycle, and must preserve
"`Active` is never destructible" at that point.

**3. `FingerprintKeyContinuityProof` is not pinned in backup manifests or an
independent witness.** Recorded by the plan as a P05 dependency; unchanged here.

**4. Least-privilege ownership covers P02's own tables only.** Pre-existing
tables remain runtime-role-owned. P05 scope.

**5. Task 13 Step 2 scenario ciphertext is padded bytes, not sealed content.**
The two crash scenarios drive real guarded transitions and use the real
`HostMaterialKeyVault` for key create, receipt, fence and erase — the parts a
crash boundary actually concerns. They do not encrypt: the lifecycle constrains
ciphertext to its disclosed size class and nothing else, so the scenarios write
padded bytes rather than pretending to seal content they never had. Encryption
under a per-material DEK is covered by `material_vault_contract`, not here.

**6. Fingerprint readiness diagnostics use stderr rather than the deployment's
structured tracing channel.** P02 deliberately did not add a logging dependency
to the infrastructure crate. The retained-cause transition gate bounds stderr
output, but routing these diagnostics through the deployment's tracing policy is
an outstanding observability decision for a later package.

## Round 4 disclosure: live P03 cohabitation

P03's preflight was captured at `2026-08-28T22:42:19.482Z` against HEAD
`6aba953`. After that capture, P03 advanced six paths which are also in P02
scope. Comparing the P03-recorded bytes with the live tree gives:

| Shared P02 path | P03 preflight bytes | Current bytes | Delta |
| --- | ---: | ---: | ---: |
| `crates/vestrace-domain/src/lib.rs` | 6066 | 6721 | +655 |
| `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs` | 48645 | 91969 | +43324 |
| `docker-compose.yml` | 16598 | 17253 | +655 |
| `docker/postgres/init-runtime-role.sh` | 14759 | 26462 | +11703 |
| `docs/getting-started.md` | 4563 | 5070 | +507 |
| `scripts/verify-dirty-baseline.mjs` | 10655 | 10725 | +70 |

P02's verifier deliberately excludes in-scope paths from byte comparison, so
it cannot see this drift. Two earlier test figures required correction during
Round 4: `credential_intent_lifecycle 15 passed, 0 failed` predated P02's R1
test and is now **16 passed, 0 failed** in the Commands observed table; and the
`runtime_role_cannot_write_directly 37 passed, 0 failed` figure in the post-fix
record is stale. The shared runtime-role binary now reports **41 passed, 0
failed**, because P03 added four tests. For these six paths P02's evidence
boundary is not separable from P03's, and criterion 16's guarantee does not
extend to them.

The current red-gate tally is four, all caused by the live P03 cohabitation:
the P02 dirty-baseline verifier exits `1` with the 20 P03-only paths; `cargo
test --workspace --locked` exits `101` on P03's deliberate `BLOCKED finding 1`;
Task 14 Step 2's `git diff --check` exits `2` on the two P03 shared-file EOF
findings; and both workspace clippy variants exit `101` because P03's
`crates/vestrace-domain/src/models/admission.rs` has `from_persisted` and
`build` at eight arguments, triggering `clippy::too_many_arguments` (8/7)
under `-D warnings`. The clippy file is listed in `scripts/p03-scope.mjs` and
is outside P02 scope.

What remains independently true is precise: `node scripts/protocol-lock.mjs
--check .` exits `0`; P02's original `runtime_executable_targets` array at
`docker/postgres/init-runtime-role.sh:200` still contains exactly 37 entries
(P03 has a separate four-entry array later in the file); and the P02 binaries
are green: `governed_mutation_is_atomic` 7,
`installation_permit_excludes` 5, `credential_guards` 6,
`erasure_holds_no_lock_during_vault_call` 3, `erasure_is_one_way` 16,
`intent_crash_boundaries` 15, and `credential_intent_lifecycle` 16. The shared
runtime-role binary is green at 41/41, but its count is not P02-only evidence.

P03 also has a second, mechanism-established shared-tree side effect. A P02
binary compiled before P03 migrations 0176--0182 landed reported
`fingerprint_continuity_is_fail_closed` as **11 passed, 1 failed**:
`production_readiness_reloads_the_host_fingerprint_record` returned
`cause=database_unavailable, error=unavailable: database is not ready`.
This is a false readiness failure, not a P02 defect. `PgStore::check` delegates
to `migrations_are_compatible`, which compares applied `_sqlx_migrations` rows
with the count and checksums in the `MIGRATOR` embedded at compile time. P03's
new migrations were absent from that already-compiled migrator, so the counts
disagreed. Rebuilding the infrastructure crate is the remedy, not a code
change: after rebuild, `cargo test -p vestrace-infrastructure --test
fingerprint_continuity_is_fail_closed` exits `0` with **12 passed, 0 failed**.

Criterion 8's literal storage wording is narrower than the table: the
`key_record_is_never_stored_in_postgres` test pins
`installation_fingerprint_continuity` to `installation_id`, `fingerprint_key_id`,
`fingerprint_key_version`, `continuity_proof`, and `created_at`. No key material
is stored, so the criterion's substance holds, but the identifiers and key
version are additional stored metadata.

Separation was attempted by the lead engineer and is not a safe current
workaround: after moving the 20 P03-only paths, `cargo fmt` failed with
`failed to resolve mod connection`, because the P03-expanded shared
`crates/vestrace-domain/src/lib.rs` declares modules whose files had been
moved. The tree was restored and all 20 digests were re-verified. A clean P02
run is therefore unreachable without reverting P03's changes to these six
shared files; they have no independent pre-P03 baseline to restore.

## What P02 does not implement

No Connection or Model revision, provider execution, qualification, embedding,
restore or backup supervision, protocol endpoint, or console workflow. No LM
Studio or remote API was contacted; no browser, TCK, accessibility, or release
qualification was established. The lead-engineer Compose smoke establishes only
deployment readiness, metrics, doctor, and sensitive-label behavior; it is not
release qualification. G0 and v1.0 remain incomplete.
