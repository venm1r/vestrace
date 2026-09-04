# Vestrace P03 Task 11 Governed Run Input Design

## Status and authority

This design records the operator-approved Task 11 amendment from 2026-09-01. It refines the existing P03 corrective acceptance without weakening it. The frozen P03 plan, P01/P02 authority, migrations 0172 through 0185, and the preflight remain byte-identical. No commit, push, deployment, non-loopback provider call, P04, P05, or P06 implementation is authorized.

The amendment permits the protected Run HTTP boundary and the minimum canonical Run files needed to separate a safe public title from confidential model input. It does not permit a second request store, a second effect state machine, or a config-selected provider path.

## Problem

The current production path treats `CreateRunRequest.title` as `CreateRun.objective`. That ordinary `String` is cloned into `AgentRun`, `RunCreated`, `agent_runs.title`, `agent_runs.objective`, response DTOs, tests, and finally `StepModelRequest`. This contradicts Task 11: model-visible Run input must be bounded and zeroizing, encrypted through the existing content-material lifecycle, referenced by the immutable Run-step attempt, and absent as plaintext from PostgreSQL, response DTOs, Debug, logs, errors, receipts, and Audit.

The attempt schema already requires a real `(workspace_id, run_id, step_id)` and fixes the input material intent, material, key, nonce, prepared attachment, effect, and MRE identities before a vault call. Therefore input cannot lawfully be encrypted at bare Run creation, before an agent step exists. Adding a Run-level input table would create the forbidden parallel input-material authority.

## Chosen API contract

`POST /v1/runs` continues to accept `title`. The title is explicitly public display metadata, is bounded by the existing Run title/objective limit, and remains the value stored in the legacy `AgentRun.objective` field and `RunCreated` payload for compatibility. It is never used as model input.

`POST /v1/runs/{id}/steps` changes only for an `assigned_actor` of `agent`: each agent step must carry exactly one `input` string. Principal and system steps must omit `input`. The input is deserialized directly into a non-`Clone`, non-serializable, redacted-`Debug` application type backed by `Zeroizing<String>`. It must contain between 1 and 32,768 UTF-8 bytes and at least one non-whitespace character. The exact accepted bytes are preserved; validation does not create a second ordinary `String`.

The route response remains the existing safe Run/step projection and never echoes input. OpenAPI describes `input` as write-only and bounded. The console SDK accepts it only in the agent-step request and exposes no response field for it. AG-UI model execution is refused until P06 supplies the same governed step-input command; its current message-to-Run-objective bridge may not persist message plaintext.

## Application boundaries

The application introduces these closed types and ports in the existing Task 11 Run/provider files:

- `ConfidentialRunInput`: owns one `Zeroizing<String>`; no `Clone`, `Serialize`, or plaintext `Debug`; exposes bytes only to a callback.
- `RunStepExecutionIdentities`: the already-required attempt, material, effect, and MRE identities allocated once for a specific Run step.
- `PrepareGovernedRunStepInput`: consumes the request id, Run id, step id, pinned binding snapshot id, and `ConfidentialRunInput`.
- `GovernedRunStepInputRepository`: reserves or rediscovers the exact attempt and material lifecycle, prepares the ciphertext, and returns only the durable attempt projection.

`NewRunStepDto` carries confidential input only while the agent-step acceptance service is running and therefore is no longer `Clone` or plaintext `Debug`. Canonical `RunStep`, `RunEvent`, `RunReference`, work items, and response DTOs contain only opaque identities.

`StepModelRequest` no longer contains `objective: String`. `GovernedProviderStepExecutor` receives the durable Run/step identity, rediscovers the attempt, reconstructs the complete MRE, and obtains the model-visible input only through the existing `ContentMaterialCodec` and vault callback. The plaintext remains zeroizing through policy evaluation and the single adapter call.

## Durable sequence and replay

Agent-step acceptance is a recoverable sequence whose durable transitions are all idempotent on the original identities:

