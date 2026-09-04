# V1 G0-04 — Embedding and transition foundation

Evidence for `docs/superpowers/plans/2026-09-03-vestrace-v1-g0-04-embedding-transition-foundation.md`.

This document records what was done, what was found, and what is not true yet.
It is written as the work happens rather than assembled afterwards, because a
document assembled afterwards records what the author remembers rather than what
the run produced.

## Standing conditions this package ran under

- No commit, no push, no deploy, no call to a model provider, and no external
  network call.
- Migrations 0172–0186 are historical and immutable. P04's forward migrations
  begin at 0187.
- Every changed path is inside `scripts/p04-scope.mjs`. Every unrelated dirty
  file in the working tree is preserved byte for byte, which
  `scripts/verify-dirty-baseline.mjs` checks against the captured preflight.

## Two deviations from the gate program, both authorized, both recorded here

**The plan was executed without independent review.** The program's rule 4
requires a plan to be independently reviewed before execution. The operator
directed execution to begin immediately (`приступай к исполнению`, 2026-09-03).
The review has not happened. Its absence is visible in this package's history:
four scope amendments were needed for file lists the plan itself had left
incomplete, and the plan's own review checklist had warned that a plan whose
file lists are visibly incomplete is not ready. A reviewer would have caught
that on paper for nothing; it was instead caught four times by failing tests.

**P03 Tasks 11–13 were never independently reviewed either.** P03 is
operator-accepted. The adversarial review recorded as due at the end of P03 was
not run. That is stated here so a later reader does not infer from P03's
acceptance that it was reviewed.

## Task 1 — Frozen scope and its verifier

`docs/development-evidence/v1-g0-04-preflight.json` captured the working tree
before any P04 change: 343 dirty entries, 52 change paths, 23 protected
authority paths, each protected path pinned by SHA-256.

`scripts/p04-scope.mjs` is generated from that preflight so the two cannot
disagree by inattention. `tests/p04_scope.test.mjs` has six tests. The decisive
one builds a throwaway repository whose entire dirty set is the protected
authority, runs the verifier over it, then breaks one specific protected file
and requires the verifier to reject it. The `mutate` parameter exists so that
file can be named: it is `scripts/verify-dirty-baseline.mjs`. Mutating
`protectedPaths[0]` would have passed while leaving the package's central
protection untested.

`scripts/verify-dirty-baseline.mjs` is protected authority from P04 onward. P03
was permitted to edit it and generalised its scope dispatch to any
`pNN-scope.mjs`; that generalisation is why no later package needs to touch it.

### Four scope amendments

| # | Added | Size | Cause |
|---|---|---|---|
| 1 | `crates/vestrace-domain/src/id.rs` | 52 → 53 | The `domain_id!` macro carries no `#[macro_export]`, so the three new identities could not be declared outside the file that already holds `EmbeddingSpaceId`. |
| 2 | 32 paths across six crates | 53 → 85 | Task 3's new guarded tables and functions failed `p03_upgrade_provisioning`, which is P03's own defence against an allowlist growing silently. The assertion was working; the file lists were wrong. Enumerated from the tree in one pass under the operator's decision to stop and complete the lists rather than amend once per failing test. |
| 3 | The four connection publisher paths | 85 → 89 | The admission policy had no producer at all (below). |
| 4 | `crates/vestrace-http/src/route_inventory.rs`, `crates/vestrace-cli/src/commands/schema.rs` | 89 → 91 | The route the third amendment authorised in as many words cannot exist without them. |
| 5 | `crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs` | 91 → 92 | The only file the shared dispatch trait can be implemented in. Caught by the verifier, not by a test. |
| 6 | `crates/vestrace-infrastructure/tests/common/mod.rs` | 92 → 93 | The fixture two embedding suites share. A judgement, not a necessity: the alternative compiles and drifts. |

`tests/p04_scope.test.mjs` asserts the change-scope size as a literal, so scope
that grows without an amendment fails there. It caught the fourth amendment.

## Task 2 — The embedding vocabulary, before any table

Every closed set in `crates/vestrace-domain/src/embedding/` is quoted from the
frozen spec with the line it came from, not designed:

- transition lifecycle `Planned -> Rebuilding -> ReadyToActivate -> Activated |
  Stale | Failed` — line 203;
- carry header and per-recipe mapping states — line 253;
- barrier lifecycle and "its entire dedicated batch is nondispatchable and
  completeness-blocking" — line 256;
- generation `Ready`/`Stale` — lines 207 and 219;
- job kinds `retrieval_query`, `delivery` and `rebuild`, and the six append-only
  job states — line 219, **after the correction recorded below**; the sets Task 2
  first declared were wrong;
- the recipe id is an identity and never a hash — line 205.

`crates/vestrace-domain/tests/embedding_contract.rs` has ten tests, each quoting
the line it enforces. A state that cannot be traced to one of those lines does
not belong in these types, and a state named there and missing here is a defect.

### Findings inside my own Task 2 work

**A `compile_fail` doc test in an integration test file asserts nothing.** The
proof that `EmbeddingSpaceKey` is not constructible from untrusted input was
first written as a `compile_fail` block in `embedding_contract.rs`. Doc tests do
not run for integration test targets, so it looked like a proof and was inert.
It was moved onto the type in `space.rs`, where it executes, and the test file
now says why it lives there.

**`EmbeddingJobState` was planned and not implemented.** It was added with an
`ALL` constant so the SQL `CHECK` constraint and the enum cannot drift: a state
added to one and not the other fails the schema contract test rather than a
production insert.

**An inherent `from_str` shadows `FromStr::from_str`.** Clippy's
`should_implement_trait` is right; two functions with the same name and
different return types on one type is a trap. `EmbeddingJobKind` implements
`FromStr` with a typed error that names the offending value, so a misspelled
column value is refused by name rather than treated as absent.

## Task 3 — Migration 0187

Three guarded tables (`embedding_space_registrations`,
`embedding_corpus_generations`, `embedding_jobs`), forced RLS, guarded-owner
ownership, and two partial unique indexes that carry invariants the application
must not be trusted to hold:

```sql
CREATE UNIQUE INDEX embedding_corpus_generations_one_ready_per_space
    ON embedding_corpus_generations (workspace_id, space_registration_id)
    WHERE state = 'ready';

CREATE UNIQUE INDEX embedding_jobs_one_successor_per_predecessor
    ON embedding_jobs (workspace_id, retries_unknown_embedding_job_id)
    WHERE retries_unknown_embedding_job_id IS NOT NULL;
```

The second is line 251's rule: one ambiguity head gets one direct successor.

`crates/vestrace-infrastructure/tests/embedding_schema_contract.rs` has six
tests. `p03_upgrade_provisioning`'s `EXPECTED_GUARDED_TABLES` went 66 → 69.

### Two defects in my own closed-world test, found by breaking it

The test `every_runtime_executable_guarded_function_is_declared_in_the_bootstrap`
reads `docker/postgres/init-runtime-role.sh` and requires every runtime-callable
guarded function to appear in the bootstrap allowlists. Twice it was wrong:

1. **It searched the whole file.** A deliberate break passed straight through
   it while the provisioner failed, because a name occurring anywhere in the
   script satisfied it. The bootstrap has two distinct `REGPROCEDURE[]` arrays —
   `allowed_targets` (ownership transfer) and `runtime_executable_targets`
   (execute grant) — and a function needs to be in both. The test now requires
   membership in a region of each kind.
