# Vestrace v1 G0/P04 Completion Design

**Date:** 2026-09-08
**Status:** approved section by section; awaiting final document review
**Owner:** P04 embedding transition and retrieval completion
**Authority:** the approved v1 product design remains frozen and authoritative; this document specifies the implementation that closes its reopened P04 obligations.

## 1. Problem

P04 has a governed embedding-job foundation, durable transition planning and carry/barrier state, result-key preparation, encrypted result preparation, result finalization, credential-completion blockers, and a durable index-rebuild event. Those parts stop before a complete production path.

The current retrieval adapter still queries plaintext pgvector rows in `memory_embeddings`. Corpus-generation membership still points at those legacy rows. No production embedding executor consumes the governed jobs, no worker consumes index-rebuild events, no in-memory index is rebuilt after restart, transition completeness cannot be proved from the canonical encrypted projections, and retrieval-query jobs do not yet own attempt-scoped generation fences or durable results. Transition-aware activation, successful staged credential rotation, source/vector erasure propagation, and operator adoption of legacy data also remain incomplete.

P04 is complete only when all of those paths use one durable authority graph, survive real process faults, and leave no ordinary execution route through plaintext vectors.

## 2. Decisions

### 2.1 Canonical storage and index

`embedding_projection_entries` and their encrypted `ContentMaterial` vector payloads are the only canonical vector state. PostgreSQL stores ciphertext and structural metadata; it does not store a plaintext vector or stable vector digest.

The query index is a disposable, process-local, exact flat cosine index. It contains only one immutable generation, keyed by `(workspace_id, space_registration_id, generation_id, generation_epoch)`. It is rebuilt from the exact generation member set by decrypting only `Live` vector materials into bounded zeroizing memory.

The v1 flat index stores contiguous `f32` vectors, precomputed norms, projection identities, and the structural result references needed by the guarded result transaction. Search uses cosine distance and resolves equal scores by ascending space-local projection ordinal. It adds no disk cache and no external index service. HNSW or a remote index can be introduced later without changing the durable authority model.

### 2.2 Legacy cutover

`memory_embeddings` is an upgrade input, never a parallel production store. The first completion migration revokes its runtime mutation path. Ordinary retrieval from a legacy generation returns `embedding-legacy-adoption-required`; it does not fall back to pgvector.

Operator adoption does not copy an ungoverned plaintext vector into canonical storage. It creates governed `rebuild` jobs from the exact available `Live` source materials and recomputes each vector through the ordinary provider-effect path. A space switches only after a complete canonical generation is Ready. The guarded cutover then retires the legacy generation and deletes its live-database plaintext rows. P04 readiness remains blocked until every legacy space reaches that state and one installation-level `legacy_plaintext_retired` gate commits.

The product-managed backup authority belongs to a later gate-program package and does not yet exist in the repository. Its future creation precondition must require `legacy_plaintext_retired`, so no supported managed backup can begin while the live database contains a legacy plaintext vector. P04 does not claim to discover or erase operator-created, unmanaged database copies; upgrade guidance tells the operator to retire such copies explicitly.

An unavailable or erased source is a visible adoption blocker. The system does not fabricate a source, silently omit a member, copy an old vector, or publish a partial generation.

### 2.3 No mixed authority

Each generation has exactly one closed member representation: `legacy_upgrade` or `encrypted_projection`. A generation never mixes them. Only `encrypted_projection` generations may become active after adoption begins. Transition targets, ordinary delivery results, rebuild results, erasure, and retrieval-query fences all use the canonical projection branch.

## 3. Component architecture

### 3.1 PostgreSQL control plane

PostgreSQL owns all durable decisions:

- embedding jobs, their snapshots, external effects, reconstruction evidence, and terminal facts;
- encrypted vector materials, projections, and ordered source dependencies;
- corpus state, generation guards, generation member sets, and current-generation CAS state;
- transition plans, recipes, batches, physical attempts, satisfactions, barriers, carries, and activation receipts;
- retrieval-generation fences, canonical result references, generation-change observations, and retry edges;
- rebuild/invalidation events, build attempts, claims, observations, and safe failure reasons;
- legacy-adoption plans, source blockers, cutover facts, and the installation-level legacy-plaintext retirement gate.

