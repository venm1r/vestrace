# Vestrace v1 G0/P04 Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every reopened P04 obligation with canonical encrypted embedding projections, disposable guarded in-memory generations, production workers, atomic transitions, fenced retrieval results, staged rotation, erasure propagation, and resumable legacy adoption.

**Architecture:** PostgreSQL remains the only durable authority for jobs, projections, generations, transitions, fences, results, erasure, and adoption. Workers build exact flat cosine indexes from encrypted `Live` vector materials into bounded zeroizing memory; every query revalidates the persisted generation guard, and no ordinary path reads or writes legacy pgvector rows.

**Tech Stack:** Rust 1.85.0, Tokio, SQLx 0.8, PostgreSQL/pgvector for legacy upgrade input only, `zeroize`, Axum HTTP, MCP, Node scope tests, Docker-hosted PostgreSQL fault fixtures.

**Spec:** `docs/superpowers/specs/2026-09-08-vestrace-v1-g0-04-completion-design.md`

## Global Constraints

- Keep all 126 existing migration files through `0196_retired_credential_erasure.sql` byte-identical; all schema changes are forward migrations `0197` through `0204`.
- Use plain `cargo`; `rust-toolchain.toml` pins Rust 1.85.0. Do not use `cargo +stable` for mandatory gates.
- Preserve `InstallationMutationPermit -> ConnectionExecutionGuard -> credential guards in canonical slot order -> space/generation guards -> material guards`.
- Never hold a long PostgreSQL transaction across a provider call or index build.
- PostgreSQL, provider evidence, Audit, logs, tombstones, and supported managed backups contain no plaintext vector or stable vector digest.
- Query and index buffers are bounded, do not derive `Debug` or serialization, and zeroize on drop.
- Claims distribute work only; they never authorize dispatch, publication, satisfaction, activation, or erasure.
- No automatic provider retry occurs after `Dispatching`; an acknowledged successor always receives fresh job, snapshot, effect, evidence, nonce, and output identities.
- Every SQL table is FORCE RLS and guarded-owner controlled; runtime roles receive only narrow SELECT/REFERENCES/EXECUTE grants.
- No task may edit a path absent from the Task 1 preflight and `scripts/p04-scope.mjs`; stop for a scope amendment instead.
- Run PostgreSQL integration and mutation suites serially. Record real exits, counts, hashes, owners, ACLs, and restoration evidence.

## File Map

**New migrations**

- `migrations/0197_embedding_canonical_generations.sql` — full space keys, canonical generation metadata/members, active-space head, legacy quarantine.
- `migrations/0198_embedding_index_builds.sql` — corpus-change events, build attempts, claims, guarded capture/publication/local-load observations.
- `migrations/0199_embedding_executor_work.sql` — embedding work claims and delivery/rebuild result acceptance, including forward replacement of delivery-only 0195 validators.
- `migrations/0200_embedding_transition_execution.sql` — exact recipes, dependencies, batches, physical attempts, satisfactions, and completeness.
- `migrations/0201_embedding_transition_activation.sql` — qualification staging, active-space CAS, staged credential rotation, activation receipts.
- `migrations/0202_embedding_retrieval_results.sql` — attempt-scoped fences, terminal result/change transactions, and one-successor retry.
- `migrations/0203_embedding_erasure_propagation.sql` — source/vector invalidation and historical post-erasure publication validation.
- `migrations/0204_embedding_legacy_adoption.sql` — adoption plans, recomputation mapping, cutover, legacy tombstones, retirement gate.

**New Rust modules**

- `crates/vestrace-domain/src/embedding/index.rs` — generation/build/change closed sets and snapshot values.
- `crates/vestrace-domain/src/embedding/retrieval.rs` — fence, generation-change, and retry vocabulary.
- `crates/vestrace-domain/src/embedding/adoption.rs` — legacy-adoption lifecycle and blockers.
- `crates/vestrace-application/src/embedding/index.rs` — index repository ports and build/load service.
- `crates/vestrace-application/src/embedding/executor.rs` — one production embedding job executor.
- `crates/vestrace-application/src/embedding/work.rs` — bounded work-claim vocabulary and repository.
- `crates/vestrace-application/src/embedding/transition_coordinator.rs` — batch observation, completeness, stale catch-up, activation coordination.
- `crates/vestrace-application/src/embedding/retrieval.rs` — retrieval job client, finalization, and authorized retry.
- `crates/vestrace-application/src/embedding/erasure.rs` — post-commit invalidation and source/vector reconciliation port.
- `crates/vestrace-application/src/embedding/adoption.rs` — operator adoption service.
- `crates/vestrace-infrastructure/src/embedding_index/mod.rs` — index exports.
- `crates/vestrace-infrastructure/src/embedding_index/flat.rs` — bounded exact cosine index.
- `crates/vestrace-infrastructure/src/embedding_index/registry.rs` — process-local immutable generation registry.
- `crates/vestrace-infrastructure/src/postgres/embedding_index_repository.rs` — guarded build/load PostgreSQL adapter.
- `crates/vestrace-infrastructure/src/postgres/embedding_work_repository.rs` — job work claims.
- `crates/vestrace-infrastructure/src/postgres/embedding_retrieval_repository.rs` — fence/result/retry adapter.
- `crates/vestrace-infrastructure/src/postgres/embedding_erasure_repository.rs` — projection invalidation adapter.
- `crates/vestrace-infrastructure/src/postgres/embedding_adoption_repository.rs` — adoption/cutover adapter.

**New focused integration tests**

- `crates/vestrace-infrastructure/tests/embedding_canonical_generations.rs`
- `crates/vestrace-infrastructure/tests/embedding_index_builds.rs`
- `crates/vestrace-infrastructure/tests/embedding_executor.rs`
- `crates/vestrace-infrastructure/tests/embedding_transition_activation.rs`
- `crates/vestrace-infrastructure/tests/embedding_retrieval_results.rs`
- `crates/vestrace-infrastructure/tests/embedding_erasure_propagation.rs`
- `crates/vestrace-infrastructure/tests/embedding_legacy_adoption.rs`
- `crates/vestrace-cli/tests/embedding_worker_once.rs`
- `crates/vestrace-http/tests/embedding_retrieval_routes.rs`
- `crates/vestrace-mcp/tests/embedding_retrieval.rs`

Existing P04 domain/application/repository/CLI/HTTP/MCP/fault files change only where the tasks below name them.

---

### Task 1: Freeze the completion scope and contract baseline

**Files:**
- Create: `docs/development-evidence/v1-g0-04c-preflight.json`
- Modify: `scripts/p04-scope.mjs`
- Modify: `tests/p04_scope.test.mjs`
- Add: `docs/superpowers/specs/2026-09-08-vestrace-v1-g0-04-completion-design.md`
- Add: `docs/superpowers/plans/2026-09-08-vestrace-v1-g0-04-completion.md`

**Interfaces:**
- Consumes: Git HEAD `a6014633b50a15a35e63600236edf6552580290f`, the two approved untracked contract documents, and the established P04r preflight schema.
- Produces: one exact `change_scope_paths` list, unchanged protected paths, an exact two-path contract-baseline porcelain capture, HEAD, and SHA-256 inventory for migrations `0001..0196`.

- [ ] **Step 1: Write the failing scope assertions**