2. **`array_region` matched the first declaration only**, which belongs to a
   different bridge, so it reported every entrypoint missing. Corrected to
   `array_regions`, collecting all regions of each kind.

Both were found by breaking the thing under test and requiring the test to
notice, which is the only way a closed-world assertion can be trusted.

## Task 4 — The admission policy had no producer

### What was found

Searching the tree rather than carrying P03's note forward: the connection
admission policy has a vocabulary and a consumer and **no producer at any
layer**.

- `ConnectionAdmissionPolicy` and `ConnectionAdmissionPolicyId` are declared in
  `crates/vestrace-domain/src/models/admission.rs`.
- `vestrace_try_admit_provider_dispatch` refuses a dispatch whose Connection has
  no current policy revision.
- Nothing wrote `connection_admission_policy_revisions` or
  `connection_admission_policy_heads`. No route, no application service, no
  repository, and no guarded SQL function.
- Six test files inserted the two rows with raw SQL as `vestrace_guarded_owner`,
  because there was no other way.

The consequence is not confined to this package: **both Run-step and embedding
dispatch were unreachable in production**, while the suite reported provider
dispatch working — because every admission test seeded, as the guarded owner, a
policy that no deployment could ever have written.

This is P03 subject matter completed inside P04 under an explicit operator
decision (`Построить публикатора`, 2026-09-03). The evidence says so rather than
presenting it as P04's own design.

### What was built

**Guarded function** `vestrace_publish_connection_admission_policy` in migration
0187. It takes the permanent `connection_execution_guards` row `FOR UPDATE`
first — the canonical lock order for any Connection-scoped mutation — then
compare-and-swaps the policy head. `expected_version = 0` means "there is no
policy yet", the same convention the connection revision head uses; a mismatch
raises `40001`, and a Connection with no execution guard raises `23514`. Its
comment records what it is *not*: a policy publisher, not a routing,
qualification, or credential authority. Ownership is transferred and `EXECUTE`
is revoked from public and granted to the runtime role, and its name is entered
in **both** bootstrap allowlist arrays.

**Application command** `PublishConnectionAdmissionPolicy` and the trait method
`publish_admission_policy_governed`, whose default implementation refuses with
`ApplicationError::Unavailable`. A caller that believed an unconfigured
authority had published would go on to dispatch against a policy nobody wrote.

**Repository** implementation on `PgConnectionRevisionRepository`, through the
same governed-mutation boundary as connection revisions, with no read of the
current head: the caller states the version it saw, and the function compares.

**Route** `POST /v1/connections/{id}/admission-policies`, governed,
`WorkspaceAdmin`, `Critical`, documented in the served OpenAPI document with the
exact bounds the domain enforces.

### Finding: the domain type would deserialize past its own validation

`ConnectionAdmissionLimits` derives `Deserialize` over private fields. Serde's
derive constructs the struct directly, so a body deserialized straight into that
type bypasses the bounds `ConnectionAdmissionLimits::new` enforces — a policy
with zero in-flight dispatches would admit nothing and stall every dispatch on
the Connection, and would be stored rather than refused.

`crates/vestrace-domain/src/models/admission.rs` is **not in P04's change
scope**, so the derive was not removed. Instead the route declares its own
plain-integer request shape and calls `new`, and three tests hold that line: an
HTTP test that a zero `max_in_flight` is a 400 that never reaches the
application, and an OpenAPI contract test that the published document states the
same bounds `new` will accept, so an operator learns them from the document
rather than from a refusal. **Removing that derive is unfinished work for a
package whose scope includes that file.**

### Finding: `schemas/openapi-v1.json` is documentation that verifies nothing

The plan names `schemas/openapi-v1.json` as the OpenAPI document to modify. It
is not the served schema. It has not changed since the initial commit, carries
33 paths against the CLI document's full set, names only `/v1/connections` among
the connection routes, and no test, build step, or console module reads it. The
served document is built by `crates/vestrace-cli/src/commands/schema.rs` and
read by `crates/vestrace-cli/tests/provider_openapi_contract.rs` through
`vestrace schema http`. That is what this task modified. Adding a path to the
static file would have looked like documentation and verified nothing.

A related, larger gap was observed and **not** fixed: the served document's
existing request schemas do not match their handlers.
`CreateConnectionRevisionRequest` documents three fields where the handler
requires thirteen, and `/v1/connections/{id}/revisions` documents a
`GovernedConnection` response where the handler returns a governed mutation
receipt. Only the newly added path and component were written accurately.

### Finding: the guarded function's narrow types are a footgun for hand-written SQL

`max_in_flight` is `SMALLINT` and the head version is `BIGINT`. PostgreSQL will
not narrow an `INTEGER` literal to `SMALLINT` while resolving an overload, so a
hand-written call with bare integer literals fails with `42883 function ... does
not exist` rather than calling the function. This was found by six suddenly-red
tests, not by reasoning. The repository is unaffected because sqlx binds typed
parameters; the fixture now casts explicitly and says why in a comment.

### What the tests prove, and how the seeding gap was closed rather than repeated

The plan forbids seeding the policy in a fixture and reporting the dispatch path
as proved, because P03's evidence already records that seeding as a gap and
repeating it would launder a known gap into a claim. So the fixture was changed
rather than supplemented: `qualification_fixture` in `provider_admission.rs` no
longer inserts the two rows as the guarded owner. It calls the guarded function
**as the restricted runtime role**, against a head that does not exist yet, and
asserts the returned version is 1.

Every admission test in that file therefore now runs against a policy written by
the production publisher. Three further tests exercise the whole Rust stack over
a Connection created moments earlier by the same repository:

- `a_fresh_connection_can_be_given_its_admission_policy` — the head advances to
  version 1 pointing at the caller's revision id, and the stored limits are the
  ones the caller stated.
- `a_publisher_that_did_not_see_the_current_policy_loses` — a second publisher
  stating version 0 is refused with
  `CONNECTION_ADMISSION_POLICY_VERSION_CONFLICT` and leaves no revision behind;
  the publisher naming version 1 succeeds and the superseded revision stays as
  history.
- `a_connection_without_an_execution_guard_gets_no_policy` — a policy for a
  Connection that cannot execute is refused, so no policy row outlives the
  Connection it claims to govern.

### Runs

Against the local PostgreSQL container (`127.0.0.1:55432`, restricted runtime
role `vestrace`), 2026-09-03:

