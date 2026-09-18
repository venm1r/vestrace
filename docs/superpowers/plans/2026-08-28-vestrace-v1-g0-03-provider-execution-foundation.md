# Vestrace v1.0 G0-03 Provider Execution Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Revision:** revised after independent adversarial review returned `VERDICT: REVISE` (two blockers and three major findings). Implementation remains unauthorized until repeat review returns `VERDICT: APPROVE` and the operator accepts the reviewed plan.

**Goal:** Build P03's provider-execution foundation: immutable Connection and Model revisions, strict credential-versus-no-auth binding, durable q1 qualification, the sole `ModelBindingSnapshot` routing authority, reconstructable `ModelRequestEvidence`, hardened OpenAI-compatible transport, and provider dispatch through the existing external-effect admission/owner/deadline/receipt/recovery lifecycle.

**Architecture:** P03 replaces the current name-and-environment-based provider selection at the execution boundary with database-pinned revision tuples. A stable Connection owns immutable configuration revisions and one permanent P02 `ConnectionExecutionGuard`; a credential branch additionally owns its exact P02 activation guard/slot/revision lineage, while a no-auth branch owns one immutable `NoAuthBindingRevision` and no credential-shaped field at all. Qualification probes and production model calls use the same hardened adapter and the same durable external-effect authority; neither q1 nor `ModelRequestEvidence` becomes routing or outcome authority. New guarded tables and functions remain owned by `vestrace_guarded_owner`, while the runtime role receives only exact `EXECUTE` and read privileges.

**Tech Stack:** Rust 1.85 / edition 2024, Axum 0.8, SQLx 0.8, PostgreSQL 17 with forced RLS and guarded `SECURITY DEFINER` operations, Reqwest 0.12 with Rustls/WebPKI, `secrecy` zeroizing containers, the pinned `openai-chat-completions-v1/q1` manifest, and the existing external-effect and fault-scenario authorities.

**Spec:** `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md` sections 6.1, 6.4, 6.4.1, 6.4.2, 11.1-11.5, 12, 13, 14.1-14.2, the P03-owned G0 bullets in section 15, and the settled provider decisions in section 17; frozen SHA-256 `B31B5BE62504E1A65F411CD31B446CD41B3D032B7282F1AA42907706EF9C1473`.

**External corpus impact:** `compatibility seam`. P03 implements only the frozen spec's narrow registered seam: canonical Run/effect truth remains distinct from provider/request-evidence projections, and the effective model-visible request is reconstructable from exact canonical revisions while retained. The donor entries are `Vestrace-prospective-technologies-research-2026-08-12.md` (`ca4c9e0f...03ff`) and `Vestrace-reference-systems-research-2026-08-11.md` (`db2b4046...1c9f`) from manifest `762c25f3...594f`. `RFC-Model-Request-Reconstruction-and-Context-Surface-2026-08-19.md` remains `defer_post_v1`: P03 must not introduce its generalized envelope, actor runtime, context-surface, raw request digest, or a second durable truth.

## Package boundary decisions

1. **P03 executes q1 ordinal 90 but does not create production EmbeddingJobs.** The exact q1 profile includes the required Embeddings probe and its `ModelRequestEvidence`, so omitting it would invent a weaker qualification profile. P04 owns production embedding jobs, spaces, corpus generations, transitions, recipes, carries, barriers, and retrieval fences.
2. **P03 completes ordinary first credential activation and non-embedding rotation.** Credential-backed qualification cannot be proved without activation, exact current-slot CAS, leases, and rotation serialization. If any live embedding corpus or transition dependency exists, rotation refuses with a typed `embedding_transition_required`; P04 later supplies the transition evidence and activation participant. P03 does not fake an empty transition.
3. **P03 does not implement restore activation.** `RestoreMaintenanceEpoch`, `TargetActivationPlan`, backup/WAL, and cross-generation cutover remain P05. The ordinary activation interfaces must accept no restore sentinel or maintenance bypass.
4. **The legacy provider/model catalog is not a second routing authority.** Existing `providers` and `models` rows remain readable compatibility projections. A row without the new guarded revision/qualification tuple is non-executable. New commands write the stable identity plus guarded head/revision records atomically; worker dispatch never falls back to `models.model_name`, `config.model`, or a secret name.
5. **P03 extends the deployment ownership bridge narrowly and supersedes one unsafe P02 entrypoint forward-only.** It adds separately allowlisted `vestrace_assign_p03_table_owner` and `vestrace_assign_p03_function_owner` helpers to the real PostgreSQL bootstrap script. Compose reruns that idempotent bootstrap as its administrative identity before every runtime-role migration, so an existing P02 volume receives new helpers rather than relying on initdb. A no-argument, hardcoded, self-revoking helper may only revoke runtime `EXECUTE` from `public.vestrace_prepare_credential_material_erasure(UUID)` after the P03 replacements exist; it accepts no object or role parameter and grants no ownership or `SET ROLE`. P03 never edits migrations 0166-0175, broadens a P02 allowlist, or re-owns a pre-existing table. PostgreSQL tests execute the same bootstrap SQL/order as Compose before applying migrations.

## What already exists and must not be duplicated

| Concern | Existing authority to extend | Live location |
| --- | --- | --- |
| Permanent Connection and credential lock order | `ConnectionExecutionGuard`, `CredentialActivationGuard`, `vestrace_acquire_credential_lock_chain` | `migrations/0171_connection_execution_guards.sql`, `0172_credential_slots_and_revisions.sql` |
| Credential key-intent, Candidate and erasure lifecycle | `CredentialIntentCommands`, `PgCredentialIntentRepository`, guarded SQL in 0173/0174 | `crates/vestrace-application/src/credential`, `crates/vestrace-infrastructure/src/postgres/credential_intent.rs` |
| Host material-key custody and zeroizing DEKs | `HostMaterialKeyVault`, `ZeroizingDek`, erasure fence | `crates/vestrace-infrastructure/src/crypto/material_vault.rs`, `crates/vestrace-domain/src/material` |
| Transaction-owned governed writes and Audit | `UnitOfWork`, `GovernedMutationRepository`, transaction-bound Audit/idempotency/outbox ports | `crates/vestrace-application/src/ports.rs`, `crates/vestrace-application/src/governed_mutation.rs`, `crates/vestrace-infrastructure/src/postgres/pool.rs` |
| External-effect lifecycle | `ExternalEffectIntent`, `EffectAuthorization`, `ExternalEffectRepository`, `ExternalEffectRecoveryService` | `crates/vestrace-domain/src/external_effects.rs`, `crates/vestrace-application/src/{external_effects,effect_repository,effect_recovery}.rs` |
| Effect persistence and lost-dispatch recovery | intents, authorizations, lifecycle transitions, owner/deadline, receipts, reconciliations | `migrations/0127-0159`, `crates/vestrace-infrastructure/src/postgres/external_effect_repository.rs` |
| Data-policy decision before provider disclosure | `evaluate_model_boundary`, persisted model-data-policy decisions | `crates/vestrace-application/src/model_data_policy.rs`, `crates/vestrace-infrastructure/src/postgres/model_data_policy_decision_repository.rs` |
| Route inventory and default deny | `route_inventory`, `mount`, `inventory_lookup` | `crates/vestrace-http/src/route_inventory.rs` |
| Current OpenAI-shaped HTTP client | redirect/proxy refusal, bounded error mapping, one chat-completions method | `crates/vestrace-infrastructure/src/providers/openai_compatible.rs` |
| Frozen q1 contract | exact manifest and fixture | `schemas/openai-compatible/openai-chat-completions-v1-q1.json`, `tests/fixtures/openai-q1/marker.png` |

P03 may add adapters and transaction-bound methods to these authorities. It must not create a second effect state machine, a second snapshot type for q1, a provider-owned retry loop, a credential-shaped no-auth sentinel, or a raw request store.

## Live defects this package closes

- `ProviderStepModelExecutor` resolves `models.model_name` immediately before dispatch and calls `TextGenerationProvider::generate` directly. It therefore has no immutable Connection/Model/qualification/auth tuple, no Run-owned binding snapshot, no MRE, and no shared effect owner/deadline/recovery evidence.
- `SecretBackedProviderFactory` chooses one base URL and secret name from process configuration. It cannot express two simultaneous credential-isolated Connections or the `None | Bearer | ApiKey | XApiKey` closed auth contract.
- `Connection` and `ModelRecord` are mutable flat catalog shapes. PostgreSQL has no immutable ConnectionRevision, ModelRevision, no-auth binding, qualification target XOR, effective qualification CAS, or ModelBindingSnapshot.
- The current adapter owns only `/chat/completions`, emits only optional Bearer auth, and does not implement `/models`, q1, canonical API-key headers, actual peer evidence, or the complete remote destination policy.
- Existing external-effect persistence methods own separate transactions. Provider dispatch needs one caller-owned transaction that takes the installation permit and Connection guard, revalidates binding/MRE/policy/admission/credential lease, then appends authorization plus `Dispatching` atomically.

## Global Constraints

- Scope is P03 only. Do not implement P04 embedding jobs/transitions, P05 restore/backup, P06 console screens/real-agent flows, AG-UI, A2A, or release qualification.
- Do not commit, stage, push, deploy, contact LM Studio, or contact a third-party provider. Provider tests use loopback servers with deterministic fixtures.
- Preserve every pre-P03 dirty path byte-for-byte unless the accepted P03 scope names it. In particular, `crates/vestrace-http/src/api/memory.rs`, `retrieval.rs`, and `runs.rs` remain protected unless an unavoidable exact P03 change is separately authorized.
- The gate-program index, P01/P02 plans and preflights, accepted P02 evidence, frozen spec, protocol lock, q1 manifest, and q1 fixture are protected authority. Do not edit or re-pin them in P03.
- A P03 scope omission is not permission to edit the path. Stop and request an auditable scope amendment; never re-capture the preflight.
- RED must be observed before GREEN for every task. An existing failing test is a product finding; do not weaken or delete it.
- Every database-owned invariant uses real PostgreSQL through `#[sqlx::test(migrations = "../../migrations")]`. The sole exception is a deployment-order regression declared with `migrations = false`: it must execute the real bootstrap provisioner first and then the real migrator, because automatic SQLx migrations run before the test body. Runtime-role refusal tests derive the per-test database from `pool.connect_options()`, override only username/password, and assert the exact SQLSTATE.
- Every new workspace-scoped table gets `ENABLE ROW LEVEL SECURITY` plus `FORCE ROW LEVEL SECURITY`, is handed to `vestrace_guarded_owner`, and denies runtime direct DML with SQLSTATE `42501`.
- Every `SECURITY DEFINER` function has a fixed `SET search_path = public, pg_temp`, no dynamic SQL, an exact signature allowlist in the deployment helper, `PUBLIC` execute revoked after handoff, and only intentional runtime `EXECUTE` grants.
- Provider mutations use P02's caller-owned governed mutation/Audit authority. Provider dispatch uses the existing external-effect authority with transaction-bound persistence; it does not mislabel an effect transition as an HTTP Audit row.
- Every common P03 mutation and dispatch takes P02 `InstallationMutationPermit::Shared` before `ConnectionExecutionGuard`; no P03 interface accepts or wires `Exclusive`.
- No-auth types contain no `CredentialSlotId`, `CredentialRevisionId`, `CredentialActivationGuardId`, lease, material, secret, or sentinel. Credential-backed operations must name the exact lineage.
- `ModelRequestEvidence` stores no raw request, prompt, output, credential, auth header, exact content byte count, unkeyed content digest, or stable content fingerprint.
- Missing usage is `unknown`, never zero. Raw provider bodies and body-derived digests/sizes never enter errors, logs, Audit, receipts, or qualification evidence.
- No retry occurs automatically after `Dispatching`. `InconclusiveUnknown` is terminal for a QualificationJob and cannot publish qualification.
- Replace per-task commit steps with scoped-diff/evidence checkpoints.
- New migrations start at `0176` and are forward-only.

## Acceptance criteria

1. A stable Connection is executable only through an immutable current `ConnectionRevision` protected by its permanent `ConnectionExecutionGuard`.
2. Auth binding is a database-enforced XOR: exact credential lineage or exact `NoAuthBindingRevision`, never both or neither; the no-auth branch has no credential-shaped field or runtime action.
3. A stable Model is executable only through an immutable `ModelRevision` bound to one exact ConnectionRevision and current compatible qualifications.
4. Every QualificationJob owns one immutable `QualificationTargetBinding`; all its probes, MREs, effects, and resulting qualification revisions cite that same binding.
5. The exact q1 manifest drives ordinals `00,10,15,20,30,35,40,50,60,70,80,90`; each network probe creates exactly one external effect and never automatically retries after dispatch.
6. `InconclusiveUnknown` is terminal, publishes no qualification, and suppresses remaining probes. Required definite failures and optional definite unsupported results follow the pinned manifest.
7. The sole `ModelBindingSnapshot` pins the exact current ConnectionRevision, ConnectionQualificationRevision, ModelRevision, ModelQualificationRevision and auth branch before a Run becomes executable; dispatch never resolves by name or process config.
8. Every provider effect owns immutable MRE whose current reconstruction is `Complete` before `Dispatching`; `Incomplete` refuses inspection/dispatch/qualification, and exact authorized erasure alone yields `Expired`.
9. A loopback observer proves the reconstructed semantic request equals the actual request emitted by the production adapter for chat, ModelsList, and q1 Embeddings, without persisting either raw request.
10. Remote transport requires HTTPS/WebPKI, rejects userinfo/query/fragment, ambient proxy, redirect, forbidden DNS answers and forbidden actual peers, and emits only the canonical closed auth headers.
11. Stable-Connection admission serializes rolling 60-second limits, concurrency leases, durable 429 throttles, and atomically commits admission, optional credential lease, effect authorization and `Dispatching`.
12. A credential lease is exact-effect/exact-authority/exact-auth-mode, one-use, zeroizing, unavailable to activation, and cannot be reused after rotation/revoke/erasure preparation.
13. First credential activation atomically publishes qualification, binds/current-points the slot, appends `Active`, closes the association, and writes Audit; any failed predicate rolls the whole transaction back.
14. Ordinary rotation atomically switches all non-embedding executable dependencies or refuses. Any live embedding dependency returns `embedding_transition_required` for P04 rather than partially activating.
15. Provider `Unknown`, worker loss, restart, and read-back reuse the shared external-effect recovery authority and preserve the original effect/MRE/snapshot; no second provider attempt is created automatically.
16. Every new/changed HTTP route is inventory-mounted, denied without its exact capability, and every governed mutation rolls back with Audit/idempotency/outbox on either-side failure.
17. Runtime direct DML on every P03 guarded table returns `42501`; guarded functions perform their intended writes successfully as the runtime role.
18. The complete pre-P03 dirty baseline is byte-identical outside accepted scope, the protocol lock remains green, and evidence states every P04/P05/P06 dependency as a non-claim.
19. A retained chat result becomes usable only through the P02 `MaterialKeyCreationIntent` `ResultPrepared -> Bound -> Live` path. No Run success, ordinary result reference, or replay-visible content exists before the exact prepared attachment is bound and atomically promoted; recovery after `ResultPrepared` never invokes the provider again.
20. The P02 Candidate-only `vestrace_prepare_credential_material_erasure(UUID)` is exact-`42501` unavailable to runtime after P03 migration. Active material is never destructible; only the exact Retired/Revoked guarded entrypoint or the distinct Candidate-abandon transaction may prepare credential erasure.
21. Every P03 HTTP route and changed DTO is represented identically in Axum routing, the CLI OpenAPI document, and the contract-checked console SDK. This updates transport contracts only; P03 adds no P06 screen or browser workflow.