Before editing the test, capture the exact two-path `git status --porcelain=v1 -z` byte stream and its SHA-256 in temporary variables/files outside the repository. Add literal assertions for the new plan/design/preflight, migrations `0197..0204`, new Rust modules, and focused test targets. Replace the old 127-path allowlist with exactly the 111 unique paths named by the Task 1-14 Files blocks, remove all 65 old-only permissions, and retain the unchanged 23-path protected list.

```javascript
assert.equal(changeScopePaths.includes('migrations/0197_embedding_canonical_generations.sql'), true);
assert.equal(changeScopePaths.includes('migrations/0204_embedding_legacy_adoption.sql'), true);
assert.equal(changeScopePaths.includes('docs/superpowers/plans/2026-09-08-vestrace-v1-g0-04-completion.md'), true);
assert.equal(protectedAuthorityPaths.includes('docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md'), true);
```

- [ ] **Step 2: Run the scope test to verify RED**

Run: `node --test tests/p04_scope.test.mjs`
Expected: exit 1 because the new literal paths and scope count are absent.

- [ ] **Step 3: Capture the contract baseline and update the scope manifest**

Restore `tests/p04_scope.test.mjs` byte-exactly after the RED run and require `git status --porcelain=v1 -z` to match the saved two-path capture. Build the preflight from that saved byte stream, both contract file hashes/sizes, HEAD, the exact 111-path completion scope, the unchanged protected paths, and hashes for all 126 existing migration files through 0196 using the established schema-v1 field names. Reapply the identical RED assertions, then update the scope module to GREEN. Do not add wildcard authority or recapture after implementation starts. Assert every historical migration is absent from `changeScopePaths` and remains byte-identical to its captured digest.

Use the exact schema-v1 field names `captured_at_utc`, `change_scope_paths`, `dirty_files`, `head`, `protected_authority_digests`, `protected_authority_paths`, `schema_version`, `status_porcelain_v1_z_base64`, `status_porcelain_v1_z_sha256`, `protected_authority_revisions`, `scope_amendments`, and `previous_captures`. The two `dirty_files` entries must have status `??`, exact byte counts, and SHA-256 values. Record the 126-file historical migration count and final filename in the scope-amendment reason and verify every existing migration digest separately.

- [ ] **Step 4: Run entry gates**

Run:

```text
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04c-preflight.json --scope p04-scope.mjs
node scripts/protocol-lock.mjs --check .
git diff --check
```

Expected: exits 0; combined scope count matches the updated literal; baseline and protocol authority remain unchanged.

- [ ] **Step 5: Commit the approved contract**

```text
git add docs/superpowers/specs/2026-09-08-vestrace-v1-g0-04-completion-design.md docs/superpowers/plans/2026-09-08-vestrace-v1-g0-04-completion.md docs/development-evidence/v1-g0-04c-preflight.json scripts/p04-scope.mjs tests/p04_scope.test.mjs
git commit -m "docs(p04): freeze completion contract"
```

### Task 2: Add the complete domain vocabulary

**Files:**
- Create: `crates/vestrace-domain/src/embedding/index.rs`
- Create: `crates/vestrace-domain/src/embedding/retrieval.rs`
- Create: `crates/vestrace-domain/src/embedding/adoption.rs`
- Modify: `crates/vestrace-domain/src/embedding/generation.rs`
- Modify: `crates/vestrace-domain/src/embedding/space.rs`
- Modify: `crates/vestrace-domain/src/embedding/job.rs`
- Modify: `crates/vestrace-domain/src/embedding/mod.rs`
- Modify: `crates/vestrace-domain/src/id.rs`
- Test: `crates/vestrace-domain/tests/embedding_contract.rs`

**Interfaces:**
- Consumes: existing `EmbeddingSpaceId`, `EmbeddingJobId`, `CorpusGenerationId`, `TransitionBatchId`, and recipe IDs.
- Produces: `CanonicalGenerationSnapshot`, `GenerationMemberRepresentation`, `IndexBuildAttemptState`, `CorpusChangeCause`, `RetrievalGenerationFence`, `RetrievalGenerationChangedReason`, `LegacyAdoptionState`, `IndexBuildAttemptId`, `CorpusChangeEventId`, and `LegacyAdoptionId`; reuses the existing `RetrievalRunId`.

- [ ] **Step 1: Write closed-set and construction tests**

```rust
#[test]
fn revoked_generation_is_terminal_and_not_current() {
    assert!(CorpusGenerationState::Revoked.is_terminal());
    assert!(!CorpusGenerationState::Revoked.is_current());
}

#[test]
fn a_retrieval_fence_carries_the_complete_generation_snapshot() {
    let fence = retrieval_fence_fixture();
    assert_eq!(fence.snapshot.member_count, 2);
    assert_eq!(fence.snapshot.guard_version, 7);
}
```

Assert exact SQL strings for every new closed enum and reject zero epochs, zero CAS versions, inconsistent dimensions, and a legacy member representation in a canonical snapshot.

- [ ] **Step 2: Run domain tests to verify RED**

Run: `cargo test -p vestrace-domain --test embedding_contract -- --nocapture`
Expected: exit 101 because the new modules, variants, IDs, and constructors do not exist.

- [ ] **Step 3: Implement the values and transitions**

Use these exact public shapes:

```rust
pub struct CanonicalGenerationSnapshot {
    pub workspace_id: WorkspaceId,
    pub space: EmbeddingSpaceKey,
    pub generation_id: CorpusGenerationId,
    pub generation_epoch: u64,
    pub guard_version: u64,
    pub corpus_revision: u64,
    pub built_through_projection_ordinal: u64,
    pub member_count: u64,
}

pub enum GenerationMemberRepresentation { LegacyUpgrade, EncryptedProjection }
pub enum CorpusGenerationState { Building, Ready, Stale, Revoked }
pub enum LegacyAdoptionState { Planned, Rebuilding, ReadyToCutover, Completed, Failed }
pub enum RetrievalGenerationChangedReason { Stale, Revoked, Replaced, CorpusChanged, MemberUnavailable }
```

Extend `EmbeddingSpaceKey` with exact model revision, model qualification, adapter profile, request-shape revision, returned model, encoding format, and dimensions. Provide a legacy-upgrade constructor that cannot satisfy `is_canonical()`.

Add `IndexBuildAttemptId`, `CorpusChangeEventId`, and `LegacyAdoptionId` through the existing `domain_id!` macro in `id.rs`; keep the existing `RetrievalRunId` unchanged.

- [ ] **Step 4: Run domain tests GREEN**

Run: `cargo test -p vestrace-domain --test embedding_contract -- --nocapture`
Expected: exit 0 with every existing and new embedding contract test passing.

- [ ] **Step 5: Commit**

```text
git add crates/vestrace-domain/src/embedding/index.rs crates/vestrace-domain/src/embedding/retrieval.rs crates/vestrace-domain/src/embedding/adoption.rs crates/vestrace-domain/src/embedding/generation.rs crates/vestrace-domain/src/embedding/space.rs crates/vestrace-domain/src/embedding/job.rs crates/vestrace-domain/src/embedding/mod.rs crates/vestrace-domain/src/id.rs crates/vestrace-domain/tests/embedding_contract.rs
git commit -m "feat(p04): define canonical generation contracts"
```

### Task 3: Make canonical generations and legacy quarantine enforceable