| Suite | Result |
|---|---|
| `vestrace-infrastructure --test provider_admission` | 12 passed, 0 failed |
| `vestrace-infrastructure --test embedding_schema_contract` | 6 passed, 0 failed |
| `vestrace-infrastructure --test connection_mutation_is_atomic` | 5 passed, 0 failed |
| `vestrace-cli --test provider_openapi_contract` | 3 passed, 0 failed |
| `vestrace-cli --test provider_runtime_wiring` | 4 passed, 0 failed |
| `vestrace-http` (all targets) | 77 passed, 0 failed |
| `vestrace-integration-tests --test http_command_contract` | 1 passed, 0 failed |
| `node --test tests/p04_scope.test.mjs` | 6 passed, 0 failed |
| `cargo clippy --workspace --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

A full workspace run has **not** been done since Task 4. It is owed before this
package is reported complete.

## Task 4, Steps 1 and 2: rediscovery and pre-dispatch recovery

### Step 3 first, because it decides whether the rest means anything

The plan requires this task to record which of two things is true, with
evidence, rather than seed a policy in a fixture and call the dispatch path
proved. The answer is the first: **the policy is published by a route this task
added, and that route is what the admission fixture now calls.** The publisher
is recorded above; `provider_admission`'s fixture calls the guarded function as
the restricted runtime role instead of inserting the two rows as the guarded
owner. Run-step and embedding dispatch are both reachable through a producer a
deployment has.

### Step 1: RED, and red at the right line

Three types beside their Run-step twins in
`crates/vestrace-application/src/provider_dispatch.rs`:
`EmbeddingJobExecutionAttempt` (the durable `embedding_jobs` row and nothing
else), `EmbeddingJobDispatchPlan`, and `EmbeddingJobAttemptRecovery` with the
same seven shapes as `RunStepAttemptRecovery`. Two methods on
`ProviderDispatchRepository` itself, each with a fail-closed `Unavailable`
default.

The suite asserts `Unavailable` **specifically**, not merely an error, because a
default that refused with the wrong variant would look red for the right reason
and later green for the wrong one: `Unavailable` tells a worker the authority is
not configured here, where a `Storage` would read as transient and be retried.
`the_embedding_methods_are_reachable_only_through_the_shared_trait` calls both
through a `&dyn ProviderDispatchRepository`, so Step 4's "no parallel trait" is
checked by the compiler rather than by a reviewer's memory.

RED landed where it should: the fixture ran end to end — workspace, connection,
revision, no-auth binding, model revision, space registration, effect intent and
the new guarded `vestrace_accept_embedding_job`, all as the runtime role — and
the two tests failed at the fail-closed default, naming it.

### The effect vocabulary has no embedding shape, and `workspace://` is the honest one

`ExternalEffectIntent` closes its execution reference over `run://<id>`,
`execution://<id>` and `workspace://`. An embedding delivery job is, in line
219's words, "the durable non-Run owner for production Embeddings" — there is no
run behind it. P03's qualification probe, also a non-Run owner, already says
`workspace://`, and the domain's own comment explains why: it is deliberately
not routable, and saying so is more honest than inventing a synthetic run.

Adding `embedding://` was rejected. The reference names an *execution*, not an
owner, and the job row references the effect rather than the other way round; a
new scheme would widen a closed vocabulary to record something it does not mean.

### Step 2: the phase is derived, not stored

`vestrace_accept_embedding_job` takes the canonical lock order and no other:
`ConnectionExecutionGuard`, then the optional exact `CredentialActivationGuard`,
then the exact `(workspace, EmbeddingSpaceKey)` registration, then the job. It
resolves nothing — the caller states the snapshot, the effect identity and the
evidence identity, and the function proves they are consistent. A function that
looked up "the current snapshot" would be routing, which the worker may not do.
Line 257's successor rule is enforced there too: a job may only name a
predecessor that is terminal `inconclusive_unknown`.

`vestrace_lock_embedding_job_recovery_authority` differs from its Run-step twin
in one deliberate way. The twin reads a persisted `phase` column on
`run_step_execution_attempts`; this one derives the phase from the evidence —
the admitted admission, the dispatch transition, the receipt, the result
preparation, and the job's own terminal state. That follows directly from the
correction recorded above: the job has six states and none of them is
`dispatching`, so a phase column on `embedding_jobs` would be a second,
independently writable answer to whether the provider had been reached, and the
job could then disagree with the effect about a possible charge.

Because the phase is derived, a derivation defect would surface as a *wrong
answer* rather than an error — and a wrong answer here means telling a worker
that a charge is accounted for when nothing wrote it down. So every arm in the
Rust classifier re-states the whole evidence tuple it expects, and a tuple that
does not match any arm is a refusal.

`a_job_claiming_unknown_without_a_dispatch_transition_is_refused` forces exactly
that mismatch: a job whose state says the effect ended ambiguously, with no
dispatch transition to have been ambiguous about. The guard was verified as
load-bearing by breaking it — dropping `dispatch_transition_id.is_some()` from
the `unknown` arm turns that test red and leaves the other six green — and the
file was then restored byte-identically, digest
`20c44f25d6ba8b0dbdabce89dd0bb9be1f6a80722f78fb46b2a1b9833b526b9c` before and
after.

One arm refuses rather than answering: a `dispatching` job past its deadline
needs lost-dispatch adoption, and that leg does not exist yet. It returns
`Unavailable` naming the missing adoption. Reporting `AdoptedUnknown` without
adopting would tell a worker a possible charge had been accounted for when
nothing had written it down.

### The fifth scope amendment, and what caught it

`crates/vestrace-infrastructure/src/postgres/provider_dispatch_repository.rs`
was not in the change scope. It is the only place the two methods can be
implemented: `PgProviderDispatchRepository` is the single implementation of the
trait, Rust permits one `impl Trait for Type` block, and the alternative — a
second Pg type implementing the same trait for embeddings — is precisely what
Step 4 asks a reviewer to confirm did not happen. Scope 91 → 92.

Worth noting against the CRLF defect above: **the verifier caught this one
immediately**, with `baseline dirty path changed`. That is the same tool, on the
same run, behaving correctly — it pins a digest for every baseline dirty file
and for every protected path. What it does not do, and did not do for the CRLF
rewrite, is check the shape of changes to files already inside the change scope.
The two outcomes together are a fair description of its reach.

### Runs