Critical writes remain guarded-owner operations exposed through narrowly granted `SECURITY DEFINER` functions. Runtime roles receive only the reads and function executions needed by their application repositories. Raw INSERT, UPDATE, DELETE, TRUNCATE, trigger changes, and authority-altering grants remain unavailable.

### 3.2 Embedding executor

The embedding executor is the single production consumer for `delivery`, `rebuild`, and `retrieval_query` jobs. It reconstructs the exact pinned request, revalidates authorization, data policy, admission, credential/no-auth binding, owner, deadline, and external-effect state, then makes the one allowed provider call.

For delivery and rebuild, it sends the bounded response to the existing result-preparation, key-binding, and result-finalization chain. For retrieval-query, it retains the response vector only in zeroizing memory and enters the generation-fence finalizer.

The executor never uses configuration-only URL/model/secret values as routing authority. It resolves a pinned adapter route from `ModelBindingSnapshot` and its immutable revisions.

### 3.3 Result reconciler and finalizer

The existing output-key reconciler remains the only authority that binds provisional result keys. The existing result finalizer remains the only authority that promotes prepared attachments to `Live` projections, advances corpus state, stales a current generation, emits a durable rebuild event, and appends job success.

This completion work extends those authorities for transition recipe satisfaction and erasure propagation; it does not add a second publication route.

### 3.4 Index builder and local registry

The index builder consumes durable corpus-change work, captures the exact canonical member set, loads its encrypted vectors, and proposes an index generation. Its candidate is useful only after the generation guard accepts the captured corpus revision, watermark, member count, and material liveness.

The local registry is an in-process cache, not durable evidence. The database never claims that a particular process currently holds an index. A worker that lacks a locally loaded current Ready generation must rebuild that exact generation under its guard before running a query. Therefore another worker, a process restart, or a cache drop cannot reuse a stale copy merely because the durable generation row remains Ready.

### 3.5 Transition coordinator

The transition coordinator advances only existing durable plans. It creates initial and successor physical attempts from immutable batches, observes terminal attempts, records recipe satisfactions, classifies ambiguity carries and barriers, creates stale-safe later versions, and invokes activation only after the exact target bijection and Ready generation exist.

It does not resolve a current/default binding for transition work. Every transition job owns a fresh ordinary `ModelBindingSnapshot` constrained to its exact plan, version, batch, ordered recipes, and target tuple.

### 3.6 Erasure coordinator

The erasure coordinator extends the existing two-phase material-erasure authority. Phase one makes source and dependent vector materials unavailable, advances every affected corpus, revokes its generations, and commits invalidation events. Only after commit do workers remove local generations. Phase two witnesses vault erasure, removes ciphertext, and appends safe tombstones.

### 3.7 Worker composition

`vestrace worker` constructs six bounded cycles:

1. embedding job dispatch;
2. result-key reconciliation;
3. result finalization;
4. index build/local generation loading;
5. transition coordination;
6. source/vector erasure propagation.

Each cycle operates per configured workspace through bounded `FOR UPDATE SKIP LOCKED` claims. Claims carry an owner and deadline. Expired claims are recoverable without changing provider-effect semantics.

`--once` runs one bounded pass of every configured cycle. It returns a nonzero status when a cycle that attempted work failed, while durable pending work remains available for the next invocation.

## 4. Durable data model

All changes are forward migrations beginning after `0196`. Migrations `0001` through `0196` remain byte-identical.

### 4.1 Space keys and active heads

`embedding_space_registrations` is extended from its legacy name/model/dimension tuple to the full structural `EmbeddingSpaceKey`. Canonical rows name the exact embedding `ModelRevision`, `ModelQualificationRevision`, adapter/profile revision, request-shape revision, returned model, encoding format, and dimension. A closed registration kind distinguishes an incomplete `legacy_upgrade` row from a complete canonical row.

`model_qualification_heads` gains one nullable embedding-only field for the retrieval-active space registration. A deferred invariant requires an embedding head's qualification and active space to be mutually compatible. The head version is the compare-and-swap handle. Chat heads cannot name embedding space state.