---

### Task 1: Capture the P03 dirty baseline and freeze scope

**Files:**
- Create first: `docs/development-evidence/v1-g0-03-preflight.json`
- Create: `scripts/p03-scope.mjs`
- Create: `tests/p03_scope.test.mjs`
- Modify: `scripts/verify-dirty-baseline.mjs`

**Interfaces:**
- `p03-scope.mjs` exports exact `changeScopePaths` and `protectedAuthorityPaths` arrays with no wildcard. Protected authority includes the frozen spec, gate-program index, P01/P02 plans and preflights, accepted P02 evidence, protocol lock, q1 manifest and fixture, and the three protected dirty HTTP files.
- The shared baseline verifier adds only the P03 scope label, allowlist entry, usage text, and module dispatch needed to load `p03-scope.mjs`; P01 and P02 behavior remains byte-for-byte equivalent at the interface.
- The preflight records HEAD, raw NUL-delimited porcelain bytes and digest, the two arrays, and every dirty path with byte length and SHA-256/absent marker, using the P02 artifact shape.

- [ ] **Step 1: Capture before the first P03 implementation write**

```powershell
git -c safe.directory=E:/Soft/vestrace rev-parse HEAD
git -c safe.directory=E:/Soft/vestrace status --porcelain=v1 -z --untracked-files=all
```

Persist the preflight first. The P03 plan itself is already protected authority and is captured, not rewritten.

- [ ] **Step 2: RED and legacy-behavior control**

Before editing the shared verifier, temporary-repository positive and protected-mutation-negative fixture matrices must pass for default P01 dispatch and explicit P02 dispatch. The complete P03 scope test must then fail specifically while the verifier lacks P03 dispatch, and its P03 fixture must prove a protected-path mutation is rejected rather than merely observing the unsupported-scope usage error.

```powershell
node --test --test-name-pattern="legacy (P01|P02) verifier" tests/p03_scope.test.mjs
node --test tests/p02_scope.test.mjs
node --test tests/p03_scope.test.mjs
```

- [ ] **Step 3: GREEN**

Extend the shared verifier's explicit scope dispatch to accept `p03-scope.mjs`, without changing its verification rules.

```powershell
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-03-preflight.json --scope p03-scope.mjs
node scripts/protocol-lock.mjs --check .
```

- [ ] **Step 4: Scoped diff review**

Confirm every later task path is listed, the arrays are disjoint, no earlier authority digest changed, and no functional code predates the preflight.

---

### Task 2: Declare the provider revision, binding, qualification, evidence, and admission contracts

**Files:**
- Create: `crates/vestrace-domain/src/connection/revision.rs`
- Create: `crates/vestrace-domain/src/models/binding.rs`
- Create: `crates/vestrace-domain/src/models/qualification.rs`
- Create: `crates/vestrace-domain/src/models/evidence.rs`
- Create: `crates/vestrace-domain/src/models/admission.rs`
- Modify: `crates/vestrace-domain/src/connection/mod.rs`
- Modify: `crates/vestrace-domain/src/models/mod.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Test in each new module.

**Interfaces:**
- `ConnectionKind = LMStudioLocal | OpenAiChatCompletionsV1`; `ConnectionAuthMode = None | Bearer | ApiKey | XApiKey`; `ConnectionRevision` contains normalized logical and runtime base URLs, adapter/profile revision, transport policy, closed auth mode, and `Option<CredentialSlotId>` validated as XOR.
- `NoAuthBindingRevision` names only workspace/Connection/ConnectionRevision and `ConnectionAuthMode::None`.
- `QualificationTargetBinding = Credential { revision_id, slot_id, activation_guard_id, expected_slot_version } | NoAuth { binding_revision_id }`.
- `QualificationJobState = Requested | Running | Succeeded | FailedDefinite | InconclusiveUnknown | Cancelled`; probe results use the exact q1 five-value vocabulary.
- `ModelRevision` contains exact ConnectionRevision, wire model id, `ModelKind = Chat | Embedding`, and provenance-bearing optional observations where absence is `Unknown`, not zero.
- `ModelBindingSnapshot` is one type with one exact `QualificationTargetBinding`; no q1-specific or transition-specific snapshot type is added.
- `ModelRequestEvidenceStatus = Complete | Incomplete | Expired`; evidence nodes are typed references, never raw bytes or stable content hashes.
- Admission types expose a versioned `ConnectionAdmissionPolicy`, rolling-window decision, lease id and typed throttle/conflict errors.

- [ ] **Step 1: RED**

Write tests named:

```text
no_auth_revision_has_no_credential_shape
credential_auth_requires_an_exact_slot
qualification_target_is_a_strict_xor
terminal_unknown_cannot_publish_qualification
model_revision_preserves_unknown_instead_of_zero
model_binding_snapshot_has_one_auth_branch
model_request_evidence_contains_only_typed_references
```

Use `static_assertions`/compile-fail coverage where a type-level property is claimed.

```powershell
cargo test -p vestrace-domain connection::revision models::binding models::qualification models::evidence models::admission
```

- [ ] **Step 2: GREEN**

Implement constructors that reject invalid combinations before persistence. No constructor accepts a caller-selected header name or a generic JSON auth object.

```powershell
cargo test -p vestrace-domain
cargo check --workspace --all-targets
```

- [ ] **Step 3: Scoped diff review**

Search the no-auth definitions for every credential identifier/type and require zero matches. Search MRE fields for `prompt`, `content`, `body`, `authorization`, `digest`, and exact byte counts; only explicitly safe structural metadata may remain.

---

### Task 3: Provision the P03 guarded-owner bridge and immutable PostgreSQL schema

**Files:**
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `docker-compose.yml`
- Modify: `docs/getting-started.md`
- Create: `migrations/0176_connection_revisions_and_no_auth_bindings.sql`
- Create: `migrations/0177_qualification_jobs_and_revisions.sql`
- Create: `migrations/0178_model_revisions_and_binding_snapshots.sql`
- Create: `migrations/0179_model_request_evidence.sql`
- Create: `migrations/0180_connection_admission_and_credential_leases.sql`
- Create: `migrations/0181_credential_activation_and_rotation.sql`
- Create: `migrations/0182_provider_effect_atomic_dispatch.sql`
- Modify: `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`
- Create: `crates/vestrace-infrastructure/tests/provider_schema_contract.rs`
- Create: `crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs`

**Interfaces:**
- New stable/head/revision tables are `connection_revision_heads`, `connection_revisions`, `no_auth_binding_revisions`, `qualification_jobs`, `qualification_target_bindings`, `qualification_probe_results`, `connection_qualification_revisions`, `connection_qualification_heads`, `model_revision_heads`, `model_revisions`, `model_qualification_revisions`, `model_qualification_heads`, `workspace_model_defaults`, `model_binding_snapshots`, `run_model_binding_snapshots`, `model_request_evidence_roots`, `model_request_evidence_nodes`, `model_request_evidence_checks`, `connection_admission_policy_heads`, `connection_admission_policy_revisions`, `connection_admission_states`, `connection_dispatch_admissions`, `provider_admission_waits`, `provider_concurrency_leases`, `provider_throttle_observations`, `credential_dispatch_leases`, and append-only credential activation/rotation evidence tables.
- Provider-result schema adds `provider_result_preparations`, `provider_result_publications`, and `artifact_revision_contents`. The latter is the typed map from an Artifact revision to one `ContentMaterialId`, erasure-bound commitment, `SizeClass`, and safe media class; it contains no raw bytes, unkeyed digest, exact plaintext length, or generic payload JSON. Existing legacy artifact digest rows remain readable but are not used for new provider results.
- Composite foreign keys enforce workspace, ConnectionRevision, ModelRevision, qualification, and binding identity parity. Partial unique indexes enforce one current head, one binding per job, one snapshot per durable cause, and one live concurrency lease per declared slot.
- `vestrace_assign_p03_table_owner(REGCLASS)` and `vestrace_assign_p03_function_owner(REGPROCEDURE)` hardcode only exact P03 objects and hardcode target owner `vestrace_guarded_owner`. They perform handoff before table revokes/function runtime grants so owner ACLs survive.
- As in the accepted P02 migrations, an automatic-migration test database may synthesize byte-equivalent P03 ownership helpers only when `current_user` is a superuser. A non-superuser runtime migration must find and use the helpers installed by the real bootstrap; it never receives a fallback that broadens its authority.
- The bootstrap-only `vestrace_disable_superseded_p02_credential_erasure()` has no parameters, contains only a static revoke of runtime `EXECUTE` on `public.vestrace_prepare_credential_material_erasure(UUID)`, revokes its own runtime `EXECUTE` in the same transaction, and cannot select another object, role, owner, or privilege. Migration 0181 calls it only after both P03 replacement functions exist and have their final ownership/ACLs. The idempotent provisioner creates/grants this helper only while migration 0181 is absent; once 0181 is recorded, it performs the same hardcoded revoke administratively, removes any residual helper, and never re-grants it on later restarts.
- Fresh deployment under runtime must use that real bootstrap helper. Ordinary automatic-migration SQLx databases may take the existing P02-style superuser fallback and perform the identical static revoke directly; that convenience is not deployment evidence. Tests assert both paths yield the same final owner/ACL/execute set, while Task 3 Step 3 alone qualifies bootstrap-before-runtime-migration ordering.
- Compose adds an idempotent `vestrace-role-provision` administrative one-shot that mounts and executes the same `init-runtime-role.sh` against PostgreSQL with the bootstrap credential after database health and before `vestrace-migrate`. The migration service itself remains `vestrace`; server/worker/MCP receive no bootstrap credential. The provisioning step changes only roles/extensions/schema ownership and exact allowlisted bridge functions, so rerunning it on an existing P02 volume neither applies migrations nor re-owns product tables. Getting-started documents the equivalent administrator-before-runtime-migrate upgrade order for non-Compose installations.
- The P03 runtime entrypoints are exact: `vestrace_prepare_retired_or_revoked_credential_erasure(UUID)` and `vestrace_prepare_candidate_abandon_and_erasure(UUID, BIGINT)`. The first admits only an exact non-current `Retired`/`Revoked` revision; the second atomically performs the guarded Candidate cancellation/association closure and erasure preparation at the expected association version. Neither admits `Active`, and the old Candidate-only entrypoint is not a fallback.
- Provider result entrypoints are exact: Task 10 forward-replaces prepare as `vestrace_prepare_provider_result(UUID, UUID, UUID, UUID, UUID, UUID, BYTEA, BIGINT) RETURNS UUID` for `(effect_id, material_intent_id, prepared_attachment_id, artifact_id, artifact_revision_id, model_execution_id, ciphertext, size_class)`, adds `vestrace_witness_provider_result_receipt(UUID, UUID) RETURNS UUID` for `(provider_result_preparation_id, external_effect_receipt_id)` returning the witnessed receipt UUID, and retains `vestrace_finalize_provider_result(UUID, BYTEA) RETURNS VOID` for `(provider_result_preparation_id, erasure_bound_commitment)`. Prepare derives run, step, MRE and fixed output ordinal zero from the normalized dispatch cause and alone calls the internal P02 `vestrace_prepare_result_material`; preparation, transaction-bound definite receipt insertion, and the exact receipt witness commit together. The SQL finalizer requires both that witnessed provider receipt and the material intent's witnessed Bound receipt, calls P02 Live promotion internally, and inserts only guarded provider-result publication/content-map rows. The 32-byte commitment is `HMAC-SHA256(DEK, domain || workspace_uuid || content_material_uuid || material_key_uuid || ciphertext_len_u64_be || ciphertext)`, where `domain` is the exact ASCII byte string `vestrace-provider-result-erasure-bound-v1\0` and UUIDs are their 16 network-order bytes. A caller-owned runtime transaction inserts the preallocated safe Artifact/Revision and model-execution envelopes, calls this SQL finalizer, and uses transaction-bound Run persistence for success/reference/work continuation. Deferred constraints require the complete tuple at commit, so these steps are one atomic `ProviderResultRepository::finalize_in` without granting the guarded owner DML on pre-P02 runtime-owned tables.
- Existing `connections`, `models`, and `providers` retain runtime ownership. Guarded creators may receive only the minimal DML needed to maintain compatibility projections; raw catalog rows without guarded heads cannot become executable.

- [ ] **Step 1: RED**

Before adding migrations, run:

```powershell
$env:DATABASE_URL = "postgres://test:test@localhost:55432/vestrace_test"
$env:VESTRACE_RUNTIME_DATABASE_URL = "postgres://vestrace:runtime-local-development-only@localhost:55432/vestrace_test"
cargo test -p vestrace-infrastructure --test provider_schema_contract -- --nocapture
cargo test -p vestrace-infrastructure --test runtime_role_cannot_write_directly -- --nocapture
cargo test -p vestrace-infrastructure --test p03_upgrade_provisioning -- --nocapture
```

Required RED includes `42P01` for absent new objects; exact-`42501` assertions must fail rather than accept absence.

- [ ] **Step 2: GREEN schema and ownership**

Implement forward migrations and extend the test bootstrap by executing the real `RUNTIME_ROLE_PROVISIONING` content from `docker/postgres/init-runtime-role.sh`. Do not write a separate equivalent helper beside it.

```powershell
cargo test -p vestrace-infrastructure --test provider_schema_contract -- --nocapture
cargo test -p vestrace-infrastructure --test runtime_role_cannot_write_directly -- --nocapture
cargo test -p vestrace-infrastructure --test p03_upgrade_provisioning -- --nocapture
```

Assert exact `42501` for INSERT/UPDATE/DELETE on every new table, exact ownership by the non-login guarded role, no empty ACL, and exact runtime execute-set equality with migration-declared grants. Invoke at least one guarded function as `vestrace` and assert its write exists.
The upgrade test also contains a static Compose assertion named `compose_provisioning_precedes_runtime_migrate_without_leaking_bootstrap_credentials`: `vestrace-role-provision` must use the bootstrap identity, `vestrace-migrate` must depend on its successful completion while using only the runtime URL, and server/worker/MCP must contain neither bootstrap URL nor bootstrap password.

- [ ] **Step 3: Deployment-path migration regression**

Use explicitly documented `#[sqlx::test(migrations = false)]` deployment tests. One creates a fresh database; the second migrates only through accepted P02, omits every P03 helper to model an existing volume, then reruns the real bootstrap. In both, run provisioning as the test superuser before invoking the real embedded migrator as runtime. Assert the existing-volume run acquires the P03 helpers, both reach the same final schema, pre-P02 tables remain runtime-owned except for the explicit Task 10 hardening of append-only `model_data_policy_decisions`, and only declared guarded tables plus that one declared deployment-evidence exception belong to `vestrace_guarded_owner`. All other PostgreSQL tests keep automatic migrations.

- [ ] **Step 4: Scoped diff review**

Confirm `WITH ADMIN FALSE, INHERIT FALSE, SET FALSE` is unchanged, the P02 allowlists are unchanged, pre-existing tables are not re-owned, and the P03 ownership helpers cannot accept an owner or undeclared object. Confirm the one-shot supersession helper has no parameter/dynamic SQL/grant path and that runtime has neither its execute privilege nor the old Candidate-only erasure privilege after migration.