| Suite | Result |
|---|---|
| `embedding_dispatch_is_atomic` | 7 passed, 0 failed |
| `embedding_schema_contract` | 7 passed, 0 failed |
| `p03_upgrade_provisioning` | 6 passed, 0 failed |
| `provider_admission` | 12 passed, 0 failed |
| `node --test tests/p04_scope.test.mjs` | 6 passed, 0 failed |
| `cargo clippy --workspace --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

### What Step 2 has not done

The atomicity half of Step 2 is not built. The suite proves rediscovery and
pre-dispatch recovery; it does not yet inject a write boundary at each leg of
one dispatch — attempt phase, effect lifecycle transition, admission row,
credential lease consumption, MRE — and assert the whole tuple rolls back.

That leg needs the `embedding_job` dispatch cause to exist, and three places
currently close over `run_step` and `qualification_probe` only:
`provider_dispatch_causes.cause_kind`, `model_request_evidence_roots.cause_kind`,
and the argument validation inside `vestrace_try_admit_provider_dispatch` and
`vestrace_lock_provider_dispatch_routing`. Extending them is forward-migration
work in 0187 over functions P03 owns, and it must not become a second admission
or routing function — the plan names that as a defect in as many words.

## Task 4, Step 2 continued: the third dispatch cause

P03 closed `provider_dispatch_causes.cause_kind` and
`model_request_evidence_roots.cause_kind` over `run_step` and
`qualification_probe`. Spec line 219 makes an embedding job a third owner of an
external effect and a `ModelRequestEvidence` identity, so the vocabulary has to
admit it — the alternative is embedding dispatch growing its own admission and
evidence tables, which is exactly what "one dispatch authority, two callers"
exists to prevent.

Migration 0187 widens both. These are widenings, not rewrites: every existing
row still satisfies the new constraints, and the closed matrix stays closed. The
embedding branch names its job and its pinned snapshot and nothing else, as the
Run branch names its run, step and snapshot; `provider_dispatch_causes` gains
one column, `embedding_job_id`, with its foreign key.

No new signature was needed for the two P03 functions that will consume it.
`embedding_jobs.external_effect_id` is unique, so an embedding cause is
identified by the effect id and the pinned snapshot id the caller already
states. Deriving the job from its own effect is a consistency proof, not a
resolution of choice — the worker still names nothing it could have chosen.

**P03's own closed-world column assertion caught the new column immediately**:
`task10_dispatch_and_governed_artifact_contract_is_database_visible` compares
`provider_dispatch_causes`'s columns against an exact list. That is the third
time in this package a P03 guard has refused to let P04 widen something
silently, and each time the guard was right. The list now carries
`embedding_job_id` with the reason, and stays exact so a fourth column still has
to be argued for.

### Precedent for replacing a P03 function rather than adding one

The two functions that still close over two causes —
`vestrace_try_admit_provider_dispatch` (~800 lines) and
`vestrace_lock_provider_dispatch_routing` — live in migration 0184, which is
historical and immutable. A forward migration can only replace them whole.

That looked expensive enough to question, so the tree was checked rather than
assumed: eleven guarded functions are already defined in more than one
migration, and `vestrace_lock_provider_dispatch_routing` is one of them. P03
replaced it in 0185, with an identical signature, for precisely this reason —
qualification probes became a second cause. P04 doing the same for a third cause
follows an established pattern instead of inventing one, and the plan's own rule
holds: no second admission, throttle, lease or recovery function may appear.

## Task 4, Step 2 complete: an embedding job dispatches through the Run step's authority

An accepted, evidenced embedding job now reaches the provider through
`vestrace_try_admit_provider_dispatch`,
`vestrace_lock_provider_dispatch_routing`, the same admission and concurrency
lease, and the same `dispatching` lifecycle transition a Run step uses. The
cause row records which owner asked and names the job.

`an_embedding_job_dispatches_through_the_shared_authority` asserts the whole
trace tuple is present and that the cause names the job and nothing else. It
also queries `pg_proc` for every function whose name contains `admit` or
`throttle` and requires exactly two — the plan calls a second admission or
throttle function a defect, and the database is where such a function would be
visible rather than in a reviewer's memory.

### Four functions closed over the cause, not two

Widening the two tables was not enough, and the first attempt failed with
`PROVIDER_DISPATCH_ROUTING_REFUSED`. Rather than widen the next function and
retry, every migration was scanned for a function body mentioning both
`'run_step'` and `'qualification_probe'`. That returned four:

| Function | Origin | Why it closes over the cause |
|---|---|---|
| `vestrace_try_admit_provider_dispatch` | 0184 | validates the cause tuple and writes the cause row |
| `vestrace_lock_provider_dispatch_routing` | 0184, replaced in 0185 | selects routing per cause |
| `vestrace_lock_model_request_evidence_for_reconstruction` | 0184 | validates the evidence root's shape per cause |
| `vestrace_create_model_request_evidence` | 0183 | validates the node matrix per cause |

All four are replaced whole in 0187 with unchanged signatures, each derived from
its original by anchored substitution rather than retyped, and each diffed
against the original so the review is a short list rather than 800 lines:

| Replacement | Added | Removed |
|---|---|---|
| routing | 41 | 0 |
| admission | 53 | 4 |
| reconstruction | 21 | 0 |
| evidence creation | 20 | 4 |

Every removal is a line replaced by its widened equivalent. In the evidence
constructor the Run branch is *shared* rather than copied, because it already
required exactly what an embedding job's evidence is — a snapshot-rooted tuple
with no qualification nodes — so its two refusal messages now say
`snapshot-rooted` instead of `run-step`, and the one claim the Run branch cannot
make for an embedding job (that the cause is the job owning this effect and this
snapshot) is checked separately.

### The widening worked in tests and would have failed in production

`p03_upgrade_provisioning` then failed with `42501 must be owner of table
provider_dispatch_causes`, and afterwards with the same for
`vestrace_lock_provider_dispatch_routing`.

`ALTER TABLE` and `CREATE OR REPLACE FUNCTION` require ownership, and both
tables and all four functions belong to `vestrace_guarded_owner`. Under
`#[sqlx::test]` migrations run as a superuser that owns everything, so the
widening passed there. In a real deployment the migrator is the restricted
runtime role, which the bootstrap grants membership of the guarded owner
`WITH INHERIT FALSE, SET FALSE` — it cannot impersonate the owner, deliberately,
because every other path to those objects goes through a `SECURITY DEFINER`
function.

**This is the class of defect that only the suite modelling the real roles can
see.** Every embedding test was green while the migration could not have run in
production.

The fix follows P03's own pattern rather than inventing one: a one-shot
`SECURITY DEFINER` hand-back in the bootstrap,
`vestrace_prepare_p04_dispatch_cause_upgrade()`, modelled on
`vestrace_prepare_task7_qualification_upgrade()`. It refuses once migration 187
is applied, hands the two tables and the four functions to the migrator, and
revokes execute on itself in the same transaction. 0187 calls it in a preamble
with a superuser fallback — which is what lets `#[sqlx::test]` run the same file
unchanged — and hands everything back through `vestrace_assign_p03_table_owner`
and `vestrace_assign_p03_function_owner` before the file ends.

A smaller mistake surfaced on the way: the appended bootstrap block used `#`
comments. Everything inside that heredoc is SQL, and the provisioning suite
extracts and executes it directly, so `#` is `42601 syntax error`. The suite said
so immediately.

### Every leg commits or rolls back together

`injected_write_boundaries_roll_every_embedding_dispatch_leg_back` tears the
transaction at each of nine boundaries — admission, intent, authorization,
dispatching, and the governed commit, before and after each — and requires all
six durable traces to be absent afterwards: the cause row, the admission, the
concurrency lease, the `dispatching` transition, the authorization, and the
Audit event. Counting one of them and calling the transaction atomic would miss
exactly the failure this suite exists to catch.

### The model-data-policy leg is not on this path

Two fault points never fired. `BeforePolicyRecord` and `AfterPolicyRecord` sit
inside `if let Some(record) = evaluation.model_data_policy`, and an embedding job
supplies `None`: its disclosure decision belongs to `EmbeddingDataPolicyGate` and
is taken before acceptance.

That is corroborated by the schema rather than only by the code —
`model_data_policy_decisions` has `run_id` and `step_id` and **no workspace
column at all**, so the row is structurally a Run-step artifact and an embedding
job could not write a truthful one.

Rather than quietly drop the two points from the loop,
`the_model_data_policy_leg_is_absent_from_an_embedding_dispatch` asserts they are
unreachable and that no decision row is written. A future change that started
recording a model data policy for embeddings would add an uncovered write to the
dispatch transaction, and it now fails here instead of passing in silence.

### A dispatch cannot name a snapshot the job did not pin

Line 219: an ordinary job "owns its snapshot resolved from the current tuple at
acceptance and never changed thereafter". `a_dispatch_naming_another_snapshot_is_
refused` passes a different snapshot and requires the refusal to come from the
database, leaving no trace behind — a worker that could substitute a snapshot
would be routing.

### Runs

| Suite | Result |
|---|---|
| `embedding_dispatch_is_atomic` | 11 passed, 0 failed |
| `embedding_schema_contract` | 7 passed, 0 failed |
| `provider_dispatch_is_atomic` | 44 passed, 0 failed |
| `provider_schema_contract` | 38 passed, 0 failed |
| `provider_runtime_role_refusals` | 45 passed, 0 failed |
| `provider_admission` | 12 passed, 0 failed |
| `p03_upgrade_provisioning` | 6 passed, 0 failed |
| `connection_mutation_is_atomic` | 5 passed, 0 failed |
| `runtime_role_cannot_write_directly` | 2 passed, 0 failed |
| `cargo clippy --workspace --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

The 94 P03 tests passing against four replaced functions is the evidence that
the replacements are widenings and not rewrites, alongside the diffs above.

