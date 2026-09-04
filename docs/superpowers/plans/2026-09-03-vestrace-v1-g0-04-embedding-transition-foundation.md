# Vestrace v1 G0-04 — embedding jobs, corpus generations, transitions, and retrieval fences

Package P04 of the twelve-package gate program. Depends on P03, which is
operator-accepted. This plan is written under the program's rule that a detailed
plan is authored only after every predecessor has accepted interfaces and
persisted evidence, and it is not executable until an independent reviewer has
traced it against the frozen spec and the live repository.

Frozen spec: `docs/superpowers/specs/2026-08-25-vestrace-v1-full-product-ag-ui-a2a-design.md`.
Every requirement below cites the spec line range it is derived from, so a
reviewer can check the derivation rather than the prose.

## Package boundary decisions

**In scope.** Durable `EmbeddingJob` identity and lifecycle; `EmbeddingSpaceKey`
as a first-class guarded key; corpus generations; transition plans, versions,
ordered recipes and batches; the ambiguity carry and barrier machinery; and the
retrieval generation fence that makes a mixed-generation read impossible rather
than unlikely.

**Explicitly not in scope, and not to be claimed.** No console screen and no P06
work. No real remote provider qualification and no LM Studio call. No backup,
restore, or activation supervisor — those are P05's. No new AG-UI or A2A
surface. No change to the P03 provider dispatch, admission, or recovery
authorities beyond adding embedding as a second caller of them.

**The boundary that matters most.** P04 does not build a second dispatch
mechanism. The spec's canonical lock order (§6.4, line 189) already places
`(workspace, EmbeddingSpaceKey)` transition and corpus guards *inside* the same
chain that begins with `ConnectionExecutionGuard`, and the ambiguity rules
(lines 245–256) are written in the same vocabulary as P03's `Unknown`,
`InconclusiveUnknown`, `Dispatching` and shared recovery winner. An embedding
job is a second kind of governed external effect, not a parallel universe. Any
task in this plan that would fork P03's dispatch, recovery, or admission
authority is a defect in this plan, not a licence.

## What already exists and must not be duplicated

Verified against the tree on 2026-09-03, not assumed:

| Exists | Where | Consequence for P04 |
| --- | --- | --- |
| `embedding_spaces` (id, workspace, name, dimensions, model) | migration `0009_embedding_spaces.sql` | predates the guarded-owner pattern: plain RLS, no revision, no immutability, created lazily by name. P04 must give it a guarded key without orphaning existing rows |
| `memory_embeddings`, dimension-free since `0148` | migrations `0009`, `0148_embeddings_have_a_space.sql` | the vector is already checked against its space's declared dimension rather than a fixed column width; do not "fix" this again |
| retrieval pipeline, 2789 lines | `crates/vestrace-application/src/retrieval/` | ports `TextRetriever`, `VectorRetriever`, `ExactRetriever`, `StructuredRetriever`, plus fusion, rerank, context builder and `RetrievalJournal`. **No port signature carries a space or generation**, so the fence has nowhere to live yet |
| lazy space creation | `retrieval/embedding.rs`, `ensure_space(context, name, model, dimensions)` | a space appears as a side effect of the first embedding. P04 must replace this with an accepted, guarded key without breaking the memory write path |
| embedding data policy | `embedding_data_policy.rs`, `embedding_data_policy_decisions` | P03 built the pre-disclosure decision record for embeddings; P04 consumes it, and must not add a second policy surface |
| P03 governed dispatch, admission, recovery | `provider_dispatch.rs`, migrations `0182`–`0186` | the authorities an embedding effect must reuse |

**Absent entirely**, confirmed by search of `crates/*/src` and `migrations/`:
`embedding_job`, `corpus_generation`, `transition_recipe`, `transition_batch`.
The identifiers `generation` and `carry` occur widely but only as ordinary
English; there is no generation or carry concept in the codebase today.

## Live defects and gaps this package inherits

Carried from P03's accepted evidence, because P04 either closes them or must
work around them:

1. **No production path publishes a connection admission policy.** Every P03
   end-to-end proof seeds `connection_admission_policy_revisions` directly, so a
   governed step on a freshly created Connection cannot dispatch. An
   `EmbeddingJob` dispatches through the same admission authority and inherits
   this exactly. P04 must either publish the policy or state that embedding
   dispatch is equally unreachable in production — it cannot quietly seed it in
   fixtures and call the path proved.
2. **`credential_activation` takes `SELECT ... FOR UPDATE` on `idempotency_keys`.**
   That privilege is not held by the restricted runtime role. It is latent today
   because no runtime-role caller reaches it; an embedding job that activates or
   consumes a credential lease may become that caller.
3. **The four run-step guarded functions are declared in two places** —
   migration `0186` and `docker/postgres/init-runtime-role.sh` — with nothing
   keeping the lists in step. P04 adds more guarded functions to the same two
   lists and must not repeat the omission that made a fresh deployment unable to
   migrate.
4. **`model_binding_snapshots` has no identity trigger.** A transition-scoped
   snapshot is required by spec lines 251 and 255 to be plan-derived and unusable
   by ordinary work; that property needs an enforcement point, and the snapshot
   table currently has only an immutability trigger.

## Global constraints

- No commit, push, deploy, LM Studio call, or external network call.
- Work only in the P04 allowlist, frozen by Task 1. Every path outside it is a
  stop-and-ask, not a judgement call.
- Preserve every unrelated dirty file byte-for-byte. Migrations `0172`–`0186`
  are historical and immutable; P04's forward migrations begin at `0187`.
- Never persist or log confidential input, a raw request digest, a DEK, a
  credential, or provider output.