**Files:**
- Create: `migrations/0197_embedding_canonical_generations.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `crates/vestrace-application/src/retrieval/ports.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/vector_retriever.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_canonical_generations.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_schema_contract.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_runtime_role_refusals.rs`
- Modify: `crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs`
- Modify: `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`

**Interfaces:**
- Consumes: Task 2 space/generation vocabulary and existing 0187/0191/0194 guards.
- Produces: `vestrace_capture_embedding_generation`, `vestrace_publish_embedding_generation`, and a resolver that returns `CanonicalGenerationSnapshot` or an exact degraded reason.

- [ ] **Step 1: Write failing fresh-install and 0196-upgrade tests**

Seed one legacy registration/generation/member and one canonical projection. Assert:

```rust
assert_eq!(legacy_resolution.unwrap_err().code(), "embedding-legacy-adoption-required");
assert_eq!(canonical.snapshot.corpus_revision, corpus_revision);
assert_eq!(canonical.snapshot.member_count, 1);
```

Attempt runtime INSERT/UPDATE/DELETE on `memory_embeddings`, both generation member branches, guard current-generation fields, and the active-space head. Assert SQLSTATE `42501` or the exact guarded invariant `23514`.

- [ ] **Step 2: Run the focused tests RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_canonical_generations --test embedding_schema_contract -- --nocapture`
Expected: exit 101 because migration 0197 and full snapshots are absent.

- [ ] **Step 3: Implement migration 0197**

Extend `embedding_space_registrations`, `model_qualification_heads`, `embedding_space_corpus_states`, `embedding_index_generation_guards`, `embedding_corpus_generations`, and `embedding_corpus_generation_members`. The head stores active space only; `embedding_index_generation_guards.current_generation_id` is the sole current-generation pointer.

Use a deferred guarded validator equivalent to:

```sql
IF generation.member_representation = 'encrypted_projection' THEN
  IF member.embedding_projection_entry_id IS NULL OR member.legacy_embedding_id IS NOT NULL THEN
    RAISE EXCEPTION 'canonical generation requires one projection member' USING ERRCODE='23514';
  END IF;
  IF projection.state <> 'live' OR material.state <> 'live' THEN
    RAISE EXCEPTION 'canonical generation member must remain Live' USING ERRCODE='23514';
  END IF;
END IF;
```

Revoke legacy runtime DML. Keep legacy reads only inside the adoption-owned guarded functions introduced in Task 10. Provision prepare/finish ownership helpers without granting PUBLIC.

- [ ] **Step 4: Replace legacy resolution behavior**

Make `PgCorpusGenerationResolver` return the complete snapshot from the active-space head plus generation guard. Make `PgVectorRetriever` fail closed for production use; retain only test/upgrade helpers until Task 11 removes its composition. Make `PgEmbeddingStore::upsert` and `delete` return `embedding-legacy-write-retired`.

- [ ] **Step 5: Run schema, upgrade, and direct-write gates GREEN**

Run:

```text
cargo test -p vestrace-infrastructure --test embedding_canonical_generations --test embedding_schema_contract --test embedding_runtime_role_refusals --test p03_upgrade_provisioning --test runtime_role_cannot_write_directly -- --nocapture
```

Expected: exit 0; both fresh and runtime 0196-to-0197 upgrades pass with exact owners/ACLs and legacy DML refused.

- [ ] **Step 6: Commit**

```text
git add migrations/0197_embedding_canonical_generations.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/retrieval/ports.rs crates/vestrace-infrastructure/src/postgres/vector_retriever.rs crates/vestrace-infrastructure/src/postgres/embedding_store.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/tests/embedding_canonical_generations.rs crates/vestrace-infrastructure/tests/embedding_schema_contract.rs crates/vestrace-infrastructure/tests/embedding_runtime_role_refusals.rs crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs
git commit -m "feat(p04): enforce canonical embedding generations"
```

### Task 4: Build and load disposable exact indexes

**Files:**
- Create: `migrations/0198_embedding_index_builds.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-application/src/embedding/index.rs`
- Modify: `crates/vestrace-application/src/embedding/mod.rs`
- Create: `crates/vestrace-infrastructure/src/embedding_index/mod.rs`
- Create: `crates/vestrace-infrastructure/src/embedding_index/flat.rs`
- Create: `crates/vestrace-infrastructure/src/embedding_index/registry.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_index_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/lib.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_index_builds.rs`

**Interfaces:**
- Consumes: `CanonicalGenerationSnapshot`, material vault `unwrap`, encrypted material bytes, corpus-change events.
- Produces: `EmbeddingIndexService::reconcile_one`, `EmbeddingIndexService::ensure_loaded`, `FlatEmbeddingIndex::search`, and `EmbeddingIndexRegistry`.

- [ ] **Step 1: Write pure flat-index RED tests**

```rust
#[test]
fn cosine_search_is_exact_and_breaks_ties_by_projection_ordinal() {
    let index = flat_index_fixture([[1.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let hits = index.search(&[1.0, 0.0], 2).unwrap();
    assert_eq!(hits.iter().map(|hit| hit.projection_ordinal).collect::<Vec<_>>(), vec![3, 7]);
}
```

Also test NaN/infinity refusal, dimension mismatch, zero norm, member/byte limits, and zeroization on registry replacement through a test-only drop witness.

- [ ] **Step 2: Run application/infrastructure unit tests RED**

Run: `cargo test -p vestrace-infrastructure embedding_index -- --nocapture`
Expected: exit 101 because the index modules do not exist.

- [ ] **Step 3: Define the application ports**

```rust
#[async_trait]
pub trait EmbeddingIndexRepository: Send + Sync {
    async fn claim_next_build(&self, context: &RequestContext, owner: &str, limit: u32) -> Result<Option<EmbeddingIndexBuildPlan>, ApplicationError>;
    async fn load_chunk(&self, context: &RequestContext, plan: &EmbeddingIndexBuildPlan, after_ordinal: Option<u64>, limit: u32) -> Result<EncryptedProjectionChunk, ApplicationError>;
    async fn publish_ready(&self, context: &RequestContext, plan: &EmbeddingIndexBuildPlan) -> Result<GenerationPublicationOutcome, ApplicationError>;
    async fn validate_current(&self, context: &RequestContext, snapshot: &CanonicalGenerationSnapshot) -> Result<CurrentGenerationValidation, ApplicationError>;
    async fn finish_attempt(&self, context: &RequestContext, attempt_id: IndexBuildAttemptId, outcome: IndexBuildOutcome) -> Result<(), ApplicationError>;
}
```

The repository never receives process-local index bytes. `publish_ready` performs and commits the guarded database CAS first. The winning service validates the exact Ready/current snapshot, installs its candidate, then validates once more; any mismatch removes and zeroizes it. A crash after CAS leaves a safe registry miss, repaired by `ensure_loaded` through the same before/after validation without changing the durable epoch.

- [ ] **Step 4: Implement migration 0198 and PostgreSQL adapter**

Evolve `embedding_index_rebuild_events` into a cause-XOR stream while preserving all existing publication/event identities. Add `embedding_index_build_attempts`, claim owner/deadline, safe observations, capture function, publication CAS, and local-load validation. Extend `init-runtime-role.sh` with the exact new tables and runtime-callable signatures; focused tests assert guarded owner, FORCE RLS, ACL, and runtime EXECUTE. No table stores bytes or an in-memory-loaded flag.

- [ ] **Step 5: Implement flat index, registry, and service**

Use `Zeroizing<Vec<f32>>`, checked multiplication for allocation, precomputed norms, deterministic `total_cmp`, and immutable `Arc<FlatEmbeddingIndex>` registry values. Zeroize a candidate on failed publication or failed post-install validation, and drop older entries only after committed invalidation.

