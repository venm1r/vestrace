# Vestrace P03 Task 11 Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` task by task. The `$cdx` role split remains Sol lead/reviewer -> one persistent Terra builder -> Sol quality gate. Do not commit, push, deploy, or call a non-loopback provider; replace commit instructions from generic workflows with explicit review checkpoints.

**Goal:** Close P03 Task 11 by accepting confidential agent-step input through the existing content-material lifecycle, executing the exact pinned provider attempt through the governed dispatch/result authorities, exposing only exact HTTP mutation commands, and proving production server/worker composition without weakening the frozen P03 controls.

**Architecture:** `POST /v1/runs` retains a safe public title. `POST /v1/runs/{id}/steps` consumes a bounded, zeroizing input only for agent steps. A single governed acceptance service commits the canonical step, pinned binding, immutable attempt/material identities, Request-Id/Audit/outbox evidence, and material reservation in one caller-owned transaction; it then performs the idempotent vault/codec lifecycle and enqueues the deterministic work item only after material and MRE are complete. The worker rediscovers that attempt, reconstructs the request from durable encrypted material, evaluates both authorization and model-data policy, calls the pinned adapter once, and delegates result publication to the already-proved finalizer. No title, config model, mutable head, legacy provider row, or secret-backed factory participates in routing.

**Tech stack:** Rust 2024 workspace, Axum, Tokio, SQLx/PostgreSQL 17, serde, zeroize, existing Vestrace domain/application ports, Node scope tests, OpenAPI/TypeScript SDK contract tests.

**Approved design:** `docs/superpowers/specs/2026-09-01-vestrace-p03-task11-governed-run-input-design.md`

## Global constraints

- Work only in the amended P03 allowlist. Preserve all unrelated dirty files byte-for-byte.
- Keep migrations 0172-0185 and the existing P03 preflight byte-identical. Migration 0186 is the only forward migration that may change.
- Do not re-capture a preflight or protected baseline. The operator amendment is represented by the exact scope edit in Task 1.
- Use `DATABASE_URL=postgres://test:test@localhost:55432/vestrace_test` and `VESTRACE_RUNTIME_DATABASE_URL=postgres://vestrace:runtime-local-development-only@localhost:55432/vestrace_test` for SQLx evidence. Run SQLx suites serially.
- Never persist or log confidential input, a raw request digest, plaintext-derived hash, DEK, credential, or provider output. Errors and evidence contain only opaque IDs, closed codes, size/media classes, and lifecycle states.
- Never call a vault or provider adapter while a PostgreSQL transaction is open.
- Missing transaction-bound, policy, vault, adapter, routing, or publication authority fails closed. Do not invent a fallback.
- Every implementation slice requires an observed RED, restoration or minimal implementation, GREEN, and a Sol review checkpoint. No commit is authorized.

## Task 1: Admit the exact operator amendment

**Files:**

- Modify: `scripts/p03-scope.mjs`
- Modify: `tests/p03_scope.test.mjs`
- Add: `docs/superpowers/specs/2026-09-01-vestrace-p03-task11-governed-run-input-design.md`
- Add: `docs/superpowers/plans/2026-09-01-vestrace-p03-task11-completion.md`

**Step 1: Write the scope-count RED test**

Update `tests/p03_scope.test.mjs` to expect 136 change paths and 17 protected paths, and to assert that the following eleven paths are change-authorized:

```text
crates/vestrace-application/src/ports.rs
crates/vestrace-application/src/run/commands.rs
crates/vestrace-application/src/run/coordinator.rs
crates/vestrace-application/src/run/handlers/advance_run.rs
crates/vestrace-application/tests/run_coordinator.rs
crates/vestrace-http/src/api/ag_ui.rs
crates/vestrace-http/src/api/runs.rs
crates/vestrace-http/tests/run_routes.rs
crates/vestrace-infrastructure/src/postgres/material_intent.rs
docs/superpowers/specs/2026-09-01-vestrace-p03-task11-governed-run-input-design.md
docs/superpowers/plans/2026-09-01-vestrace-p03-task11-completion.md
```