- Never call a vault or a provider adapter while a PostgreSQL transaction is
  open.
- Missing authority fails closed with no fallback.
- **`scripts/verify-dirty-baseline.mjs` enters `protectedAuthorityPaths` in this
  package.** P03 was permitted to edit it; P04 is not. This is a standing
  instruction carried into Task 1.

## Acceptance criteria

Each criterion names the spec lines it derives from. A criterion that cannot be
traced to the spec does not belong here.

### A. Embedding jobs are governed external effects

- A1. An `EmbeddingJob` has durable identity, an immutable
  `ModelBindingSnapshot` resolved once at acceptance under the guard and never
  changed, and fresh external-effect and MRE identities and nonces. *(line 257)*
- A2. At most one adapter invocation exists per external-effect identity, and
  nothing automatically retries once `Dispatching` exists. *(line 245)*
- A3. A recovered ordinary `EmbeddingJob` `Unknown` is advanced only by the
  authorized, versioned `Acknowledge possible duplicate provider charge and start
  new embedding job` command, requiring `embedding.retry_after_unknown`, leaving
  the predecessor unchanged and linking `retries_unknown_embedding_job_id`. The
  same idempotency key returns the same successor; a different key conflicts.
  *(line 257)*

### B. Spaces, corpora and generations

- B1. `(workspace, EmbeddingSpaceKey)` is a guarded key taken in the canonical
  lock order, after `ConnectionExecutionGuard` and any
  `CredentialActivationGuard`, before predecessor job/effect and barrier
  lineage. *(line 189)*
- B2. A corpus generation is immutable once published, and a vector belongs to
  exactly one generation.
- B3. Existing `embedding_spaces` rows survive the introduction of the guarded
  key; no memory loses its embeddings.

### C. Transitions

- C1. A transition version's planning transaction is guarded and follows each
  logical `TransitionBatchId` lineage to its latest terminal
  `InconclusiveUnknown` attempt with no direct same-version or carry successor;
  that attempt is the one and only eligible ambiguity head. *(line 253)*
- C2. Classification is at recipe granularity against the predecessor's fixed
  ordered wire batch. Each old recipe/input ordinal gets either exact structural
  equivalence to a new one that remains required, or immutable
  `NoLongerRequired` evidence. No added recipe, collapse, reorder, hidden
  regroup, equality oracle, or mixing with unaffected work. *(line 253)*
- C3. `EmbeddingTransitionAmbiguityCarry` has the closed header lifecycle
  `AwaitingAcknowledgement -> SuccessorCreated | NoLongerRequired`, at most one
  current open mapping per head, and immutable history. *(line 253)*
- C4. A barrier is target-version and batch bound, with the closed lifecycle
  `AwaitingPredecessorTerminal -> ResolvedToCarry | ResolvedSatisfiedExisting |
  ResolvedDefinite | NoLongerRequired | Superseded`, at most one current open
  barrier per predecessor attempt and lineage, and its whole dedicated batch is
  nondispatchable while open. *(line 256)*
- C5. A predecessor has at most one direct successor across the same-version
  edge or the cross-version carry edge. *(lines 251, 255)*
- C6. A transition-scoped `ModelBindingSnapshot` is derived only from the
  immutable transition plan tuple, never from current or default resolution, and
  is unusable by ordinary work, defaults, or q1. *(lines 251, 255)*

### D. Retrieval generation fences

- D1. A retrieval result can never mix embedding spaces or generations. This is
  the package's headline exit criterion and must be enforced where a mixed read
  is *unrepresentable*, not merely rejected late.
- D2. The fence is visible in the port signature: a retriever that could not
  state its generation could not be fenced.

### E. Evidence

- E1. Component, PostgreSQL, worker-restart and fault evidence, with no mixed
  space and no duplicate dispatch observed. *(gate program, P04 row)*
- E2. Every refusal names its observed SQLSTATE **and** the mechanism that
  produced it. P03's qualification established that several mechanisms on these
  paths share `42501` and `23514`, so a bare code cannot identify which guard
  spoke.
- E3. No in-memory double is cited for a database or crash-boundary invariant.

## External corpus impact

`defer post-v1`.

No manifest entry in `docs/external-corpus/` is affected. P04 touches the
embedding, transition and retrieval surfaces, none of which is described by a
registered external corpus entry; the registered corpus concerns planning
documents rather than runtime behaviour, and §17.1 of the spec keeps it outside
the v1 boundary. No compatibility seam is required and nothing is adopted now.

## Task decomposition

Twelve tasks, executed strictly in order. Tasks 1 to 3 are detailed below;
Tasks 4 to 12 are specified after them at decreasing depth and must be brought
to the same detail before the reviewer sees this plan a second time.

---

### Task 1: Capture the P04 dirty baseline and freeze scope

**Files:**
- Create first: `docs/development-evidence/v1-g0-04-preflight.json`
- Create: `scripts/p04-scope.mjs`
- Create: `tests/p04_scope.test.mjs`
- **Do not modify:** `scripts/verify-dirty-baseline.mjs`

**Interfaces:**
- `p04-scope.mjs` exports exact `changeScopePaths` and `protectedAuthorityPaths`
  arrays, sorted, unique, disjoint, with no wildcard.
- **The verifier needs no change.** Its dispatch already accepts any module
  matching the `pNN-scope.mjs` pattern and imports `changeScopePaths` and
  `protectedAuthorityPaths` from it; only `p01-scope.mjs` is special-cased, for
  its legacy uppercase export names. P03 generalised this, which is precisely
  why the verifier can now be protected: no later package has a reason to edit
  it. A task that finds itself needing to is reporting a defect in this
  assumption, not proceeding.