- [ ] **Step 6: Run GREEN integration tests**

Run: `cargo test -p vestrace-infrastructure --test embedding_index_builds -- --nocapture`
Expected: exit 0 for nonempty, zero-member, concurrent CAS, stale corpus, lazy reload, and bounded-memory refusal cases.

- [ ] **Step 7: Commit**

```text
git add migrations/0198_embedding_index_builds.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/index.rs crates/vestrace-application/src/embedding/mod.rs crates/vestrace-infrastructure/src/embedding_index/mod.rs crates/vestrace-infrastructure/src/embedding_index/flat.rs crates/vestrace-infrastructure/src/embedding_index/registry.rs crates/vestrace-infrastructure/src/postgres/embedding_index_repository.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/src/lib.rs crates/vestrace-infrastructure/tests/embedding_index_builds.rs
git commit -m "feat(p04): build guarded in-memory embedding indexes"
```

### Task 5: Add the production embedding executor and rebuild acceptance

**Files:**
- Create: `migrations/0199_embedding_executor_work.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-application/src/embedding/work.rs`
- Create: `crates/vestrace-application/src/embedding/executor.rs`
- Modify: `crates/vestrace-application/src/embedding/mod.rs`
- Modify: `crates/vestrace-application/src/embedding/job.rs`
- Modify: `crates/vestrace-application/src/embedding/result.rs`
- Modify: `crates/vestrace-application/src/embedding/finalization.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_work_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_job_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_result_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_executor.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_result_preparation.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_result_finalization.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_effect_recovery.rs`

**Interfaces:**
- Consumes: governed job acceptance, provider dispatch authority, key preparation, result preparation/finalization.
- Produces: one `EmbeddingExecutor::execute` and bounded `EmbeddingWorkRepository` claims.

- [ ] **Step 1: Write RED tests for delivery/rebuild XOR and call count**

Use the loopback provider to accept one delivery and one rebuild job. Assert each calls the provider once and reaches Live/Succeeded only through preparation and finalization. Seed a mismatched acceptance branch and assert exact refusal.

```rust
assert_eq!(provider.calls(), 1);
assert_eq!(job_state(&pool, rebuild_job).await, "succeeded");
assert_eq!(live_projection_count(&pool, rebuild_job).await, output_count);
```

- [ ] **Step 2: Run RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_executor -- --nocapture`
Expected: exit 101 because no executor/claim authority exists and 0195 accepts delivery only.

- [ ] **Step 3: Add work and executor ports**

```rust
pub enum EmbeddingWorkKind { Dispatch, ReconcileKeys, FinalizeResult, BuildIndex, CoordinateTransition, PropagateErasure }

#[async_trait]
pub trait EmbeddingWorkRepository: Send + Sync {
    async fn claim(&self, context: &RequestContext, kind: EmbeddingWorkKind, owner: &str, limit: u32) -> Result<Vec<EmbeddingWorkClaim>, ApplicationError>;
    async fn finish(&self, context: &RequestContext, claim: &EmbeddingWorkClaim, outcome: EmbeddingWorkOutcome) -> Result<(), ApplicationError>;
}

impl EmbeddingExecutor {
    pub async fn execute(&self, context: &RequestContext, job_id: EmbeddingJobId) -> Result<EmbeddingExecutionOutcome, ApplicationError>;
}
```

Claims may identify work but cannot manufacture `ProviderDispatchAuthority`.

- [ ] **Step 4: Implement migration 0199 forward replacements**

Add bounded job claims and a closed delivery/rebuild acceptance XOR. Forward-replace every 0195 validator that hard-codes `purpose='delivery'`. Keep retrieval-query excluded from persisted-output preparation. Preserve existing publication and receipt identities. Extend the provisioning script with each new relation and exact runtime-callable signature and assert owner/ACL parity on fresh install and 0198-to-0199 upgrade.

Replace the permanent `ciphertext exists + projection Live` replay rule with a phase-aware rule: pre-erasure publication requires exact Live ciphertext; post-erasure history requires the exact guarded erasure preparation/tombstone lineage introduced by migration 0203. Until 0203 exists, the new branch is closed and cannot be selected.

- [ ] **Step 5: Implement the executor**

Reconstruct and dispatch the same `EffectiveModelRequest`, retain bounded response buffers, validate exact model/dimension/index order, and call existing preparation. Definite provider/schema failures abandon only unprepared intents and terminalize with persisted evidence. Do not invoke an acknowledged successor automatically.

- [ ] **Step 6: Run preparation/finalization/recovery GREEN**

Run:

```text
cargo test -p vestrace-infrastructure --test embedding_executor --test embedding_result_preparation --test embedding_result_finalization --test embedding_effect_recovery -- --nocapture
```

Expected: exit 0; existing 14E behavior remains green and rebuild uses the same result chain.

- [ ] **Step 7: Commit**

```text
git add migrations/0199_embedding_executor_work.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/work.rs crates/vestrace-application/src/embedding/executor.rs crates/vestrace-application/src/embedding/mod.rs crates/vestrace-application/src/embedding/job.rs crates/vestrace-application/src/embedding/result.rs crates/vestrace-application/src/embedding/finalization.rs crates/vestrace-infrastructure/src/postgres/embedding_work_repository.rs crates/vestrace-infrastructure/src/postgres/embedding_job_repository.rs crates/vestrace-infrastructure/src/postgres/embedding_result_repository.rs crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/tests/embedding_executor.rs crates/vestrace-infrastructure/tests/embedding_result_preparation.rs crates/vestrace-infrastructure/tests/embedding_result_finalization.rs crates/vestrace-infrastructure/tests/embedding_effect_recovery.rs
git commit -m "feat(p04): execute governed embedding jobs"
```

### Task 6: Make transition recipes and batches prove an exact bijection

**Files:**
- Create: `migrations/0200_embedding_transition_execution.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `crates/vestrace-application/src/embedding/transition.rs`
- Modify: `crates/vestrace-application/src/embedding/carry.rs`
- Modify: `crates/vestrace-application/src/embedding/barrier.rs`
- Create: `crates/vestrace-application/src/embedding/transition_coordinator.rs`
- Modify: `crates/vestrace-application/src/embedding/mod.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_transition_planning.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_carry_classification.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_transition_barriers.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_transition_activation.rs`

**Interfaces:**
- Consumes: Live canonical projections, fresh rebuild jobs, existing carry/barrier rules.
- Produces: batch attempts, recipe satisfactions, completeness proof, and `ReadyToActivate` progress.

- [ ] **Step 1: Write bijection RED cases**

Assert success for one exact result and exact `SatisfiedExisting`; assert `23514` for omitted, duplicate, wrong input order, wrong batch, wrong target space, failed attempt, and physical-job reuse.

```rust
assert_eq!(prove_completeness(&pool, plan_id).await.unwrap().state, EmbeddingSpaceTransitionState::ReadyToActivate);
assert_sqlstate(prove_with_duplicate_satisfier(&pool, plan_id).await, "23514");
```