Also assert that `crates/vestrace-http/src/api/runs.rs` is absent from `protectedAuthorityPaths`; retain exact membership assertions for every other protected path.

Run:

```powershell
node --test tests/p03_scope.test.mjs
```

Expected RED: count/membership mismatch against the current 125 change paths and 18 protected paths.

**Step 2: Apply the one exact scope amendment**

Add the eleven sorted paths to `changeScopePaths`; remove only `crates/vestrace-http/src/api/runs.rs` from `protectedAuthorityPaths`. Do not change the P03 preflight or verifier.

**Step 3: Prove scope and protected authority**

Run:

```powershell
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-03-preflight.json --scope p03-scope.mjs
```

Expected GREEN: scope tests pass, counts are exactly 136/17, and the dirty-baseline verifier reports only the already-authorized cdx cohabitation findings.

**Review checkpoint:** Sol compares the eleven admitted paths and the single protected-path removal against this plan. Any twelfth new path stops implementation for another explicit amendment.

## Task 2: Separate public Run title from confidential agent input

**Files:**

- Modify: `crates/vestrace-application/src/run/commands.rs`
- Modify: `crates/vestrace-application/src/run/coordinator.rs`
- Modify: `crates/vestrace-application/src/run/handlers/advance_run.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-application/tests/run_coordinator.rs`
- Modify: `crates/vestrace-http/src/api/runs.rs`
- Modify: `crates/vestrace-http/src/api/ag_ui.rs`
- Modify: `crates/vestrace-http/tests/run_routes.rs`

**Step 1: Add structural and route RED tests**

Add compile-time assertions in the application test surface that `ConfidentialRunInput` implements neither `Clone` nor `serde::Serialize`, and that its `Debug` output is exactly `ConfidentialRunInput([REDACTED])`. Add route tests using a unique sentinel which prove:

- `CreateRunRequest.title` remains accepted and returned as safe display metadata;
- an agent step without `input` is HTTP 400;
- an agent step with empty, whitespace-only, or more than 32,768 UTF-8 bytes is HTTP 400;
- a principal or system step with `input` is HTTP 400;
- an agent step with valid input reaches the governed acceptance mock by ownership, and neither response nor captured safe command contains the sentinel;
- AG-UI refuses its current message-to-objective execution bridge with a closed `governed_run_input_required` error and does not call the Run orchestrator.

Run:

```powershell
cargo test -p vestrace-application --test run_coordinator confidential_run_input -- --nocapture
cargo test -p vestrace-http --test run_routes governed_agent_input -- --nocapture
cargo test -p vestrace-http ag_ui -- --nocapture
```

Expected RED: the type and route contract do not exist; AG-UI still maps message plaintext to `CreateRun.objective`.

**Step 2: Introduce the closed input types**

In `run/commands.rs`, keep the existing field name `CreateRun.objective` for compatibility but document and validate it as public display metadata only. Add:

```rust
pub const MAX_CONFIDENTIAL_RUN_INPUT_BYTES: usize = 32_768;

pub struct ConfidentialRunInput(zeroize::Zeroizing<String>);

impl ConfidentialRunInput {
    pub fn parse(value: String) -> Result<Self, ApplicationError>;
    pub fn with_bytes<R>(&self, use_bytes: impl FnOnce(&[u8]) -> R) -> R;
}

pub enum NewRunStepInput {
    None,
    Confidential(ConfidentialRunInput),
}
```

Implement manual redacted `Debug`; do not implement `Clone`, `Serialize`, `AsRef<str>`, `Display`, or any plaintext accessor. Change `NewRunStepDto` to own `NewRunStepInput` and remove derived traits that would copy or reveal it.

**Step 3: Make the route consume input exactly once**

Add `input: Option<String>` to the HTTP request DTO with write-only OpenAPI metadata in Task 7. Parse actor first, enforce the actor/input matrix, and immediately convert an agent input to `ConfidentialRunInput`. Do not clone it and do not place it in `RunStep`, `RunEvent`, `RunReference`, tracing fields, response DTOs, or errors.

For non-agent steps preserve the existing coordinator path. For agent steps call the governed acceptance authority introduced in Task 3; until that authority is injected, return `ApplicationError::Unavailable("governed Run-step input authority is not configured")`.