- `protectedAuthorityPaths` includes, at minimum: the frozen spec, the gate
  program index, the P01, P02 and P03 plans and preflights, the accepted P01,
  P02 and P03 evidence documents, `schemas/protocol-lock.json`, the q1 manifest
  and fixture, and **`scripts/verify-dirty-baseline.mjs`**.
- The preflight records HEAD, raw NUL-delimited porcelain bytes and their
  digest, both arrays, and every dirty path with byte length and SHA-256, using
  the P03 artifact shape so the same verifier reads it unchanged.

- [ ] **Step 1: Capture before the first P04 implementation write**

```powershell
git -c safe.directory=E:/Soft/vestrace rev-parse HEAD
git -c safe.directory=E:/Soft/vestrace status --porcelain=v1 -z --untracked-files=all
```

The baseline will be large — P03 closed at 317 entries and none of it is
committed. That is expected and is not a reason to commit first: the program's
verifier exists to make a large dirty baseline safe rather than to avoid one.
This plan file is itself protected authority and is captured, not rewritten.

- [ ] **Step 2: RED**

Three failures must be observed before anything is written, and each must fail
for its own reason rather than for a shared one:

```powershell
node --test tests/p04_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04-preflight.json --scope p04-scope.mjs
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs
```

The first must fail because `scripts/p04-scope.mjs` does not exist; the second
must fail with the verifier's own `scope module does not exist` error, proving
the dispatch is reached rather than silently defaulting to P01; the third must
still pass, proving P04's arrival changed nothing for its predecessors.

A fourth, negative case is required and is the one that matters: a fixture in
which `scripts/verify-dirty-baseline.mjs` is mutated must be **rejected** by the
P04 scope. If that fixture passes, the protection is decorative.

- [ ] **Step 3: GREEN**

```powershell
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04-preflight.json --scope p04-scope.mjs
node scripts/protocol-lock.mjs --check .
```

The verifier exits 1 while any authorized cohabitation path remains dirty; the
finding list must contain exactly those paths and nothing else, and the exact
list is recorded rather than described.

- [ ] **Step 4: Scoped diff review**

Confirm every later task path is listed, the arrays are disjoint and sorted, no
earlier authority digest changed, `scripts/verify-dirty-baseline.mjs` appears in
the protected array and nowhere in the change array, and no functional code
predates the preflight.

---

### Task 2: Declare the embedding space key, corpus generation, and job contracts

**Files:**
- Create: `crates/vestrace-domain/src/embedding/mod.rs`
- Create: `crates/vestrace-domain/src/embedding/space.rs`
- Create: `crates/vestrace-domain/src/embedding/generation.rs`
- Create: `crates/vestrace-domain/src/embedding/job.rs`
- Modify: `crates/vestrace-domain/src/lib.rs`
- Modify: `crates/vestrace-application/src/lib.rs`
- Create: `crates/vestrace-domain/tests/embedding_contract.rs`

**Interfaces:**
- `EmbeddingSpaceKey` is a value type over workspace, space name, model and
  dimensions that cannot be constructed from a name alone. The current lazy
  `ensure_space(context, name, model, dimensions)` proves why: a space that can
  appear as a side effect of its first write has no moment at which anyone
  authorized it.
- `CorpusGenerationId` is opaque. A generation carries its space key, an
  ordinal, and a closed state; it is immutable once published.
- `EmbeddingJobId`, `TransitionVersion`, `TransitionBatchId` and
  `TransitionRecipeOrdinal` are opaque identities. Ordinals are typed rather
  than bare integers, because spec line 253 requires classification against a
  *fixed ordered wire batch* and an untyped index invites the reorder that the
  same line forbids.
- The closed lifecycles are declared as Rust enums with no catch-all variant, so
  a new state is a compile error at every match site: `EmbeddingJobState`;
  `CarryHeaderState` with `AwaitingAcknowledgement`, `SuccessorCreated`,
  `NoLongerRequired`; `CarryMappingState` with `AwaitingAcknowledgement`,
  `NoLongerRequired`; `BarrierState` with `AwaitingPredecessorTerminal`,
  `ResolvedToCarry`, `ResolvedSatisfiedExisting`, `ResolvedDefinite`,
  `NoLongerRequired`, `Superseded`.
- Nothing in this task performs I/O. The task's whole purpose is that the
  vocabulary exists before any table or service does, so a later task cannot
  quietly invent a fifth barrier state in SQL.

- [ ] **Step 1: RED**

`embedding_contract.rs` asserts the closed sets and the construction rules and
fails to compile or fails its assertions before the types exist.

```powershell
cargo test -p vestrace-domain --test embedding_contract
```

- [ ] **Step 2: GREEN**

The suite must include a negative for each closed lifecycle: a transition the
spec forbids — `SuccessorCreated` back to `AwaitingAcknowledgement`,
`Superseded` to `ResolvedToCarry`, a second direct successor for one head — is
refused by the type or by an explicit guard, and the test names which of the two
refused it.

- [ ] **Step 3: Scoped diff review**

Confirm no `Deserialize` derive on a type that must not be constructible from
untrusted input, and no `From<Uuid>` conversion that would let a caller mint an
identity it was not given.

---

### Task 3: Provision the guarded schema for jobs, spaces, generations and ACLs

**Files:**
- Create: `migrations/0187_embedding_jobs_and_corpus_generations.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-infrastructure/tests/embedding_schema_contract.rs`
- Modify: `crates/vestrace-infrastructure/tests/runtime_role_cannot_write_directly.rs`

**Interfaces:**
- Every new table is owned by `vestrace_guarded_owner`, has RLS enabled and
  forced, carries a non-empty ACL, and refuses direct runtime DML with `42501`.