- [ ] **Step 2: Run transition suites RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_transition_activation --test embedding_transition_planning -- --nocapture`
Expected: exit 101 because old recipes contain only opaque arrays and no target lineage.

- [ ] **Step 3: Implement migration 0200**

Add exact old projection/dependency fields, `embedding_transition_batches`, ordered batch recipes, physical attempts, strict-XOR satisfactions, and append-only observations. Upgrade carry/barrier mappings to foreign-key the exact batch/recipe identities. Use deferred constraints for the whole lineage-wide bijection. Extend provisioning with exact relation/signature grants and assert owner/ACL parity on fresh install and upgrade.

- [ ] **Step 4: Implement repository and coordinator methods**

```rust
async fn create_batch_attempt(&self, context: &RequestContext, command: CreateTransitionBatchAttempt) -> Result<EmbeddingJobId, ApplicationError>;
async fn observe_attempt(&self, context: &RequestContext, command: ObserveTransitionAttempt) -> Result<TransitionProgress, ApplicationError>;
async fn prove_completeness(&self, context: &RequestContext, command: ProveTransitionCompleteness) -> Result<TransitionProgress, ApplicationError>;
```

All commands carry expected versions and immutable IDs. The caller cannot assert a satisfaction kind without the SQL authority deriving it from terminal result facts.

- [ ] **Step 5: Run transition/carry/barrier GREEN**

Run:

```text
cargo test -p vestrace-infrastructure --test embedding_transition_planning --test embedding_carry_classification --test embedding_transition_barriers --test embedding_transition_activation -- --nocapture
```

Expected: exit 0 with exact bijection and existing ambiguity behavior preserved.

- [ ] **Step 6: Commit**

```text
git add migrations/0200_embedding_transition_execution.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/transition.rs crates/vestrace-application/src/embedding/carry.rs crates/vestrace-application/src/embedding/barrier.rs crates/vestrace-application/src/embedding/transition_coordinator.rs crates/vestrace-application/src/embedding/mod.rs crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/tests/embedding_transition_planning.rs crates/vestrace-infrastructure/tests/embedding_carry_classification.rs crates/vestrace-infrastructure/tests/embedding_transition_barriers.rs crates/vestrace-infrastructure/tests/embedding_transition_activation.rs
git commit -m "feat(p04): prove embedding transition completeness"
```

### Task 7: Activate transitions and staged credential rotation atomically

**Files:**
- Create: `migrations/0201_embedding_transition_activation.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `crates/vestrace-application/src/embedding/transition.rs`
- Modify: `crates/vestrace-application/src/embedding/transition_coordinator.rs`
- Modify: `crates/vestrace-application/src/connections.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/qualification_job_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/credential_activation.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/model_binding_repository.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_transition_activation.rs`
- Modify: `crates/vestrace-infrastructure/tests/credential_activation.rs`

**Interfaces:**
- Consumes: Task 6 completeness, target Ready/current generation guard, existing credential activation guards and 0196 erasure blockers.
- Produces: `vestrace_activate_embedding_transition` and `TransitionActivationReceipt`.

- [ ] **Step 1: Write activation and rotation RED tests**

Cover credential-to-credential, credential-to-no-auth, no-auth-to-credential, no-auth-to-no-auth, expired qualification, changed slot version, erasure-prepared candidate, incomplete completion-blocker adoption, and concurrent erasure.

```rust
let receipt = activate_rotation(&pool, fixture).await.unwrap();
assert_eq!(current_model_qualification(&pool).await, receipt.target_qualification_id);
assert_eq!(active_embedding_space(&pool).await, receipt.target_space_id);
assert_eq!(credential_state(&pool, receipt.source_credential_id).await, "retired");
assert_eq!(credential_state(&pool, receipt.target_credential_id).await, "active");
```

- [ ] **Step 2: Run activation tests RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_transition_activation --test credential_activation -- --nocapture`
Expected: exit 101 because transition-aware activation does not exist.

- [ ] **Step 3: Implement migration 0201**

Make embedding qualification finalization create an ordinary or `rotation_staged` transition instead of advancing a live embedding head. Add immutable activation receipts. The activation function locks connection, unique credential guards in canonical order, then source/target space guards; it verifies completion-blocker adoption, exact Ready/current target generation, head version, recipes, qualification, credential lineage, and erasure state before one transaction changes qualification head and active space. Provision only its exact guarded relations/functions and assert fresh/upgrade owner and ACL parity.

- [ ] **Step 4: Implement application/repository activation**

```rust
async fn activate(&self, context: &RequestContext, command: ActivateEmbeddingTransition) -> Result<TransitionActivationReceipt, ApplicationError>;
```

The service supplies identities and expected versions only. SQL derives the allowed guard set and candidate/retirement events. It never holds a provider lease during activation.

- [ ] **Step 5: Run activation GREEN and rotation-before-adoption proof**

Run: `cargo test -p vestrace-infrastructure --test embedding_transition_activation --test credential_activation -- --nocapture`
Expected: exit 0; successful rotation commits only after exact adoption, while every mismatched/racing case leaves head and credentials unchanged.

- [ ] **Step 6: Commit**

```text
git add migrations/0201_embedding_transition_activation.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/transition.rs crates/vestrace-application/src/embedding/transition_coordinator.rs crates/vestrace-application/src/connections.rs crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs crates/vestrace-infrastructure/src/postgres/qualification_job_repository.rs crates/vestrace-infrastructure/src/postgres/credential_activation.rs crates/vestrace-infrastructure/src/postgres/model_binding_repository.rs crates/vestrace-infrastructure/tests/embedding_transition_activation.rs crates/vestrace-infrastructure/tests/credential_activation.rs
git commit -m "feat(p04): activate embedding transitions atomically"
```

### Task 8: Persist fenced retrieval results and one confirmed retry

**Files:**
- Create: `migrations/0202_embedding_retrieval_results.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-application/src/embedding/retrieval.rs`
- Modify: `crates/vestrace-application/src/embedding/mod.rs`
- Modify: `crates/vestrace-application/src/retrieval/request.rs`
- Modify: `crates/vestrace-application/src/retrieval/ports.rs`
- Modify: `crates/vestrace-application/src/retrieval/service.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_retrieval_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_retrieval_results.rs`
- Modify: `crates/vestrace-infrastructure/tests/retrieval_generation_fence.rs`

**Interfaces:**
- Consumes: current active space, Ready/current generation guard, local index registry, embedding executor/provider dispatch.
- Produces: `EmbeddingRetrievalJobClient`, guarded terminal finalization, and `RetryRetrievalGenerationChanged`.

- [ ] **Step 1: Write the two-order and retry RED tests**

Pause one fixture after provider response. In query-first order assert result/Succeeded then later staling. In change-first order assert receipt + generation-change + FailedDefinite and zero result rows. Assert same-key replay returns one successor and another key conflicts.

```rust
assert_eq!(provider.calls(), 1);
assert_eq!(retrieval_result_count(&pool, job).await, 0);
assert_eq!(job_failure_reason(&pool, job).await, "retrieval_generation_changed");
```

- [ ] **Step 2: Run retrieval suites RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_retrieval_results --test retrieval_generation_fence -- --nocapture`
Expected: exit 101 because the existing fence is request-level and results are not job-owned.

- [ ] **Step 3: Define the retrieval client and repository**

```rust
#[async_trait]
pub trait EmbeddingRetrievalJobClient: Send + Sync {
    async fn retrieve(&self, context: &RequestContext, request_id: RetrievalRunId, request: &NormalizedRetrievalRequest, deadline: DateTime<Utc>) -> Result<EmbeddingRetrievalOutcome, ApplicationError>;
}

pub enum EmbeddingRetrievalOutcome {
    Completed(Vec<RetrievalCandidate>),
    Degraded(EmbeddingRetrievalDegradation),
}
```

Generate `RetrievalRunId` once in `RetrievalService::search`. No HTTP, MCP, vector adapter, or worker layer creates another logical request identity.