**Step 4: Remove title-based scheduling and AG-UI leakage**

Keep title/objective persistence for display compatibility but remove every production read of `AgentRun.objective` from model execution. `AdvanceRunHandler` must not enqueue an agent step merely because `input_references` is empty; the Task 3 authority is the only code that enqueues it after input is Live and MRE is Complete. Replace AG-UI's CreateRun call with the typed refusal until P06 owns the governed command.

**Step 5: Re-run focused tests**

Run the three commands from Step 1 plus:

```powershell
rg -n "objective: updated_run\.objective|objective\.clone\(\).*StepModel|request\.message.*CreateRun" crates/vestrace-application/src crates/vestrace-http/src
```

Expected GREEN: focused tests pass and the search returns no production model-input bridge.

**Review checkpoint:** Sol checks move-only ownership, redacted formatting, byte—not scalar—limit, exact actor/input matrix, and absence of a plaintext accessor.

## Task 3: Accept and replay one governed Run-step input

**Files:**

- Modify: `crates/vestrace-application/src/ports.rs`
- Modify: `crates/vestrace-application/src/provider_dispatch.rs`
- Modify: `crates/vestrace-application/src/run/coordinator.rs`
- Modify: `crates/vestrace-application/src/run/ports.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/material_intent.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run/store.rs`
- Modify: `migrations/0186_provider_execution_wiring.sql`
- Modify: `crates/vestrace-infrastructure/tests/provider_dispatch_is_atomic.rs`
- Modify: `crates/vestrace-infrastructure/tests/provider_schema_contract.rs`
- Modify: `crates/vestrace-infrastructure/tests/run_acceptance_binding_race.rs`
- Add: `crates/vestrace-infrastructure/tests/provider_runtime_role_refusals.rs`

**Step 1: Write the database and application RED tests**

Extend the real PostgreSQL suites to prove:

- one caller-owned transaction commits the new step, exact pinned binding, attempt reservation, material reservation, Audit, outbox, and idempotency row or rolls all of them back;
- the attempt fixes all identities listed by `RunStepExecutionAttempt` and replay cannot transpose any one of them;
- material owner kind is `model_request_input`, owner id is the exact step id, and ordinal is zero;
- no `ExecuteStep` work item exists before material is Live and MRE is Complete;
- replay before `ContentPrepared` uses the same identities; replay after it compares decrypted bytes and accepts equal/refuses unequal input;
- startup recovery parks a pre-prepared attempt without plaintext and never allocates replacement identities;
- the restricted runtime role can call only the exact guarded functions and receives SQLSTATE `42501` for direct DML;
- a sentinel is absent from all text/JSON columns in `agent_runs`, `run_events`, `audit_log`, idempotency, outbox, receipts, attempts, MRE, and work-item tables.

Run serially:

```powershell
$env:DATABASE_URL='postgres://test:test@localhost:55432/vestrace_test'
$env:VESTRACE_RUNTIME_DATABASE_URL='postgres://vestrace:runtime-local-development-only@localhost:55432/vestrace_test'
$env:RUST_TEST_THREADS='1'
cargo test -p vestrace-infrastructure --test run_acceptance_binding_race -- --nocapture
cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic governed_run_step_input -- --nocapture
cargo test -p vestrace-infrastructure --test provider_schema_contract governed_run_step_input -- --nocapture
cargo test -p vestrace-infrastructure --test provider_runtime_role_refusals governed_run_step_input -- --nocapture
```

Expected RED: there is no transaction-bound reservation/completion API and agent input can be scheduled without durable material.

**Step 2: Add fail-closed caller-owned ports**

Add default-fail-closed methods to existing traits; do not create a second repository family:

```rust
async fn reserve_in(
    &self,
    context: &RequestContext,
    unit_of_work: &mut dyn UnitOfWork,
    intent: &MaterialKeyCreationIntent,
) -> Result<(), ApplicationError>;

async fn prepare_content_in(
    &self,
    context: &RequestContext,
    unit_of_work: &mut dyn UnitOfWork,
    intent_id: MaterialKeyCreationIntentId,
    attachment_id: PreparedMaterialAttachmentId,
    ciphertext: &[u8],
    size_class: SizeClass,
) -> Result<(), ApplicationError>;

async fn reserve_run_step_attempt_in(
    &self,
    context: &RequestContext,
    unit_of_work: &mut dyn UnitOfWork,
    attempt: &RunStepExecutionAttempt,
) -> Result<(), ApplicationError>;

async fn enqueue_run_step_after_input_ready(
    &self,
    context: &RequestContext,
    attempt: &RunStepExecutionAttempt,
    item: &WorkItem,
) -> Result<(), ApplicationError>;
```

`PgMaterialIntentRepository`, `PostgresRunStore`, and `PgProviderDispatchRepository` downcast the same `UnitOfWork` for the first transaction. The final enqueue method opens its own short transaction only after all material/MRE prerequisites are durable and delegates to the guarded 0186 function. Defaults must return `ApplicationError::Unavailable` or `Internal` and must never open a hidden transaction around a caller-owned operation.

**Step 3: Add the composite acceptance authority**

In `provider_dispatch.rs`, define the exact command and safe outcome:

```rust
pub struct PrepareGovernedRunStepInput {
    pub run_id: AgentRunId,
    pub expected_run_version: RunVersion,
    pub step_id: RunStepId,
    pub assigned_actor: RunActorRef,
    pub plan_step_reference: Option<String>,
    pub input: ConfidentialRunInput,
    pub identities: RunStepExecutionAttempt,
    pub audit: AuditEntry,
    pub idempotency: IdempotencyRecord,
    pub outbox: Vec<OutboxMessage>,
}

pub struct GovernedRunStepInputOutcome {
    pub run: RunSnapshot,
    pub attempt: RunStepExecutionAttempt,
}

#[async_trait]
pub trait GovernedRunStepInputAuthority: Send + Sync {
    async fn accept(
        &self,
        context: &RequestContext,
        command: PrepareGovernedRunStepInput,
    ) -> Result<GovernedRunStepInputOutcome, ApplicationError>;
}
```

The HTTP/application boundary allocates identities once from the request command. The repository validates the exact model-binding snapshot from the canonical Run; it must not choose a current head.

**Step 4: Extend migration 0186 and PostgreSQL implementation**

Extend the existing attempt reserve/inspect functions so the first transaction performs the exact acceptance tuple and guarded evidence. Add a guarded enqueue function whose predicates require the exact attempt, Live input material, Complete exact MRE, and no prior deterministic work item. Grant runtime only `EXECUTE` on those functions. Do not add a Run-input table, trigger-only singleton rule, or runtime table DML.

The service sequence is fixed:

1. begin transaction;
2. build canonical Run event/step using the expected version;
3. `RunStorePort::commit_in` without an agent `ExecuteStep` item;
4. reserve exact attempt and material intent in the same transaction;
5. apply governed idempotency/Audit/outbox in the same transaction;
6. commit;
7. `MaterialKeyVault::create_if_absent` with fixed key/nonce;
8. record provisional lifecycle transitions;
9. call `with_unwrapped_dek` and seal only inside the callback;
10. persist `ContentPrepared`, bind, and finalize Live;
11. complete the exact MRE with one governed-input material node;
12. enqueue the deterministic `ExecuteStep` item through the guarded function.

No vault call occurs in steps 1-6. No work item exists before step 12.

**Step 5: Implement replay without plaintext evidence**

On same Request-Id, reload the original attempt identities. Before content preparation, consume the replayed input to resume the same lifecycle. After preparation, unwrap and decrypt inside the vault callback, compare with constant-time length/equality behavior appropriate to the in-memory bytes, zeroize both values, and refuse mismatch with a closed conflict. Never write a hash or raw request body digest. Recovery without supplied plaintext returns a parked/prepared classification and does not fabricate input.

**Step 6: Mutation-test the enqueue guard**

Temporarily remove one predicate at a time—Live material, Complete MRE, exact step/attempt match—and show the corresponding test fails. Restore the function after each RED. Re-run all four serial commands from Step 1.