## Task 4, acceptance and composition

### The acceptance command

`crates/vestrace-application/src/embedding/job.rs` declares
`AcceptEmbeddingJob` and a one-method `EmbeddingJobRepository`. One method is
the point: everything an accepted job does afterwards — rediscovery, dispatch,
recovery — belongs to `ProviderDispatchRepository`, and a second acceptance-plus-
dispatch port for embeddings is what this package exists to avoid.

The command carries the effect intent rather than letting the repository build
one. `ExternalEffectIntent::new` allocates a fresh id, so constructing it at the
storage boundary would give a same-key replay a different effect than the job it
is being accepted for — the same trap the Run-step path documents in its own
comment.

`PgEmbeddingJobRepository` writes the intent and calls
`vestrace_accept_embedding_job` inside one governed mutation. The intent goes
first because every later dispatch converges on that exact effect id, and a job
without its effect is a job no dispatch could ever find.

Four tests hold it:

- `acceptance_writes_the_job_and_its_effect_together` — both rows exist, and the
  accepted job starts `requested` and retries nothing.
- `acceptance_into_an_unauthorized_space_leaves_nothing` — a job naming an
  unregistered space leaves no job, no effect and no audit. This is the refusal
  that closes the lazy path P04 replaces: a space that existed because something
  needed to write a vector into it.
- `a_successor_of_a_live_predecessor_is_refused` — line 257 makes terminal
  `InconclusiveUnknown` the only state a successor may be created against, and
  the predecessor is verified unchanged afterwards.
- `unconfigured_acceptance_is_unavailable` — the default refuses by name.

### Composition, and one placeholder that was removed rather than kept

`GovernedProviderRuntime` now composes embedding acceptance from the same store
and exposes it beside `dispatch()`. The dispatch authority is not duplicated:
an accepted embedding job reaches the provider through the same instance the
Run-step executor holds.

`the_governed_runtime_composes_one_dispatch_authority_for_both_callers` checks
that in the composition rather than in a reviewer's memory — exactly one
`PgProviderDispatchRepository::new(` in the file, embedding acceptance built from
the shared store, and no `embedding_dispatch` anything.

**The worker was left unwired, deliberately.** A line was written into
`worker.rs` holding the acceptance authority in a `let _`, and then removed: it
would have satisfied a wiring assertion while wiring nothing. There is no work
item kind an embedding job is leased under yet, and the worker's own comment
states the rule this would have broken — a handler registered now "would report
success having called nothing".

So the honest position is: the graph is composed and the authority is reachable,
and **no embedding job is dispatched outside a test**. That is recorded in *Not
true yet* rather than presented as wiring.

### Runs

| Suite | Result |
|---|---|
| `embedding_dispatch_is_atomic` | 15 passed, 0 failed |
| `embedding_schema_contract` | 7 passed, 0 failed |
| `p03_upgrade_provisioning` | 6 passed, 0 failed |
| `vestrace-cli` (all targets) | 81 passed, 0 failed |
| `cargo clippy --workspace --all-targets` | clean |
| `cargo fmt --all -- --check` | clean |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

## Task 5: recovering an ambiguous embedding effect

### The job gained a version, because line 257 asks for one

`embedding_jobs` now carries `version BIGINT NOT NULL DEFAULT 1`, bumped by each
state transition. Line 257 requires the acknowledgement command to be
expected-version checked, and append-only states alone cannot express "the job I
read is still the job I am acting on". `vestrace_accept_embedding_job` gained
`target_expected_predecessor_version`, and it refuses a successor without one and
a version without a successor, so the pair cannot drift apart in either
direction.

### One recovery winner, and the adoption that was previously refused

`vestrace_finalize_embedding_job_unknown` appends the job's terminal
`inconclusive_unknown` after the shared recovery winner has left the effect
`Unknown`. It publishes nothing, releases no admission, and calls no adapter --
whether the provider ran is precisely what is unknown, so there is nothing to
ask. It requires the effect's own `unknown` outcome to exist first: without that
check it would be *deciding* a call was ambiguous, which is the recovery winner's
decision and not its own.

It is idempotent, which is what makes "a crash between the effect outcome and the
job outcome is recovered without another adapter call" true rather than hoped
for.

`recover_embedding_job_attempt` now implements `AdoptedUnknown` instead of
refusing: it adopts the lost dispatch through the same
`adopt_lost_dispatch_in` the Run step uses, then finalizes the job, both in the
caller's transaction.

### What the suite proves, and how

`embedding_effect_recovery` measures the property against the database rather
than against a counter in a double. A second provider call would need a second
effect and a second admission, and those are rows: `dispatch_footprint` counts
the effects, admissions, concurrency leases and `dispatching` transitions, and
recovery must not change any of them.

- `recovery_waits_for_the_dispatch_deadline` — before the deadline the answer is
  `AwaitDispatchDeadline` and the job is untouched. Line 245 makes loss after
  `Dispatching` deadline-gated; a worker that adopted the moment it noticed an
  in-flight dispatch would be racing a provider it cannot see.
- `past_the_deadline_one_winner_adopts_and_finalizes` — the effect gains exactly
  one `unknown` outcome, the job becomes terminal at version 2, and the dispatch
  footprint is unchanged.
- `a_second_recovery_adds_nothing` — the second run reports `AlreadyUnknown`, the
  version stays at 2 and no second outcome is appended.
- `a_terminal_unknown_job_is_never_presented_as_dispatchable` — line 257 makes
  the authorized acknowledgement the sole successor path, so a second dispatch
  is refused and leaves no footprint.
- `recovery_is_workspace_bound` — another workspace cannot terminalize this job.

**The recovery time is passed in, not waited for.** P03's qualification
established that the executor's time-dependent branches cannot both be driven end
to end without waiting out the dispatch TTL. This task inherits that limit and
states it plainly: both the pre-deadline and post-deadline branches are proved
against the database with an explicit `recovered_at`, and neither is proved by
sleeping or by a real elapsed TTL.

The plan asks for an adapter call counter that "stays at one". There is no
executor calling an adapter for embeddings yet, so a counter would have read zero
and proved nothing. The footprint assertion is the same property measured where
it is actually observable.

### Step 3: the break P03's plan named and did not perform

Recovery was modified to mint a fresh external-effect intent after `Dispatching`
— exactly what a second provider call would require — and the suite was run:
three of five tests failed, including
`past_the_deadline_one_winner_adopts_and_finalizes` on "recovery created a new
dispatch footprint". The two that stayed green are the pre-deadline and
cross-workspace cases, which never reach adoption; that they stayed green is
itself the right result.

The file was restored byte-identically, digest
`770a46dab35875416cf4e180ef63f03deb0f6a896f3b345a6ffa30da58544b32` before and
after, and the suite returned to 5 of 5.

### The sixth scope amendment, and why this one is a judgement rather than a necessity

`crates/vestrace-infrastructure/tests/common/mod.rs` was added to the scope, 92 →
93. Unlike the previous five this one is not mechanically forced — duplicating
the fixture compiles. Two suites now need the same governed fixture: a workspace,
connection, revision, no-auth binding, model revision, qualification chain,
binding snapshot, space registration, effect intent, accepted job, evidence root
and admission policy. Rust test binaries share no code, so the choice was one
shared module or roughly 250 duplicated lines. Two copies of the definition of a
governed embedding job drift, and a drifted fixture produces evidence about a
system that does not exist.

It is the operator's to reverse; the alternative is duplication and the drift
risk that comes with it.