- [ ] **Step 4: Implement migration 0202**

Add one-to-one fences, result headers/ordered references, generation-change observations, and retry edges. Acceptance captures snapshot + fence under the current space generation guard. Terminal functions retain the guard through local search and SQL commit. Persist only safe references/ranks/scores. Extend provisioning with exact relations and guarded signatures and assert owner/ACL parity on fresh install and upgrade.

- [ ] **Step 5: Implement application and PostgreSQL adapters**

Route retrieval query output to the new finalizer, zeroize the query vector on every branch, and map missing local index, legacy adoption, transition not ready, and generation not ready into closed degradation values.

- [ ] **Step 6: Run retrieval GREEN**

Run:

```text
cargo test -p vestrace-infrastructure --test embedding_retrieval_results --test retrieval_generation_fence --test retrieval_classification_boundary --test vector_retriever_data_policy -- --nocapture
cargo test -p vestrace-application retrieval -- --nocapture
```

Expected: exits 0; no query-vector bytes/digest persist and provider-call counts remain exact.

- [ ] **Step 7: Commit**

```text
git add migrations/0202_embedding_retrieval_results.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/retrieval.rs crates/vestrace-application/src/embedding/mod.rs crates/vestrace-application/src/retrieval/request.rs crates/vestrace-application/src/retrieval/ports.rs crates/vestrace-application/src/retrieval/service.rs crates/vestrace-infrastructure/src/postgres/embedding_retrieval_repository.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/tests/embedding_retrieval_results.rs crates/vestrace-infrastructure/tests/retrieval_generation_fence.rs
git commit -m "feat(p04): fence embedding retrieval results"
```

### Task 9: Propagate source and vector erasure through generations

**Files:**
- Create: `migrations/0203_embedding_erasure_propagation.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-application/src/embedding/erasure.rs`
- Modify: `crates/vestrace-application/src/embedding/mod.rs`
- Modify: `crates/vestrace-application/src/material/erasure.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_erasure_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/erasure.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_erasure_propagation.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_result_finalization.rs`

**Interfaces:**
- Consumes: material erasure preparation, projection dependency graph, generation/event authority, transition plans.
- Produces: atomic dependent-vector preparation, generation revocation, transition staling, post-commit invalidation, and historically valid erased publications.

- [ ] **Step 1: Write erasure RED tests**

Seed shared sources, multiple spaces, a Ready generation, an active transition recipe, and a published result. Pause at ErasurePrepared and assert source/vector hydrate, local query, new reference creation, and transition activation all fail before tombstone.

```rust
assert_eq!(generation_state(&pool, generation).await, "revoked");
assert_eq!(transition_state(&pool, transition).await, "stale");
assert!(registry.get(&generation_key).is_none());
```

- [ ] **Step 2: Run erasure/finalization RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_erasure_propagation --test embedding_result_finalization -- --nocapture`
Expected: exit 101 because erasure does not traverse projection dependencies and 0195 replay rejects legal post-erasure history.

- [ ] **Step 3: Implement migration 0203**

In phase one lock all affected spaces/materials canonically, prepare dependent projections/vector materials, advance corpus states, revoke current generations, emit cause-bound invalidation events, and stale affected transition versions. Add exact erasure-preparation/tombstone foreign keys.

Forward-replace 0195 publication/recovery validators so a historical publication is valid in exactly one branch: still-Live ciphertext, or exact later erasure/tombstone lineage. Never delete publication, provider receipt, job success, or structural projection identity. Extend provisioning with exact erasure relations/functions and assert fresh/upgrade owner and ACL parity.

- [ ] **Step 4: Implement post-commit registry invalidation**

`EmbeddingErasureService::reconcile_one` consumes only committed invalidation events and calls `EmbeddingIndexRegistry::remove_space_before_epoch`. Query still validates the database guard, so missed notification remains fail-closed.

- [ ] **Step 5: Run erasure and retained-history GREEN**

Run:

```text
cargo test -p vestrace-infrastructure --test embedding_erasure_propagation --test embedding_result_finalization --test embedding_index_builds -- --nocapture
```

Expected: exit 0; phase-one refusal precedes tombstone, finalization history remains valid, and rebuilt generations contain only remaining Live members.

- [ ] **Step 6: Commit**

```text
git add migrations/0203_embedding_erasure_propagation.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/erasure.rs crates/vestrace-application/src/embedding/mod.rs crates/vestrace-application/src/material/erasure.rs crates/vestrace-infrastructure/src/postgres/embedding_erasure_repository.rs crates/vestrace-infrastructure/src/postgres/erasure.rs crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/tests/embedding_erasure_propagation.rs crates/vestrace-infrastructure/tests/embedding_result_finalization.rs
git commit -m "feat(p04): propagate embedding vector erasure"
```

### Task 10: Replace legacy backfill with resumable governed adoption

**Files:**
- Create: `migrations/0204_embedding_legacy_adoption.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-application/src/embedding/adoption.rs`
- Modify: `crates/vestrace-application/src/embedding/mod.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_adoption_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_store.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/vector_retriever.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_legacy_adoption.rs`

**Interfaces:**
- Consumes: legacy upgrade members, Live source materials, ordinary rebuild jobs, canonical generation builder.
- Produces: `EmbeddingLegacyAdoptionService::start_or_resume`, exact cutover receipts, legacy tombstones, and `legacy_plaintext_retired`.

- [ ] **Step 1: Write adoption RED tests**

Cover complete adoption, missing source blocker, erased source blocker, restart after job acceptance, duplicate command, partial target refusal, cutover race, legacy row deletion, and database-wide retirement gate.

```rust
let replay = service.start_or_resume(&context, command.clone()).await.unwrap();
assert_eq!(replay.plan_id, first.plan_id);
assert_eq!(legacy_vector_count(&pool).await, 0);
assert!(legacy_plaintext_retired(&pool).await);
```

- [ ] **Step 2: Run adoption RED**

Run: `cargo test -p vestrace-infrastructure --test embedding_legacy_adoption -- --nocapture`
Expected: exit 101 because no durable plan/cutover authority exists.

- [ ] **Step 3: Implement migration 0204**

Add adoption headers/members/blockers/cutover receipts, retired legacy identity tombstones, and the installation retirement gate. Before retirement, validators require the exact `memory_embeddings` row; cutover records its opaque identity, removes executable/FK dependence, deletes plaintext, and preserves non-content history. Extend provisioning with exact adoption relations/functions and assert fresh/upgrade owner and ACL parity.

Only a database-wide guarded scan with every adoption Completed and zero legacy rows may commit `legacy_plaintext_retired`. Add no product-managed backup implementation; later backup creation must consume this gate.

- [ ] **Step 4: Implement service and repository**

```rust
pub async fn start_or_resume(&self, context: &RequestContext, command: StartLegacyAdoption) -> Result<LegacyAdoptionProgress, ApplicationError>;
pub async fn advance(&self, context: &RequestContext, plan_id: LegacyAdoptionId) -> Result<LegacyAdoptionProgress, ApplicationError>;
```

Create rebuild jobs from exact Live sources, reuse no legacy vector bytes, and report blockers as stable typed reasons.

- [ ] **Step 5: Run adoption and dump-canary GREEN**

Run: `cargo test -p vestrace-infrastructure --test embedding_legacy_adoption -- --nocapture`
Expected: exit 0; after cutover both SQL canary search and a fresh test dump search find neither vector plaintext nor its stable digest.

- [ ] **Step 6: Commit**

```text
git add migrations/0204_embedding_legacy_adoption.sql docker/postgres/init-runtime-role.sh crates/vestrace-application/src/embedding/adoption.rs crates/vestrace-application/src/embedding/mod.rs crates/vestrace-infrastructure/src/postgres/embedding_adoption_repository.rs crates/vestrace-infrastructure/src/postgres/embedding_store.rs crates/vestrace-infrastructure/src/postgres/vector_retriever.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-infrastructure/tests/embedding_legacy_adoption.rs
git commit -m "feat(p04): adopt legacy embeddings through governed rebuilds"
```

### Task 11: Compose all embedding work in the production worker

**Files:**
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Modify: `crates/vestrace-cli/src/commands/rebuild.rs`
- Modify: `crates/vestrace-cli/src/commands/server.rs`
- Modify: `crates/vestrace-cli/src/commands/mcp.rs`
- Modify: `crates/vestrace-cli/src/main.rs`
- Modify: `crates/vestrace-infrastructure/src/config.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Create: `crates/vestrace-cli/tests/embedding_worker_once.rs`
- Modify: `crates/vestrace-cli/tests/worker_once_cli.rs`
- Modify: `crates/vestrace-cli/tests/provider_runtime_wiring.rs`