---

### Task 4: Create stable Connections and immutable revisions through one governed transaction

**Files:**
- Modify: `crates/vestrace-application/src/connections.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/connection_revision_lifecycle.rs`
- Create: `crates/vestrace-infrastructure/tests/connection_mutation_is_atomic.rs`

**Interfaces:**
- `CreateConnectionRevision` accepts a preallocated stable Connection id, revision id, guard id, kind, exact URLs, transport policy, auth mode, optional exact slot, expected head version, idempotency key, and Audit context.
- `ConnectionRevisionRepository::create_governed` owns one P02 `UnitOfWork`: take `InstallationMutationPermit::Shared`, insert stable identity/compatibility projection, call `vestrace_ensure_connection_execution_guard`, insert guarded revision/head/no-auth binding as applicable, save idempotency and outbox, record Audit, advance mutation watermark, then commit.
- The stable Connection owns enabled/disabled/archived state plus expected-version current-revision CAS; no immutable revision owns mutable operational state. `revise_governed` takes the shared permit then locks `ConnectionExecutionGuard`, checks expected head version, creates a new immutable revision, atomically advances only the head, and never edits an old revision.
- Auth-required creation refuses absent slot/activation guard. `None` creates its one-to-one no-auth binding in the same transaction and refuses every credential reference.

- [ ] **Step 1: RED**

Write real-PostgreSQL tests:

```text
connection_creation_atomically_creates_its_permanent_guard
no_auth_revision_atomically_creates_one_no_auth_binding
auth_required_revision_cannot_own_a_no_auth_binding
connection_revision_is_immutable_and_head_uses_expected_version
connection_mutation_rolls_back_when_audit_fails
audit_rolls_back_when_connection_mutation_fails
raw_catalog_row_is_not_executable_without_a_guarded_head
```

```powershell
cargo test -p vestrace-infrastructure --test connection_revision_lifecycle -- --nocapture
cargo test -p vestrace-infrastructure --test connection_mutation_is_atomic -- --nocapture
```

- [ ] **Step 2: GREEN**

Implement the repository using `PgStore::begin_scoped` and transaction-bound P02 ports. No self-transacting audit or post-commit guard creation is permitted.

- [ ] **Step 3: Concurrency proof**

From two independent pools, race revisions at the same expected head version. Assert exactly one commits, the loser receives typed `CONNECTION_VERSION_CONFLICT`, and neither creates a second guard or a partially current revision.

- [ ] **Step 4: Scoped diff review**

Verify no update statement targets an immutable revision row and no production path reads `providers` or `models.model_name` as Connection routing authority.

---

### Task 5: Complete credential first activation, ordinary rotation, and exact-effect dispatch leases

**Files:**
- Create: `crates/vestrace-application/src/credential/activation.rs`
- Modify: `crates/vestrace-application/src/credential/mod.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/credential_activation.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/credential_dispatch_lease.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/credential_activation.rs`
- Create: `crates/vestrace-infrastructure/tests/credential_dispatch_lease.rs`
- Modify: `crates/vestrace-infrastructure/tests/erasure_is_one_way.rs`

**Interfaces:**
- `CredentialActivationRepository::activate_first` takes `InstallationMutationPermit::Shared -> ConnectionExecutionGuard -> CredentialActivationGuard -> slot -> Candidate/association/qualification rows`, then in one transaction publishes the exact Connection qualification, binds the slot to the stable Connection, CASes absent current revision to Candidate, appends `Active`, closes association `Activated`, records governed Audit/idempotency/outbox, and advances the watermark.
- `rotate` re-queries the complete current non-embedding executable ModelRevision dependency set and candidate qualifications under the same guards. Any live embedding dependency yields `CredentialActivationError::EmbeddingTransitionRequired`; no partial pointer changes occur.
- `revoke` is immediate after the canonical locks and permanently blocks future resolution. It does not fabricate an embedding transition or destroy material inside the revoke transaction.
- Migration 0181 replaces runtime use of the P02 Candidate-only erasure entrypoint with the two exact functions declared in Task 3. Ordinary destruction is available only after `Retired`/`Revoked`. Live Candidate abandonment is a separate guarded command that atomically appends cancellation/association evidence and erasure preparation; it is not the pre-live intent-abort branch and never accepts Active.
- `CredentialDispatchLease` names exact workspace, purpose, CredentialRevision, authorization decision, effect id, normalized destination authority, auth mode, issue/expiry and one append-only state. Only the pre-dispatch transaction can append `ConsumedForDispatch`.
- Credential unwrap returns `ZeroizingDek`/zeroizing plaintext and occurs only after every database predicate passes; it is not used by activation.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test credential_activation -- --nocapture
cargo test -p vestrace-infrastructure --test credential_dispatch_lease -- --nocapture
```

Tests separately prove:

```text
first_activation_is_all_or_nothing
preparing_or_unbound_candidate_cannot_activate
late_qualification_cannot_activate_a_changed_slot
rotation_switches_every_non_embedding_dependency_or_none
embedding_dependency_refuses_without_p04_transition_evidence
activation_never_creates_or_consumes_a_dispatch_lease
lease_is_exact_effect_exact_authority_and_one_use
rotated_revoked_erasure_prepared_and_destroyed_revisions_cannot_lease
superseded_candidate_only_erasure_entrypoint_is_runtime_42501
active_credential_material_is_never_destructible
retired_and_revoked_use_only_the_ordinary_erasure_entrypoint
candidate_erasure_requires_the_atomic_candidate_abandon_entrypoint
```

- [ ] **Step 2: GREEN**

Implement only guarded functions and repositories. Reuse the P02 intent/material/vault types; do not introduce a second credential store or copy plaintext into PostgreSQL.

- [ ] **Step 3: Revisit the P02 erasure precondition**

Implement the forward-only supersession from Task 3; do not edit migration 0174. Add RED/GREEN assertions that the old signature returns exact `42501` as runtime, `Active` is rejected with the exact guarded state SQLSTATE/message, Retired/Revoked are admitted only by the ordinary P03 function, and Candidate is admitted only by the association-cancelling Candidate-abandon function. Terminal effects/expired leases cease to block while `Authorized`/`Dispatching` or usable leases do block.

- [ ] **Step 4: Scoped diff review**

Confirm no activation signature accepts a lease, no lease signature accepts Candidate, and every decrypt result is zeroizing and absent from `Debug`/Serialize/log fields.

---

### Task 6: Create Model revisions, effective qualifications, defaults, and the sole binding snapshot

**Files:**
- Modify: `crates/vestrace-application/src/models/ports.rs`
- Modify: `crates/vestrace-application/src/models/mod.rs`
- Create: `crates/vestrace-application/src/models/revisions.rs`
- Create: `crates/vestrace-application/src/models/bindings.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_binding_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run_command_committer.rs`
- Create: `crates/vestrace-infrastructure/tests/model_binding_snapshot.rs`
- Create: `crates/vestrace-infrastructure/tests/run_acceptance_binding_race.rs`

**Interfaces:**
- `ModelRevisionRepository::create_governed` takes `InstallationMutationPermit::Shared` then creates a stable Model compatibility row plus immutable revision/head under the referenced Connection guard and P02 mutation/Audit transaction.
- A workspace default names a stable Model and required capabilities, not a qualification id. Refresh/expiry never rewrites the default.
- `ModelBindingResolver::resolve_for_run_in` locks Connection guard first, optionally the exact credential guard, then requires current ConnectionRevision, unexpired compatible ConnectionQualificationRevision, current ModelRevision, unexpired ModelQualificationRevision, and exact same auth-binding branch.
- `ModelBindingSnapshot` is inserted before the Run becomes executable and linked one-to-one through `run_model_binding_snapshots`. `run_command_committer` performs snapshot insertion and Run/work-item acceptance in the same SQL transaction.
- A no-auth resolution takes no CredentialActivationGuard and proves absence of every credential reference. Credential resolution takes exactly one guard and pins exact slot version/revision.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test model_binding_snapshot -- --nocapture
cargo test -p vestrace-infrastructure --test run_acceptance_binding_race -- --nocapture
```

Tests prove incompatible/expired qualifications, discovery-only evidence, Candidate outside its guarded activation path, and raw legacy catalog rows cannot produce a snapshot.

- [ ] **Step 2: GREEN**

Implement the resolver and guarded SQL. The current HTTP create-run payload remains unchanged: the workspace default is resolved inside the durable acceptance transaction, so the protected dirty `api/runs.rs` file is not touched.

- [ ] **Step 3: Genuine concurrency proof**

Race Run acceptance with credential rotation and with Connection head revision. Assert either the old complete tuple and Run commit first, or the new tuple is observed; no mixed tuple or retired credential can be pinned.

- [ ] **Step 4: Scoped diff review**

Repository-wide search must show exactly one `struct ModelBindingSnapshot`. `QualificationTargetBinding` and q1 target records must not implement a routing resolver.

---

### Task 7: Execute the exact q1 QualificationJob lifecycle through shared effects

**Files:**
- Create: `crates/vestrace-application/src/provider_qualification.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-infrastructure/src/openai_q1.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/qualification_job_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/qualification_target_binding.rs`
- Create: `tests/openai_q1_loopback.rs`

**Interfaces:**
- `OpenAiQ1Profile::from_pinned_manifest()` loads the compile-time pinned JSON and exposes exact ordered probes `00,10,15,20,30,35,40,50,60,70,80,90`, exact bounds, prerequisites, status mapping and nonce derivation.
- `QualificationJobService::request` takes `InstallationMutationPermit::Shared` before the Connection/optional credential guards and persists one immutable target binding before any probe. `run_next_probe` uses Task 10's shared permit/admission/dispatch authority, creates one MRE and one external effect per network probe, and makes static probes create neither effect nor fake receipt.
- Job cancellation is accepted only between probes. A dispatching probe cannot be cancelled.
- Required probes must pass. Optional probes may be `UnsupportedDefinite` only under the manifest's exact status/oracle rules. `InconclusiveUnknown` stops the suite, is terminal, and publishes no qualification.
- The success finalizer re-locks canonical guards, verifies every required oracle/effect/MRE is terminal and current `Complete`, creates immutable qualification revisions, and either makes exact-current Active/no-auth evidence effective or leaves Candidate evidence non-effective for Task 5 activation.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test qualification_target_binding -- --nocapture
cargo test --test openai_q1_loopback -- --nocapture
```

The loopback server deliberately returns each manifest status/oracle class and counts adapter invocations. Assert one invocation/effect for every network probe and zero automatic retry.

- [ ] **Step 2: GREEN**

Implement the parser/runner without changing the q1 manifest or fixture. The runner must derive nonce as the first 96 SHA-256 bits of `QualificationJobId || profile_digest || probe_ordinal`, encoded as 24 lowercase hex characters.

- [ ] **Step 3: Crash and Unknown proof**

Interrupt after `Dispatching` and before response persistence. Restart the worker/reconciler and assert the original effect becomes `Unknown`/reconciled under existing recovery, the job becomes terminal `InconclusiveUnknown`, no second HTTP request occurs, and no qualification is published.

- [ ] **Step 4: Scoped diff review**

Compare the runner's exact probe order, required/optional flags, bounds and status table to `tests/protocol_q1_manifest.rs`; no locally invented q1 profile field is permitted.

**Task 7 corrective acceptance after independent plan review (`VERDICT: REVISE`):**

- Task 7 is implemented only through the new forward migration `migrations/0185_qualification_job_lifecycle.sql`; already-applied migrations 0177, 0180, 0183 and 0184 remain historical source authority and are not rewritten to smuggle upgrade behavior. Migration 0185 adds exact fixed-search-path guarded functions for atomic job-plus-target request, ordered probe-result transition, between-probes cancellation, shared post-network completion/recovery, and success finalization. Runtime receives only the exact required `EXECUTE` grants; direct runtime DML remains exact `42501`, internal helpers remain non-executable, and 0176/bootstrap function-owner allowlists plus fresh and 0184-to-0185 upgrade proofs advance together.
- Request creation holds `InstallationMutationPermit::Shared` before Connection and optional credential guards and atomically pins the immutable ConnectionRevision, authentication branch, expected slot version, and exact chat plus embedding ModelRevision identities required by the q1 templates. Composite same-workspace foreign keys make restart/head races reproduce the same wire model ids. Probe execution never consults mutable Connection, credential or model heads after this binding exists.
- The existing shared credential-dispatch lease authority is forward-replaced, not forked. A `run_step` dispatch remains exact current-Active only. A `qualification_probe` dispatch may use only the Candidate or Active credential revision named by its immutable `QualificationTargetBinding` and matching `provider_dispatch_cause`, with intact candidate association/material, exact activation guard and expected slot version under the same permit/guard order. No q1-only lease, unwrap, admission, effect or dispatch authority is permitted.
- The existing governed provider request/result/error types and the single OpenAI-compatible adapter are extended just enough to express every pinned q1 oracle: multipart image input, tool-call identity/arguments, `tool_choice`, parallel tool calls, JSON-schema response format, stream options/order/usage timing, returned model identity, embedding usage and exact safe non-2xx HTTP status classification. Migration 0185 and the already-admitted MRE application/repository paths add closed, safe canonical q1 request-source fields/nodes for every such request choice, including assistant tool-call replay and multipart structure; one Complete MRE must reconstruct the exact request handed unchanged to the adapter. Per-field mutation tests prove the binding, while no raw request body, auth or content copy is retained. Q1 may not pass through the legacy generic JSON compatibility DTOs. Credential-bearing allocations remain bounded and zeroizing, and errors/evidence retain no auth, raw body or content.
- The shared dispatch authority gains one idempotent post-network completion operation that composes the exact receipt, lease release, optional 429 throttle observation and replay under the original effect/cause. Qualification recovery discovers the original cause and turns a lost post-`Dispatching` attempt into terminal `InconclusiveUnknown` without a second adapter call. Static ordinals `00` and `15` create no MRE, effect, receipt or adapter invocation; every network ordinal serializes one intent and one MRE by job/ordinal before dispatch and can create exactly one external effect.
- Nonce derivation is fixed byte-for-byte: concatenate the 16 RFC 4122 UUID bytes of `QualificationJobId`, the 32 raw SHA-256 bytes of the pinned manifest, and the two ASCII ordinal bytes; take SHA-256, retain the first 12 bytes, and encode 24 lowercase hexadecimal characters. An independent fixed vector and the manifest-contract test must agree.
- Success finalization re-locks the canonical target guards, verifies the exact 12-probe order/prerequisite matrix plus terminal effect/MRE/oracle evidence, atomically writes immutable connection and model qualification revisions, and marks the job succeeded. For a Candidate credential those revisions are stored as pending and no effective connection or model qualification head changes; Candidate evidence remains non-effective. Migration 0185 forward-replaces the exact Task 5 first-activation and rotation functions so, after their existing Candidate/slot/CAS predicates succeed, activation advances the connection qualification head and both pinned chat/embedding model qualification heads atomically with the credential-slot transition. An already-Active or no-auth finalization may advance its matching effective heads immediately. Definite failure, cancellation and `InconclusiveUnknown` publish no qualification.
- This corrective Task 7 slice explicitly admits the already-global P03 domain target-binding path; shared provider dispatch, MRE and governed provider ports; PostgreSQL dispatch, credential-lease, MRE and qualification repositories; the OpenAI-compatible adapter; Task 5 credential-activation repository and tests; 0176/bootstrap authority lists; and their schema, runtime-role, fresh/upgrade, dispatch, lease, MRE, activation, transport, RLS and q1 tests. The operator-authorized scope amendment expands global P03 scope from 121 to 123 paths by adding only `migrations/0185_qualification_job_lifecycle.sql` and `crates/vestrace-infrastructure/src/lib.rs`; the crate root may change only to declare/export `openai_q1`. Task 11 wiring, non-loopback network/LM Studio calls, commits, pushes and deployment remain outside this slice.

---

### Task 8: Harden one production OpenAI-compatible transport for all provider calls

**Files:**
- Modify: `crates/vestrace-application/src/providers/ports.rs`
- Modify: `crates/vestrace-infrastructure/src/providers/openai_compatible.rs`
- Modify: `crates/vestrace-infrastructure/src/providers/mod.rs`
- Remove production use only: `crates/vestrace-infrastructure/src/providers/secret_backed.rs`
- Create: `crates/vestrace-infrastructure/tests/openai_transport_security.rs`
- Create: `crates/vestrace-infrastructure/tests/openai_auth_modes.rs`

**Interfaces:**
- The adapter exposes typed `models_list`, `chat_completions`, and `embeddings`; q1 and production dispatch call the same methods.
- URL normalization owns endpoint joining and accepts a fixed base prefix. Remote profiles require HTTPS, no userinfo/query/fragment, WebPKI, `.redirect(Policy::none())`, `.no_proxy()`, bounded connect/read/total timeouts, and no caller Host/header override.
- Before building a remote client, resolve all A/AAAA results; refuse any loopback/private/link-local/multicast/unspecified/metadata address. Pin the accepted set in the client and require `Response::remote_addr()` to be one of the accepted public peers. `LMStudioLocal` is the sole HTTP exception and records the host-bridge peer as `RemoteProvider` when not loopback.
- Auth injection is closed and canonical: `None` emits no auth header; `Bearer` emits `Authorization: Bearer`; `ApiKey` emits `api-key`; `XApiKey` emits `x-api-key`. No credential enters Debug or an error.
- Errors persist only bounded status/code/type and an allowlisted provider correlation id. Raw body, body excerpt, body digest/fingerprint and exact body size are discarded.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test openai_transport_security -- --nocapture
cargo test -p vestrace-infrastructure --test openai_auth_modes -- --nocapture
```