One mistake was made and caught during the extraction: the acceptance tests moved
into the shared module along with the fixture, so they were reported as
`common::...` and would have run once per including suite. Tests are not fixture;
they were moved back.

### Runs

| Suite | Result |
|---|---|
| `embedding_effect_recovery` | 5 passed, 0 failed |
| `embedding_dispatch_is_atomic` | 15 passed, 0 failed |
| `node --test tests/p04_scope.test.mjs` | 6 passed, 0 failed |
| `cargo fmt --all -- --check` | clean |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

### The acknowledgement command and its route

`POST /v1/embedding-jobs/{id}/acknowledge-unknown` now reaches the guarded
successor path from outside the process. The capability
`embedding.retry_after_unknown` gates it through the route inventory, which is
what builds the `AuthorizationRequest`; the handler requires an
`Idempotency-Key`, refuses a path/body disagreement before the application is
touched, returns `409` on a policy refusal and `503` when no authority is
configured. `server.rs` wires it to `governed.embedding_jobs()`, the real
runtime repository, not a placeholder.

Only `EmbeddingRetryAfterUnknown` was added. Line 257 names two further
transition-scoped acknowledgements; they belong to Tasks 7 and 8, and adding them
now would leave unused variants standing as if their routes existed.

### The plan asserted three properties this system does not have

Each was caught by the implementer reading the source, not by the author who
wrote the requirement. They are recorded because the pattern matters more than
the three fixes.

**Receipt replay does not exist.** The requirement said a repeated request under
the same `Idempotency-Key` replays the original receipt, and called this "the
governed-mutation boundary's existing behaviour". It is not.
`PgGovernedMutationRepository::commit_on` takes an advisory lock on
(workspace, key), reads the stored `request_hash`, and refuses with
`IDEMPOTENCY_KEY_REUSE_CONFLICT` only when it *differs*. On a matching hash it
falls through and applies the mutation again: the key insert is
`ON CONFLICT DO NOTHING`, so nothing fails, but a second audit event is
recorded, the outbox message is written twice, and a new receipt with a new
`audit_event_id` is returned. The helper that does compare and replay,
`find_idempotent_result`, belongs to `cognitive_mutation_repository.rs` — a path
governed mutations do not take.

**The idempotency hash covered a value the caller never states.** The first
composition included the successor's effect id. That id is allocated by
`ExternalEffectIntent::new`, so two byte-identical requests would have produced
two different hashes and been reported as "you reused this key with a different
request", which would have been a lie told to a client. Only caller-stated values
are hashed now.

**The effect id cannot be caller-stated, and should not be.**
`ExternalEffectIntent::new` allocates it and accepts none. That is deliberate:
an external-effect ledger whose ids callers may name is one where a caller can
collide with an existing effect on purpose. Four comments already in this
codebase record that the boundary constructs the intent. The requirement was
rewritten around the constraint rather than the constraint around the
requirement, and `crates/vestrace-domain/src/external_effects.rs` gained no
fixed-id constructor.

### What the acknowledgement may therefore claim, and what it may not

A different request under the same key is refused. A repeated identical request
creates no second successor and no second effect, because
`vestrace_accept_embedding_job` refuses a tuple that does not match — and since
the successor *is* the duplicate charge line 257 governs, that is the property
the requirement actually needed.

It is not transparent under replay. The boundary mints a fresh effect id, so an
identical retry is refused with `409 EMBEDDING_JOB_ACCEPTANCE_REFUSED` rather
than returning the original `201`. The test asserts that status explicitly, so
the coarseness is pinned rather than discovered later. Making a replay return
`201` requires the receipt replay recorded below as a debt.

### Two defects found in passing, one fixed and one recorded

**Fixed.** The served OpenAPI document's `governed_mutation_headers` described
`x-workspace-id`, `x-principal-id` and `x-request-id` and omitted
`Idempotency-Key`, which twelve handlers require through
`required_idempotency_key` and refuse the request without. The published
contract therefore described requests this system rejects. The header is
appended to the shared block — appended, not prepended, because
`provider_openapi_contract.rs` asserts `parameters[0]` and `parameters[1]` by
position and prepending would have broken the contract test for every governed
route at once.

Why the omission survived is visible one route away:
`/v1/memories/{id}/revisions` documents `idempotency-key` *inline*, in its own
parameter list, bypassing the shared block. One correct route, written by hand,
is exactly what makes a systematic gap look addressed.

**Recorded, not fixed.** Receipt replay across the governed mutation boundary.
Closing it means storing the receipt in `idempotency_keys.response_payload` — the
column exists and every handler writes `None` into it — and short-circuiting in
`commit_on`. That changes the observable behaviour of all twelve governed
mutation routes and their evidence, and belongs to a package that owns the
governed boundary rather than one that owns embeddings.

### The successor's kind was minted at the boundary

The first implementation hardcoded `kind: EmbeddingJobKind::Delivery`. There are
three kinds, and neither the request nor the guarded function carried or compared
one, so acknowledging an ambiguous `RetrievalQuery` or `Rebuild` job would have
created a successor of the wrong kind, silently, for two kinds out of three.

It was reachable only in principle — no producer of those kinds exists yet — which
is precisely why it would have survived to the package that added one. The kind
is now caller-stated and hashed, and the guarded function's predecessor lookup
requires `kind = target_kind` alongside workspace, id, state and version, folded
into the existing query rather than added as a second one. The refusal was
observed failing before the predicate existed and passing after.

### The seventh scope amendment

`crates/vestrace-domain/src/security/mod.rs`, 93 → 94. Line 257 makes the
acknowledgement an authorized command, and the only mechanism this system has for
authorizing a route is a `Capability` variant named by a `route_inventory`
descriptor — `governed!` builds the `AuthorizationRequest` straight from it.
`Capability` had twenty-six variants and no embedding member. The alternative was
to authorize the acknowledgement under an unrelated existing capability, gating
the route under a name that does not describe it. Tasks 7 and 8 need no further
amendment, because this file is now in scope.

### Runs, after the acknowledgement

| Suite | Result |
|---|---|
| `embedding_effect_recovery` | 11 passed, 0 failed |
| `embedding_dispatch_is_atomic` | 15 passed, 0 failed |
| `embedding_schema_contract` | 7 passed, 0 failed |
| `vestrace-http --lib` | 45 passed, 0 failed |
| `route_inventory_is_exhaustive` | 8 passed, 0 failed |
| `provider_openapi_contract` | 4 passed, 0 failed |
| `node --test tests/p04_scope.test.mjs` | 6 passed, 0 failed |
| `cargo clippy --workspace --all-targets` | 0 warnings |
| `cargo fmt --all -- --check` | clean |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

Every row was re-run by the reviewer rather than taken from the implementer's
report. One report claim did not survive that check: an unformatted blank line was
described as pre-existing and as a file restored byte-for-byte, when the file
differed from its captured digest by exactly one byte and had been modified
inside the build's own window. It was removed, and the file now matches its
captured digest exactly.

### A note on the tooling, since it cost an hour

One planning review was reported as having timed out after ten minutes. That was
true of the agent dispatching it and false of the work: the process outlived its
dispatcher, hung, and eventually exited, leaving its job record marked `running`,
which then refused every later resume. The cancel path could not clear it,
because it treats a job as cancelled only on a successful `taskkill`, and
`taskkill` returns 128 for a process that is already gone.

A dispatcher's timeout is not the dispatched work's outcome, and a status of
"running" is a claim about a record, not about a process.

## A race in P03's zeroization evidence, found by running the suite more than once