**Interfaces:**
- Consumes: Tasks 4–10 services and repositories.
- Produces: six bounded worker cycles, governed rebuild/adoption CLI, and server/MCP retrieval clients without legacy provider/index composition.

- [ ] **Step 1: Write composition RED tests**

Assert the real CLI binary with `worker --once` advances one accepted embedding job through its available phase, reports a failed attempted cycle with nonzero exit, and resumes an expired claim. Assert source inspection/behavior has no constructed `EmbedMemoryHandler`, direct `EmbeddingBackfillService`, `PgEmbeddingStore` writer, or production `PgVectorRetriever`.

- [ ] **Step 2: Run CLI composition RED**

Run:

```text
cargo test -p vestrace-cli --test embedding_worker_once --test worker_once_cli --test provider_runtime_wiring -- --nocapture
```

Expected: exit 101 because current worker/rebuild/server still construct the three legacy paths.

- [ ] **Step 3: Add bounded configuration**

Add validated values for claim batch, claim lease, maximum index members, maximum index bytes, maximum dimensions, build chunk size, concurrent builders, and retrieval wait budget. Defaults must be finite and tests must reject zero/overflowing values.

- [ ] **Step 4: Replace worker composition**

Construct one governed provider runtime, one material vault, one local registry, and the six services. Each configured workspace runs each bounded cycle. Merge outcomes into `PollOutcome`; a cycle error sets `failed`, while pending/deferred work alone does not spin or report success.

- [ ] **Step 5: Replace rebuild/server/MCP legacy routes**

`rebuild embeddings` calls `EmbeddingLegacyAdoptionService::start_or_resume` and optionally observes progress. Server and MCP install `EmbeddingRetrievalJobClient`. Remove direct provider calls and pgvector retrieval composition; no fallback remains.

- [ ] **Step 6: Run CLI GREEN**

Run:

```text
cargo test -p vestrace-cli --test embedding_worker_once --test worker_once_cli --test provider_runtime_wiring -- --nocapture
```

Expected: exit 0 with real binary composition and exact one-pass failure semantics.

- [ ] **Step 7: Commit**

```text
git add crates/vestrace-cli/src/commands/worker.rs crates/vestrace-cli/src/commands/rebuild.rs crates/vestrace-cli/src/commands/server.rs crates/vestrace-cli/src/commands/mcp.rs crates/vestrace-cli/src/main.rs crates/vestrace-infrastructure/src/config.rs crates/vestrace-infrastructure/src/postgres/mod.rs crates/vestrace-cli/tests/embedding_worker_once.rs crates/vestrace-cli/tests/worker_once_cli.rs crates/vestrace-cli/tests/provider_runtime_wiring.rs
git commit -m "feat(p04): compose the production embedding worker"
```

### Task 12: Expose guarded retry, transition, adoption, and retrieval status