Use loopback DNS/HTTP fixtures to prove redirect refusal, proxy non-use, mixed safe/forbidden DNS refusal, peer mismatch refusal, canonical headers, no-auth absence, byte/time/event bounds, and raw-body redaction.

- [ ] **Step 2: GREEN**

Refactor the existing adapter rather than adding a second client. Preserve the already passing redirect/proxy and safe-error tests.

- [ ] **Step 3: Semantic adapter observation**

Capture request method/path/headers and parsed JSON at loopback for ModelsList, chat and Embeddings. Assert exact endpoint joining, no `dimensions` field for Embeddings, and no unauthorized header. Do not snapshot raw credential values.

- [ ] **Step 4: Scoped diff review**

Search production code for direct `reqwest::Client::new`, `/chat/completions`, `/models`, `/embeddings`, and auth-header construction. Every provider network path must converge on this adapter.

---

### Task 9: Persist and recompute ModelRequestEvidence without retaining a request duplicate

**Files:**
- Modify: `crates/vestrace-domain/src/models/evidence.rs`
- Create: `crates/vestrace-application/src/model_request_evidence.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Modify: `crates/vestrace-application/src/providers/ports.rs`
- Create: `crates/vestrace-infrastructure/src/crypto/content_material_codec.rs`
- Modify: `crates/vestrace-infrastructure/src/crypto/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/model_request_evidence_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/providers/openai_compatible.rs`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `migrations/0176_connection_revisions_and_no_auth_bindings.sql`
- Create: `migrations/0183_model_request_reconstruction_contract.sql`
- Create: `crates/vestrace-infrastructure/tests/model_request_evidence.rs`
- Modify: `crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs`
- Modify: `crates/vestrace-infrastructure/tests/provider_schema_contract.rs`
- Modify: `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`
- Create: `tests/model_request_semantic_observation.rs`

**Interfaces:**
- `ModelRequestEvidenceRepository::create_in` writes one immutable root per external effect and ordered typed nodes referencing the exact cause, binding snapshot or qualification target, model/configuration/qualification/profile revisions, governed input material revisions, tool schemas, sampling/limits and request-shape revision.
- `reconstruct_current_in` accepts the caller-owned `UnitOfWork` and is the only production reconstructor. While the caller holds the exact binding and material guards, it revalidates current `Complete`, hydrates the named Live materials, materializes pinned defaults, and returns one bounded zeroizing `EffectiveModelRequest` closed over `ModelsList | ChatCompletions | Embeddings`.
- `EffectiveModelRequest` has no `Clone`, `Serialize`, or byte-revealing `Debug`. Policy and authorization receive shared borrows of that exact instance; after the pre-dispatch transaction commits, the adapter consumes the same instance. No check-body/send-body pair or second renderer exists.
- The closed request kind variants are `ModelsList`, `ChatCompletions`, and `Embeddings`. Embeddings binds ordered governed inputs plus exact `model` and `encoding_format = float`; it has no dimensions field.
- `reconstruct_current` reads the named retained canonical sources and returns a semantic DTO plus `Complete`, a typed missing-reference list plus `Incomplete`, or only `Expired` when every missing governed byte is explained by exact authorized erasure preparation/tombstone evidence.
- `model_request_evidence_checks` is append-only observation history. Current status is recomputed; an earlier `Incomplete` row is never rewritten away when a later implementation reconstructs successfully.
- The semantic DTO exists only in bounded process memory for adapter invocation/inspection. PostgreSQL stores neither it nor its serialization/digest.
- `0183_model_request_reconstruction_contract.sql` supplies the previously missing canonical sources as immutable, workspace-scoped, forced-RLS tables for request shape, sampling, limits and tool-schema revisions. Each table has typed bounded columns; the Chat request-shape revision carries an exact ordered array of closed non-content message roles (`system | user | assistant | tool`) whose cardinality and positions are database-enforced as a bijection with that root's ordered governed-input nodes, while non-Chat shapes forbid roles. Tool JSON is permitted only in its specifically named schema column. No catch-all payload, rendered request, canonical JSON or content digest is stored.
- Runtime creation of canonical revisions, one evidence root with its ordered nodes, and append-only reconstruction checks uses exact allowlisted `SECURITY DEFINER` entrypoints. Direct runtime DML remains `42501`; the bootstrap allowlist and exact ACL tests advance together. `Expired` checks link every missing governed material to its exact finalized erasure preparation/tombstone rather than retaining one unexplained aggregate count. The append guard locks the root's full governed-input set and rejects an omitted input unless it is still Live with a present SQL-observable valid frame: bounded power-of-two length plus exact magic/version header. AAD, authenticated-open and padding validity remain process-only observations because proving them requires a DEK unwrap; when any exact finalized erasure already makes the request unreconstructible, the required `Expired` path performs zero unwraps and therefore cannot claim those process-only observations.
- Governed material bytes use one shared versioned padded AEAD frame. The frame carries its nonce, keeps the exact content length inside authenticated ciphertext, and binds AAD to workspace id, material id, material-key id and the fixed codec profile. The same production codec seals fixtures and opens Live material during reconstruction; ad-hoc test decoders and unframed plaintext fixtures do not qualify.
- `create_in` and `reconstruct_current_in` downcast only the caller-owned PostgreSQL unit of work. Reconstruction locks the exact binding and material rows in deterministic order, recomputes status and hydrates before releasing those guards. Erasure preparation either wins before hydration and yields no request bytes, or waits until the guarded reconstruction/dispatch transaction completes.
- The guarded creator enforces this closed node matrix in PostgreSQL, not only in Rust. Every root has exactly one matching `external_effect`, `connection_revision`, `request_shape_revision`, and `limits_revision`. `ModelsList` forbids model, sampling, tool and governed-input nodes. `ChatCompletions` requires exactly one model and sampling revision and one-or-more governed input materials, and permits ordered tool-schema revisions. `Embeddings` requires exactly one model revision and one-or-more governed input materials, and forbids sampling, tool and dimensions-shaped nodes. A `run_step` root additionally requires exactly one matching `binding_snapshot`, one connection qualification and one model qualification and forbids qualification-target/probe nodes. A `qualification_probe` root instead requires exactly one matching qualification target and probe ordinal, forbids binding and qualification-revision nodes, and uses a model revision only for chat/embeddings. The validator refuses wrong-kind, cross-workspace, wrong-version, duplicate, missing, extra, non-contiguous or tuple-incompatible nodes before any root can be observed as `Complete`.
- Canonical request-shape, sampling, limits and tool-schema revisions plus their MRE root/nodes are created by one caller-owned transaction. The root owns the exact source-revision set; all effective defaults are persisted as fully resolved bounded values, never reread from current Rust/adapter defaults. Advisory serialization on external-effect identity makes an exact replay return the existing root and makes any unequal replay fail with `MODEL_REQUEST_EVIDENCE_CONFLICT`; concurrent exact and conflicting replay are tested.
- The production OpenAI-compatible adapter adds one consuming entrypoint over `EffectiveModelRequest`. Policy and authorization borrow that object before the transaction commits; the adapter then consumes it and renders its private typed wire form exactly once. Governed dispatch, semantic observation and q1 integration may not convert it through the legacy `ChatCompletionsRequest`, `EmbeddingsRequest`, generic `serde_json::Value`, or a second check/send renderer. Existing Task 8 compatibility methods may remain only for their already-supported non-governed callers and tests.
- Codec ceilings are fixed at 1 MiB per framed material and exactly 4 MiB aggregate decoded request content. Before selecting or allocating ciphertext, `octet_length` and node count enforce a separate derived framed-allocation ceiling: for `N` governed inputs it is at most `N * 4096 + 2 * 4 MiB`, which follows from the codec's 4096-byte minimum and power-of-two padding and therefore admits every valid request whose decoded aggregate is at most 4 MiB while bounding a collected frame set to at most 24 MiB at `N = 4096`. The exact decoded aggregate is checked again after authenticated open. Every plaintext-bearing intermediate is zeroized on success and error. Malformed/truncated/unknown-version frames, impossible encrypted lengths, padding mismatch, wrong AAD, legacy unframed P02 bytes, vault `NotFound`/`Erased`/`ErasurePrepared` without exact finalized database erasure, and cryptographic open failure derive typed `Incomplete`; transient vault `Unavailable` returns unavailable and appends no false status. Exact database-authorized finalized erasure derives `Expired` without vault unwrap. SQL, size, node and frame-header refusals prove zero unwraps; wrong-AAD, padding and authenticated-open failures prove exactly one bounded unwrap plus typed `Incomplete`; finalized erasure proves zero unwraps.
- The lock order below the caller-held `ConnectionExecutionGuard` is exact: binding snapshot or qualification target; immutable request-source/revision rows; all `content_materials` rows in ascending material UUID; their `material_key_creation_intents` rows in ascending intent UUID; then byte rows. Erasure preparation/tombstone evidence is immutable and is read without a row lock after material state is known; reconstruction never waits on a preparation row while holding a material row. This avoids the existing finalizer's inverse `preparation -> material -> intent` lock path, while a held `Live` material row already forces a concurrent preparation to wait before it can change state. Independent-session races cover preparation and finalization against both the first and a later input and refuse any `40P01` outcome.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test model_request_evidence -- --nocapture
cargo test --test model_request_semantic_observation -- --nocapture
```

Required tests:

```text
complete_evidence_reconstructs_from_exact_revisions
missing_retained_source_is_incomplete_and_blocks_dispatch
authorized_erasure_alone_derives_expired
historical_incomplete_check_is_append_only
unknown_effect_preserves_the_same_evidence_root
postgres_never_stores_the_raw_semantic_request_or_a_stable_digest
wrong_kind_cross_workspace_wrong_version_duplicate_missing_and_extra_nodes_are_refused
concurrent_exact_replay_returns_one_root_and_conflicting_replay_is_refused
historical_evidence_survives_adapter_default_changes
loopback_observes_semantic_equality_with_the_production_adapter
effective_request_is_nonclone_and_nonserializable
policy_authorization_and_adapter_observe_one_request_instance
erasure_race_cannot_hydrate_after_complete_check
multi_material_erasure_preparation_and_finalization_races_do_not_deadlock
sql_size_header_and_unframed_refusals_do_not_unwrap
wrong_aad_padding_and_open_failures_unwrap_once_and_zeroize
```

- [ ] **Step 2: GREEN**

Implement reconstruction with explicit typed node loaders. Do not use generic JSON source blobs or concatenate canonical JSON for hashing. The loopback semantic observer may render the borrowed live object for comparison but cannot persist it.

- [ ] **Step 3: Proof by breaking**

Remove one required sampling/tool/input node in a test transaction and show the dispatch gate returns `MODEL_REQUEST_EVIDENCE_INCOMPLETE` before the loopback request counter advances. Mark the exact material erased and show only `Expired`, never recovered `Complete`. Race erasure preparation against dispatch from independent sessions and assert either guarded hydration/dispatch commits first or erasure wins and no request object/provider bytes exist; no separate post-check hydration is reachable.

- [ ] **Step 4: Scoped diff review**

Inspect schemas and Rust types for raw request/prompt/output/auth fields, `canonical_request_digest`, exact plaintext lengths and generic payload JSON. Any match must be removed or justified as a non-content typed reference.

---

### Task 10: Make dispatch atomic and finish retained provider results through P02 material authority