Expected GREEN: all atomicity/schema/RLS/replay tests pass with original identities and no sentinel persistence.

**Review checkpoint:** Sol inspects transaction ownership, vault ordering, replay identity equality, runtime grants, and mutation RED/restore/GREEN evidence.

## Task 4: Install the production dispatch policy evaluator

**Files:**

- Modify: `crates/vestrace-application/src/provider_dispatch.rs`
- Modify: `crates/vestrace-application/tests/execute_step.rs`
- Modify: `crates/vestrace-infrastructure/tests/provider_dispatch_is_atomic.rs`

**Step 1: Write policy RED tests**

Add tests for Run-step allow, capability deny, model-data deny in audit and enforce modes, qualification without a Run decision row, missing Run/step cause, missing destination/settings, and repository failure before dispatch. Assert that no admission/effect/adapter call occurs on every deny or missing-authority path.

Run:

```powershell
cargo test -p vestrace-application --test execute_step configured_provider_dispatch_policy -- --nocapture
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic policy -- --nocapture
```

Expected RED: only the trait exists; production composition has no evaluator and its signature cannot bind a Run decision to the exact cause.

**Step 2: Close the evaluator signature**

Change the trait to receive the immutable cause and destination selected by the pinned connection revision:

```rust
async fn evaluate(
    &self,
    context: &RequestContext,
    intent: &ExternalEffectIntent,
    cause: &ProviderDispatchCause,
    target: &ProviderDispatchTarget,
    request: &EffectiveModelRequest,
) -> Result<ProviderDispatchPolicyEvaluation, ApplicationError>;
```

Add `ConfiguredProviderDispatchPolicyEvaluator` backed by `SharedPolicyDecisionEngine` and `ModelDataPolicySettings`. It evaluates the exact capability request, then `evaluate_model_boundary`; a Run-step creates `ModelDataPolicyDecisionRecord` with the cause's exact run/step ids, while qualification returns `None`. It creates decision IDs but never selects a connection, revision, model, binding, credential, URL, or route.

**Step 3: Preserve atomic durable policy evidence**

The PostgreSQL dispatch repository must record the authorization decision and optional Run policy decision before admission/effect writes in the same transaction. Deny is a durable decision with no adapter authority. Missing settings/engine/identity returns fail-closed before any disclosure.

**Step 4: Re-run RED tests and existing atomic suite**

Run the Step 1 commands and:

```powershell
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic -- --nocapture
```

Expected GREEN: both policy authorities are evaluated and persisted atomically; existing 41-test dispatch suite remains green or grows monotonically.

**Review checkpoint:** Sol proves the evaluator is default-deny and route-blind, and that Run/step IDs originate only from `ProviderDispatchCause`.

## Task 5: Replace the legacy worker path with one governed attempt executor

**Files:**

- Modify: `crates/vestrace-application/src/run/model_step.rs`
- Modify: `crates/vestrace-application/src/run/handlers/execute_step.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-application/tests/execute_step.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs`
- Add: `crates/vestrace-infrastructure/tests/provider_effect_recovery.rs`
- Modify: `crates/vestrace-infrastructure/tests/provider_result_binding.rs`

**Step 1: Write executor RED tests**

Add fake-authority tests and real PostgreSQL tests which prove:

- `StepModelRequest` carries only run/step identity and never title/objective;
- executor rediscovers the one attempt and exact pinned binding/connection revision;
- reconstructed input is available only as zeroizing provider request data;
- policy deny causes zero adapter calls;
- Prepared dispatch calls the supplied pinned adapter once and unchanged;
- post-network completion uses the original authority and receipt;
- after `Dispatching`, retry/recovery adopts Unknown and never calls the adapter again;
- result preparation/finalization publishes the Run step atomically;
- `ExecuteStepHandler` does not call its legacy `complete_step` after governed publication;
- no `config.model`, mutable head, legacy provider row, or `SecretBackedProviderFactory` participates.

Run:

```powershell
cargo test -p vestrace-application --test execute_step governed_provider_step_executor -- --nocapture
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test provider_effect_recovery -- --nocapture
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test provider_result_binding -- --nocapture
```