The qualification head does not store a generation pointer. Result publication and erasure may lawfully leave the active space temporarily without a Ready generation while an asynchronous rebuild runs. The exact space's `embedding_index_generation_guards` row is the only current-generation authority, and retrieval acceptance must resolve it independently under that guard.

### 4.2 Corpus state and generations

The existing `embedding_space_corpus_states` remains the single monotonic corpus authority. It carries `corpus_revision`, next projection ordinal, and current live-member count.

The existing `embedding_index_generation_guards` remains the per-space epoch fence. It gains the current generation ID and an explicit CAS version while retaining its monotonic generation epoch.

The existing `embedding_corpus_generations` represents durable `EmbeddingIndexGeneration` metadata. It gains:

- `Building | Ready | Stale | Revoked` state;
- generation epoch and captured guard version;
- captured corpus revision;
- built-through projection ordinal;
- exact member count;
- member representation `legacy_upgrade | encrypted_projection`;
- safe lifecycle reason and timestamps.

At most one generation is Ready/current for a space. A Ready row whose ID is not the guard's current generation is invalid. Only a canonical generation whose captured fields equal the current corpus state may be published.

### 4.3 Generation members

`embedding_corpus_generation_members` changes to an ordinal-based identity and contains a strict member XOR:

- a legacy member names one `memory_embeddings` row; or
- a canonical member names one `embedding_projection_entries` row.

Partial unique indexes prevent duplicates within either branch. A deferred guarded-owner validator requires the branch to match the generation representation. Canonical members must name exact `Live` projections and `Live` vector materials in the generation's space, with unique projection ordinals and a complete source-dependency graph.

Legacy membership can be created only by the one upgrade-provisioning path. It cannot be extended after the completion migration begins.

The legacy branch retains the old embedding UUID as an opaque historical identity after cutover, not as a continuing foreign key to plaintext storage. Before retirement, the guarded validator requires the corresponding `memory_embeddings` row to exist and match its workspace and space. The cutover transaction records a retirement tombstone for that identity, removes the live FK dependency and generation member's executable legacy state, then deletes the plaintext row. After retirement no function may hydrate or query that identity.

### 4.4 Transition plans, recipes, and batches

The existing transition tables are extended rather than replaced.

Each plan version additionally fixes:

- transition kind `ordinary_refresh | rotation_staged`;
- exact source model and connection qualifications;
- exact source space, generation, epoch, corpus revision, projection watermark, and member count;
- complete source credential slot/guard/version lineage when credential-backed;
- complete target structural space tuple;
- the captured recipe count and restricted derivation version.

`embedding_transition_plan_recipes` gains the exact old projection identity and target input ordinal. `embedding_transition_recipe_dependencies` stores its ordered source material dependencies.

`embedding_transition_batches` gives every logical `TransitionBatchId` a durable header. `embedding_transition_batch_recipes` fixes its ordered recipes and wire input order. `embedding_transition_job_attempts` binds each fresh physical job and its fresh job-owned transition snapshot to one exact batch.

`embedding_transition_recipe_satisfactions` has a strict XOR:

- actual success: exact terminal job/input/projection/material tuple; or
- `SatisfiedExisting`: exact prior recipe and still-Live target projection lineage.

Unique and deferred constraints require one lineage-wide satisfier per required recipe and one recipe per target transition projection. Failed, cancelled, or ambiguous physical attempts cannot satisfy a recipe.

The existing carry and barrier tables continue to govern ambiguous predecessors. Their mappings are upgraded to reference the new batch and recipe identities rather than duplicating ordered arrays as independent authority.

Append-only transition observations record every state change, blocker, stale reason, retry, and activation result.

### 4.5 Retrieval fences and results

`embedding_retrieval_generation_fences` is one-to-one with a retrieval-query job. It names the exact space key, Ready/current generation ID and epoch, corpus revision, member watermark/count, and generation-guard CAS version captured by that attempt.

`embedding_retrieval_results` is one-to-one with a successful retrieval-query job. Ordered child rows name only canonical projection, source material/memory revision, rank, and safe score evidence. They contain no query vector, stored vector, or vector digest.

`embedding_retrieval_generation_changes` is one-to-one with a retrieval job that received a definite provider response but lost its captured generation race. It records a closed safe reason and the observed current generation metadata.