**Files:**
- Modify: `crates/vestrace-domain/src/artifact/mod.rs`
- Modify: `crates/vestrace-domain/src/run/step.rs`
- Modify: `crates/vestrace-application/src/artifacts.rs`
- Modify: `crates/vestrace-application/src/effect_repository.rs`
- Modify: `crates/vestrace-application/src/external_effects.rs`
- Modify: `crates/vestrace-application/src/governed_mutation.rs`
- Modify: `crates/vestrace-application/src/credential/activation.rs`
- Modify: `crates/vestrace-application/src/model_data_policy.rs`
- Modify: `crates/vestrace-application/src/run/ports.rs`
- Create: `crates/vestrace-application/src/provider_dispatch.rs`
- Create: `crates/vestrace-application/src/provider_result.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/artifact_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/external_effect_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/credential_dispatch_lease.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/model_data_policy_decision_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/pool.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/run/store.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/provider_result_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/provider_dispatch_is_atomic.rs`
- Create: `crates/vestrace-infrastructure/tests/provider_admission.rs`
- Create: `crates/vestrace-infrastructure/tests/provider_result_binding.rs`
- Modify: `crates/vestrace-infrastructure/tests/credential_dispatch_lease.rs`
- Modify: `crates/vestrace-infrastructure/tests/provider_schema_contract.rs`
- Modify: `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`
- Modify: `crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `migrations/0176_connection_revisions_and_no_auth_bindings.sql`
- Create: `migrations/0184_provider_dispatch_and_result_contract.sql`

**Interfaces:**
- Existing self-transacting effect and Run repository methods remain thin wrappers. Add transaction-bound `save_intent_in`, `record_authorization_in`, `record_dispatch_started_in`, receipt/finalization counterparts, and `RunStorePort::commit_in`, all accepting the caller-owned `UnitOfWork` and reusing one SQL implementation. `external_effect_authorizations` and the governed-mutation evidence dependencies `audit_events`, `idempotency_keys`, and `outbox` remain pre-P03 runtime-owned, forced-workspace-RLS tables whose repositories keep their shared direct SQL implementations; do not invent duplicate guarded functions or transfer these legacy tables to the guarded owner. Because automatic SQLx migrations are admin-owned while the real fresh deployment migrates as runtime, the 0176/bootstrap dependency helper must grant only each repository's actual operations: `external_effect_authorizations`, `audit_events`, and `idempotency_keys` receive `SELECT, INSERT`; `outbox` receives `SELECT, INSERT, UPDATE` for delivery state; none receives `DELETE`, and the first three receive no `UPDATE`. Migration 0184 invokes that helper so an already-migrated volume whose bootstrap was refreshed after 0183 receives the grants before provider dispatch. Tests assert every exact ACL, prove same-workspace repository operations succeed, and behaviorally prove forced RLS rejects restricted-runtime cross-workspace reads/writes on all four tables. The upgrade proof migrates through 0183, models the missing grants and stale helper, reruns the real provisioner, then applies only 0184 as runtime and proves the refreshed helper is invoked before successful authorization and Audit/idempotency/outbox writes; the full atomic-dispatch test remains the behavioral composition proof. Add transaction-bound credential-lease `issue_in`; retain the existing `consume_in` and its consume-before-unwrap ordering. No provider repository may duplicate effect, Run, credential, Audit, idempotency, or outbox SQL.
- `GovernedMutationRepository::commit_in` accepts the caller-owned `UnitOfWork` and reuses the exact existing Audit/idempotency/outbox plus `vestrace_record_governed_mutation_audit_mark_and_advance` implementation; the existing `commit` remains an acquire/call/commit wrapper. `ModelDataPolicyDecisionRepository::record_in` likewise shares one implementation with its existing self-transacting `record` wrapper. Because runtime has no direct `INSERT` authority on `model_data_policy_decisions`, both policy repository paths call the exact fixed-search-path guarded entrypoint `vestrace_record_model_data_policy_decision(UUID, UUID, UUID, TEXT, TEXT, TEXT, TEXT, TEXT, TEXT, TIMESTAMPTZ) RETURNS UUID`. Migration 0184 transfers this append-only deployment-evidence table to `vestrace_guarded_owner`, revokes runtime/public write authority, retains runtime `SELECT` only (explicitly revoking the generic handoff's unnecessary `REFERENCES`), and grants runtime exact `EXECUTE` on the guarded function; it must not add workspace ownership or RLS to this deliberately deployment-scoped table. Exact same-row replay returns the original id, while any unequal tuple for an existing id is SQLSTATE `23514`. Runtime-role coverage must invoke the legacy self-transacting `record` path as well as the transaction-bound path, so a stale raw `INSERT` cannot pass only under the SQLx migration owner. Independent runtime sessions race identical and conflicting same-id records: identical contenders both succeed with exactly one row, while an unequal contender is exact SQLSTATE `23514` and the original row remains unchanged. The 0176 fallback, real bootstrap allowlists, schema/ACL contract, runtime direct-DML refusal/execute proof, and P03 upgrade expected-owner set advance together. Provider dispatch must call both transaction-bound paths in its one permit transaction; governed Audit does not replace the dedicated pre-disclosure model-data-policy decision row, and the provider repository may not duplicate either repository's SQL. The atomicity matrix injects failures immediately before and after both calls and requires rollback to leave no policy-decision, Audit, idempotency, outbox, audit-mark or watermark row, while exact success creates each requested identity once. Scoped review requires the provider-dispatch production path to call only `record_in` and `commit_in`, never their legacy self-transacting wrappers.
- `0184_provider_dispatch_and_result_contract.sql` is the only forward schema mutation for this slice. It adds immutable `provider_dispatch_causes` with `(external_effect_id, workspace_id, model_request_evidence_id, model_request_evidence_check_id, cause_kind, run_id, step_id, model_binding_snapshot_id, qualification_job_id, qualification_target_binding_id, qualification_probe_ordinal)`. `cause_kind = 'run_step'` requires run/step/snapshot and forbids all qualification fields; `cause_kind = 'qualification_probe'` requires job/target/closed ordinal and forbids run/step/snapshot. Composite FKs bind effect, the exact MRE root/check pair, Run-step, snapshot, qualification job and target; guarded creation additionally requires exact equality to the immutable MRE root's effect, cause id, snapshot/target and qualification-probe ordinal node. The migration adds exact provider-result receipt witnessing and `artifact_revisions.storage_kind NOT NULL DEFAULT 'legacy_digest' CHECK (storage_kind IN ('legacy_digest','governed_material'))`, backfills legacy rows, and conditionally permits `content_hash`/`byte_size` to be null only for `governed_material`. A deferred bidirectional constraint requires every governed revision to acquire exactly one typed `artifact_revision_contents` row before commit, requires every such content row to reference a governed revision, and forbids any content row for a legacy revision. Migration 0176's automatic-test fallback, the real bootstrap allowlist, provider schema tests, runtime-role refusal/execute tests, and upgrade provisioning advance together. Runtime retains no direct admission-table DML and the guarded owner receives no Artifact/Run/model-execution/work-table DML.
- The exact admission functions are `vestrace_try_admit_provider_dispatch(UUID, UUID, UUID, UUID, UUID, UUID, UUID, UUID, TEXT, UUID, UUID, UUID, UUID, UUID, TEXT, INTEGER)` for `(admission_id, wait_id, concurrency_lease_id, workspace_id, connection_id, connection_revision_id, effect_id, MRE_id, cause_kind, run_id, step_id, snapshot_id, qualification_job_id, qualification_target_id, qualification_probe_ordinal, dispatch_ttl_seconds)`, returning `TABLE(decision TEXT, retry_after_seconds INTEGER, concurrency_lease_id UUID, wait_deadline_at TIMESTAMPTZ, dispatch_expires_at TIMESTAMPTZ)`; `vestrace_release_provider_dispatch(UUID, UUID, UUID) RETURNS UUID` for `(workspace_id, effect_id, receipt_id)`, returning the released lease UUID; and `vestrace_record_provider_throttle(UUID, UUID, UUID, UUID, INTEGER) RETURNS UUID` for `(observation_id, workspace_id, effect_id, receipt_id, retry_after_seconds)`, returning the observation UUID. The database uses `NOW()` for request, wait, admission, release and throttle observation times; `dispatch_ttl_seconds` is checked in the closed `1..=900` range and the concurrency-lease/dispatch expiry is derived once from database time, so runtime cannot time-shift accounting or pin a slot. A deferred constraint requires the committed `Dispatching` transition deadline to equal the admission-returned/concurrency-lease expiry. The guarded admission function itself acquires the exact permanent `ConnectionExecutionGuard`, locks the named MRE root/nodes and lower material rows in Task 9's canonical reconstruction order, revalidates every SQL-observable source/existence/Live/byte-frame/no-finalized-erasure predicate, then requires and pins the exact latest append-only `complete` check in `provider_dispatch_causes`; repository prechecks are not its database authority, while AEAD/AAD/padding validation remains process-only through the immediately preceding `reconstruct_current_in`. It then locks policy/state in canonical order and admits. All three are fixed-search-path guarded-owner functions with exact runtime execute grants and no dynamic SQL.
- `ProviderDispatchRepository::prepare_dispatch` takes `InstallationMutationPermit::Shared`, then ConnectionExecutionGuard, optional exact CredentialActivationGuard, exact snapshot/target/evidence rows, admission policy/window/lease rows, predecessor effect where applicable, and lower material rows.
- In that one transaction it re-evaluates live authorization and snapshot ceiling, calls only `reconstruct_current_in` while the binding/material guards are held, obtains one bounded zeroizing `EffectiveModelRequest`, and applies data policy and authorization to that same instance. For an in-memory permitted decision, the exact guarded admission entrypoint first serializes rolling-window/concurrency/throttle and persists the normalized effect cause/admission; only an admitted result is then persisted through the transaction-bound effect repository with the exact allowed authorization and `Authorized` transition required by credential-lease issuance; then the credential branch alone issues and consumes its exact lease; finally the effect repository appends `Dispatching` using the exact database-returned deadline, the mutation watermark advances, and the transaction commits. Admission conflict/throttle commits only its terminal admission/wait evidence, with no authorization, credential lease or `Dispatching`; exact replay may later continue only that same effect/cause tuple. It returns the committed dispatch authority plus that same request object; rollback zeroizes it and sends no bytes.
- Denial appends safe denial/Audit evidence but no `Authorized`, admission lease, credential lease, or `Dispatching`. No-auth creates no credential row or decrypt operation.
- Admission never sleeps while holding `ConnectionExecutionGuard`. It atomically reclaims expired unreleased concurrency leases before counting/selecting a declared slot. Saturation creates or exact-replays one durable `provider_admission_waits` row and returns typed pre-dispatch `Conflict` plus its deadline; a later explicit attempt either terminalizes that wait as `admitted` and creates the one admission/lease tuple, or as `timeout` and persists the terminal conflict decision. Active throttle returns the capped retry interval and persists only the terminal throttled admission. Exact same-identity replay is idempotent; any unequal tuple is SQLSTATE `23514` and maps to the typed admission conflict.
- Migration 0184 extends the closed wait terminal reasons from `admitted | timeout | cancelled` to `admitted | timeout | cancelled | throttled`. If throttle becomes active while an effect is waiting, its next explicit attempt atomically terminalizes that exact wait as `throttled` beside the terminal throttled admission; no pending wait survives any terminal admission decision. Independent-session coverage races throttle observation against a saturated wait retry.
- The network adapter is called only after commit and consumes the exact `EffectiveModelRequest` returned by `prepare_dispatch`; no adapter or service reconstructs/clones it. Receipt/reconciliation appends reuse the original effect id, snapshot and MRE.
- `ProviderResultFinalizer` handles only bounded validated chat content that must be retained. `ModelsList` and q1 qualification observations persist only their closed structural evidence and never retain a raw response body.
- Before encrypting retained result bytes, the finalizer reserves one fixed P02 `MaterialKeyCreationIntent` for the exact normalized effect/cause/output ordinal, creates the provisional vault key idempotently, records its receipt, and writes ciphertext only as the exact nonordinary `PreparedMaterialAttachment` with `ResultPrepared`. Every prepare, replay, witness, provider-owned recovery-bind and finalize path uses the common lock order `agent_runs -> run_steps -> external_effect_intents(effect_id) -> provider_result_preparations when present -> material_key_creation_intents`. A path may first read identities without row locks, but must then lock the exact active Run and `running` step, lock the effect, and re-read/lock/revalidate every lower row in that order. First prepare observes no preparation only after the parent locks, then locks the intent before inserting the preparation and captures the exact Run version. Independent-session prepare/replay-versus-witness/finalize tests reject `40P01`.
- Once a `result_prepared` row commits, migration 0184 guards `agent_runs` and `run_steps` against cancellation, failure, expiry, skip or any other incompatible terminalization while that exact pending preparation exists. Publication and Run-step success occur in the same finalization transaction, so the guard observes `published` before allowing the matching success/version/work tuple at commit. Prepare/recovery races against independent-session Run cancellation and failure prove either termination wins before `ResultPrepared` and preparation is refused, or `ResultPrepared` wins and incompatible termination is exact SQLSTATE `23514`; no un-abandonable result is stranded.
- Runtime calls only the exact forward-replaced provider-result prepare, receipt-witness and finalize entrypoints. Each is owned by `vestrace_guarded_owner`, has fixed search path/no dynamic SQL, and validates the normalized effect/run/step/MRE/intent/attachment/artifact/revision/model-execution/output tuple. `PUBLIC` is revoked and runtime receives only these exact execute grants; direct runtime invocation of `vestrace_prepare_result_material(UUID, UUID, BYTEA, BIGINT)` remains exact SQLSTATE `42501`.
- The definite provider receipt is inserted transaction-bound beside `ResultPrepared` and contains exactly one canonical evidence reference `provider_result_preparation:<lowercase-hyphenated-uuid>`. The witness entrypoint requires the same effect, `acknowledged` outcome, that exact one-element marker array, and its matching `receipt_recorded` lifecycle transition. Replaying the same `(preparation_id, receipt_id)` is idempotent; a different receipt, effect, marker or outcome is SQLSTATE `23514` and maps to provider-result conflict. It does not publish an ordinary content reference or Run success. Provider-result recovery—not the generic P02 reconciler—resumes the exact `ResultPrepared` identities, obtains and persists the provisional-key bind receipt, and then calls `ProviderResultRepository::finalize_in` without another network call. `finalize_in` uses one caller-owned runtime transaction: insert the preallocated governed Artifact/Revision and safe model-execution envelopes as runtime; compute the exact DEK-bound commitment and call guarded-owner/fixed-search-path `vestrace_finalize_provider_result(UUID, BYTEA)` to require both receipt witnesses, perform P02 Live promotion, and insert guarded `ArtifactRevisionContent`/publication; then append the owning Run-step success/reference/work continuation through transaction-bound Run persistence. An exact replay compares the supplied commitment and returns the original publication; a different commitment or tuple is SQLSTATE `23514` and maps to provider-result conflict. Deferred database constraints require all legs before commit. Any failure rolls back the Live promotion and every runtime-owned row together; the guarded owner receives no Artifact/Run/model-execution/work-table DML grant. `ResultPrepared` never abandons.
- Every definite, failed or unknown post-network receipt is inserted through the transaction-bound effect repository and releases its exact concurrency lease in the same caller-owned transaction. A 429 receipt has safe `response_class = 'http_429'` and atomically records the policy-capped throttle observation with that release. Receipt insertion, release and optional throttle are all-or-none; no release or throttle may be recorded for a foreign effect/receipt pair.
- `ArtifactRevisionContent` replaces the provider path's `ArtifactContent`/`StoredArtifact.content_hash` contract with opaque Artifact/Revision/ContentMaterial identities, erasure-bound commitment, `SizeClass`, and safe media class. The legacy artifact repository remains available for pre-P03 callers, but `GovernedProviderStepExecutor` cannot call `ArtifactRepository::store`, write `artifact_blobs`, expose `content_hash`, or expose an exact byte size.
- A crash before `ResultPrepared` leaves no useful result and follows the original post-`Dispatching` unknown/recovery rules without an automatic provider retry. A crash after `ResultPrepared` resumes bind/promotion on the same identities without another network call. Raw response bytes remain only in bounded zeroizing memory until encrypted or discarded.

- [ ] **Step 1: RED atomicity matrix**

```powershell
cargo test -p vestrace-infrastructure --test provider_dispatch_is_atomic -- --nocapture
cargo test -p vestrace-infrastructure --test provider_admission -- --nocapture
cargo test -p vestrace-infrastructure --test provider_result_binding -- --nocapture
```

Inject failure after each write boundary and assert all-or-none for the model-data-policy decision, admission record, concurrency lease, authorization, optional credential lease, `Dispatching`, governed Audit/idempotency/outbox, audit mark and mutation watermark. Inject immediately before and after transaction-bound policy `record_in` and governed `commit_in`; rollback must leave each of those rows absent, while exact success creates every requested identity once. Invoke the legacy policy `record` through the restricted runtime pool and prove it uses the same guarded function. Race independent runtime sessions on the same decision id: exact tuples are idempotent with one row, and unequal tuples return exact `23514` without mutation. Assert the provider counter remains zero for every pre-commit failure, the request buffer is zeroized on rollback, and scoped production review finds no provider-dispatch call to the legacy self-transacting `record` or `commit` wrappers.

**Task 10C corrective acceptance after independent implementation review (`VERDICT: REVISE`):**

- `ProviderDispatchRequest::credential` is data to validate, never routing authority. Immediately after acquiring the shared installation permit, the repository resolves the already-existing permanent `ConnectionExecutionGuard` for the exact workspace/Connection and locks it through `vestrace_ensure_connection_execution_guard` before any MRE reconstruction or lower-row lock. While that outer guard is held, it loads and locks the exact routing authority named by the cause: `ModelBindingSnapshot` for `run_step`, or `QualificationTargetBinding` for `qualification_probe`, together with its pinned `ConnectionRevision`. It refuses any workspace/Connection/revision/cause mismatch before admission. No missing guard or binding is synthesized by dispatch.
- The real restricted-runtime race exposed a Task 9 mechanism gap: direct `SELECT ... FOR SHARE` in `PgModelRequestEvidenceRepository::reconstruct_current_in` requires table mutation privilege that runtime intentionally does not have. Do not grant runtime `UPDATE` and do not weaken reconstruction to unlocked reads. The already-P03-scoped `crates/vestrace-infrastructure/src/postgres/model_request_evidence_repository.rs` is admitted to this corrective slice. Migration 0184 adds one exact guarded-owner, fixed-search-path, no-dynamic-SQL `vestrace_lock_model_request_evidence_for_reconstruction(UUID, UUID)` entrypoint. At the start of every `reconstruct_current_in`, before any direct source read, the repository invokes it in the caller transaction. The function validates workspace/root/cause structure and takes the canonical locks on the exact snapshot or qualification binding, evidence nodes, immutable revision sources, governed input materials in ascending material id, their intents in ascending intent id, and byte rows; it returns no source or plaintext data. Direct repository reads that follow use ordinary `SELECT` only while those transaction-held locks remain live. Runtime receives exact `EXECUTE` and retains `SELECT`-only table ACLs; `PUBLIC` receives none. An ownership audit must enumerate every row-locked relation and prove all are owned by `vestrace_guarded_owner` except the single legacy runtime-owned `external_effect_intents` source. For that append-only exception, migration 0184 first installs a `BEFORE UPDATE OR DELETE` immutability trigger returning exact SQLSTATE `23514`, then the 0176/bootstrap dependency helper grants `vestrace_guarded_owner` only column-level `UPDATE(id)` in addition to `SELECT`; it grants no table-level `UPDATE`, `INSERT`, or `DELETE`. A restricted-runtime regression must first prove `UPDATE(id)` is sufficient for the guarded `FOR SHARE`, then prove actual same-workspace `UPDATE` and `DELETE` of the intent still return exact `23514` with an unchanged row; runtime's own privilege set does not expand. The same corrected grant/trigger must make the pre-existing admission source lock executable. The 0176 fallback, real bootstrap allowlist, 0184 invocation/ownership, schema/column-ACL tests, runtime direct-DML refusal/execute proof, and 0183-to-0184 upgrade proof advance together. Existing Task 9 reconstruction, erasure-race, malformed-frame, zero-unwrap and real-vault tests must be rerun because this changes their locking mechanism, although no Task 9 data contract or global 120-path scope changes.
- Auth branch validation is exact. A pinned no-auth branch requires `credential = None` and `ConnectionAuthMode::None`. A pinned credential branch requires `Some` and exact equality with the binding's credential revision, slot and activation guard, the ConnectionRevision's slot and non-none auth mode, and the normalized destination authority derived from that revision's runtime URL. The ordinary credential lease authority must still reject a credential that has ceased to be current/usable after snapshot creation. Wrong branch, stale or different current credential, wrong auth mode, and wrong destination are pre-admission refusals with no effect/admission/lease/dispatch/governed row and zero vault unwrap.
- The outer guard must precede real reconstruction. An independent-pool production-composition race uses the real MRE repository and a shared governed input on one stable Connection; valid contenders serialize without `40P01`, mixed authority, or over-admission. `FixedEvidence` is not admissible evidence for this lock-order property.
- Model-data-policy and external authorization remain independent decisions. Cause identity must still match. An allowed authorization is invalid only when paired with an `Enforce` model-policy denial. An `Observe` denial plus allowed authorization persists the denied model-policy evidence and may reach `Prepared`; any external-authorization denial reaches `Denied` whether the model-policy verdict is allowed or denied. Tests cover Enforce-denied/authorization-allowed refusal, Observe-denied/authorization-allowed preparation, and model-policy-allowed/authorization-denied denial.
- The atomic matrix exercises a real credential-backed composition as well as no-auth. It covers `Before/AfterCredentialIssue` and `Before/AfterCredentialConsume` in addition to every existing boundary, asserts exact all-or-none rows, and asserts the expected unwrap count (zero before consume; exactly one once consume has crossed the vault boundary). The credential success case creates exactly one consumed exact-effect lease.
- The adapter sentinel is wired to the actual `Prepared` return seam: the test harness invokes a consuming adapter probe only after `prepare_dispatch` returns `Prepared`, proves one call on success, and proves zero calls for every pre-commit error. Merely allocating and rereading an unrelated counter is forbidden. There is still no network or LM Studio call in Task 10C.
- Rollback zeroization uses an observable allocation-lifetime probe, not `needs_drop`. The integration-test allocator records the exact reconstructed content allocation selected by the policy borrow, observes that same allocation at deallocation after an injected rollback, and asserts its tracked content bytes are zero before forwarding deallocation. The probe must be allocation-specific, non-logging, and must also retain the existing same-allocation policy/caller assertion; no freed memory is read by test code.
- Repository-level production-composition tests cover `Denied`, saturated admission `Conflict`, and active-throttle conflict. Denial commits only safe policy/authorization/Audit evidence; conflict and throttle commit only their exact terminal admission/wait evidence; none creates a credential lease, `Dispatching`, or adapter call. No test double may substitute for the PostgreSQL authority under test.

**Task 10C second corrective acceptance after final independent implementation review (`VERDICT: REVISE`):**

- Credential-backed `Prepared` must be executable without a second unwrap or any Task 11 bypass. Extend the existing scoped content-material codec module with one exported `CredentialMaterialCodec` rather than creating another store or an unscoped file. Its only writable profile is `credential_v2`: an authenticated AES-256-GCM frame with a fresh 96-bit nonce, explicit version/magic, bounded non-empty UTF-8 plaintext, zero padding from 4 KiB to the next power of two, and a fixed 64-KiB frame ceiling. AAD binds the exact profile, workspace, Connection, credential slot, CredentialRevision, MaterialKey, CredentialKeyCreationIntent and IntentNonce identities. The transient prepared-attachment id is deliberately excluded because Candidate finalization deletes that attachment; it remains an exact lifecycle-command identity, not cryptographic context. Wrong profile/AAD/intent/nonce, malformed/tampered/oversized frames, invalid padding, invalid UTF-8 and empty plaintext fail closed without returning credential bytes; `legacy_v1` is never dispatchable. The same codec is the only seam Task 11 may use to create `CredentialPrepared` bytes from operator input.
- The codec is not an HTTP dependency. Add an application-level `CredentialMaterialPreparer` port in the already-admitted `credential/activation.rs`. Its command consumes one non-cloneable zeroizing operator credential and names the exact reserved credential intent/prepared attachment plus typed workspace, Connection, slot, CredentialRevision, MaterialKey, nonce and `CredentialV2` AAD identities; its result exposes only opaque prepared identities/state, never ciphertext, plaintext or a DEK. The sole infrastructure implementation composes `CredentialMaterialCodec`, `MaterialKeyVault` and the existing credential-intent lifecycle to idempotently create/receipt the fixed provisional key, seal under the allocated revision identity, and persist the guarded `CredentialPrepared` frame. Task 11 CLI composition injects this application port into the HTTP state; the HTTP crate neither imports infrastructure nor chooses a frame/profile/AAD. A dependency contract keeps `vestrace-http` free of `vestrace-infrastructure`, and a real port-to-dispatch test proves that a frame created through the preparer opens only through the same exact lease/codec identities.
- Preparer replay is state-first and exact-tuple, not catch-and-convert conflict handling. Under the existing ConnectionExecutionGuard/CredentialActivationGuard chain, it first reads the persisted intent, revision, material, nonce and any still-live prepared attachment/material identity. `CredentialPrepared` or any later exact replay immediately zeroizes the newly supplied operator credential and returns the original persisted opaque intent/revision/material identity (plus the attachment identity only while it still exists), with zero `create_if_absent`, unwrap, nonce generation, seal or ciphertext-write calls. Any unequal workspace/Connection/slot/revision/material/intent/nonce or still-persisted attachment tuple fails unchanged. Only the exact pre-prepared state may cross the bounded vault/codec/persist sequence, and concurrent same-identity callers serialize on the existing guard chain so exactly one creates one frame; the loser re-runs state-first replay rather than resealing. Boundary-fault and independent-pool race tests require one durable frame, identical opaque results, zero extra vault/codec calls after the winning commit, no lock inversion and lawful resume after every pre-commit external-key boundary.
- `CredentialDispatchLeaseRepository::consume_for_dispatch` returns a non-cloneable zeroizing typed `ConnectionAuth`, not a borrowed DEK and not an identifier-only success. The PostgreSQL implementation first passes the existing guarded one-use/current/usable lease consume predicate, then loads the exact `credential_prepared_materials` frame and immutable revision/profile/AAD identities in that caller-owned transaction, validates the bounded frame before unwrap, and decrypts only inside the post-predicate vault callback. Any decode/decrypt/type/fault/commit failure rolls back lease consumption and zeroizes every plaintext allocation. The no-auth branch returns `ConnectionAuth::None` without a credential row, ciphertext read or unwrap. `ProviderDispatchOutcome::Prepared` owns the resulting auth beside the exact request and authority; the Task 11 adapter must consume that value exactly once and cannot resolve, unwrap or reconstruct it again.
- Credential tests use a real codec-produced encrypted sentinel under the counting vault key, never arbitrary bytes. The post-commit adapter harness consumes the exact returned `ConnectionAuth`, observes the sentinel once with the canonical auth variant, and proves zero calls for every refusal or rollback. Focused codec/lease/dispatch tests cover tamper, wrong AAD/profile, frame/plaintext bounds, invalid UTF-8, zero unwrap before the guarded consume boundary, exactly one unwrap on success, exact lease rollback on every later fault, and observable zeroization of the decrypted sentinel on all post-decrypt failure exits. Task 11's credential command test must later prove its persisted frame opens through this same dispatch path; no raw plaintext or DEK enters PostgreSQL, `Debug`, serialization, logs or errors.
- One canonical row-lock order applies to dispatch and standalone Task 9 reconstruction: shared installation permit -> existing permanent ConnectionExecutionGuard -> optional existing CredentialActivationGuard/slot chain -> MRE root -> snapshot or qualification target -> MRE nodes -> immutable sources -> sorted material/intents/bytes -> admission policy/state. `vestrace_lock_provider_dispatch_routing` may perform only an ordinary immutable preliminary lookup after the Connection guard to discover the pinned auth branch; it then acquires the already-existing credential chain without synthesis, invokes `vestrace_lock_model_request_evidence_for_reconstruction`, and only afterward re-reads/revalidates the exact effect/evidence/cause/routing/current-credential tuple through ordinary reads or re-entrant locks. It must not lock a snapshot, qualification target, revision or evidence row before the MRE root. The guarded admission path preserves the same root-first order and exact qualification ordinal check. Reconstruction may invoke the helper again re-entrantly in the same transaction.
- The forward 0184 replacement of credential lease consumption obeys that same outer-guard order: an ordinary immutable lease lookup may discover the exact slot, but no lease/slot/revision/intent/effect row is locked before the existing ConnectionExecutionGuard and CredentialActivationGuard/slot chain. It then locks and revalidates the exact lease/current credential/effect tuple before the one-use transition and post-predicate frame open. This replacement preserves the existing signature/SQLSTATE authority while preventing an independent transaction-bound consumer from holding a lease row and waiting inward on a guard already held by dispatch.
- A deterministic three-session PostgreSQL regression stages the former deadlock: an owner session holds the exact snapshot/target row, dispatch queues first, and standalone real-MRE reconstruction queues second while holding the evidence root; after the blocker releases, both fixed paths must complete in canonical order with valid outcomes and no `40P01`, mixed authority, leaked lease or over-admission. The test must observe the intended wait/held-lock state before release so a scheduler miss cannot vacuously pass. A temporary restoration of the old snapshot-before-root routing lock must produce `40P01`; restore the canonical order and rerun GREEN.
- Production-composition coverage includes both no-auth and credential-backed `QualificationProbe` success. Separate wrong job, wrong target, wrong closed ordinal, wrong auth branch and no-longer-current credential cases must fail before credential unwrap/adapter invocation and leave zero admission/effect/lease/`Dispatching`/governed rows. These tests use the real PostgreSQL routing, MRE, policy, admission and credential authorities; `FixedEvidence` and a run-step-only fixture are not substitutes.
- The allocation zeroization witness is isolated from the default parallel test runner. Tracking is disabled outside the one guarded zeroization case; only its unique exact request and credential sentinels may register. Use one atomic pointer per fixed compile-time sentinel length, or an equivalently atomic tokenized fixed-capacity registry that performs no allocation inside the allocator; the existing split process-global pointer/length pair is forbidden. The test serializes tracker activation, proves the policy and caller borrow the same request allocation, and observes that exact request and credential allocation zeroed at deallocation without reading freed memory.
- This correction additionally admits only the already-global-P03 paths needed for the missing mechanism: `crates/vestrace-application/src/credential/activation.rs`, `crates/vestrace-infrastructure/src/crypto/content_material_codec.rs`, `crates/vestrace-infrastructure/src/crypto/mod.rs`, `crates/vestrace-infrastructure/src/postgres/credential_dispatch_lease.rs`, `crates/vestrace-infrastructure/tests/credential_dispatch_lease.rs`, and `crates/vestrace-infrastructure/tests/credential_intent_lifecycle.rs`. They remain inside the exact 120-path P03 scope. Task 10D, Task 7 and Task 11 wiring remain unstarted; Task 10C still performs no network or LM Studio call.

**Task 10C third corrective acceptance after repeat independent implementation review (`VERDICT: REVISE`):**

- A `CredentialDispatchLease` returned to application code is a convenience projection, never post-SQL authority. After the guarded consume transition succeeds but before any prepared-frame read, validation or vault unwrap, the repository reloads the persisted lease row under the already-held lock and compares every immutable public authority field: workspace, Connection, effect, authorization, slot, CredentialRevision, activation guard, destination authority, auth mode, issue/expiry and lease identity. Any caller mutation rolls the transaction back with zero frame read/unwrap. The closed auth variant is selected only from the persisted row; caller-supplied mode is never used to construct `ConnectionAuth`. Valid-mode substitution, unsupported mode, destination, authorization and guard mutations each prove zero unwrap, unchanged one-use lease state after rollback, no dispatch rows and no adapter call. Decrypted UTF-8 remains in a zeroizing container through validation and direct transfer into `ProviderCredential`, so no error path owns an ordinary plaintext `String`.
- `PgCredentialMaterialPreparer` is a common P03 mutation and must acquire `InstallationMutationPermit::Shared` before the Connection/Credential guard chain. Its constructor receives the shared permit authority; `prepare` performs state-first inspection, vault/codec work, guarded lifecycle calls and commit through that permit's single caller-owned `UnitOfWork`, with no raw or nested transaction. A real independently held exclusive permit blocks preparation before vault activity. A generic permit-acquisition error is propagated unchanged with zero create/unwrap/seal/write calls; this slice does not claim that the current permit owns the separate fingerprint-readiness authority. Tests prove no intent/frame transition becomes visible across either refusal and that successful replay still uses the same permit/guard order.
- “CredentialPrepared or any later exact replay” includes every effective lifecycle state, not only the live branch. The opaque result state and PostgreSQL mapping cover `credential_prepared`, `bound`, `candidate`, `active`, `retired`, `revoked`, `credential_abandon_prepared`, `erasure_prepared`, `destroyed` and `abandoned`. Exact replay in each returns the persisted intent/revision/material identities and its actual safe state, immediately zeroizes the newly supplied operator credential, performs zero vault/codec/write calls and leaves rows unchanged even when ciphertext or the transient attachment has already been lawfully removed. Unequal identity tuples remain refusals and no terminal state is presented as usable.
- One serialized credential-backed rollback matrix tracks both the exact reconstructed request allocation and the exact decrypted auth allocation. It covers every post-decrypt boundary through `AfterCredentialConsume`, `Before/AfterDispatching`, `Before/AfterGovernedCommit`, plus a real database commit error after all application writes (for example an isolated test-only deferred-constraint failure). For every exit, both allocations are observed zero at deallocation, lease consumption and all policy/admission/effect/dispatch/governed rows roll back, and adapter calls remain zero. Pre-decrypt boundaries continue to prove request zeroization and zero auth allocation/unwrap. The tracker remains disabled for other parallel tests and uses the already-approved atomic allocation-specific registry.
- QualificationProbe auth-XOR coverage is branch-specific: a credential target with `credential = None` and a no-auth target with `credential = Some(...)` are separate real production-composition refusals in addition to wrong mode/job/target/ordinal/current revision. Each proves zero unwrap, admission/effect/lease/Dispatching/governed rows and adapter calls.
- Fresh and 0183-to-0184 upgrade ACL proofs assert the complete privilege tuple for both `credential_prepared_materials` and `credential_prepared_attachments`: only the exact required runtime `SELECT` columns/table reads are true; table/column `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`, `TRIGGER` and `REFERENCES` are false, `PUBLIC` has none, and guarded-owner/runtime grants remain no broader than the implementation needs. The stale-helper test removes and restores the same exact SELECT authority and proves no generic handoff reintroduces `REFERENCES`.
- These repairs use only the already-approved Task 10C paths and do not broaden the exact 120-path P03 scope. Task 10D, Task 7, Task 11, network/LM Studio calls, commits, pushes and deployment remain outside this slice.

**Task 10C fourth corrective acceptance after final independent implementation review (`VERDICT: REVISE`):**

- The credential-backed atomic rollback matrix proves the complete effect/governed transaction, not a subset of its rows. Its exact-count helper includes the current `external_effect_intents` row in addition to policy, admission, lease, authorization, dispatch, Audit, idempotency and outbox rows. Every credential fault case captures the request Audit identity and pre-attempt installation watermark, then asserts zero governed Audit marks, zero watermark-advance rows and an unchanged watermark. Caller-mutated lease refusal cases use the same expanded exact-count authority so a leaked intent or independently committed governed leg cannot pass.
- QualificationProbe auth-XOR tests wire their adapter assertion to the actual `Prepared` seam. Each test accepts only the expected typed `Policy` refusal. If production unexpectedly returns `Prepared`, the test routes that exact outcome through the real consuming adapter harness before failing; allocating and rereading an unrelated counter remains forbidden. The expected refusal path therefore proves zero actual adapter calls, while an erroneous success cannot evade the sentinel.
- The fresh-schema ACL proof asserts positive runtime column `SELECT` for the complete required `credential_prepared_materials` tuple: `workspace_id`, `intent_id`, `credential_revision_id` and `ciphertext`, and explicitly asserts negative runtime column `SELECT` for every other column, including `id` and `created_at`, alongside its existing negative table/column privilege matrix. The 0183-to-0184 upgrade proof retains the same complete tuple, and neither path treats upgrade-only coverage as fresh-deployment evidence.
- These proof repairs change only already-approved Task 10C test paths, preserve the exact 120-path P03 scope, and do not authorize Task 10D, Task 7, Task 11, network/LM Studio calls, commits, pushes or deployment.

- [ ] **Step 2: GREEN**

Implement transaction-bound effect methods and `PgProviderDispatchRepository`. Do not duplicate SQL transition rules outside `PgExternalEffectRepository`; factor shared private helpers that accept the transaction connection.

- [ ] **Step 3: Independent-session concurrency proof**

Race requests against one stable Connection from independent pools. Assert serialized 60-second accounting, bounded active leases, typed `PROVIDER_ADMISSION_CONFLICT`, durable 429 throttle, and no over-admission across Connection revisions.

- [ ] **Step 4: No retry after dispatch**

Force timeout/EOF after the loopback server accepts bytes. Assert one request, one effect, one MRE, terminal ambiguous evidence/recovery, and no automatic second request. A pre-dispatch definite failure may be re-requested only as a new explicitly accepted cause with fresh identities.

**Task 10D corrective acceptance after independent plan review (`VERDICT: REVISE`):**

- Generic P02 material resumption must not publish a provider result outside the provider finalization transaction. Migration 0184 adds an internal fixed-search-path deferred constraint trigger on `material_key_creation_intents` transitions for `owner_kind = 'provider_result'`: any committed transition to `Live` requires the exact provider-result preparation for that intent to be `published`. The provider finalizer may promote Bound to Live and publish in one transaction because the constraint observes final commit state; standalone `MaterialIntentResumption::resume` from Bound must fail exact SQLSTATE `23514`, roll back to the same Bound intent and `result_prepared` preparation, and leave every Artifact/Revision/content-map/model-execution/Run/work row absent. Fresh, runtime-role and 0183-to-0184 upgrade schema proofs include the trigger/function ownership and non-executable internal ACL. A real regression binds the provisional key, invokes the generic P02 resumption path, observes that refusal and unchanged state, then completes through provider-owned recovery with the same preallocated identities and no second provider request.
- Installing that trigger on the pre-P03 guarded table never transfers or temporarily hands back `material_key_creation_intents` ownership and never broadens a P02 object allowlist. The real bootstrap supplies one no-argument, hardcoded, fixed-search-path `SECURITY DEFINER` helper that can install only the exact named deferred constraint trigger on `public.material_key_creation_intents` after the exact `vestrace_validate_provider_result_material_live()` trigger function exists. It has no dynamic SQL, object/role/privilege parameter or alternate target; it revokes its own runtime `EXECUTE` in the same migration transaction after the exact trigger is installed, while `PUBLIC` never receives execution. An automatic admin-owned SQLx migration may directly create the byte-equivalent trigger without this deployment bridge; a runtime upgrade without either administrator ownership or the exact helper fails `42501`. Immediate fresh and 0183-to-0184 upgrade proofs assert that the P02 table owner is unchanged before/after, the existing Task 10 P03 handback arrays still contain only their declared P03 objects, and any still-present admin-owned helper is non-executable to runtime/PUBLIC. An explicit post-0184 rerun of the idempotent administrative bootstrap detects the recorded successful migration, physically removes the disabled helper, and proves it is neither recreated nor re-granted on later reruns.
- The governed `execute_effective` adapter boundary returns a distinct owned, bounded, non-`Clone`, non-serializable chat result whose retained content is one `Zeroizing<String>` and whose remaining output is only closed safe structural evidence and `Known | Unknown` usage. It must not return or retain `ChatCompletionsResponse`, `ChatCompletionsOutput`, `ChatSseEvent`, `serde_json::Value`, raw SSE events or a raw body. The Task 8 compatibility `chat_completions` DTO/method may remain only for its existing nongoverned callers and tests; it is not admissible in provider dispatch, q1 or provider-result finalization.
- The governed JSON and SSE readers immediately bounded-copy each immutable/shared Reqwest chunk into adapter-owned zeroizing storage, retain or log no transport chunk, and extract the single bounded UTF-8 chat content plus closed evidence without leaving an ordinary content copy, parsed JSON tree, SSE event collection or parser-owned scratch allocation alive. Every application-owned mutable body/content/parser allocation after receipt from Reqwest is zeroized on success, malformed input, limit refusal, timeout and every late provider-result fault; ephemeral third-party Reqwest/Hyper/kernel transport buffers that the application cannot mutably own are explicitly outside this zeroization witness. The finalizer consumes the same zeroizing content directly into the shared framed material codec without converting it through an ordinary `String` or byte vector. Allocation-specific, serialized probes cover the exact application-owned JSON/SSE body, content and parser allocations on success, parse/limit refusal, post-response pre-encryption failure, every post-encryption pre-commit boundary and successful ownership transfer into the prepared frame. Safe Debug/errors/Audit/effect receipts contain neither output bytes, raw body, digest nor exact plaintext length.
- The infrastructure crate's `#![forbid(unsafe_code)]` remains unchanged. Allocation probes use a safe observer seam compiled only under `cfg(debug_assertions)` and marked `#[doc(hidden)]`; ordinary constructors install a passive no-op fixed metadata sink, while an explicitly named debug-only constructor admits only the equivalent passive integration-test sink, never an arbitrary callback that can panic, block or influence control flow. The sink receives only a closed buffer-kind enum, pointer address and initialized length/capacity metadata at allocation registration; it receives no byte borrow/value, cannot affect parsing or dispatch, performs no logging, and is absent from release builds. The integration-test crate alone owns its unsafe `GlobalAlloc` witness and fixed atomic registry, checking only the initialized range of the exact registered allocation immediately before deallocation. A release check plus source/API contract proves the observer seam is excluded outside debug builds. Product code does not weaken or locally override the unsafe-code prohibition.
- This correction explicitly admits the already-global-P03 paths `crates/vestrace-application/src/providers/ports.rs`, `crates/vestrace-infrastructure/src/providers/openai_compatible.rs`, `crates/vestrace-infrastructure/tests/openai_transport_security.rs` and `crates/vestrace-infrastructure/tests/openai_auth_modes.rs` to Task 10D. The operator-authorized 2026-08-31 scope amendment expands the exact global P03 scope from 120 to 121 paths by adding only `crates/vestrace-domain/src/external_effects.rs`, and within that captured-baseline file authorizes only the 772-byte `ExternalEffectReceipt::provider_result_acknowledged` constructor needed to create the preallocated acknowledged receipt identity with canonical `provider_result_preparation:<uuid>` evidence. Removing that exact constructor must reproduce the captured 50,581-byte file and SHA-256 `ed0a92fb995cbcf302b5a34e5e77457b9bdc4a4454a01cf07666452d4506d787`; any other byte-level divergence is a stop-and-report condition. Task 7 and Task 11 wiring, non-loopback network/LM Studio calls, commits, pushes and deployment remain outside this slice.