`provider_schema_contract` failed intermittently while this work proceeded —
`provider_result_repository_recovers_one_atomic_publication_and_exact_replay` on
one run, `run_provider_result_prepare_fails_closed_without_dispatch_release_and_
rolls_back` on another, and neither when run alone. A different test each time is
the signature of shared state, not of a defect in either test.

The cause is a custom global allocator hook driven by one set of process-global
statics: `RESULT_CONTENT_POINTER`, `RESULT_CONTENT_LEN`,
`RESULT_CONTENT_OBSERVED`, `RESULT_CONTENT_ZERO`. Two tests arm that observer,
libtest runs them on parallel threads, and whichever stored the pointer last was
the only one the hook could see. The other then failed with "did not deallocate
the exact retained-result allocation".

This matters beyond the inconvenience. **The zeroization those tests prove is
real; the evidence for it was being decided by a race.** A green run was partly
luck, and a red run said nothing about zeroization. P03's evidence cites this
suite.

The fix is a process-wide mutex held across each test's whole arm-run-assert
region, with poisoning absorbed so a failing test cannot turn its sibling into a
misleading poison error.

Verified by breaking it rather than by assertion: with the guard removed, the two
tests run together failed 1 of 4 times; with it restored — digest
`84c2d74d51785bf6d3811c834422887d40f45641cf6c21a7e80c6775fb486ac7` before and
after the probe — they passed 4 of 4, and the full 38-test suite passed three
consecutive runs. Three clean runs is evidence, not proof, and the honest
statement is that the observed failure rate went from roughly one run in three to
zero in seven.

## Correction to Tasks 2 and 3, found while reading the spec for Task 4

Task 4 required reading spec line 219 in full rather than through the plan's
citation. Two of the closed sets Task 2 declared, and Task 3 wrote into the
schema, were wrong. Both are now corrected, and both are recorded here rather
than quietly amended, because Tasks 5–12 build on these sets.

### The job kinds were two and are three

Line 219 reads: "Its closed kinds are `retrieval_query`, `delivery`, and
`rebuild` (backfill is `rebuild` mode)."

Task 2 declared two, `Delivery` and `Rebuild`, citing the later sentence "For
`delivery` or `rebuild`, the validated production response must match the job's
exact EmbeddingSpaceKey". That sentence names the kinds whose response is
persisted as vectors. It is not the closed set, and reading it as one made
`retrieval_query` — the kind retrieval actually issues — unrepresentable in both
the domain type and the `embedding_jobs.kind` CHECK constraint.

`EmbeddingJobKind` now carries three variants and an `ALL` constant, plus
`produces_persisted_vectors`, which is the predicate the misread sentence
actually states. A new closed-world test,
`the_job_kind_check_matches_the_declared_enum`, holds the constraint and the enum
to the same three literals in both directions.

### The job states were ten and are six