`embedding_retrieval_generation_retries` records the authorized predecessor/successor edge, expected predecessor version, idempotency identity, acknowledgement Audit event, and warning version. A predecessor has at most one direct successor. The retry edge is distinct from the existing retry-after-Unknown edge; a job cannot use both.

### 4.6 Index change events and attempts

The existing `embedding_index_rebuild_events` becomes the one append-only corpus-change stream. Existing publication rows retain their exact circular publication/event relationship. New rows use a closed cause XOR such as result publication, source/vector erasure, transition publication, legacy cutover, or operator rebuild. Cause-specific deferred checks allow member counts to increase, decrease, or remain stable only where that cause permits.

`embedding_index_build_attempts` records claim owner/deadline, source event or startup/lazy-load cause, captured generation snapshot, terminal state, safe error, and the generation it proposed or loaded. Duplicate local builds are safe; only the generation guard publishes current durable state.

No row records vector bytes, an in-memory pointer, or an assertion that a process still has the generation loaded.

### 4.7 Legacy adoption

`embedding_legacy_adoptions` is an upgrade-only durable plan keyed by workspace and legacy space registration. It fixes the legacy generation/watermark, target canonical space, source inventory, target jobs, target generation, state, blockers, and cutover version.

Its member rows map each legacy row to the exact current `Live` source materials from which a governed rebuild job is created. This map is provenance for migration work, not permission to reuse a vector.

The cutover receipt fixes the canonical generation, deleted live-database legacy-row count, qualification-head version, and Audit identity. Adoption reaches terminal `Completed` only when the canonical cutover committed and its live-database legacy rows are absent. The installation-level `legacy_plaintext_retired` gate commits only when every registered legacy space is Completed and a database-wide guarded scan finds no `memory_embeddings` rows. Future product-managed backup creation must require that gate. Unmanaged backup disposal remains an explicit operator upgrade obligation and is not represented as completed product evidence.

### 4.8 Staged credential rotation

An immutable transition activation receipt links the exact source and target credential/no-auth branches, acquired guard set, pre/post slot versions, credential activation/retirement events, qualification-head CAS versions, target space, and target generation.

For a credential-backed source, activation must prove that every result-completion blocker referencing the retiring credential is already adopted by its exact durable result or lawfully terminalized. Retirement does not authorize destruction. The existing retired-credential erasure authority still refuses destruction while any job, effect, lease, prepared result, source dependency, transition, or completion blocker can require that credential.

## 5. Execution flows

### 5.1 Delivery and rebuild

The established chain remains:

`Requested -> output-key intents -> Dispatching -> provider response -> ResultPrepared -> key binding -> Live projections -> Succeeded`.

The provider is never called again after a crash in or after `Dispatching` unless an authorized acknowledgement creates a fresh successor job/effect. A committed `ResultPrepared` marker can only bind and finalize; it cannot be abandoned.

### 5.2 New generation publication

1. A builder claims corpus-change work.
2. Under the generation guard it records a `Building` generation and captures corpus revision, watermark, ordered canonical members, and material identities.
3. Outside a long database transaction it loads vectors in bounded chunks. Every chunk uses the canonical space/material lock order and accepts only `Live` material.
4. It constructs a zeroizing flat-index candidate under the new generation identity.
5. It reacquires the exclusive generation guard, revalidates the complete snapshot, and attempts the database CAS. Successful CAS marks the generation Ready/current and stales the prior generation; a failed CAS zeroizes the losing candidate.
6. After a successful commit, the winner revalidates that exact Ready/current snapshot, installs the candidate in its private registry, and revalidates once more. Any mismatch removes and zeroizes the candidate. A crash between durable publication and local installation leaves only a registry miss, which the guarded local-load path repairs.
7. Invalidation notifications cause every process to drop older local generations.

A process that restarts or never built the generation has no local index. Before it queries an existing Ready/current generation, it performs a local load attempt: capture that exact generation under its guard, rebuild it, reacquire and revalidate the same guard state, then install it locally. This does not create another durable generation or change the epoch.

### 5.3 Transition rebuild and activation