- `embedding_spaces` gains its guarded key **without** losing existing rows.
  Migration `0009` created it under plain RLS and it is not empty in any
  deployment that has embedded a memory. The migration must state its backfill
  and prove it, not assume the table is empty.
- Every guarded function this migration hands to the owner is declared in
  **both** allowlist arrays in `docker/postgres/init-runtime-role.sh`. P03's
  qualification found a function missing from those arrays, which made a fresh
  deployment unable to migrate while every superuser-provisioned test passed.
  The same omission here would be the same defect.
- The canonical lock order is enforced in SQL, not by convention: connection
  execution guard, then any credential activation guard in slot order, then the
  exact workspace and embedding space key transition or corpus guard, then the
  predecessor job or effect, then barrier lineage, then batch and recipe
  mappings.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test embedding_schema_contract
```

Before the migration, every ACL, ownership, RLS and lock-order assertion fails
with an absent object rather than a wrong grant. That distinction is recorded:
`42P01` is not evidence of a guard.

- [ ] **Step 2: GREEN, including the provisioning path**

```powershell
cargo test -p vestrace-infrastructure --test embedding_schema_contract
cargo test -p vestrace-infrastructure --test p03_upgrade_provisioning
cargo test -p vestrace-infrastructure --test runtime_role_cannot_write_directly
```

`p03_upgrade_provisioning` is included deliberately: it is the only suite that
runs the real Compose provisioner and then migrates as the restricted runtime
role, and it is the suite that caught P03's allowlist omission. A green
per-test database proves nothing about that path.

- [ ] **Step 3: Proof by breaking**

Remove one new function from one allowlist array and observe
`p03_upgrade_provisioning` fail with `42501` naming that exact signature;
restore and verify the file byte-identical by digest. Grant the runtime role
`INSERT` on one new guarded table and observe the refusal test fail on the
**message**, not merely the SQLSTATE — P03 established that the immutability
trigger raises the same `42501` as the table ACL, so a code-only assertion
cannot see an ACL being removed.

- [ ] **Step 4: Scoped diff review**

Confirm the closed-world refusal test inherited from P03 still passes and now
covers the new tables without being edited: it derives its set from `pg_class`,
so a new guarded table it does not cover is a defect in the migration, not in
the test.

---

### Task 4: Accept and dispatch an ordinary embedding job

**Files:**
- Create: `crates/vestrace-application/src/embedding/job.rs`
- Create: `crates/vestrace-application/src/embedding/mod.rs`
- Modify: `crates/vestrace-application/src/provider_dispatch.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_job_repository.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/mod.rs`
- Modify: `crates/vestrace-cli/src/commands/worker.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_dispatch_is_atomic.rs`
- Modify: `migrations/0187_embedding_jobs_and_corpus_generations.sql`

**Interfaces:**
- The embedding job reaches the provider through the **existing** authorities.
  Concretely, it calls `vestrace_try_admit_provider_dispatch`,
  `vestrace_issue_credential_dispatch_lease`,
  `vestrace_consume_credential_dispatch_lease`,
  `vestrace_record_provider_throttle` and `vestrace_release_provider_dispatch`
  — the same five functions P03's Run-step dispatch calls. A new admission or
  throttle function in this task is a defect.
- `ProviderDispatchRepository` gains `load_embedding_dispatch_plan` and
  `recover_embedding_job_attempt`, mirroring the shapes of
  `load_run_step_dispatch_plan` and `recover_run_step_attempt`, each with a
  fail-closed default that returns `ApplicationError::Unavailable`. Adding them
  to the same trait is deliberate: it is what makes "one dispatch authority,
  two callers" checkable by a reviewer rather than asserted.
- Acceptance resolves the immutable `ModelBindingSnapshot` exactly once, inside
  the guarded transaction, and never re-resolves it. *(line 257)*
- The executor calls the adapter at most once per external-effect identity and
  never retries after `Dispatching`. *(line 245)*

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test embedding_dispatch_is_atomic
```

The suite fails while `load_embedding_dispatch_plan` returns its fail-closed
default. That default is the RED state, and the test must assert
`Unavailable` specifically rather than any error: a fail-closed path that
returned the wrong error would still look red.

- [ ] **Step 2: GREEN**

Every leg of one dispatch commits or rolls back together: attempt phase, effect
lifecycle transition, admission row, credential lease consumption, MRE. The
suite injects a write boundary at each leg and asserts the whole tuple rolls
back, following the pattern of P03's `injected_write_boundaries_roll_every_dispatch_leg_back`.

- [ ] **Step 3: The admission gap, confronted rather than seeded**

P03 left no production path that publishes a connection admission policy, so
`vestrace_try_admit_provider_dispatch` refuses with `provider dispatch admission
policy is absent`. This task must record which of these is true, with evidence:

- the policy is published by a route this task adds, and embedding dispatch is
  reachable in production; or
- it is not, and **both** Run-step and embedding dispatch are unreachable in
  production, which is a G0 exit problem and not merely a fixture inconvenience.

Seeding the policy in a fixture and reporting the dispatch path as proved is
explicitly forbidden here, because P03's evidence already records that the
seeding is a gap and repeating it would launder a known gap into a claim.

- [ ] **Step 4: Scoped diff review**

Confirm no second admission, throttle, lease or recovery function was created;
that the new trait methods sit on `ProviderDispatchRepository` rather than on a
parallel trait; and that the worker composes one executor that serves both
callers.

---

### Task 5: Recover an ambiguous embedding effect