1. Under `InstallationMutationPermit::Shared`, commit the canonical agent step, pinned `run_model_binding_snapshots` relation, immutable `run_step_execution_attempts` reservation, Request-Id record, governed Audit/outbox, and the reserved material intent in one caller-owned PostgreSQL transaction. No vault call occurs while this transaction is open.
2. Call `MaterialKeyVault::create_if_absent` with the attempt's fixed material-key id and nonce. Record the provisional-created and provisional-receipt transitions through the existing guarded lifecycle.
3. Unwrap only that key, seal the input with `ContentMaterialCodec` using the exact workspace/material/key AAD, and commit `ContentPrepared` with the fixed attachment id. No plaintext or DEK enters PostgreSQL.
4. Bind and finalize the content material through the existing guarded functions, append the complete MRE containing the single governed-input material node, then enqueue the one deterministic `ExecuteStep` work item.
5. Consume and zeroize the input before returning. A response contains only safe Run/step state.

The request is not considered accepted until the first transaction commits. Before ciphertext is prepared, a crash leaves the same reserved attempt and material identities; a replay with the same Request-Id may supply the input again and continue them. Once ciphertext is prepared, replay compares the supplied bytes inside the vault callback against the already encrypted content and refuses unequal input without persisting a plaintext hash or raw request digest. After the safe response is recorded, same Request-Id and equal input returns it; unequal input conflicts.

Startup recovery never invents plaintext. It may resume states whose ciphertext is already durable. A pre-prepared reservation without a replayed request is parked and reported; it is not replaced by a second material identity. Task 12 may add more crash points but cannot change this rule.

## PostgreSQL authority

Migration 0186 remains the only forward migration changed by this amendment. It extends the existing attempt reservation/inspection functions rather than creating a Run-input table. The guarded reservation must prove:

- the exact Run step exists in the workspace;
- the Run has the exact pinned `ModelBindingSnapshot`;
- the attempt identities match on replay;
- the material intent owner is `model_request_input`, its owner id is the exact step id, and ordinal is zero;
- a runnable work item cannot exist until the exact input material is Live and the exact MRE check is Complete.

Runtime receives only the exact function execution and existing read privileges needed by repositories. Direct runtime DML on Task 11 guarded tables remains `42501`. Historical migrations are not edited.

## Production policy evaluator

The production `ProviderDispatchPolicyEvaluator` is an application adapter over two existing authorities, not a default-allow implementation:

- the configured capability policy engine evaluates the exact external-effect authorization request;
- the model-data boundary evaluates the reconstructed request's destination and the deployment sensitivity/mode settings.

For a Run-step cause, the evaluator receives the exact Run and step ids required to create `ModelDataPolicyDecisionRecord`. Qualification causes have no Run/step decision row. Missing cause identity, destination, policy settings, or policy engine fails closed. The evaluator returns a fresh decision identity but never chooses a Connection, model, binding, credential, or base URL.

## Server and worker composition

Server and worker construct the same governed repositories from the runtime-scoped `PgStore`, the writable host material-vault root, the read-only bootstrap-secret root, the content/credential codecs, the configured policy engine, and the hardened OpenAI-compatible adapter. Neither composition root calls `migrate`, uses `config.model` for routing, or constructs `SecretBackedProviderFactory`.

The server injects governed step-input acceptance and the already-defined HTTP mutation/projection authorities. The worker injects `GovernedProviderStepExecutor` into `ExecuteStepHandler`, the durable qualification worker, and attempt recovery. Missing vault, policy, adapter, or repository authority refuses startup rather than registering a legacy executor.

## HTTP mutation contract

Connection, revision, qualification, credential, and model mutation requests must name every immutable tuple member required by their application command. HTTP may parse typed UUIDs, closed enums, bounded URLs, and non-secret profile fields; it may not select current heads, synthesize slots, default auth branches, or manufacture qualification targets.

Every mutation uses Request-Id idempotency and the existing governed mutation repository so its first lifecycle transition, Audit, outbox, and response identity commit atomically. Same key and same canonical request returns the original response; same key and a different canonical request conflicts. Candidate abandon resumes its existing lifecycle. Legacy `POST /providers` remains the typed `legacy_provider_registry_retired` refusal.

## Failure behavior