The qualification finalizer stores a successful new embedding qualification as non-effective and atomically creates a transition plan when a live old corpus exists. The plan captures every old Live projection as a distinct recipe, including repeated dependency sets.

The coordinator creates batches and fresh physical rebuild jobs. Each terminal successful projection can satisfy only its assigned recipe. Ambiguous attempts enter the existing carry/barrier flow and never satisfy a recipe.

When all currently required recipes have exactly one valid satisfier, the coordinator builds a target generation from the same set. The guarded completeness transaction compares recipes, satisfactions, target projections, generation members, corpus revision, watermark, and counts. Only an exact bijection moves the plan to `ReadyToActivate`.

Activation acquires `ConnectionExecutionGuard`, each distinct credential activation guard once in canonical slot order, then source and target space/generation guards. It revalidates the old head, source recipe set, source/target auth XORs, target qualification, target Ready/current generation, and every activation blocker. One CAS changes the effective qualification and retrieval-active space while the target generation guard proves its exact generation remains Ready/current; the transaction records that generation in the activation receipt and marks the transition Activated.

An addition, erasure, source-dependency change, qualification expiry, credential change, or corpus mutation before activation makes the version Stale. A later version may carry only exact still-Live `SatisfiedExisting` lineage and creates new recipes/jobs for every other required member.

### 5.4 Rotation-staged activation

A candidate-bound successful embedding qualification creates a `rotation_staged` transition. Only that transition's exact job snapshots may use the Candidate credential. Ordinary jobs, defaults, retrieval queries, and qualification heads cannot select it.

The final activation transaction performs the transition checks, proves result-completion adoption for the old credential, activates the candidate, retires the old credential, and switches qualification plus retrieval-active space while holding and revalidating the target generation guard. If any branch, slot version, material-erasure state, revocation, completion blocker, or generation predicate changed, the transaction performs none of those mutations.

### 5.5 Retrieval query

Acceptance resolves a fresh current ordinary `ModelBindingSnapshot` and atomically creates the retrieval job, effect, model-request evidence, and `RetrievalGenerationFence` under the exact generation guard.

After the one provider response, the worker reacquires that guard and holds it through local index search and the result transaction. There are two legal outcomes:

- if the fence still matches, it queries the exact local generation, revalidates every selected projection/vector/source as `Live`, and atomically writes ordered result references, the definite provider receipt, and job `Succeeded`;
- if generation state changed first, it atomically writes the definite provider receipt, `RetrievalGenerationChanged`, and job `FailedDefinite`, with no result.

The transient query vector is zeroized in both cases. A crash before either terminal transaction leaves the already-Dispatching effect deadline-gated for Unknown recovery and does not cause an automatic provider retry.

The confirmed retry command requires `embedding.retry_retrieval_generation_changed`, a matching expected version, and an idempotency key. It warns of one additional provider call/charge and creates exactly one fresh successor with a new snapshot, fence, effect, evidence, and nonces.

### 5.6 Source and vector erasure

Phase one follows the common lock order, marks the source and every dependent projection/vector material unavailable, advances affected corpus revisions, revokes current generations, and commits invalidation events before `MaterialErasurePrepared`. A transition using any affected recipe becomes Stale in that same authority chain.

Post-commit event consumption drops all affected local indexes. Queries still independently reacquire the database guard, so a delayed notification cannot make a retained index usable.

After witnessed vault erasure, phase two deletes ciphertext, records safe tombstones, resolves blockers, and permits a later generation build only from the remaining Live member set.

### 5.7 Legacy adoption

The operator command creates or resumes one adoption plan per legacy space. It validates current source mappings, reports blockers, and creates bounded governed rebuild jobs. Worker restart resumes those jobs and their existing effects; it does not enqueue duplicate logical members.

Once all required sources have Live canonical projections, the normal generation builder publishes the target generation. A guarded cutover sets the canonical active space while proving the target generation Ready/current under its own guard, marks the legacy generation stale, deletes the live legacy vectors, and records exact counts. Installation readiness remains incomplete until every legacy adoption is Completed and the guarded database-wide scan commits `legacy_plaintext_retired`.

## 6. Failure and concurrency rules