**Files:**
- Modify: `crates/vestrace-application/src/embedding/job.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_job_repository.rs`
- Create: `crates/vestrace-http/src/api/embedding_jobs.rs`
- Modify: `crates/vestrace-http/src/api/mod.rs`
- Modify: `crates/vestrace-http/src/router.rs`
- Modify: `schemas/openapi-v1.json`
- Create: `crates/vestrace-infrastructure/tests/embedding_effect_recovery.rs`
- Modify: `migrations/0187_embedding_jobs_and_corpus_generations.sql`

**Interfaces:**
- After the shared recovery winner appends `Unknown`, the job finalizer
  idempotently appends terminal `InconclusiveUnknown`, releases only the job
  scheduler lease, and publishes nothing. A crash between those two writes is
  recovered from the immutable effect outcome **without another adapter call**.
  *(line 245)*
- Scheduled refresh skips a latest `InconclusiveUnknown` and exposes
  `operator_acknowledgement_required`. *(line 245)*
- `POST /v1/embedding-jobs/{id}/acknowledge-unknown` requires
  `embedding.retry_after_unknown` through the shared authorization authority,
  takes an expected job version and an operator-supplied `Idempotency-Key`,
  appends immutable Audit acknowledgement evidence, leaves the predecessor job
  and its `Unknown` outcome unchanged, and creates exactly one successor with
  fresh external-effect, MRE, nonce and material-key identities plus
  `retries_unknown_embedding_job_id`. The same key returns the same successor; a
  different key conflicts. *(line 257)*
- The successor never reuses the predecessor's authorization decision, receipt,
  vector, credential lease or binding, and re-enters every current check.
  *(line 257)*
- The route is inventory-covered: `route_inventory_is_exhaustive` must include
  it without being told to.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test embedding_effect_recovery
cargo test -p vestrace-http --test route_inventory_is_exhaustive
```

- [ ] **Step 2: GREEN, with the adapter counted**

The recovery suite holds an adapter call counter across the crash boundary and
asserts it stays at one. P03's qualification established that the executor's
time-dependent branches cannot both be driven end to end without waiting out the
dispatch TTL; this task inherits that limit and must state which branch it
proves against the database and which it proves at the executor level, rather
than implying both are end-to-end.

- [ ] **Step 3: Proof by breaking**

Make recovery create a fresh effect after `Dispatching` and observe the
call-count assertion fail; restore. This is the exact break P03's plan named and
did not perform, so it is performed here.

- [ ] **Step 4: Scoped diff review**

Confirm the acknowledgement command cannot be reached by scheduling or recovery,
only by an authorized caller, and that `retry-only-when-safe` remains a distinct
path that cannot acknowledge ambiguity. *(line 246)*

---

### Task 6: Plan a transition version

**Files:**
- Create: `crates/vestrace-application/src/embedding/transition.rs`
- Create: `crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs`
- Create: `migrations/0188_embedding_transitions.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-infrastructure/tests/embedding_transition_planning.rs`

**Interfaces:**
- A transition plan is immutable and carries the source and target
  auth-binding-XOR tuple, version, batch, ordered recipes and target. Every
  later derivation reads from it and never from current or default resolution.
  *(lines 251, 255)*
- The transition-scoped `ModelBindingSnapshot` is job-owned and derived only
  from that plan tuple. It must be **unusable by ordinary work, defaults and
  q1**, which needs an enforcement point: `model_binding_snapshots` has an
  immutability trigger but no identity trigger, so this task adds the predicate
  that makes an ordinary consumer unable to select a transition-scoped snapshot.
  *(lines 251, 255; inherited gap 4)*
- Planning runs inside one guarded transaction taking the canonical lock order.
  *(line 189)*
- Recipes are an immutable **ordered subsequence** of the predecessor wire
  order. No added recipe, collapse, reorder or hidden regroup. *(line 253)*

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test embedding_transition_planning
```

- [ ] **Step 2: GREEN**

The suite proves the ordering property positively and negatively: a plan whose
recipes preserve predecessor-relative order is accepted, and one that reorders,
collapses, adds or regroups is refused with an exact SQLSTATE **and** message.

- [ ] **Step 3: Proof by breaking**

Remove the transition-scope predicate and observe an ordinary dispatch select a
transition-scoped snapshot; restore and verify the migration byte-identical.
This break is the point of the task: without it, "unusable by ordinary work" is
a sentence rather than a property.

- [ ] **Step 4: Scoped diff review**

Confirm the new guarded functions are in both allowlist arrays, and that the
closed-world refusal test covers the new tables without edits.

### Task 7: Recipe-granular classification and the ambiguity carry