- Invalid or oversized input: HTTP 400 before any durable mutation or vault call.
- Missing grant: exact authorization refusal before input preparation.
- Missing governed authority or policy setting: fail closed; no legacy fallback.
- Vault create/seal/unwrap failure: no work item or adapter call; replay keeps the original attempt identities.
- Material/MRE mismatch: typed conflict; no second material, effect, or MRE.
- Adapter failure before acknowledged receipt: use the existing non-success completion authority.
- Lost dispatch after `Dispatching`: adopt Unknown and never call the adapter again.
- Result finalization fault: the already-proved atomic result/Run transaction rolls back, leaving `ResultPrepared` recoverable.

Errors, tracing fields, Audit, receipts, and outbox payloads name only opaque ids, states, closed error codes, and safe size/media classes.

## Verification

The implementation follows observed RED, restore, GREEN for each independent slice. Required focused evidence includes:

- structural tests proving confidential input has no `Clone`, `Serialize`, or plaintext `Debug` path;
- HTTP tests proving agent input is required, system/principal input is refused, responses never echo the sentinel, and AG-UI cannot persist message plaintext;
- real PostgreSQL tests proving the sentinel is absent from `agent_runs`, `run_events`, Audit, idempotency, outbox, receipts, and every text/JSON column inspected by the restricted workspace session;
- vault/codec tests proving fixed identities, one effective key, bounded frame, crash replay, mismatch refusal, and zeroization on every exit;
- worker tests proving exact pinned routing, one adapter call, no retry after `Dispatching`, and no `complete_step` after provider finalization;
- mutation rollback tests for every new HTTP command;
- exact inventory/OpenAPI/SDK parity;
- runtime composition tests for both server and worker;
- serial SQLx suites, scope tests, protected dirty-baseline verification, and independent Sol review.

LM Studio and external network calls are not part of this evidence.

## Amendment 2026-09-02: the effect intent and the canonical request revisions

This amendment closes the two inputs the original design left unnamed. Task 3 implementation stopped at them rather than fabricating them, which was correct: both are security-relevant facts that no existing relation determines.

### What was found

`model_request_evidence_roots.external_effect_id` carries a mandatory composite foreign key to `external_effect_intents(id, workspace_id)` (migration 0179), and `vestrace_create_model_request_evidence` independently refuses creation with SQLSTATE `23514` unless that exact effect row already exists (migration 0183). The accepted first transaction reserves only the UUID held in `RunStepExecutionAttempt.external_effect_id`; it persists no `ExternalEffectIntent`. `ExternalEffectIntent::new` allocates `ExternalEffectId::new()` unconditionally and there is no constructor accepting an already-fixed id. The exact MRE therefore could not be created at step 4 of the durable sequence.

Separately, a `chat_completions` MRE requires exactly one `request_shape_revision`, one `sampling_revision`, and one `limits_revision` node, and `model_binding_snapshots` (migration 0178) pins none of them. That absence was first read as a missing determination. It is not: those tables are workspace-scoped, versioned and idempotently created because the revisions are evidence authored per request, not configuration selected per binding. Ruling 2 below records what follows.

This is not a scope blocker. `crates/vestrace-domain/src/external_effects.rs` and `migrations/0186_provider_execution_wiring.sql` are already change-authorized; only the design ruling was missing.

### Ruling 1: the intent allocates, the attempt adopts

The acceptance service constructs the `ExternalEffectIntent` for the agent step and persists it through the existing effect authority inside the same first caller-owned transaction, before the attempt reservation. The attempt's `external_effect_id` is that intent's own allocated id.

`ExternalEffectIntent::new` is unchanged and no fixed-id constructor is admitted. The alternative — admitting `with_reserved_id` so the HTTP boundary keeps allocating every identity — was rejected: it widens a domain constructor whose single allocation site is the reason an effect id cannot be forged by a caller.

This narrows one sentence of `Application boundaries`. The boundary allocates every identity in `RunStepExecutionIdentities` **except** the effect id, which it adopts from the intent it just constructed. Identities are still allocated exactly once, from the request command, and replay still rediscovers them rather than allocating again.

The intent's immutable fields are derived, never stated by the caller:

- `execution_ref` is `run://<run_id>`, which `validate_execution_ref` already requires to name a Run or Execution;
- `adapter` and `target` come from the pinned connection revision reached through the binding snapshot, never from a mutable head;
- `operation` is the MRE request kind;
- `risk`, `reversibility`, `idempotency_profile`, `delivery_semantics`, and `required_capability` are fixed for the governed Run-step model operation and are not per-request choices;
- `normalized_arguments_digest` and `precondition_digest` are computed over the pinned canonical revision identities and versions, the request kind, and the binding snapshot id **only**. Neither is derived from the confidential input, in any form, including its length. A plaintext-derived digest is refused by the global constraint against persisting a raw request digest or plaintext-derived hash.

### Ruling 2: the canonical revisions are created as evidence, not selected

The three revisions are not chosen from a registry, and no registry to choose from should exist. They describe the shape of the request the system is about to send, so they are allocated and declared by the same command that creates the evidence — exactly as `CreateModelRequestEvidence::models_list_probe` already does for a qualification probe, where it allocates `request_shape_id` and `limits_id` itself at version 1 and states fixed values.

A Run-step constructor follows that pattern. It allocates the request-shape, sampling and limits identities, declares the fixed `chat_completions` shape for a governed agent step, and adds the single `governed_input_material` node. Migration 0183's creators are idempotent — `ON CONFLICT (id) DO NOTHING` followed by an exact-value re-check that raises `40001` — so a replay carrying the same identities and the same values converges, while a replay whose values differ conflicts instead of silently rewriting evidence.

`model_binding_snapshots` therefore gains no columns and migration 0186 gains no `ALTER` for them. The binding keeps pinning what it already pins: which connection revision, model revision and qualification revisions the Run is bound to. Which request shape was sent is a property of the request, not of the binding, and belongs in the evidence that reconstructs it.

This supersedes an earlier ruling that would have pinned the three revisions into the binding snapshot. That ruling rested on the escalation's phrasing that "nothing determines them", which is true only of selection. Implementing its domain half showed the cost it would have carried: six columns and foreign keys added to an 0178 table, no answer to where their values come from at snapshot creation, and every snapshot created before the amendment left unable to admit an agent step. None of that is necessary.

The earlier `Verification` bullets tied to the retired ruling are replaced by:

- a replay carrying the same revision identities and values converges on the same evidence;
- a replay carrying the same identities with different values is refused with `40001` rather than rewriting the evidence;
- the constructor's declared shape is fixed for a governed agent step and is not read from deployment configuration at request time.

### Ruling 3: the destination reaches policy through the pinned target

`Production policy evaluator` above says the model-data boundary evaluates "the reconstructed request's destination". That is corrected: the destination is classified once, from the pinned connection revision's kind and base URL, inside the routing transaction that already proved the revision, and travels to the evaluator on `ProviderDispatchTarget`. The reconstructed request carries governed content and is not consulted for routing.

The evaluator therefore parses no URL and resolves no host, which is what makes "never chooses a Connection, model, binding, credential, or base URL" true rather than merely intended. The policy-time classification is the static half of the adapter's dispatch-time peer check and shares its loopback rule; it is never more permissive, because every input on which the two differ is one the dispatch-time check refuses outright.

Under `Enforce`, a data-boundary refusal denies the authorization itself. An allowed authorization beside a denied enforced record is the exact pair the dispatch repository rejects, so returning it would surface a policy refusal as a binding error rather than a durable denial.

### Verification this amendment adds

- The intent row exists before the attempt references it, and both commit or roll back together.
- Replay rediscovers the same effect id and creates no second intent.
- A digest column is proved independent of the input: two accepted steps whose confidential inputs differ, with every pinned revision equal, produce identical `normalized_arguments_digest` values.

## Scope amendment

The implementation plan must enumerate every newly admitted path before product edits. At minimum the amendment includes the protected Run HTTP boundary, Run command/coordinator tests needed for the confidential-input transport, the AG-UI refusal path, this design, and its implementation plan. No preflight is recaptured. The operator amendment reclassifies only the exact newly admitted Run paths; every other protected-authority hash remains unchanged.