**Files:**
- Modify: `crates/vestrace-http/src/api/embedding_jobs.rs`
- Modify: `crates/vestrace-http/src/api/models.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `crates/vestrace-http/src/route_inventory.rs`
- Modify: `crates/vestrace-http/src/health.rs`
- Modify: `crates/vestrace-mcp/src/server.rs`
- Modify: `crates/vestrace-cli/src/commands/schema.rs`
- Modify: `crates/vestrace-cli/tests/provider_openapi_contract.rs`
- Create: `crates/vestrace-http/tests/embedding_retrieval_routes.rs`
- Create: `crates/vestrace-mcp/tests/embedding_retrieval.rs`
- Modify: `crates/vestrace-http/tests/route_inventory_is_exhaustive.rs`

**Interfaces:**
- Consumes: retrieval retry, transition resume/activate, adoption progress, closed degradation codes.
- Produces: authorized mutation routes and safe read-only detail without vector/credential content.

- [ ] **Step 1: Write HTTP/OpenAPI/MCP RED tests**

Assert `POST /v1/embedding-jobs/{id}/retry-generation-changed` requires confirmation, expected version, idempotency, and capability. Assert Models/readiness expose exact safe reasons and no vector/digest fields. Assert route inventory and checked-in OpenAPI match.

- [ ] **Step 2: Run contract tests RED**

Run:

```text
cargo test -p vestrace-http --test embedding_retrieval_routes --test route_inventory_is_exhaustive -- --nocapture
cargo test -p vestrace-cli --test provider_openapi_contract -- --nocapture
cargo test -p vestrace-mcp --test embedding_retrieval -- --nocapture
```

Expected: nonzero exits because the routes and schemas are absent.

- [ ] **Step 3: Implement mutations and read models**

Map typed application commands only. Use the operation `embedding.retry_retrieval_generation_changed` and existing governed mutation headers. Return conflict for a stale version/different idempotency key and the prior successor for exact replay.

Expose IDs, state, epoch, counts, attempts, blockers, and safe reasons. Never expose ciphertext, vector bytes, stable digests, credentials, or a database assertion about process-local index presence.

- [ ] **Step 4: Verify the served OpenAPI document**

Extend `crates/vestrace-cli/src/commands/schema.rs`, then let `provider_openapi_contract` launch the real `vestrace schema http` binary and inspect its JSON. The static `schemas/openapi-v1.json` file is not a served authority and must remain unchanged. Do not edit protocol-lock authority.

- [ ] **Step 5: Run HTTP/MCP/OpenAPI GREEN**

Run the three commands from Step 2.
Expected: exits 0; mutation and read-only surfaces match the route inventory and checked-in schema.

- [ ] **Step 6: Commit**

```text
git add crates/vestrace-http/src/api/embedding_jobs.rs crates/vestrace-http/src/api/models.rs crates/vestrace-http/src/api/mod.rs crates/vestrace-http/src/router.rs crates/vestrace-http/src/route_inventory.rs crates/vestrace-http/src/health.rs crates/vestrace-http/tests/embedding_retrieval_routes.rs crates/vestrace-http/tests/route_inventory_is_exhaustive.rs crates/vestrace-mcp/src/server.rs crates/vestrace-mcp/tests/embedding_retrieval.rs crates/vestrace-cli/src/commands/schema.rs crates/vestrace-cli/tests/provider_openapi_contract.rs
git commit -m "feat(p04): expose embedding completion operations"
```

### Task 13: Prove real restart, duplicate-dispatch, index, and activation faults

**Files:**
- Modify: `crates/vestrace-fault-scenario/src/child.rs`
- Modify: `crates/vestrace-fault-scenario/src/main.rs`
- Modify: `crates/vestrace-fault-scenario/src/report.rs`
- Modify: `crates/vestrace-fault-scenario/src/settings.rs`
- Create: `crates/vestrace-fault-scenario/src/scenarios/embedding_worker_completion_crash.rs`
- Modify: `tests/embedding_fault_scenario_e2e.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_worker_restart.rs`

**Interfaces:**
- Consumes: real worker binary/services, loopback provider/vault, PostgreSQL fault fixtures.
- Produces: child-abort evidence for every newly owned boundary and exact provider-call counts.

- [ ] **Step 1: Add RED fault points and expected matrix**

Define points after work claim, after Dispatching, after provider response, after ResultPrepared, after key receipt, before/after index CAS, before/after retrieval terminal transaction, before/after activation commit, and after erasure invalidation commit.

```rust
const COMPLETION_POINTS: &[EmbeddingCompletionFaultPoint] = &[
    EmbeddingCompletionFaultPoint::AfterWorkClaim,
    EmbeddingCompletionFaultPoint::BeforeIndexCas,
    EmbeddingCompletionFaultPoint::AfterIndexCas,
    EmbeddingCompletionFaultPoint::BeforeActivationCommit,
    EmbeddingCompletionFaultPoint::AfterActivationCommit,
];
```

- [ ] **Step 2: Run ignored fault suite RED**

Run:

```text
cargo build -p vestrace-fault-scenario
cargo test --test embedding_fault_scenario_e2e -- --ignored --nocapture --test-threads=1
```

Expected: build or test exits nonzero because new scenarios are not implemented.

- [ ] **Step 3: Implement child abort and parent observation**

Every child performs real setup, reaches exactly one durable boundary, flushes its synchronization marker, and aborts. The parent restarts normal services and checks exact job/effect/result/generation/transition state plus loopback call count. Do not replace process death with returned errors.

- [ ] **Step 4: Prove duplicate claims without duplicate provider dispatch**

Run two workers against one eligible job and force one claim lease to expire. Assert one `Dispatching` transition/provider request, one terminal result, and immutable losing claim observations. Local index computation may duplicate; durable generation publication remains one-CAS.

- [ ] **Step 5: Run fault suite GREEN**

Run the commands from Step 2.
Expected: exits 0; report enumerates every fault point and observed call count.

- [ ] **Step 6: Commit**

```text
git add crates/vestrace-fault-scenario/src/child.rs crates/vestrace-fault-scenario/src/main.rs crates/vestrace-fault-scenario/src/report.rs crates/vestrace-fault-scenario/src/settings.rs crates/vestrace-fault-scenario/src/scenarios/embedding_worker_completion_crash.rs tests/embedding_fault_scenario_e2e.rs crates/vestrace-infrastructure/tests/embedding_worker_restart.rs
git commit -m "test(p04): prove embedding worker crash recovery"
```

### Task 14: Qualify mutations and close P04 evidence

**Files:**
- Modify: `crates/vestrace-infrastructure/tests/embedding_canonical_generations.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_index_builds.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_transition_activation.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_retrieval_results.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_erasure_propagation.rs`
- Modify: `crates/vestrace-infrastructure/tests/embedding_legacy_adoption.rs`
- Modify: `docs/development-evidence/v1-g0-04-embedding-transition-foundation.md`

**Interfaces:**
- Consumes: final production migrations/functions and all focused tests.
- Produces: RED/exact-restore/GREEN evidence and final lead-reviewable P04 acceptance record.

- [ ] **Step 1: Establish the unchanged final-source baseline**

Run every focused suite from Tasks 2–13 once without mutation. Record exit, count, duration, test-source SHA-256, migration hashes, and function owner/ACL/runtime EXECUTE for each mutated SQL authority.

- [ ] **Step 2: Run one mutation at a time**

Use isolated databases and mutate exactly one primary predicate per run:

```text
generation current-ID/epoch fence
corpus revision CAS
canonical member Live predicate
recipe one-satisfier bijection
target credential slot/version lineage
rotation completion-blocker adoption
retrieval one-successor uniqueness
source/vector erasure invalidation
legacy runtime-write refusal
```

Expected for every unchanged focused command: RED exit 101 or another documented nonzero test exit. Record whether the probe observed unsafe persisted state or an independent defense stopped it at a different exact boundary.

- [ ] **Step 3: Restore byte-exactly after each mutation**

Compare original/restored `pg_get_functiondef`, owner, `proacl`, runtime EXECUTE, and relevant migration/test hashes. Then rerun the identical command and require GREEN exit 0 before installing the next mutation.

- [ ] **Step 4: Run the final complete gate matrix**

```text
cargo test -p vestrace-domain --test embedding_contract -- --nocapture
cargo test -p vestrace-application retrieval -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_canonical_generations --test embedding_index_builds --test embedding_executor --test embedding_transition_activation --test embedding_retrieval_results --test embedding_erasure_propagation --test embedding_legacy_adoption -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_schema_contract --test embedding_runtime_role_refusals --test p03_upgrade_provisioning --test runtime_role_cannot_write_directly -- --nocapture
cargo test -p vestrace-cli --test embedding_worker_once --test worker_once_cli --test provider_runtime_wiring --test provider_openapi_contract -- --nocapture
cargo test -p vestrace-http --test embedding_retrieval_routes --test route_inventory_is_exhaustive -- --nocapture
cargo test -p vestrace-mcp --test embedding_retrieval -- --nocapture
cargo build -p vestrace-fault-scenario
cargo test --test embedding_fault_scenario_e2e -- --ignored --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy -p vestrace-domain -p vestrace-application -p vestrace-infrastructure -p vestrace-cli -p vestrace-http -p vestrace-mcp -p vestrace-fault-scenario --all-targets -- -D warnings
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04c-preflight.json --scope p04-scope.mjs
node scripts/protocol-lock.mjs --check .
git diff --check
```

Expected: every command exits 0. Record exact counts and durations; do not collapse skipped, blocked, or unverified checks into PASS.

- [ ] **Step 5: Update evidence and perform final review**

Map frozen-spec embedding clauses to tests and retained mutation/fault evidence. Remove or supersede every P04 `Not true yet` item for executor, index, transition activation, retrieval fence/result/retry, erasure, rotation, adoption, and worker recovery. State explicitly that P04 closure does not complete the remaining v1 gate packages.

- [ ] **Step 6: Commit final evidence**

```text
git add crates/vestrace-infrastructure/tests/embedding_canonical_generations.rs crates/vestrace-infrastructure/tests/embedding_index_builds.rs crates/vestrace-infrastructure/tests/embedding_transition_activation.rs crates/vestrace-infrastructure/tests/embedding_retrieval_results.rs crates/vestrace-infrastructure/tests/embedding_erasure_propagation.rs crates/vestrace-infrastructure/tests/embedding_legacy_adoption.rs docs/development-evidence/v1-g0-04-embedding-transition-foundation.md
git commit -m "test(p04): qualify embedding completion"
```

- [ ] **Step 7: Stop before push**

Show `git status --short --branch`, the commit list since the Task 1 baseline, final gate evidence, and any material limitation. Push only after a separate explicit user instruction.