Line 219 reads: the job "records append-only `Requested -> Running -> Succeeded |
FailedDefinite | InconclusiveUnknown | Cancelled` transitions".

Task 2 declared ten, adding `Waiting`, `Authorized`, `Dispatching` and
`ResultPrepared`. Those four came from line 256, which lists the phases
new-version planning may find an overlapping predecessor **physical attempt** in.
They are not job states:

- `Dispatching` is the external effect's lifecycle state, which P03 already owns.
- `waiting_for_result_keys` is the attempt's visible pre-dispatch phase.
- `ResultPrepared` is the immutable `EmbeddingJobResultPrepared` marker.

Had this survived, `embedding_jobs.state` would have been a second, independently
writable answer to whether the provider had been reached — the one question this
system may not have two answers to, and the question P03's whole recovery design
turns on.

`EmbeddingJobState` now carries the six states line 219 names, with
`may_advance_to` encoding exactly the arrows that line draws and no others. The
edge `Requested -> Cancelled` is *not* admitted: it looks reasonable and is not
written in the spec, so admitting it would be the type inventing lifecycle rather
than recording it. `has_reached_the_provider` is gone, replaced by
`may_have_a_successor`, which states the property line 257 actually needs: the
terminal `InconclusiveUnknown` head is the only state an authorized
duplicate-charge acknowledgement may give a successor.

### How this was missed and what would have caught it

Task 2's tests each quote the spec line they enforce, which is why they looked
convincing: they quoted the line, and asserted a set the line does not contain.
Quoting a citation is not reading the source. The independent plan review the
operator deferred is the mechanism that exists to catch exactly this, and it
would have — the plan's own review checklist asks whether every closed set is
traceable to a spec line, which is a question a reviewer answers by opening the
spec.

Verified after the correction: `vestrace-domain --test embedding_contract` 11
passed, `vestrace-infrastructure --test embedding_schema_contract` 7 passed.

## Defect introduced by this package's own tooling: silent CRLF rewriting

Every file this session edited through Python's `pathlib.write_text` had all of
its bytes rewritten. `write_text` opens in text mode, and on Windows text mode
translates every `\n` to `\r\n`. Fifteen in-scope files were converted from LF
to CRLF without a single line of their content changing.

### How it surfaced

`p03_upgrade_provisioning` began failing four of six tests with "the provisioner
must contain the SQL heredoc terminator". That test locates the provisioner's
SQL by splitting `docker/postgres/init-runtime-role.sh` on `"\nSQL\n"`, and in a
CRLF file the terminator reads `"\nSQL\r\n"`. The assertion was right; the file
had been rewritten under it.

### Why nothing else caught it

- `git diff` shows nothing: `core.autocrlf=true` normalizes line endings on
  diff, so a whole-file terminator change is invisible there.
- `cargo fmt --check` passes: rustfmt accepts either style.
- `scripts/verify-dirty-baseline.mjs` passes, and this is the interesting one.
  The verifier pins a SHA-256 for every **protected** path and every unrelated
  dirty file. All fifteen were inside `changeScopePaths`, and for those it
  checks membership, not shape. **The verifier protects the tree a package must
  not touch; it says nothing about the shape of changes inside the scope a
  package may touch.** That is by design, and it is worth stating because the
  green exit code on this run does not mean what a reader might assume.

The measurement that proved it was the preflight's recorded byte counts:
`crates/vestrace-http/src/route_inventory.rs` was 11048 bytes at capture and
11489 after a six-line addition. The addition is 128 bytes; the other 313 are
one carriage return per line of the file.

### What was repaired, and what could not be

Fifteen files this package had legitimately edited were converted back to LF,
which is what the rest of this working tree uses: `migrations/0186`,
`provider_dispatch_is_atomic.rs` and `scripts/verify-dirty-baseline.mjs` are all
LF. `p03_upgrade_provisioning` is green again, 6 of 6.

Seven other in-scope files were caught by the same normalisation pass and should
not have been: **P04 had never edited them**, they were CRLF in the captured
baseline, and they are the operator's own unrelated dirty work. Being inside the
change scope is permission to change a file when the package needs to, not
licence to rewrite one it never touched.

Two were restored to their exact captured bytes, verified against the preflight
SHA-256:

- `crates/vestrace-fault-scenario/src/report.rs`
- `tests/c8_core_memory_qualification.rs`

Five could not be. Their originals were **mixed**: mostly CRLF with a minority of
LF-terminated lines, in a pattern no reconstruction recovered. Anchoring the
LF lines on the lines that differ from `HEAD`, and searching every contiguous
window of the right length, both failed to reproduce the recorded digest, and an
exhaustive search over multiple disjoint windows is not tractable. They now end
LF throughout:

| Path | Captured | Now |
|---|---|---|
| `crates/vestrace-application/src/retrieval/ports.rs` | 8381 | 8176 |
| `crates/vestrace-fault-scenario/src/main.rs` | 30522 | 29847 |
| `crates/vestrace-http/tests/health.rs` | 13699 | 13283 |
| `crates/vestrace-http/tests/request_span.rs` | 13727 | 13324 |
| `tests/http_command_contract.rs` | 10221 | 9917 |

Their **content is unchanged** — every one is byte-identical to its captured
state once line endings are normalised, and each suite that reads them passes.
That is a mitigation, not a defence: the standing constraint is byte-for-byte
preservation, and five files no longer satisfy it. The operator should know
before any diff of their own work is taken.

### The rule this package now follows

Repository files are written with `write_bytes`, never `write_text`. A text-mode
write on this platform is a whole-file rewrite disguised as an edit.

## Task 6 — transition-version planning (2026-09-04)

Migration 0188 adds an immutable guarded transition plan, ordered immutable
recipe rows, and the positive `model_binding_snapshot_scopes` mark. Existing
snapshots are backfilled as `ordinary` before the deferrable scope invariant is
installed. The two legitimate creators then write their scope in the same
transaction: the legacy Run snapshot creator writes `ordinary`, while the
transition planner writes `transition` with its plan id.

The guarded planner stores the source/target auth-binding XOR, target execution
tuple, batch, and structural recipe identities. It takes the connection,
optional credential, embedding-space, then transition locks; a successor must
preserve precisely the predecessor identities and their order. Ordinary
embedding acceptance and model-request evidence creation both fail closed for a
transition or unscoped snapshot.

The focused database proof passed six tests, including an atomic refusal count
that covers the base snapshot table, the deferred-at-COMMIT scope invariant, and
the exact ordered-successor cases. For the required break proof, removing the
ordinary-scope predicate from `vestrace_accept_embedding_job` made the suite
fail because it accepted a transition snapshot; restoring the predicate returned
the suite to green. The migration SHA-256 was identical before and after the
break: `E993B20DC0CEB81BCEE3F753C96B901E3506920D2A17BE3065D570C3633EDBB3`.

### The invariant reached outside the package, and the review is what found it

The implementer's report closed by saying it had attempted three provider
regression binaries but that "the terminal detached overlapping cargo processes
and produced only partial output", and did not count them. Re-running them
individually showed they were not truncated. They failed:

| Suite | Result |
|---|---|
| `provider_dispatch_is_atomic` | 0 passed, 44 failed |
| `provider_schema_contract` | 21 passed, 17 failed |
| `model_request_evidence` | 29 passed, 3 failed |

Sixty-four failures, one cause, counted in each suite's output rather than
inferred: `23514 "model binding snapshot requires a durable scope"`, raised from
`vestrace_validate_model_binding_snapshot_scope()`.

Nothing was wrong with the trigger. Six fixtures across three suites insert into
`model_binding_snapshots` directly instead of going through
`vestrace_create_run_model_binding_snapshot`, so they never wrote a scope row.
That shortcut had always existed and had never cost anything; the new invariant
is the first thing to charge for it.

**The lesson this task actually taught is about blast radius.** A deferrable
constraint trigger on a shared P03 table is not a local change, however local its
migration looks. The plan treated 0188 as self-contained and listed eleven files;
the invariant it installs applies to every writer of that table in the
repository, and two of the three affected suites were outside the P04 allowlist
entirely. This was not foreseen, and the eighth scope amendment exists because of
it.

The alternative was available and was rejected: weaken, defer or narrow the
trigger until the fixtures pass. That would have let test convenience decide
production semantics, which is the exact thing this package instructs its
implementer not to do.

### The eighth scope amendment

`crates/vestrace-infrastructure/tests/model_request_evidence.rs` and
`crates/vestrace-infrastructure/tests/provider_dispatch_is_atomic.rs`, 94 → 96.
Recorded with the restriction that it authorises **additive fixture rows only**:
no assertion, expected count, message comparison or test name in a P03 suite may
change under it.

That restriction was checked by counting rather than by reading — the two larger
files are over six thousand lines each, and a quietly deleted assertion is not
something a human finds in that volume. Before and after the fix:

| Suite | Lines | `assert` | `async fn` |
|---|---|---|---|
| `provider_dispatch_is_atomic` | 6017 → 6027 | 227 → 227 | 78 → 78 |
| `provider_schema_contract` | 6428 → 6440 | 265 → 265 | 47 → 47 |
| `model_request_evidence` | 3761 → 3767 | 142 → 142 | 47 → 47 |

Twenty-eight lines added, six `model_binding_snapshot_scopes` inserts, nothing
removed. The migration's SHA-256 is unchanged from before the fix round, which is
how "the trigger was not weakened to make the fixtures pass" is established
rather than asserted.

Applying the amendment also produced its own small demonstration: the path was
inserted at the wrong sort position, and the verifier refused immediately with
`preflight change_scope_paths differ from p04-scope.mjs`. The mechanism catches
the person maintaining it, which is the only kind of check worth having.

### Runs, re-executed by the reviewer

Every binary was run individually rather than as one `cargo test` invocation.
Batching them is what produced the "partial output" the implementer's report
mistook for a tooling problem, and an unread result is not a passing one.

| Suite | Result |
|---|---|
| `provider_dispatch_is_atomic` | 44 passed |
| `provider_schema_contract` | 38 passed |
| `model_request_evidence` | 32 passed |
| `runtime_role_cannot_write_directly` | 45 passed |
| `provider_admission` | 12 passed |
| `embedding_dispatch_is_atomic` | 15 passed |
| `embedding_effect_recovery` | 11 passed |
| `embedding_schema_contract` | 7 passed |
| `embedding_transition_planning` | 6 passed |
| `p03_upgrade_provisioning` | 6 passed |
| `vestrace-http --lib` | 45 passed |
| `cargo clippy --workspace --all-targets` | 0 warnings |
| `cargo fmt --all -- --check` | clean |
| `node --test tests/p04_scope.test.mjs` | 6 passed |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

## Not true yet

- Tasks 7–12 are not started: the ambiguity carry and its recipe-granular
  classification, barriers and supersession, retrieval generation fences,
  refusal and mixed-space qualification, worker-restart and fault evidence, and
  integrated verification. Tasks 5 and 6 are complete and reviewed.
- Task 6 leaves the transition dispatch path deliberately closed. A
  transition-scoped snapshot is attachable by nothing until Task 7 opens the
  legitimate transition-member path, which is stricter than the end state and
  never looser.
- A successor transition version may not drop a recipe. Line 253 permits a drop
  only against immutable `NoLongerRequired` evidence, and that evidence is
  Task 7's; allowing a proper subset now would commit a version that had
  silently discarded predecessor recipes.
- The P04 plan has not been independently reviewed.
- P03 Tasks 11–13 have not been independently reviewed.
- The `Deserialize` derive on `ConnectionAdmissionLimits` is still there.
- The served OpenAPI document's pre-existing request schemas still disagree with
  their handlers.
- Five in-scope files that P04 never edited no longer match their captured bytes;
  their line endings were normalised and cannot be restored exactly.
- A repeated identical acknowledgement is refused rather than replayed: the
  governed mutation boundary does not replay receipts, and closing that gap
  would change all twelve governed mutation routes.
- No embedding job is dispatched outside a test. The governed graph composes
  acceptance and dispatch, but the worker registers no handler for embedding
  work, because no work-item kind leases it yet.