- [ ] **Step 5: RED then GREEN result-material crash matrix**

Crash before provisional-key creation, after vault create before receipt, after receipt before `ResultPrepared`, after `ResultPrepared`, after bind, after structural runtime-envelope insertion, after Live promotion, and before Run success. Assert no raw response is durable, no ordinary reference or success is visible before promotion, every external key has a lawful terminal path, and every post-`ResultPrepared` recovery completes the same preallocated identities with a loopback request count of one. Replaying `finalize_in` returns the original publication and creates no second Artifact revision/model execution/Run event/work item. Temporarily bypass the `ResultPrepared` requirement or one deferred finalization leg and observe the early/partial-visibility assertion fail; restore it and re-run GREEN.

- [ ] **Step 6: Scoped diff review**

Confirm every network call is textually downstream of the committed `Dispatching` result and no provider adapter owns retry, admission, authorization, credential or effect state. Search for every `EffectiveModelRequest` constructor/renderer and require one production reconstruction path plus the loopback-only observer. Search Run events, effect receipts and provider evidence for raw response fields; retained bytes may exist only as P02 ciphertext behind a promoted `Live` typed reference. Search the production model-step path for `ArtifactRepository::store`, `artifact_blobs`, `content_hash`, and exact byte sizes and require zero matches.