- Provider effects retain the existing at-most-once Dispatching authority. Worker recovery never invents a result and never retries automatically after Dispatching.
- Claims are work-distribution hints. They do not authorize a provider call, publish a generation, satisfy a recipe, or activate a transition.
- Concurrent builders may duplicate local computation. Only one exact CAS can publish; every loser zeroizes its candidate.
- A local registry miss is fail-closed and triggers a guarded local load. It never enables pgvector fallback.
- A database Ready generation cannot by itself prove that any process has an index loaded.
- A retained local index cannot be queried without a matching current database epoch, corpus revision, generation ID, and Live selected members.
- Transition retries always receive fresh physical job, snapshot, effect, evidence, nonce, and output-intent identities.
- A failed, cancelled, Unknown, or InconclusiveUnknown attempt cannot satisfy a transition recipe.
- Erasure and activation serialize through the same space/generation/material guards, so exactly one ordering becomes durable.
- Every idempotent command returns its one prior result for the same key and conflicts on a different key after the unique successor/cutover already exists.

## 7. Operational surface

### 7.1 Worker and rebuild command

The legacy `EmbedMemoryHandler`, `PgEmbeddingStore` write path, and direct-provider implementation in `vestrace rebuild embeddings` are retired.

Memory-created and memory-revised outbox messages enqueue governed delivery jobs. `vestrace rebuild embeddings` creates or resumes durable adoption/rebuild plans, prints plan identities, counts, progress, and blockers, and optionally waits by observing durable state. It never calls the provider itself.

### 7.2 Retrieval integration

HTTP and MCP preserve their synchronous retrieval result shape through an `EmbeddingRetrievalJobClient`. It accepts the durable retrieval-query job and waits only within the request budget. A terminal vector result participates in fusion. A pending, failed, missing-local-index, legacy-adoption, or transition-not-ready result is recorded as an exact degraded vector-channel outcome; the service never performs an inline fallback provider call.

The logical retrieval request identity is generated once at service admission and is the idempotency cause for the job. Protocol request identifiers may be mapped only through the existing untrusted-ID fingerprint boundary; they are never database authority by themselves.

### 7.3 Commands and visibility

The existing acknowledgement routes for ordinary Unknown and transition carry remain. The completion adds the confirmed generation-changed retry command and guarded transition resume/rebuild commands.

Read-only job, transition, generation, and adoption views expose identities, states, epochs, counts, attempt lineage, progress, blockers, and safe failure reasons. They never expose vector plaintext, vector digests, ciphertext, credential material, or a claim that a database transaction changed another process's memory.

Doctor, readiness, and Models use exact reasons including:

- `embedding-legacy-adoption-required`;
- `embedding-index-not-loaded`;
- `embedding-generation-not-ready`;
- `qualification-transition-not-ready`;
- `embedding-erasure-pending`;
- `embedding-index-build-failed`.

Metrics contain bounded safe counts and durations only: queue depth and age, build attempts, CAS discards, local loaded generations, stale/revoked events, transition blockers, and adoption progress.

## 8. Security and lock order

The canonical critical order is:

1. installation mutation permit;
2. `ConnectionExecutionGuard`;
3. every distinct `CredentialActivationGuard` and slot in canonical slot order;
4. source and target space corpus/generation guards in canonical space order;
5. content-material guards in canonical material-ID order.

Provider calls and index construction do not hold long database locks. Each pre-dispatch or hydration transaction acquires only the guards required for its bounded decision or chunk. Publication and activation reacquire and revalidate the full exact snapshot before committing.

Plaintext credential, source, provider body, response vector, query vector, and local index bytes are bounded and zeroized after use. PostgreSQL, provider evidence, Audit, logs, and tombstones contain no plaintext vector or stable vector digest after adoption. The durable `legacy_plaintext_retired` gate prevents the later product-managed backup authority from creating its first backup before that condition holds.

## 9. Verification

### 9.1 Schema and upgrade

- fresh installation and non-superuser runtime provisioning;
- real `0196 -> latest` upgrade with seeded legacy and canonical states;
- RLS/FORCE RLS, exact owners, ACLs, grants, and raw-write refusals;
- closed-enum parity between Rust and SQL;
- exact composite foreign keys and deferred invariants;
- zero hash differences for migrations `0001` through `0196`.