Expected RED: `ProviderStepModelExecutor` still reads `AgentRun.objective` and bypasses the governed attempt authority.

**Step 2: Narrow the execution request and outcome**

Change `StepModelRequest` to `{ run_id, step_id }`. Introduce a closed outcome which distinguishes `Published`, `Denied`, `Conflict`, and `RecoveredUnknown`; it carries no provider text. Retain `ProviderStepModelExecutor` only for non-production tests if required, but make production registration accept only `GovernedProviderStepExecutor`.

**Step 3: Implement `GovernedProviderStepExecutor`**

Its dependencies are the existing attempt/dispatch repository, MRE reconstruction authority, policy evaluator, credential lease authority, pinned provider adapter factory, provider-result preparer/finalizer, vault, and codecs. Execute this order exactly:

1. load exact attempt by run/step;
2. classify recovery before any adapter call;
3. reconstruct Complete MRE from the attempt's material identity;
4. prepare dispatch transaction and receive immutable target/request/auth/authority;
5. choose adapter from `ProviderDispatchTarget.kind` only;
6. invoke once outside all transactions;
7. persist acknowledged/non-success completion with the same authority;
8. prepare encrypted result and call the atomic finalizer;
9. return only the closed outcome.

No retry path may invoke after durable `Dispatching`. Do not call `ExecuteStepHandler::complete_step` after the finalizer has advanced the Run.

**Step 4: Re-run focused and recovery suites**

Run the Step 1 commands and:

```powershell
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic -- --nocapture
```

Expected GREEN: exact one-call behavior, Unknown adoption, and atomic publication all pass.

**Review checkpoint:** Sol traces the call graph from work item to adapter and confirms there is exactly one production route and one result finalizer.

## Task 6: Expose exact governed provider mutations over HTTP

**Files:**