**Files:**
- Modify: `crates/vestrace-application/src/embedding/transition.rs`
- Create: `crates/vestrace-application/src/embedding/carry.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs`
- Create: `migrations/0189_embedding_transition_carry.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Modify: `crates/vestrace-http/src/api/embedding_jobs.rs`
- Modify: `schemas/openapi-v1.json`
- Create: `crates/vestrace-infrastructure/tests/embedding_carry_classification.rs`

**Interfaces:**

The whole task is one guarded transaction and five properties that must hold
inside it. Each is stated as something a test can fail.

- **One eligible head.** Planning follows each logical `TransitionBatchId`
  lineage to its latest terminal `InconclusiveUnknown` attempt that has no direct
  same-version and no carry successor. That attempt is the one and only eligible
  ambiguity head. Earlier unknown attempts that already have a direct successor
  are immutable linked ancestry: never reclassified, never given
  `NoLongerRequired`, never given a second child. The transaction cannot skip a
  newer eligible head, disconnect a lineage, or classify twice. *(line 253)*
- **Recipe granularity against a fixed wire order.** For each old recipe and
  input ordinal, the transaction persists either exact structural equivalence to
  one new-version recipe and input ordinal that remains required, or immutable
  `NoLongerRequired` evidence. No added recipe, collapse, reorder, hidden
  regroup, equality or hash oracle, and no mixing with unaffected or new
  scheduler work. *(line 253)*
- **A dedicated batch or nothing.** When any recipe survives, exactly one
  dedicated current target `TransitionBatch` is created containing exactly the
  mapped new recipes, as an immutable ordered subsequence of the predecessor
  wire order, with the old to new mapping persisted. When none survives there is
  no retry and no blocker at all — not an empty batch. *(line 253)*
- **At most one current open carry per head.** If a later target version is
  planned before acknowledgement, the guard terminalizes the obsolete mapping as
  `NoLongerRequired` with reason `superseded target mapping` and creates and
  links the next current mapping for the same no-child head and still-required
  subset. Historical mappings are immutable. *(line 253)*
- **`Succeeded` is never rescheduled directly.** The guarded terminal classifier
  must first record exact `SatisfiedExisting` when target lineage, space and
  `Live` predicates hold, or immutable definite-resolution evidence that alone
  releases lawful new scheduling. *(line 253)*

The acknowledgement command is
`POST /v1/embedding-transitions/{id}/acknowledge-carry`, requiring
`embedding.retry_carried_transition_batch_after_unknown`. It names the whole
predecessor possible charge, the exact mapped surviving subset, old and new
versions, old and new batch ids, and the ordered old to new recipe and input
mapping. It leaves the predecessor and the possible charge unchanged, reconciles
only old unprepared intents to `Abandoned`, and atomically marks the carry
`SuccessorCreated` while creating exactly one fresh physical new-version attempt
with plan-derived transition-scoped snapshot and fresh identities. The same
idempotency key returns that successor; a different key conflicts. A stale,
expired, revoked, removed, changed, activated or superseded mapping refuses; it
never dispatches from an obsolete carry. *(line 255)*

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test embedding_carry_classification
```

- [ ] **Step 2: GREEN, property by property**

One test per property above, each named after the property rather than after the
function it calls, so a reviewer reading the test list reads the spec. The
negative cases are the point: a second child for one head, a reordered subset, a
regrouped batch, an empty surviving batch, and a direct reschedule of a
`Succeeded` recipe must each be refused with an exact SQLSTATE **and** message.

- [ ] **Step 3: Concurrency, not just sequence**

Two independent pools plan overlapping target versions against one lineage
simultaneously, on the pattern of P03's `run_acceptance_races_*` suites. The
assertion is that exactly one current open carry mapping exists afterwards and
the other planner observed a complete state, never a mixture.

P03's qualification is a warning here: its two race suites passed eight times
out of eight with the connection-guard row lock removed, so a race test that
merely runs two things at once proves nothing. This task's race test must be
shown to fail when the guard is removed, and that break is required evidence,
not optional.

- [ ] **Step 4: Scoped diff review**

Confirm no equality or hash oracle was introduced for structural equivalence,
that mapping rows are insert-only, and that no code path can create a second
direct successor across either edge.

---

### Task 8: Barriers and supersession

**Files:**
- Create: `crates/vestrace-application/src/embedding/barrier.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/embedding_transition_repository.rs`
- Create: `migrations/0190_embedding_transition_barriers.sql`
- Modify: `docker/postgres/init-runtime-role.sh`
- Create: `crates/vestrace-infrastructure/tests/embedding_transition_barriers.rs`

**Interfaces:**
- When new-version planning finds an overlapping predecessor attempt in
  `Requested`, `Running`, waiting, `Authorized`, `Dispatching`, or
  ResultPrepared/completion-only reconciliation, it first appends one immutable
  `AwaitingPredecessorTerminal` barrier observation **and** its dedicated
  immutable `TransitionBatch`, containing exactly the mapped still-required
  overlap in preserved predecessor-relative wire order. No unaffected or new
  recipe may share that batch. *(line 256)*
- The barrier is target-version and batch bound, with the closed lifecycle
  `AwaitingPredecessorTerminal -> ResolvedToCarry | ResolvedSatisfiedExisting |
  ResolvedDefinite | NoLongerRequired | Superseded`. Its entire dedicated batch
  is nondispatchable and completeness-blocking while it is open, while unrelated
  batches proceed normally. At most one current open barrier exists per
  predecessor attempt and logical-batch lineage. *(line 256)*
- Supersession and terminal classification serialize under the canonical lock
  order. If the target version goes stale before the predecessor terminalizes,
  guarded current-version planning revalidates the still-required overlap; if
  overlap remains it appends `Superseded`, terminalizes the obsolete batch, and
  creates the single linked successor barrier with `supersedes_barrier_id` and a
  fresh preserved-order batch; if none remains it appends `NoLongerRequired` and
  creates no successor. Superseded barriers and batches are immutable history.
  *(line 256)*
- **Absence is not death.** `Dispatching` remains deadline-gated through the
  shared recovery authority, and `ResultPrepared` must complete its existing
  bind and finalize path before classification. *(line 256)*