### 9.2 Generation and index

- nonempty and zero-member build, CAS publication, local load, and query;
- concurrent builders with one durable winner and zeroized losers;
- corpus mutation, transition publication, and erasure invalidation;
- real process restart with an empty registry and guarded reload;
- refusal on wrong epoch, revision, space, dimension, member set, or material state;
- bounded-memory failure without a Ready publication.

### 9.3 Transition and rotation

- ordinary and rotation-staged plans across credential/no-auth combinations;
- exact success and `SatisfiedExisting` recipe paths;
- omitted, duplicate, reordered, wrong-batch, wrong-space, and wrong-lineage refusals;
- stale-version catch-up after addition, removal, erasure, expiry, and credential change;
- atomic qualification/active-space activation while the exact target generation guard remains locked and Ready/current;
- successful rotation-before-adoption proof and refusal of premature credential destruction.

### 9.4 Retrieval fence and retry

- both query-first and generation-change-first linearization orders;
- no persisted query vector or digest;
- selected-member Live revalidation;
- exactly one authorized successor and idempotency conflict behavior;
- exact provider-call counts proving no automatic retry.

### 9.5 Legacy adoption and retention

- legacy runtime writes and ordinary retrieval refused;
- governed recomputation from Live sources;
- restart/resume without duplicate logical members or provider effects;
- no partial or mixed-generation cutover;
- primary database and test dump scans for canary plaintext vector and stable digest;
- the installation retirement gate cannot commit until every adoption is complete and a guarded database-wide scan finds zero legacy vector rows;
- upgrade guidance explicitly distinguishes unsupported unmanaged copies from future product-managed backup evidence.

### 9.6 Real fault suite

The fault harness terminates a real worker child after claim, after Dispatching, after provider response, after ResultPrepared, after key receipt, before and after index CAS, and before and after activation commit. Each scenario verifies restart, durable resumption, duplicate-claim handling, provider-call count, one terminal result, and absence of mixed or prematurely visible state.

### 9.7 Mutation qualification

Focused mutations remove or weaken the generation fence, corpus CAS, Live predicates, recipe bijection, credential lineage, completion blockers, erasure invalidation, legacy-write guard, and one-successor rule. Every unchanged probe must return a nonzero RED result. Each mutation is restored byte-exactly before the GREEN run.

Evidence distinguishes mutations that expose an unsafe persisted state from defense-preserving mutations stopped by an independent invariant. Both require an unchanged probe to fail and retain original/mutant/restored hashes, exact exits, and authority comparisons.

### 9.8 Final gates

The final acceptance includes all targeted domain, application, infrastructure, CLI, HTTP, and MCP suites; the complete ignored embedding fault suite; `cargo fmt --all -- --check`; pinned Rust 1.85.0 Clippy with `-D warnings`; P02/P03/P04 scope tests; dirty-baseline verification; protocol-lock verification; and a requirement-to-evidence matrix for the frozen embedding clauses.

P04 evidence must contain no remaining `Not true yet` item for embedding production execution, transition activation, retrieval fences/results/retry, index rebuilding, erasure propagation, staged rotation, operator adoption, or worker fault recovery.

## 10. Implementation scope

The implementation plan may activate only the paths needed in these groups:

- new forward migrations after `0196` and the runtime-role provisioner;
- embedding domain and application modules;
- PostgreSQL embedding, transition, retrieval, erasure, and vault repositories;
- worker, rebuild, server, MCP, HTTP, OpenAPI, readiness, and doctor wiring;
- embedding fault scenarios and targeted P04 integration/contract tests;
- P04 preflight, scope tests, scope manifest, and development evidence;
- this approved design and its later approved implementation plan.

Historical migrations, the frozen full-product specification, protocol-lock authority, and unrelated UI/product modules remain protected. Exact paths and the clean baseline are frozen in a new preflight before implementation begins. Any path absent from that approved preflight requires a separate scope amendment.

## 11. Completion boundary

This design closes the reopened P04 package. It does not by itself declare the entire v1.0 gate program complete. Later packages may rely on P04 only after all verification and mutation evidence above passes and the final P04 evidence explicitly records acceptance.