---

### Task 11: Wire provider execution, governed HTTP commands, qualification worker, and recovery

**Files:**
- Modify: `crates/vestrace-application/src/run/model_step.rs`
- Modify: `crates/vestrace-application/src/run/handlers/execute_step.rs`
- Modify: `crates/vestrace-application/src/run/mod.rs`
- Modify: `crates/vestrace-application/tests/execute_step.rs`
- Modify: `crates/vestrace-http/src/api/connections.rs`
- Modify: `crates/vestrace-http/src/api/artifacts.rs`
- Modify: `crates/vestrace-http/src/api/models.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs`
- Modify: `crates/vestrace-http/src/route_inventory.rs`
- Modify: `crates/vestrace-http/src/lib.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Modify: `crates/vestrace-cli/src/commands/schema.rs`
- Modify: `crates/vestrace-infrastructure/src/config.rs`
- Create: `migrations/0186_provider_execution_wiring.sql`
- Modify: `apps/console/src/sdk/client.ts`
- Create: `crates/vestrace-http/tests/provider_routes.rs`
- Create: `crates/vestrace-cli/tests/provider_runtime_wiring.rs`
- Create: `crates/vestrace-cli/tests/provider_openapi_contract.rs`
- Create: `apps/console/tests/providerClientContract.test.mjs`

**Interfaces:**
- Replace `ProviderStepModelExecutor`'s name/config factory with `GovernedProviderStepExecutor`, which loads the Run's already-pinned snapshot and MRE cause, asks `ProviderDispatchService` to commit the shared pre-dispatch transaction, invokes the hardened adapter once, and gives bounded validated chat content to `ProviderResultFinalizer`. Only its P02-backed bind/promote finalizer may publish the typed result and Run-step success.
- Worker qualification processing resumes durable `Requested`/`Running` jobs and shared ambiguous effects. It never creates a new job/effect after `Dispatching` without an explicit acknowledged-successor command.
- Runtime `config.model` may seed a non-secret LMStudioLocal configuration candidate only; it cannot act as dispatch routing, qualification evidence, or a credential source. Production worker startup refuses any legacy config-only model execution path.
- Exact governed routes added to inventory and mounted structurally:

```text
POST /v1/connections                                                WorkspaceAdmin Critical
POST /v1/connections/{id}/revisions                                 WorkspaceAdmin Critical
POST /v1/connections/{id}/qualifications                            ProviderWrite   High
POST /v1/connections/{id}/credentials                               WorkspaceAdmin Critical
POST /v1/connections/{id}/credentials/{revision_id}/abandon         WorkspaceAdmin Critical
POST /v1/connections/{id}/credentials/{revision_id}/activate        WorkspaceAdmin Critical
POST /v1/connections/{id}/credentials/{revision_id}/revoke          WorkspaceAdmin Critical
POST /v1/models/{id}/revisions                                      ModelWrite      Medium
POST /v1/models/{id}/qualifications                                 ModelWrite      High
```

- Existing `GET /v1/connections`, `GET /v1/models` and `GET /v1/providers` become safe projections over guarded truth. Existing `POST /v1/providers` is retained only as a typed refusal `legacy_provider_registry_retired`; it cannot create an inert parallel provider authority. Existing `POST /v1/models` creates the stable identity plus first immutable revision and requires an exact ConnectionRevision.
- Every mutation includes request id idempotency and uses P02 governed mutation/Audit authority. Response DTOs contain opaque ids, state, safe blockers and qualification metadata, never secrets or raw provider bodies.
- The CLI OpenAPI document describes every route above plus the changed Connection, Model, qualification, provider-refusal, and material-backed Artifact DTOs. The console SDK mirrors those exact transport types/methods but adds no component, route, navigation, or browser workflow; those remain P06.
- Provider-created Artifact responses expose only opaque revision/material ids, erasure-bound commitment, `SizeClass`, and safe media class. Legacy digest/size fields remain optional only for legacy revisions and are never populated from a new provider result.

- [ ] **Step 1: RED route and wiring tests**

```powershell
cargo test -p vestrace-http --test provider_routes -- --nocapture
cargo test -p vestrace-cli --test provider_runtime_wiring -- --nocapture
cargo test -p vestrace-cli --test provider_openapi_contract -- --nocapture
node --test apps/console/tests/providerClientContract.test.mjs
```

Assert every new route is present in `ROUTE_INVENTORY`, denied without exact grants, and cannot be mounted without its descriptor. Derive the expected HTTP method/path set from `ROUTE_INVENTORY` and assert the OpenAPI and SDK contract cover the P03 subset with matching request/response shapes. Assert server and worker construct the same repository/adapter/effect authorities and neither calls `store.migrate()`.

- [ ] **Step 2: GREEN**

Implement HTTP handlers as thin typed command adapters and wire the concrete repositories. Replace the model-step test fixture's `content_hash` outcome with the opaque material-backed result reference and keep all unrelated execution assertions unchanged. Do not modify the three protected dirty API files; model binding is inserted inside `run_command_committer` from Task 6.

- [ ] **Step 3: Governed rollback**

For Connection create/revise, Model create/revise, qualification request, Candidate abandon/activate/revoke, inject mutation-side and Audit-side failure. Assert HTTP returns refusal/error and PostgreSQL contains neither a partial mutation nor orphan Audit/idempotency/outbox.

- [ ] **Step 4: Legacy bypass refusal**

Assert a `models`/`providers` row inserted through legacy repository or raw SQL cannot be selected by the worker and that `SecretBackedProviderFactory` is absent from production composition.

- [ ] **Step 5: Scoped diff review**

Search for `model_name` lookup at dispatch, `config.model` routing, direct `.generate(` from the Run worker, production `SecretBackedProviderFactory`, provider-result `content_hash`, and provider-result writes to `artifact_blobs`. All must be absent from the execution path. Compare Axum inventory, OpenAPI and SDK route/DTO sets; any drift is a failure, not a P06 deferral.

**Task 11 corrective acceptance after independent plan review (`VERDICT: REVISE`):**

- Task 11 is implemented only through the new forward migration `migrations/0186_provider_execution_wiring.sql`; migrations 0172 through 0185 remain historical authority and are not rewritten. Migration 0186 creates one immutable, discoverable Run-step execution attempt keyed uniquely by `(workspace_id, run_id, step_id)` with fixed identities for its governed input material lifecycle, external effect and MRE. Replay with different identities is refused; concurrent workers converge on the same tuple. Recovery classifies the durable phase exactly: pre-dispatch resumes the same effect/MRE, admitted but not `Dispatching` may repeat admission only with that same effect, expired `Dispatching` without receipt/result adopts Unknown and never calls the adapter again, `ResultPrepared` recovers by the original cause/effect and finalizes, and `Published` is an idempotent no-op.
- Run input is accepted as one non-`Clone`, bounded, zeroizing value and is encrypted through the existing material vault and content codec. The execution-attempt reservation fixes the material intent/key/reference identities before any vault call; restart never creates another material. Only the opaque governed material node enters the Complete MRE. Objective plaintext is absent from PostgreSQL, DTOs, Debug, logs, errors, effect receipts and Audit. The existing MRE/provider-result/codec/vault paths own this mechanism; no raw request or parallel input-material authority is permitted.
- Shared dispatch loads the Run's already-pinned ModelBindingSnapshot and the exact immutable ConnectionRevision in the dispatch transaction, revalidates the current qualification/auth branch, and returns one closed adapter target `{ConnectionKind, runtime_base_url}` together with the reconstructed request and owned zeroizing auth. Neither `config.model`, stable `models.model_name`, a mutable head nor the legacy provider registry may participate in execution routing.
- A successful Run provider call does not use the failure/q1 `complete_post_network` sequence. An additive provider-result `prepare_after_dispatch` authority creates the ResultPrepared tuple, canonical acknowledged receipt/witness and dispatch-lease release in one caller-owned transaction under the original effect/cause. Provider-result finalization alone promotes the material and publishes the single Run-step success plus continuation. The handler emits neither an early `AdvanceRun` nor a legacy `complete_step` after provider finalization. Failure, Unknown and optional throttle continue through the shared non-success completion/recovery authority.
- Candidate abandon extends only the already-admitted credential activation application/PG ports and uses the existing guarded prepare-candidate-abandon authority followed by vault erasure receipt and guarded finalize. HTTP replay discovers and resumes the same lifecycle; Candidate is not terminal before erasure/finalize. Every Connection, Model, qualification and credential mutation atomically commits Request-Id hash, governed Audit/outbox and its first durable lifecycle transition. Same key/same hash returns the original response, same key/different hash conflicts, and any mutation-side or Audit-side fault leaves no lifecycle row or orphan evidence.
- Safe Connection/Model/Provider projections read guarded revision/head/qualification truth through application ports, never direct HTTP SQL. Mixed legacy/provider Artifact projection treats provider `content_hash` and exact byte size as absent and exposes only opaque artifact/revision/material ids, erasure-bound commitment, `SizeClass` and safe media class; provider responses never read or populate `artifact_blobs` or legacy digest/size fields. Legacy `POST /providers` is an exact typed refusal and legacy rows are not executable.
- `crates/vestrace-http/src/router.rs` is admitted solely so `AppState` and its builders/accessors can inject the governed revision, qualification, credential and projection authorities. Route inventory, Axum mount, authorization descriptors, OpenAPI and SDK are one exact method/path/DTO set. Every mutation route proves exact grant and Request-Id replay/conflict/rollback behavior; thin HTTP adapters do not own PostgreSQL or vault logic.
- Production composition uses explicit distinct configuration for a writable material-vault root and read-only bootstrap-secret root. Startup refuses missing or overlapping roots and has no embedded bootstrap key. Compose mounts both with the exact modes. `config.model` may only seed a no-secret `LMStudioLocal` candidate; server/worker fail closed on config-only routing, the legacy `SecretBackedProviderFactory`, absent governed authorities, or nested migration. Authentication construction retains no ordinary `String` staging/cache or sensitive-header duplicate, consumes the credential once, bounds its lifetime through the single adapter call and zeroizes the source buffer on success/refusal/error.
- Focused acceptance includes deterministic concurrent Run-attempt uniqueness, each recovery phase, no adapter retry after `Dispatching`, same-identity `ResultPrepared` recovery, exactly one Run success/continuation, provider-result receipt-plus-release atomicity, governed-input vault crash/replay, HTTP mutation/Audit rollback, Candidate-abandon recovery, safe mixed Artifact projection, exact inventory/OpenAPI/SDK parity, auth-allocation lifetime and startup vault-root refusal. Task 12 may extend the fault matrix but cannot supply a missing Task 11 production mechanism.
- The operator-authorized scope amendment expands global P03 scope from 123 to 125 paths by adding only `crates/vestrace-http/src/router.rs` and `migrations/0186_provider_execution_wiring.sql`. Every other corrective file named above is already in global P03 scope. No Task 12 exhaustive fault work, P04/P06 behavior, non-loopback network/LM Studio call, baseline recapture, commit, push or deployment is authorized.

---

### Task 12: PostgreSQL refusal, recovery-fault, and branch-isolation qualification

**Files:**
- Create: `crates/vestrace-infrastructure/tests/provider_runtime_role_refusals.rs`
- Create: `crates/vestrace-infrastructure/tests/provider_effect_recovery.rs`
- Create: `crates/vestrace-infrastructure/tests/provider_branch_isolation.rs`
- Modify only if required: `crates/vestrace-fault-scenario/src/child.rs`
- Modify only if required: `crates/vestrace-fault-scenario/src/report.rs`
- Create: `tests/provider_effect_fault_scenario_e2e.rs`

**Interfaces:**
- Refusal tests connect as runtime role to the same per-test database derived from `pool.connect_options()` and assert exact `42501`, never merely `is_err()`.
- Provider fault scenarios reuse the existing five external-effect boundaries in `EffectFaultPoint::required_points()` unchanged. No new enum and no forked provider fault harness.
- Each fault scenario asserts exact surviving intent/authorization/owner/deadline/receipt/reconciliation/job/snapshot/MRE state and the number of loopback requests.

- [ ] **Step 1: RED runtime-role coverage**

```powershell
cargo test -p vestrace-infrastructure --test provider_runtime_role_refusals -- --nocapture
```

Before grants/ownership are correct, absent objects (`42P01`) and wrong ACLs must fail exact-`42501` assertions.

- [ ] **Step 2: GREEN runtime-role coverage**

Assert INSERT/UPDATE/DELETE refusal for every P03 guarded table and successful invocation of each intentionally runtime-callable guarded function. Assert internal helper functions remain non-executable. Specifically assert exact `42501` for the generic P02 `vestrace_prepare_result_material` and superseded Candidate-only credential erasure function, successful valid-tuple invocation of the narrow P03 result prepare/finalize functions, and exact runtime execute-set equality with migration-declared grants.

- [ ] **Step 3: RED then GREEN recovery matrix**

```powershell
cargo test -p vestrace-infrastructure --test provider_effect_recovery -- --nocapture
cargo test -p vestrace-infrastructure --test provider_branch_isolation -- --nocapture
cargo test --test provider_effect_fault_scenario_e2e -- --nocapture
```

Cover before-intent, after-intent-before-authorization, after-authorization-before-dispatch, after-dispatch-before-receipt, after provisional result-key create/receipt, after `ResultPrepared`, after bind, and after receipt-before-reconciliation. The post-dispatch boundaries must never call the provider twice. No-auth rows must have no credential artifacts; two credential-backed Connections with identical wire model ids must never cross-use slots, headers, leases or qualifications. `ResultPrepared` recovery must finish the same retained result without publishing early success.

- [ ] **Step 4: Proof by breaking**

Temporarily remove the Connection guard lock in the test migration/function and observe the acceptance/rotation race admit a mixed tuple. Temporarily make the no-auth dispatcher accept a credential field and observe branch-isolation fail. Temporarily make recovery create a fresh effect after Dispatching and observe the request-count assertion fail. Restore each mechanism and re-run GREEN.

- [ ] **Step 5: Scoped diff review**

Confirm fault point names/count remain unchanged, every refusal names its observed SQLSTATE, and no in-memory double is cited for a database or crash-boundary invariant.

---

### Task 13: Integrated P03 verification, truthful evidence, and independent review

**Files:**
- Create: `docs/development-evidence/v1-g0-03-provider-execution-foundation.md`
- Inspect only: every P03 path and repository-wide status.

**Interfaces:**
- Evidence records source revision, preflight digest, exact scoped file list, commands/exit codes/test counts, every observed SQLSTATE, loopback request counts, fault-boundary results, external-corpus decision, and explicit non-claims.

- [ ] **Step 1: Fresh focused and workspace gates**

```powershell
$env:DATABASE_URL = "postgres://test:test@localhost:55432/vestrace_test"
$env:VESTRACE_RUNTIME_DATABASE_URL = "postgres://vestrace:runtime-local-development-only@localhost:55432/vestrace_test"
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-03-preflight.json --scope p03-scope.mjs
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs
node scripts/protocol-lock.mjs --check .
node scripts/verify-p01-text-hygiene.mjs --check .
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
node --test apps/console/tests/providerClientContract.test.mjs
npm --prefix apps/console run typecheck
cargo test --test compose_smoke -- --ignored --test-threads=1
git -c safe.directory=E:/Soft/vestrace diff --check
```

Every command must exit `0`. The Compose smoke is required because P03 changes the administrative provisioning-before-runtime-migration path; if Docker is unavailable it is `BLOCKED`, never pass, and operator handoff cannot claim deployment qualification until an authorized environment records the real result. LM Studio, remote APIs and browser evidence are not part of P03 and must not be substituted for loopback/PostgreSQL evidence.

- [ ] **Step 2: Repository and authority hygiene**

Compare complete porcelain status with the P03 preflight. Recompute protected-authority and protected dirty-file hashes. Confirm q1 manifest/fixture and P01/P02 authority bytes are unchanged.

- [ ] **Step 3: Persist truthful scoped evidence**

State explicitly:

- P03 established immutable Connection/Model revisions, auth-binding XOR, q1 qualification, ordinary credential activation/rotation boundary, sole ModelBindingSnapshot, MRE, hardened provider transport, admission and shared effect dispatch/recovery;
- the superseded P02 Candidate-only credential-erasure runtime entrypoint is closed, and the P03 Retired/Revoked versus Candidate-abandon paths are distinct;
- provider chat results use the narrow P03 ResultPrepared wrapper and atomic P02 bind/Live/typed-Artifact/Run-success finalizer; the generic prepare function remains runtime-denied and the legacy artifact hash/blob path is not used for provider output;
- the Axum route inventory, CLI OpenAPI and console SDK transport contract were cut over together, without adding a console screen or claiming P06;
- fresh and existing-P02 database provisioning both ran the real administrative bootstrap before runtime migration, and the Compose smoke result is recorded as executed evidence rather than inferred from SQLx tests;
- q1 ordinal 90 was qualified, but P03 created no production EmbeddingJob, embedding transition, corpus generation, carry, barrier or retrieval fence;
- rotation with live embedding dependencies remains a typed refusal until P04 supplies exact transition evidence;
- P03 implemented no restore/backup/TargetActivationPlan, console workflow, real Agent publication, AG-UI/A2A change, LM Studio or independent remote-provider qualification;
- loopback success is semantic development evidence, not real-provider or release evidence;
- P02 outstanding pre-existing-table ownership and fingerprint backup/witness obligations remain P05;
- G0 and v1.0 remain incomplete.

- [ ] **Step 4: Independent adversarial review**

An independent reviewer traces every acceptance criterion to live code, migration ownership/ACLs, exact q1 manifest fields, real PostgreSQL evidence and fresh command output. The reviewer must specifically attack auth XOR, snapshot-at-acceptance, MRE non-duplication, transport DNS/peer policy, atomic pre-dispatch transaction, no-retry-after-Dispatching, and P03/P04/P05 boundaries, then return material findings plus `VERDICT: APPROVE` or `VERDICT: REVISE`.

Correct every material finding under the frozen scope amendment rule, repeat focused verification, and repeat independent review.

- [ ] **Step 5: Operator handoff**

Report the scoped outcome and the remaining package count. Do not start P04 or P05 until P03 is operator-accepted. Do not call the result G0-complete or v1.0-ready.