- Candidate abandonment terminalizes only the current open barrier as
  `NoLongerRequired(candidate_abandoned)`; prior `Superseded` history is
  unchanged. *(line 256)*

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test embedding_transition_barriers
```

- [ ] **Step 2: GREEN**

Each of the six terminal states is reached by the situation the spec assigns to
it, and each assertion names the state and the evidence that produced it. The
classifier-versus-supersession race is exercised on two independent pools: if
classification wins it resolves the then-current barrier; if supersession wins
the classifier follows the linked chain and resolves only the latest current
barrier. Neither path may move a recipe to another batch, invoke the provider,
create a direct job child, create a duplicate call, or omit a recipe.

- [ ] **Step 3: Proof by breaking**

Make the dedicated barrier batch dispatchable while open and observe a
completeness assertion fail. Allow a second current open barrier for one lineage
and observe the uniqueness assertion fail. Both restored and verified by digest.

- [ ] **Step 4: Scoped diff review**

Confirm the barrier lifecycle has no catch-all state, that `supersedes_barrier_id`
is insert-only, and that no scheduler path can dispatch a barrier-owned batch.

---

### Task 9: Retrieval generation fences

**Files:**
- Modify: `crates/vestrace-application/src/retrieval/ports.rs`
- Modify: `crates/vestrace-application/src/retrieval/request.rs`
- Modify: `crates/vestrace-application/src/retrieval/service.rs`
- Modify: `crates/vestrace-application/src/retrieval/fusion.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/vector_retriever.rs`
- Modify: `crates/vestrace-infrastructure/src/postgres/text_retriever.rs`
- Create: `crates/vestrace-infrastructure/tests/retrieval_generation_fence.rs`
- Modify: `crates/vestrace-application/tests/retrieval_*` as required

**Interfaces:**
- This is the package's headline exit criterion: **a retrieval result can never
  mix embedding spaces or generations.** The fence must make a mixed read
  unrepresentable rather than rejected late, because a late rejection is a test
  that passes while the defect remains reachable by the next caller.
- Concretely, `NormalizedRetrievalRequest` carries the resolved
  `(EmbeddingSpaceKey, CorpusGenerationId)` and the retriever ports take it, so
  a retriever that could not state its generation could not compile. Today none
  of the four port signatures mentions a space at all, which is why the fence has
  nowhere to live.
- Fusion combines candidates only within one generation. Two candidates from
  different generations reaching fusion is a bug the type system should have
  prevented; fusion asserts it anyway and says so, because defence in depth here
  costs one comparison.
- The vector retriever's SQL filters by generation, and the filter is not
  optional: a query built without it does not compile.

- [ ] **Step 1: RED**

```powershell
cargo test -p vestrace-infrastructure --test retrieval_generation_fence
cargo test -p vestrace-application --test retrieval_e2e
```

- [ ] **Step 2: GREEN**

Two generations of the same space are populated with deliberately
distinguishable vectors. A retrieval pinned to generation N returns only
generation N candidates, at every channel and after fusion and rerank. A
retrieval pinned to a generation that does not exist refuses rather than falling
back to the newest.

- [ ] **Step 3: Proof by breaking**

Remove the generation predicate from the vector retriever's SQL and observe the
fence test fail with candidates from both generations named individually, not as
a count. A count would pass if the two generations happened to return the same
number of rows.

- [ ] **Step 4: Scoped diff review**

Confirm no retrieval path constructs a `NormalizedRetrievalRequest` with a
defaulted or inferred generation, and that the journal records the generation
each run was fenced to, so an operator reading `retrieval_runs` can tell which
corpus answered.

### Task 10: Refusal, mixed-space and duplicate-dispatch qualification

**Files:**
- Create: `crates/vestrace-infrastructure/tests/embedding_runtime_role_refusals.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_space_isolation.rs`
- Modify: `crates/vestrace-infrastructure/tests/provider_runtime_role_refusals.rs`

**Interfaces:**
- **Every refusal names its observed SQLSTATE and the mechanism that produced
  it.** P03 established by breaking one that the table ACL and the immutability
  trigger both raise `42501` on the same statement, so a code-only assertion
  passes straight through a removed ACL. Every refusal test in this task asserts
  the exact message or constraint name as well.
- The closed-world refusal test inherited from P03 derives its table set from
  `pg_class` and must cover every new guarded table **without being edited**. If
  it needs editing, the migration forgot a guard.
- Mixed-space isolation is proved the way P03 proved branch isolation: two
  spaces in one workspace whose models advertise the **same** wire model id and
  the same dimensions, so nothing but the pinned identities can tell them apart.
  A vector, a generation, a job and a retrieval fenced to one must never reach
  the other.
- Duplicate dispatch is proved with an adapter call counter across a real crash
  boundary, not with a mock that records intent.

- [ ] **Step 1: RED, then GREEN**

```powershell
cargo test -p vestrace-infrastructure --test embedding_runtime_role_refusals
cargo test -p vestrace-infrastructure --test embedding_space_isolation
cargo test -p vestrace-infrastructure --test provider_runtime_role_refusals
```

- [ ] **Step 2: Proof by breaking, three of them**

Grant the runtime role `INSERT` on one new guarded table and observe the message
assertion fail while the SQLSTATE assertion still passes — recording that
difference is the point of the break, not a side note. Remove the space
predicate from one guarded function and observe mixed-space isolation fail
naming the crossing rows. Allow a second adapter invocation for one effect
identity and observe the counter assertion fail.

- [ ] **Step 3: Scoped diff review**

Confirm no refusal test asserts only `is_err()`, and that no in-memory double is
cited for any database invariant in this task.

---

### Task 11: Worker-restart and fault evidence

**Files:**
- Modify: `crates/vestrace-fault-scenario/src/child.rs`
- Modify: `crates/vestrace-fault-scenario/src/settings.rs`
- Create: `tests/embedding_fault_scenario_e2e.rs`
- Create: `crates/vestrace-infrastructure/tests/embedding_worker_restart.rs`

**Interfaces:**
- The five external-effect fault points in `EffectFaultPoint::required_points()`
  are reused unchanged. No new enum, no forked harness. *(gate program P04 row;
  the same rule P03 was held to)*
- Each scenario asserts the exact surviving job, effect, intent, authorization,
  owner, deadline, receipt, reconciliation, snapshot and MRE state, and the
  number of loopback requests.
- Worker restart is proved by a process that actually dies. P03 proved its
  crash boundary with a panicking adapter, which is faithful only where no
  transaction is open across the crash; a barrier or carry classification that
  crashes mid-transaction needs a real `abort()`, and this task must say which
  boundaries it proves by which mechanism rather than implying one covers both.

**A cost this task must confront before it starts.** P03 declined to build the
provider fault-scenario harness because `child.rs` drives a generic webhook
effect and a governed provider dispatch needs its whole fixture rebuilt inside a
standalone binary. An embedding job needs the same fixture. If that cost is
still not worth paying, this task records what is therefore unproved — the four
required points other than the one a panic can reach — rather than quietly
narrowing the exit criterion. The gate program's P04 row names worker-restart
and fault evidence explicitly, so narrowing it is an operator decision, not a
builder's.

- [ ] **Step 1: RED, then GREEN**

```powershell
cargo test -p vestrace-infrastructure --test embedding_worker_restart
cargo build -p vestrace-fault-scenario
cargo test --test embedding_fault_scenario_e2e -- --ignored --nocapture
```

- [ ] **Step 2: Scoped diff review**

Confirm the fault point names and count are unchanged, and that the harness was
not tuned toward `FaultObservation::expected` — every field must be read back
from persisted rows or counted by the party dispatched to.

---

### Task 12: Integrated verification, truthful evidence, and independent review

**Files:**
- Create: `docs/development-evidence/v1-g0-04-embedding-transition-foundation.md`
- Inspect only: every P04 path and repository-wide status.

**Interfaces:**
- Evidence records the source revision, preflight digest, exact scoped file
  list, commands with exit codes and test counts, every observed SQLSTATE **and
  mechanism**, loopback request counts, fault-boundary results, the external
  corpus decision, and explicit non-claims.

- [ ] **Step 1: Fresh full gate sweep**

```powershell
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04-preflight.json --scope p04-scope.mjs
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs
node scripts/protocol-lock.mjs --check .
node scripts/verify-p01-text-hygiene.mjs --check .
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
node --test apps/console/tests/providerClientContract.test.mjs
npm --prefix apps/console run typecheck
cargo test --test compose_smoke -- --ignored --test-threads=1
git diff --check
```

Three warnings carried from P03's Task 13, because each cost real time there:

- **`cargo test --workspace` builds only the debug profile.** The release
  profile is compiled by exactly one gate in this repository, inside
  `docker compose up --build`. P03 shipped a tree whose release build failed and
  every other gate was green.
- **`include_str!` and `include_bytes!` create source-tree dependencies no
  manifest declares.** They are invisible to `cargo test` because the whole tree
  is present, and only a narrowed build context can notice. If this package adds
  one, the Dockerfile and `.dockerignore` must be updated in the same task.
- **Never filter test output through a `grep` that drops libtest's `failures:`
  block.** P03 nearly recorded a sensitive-label leak in the metrics endpoint
  that did not exist; the stack had simply failed to start.

- [ ] **Step 2: Repository and authority hygiene**

Recompute every protected-authority digest from the tree rather than trusting
the preflight, and reconcile each difference against
`protected_authority_revisions`. Confirm the q1 manifest and fixture and the
P01, P02 and P03 authority bytes are unchanged.

- [ ] **Step 3: Persist truthful scoped evidence**

State explicitly, separating claims checked in this package from claims carried:

- what P04 established: guarded embedding jobs, spaces and generations;
  transitions with recipe-granular classification, carry and barriers; and the
  retrieval generation fence;
- that embedding dispatch reuses P03's admission, credential-lease, throttle and
  recovery authorities and adds none of its own;
- whether a production path publishes a connection admission policy, and if not,
  that both Run-step and embedding dispatch remain unreachable in production;
- that P04 implemented no backup, restore or activation supervisor, no console
  screen, no AG-UI or A2A change, no real Agent publication, and called no LM
  Studio or remote provider;
- that loopback success is semantic development evidence, not real-provider or
  release evidence;
- that P02's pre-existing-table ownership and fingerprint backup obligations
  remain P05's;
- that G0 and v1.0 remain incomplete.

- [ ] **Step 4: Independent adversarial review**

A reviewer who did not write this package traces every acceptance criterion to
live code, migration ownership and ACLs, real PostgreSQL evidence and fresh
command output, and attacks specifically: the one-eligible-head rule, the
at-most-one-current-carry and one-current-barrier uniqueness, the ordered
subsequence property, the transition-scoped snapshot's unusability by ordinary
work, the retrieval fence's unrepresentability claim, and the P04/P05 boundary.
The verdict is `APPROVE` or `REVISE`; the package closes only on `APPROVE` with
no unresolved blocker.

- [ ] **Step 5: Operator handoff**

Report the scoped outcome and the remaining package count. Do not start P05
until P04 is operator-accepted, and do not call the result G0-complete or
v1.0-ready.

---

## Review checklist for this plan

This plan is not executable until an independent reviewer has traced it against
the frozen spec and the live repository. The reviewer is asked specifically to
check:

1. Every acceptance criterion cites a spec line that actually says it.
2. No task forks P03's dispatch, admission, recovery or throttle authority.
3. The task order has no forward dependency: nothing in Task N needs an
   interface first created in Task N+1.
4. The inherited defects in this plan's third section are each either closed by
   a named task or explicitly carried with a reason.
5. Tasks 4 to 12 are detailed to the depth of Tasks 1 to 3 before execution
   begins.
6. The file lists are complete enough that Task 1 can freeze a scope from them
   without a later amendment. P03 needed sixteen amendments; a plan that expects
   none is naive, but a plan whose file lists are visibly incomplete is not
   ready.