- Modify: `crates/vestrace-http/src/api/connections.rs`
- Modify: `crates/vestrace-http/src/api/models.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `crates/vestrace-http/src/route_inventory.rs`
- Modify: `crates/vestrace-http/tests/provider_routes.rs`
- Modify: `crates/vestrace-infrastructure/tests/connection_mutation_is_atomic.rs`
- Modify: `crates/vestrace-infrastructure/tests/credential_activation.rs`
- Modify: `crates/vestrace-infrastructure/tests/qualification_target_binding.rs`

**Step 1: Write route and rollback RED tests**

For connection revision creation, model revision creation, qualification request, credential activate/rotate/revoke, and Candidate abandon, test:

- every immutable command field is required and parsed to its closed type;
- Request-Id is mandatory and becomes the idempotency key;
- same key/same canonical request returns the original response;
- same key/different request conflicts;
- no current head, slot, auth branch, target, or revision is inferred;
- injected faults after lifecycle mutation, Audit, outbox, and response recording roll back every leg;
- legacy `POST /providers` remains the typed `legacy_provider_registry_retired` refusal.

Run:

```powershell
cargo test -p vestrace-http --test provider_routes governed_mutation -- --nocapture
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test connection_mutation_is_atomic -- --nocapture
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test credential_activation -- --nocapture
$env:RUST_TEST_THREADS='1'; cargo test -p vestrace-infrastructure --test qualification_target_binding -- --nocapture
```

Expected RED: current HTTP DTOs do not expose the complete application command tuples.

**Step 2: Map exact DTOs to existing commands**

Build request DTOs one-to-one from the fields already required by `CreateConnectionRevision`, `CreateModelRevision`, `QualificationJobRequest`, `CredentialActivationCommand`, `CredentialRotationCommand`, `CredentialRevocationCommand`, and `CandidateCredentialAbandonCommand`. HTTP may parse UUIDs, enums, URLs, and safe profiles; it may not query a head to fill a missing version or synthesize an identity omitted by the caller.

Each handler constructs `AuditEntry`, `IdempotencyRecord`, and `OutboxMessage` from the authenticated context and Request-Id, then calls the existing governed application service/repository. Response DTOs contain only safe identities/statuses.

**Step 3: Register only explicit mutation routes**

Register the exact paths/methods in `api/mod.rs`, `router.rs`, and `route_inventory.rs`. Keep route inventory as the single source used by OpenAPI. Do not add compatibility aliases.

**Step 4: Re-run route and atomicity tests**

Run all Step 1 commands.

Expected GREEN: every mutation has exact command parity and atomic evidence; no lifecycle can be reached through an inferred/defaulted tuple.

**Review checkpoint:** Sol compares every HTTP field against the application command definition and rejects any hidden lookup/default.

## Task 7: Compose the same governed runtime in server and worker

**Files:**

- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Modify: `crates/vestrace-cli/src/commands/schema.rs`
- Modify: `crates/vestrace-cli/tests/provider_runtime_wiring.rs`
- Modify: `crates/vestrace-cli/tests/provider_openapi_contract.rs`
- Modify: `crates/vestrace-infrastructure/src/config.rs`
- Modify: `crates/vestrace-infrastructure/src/lib.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/providers/mod.rs`
- Modify: `apps/console/src/sdk/client.ts`
- Modify: `apps/console/tests/providerClientContract.test.mjs`
- Modify: `docs/getting-started.md`
- Modify: `docker-compose.yml`

**Step 1: Preserve the known runtime RED**

Run:

```powershell
cargo test -p vestrace-cli --test provider_runtime_wiring -- --nocapture
```

Expected RED before composition: 2 passed, 1 failed with `server does not construct governed provider dispatch authority`. Record this exact baseline; it is setup evidence, not completion.

Add assertions that both server and worker construct `PgProviderDispatchRepository`, the same configured evaluator, codecs/vaults, pinned adapter, governed executor, result finalizer, qualification worker, and recovery service; assert neither uses `SecretBackedProviderFactory`, `config.model` routing, or calls `migrate`.

**Step 2: Build one shared composition helper**

Expose an infrastructure constructor returning a closed `GovernedProviderRuntime` bundle containing the governed repositories/services only. Inputs are runtime `PgStore`, writable material-vault root, read-only bootstrap-secret root, configured policy engine/settings, codecs, and hardened adapter configuration. Constructor validation fails startup on any missing authority or unsafe root relationship.

Server injects governed Run-step input acceptance plus mutation/projection services into `AppState`. Worker injects `GovernedProviderStepExecutor`, qualification handling, and attempt recovery into its registry. Both consume the same bundle; neither constructs a parallel adapter/repository graph.

**Step 3: Update OpenAPI and SDK from the route contract**

Generate/update schema so agent-step `input` is required by the actor contract, `writeOnly: true`, `minLength: 1`, and `maxLength: 32768`; no response contains it. Add all explicit mutation DTOs/routes. Update the TypeScript client and contract test together; do not hand-edit generated output without updating its generator/source.

Run:

```powershell
cargo test -p vestrace-cli --test provider_openapi_contract -- --nocapture
node --test apps/console/tests/providerClientContract.test.mjs
```

Expected GREEN: route inventory, OpenAPI, and SDK agree exactly and confidential input is request-only.

**Step 4: Document local configuration without exercising LM Studio**

Update `docs/getting-started.md` and compose configuration with the separate writable vault and read-only bootstrap roots, policy requirements, exact mutation-before-execution sequence, and fail-closed startup behavior. Mention that an OpenAI-compatible loopback endpoint may be configured explicitly; do not run LM Studio or make any provider call as qualification evidence.

**Step 5: Flip the runtime gate**

Run:

```powershell
cargo test -p vestrace-cli --test provider_runtime_wiring -- --nocapture
```

Expected GREEN: 3 passed, 0 failed, including exact server governed-dispatch construction and worker governed-executor construction.

**Review checkpoint:** Sol checks both composition roots and the shared constructor for identical authority, startup refusal, root separation, and absence of legacy routing.

## Task 8: Qualify Task 11 and hand off to Sol quality gate

**Files:**

- Add: `docs/development-evidence/v1-g0-03-provider-execution-foundation.md`
- Verify only: every Task 11 path in `scripts/p03-scope.mjs`

**Step 1: Run targeted formatting**

Run `rustfmt` only on Task 11 Rust files first, then:

```powershell
cargo fmt --all --check
```

Record the full-workspace result honestly. The existing broad dirty formatting drift may remain a non-qualifying exit 1; do not rewrite unrelated files to make it green.

**Step 2: Run focused application/HTTP/CLI suites**

```powershell
cargo test -p vestrace-application --test execute_step -- --nocapture
cargo test -p vestrace-application --test run_coordinator -- --nocapture
cargo test -p vestrace-http --test run_routes -- --nocapture
cargo test -p vestrace-http --test provider_routes -- --nocapture
cargo test -p vestrace-cli --test provider_runtime_wiring -- --nocapture
cargo test -p vestrace-cli --test provider_openapi_contract -- --nocapture
node --test apps/console/tests/providerClientContract.test.mjs
```

Expected GREEN: all Task 11 focused suites pass; runtime wiring is 3/3.

**Step 3: Run serial SQLx qualification**

```powershell
$env:DATABASE_URL='postgres://test:test@localhost:55432/vestrace_test'
$env:VESTRACE_RUNTIME_DATABASE_URL='postgres://vestrace:runtime-local-development-only@localhost:55432/vestrace_test'
$env:RUST_TEST_THREADS='1'
cargo test -p vestrace-infrastructure --test provider_schema_contract -- --nocapture
cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic -- --nocapture
cargo test -p vestrace-infrastructure --test provider_effect_recovery -- --nocapture
cargo test -p vestrace-infrastructure --test provider_result_binding -- --nocapture
cargo test -p vestrace-infrastructure --test provider_runtime_role_refusals -- --nocapture
cargo test -p vestrace-infrastructure --test run_acceptance_binding_race -- --nocapture
cargo test -p vestrace-infrastructure --test connection_mutation_is_atomic -- --nocapture
cargo test -p vestrace-infrastructure --test credential_activation -- --nocapture
cargo test -p vestrace-infrastructure --test qualification_target_binding -- --nocapture
cargo test -p vestrace-infrastructure --test p03_upgrade_provisioning -- --nocapture
```

Expected GREEN: all commands exit 0; the previously green dispatch/schema/result/upgrade suites remain green with monotonically increased test counts.

**Step 4: Run workspace and governance gates**

```powershell
cargo test --workspace -- --nocapture
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-03-preflight.json --scope p03-scope.mjs
git status --short
git diff --check
```

Classify each command as PASS, FAIL, BLOCKED, or UNRUN with exact exit code/test count. A stalled or partial workspace run is non-qualifying. Do not repair unrelated dirty work.

**Step 5: Update evidence without claiming more than observed**

Append Task 11 commands, exits, counts, mutation RED/restore/GREEN, sentinel-negative queries, runtime 3/3 result, and any honest full-workspace limitation to `docs/development-evidence/v1-g0-03-provider-execution-foundation.md`. Do not alter the preflight.

**Step 6: Independent Sol quality gate**

Sol reviews the final diff and evidence against the approved design and this plan, specifically:

- exact scope and protected hashes;
- title/input separation and zeroization;
- one attempt/material/effect/MRE identity chain;
- no vault/provider call inside a transaction;
- policy-before-disclosure and pinned routing;
- no adapter retry after `Dispatching`;
- result publication atomicity and no double Run completion;
- HTTP exact-tuple mutations and rollback;
- server/worker parity and fail-closed startup;
- OpenAPI/SDK parity;
- honest gate classification.

Task 11 closes only on Sol `APPROVE` with no unresolved blocker. No commit, push, deploy, LM Studio call, or external network call follows.

## Execution order and stop conditions

Execute Tasks 1 through 8 strictly in order. Stop and return to the operator if:

- any required edit falls outside the 136-path amended scope;
- a protected hash other than the one explicitly reclassified Run route changes;
- a required caller-owned transaction seam cannot be implemented without a new authority/table;
- input equality replay would require durable plaintext-derived evidence;
- worker composition requires config-selected routing or a legacy provider factory;
- any previously proved P03 invariant regresses and cannot be restored within the admitted scope.

The selected execution mode is the existing `$cdx` workflow: one persistent Terra builder performs the tasks and produces evidence; Sol reviews after every task and owns the final quality gate.
