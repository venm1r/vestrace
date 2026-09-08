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

## Task 7 — recipe-granular classification and the ambiguity carry (2026-09-05)

Migration 0189 adds the carry header and its immutable per-recipe classification
rows, extends the planner to classify a lineage's one eligible ambiguity head,
opens exactly one path through Task 6's snapshot fence, and adds the authorized
acknowledgement that turns a carry into a single fresh successor attempt.

### The lineage was already followable, and a column would have been a second answer

`embedding_jobs` has no `transition_batch_id`, and the first draft of this task's
requirement was written as though it did. The link exists through what Task 6
built, constrained end to end by foreign keys:

`embedding_jobs.model_binding_snapshot_id` → `model_binding_snapshot_scopes`,
whose `transition_plan_id` is `NOT NULL` exactly when `scope = 'transition'` →
`embedding_transition_plans.transition_batch_id`.

A job belongs to a transition lineage if and only if its snapshot is
transition-scoped. Adding a convenience column would have created a second,
independently writable answer to which batch a job belongs — the defect this
package already corrected once, when `embedding_jobs.state` nearly became a
second answer to whether the provider had been reached.

"Latest" then needs no ordering at all. The chain is linked by
`retries_unknown_embedding_job_id`, so every earlier unknown attempt has a
successor by construction and the tail is the head. Where two candidates match,
the transaction raises `23514` rather than choosing: a rule that must pick
between two will eventually pick wrong, silently.

### What the review caught that the author did not

The planning review returned four blockers and three majors. Three of them were
contradictions the author had written and could not have found by re-reading his
own prose:

- **Immutability against supersession.** The plan declared mapping rows
  insert-only *and* required an open mapping to be terminalized. Line 253
  separates them: the header's state moves along
  `AwaitingAcknowledgement -> SuccessorCreated | NoLongerRequired`; the
  per-recipe rows are fixed at insert. Supersession terminalizes a header and
  inserts a new one. That is also what makes "at most one current open carry per
  head" expressible as a partial unique index — a superseded header leaves the
  predicate.
- **A circular acceptance check.** The plan asked `vestrace_accept_embedding_job`
  to admit a transition-scoped snapshot when the job is "the successor of a
  *current* carry". The acknowledgement marks the carry `SuccessorCreated` in the
  same transaction that accepts the job, so by the time anyone asks it is not
  current. The durable binding is the header's `successor_embedding_job_id`,
  checked by identity.
- **Evidence demanded from a vocabulary the plan had excluded.** `SatisfiedExisting`
  and definite resolution are `BarrierState` members, deferred to Task 8. Task 7
  therefore refuses a `Succeeded` predecessor outright and Task 8 will add the
  lawful release — stricter now, never looser, the same shape as Task 6
  forbidding recipe drops until this task's evidence existed.

The review also found that Task 6's planner compared only
`array_agg(recipe_identity ORDER BY recipe_ordinal)` and never the input
ordinals persisted beside them, so a caller could reuse an identity while
changing a recipe's input structure and pass both the idempotency branch and the
predecessor check. 0189 re-declares the planner comparing the full structure, and
the test for it was written before the comparison was widened, observed failing,
and observed passing after.

### A landmine found by execution rather than by reading

The obvious way to widen that comparison is a second `array_agg` over
`input_ordinals`. Run against the live database, that raises
`cannot accumulate arrays of different dimensionality`: PostgreSQL will not
aggregate variable-length arrays. A plan whose recipes all take the same number
of inputs would have passed such an implementation and every fixture written
from it, and failed on the first plan with differing counts — in production, not
in tests.

The implementer avoided it differently and better than the row-wise `FULL JOIN`
the plan prescribed:
`jsonb_agg(jsonb_build_array(recipe_identity::TEXT, input_ordinals) ORDER BY recipe_ordinal)`,
compared with `IS DISTINCT FROM`. JSONB accepts the varying lengths, and both
sides remain rows this system wrote, compared as values — not a hash oracle over
recipe content.

### The tests that tested nothing

The first implementation's carry suite was 41 lines of six `#[test]` functions
that `include_str!` the migration and assert `MIGRATION.contains("...")`. It ran
in 0.00 seconds and never opened a database. A substring assertion passes when
the text sits in a comment, in an unreachable branch, or in a function nobody
calls; it fails only when someone edits the text. One of them asserted the
*absence* of `"state='succeeded' AND state='inconclusive_unknown'"` — a condition
false by construction, since a column cannot equal two values at once — so it
could not fail under any implementation.

The names were accurate and read like the specification, which is what made the
list convincing. The 0.00-second runtime is what gave it away; the replacement
suite is 979 lines of eight `#[sqlx::test]` tests and runs in about seven
seconds.

### The concurrency proof, and the instrument that nearly invalidated it

`concurrent_same_version_planners_serialize_the_carry_for_one_ambiguity_head`
plans overlapping versions against one lineage from two independently
provisioned pools under `tokio::join!`.

Its first version passed for the wrong reason — the two planners shared the
connection and space locks, so the transition lock's removal changed nothing.
The implementer found this, said so, and strengthened the test with a separately
provisioned target binding and a test-local delay between the read and the
insert. That report is the useful part: a race test that has never been seen to
fail is not evidence, and P03's qualification is the standing warning — its two
race suites passed eight times out of eight with the connection-guard row lock
removed.

The break is against the guard that actually holds the property, the
`FOR UPDATE` on `embedding_transitions` inside
`vestrace_plan_embedding_transition_version`. Breaking a carry-specific lock
instead would have left that one standing and the suite passing, proving nothing.
With it removed:

| | SQLSTATE | Message |
|---|---|---|
| Lock removed | `23505` | `duplicate key value violates unique constraint "embedding_transition_plans_identity_key"` |
| Lock present | `40001` | `EMBEDDING_TRANSITION_PLAN_IDENTITY_CONFLICT` |

Without the lock the planners race past the read and one meets a raw uniqueness
violation instead of the guarded conflict. The migration's SHA-256 was
`1250826E4BEC80C1A5870F2BFEF2167A2D26C09EEA59C1CC4CF0308BCE96AD46` before the
break and after the restore, and the reviewer re-computed it from the file on
disk.

### Two defects the review found by running what the implementer had not

Its report listed the three provider suites as inconclusive — "the terminal
detached overlapping cargo processes and produced only partial output". Run
individually they were not truncated:

- **A one-shot helper called twice.** `vestrace_prepare_p04_embedding_transition_upgrade()`
  revokes EXECUTE from itself at the end of its own body. 0188 calls it and
  disarms it; 0189 called the same helper and was denied with `42501`, so the
  migration would not apply at all under the restricted role. The guard at both
  sites tested `to_regprocedure(...) IS NOT NULL` — existence — and the function
  still exists after the revoke. It checked the wrong property. 0189 now has its
  own helper and both guards also require
  `has_function_privilege(..., 'EXECUTE')`. `#[sqlx::test]` runs as superuser and
  cannot see any of this; `p03_upgrade_provisioning` is the only run that can.
- **P04 functions added to the P02 bridge.** The two
  `allowed_targets` / `runtime_executable_targets` pairs in
  `init-runtime-role.sh` are not two copies of one allowlist: they belong to
  `vestrace_assign_p02_function_owner` and `vestrace_assign_p03_function_owner`,
  one per package. The instruction given to the implementer said "both copies of
  each array", which is false, and it followed it. `p02_owner_helper_is_bounded_and_only_p02_migrations_use_it`
  caught it through its exact count, 56 against 58. The three entries were
  removed from the P02 bridge; the carry *tables* belong in
  `vestrace_assign_p03_table_owner` and were correctly placed there.

### Three instructions that cost more than the code

This task's delays were not caused by the implementation. Each came from a rule
this package's own author stated imprecisely, followed exactly:

| Instruction | What it broke |
|---|---|
| "all eleven paths were checked" above a list of sixteen, and a scope rule stated as that list rather than as `changeScopePaths` | a builder stopped on a path that was in scope all along |
| "do not weaken, delete or skip an existing test" | a builder spent three hours refusing to update one expected message that 0189 had deliberately renamed |
| "both copies of each array; there are two of each" | P04 functions written into the P02 ownership bridge |

The pattern is the same each time, and it is not the implementer's: a rule stated
about the system, not read from it. The corrective is the one this package
already applies to claims about code — check it against the source before writing
it down.

### Runs, re-executed by the reviewer

Each binary in its own invocation. Batching them is what let an earlier report
mistake 64 failures for truncated output, and an unread result is not a passing
one.

| Suite | Result |
|---|---|
| `embedding_carry_classification` | 8 passed |
| `embedding_transition_planning` | 7 passed |
| `embedding_dispatch_is_atomic` | 15 passed |
| `embedding_effect_recovery` | 11 passed |
| `embedding_schema_contract` | 7 passed |
| `p03_upgrade_provisioning` | 6 passed |
| `provider_admission` | 12 passed |
| `provider_dispatch_is_atomic` | 44 passed |
| `provider_schema_contract` | 38 passed |
| `model_request_evidence` | 32 passed |
| `runtime_role_cannot_write_directly` | 45 passed |
| `vestrace-http --lib` | 45 passed |
| `cargo clippy --workspace --all-targets` | 0 warnings |
| `cargo fmt --all -- --check` | clean |
| `node --test tests/p04_scope.test.mjs` | 6 passed |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

## Task 8 — barriers and supersession (2026-09-05)

Migration 0190 adds the barrier header and its immutable per-recipe mappings,
the closed lifecycle
`AwaitingPredecessorTerminal -> ResolvedToCarry | ResolvedSatisfiedExisting | ResolvedDefinite | NoLongerRequired | Superseded`,
the partial unique index that keeps at most one open barrier per predecessor
lineage, supersession through a linked chain, and the refusal that makes a
barrier's dedicated batch nondispatchable while it is open.

### Completeness is recorded, not simulated

Line 256 requires the dedicated batch to be "nondispatchable **and**
completeness-blocking". Only the first half is enforceable here: there is no
completeness surface anywhere in this repository. `operator_acknowledgement_required`
appears nowhere, nothing computes completeness for anything, and
`BarrierState::blocks_dispatch()` was declared in Task 2 and called by nothing
until now.

So the barrier persists the open fact as a queryable row and does not invent a
completeness projection to block. Building one here would have meant asserting a
property against a surface this task also authored — a check marking its own
homework. The gap is named rather than closed, and belongs to whichever package
owns completeness.

### Where the refusal had to live, and why not where the plan first put it

The plan's first draft refused "a job whose transition batch has an open
barrier" at job acceptance and dispatch admission. That is unimplementable: a job
reaches a batch only through its snapshot's plan, and
`embedding_transition_plans` carries a single `transition_batch_id` for the whole
plan. A barrier's dedicated batch is deliberately not that one — line 256 forbids
any unaffected or new recipe from sharing it — so a generic job has no path to it
and a check at `vestrace_accept_embedding_job` would have consulted the wrong
batch entirely. It would also have collided with Task 7's R7 relaxation, the one
path permitted to attach a transition-scoped snapshot.

0189 already showed the answer: a carry row stores
`predecessor_transition_batch_id` and `successor_transition_batch_id` itself. The
entity that owns a dedicated batch names it. So the barrier stores its own, and
the refusal lives at the two places that name a dedicated batch when creating
work for it — the guarded planner and the carry acknowledgement.

### Four spec names, four different homes, and two that do not exist

Line 256 lists the phases a predecessor attempt may be found in — `Requested`,
`Running`, waiting, `Authorized`, `Dispatching`, ResultPrepared — as though they
were one vocabulary. The domain had already settled this during the Task 2/3
correction, in the comment above `EmbeddingJobState`: `Dispatching` and
`Authorized` are the external effect's lifecycle, which P03 owns; `Waiting` is
`waiting_for_result_keys`; `ResultPrepared` is the immutable
`EmbeddingJobResultPrepared` marker.

Checking those against the tree found something the comment does not say:
**neither `waiting_for_result_keys` nor `EmbeddingJobResultPrepared` exists.**
Both appear only in comments — in `job.rs`, inside 0187, and in a test's doc
line. There is no table, column or marker for either.

That leaves no hole, because all four are sub-phases of a predecessor whose job
has not terminalized. A job waiting for result keys, a job whose effect is
dispatching, and a job with a prepared result are all `Requested` or `Running`.
The predicate is therefore one condition, not a list, and no phase column was
added to `embedding_jobs` — that column would be the second answer to "has the
provider been reached" this package refuses to have.

### The closed world the transition states did not have

`embedding_schema_contract.rs` had been asserting that the `embedding_jobs` state
CHECK names exactly `EmbeddingJobState::ALL`, in both directions. Nothing did the
same for the transition vocabularies: `BarrierState`, `CarryHeaderState` and
`CarryMappingState` declared no `ALL`, so Task 7's carry CHECK constraints were
tied to their Rust enums by nothing at all. Task 8 adds the three constants and
their contract tests. The suite grew from 7 tests to 10.

An earlier draft of that requirement asked SQL to "derive its refusal from
`blocks_dispatch()` rather than re-listing states". PostgreSQL cannot call a Rust
`const fn`; the contract test is the mechanism this repository already had, and
the requirement was rewritten to use it.

### Three rounds spent on one error message

Migration 0190 failed under the restricted role with
`42501 must be owner of function vestrace_acknowledge_carried_transition_batch_after_unknown`.
The diagnosis took three rounds because PostgreSQL emits that same sentence for a
denied `ALTER FUNCTION` and a denied `CREATE OR REPLACE`, and every reading of it
pointed at ownership.

Ruled out along the way, each by reading the source: the signatures match to the
parameter; the helper does list the function, with an explicit postcondition; all
three P04 helpers are `SECURITY DEFINER`; the creation guard fires correctly on a
fresh database. A probe then reported the helper running as `test` — and a
database query established that `test` is a superuser, which meant the helper
could alter anything and the ownership theory was dead.

The actual cause was ordering inside 0190 itself: it handed its functions back to
`vestrace_guarded_owner` **before** it had finished replacing them. The hand-back
now happens last. Nothing was wrong with the ownership bridge.

This is the fourth time in this package that a check examined the wrong property:
a guard testing a function's existence rather than the right to execute it; a
watcher testing one job state rather than two; a test suite reading a migration's
text rather than the database's behaviour; and now a diagnosis reading a message
that means two different things.

### What the implementer refused to do

Asked to finish the suite while told the schema was accepted, it reported that
A1 and A4 could not be met: 0190's classifier produced only three of the five
terminal states, supersession lacked the stale-target decision, and there was no
race coordination to test. It added that reaching the missing states by updating
tables directly "would make those tests test fixtures, not the guarded
behavior", and declined.

It was right on both counts. "The schema is accepted, do not rewrite it" had been
said after reading the migration's size and structure rather than checking it
against this task's own acceptance criteria, so the instruction and the criteria
contradicted each other. The refusal to close that gap with fixture writes is the
same discipline that made Task 7's substring suite unacceptable.

### Runs, re-executed by the reviewer

Each binary in its own invocation.

| Suite | Result |
|---|---|
| `embedding_transition_barriers` | 10 passed |
| `embedding_carry_classification` | 8 passed |
| `embedding_transition_planning` | 7 passed |
| `embedding_dispatch_is_atomic` | 15 passed |
| `embedding_effect_recovery` | 11 passed |
| `embedding_schema_contract` | 10 passed |
| `p03_upgrade_provisioning` | 6 passed |
| `provider_admission` | 12 passed |
| `provider_dispatch_is_atomic` | 44 passed |
| `provider_schema_contract` | 38 passed |
| `model_request_evidence` | 32 passed |
| `runtime_role_cannot_write_directly` | 45 passed |
| `vestrace-http --lib` | 45 passed |
| `vestrace-domain --test embedding_contract` | 12 passed |
| `cargo clippy --workspace --all-targets` | 0 warnings |
| `cargo fmt --all -- --check` | clean |
| `node --test tests/p04_scope.test.mjs` | 6 passed |
| `scripts/verify-dirty-baseline.mjs --check` | exit 0 |

Assertion counts in every pre-existing suite are unchanged — carry 22, planning
20, provider dispatch 227, provider schema 265, model-request evidence 142 —
which is how "the carry fixture was adapted for barriers without weakening it" is
established rather than asserted.

Both dispatch-gate mutations were performed and restored, the migration's
SHA-256 identical before and after at
`6BF09B1DA7689CF0BBC7FCB84964810145FB8B85E5ED6A8357B2041F1D6A77DF`: with the
barrier gate removed, planning returned `Ok(...)` instead of the required
refusal; with the partial unique index removed, the second open barrier inserted
successfully.

## Task 9 — the retrieval generation fence (2026-09-05)

Migration 0191 adds corpus-generation membership, the `building -> ready -> stale`
lifecycle and its guarded open/enrol/publish/stale functions; `PgEmbeddingStore`
registers a space and enrols each embedding it writes; a resolver port turns a
space name and model into a Ready generation; both retrievers filter their SQL by
membership in that generation and report the generation they actually read;
fusion refuses a candidate carrying any other generation.

This is the package's headline exit criterion, and it took seven build rounds.
Three of the defects those rounds surfaced were not in Task 9 at all.

### The field that proved nothing

The fifth adversarial review said a public `corpus_generation_id` on
`RetrievalCandidate` "does not prove it came from the selected row — either
retriever can copy the request pin". The finding was recorded and the
implementation did not close it: both retrievers set the field from
`request.corpus_generation_id`. The fence therefore compared the pin with itself,
and every test passed.

What caught it was not a review and not a test run. It was the attempt to build
A3's mutation: an implementer asked to remove the SQL predicate and show the
suite failing reported that it *could not*, because the intruding rows would
still report the requested generation. A test that exists to catch a defect
found it by being impossible to write against it.

Both retrievers now select `member.corpus_generation_id` from the joined
membership row. The value a candidate reports is a fact read from the row that
produced it.

### Three production failures that no test in this repository could see

Task 9 exposed three separate defects that every suite passed straight through.
They share one cause, and it is worth stating before the list: **each test builds
a fresh database and grants itself whatever it needs.** A defect that only
appears when data already exists, or when the caller has only production's
authority, is invisible to all of them at once.

**One — the backfill that silently did nothing, and it is Task 6's too.**
0191's backfill opened `FOR workspace IN SELECT id FROM workspaces`, then set
`vestrace.workspace_id` inside the loop. `workspaces` has FORCE ROW LEVEL
SECURITY since 0003, and `docker-compose.yml:49` migrates as `vestrace`, which
`init-runtime-role.sh` creates NOBYPASSRLS. The outer scan returned zero rows and
the loop body — which was correct — was never entered.

0191 is the only migration in the repository that reads `workspaces`; there was
no pattern to follow and the one invented here does not work. **The same defect
exists in 0188, which is Task 6, accepted and already pushed**: line 269
backfills `model_binding_snapshot_scopes` by selecting from
`model_binding_snapshots`, which has had FORCE RLS since 0178, and inserts zero
rows under the migration role.

Both fail closed, which is the only good news: `vestrace_snapshot_scope` returns
NULL and all three consumers reject on `IS DISTINCT FROM 'ordinary'`, and the
resolver refuses with "has no Ready generation". The upgrade failure mode is
refusal, not wrong answers.

No cross-workspace mechanism was invented to fix it. This system never
enumerates across workspaces — the application contains no query against
`workspaces` at all — and a BYPASSRLS role or a loosened policy would be a
permanent hole bought for a one-time convenience. Both backfills now use the
role-capability guard this package already used four times in 0187–0190: if the
role can see across workspaces, backfill; if not, skip with a `RAISE WARNING`
naming what was skipped. The migration succeeds either way and never again
succeeds while doing nothing.

**Two — the ACL gap a fixture was hiding.** The membership validator joins
`memory_embeddings`, which 0187 deliberately leaves under legacy runtime
ownership. In production the validator is owned by `vestrace_guarded_owner` and
runs inside a SECURITY DEFINER call, so it cannot read that table: **no
membership row could have been created in production at all**, no generation
would ever gain members, and retrieval would have refused forever.

Eleven passing tests passed because a fixture named
`use_test_owner_for_legacy_embedding_validation` reassigned the validator's owner
to the test superuser. The fixture is deleted, not kept as a fallback. The grant
follows the pattern already at 0171:23, 0174:960, 0180:439-440 and 0184:279, but
narrowed to `GRANT SELECT (id, workspace_id, space_id)` — the validator needs
identity columns and has no business reading `embedding`. Red was reproduced
after the deletion and before the grant, on a real store enrolment.

**Three — Task 4's worker composition was never done.** Task 4's step-4 review
item required "that the worker composes one executor that serves both callers".
Its test proves the *authority* is shared — the same repository reconstructs an
`Embeddings` request shape — which is real and stands. But no worker composes it:
`worker.rs:423-455` holds only the legacy `EmbedMemoryHandler` wiring, and the
live embedding path calls the raw provider and `store.upsert` directly with no
job, MRE or dispatch. The machinery built by Tasks 1–8 is an HTTP-reachable
governed surface with no internal driver. This is recorded, not fixed: building
that worker is beyond Task 9 and beyond P04's stated scope. It is also why Task
11's fifth fault point is unreachable — one root cause, two symptoms.

### A position recorded where a rule was needed, for the fifth time

0191 called its guarded helpers from the backfill while they were still owned by
`vestrace`, so `SECURITY DEFINER` ran as `vestrace` and
`vestrace_reject_raw_p03_mutation()` correctly refused with 42501. The guard was
working.

The cause was a lesson from Task 8 written down wrongly. 0190 had handed its
functions back *before* it finished replacing them and lost the right to replace
them; that was recorded as "hand back last", which is a position. The rule is
**hand back after every definition is complete and before the first call that
needs guarded ownership** — and the backfill, which only calls, belongs after.
`vestrace_prepare_p04_generation_fence_upgrade()` turned out to be a two-way
toggle rather than a one-way hand-back, which is why the ordering had a correct
answer at all.

This is the fifth time in this package a number or a position was recorded where
a rule was needed.

### A3, performed

Seven rounds claimed the fence; none had broken it. With
`AND member.corpus_generation_id = $6` removed from the vector SQL, the suite
fails and names the intruding candidate and its generation individually rather
than reporting a count of mismatches — `assert_candidate_generations` prints the
offending `memory_id`. Restored, `vector_retriever.rs` returns to
`5C0F400EFDC393C6ABD2FB94ED6309A830C378A4A101B6187CE2B884D894E012`. The earlier
`45600A55…` is void as a restore target: it predates the provenance correction.

A2 honours its three constraints, and two of them are established rather than
assumed: `channel_limit > 2` is asserted, and the requirement that a
stale-generation row would rank inside that limit is *measured* by asking the
database for both distances through its own `<=>` operator and asserting
`stale_distance < ready_distance`.

### A6, both branches

Branch (a): under a role that can see across workspaces, the backfill produces a
Ready generation containing a pre-0191 embedding and retrieval serves it. Branch
(b): under the restricted runtime role production actually migrates with, 0191
succeeds, creates no generation, and retrieval refuses with exactly
`unavailable: embedding space generation-fence has no Ready generation`.

Branch (b) is the more valuable of the two and the first test in this repository
that exercises an upgrade over pre-existing data. Its restricted connection is
derived from the test pool's own `connect_options()` with only the username and
password overridden, which is what keeps it from reaching the base database and
passing on `42P01` instead of `42501`.

### Runs, re-executed by the reviewer

| Suite | Result |
|---|---|
| `retrieval_generation_fence` | 12 passed |
| `text_retriever` | 7 passed, 15 assertions |
| `vector_retriever_data_policy` | 3 passed, 7 assertions |
| `embedding_schema_contract` | 10 passed |
| `p03_upgrade_provisioning` | 6 passed |
| `embedding_transition_planning` | 7 passed |
| `embedding_dispatch_is_atomic` | 15 passed |
| `runtime_role_cannot_write_directly` | 45 passed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets` | 0 warnings |
| P04-scoped dirty-baseline check | exit 0 |

The reviewer's own first fence run reported 10 of 12 and both A6 branches
failing. The cause was the reviewer's environment, not the code: those two tests
read `VESTRACE_RUNTIME_DATABASE_URL`, which had not been set. With the restricted
role available the suite is 12 of 12 in 7.24 seconds.

Two figures the reviewer had been carrying as baselines were wrong: `text_retriever`
and `vector_retriever_data_policy` hold 7 and 3 tests, not 10 and 6. The
assertion counts that establish nothing was weakened — 15 and 7 — are unchanged.

## Task 10 — refusal, mixed-space and duplicate-dispatch qualification (2026-09-05)

Two new suites qualify what Tasks 1–9 built: every P04 guarded table refuses the
runtime role for a named reason, and two spaces that are indistinguishable by
model and geometry cannot leak into each other. The plan for this task was
reviewed adversarially before any code was written, and the review returned
three blockers. Two of them were errors in the plan. The third was a security
defect in already-shipped code, and it is the substance of this task.

### The registration that accepted spaces which did not exist

`vestrace_register_embedding_space` validated that its arguments were non-null
and non-empty and that the `(workspace_id, name, model, dimensions)` tuple was
idempotent. It never checked that an `embedding_spaces` row with
`id = target_space_id` existed in that workspace, or that its name, model and
dimensions matched what was being registered.

The runtime role holds EXECUTE on that function and on the generation open and
enrol functions (`init-runtime-role.sh:503-506`), and `memory_embeddings` is
deliberately left unguarded by 0187. So the runtime role could write a vector
under an arbitrary `space_id`, register a logical space naming it with any name
and model, enrol it, publish, and retrieval — which resolves a space by name and
model — would serve it. The membership trigger does not catch this: it compares
the registration's `space_id` against the embedding's, and under a forge both
hold the same invented value.

That is a mixed-space leak available to the exact role the fence exists to
constrain, and it was reachable before this task.

**It was not theoretical: three test files were already using it.** Fixing the
function turned `embedding_schema_contract` from 10/10 to 8/10, because
`a_space_has_at_most_one_ready_generation` and
`registration_is_idempotent_on_the_tuple_and_refuses_a_second_space` had been
registering spaces with a freshly minted `Uuid::now_v7()` since they were
written, and `embedding_carry_classification` had been passing a connection id
where a space id belonged. None of that was noticed by the tests themselves, by
the five adversarial reviews Task 9 went through, or by any acceptance this
package has performed.

The fixtures were corrected to create the legacy row first, as the production
store does. The validation was not relaxed and no assertion was weakened. One
call site was deliberately left registering a space that does not exist — the
refusal probe in `embedding_runtime_role_refusals`, which exists to prove the
refusal happens. Telling that apart from a fixture leaning on the hole is
exactly where a "fix" can quietly delete its own proof.

The check needs
`GRANT SELECT (id, workspace_id, name, model, dimensions) ON embedding_spaces TO
vestrace_guarded_owner` — column-narrowed for the same reason Task 9 narrowed
the `memory_embeddings` grant: the validator needs identity and has no business
reading vectors. 0187 was amended rather than a new migration added; it is this
package's own and unreleased, and 0188's checksum had already changed, so the
"rebuild any database that applied the earlier file" consequence was already
recorded and gains nothing new.

### Two acceptance criteria that could not have been met as written

**A3 was impossible.** The original wording asked for two spaces
"distinguishable only by pinned identities". `embedding_space_registrations`
enforces `UNIQUE (workspace_id, name, model, dimensions)` (0187:28) and the
resolver looks a space up by name and model, so two same-model, same-dimension
spaces are *required* to differ by name. The criterion was rewritten to the
property that is both achievable and worth having: nothing about the vectors,
the provider or the wire model distinguishes them — both are
`same-wire-model` at 2 dimensions — so any leak would have to come from the
identity plumbing rather than from the geometry.

**A4's counter would have counted nothing.** The original asked for zero further
adapter calls across a crash, read from a real counter. No worker composes an
embedding-job executor, so an embedding recovery never invokes an adapter and
the counter would have established only that an uncalled test double stayed
uncalled. The duplicate-dispatch property is proved at the transition level
instead, and this evidence states plainly that no embedding adapter call was
counted, because the missing worker recorded in Task 9's evidence is the reason.

### The closed-world test cannot detect the forgetting that matters

The catalog-derived test in `provider_runtime_role_refusals.rs` selects tables
where `pg_get_userbyid(class.relowner) = 'vestrace_guarded_owner'` and then
asserts `tables.len() >= 64`. Its own doc comment says "the cost of forgetting
is a red test rather than a silent write path". That is true for one kind of
forgetting — a table handed to the guarded owner but declared in no matrix — and
false for the kind that matters more: a table never handed over at all simply
does not appear in the query, and 64 remains satisfied because P02 and P03
supply 64 on their own.

This document previously repeated that claim. It was wrong. The test now also
asserts, without any change to its catalog derivation, that all twelve P04
tables appear in the derived set, naming any that is missing.

### Runs, re-executed by the reviewer

| Suite | Result |
|---|---|
| `embedding_runtime_role_refusals` | 2 passed |
| `embedding_space_isolation` | 4 passed |
| `provider_runtime_role_refusals` | 2 passed |
| `embedding_schema_contract` | 10 passed |
| `embedding_carry_classification` | 8 passed |
| `retrieval_generation_fence` | 12 passed |
| `p03_upgrade_provisioning` | 6 passed |
| `runtime_role_cannot_write_directly` | 45 passed |
| `embedding_dispatch_is_atomic` | 15 passed |
| `embedding_effect_recovery` | 11 passed |
| `embedding_transition_planning` | 7 passed |
| `embedding_transition_barriers` | 10 passed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets` | 0 warnings |
| P04-scoped dirty-baseline check | exit 0 |

Mutations performed and restored: a temporary runtime INSERT grant on
`embedding_jobs` made the refusal suite fail on its catalog-privilege assertion;
replacing the generation/member space comparison with `IF FALSE` made the
cross-space test fail reporting `rows_affected: 1`. 0187 restored to
`4F899665EBF6F5AC8F58B00DA2EF08AF18ED9C9A42B3CE7DCF54EECE7ACD3543` and 0191 to
`2A658619F599BDF23176241FC16EF3C876AA6DDDD653F236C5BD9E730A0C990F`.

`embedding_schema_contract`'s regression was caught by a run the reviewer added
because 0187 had changed, not by the verification matrix the reviewer had given
the implementer. The matrix was the reviewer's and the omission was the
reviewer's.

### A fixture that reduces its own authority

`prepare_legacy_embedding_runtime_ownership` in the shared test module hands the
legacy tables to `vestrace` before the runtime paths are exercised, mirroring
what the deployment bootstrap does, and asserts the resulting owner. SQLx
provisions its disposable databases as the test migrator, which owns everything;
that is precisely the discrepancy that hid three separate production failures in
Task 9. This fixture moves a test toward production authority instead of away
from it, and it is the pattern the remaining tasks should follow.

## Task 11 — worker-restart and fault evidence (2026-09-05)

A real child process runs real services, drives a governed embedding dispatch to
a named point, and dies by `std::process::abort()`. A parent then reads back
what survived and a loopback listener outside the process counts what the
provider saw.

### The plan this task nearly shipped instead

The first version of this task's plan declined to build the standalone scenario
and proposed proving two of the five fault points *impossible* using
`ProviderDispatchFaultInjector`, calling that substitute "stronger" than the
evidence the task asks for. Its own adversarial review rejected it on three
counts, and all three were right.

**Its central premise was false.** It claimed points 1 and 2 were structurally
unreachable because `provider_dispatch_repository.rs:649-753` writes intent,
authorization, admission, dispatch transition and the governed mutation through
one unit of work with a single commit. That is the *allowed* branch. The
**denied** branch at `provider_dispatch_repository.rs:546-585` durably writes
intent and authorization, commits, and returns with no dispatch transition — so
a durable authorization without a dispatch is an ordinary outcome, and the
universal claim was drawn from one branch.

**The intent pre-exists.** An accepted embedding job persists its effect intent
at acceptance, and the dispatch-side insert is `ON CONFLICT DO NOTHING`. "No
intent row survives" could never have been observed at `AfterIntent`, whatever
the transaction did.

**And the substitution was not the builder's to make.** An injected
`ApplicationError` proves in-process transactional rollback on an error return.
It does not prove abort semantics, the loss of in-memory state, a fresh
process's recovery, or a count taken outside the process. Naming one a stronger
form of the other is the same substitution of one property for another that this
package caught seven times in other people's work; this was the eighth, and the
first written by the reviewer. The operator was asked, because the P04 plan
reserves this decision for them, and chose to pay the cost.

### What the scenario does

The fixture is built inside the scenario file — 31 setup operations covering
tenancy, guarded connection and model revisions, space registration, the
accepted job's pre-existing durable intent, qualification and snapshot fixtures,
acceptance, admission policy and complete evidence.

An earlier attempt reached for the existing fixture with
`#[path = "../../../vestrace-infrastructure/tests/common/mod.rs"]`, pulling
another crate's test module into a binary crate. That produced sixteen compile
errors, and adding the missing dependencies would have meant amending a manifest
outside the change scope to couple a shipping binary to test-only code. The two
existing scenarios are self-contained for a reason and this one now is too. No
manifest was changed and the twelfth scope amendment was not needed.

### Results, re-executed by the reviewer

Every count below is read back from PostgreSQL or counted by the loopback
listener; none is stated by the process under test, which is this crate's own
stated rule and a test forbids violating it by name.

| Point | Surviving state |
|---|---|
| `after_intent_persistence` | intent 1; authorization, admission, dispatch, receipt, loopback all 0 |
| `after_authorization_before_dispatch` | intent 1; authorization, admission, dispatch, receipt, loopback all 0 |
| `after_dispatch_before_receipt` | intent 1; authorization, admission, dispatch, receipt, loopback all 0 |
| `after_receipt_before_outcome_confirmation` | authorization, admission, dispatch, deadline, receipt and loopback all 1 |
| `after_outcome_before_run_commit` | **unproved** |

A control child runs beside every one of them and reports authorization 1,
admission 1, dispatch 1, deadline 1. That is what makes the zeros mean
something: an absence is only evidence when the presence has been shown.

`after_dispatch_before_receipt` reports zero loopback requests despite its name.
That is correct and worth stating rather than glossing: the dispatch transition
is written inside the same transaction, and the provider is reached only after
that transaction commits. The point names a position in the code, not a call
that crossed the wire.

### The mutation that makes this a proof

Without it the scenario would pass whether the governed dispatch were atomic or
not, which is what "unfalsifiable" means and what Task 8's race test turned out
to be. Inverting an assertion was explicitly refused as a substitute: that
proves only that an assertion can fail.

The authorization was committed before the abort, and the scenario then FAILED
at `after_authorization_before_dispatch` reporting `authorization_count=1`,
`admission_count=1`, `dispatching_count=0` — a surviving authorization without
a dispatch. Restored, the file returns to
`7C86C1309391EE932A6E6B0E5383EE8B4A8D14557BF9DA9C09A191EAB948A08C`.

### What is not proved, and why

`after_outcome_before_run_commit` has nothing to crash. No worker composes an
embedding-job executor: `worker.rs:119-134` composes a Run-step executor and
`worker.rs:423-455` the legacy `EmbedMemoryHandler`, which calls the embedding
provider and upserts directly. This is the Task 4 debt recorded in Task 9's
evidence, surfacing a second time. It is a finding about the system, not a
narrowed exit criterion, and no worker was invented to make the point reachable.

The scenario's fixture seeds qualification and snapshot rows as
`vestrace_guarded_owner` because their producers do not exist. So what is proved
is the crash behaviour of a state that production cannot yet lawfully construct
by itself. That is a real limitation and it is stated here rather than left for
a reader to infer.

## Task 12 — integrated verification (2026-09-05)

Source revision `834805d41a6e59c3c3cc93df3b7bbc0b5ee7f707`. Preflight
`docs/development-evidence/v1-g0-04-preflight.json`, SHA-256
`808b80f00e4a82447273171e9b2713a692b95c400ec94d8dc64cb4bd9c4cc42f`, captured
against head `6aba953c684fc60ad822cafa283f3cda8966e799`, holding 102 scoped
paths after twelve amendments and 343 dirty files.

### The sweep

Every command run by the reviewer, each in its own invocation.

| Gate | Result |
|---|---|
| `verify-dirty-baseline.mjs --check` with `p04-scope.mjs` | exit 0 |
| `node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs` | 16 passed |
| `scripts/protocol-lock.mjs --check` | exit 0 |
| `scripts/verify-p01-text-hygiene.mjs --check` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0, 3m20s |
| `cargo test --workspace --locked` | **219 suites, 1901 passed, 0 failed** |
| `node --test apps/console/tests/providerClientContract.test.mjs` | 3 passed |
| `npm --prefix apps/console run typecheck` | exit 0 |
| `cargo test --test compose_smoke -- --ignored --test-threads=1` | 4 passed, 425.92s |
| `git diff --check` | exit 0 |

All 23 protected authority paths were recomputed from the tree: 23 of 23 match
by digest and byte count, against zero authorized revisions.

`compose_smoke` rebuilt the release images — `vestrace-server`,
`vestrace-worker` and `vestrace-migrate`, all stamped 18:10:58 during the run.
The plan carries a warning that this is the only gate in the repository that
compiles the release profile, and that P03 once shipped a tree whose release
build failed while every other gate was green. This tree's release build
compiles.

### What the sweep found, which is the point of having it

Three defects in work already reviewed, accepted and pushed.

**Task 9 broke seven tests and the reviewer's matrix missed all of them.** The
generation fence made the corpus-generation resolver mandatory, and
`RetrievalService::search` now refuses with
`Unavailable("retrieval corpus generation resolver is not configured")` when
none is set. That refusal is correct and was kept. But six unit tests in
`crates/vestrace-application/src/retrieval/service.rs` and one in
`crates/vestrace-infrastructure/tests/retrieval_classification_boundary.rs`
construct the service without one. Task 9's verification covered infrastructure
suites and never ran `cargo test -p vestrace-application --lib`, although Task 9
changed that very crate's retrieval service. The matrix was the reviewer's and
so was the omission.

**And it broke a production path.** `crates/vestrace-cli/src/commands/server.rs`
was wired with the resolver; `crates/vestrace-cli/src/commands/mcp.rs` was not.
Every retrieval through the MCP server refused, for two pushes. It failed
closed, so no wrong answer was ever returned — but no answer was returned
either. Both composition roots now take the space and model names from
`AppConfig`, so they cannot drift apart silently again.

Neither was found by five adversarial reviews of Task 9, nor by any acceptance
this package performed. Both were found by the one run that looks at the whole
tree rather than at chosen suites — and only once its output stopped being
filtered. The plan carries that warning from P03 for exactly this reason.

**Two failures were environmental and were proved so rather than assumed.**
`installation_permit_excludes::exclusive_excludes_shared` failed once with
`duplicate key value violates unique constraint "databases_pkey"` in
`_sqlx_test.databases` — sqlx's own disposable-database bookkeeping colliding
under a parallel workspace run. `q9_key_provider_trust_cli` failed once on
`sign.status.success()` while a builder was rebuilding the CLI into a separate
target directory. Each passes in isolation; each was re-run rather than
dismissed.

### What P04 established

- Guarded embedding jobs, space registrations, and corpus generations with a
  `building -> ready -> stale` lifecycle.
- Embedding transitions with recipe-granular classification, ambiguity carry,
  and barriers with a closed five-state lifecycle.
- The retrieval generation fence: both channels filter by membership in a
  resolved Ready generation and report the generation they actually read, and
  fusion refuses a candidate carrying any other.
- Runtime-role refusal for all twelve new guarded tables, mixed-space isolation
  for two spaces sharing a wire model and dimensions, and a real
  `std::process::abort()` fault scenario for governed embedding dispatch.

### Dispatch reuses P03's authorities and adds none of its own

The five guarded functions an embedding dispatch calls are exactly the five a
Run step calls. 0187 does contain a whole-body `CREATE OR REPLACE` of
`vestrace_try_admit_provider_dispatch` with the same signature, which is not a
second admission function: 0184 is historical and immutable so a forward
migration cannot patch it, and P03 itself set this precedent in 0185 when
qualification probes became a second dispatch cause. Embedding is the third.
No second admission, throttle, credential-lease or recovery authority exists.

### A production path does publish a connection admission policy

`POST /v1/connections/{id}/admission-policies` is mounted at
`crates/vestrace-http/src/api/connections.rs:133-136`, its handler calls
`publish_admission_policy_governed`, and `connection_routes()` is merged into
the router at `crates/vestrace-http/src/api/mod.rs:236`. Publication runs
through `ConnectionAdmissionPolicyMutation`, inside P02's atomic authority. So
neither Run-step nor embedding dispatch is unreachable in production for want of
a policy. This was checked against the tree rather than carried forward from
P03's record, where the same line stood as a limitation.

### What P04 did not do

No backup, no WAL archive, no restore or activation supervisor. No console
screen, no AG-UI change, no A2A change, no real Agent publication. No call to LM
Studio and none to any remote provider: every provider interaction in this
package is a loopback stub inside the test process.

Loopback success is semantic development evidence. It is not real-provider
evidence and it is not release evidence.

P02's pre-existing-table ownership obligation and its fingerprint backup
obligation are untouched here and remain P05's.

## Not true yet

- **P04 is reopened as of 2026-09-06.** Tasks 1–12 are complete and reviewed
  apart from the concurrency mutation named below, but the package did not meet
  its own gate-program row: four of six embedding-job states are unreachable,
  there is no completion authority, and the specification's
  `RetrievalGenerationFence` does not exist. See "Reopened" at the end of this
  document.
- The P04 plan has not been independently reviewed, and P03 Tasks 11–13 have not
  been adversarially reviewed. Both were carried into this package and neither
  was discharged by it.
- The fifth external-effect fault point, `AfterOutcomeBeforeRunCommit`, is
  **unproved**. Nothing composes an embedding-job executor, so there is no
  worker to crash at that boundary.
- Task 11's scenario proves the crash behaviour of a state whose qualification
  and snapshot rows are seeded as `vestrace_guarded_owner` because their
  producers do not exist. Production cannot yet construct that state lawfully by
  itself.
- Duplicate dispatch is proved at the transition level and **not** by an adapter
  call counter. No worker composes an embedding-job executor, so an embedding
  recovery never reaches an adapter and a counter would only establish that an
  uncalled test double stayed uncalled.
- The catalog-derived closed-world refusal test cannot detect a table that was
  never handed to `vestrace_guarded_owner`: such a table is absent from its
  query and its `>= 64` floor is satisfied by P02 and P03 alone. The twelve P04
  tables are now asserted by name, which covers this package but not the next
  one.
- No worker composes the embedding-job executor. Task 4's step-4 review item
  required it, its test proves only that the dispatch *authority* is shared, and
  `worker.rs:423-455` still holds only the legacy `EmbedMemoryHandler` wiring.
  The live embedding path calls the raw provider and `store.upsert` directly. The
  job machinery is an HTTP-reachable governed surface with no internal driver.
- There is no operator backfill command. A database upgraded under the
  restricted runtime role gets no corpus generations and refuses retrieval until
  one exists. The only shape compatible with a system that never enumerates
  across workspaces is a per-workspace operator-invoked command, and it is
  outside this package's change scope.
- `embedding_spaces` and `memory_embeddings` are not guarded-owner tables and
  escape the catalog-derived closed-world refusal test. 0187 preserves their
  legacy ownership deliberately. The runtime role can write `memory_embeddings`
  directly; the fence survives only because retrieval requires a membership row
  and membership is guarded.
- Modifying 0188 changed its checksum, so any database that had already applied
  the earlier 0188 will refuse to migrate and must be rebuilt. Test databases are
  built fresh per test and are unaffected; a persistent Compose volume is not.
- Task 8's classifier-versus-supersession race passes and is **not**
  independently mutation-proven, and the attempt to prove it established why: the
  locks it targets are not load-bearing. Removing the barrier-header lock, and
  then both planner-side locks together with an advisory-lock handshake proving
  the two paths genuinely interleave, left the suite passing. The invariant is
  held by the partial unique index on the open state, which the database enforces
  whatever the interleaving — and that mutation *was* performed: with the index
  removed, the second open barrier inserts and the test fails. So the property is
  proven, but by a different guard than the acceptance criterion assumed, and the
  race test itself remains a scenario rather than a proof.
- The barrier's dedicated batch is nondispatchable and is **not** completeness
  blocking, because no completeness surface exists to block.
- `waiting_for_result_keys` and `EmbeddingJobResultPrepared` are named by the
  specification and by this repository's comments, and exist nowhere in it.
- A `Succeeded` predecessor is refused outright rather than released against
  `SatisfiedExisting` or definite-resolution evidence. Those are barrier states
  and belong to Task 8; until then the rule is stricter than the specification,
  never looser.
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

## Reopened (2026-09-06)

P04 was declared complete on 2026-09-05 and pushed. Planning P05 required
reading specification section 11.6 against this package's output, and that
reading found P04 had not met its own gate-program row. The operator decided to
reopen it rather than carry the gap forward. This section records what was
found; the sections above are left as written.

### Four of six embedding-job states are unreachable

`EmbeddingJobState` declares `Requested`, `Running`, `Succeeded`,
`FailedDefinite`, `InconclusiveUnknown` and `Cancelled`
(`crates/vestrace-domain/src/embedding/job.rs:98-105`). The only
`UPDATE embedding_jobs` in the entire migration set is
`migrations/0187_embedding_jobs_and_corpus_generations.sql:819-821`, inside
`vestrace_finalize_embedding_job_unknown`, writing `inconclusive_unknown`.
`Requested` arrives from the column default at `0187:103`.

Nothing writes `Running`, `Succeeded`, `FailedDefinite` or `Cancelled`. An
embedding job can be accepted and declared unknown, and nothing else. The
evidence above describes the job lifecycle as built; what was built is its
acceptance and its unknown finalization.

### There is no completion authority, and the specification names a chain

Section 11.6 requires, before `Succeeded`: one repository transaction that
writes each ciphertext and its non-live `ResultFinalizing`
`EmbeddingProjectionEntry` as a provider-result-specialized
`PreparedMaterialAttachment`, appends the definite provider receipt and the
immutable `EmbeddingJobResultPrepared` marker, and **deliberately neither
activates a projection nor appends `Succeeded`**; then a reconciler that binds
each provisional key to that exact marker and records bound receipts; then one
database finalizer that atomically promotes every attachment to typed
vector/projection rows and `Live` materials, advances the space corpus, marks
the current Ready generation stale, emits the rebuild event, and appends
`Succeeded`.

Of the artefacts that chain names, the generic material-attachment machinery
exists from P02 (`prepared_material_attachment` in four migrations). Everything
embedding-specific does not: `EmbeddingProjectionEntry`,
`RetrievalGenerationFence`, `RetrievalGenerationChanged`, and — as this
document already recorded under Task 8 — `EmbeddingJobResultPrepared` and
`waiting_for_result_keys`, which appear only in comments.

`PreparedProviderResult` is Run/Step-bound
(`crates/vestrace-application/src/provider_result.rs:117-126`) and its finalizer
commits a Run result
(`crates/vestrace-infrastructure/src/postgres/provider_result_repository.rs:444-479`),
so it cannot serve as the embedding completion authority.

### "Retrieval generation fences" was built in a different sense than the row means

The gate program's P04 row promises retrieval generation fences. Task 9 built
the corpus-generation filter: both retrievers restrict candidates to membership
in a resolved Ready generation and report the generation they read. That is real
and it is proved.

Section 11.6's `RetrievalGenerationFence` is a different object — an immutable
tuple captured by a `retrieval_query` job at acceptance, holding the exact
`EmbeddingSpaceKey`, the Ready generation id and epoch, the captured corpus
revision and member watermark, and the generation-guard CAS version, serving as
reconstruction and effect-input authority for one attempt. It does not exist.

Two related things share a name here, and this document previously implied the
row was satisfied.

### What reopening covers

The unbuilt half of this package's own row: the job completion chain, the
terminal transitions other than `InconclusiveUnknown`, the cancellation path,
the pre-dispatch `FailedDefinite` paths, the waiting-for-result-keys phase, and
the specification's `RetrievalGenerationFence`.

It also covers the driver debts this package recorded and did not close: nothing
composes an embedding-job executor
(`crates/vestrace-cli/src/commands/worker.rs:102-134`, `:416-455`).

An earlier draft of this correction claimed the composition root could not be
tested by any Rust test and that this was why the driver defects escaped. That
is false and is not the reason. Integration tests already launch the binary
through `CARGO_BIN_EXE_vestrace`, `RunWorker::run_once` and
`OutboxDispatcher::drain_once` are public, and `command_contract.rs` and
`runtime_schema_gate.rs` inspect composition too. The means existed; the tests
were not written.

### Task 13 continuation contract — bounded worker cycle (2026-09-06)

**Goal.** Finish the existing `worker --once` change authorized by the last
scope amendment in `v1-g0-04r-preflight.json`. This is a bounded CLI step, not
acceptance of reopened P04. The root `PLAN.md` describes unrelated Slice 18;
this contract is recorded in an already-admitted evidence path instead of
changing that out-of-scope file or the frozen package plan.

**Requirements and constraints.** Run one sequential cycle over the configured
workspaces and existing run, outbox, reconciliation and outcome-delivery polls.
Exit 0 when work completes, 3 when idle, and 1 when a poll or delivery fails;
failure takes precedence over successful work elsewhere in the same cycle.
Keep ordinary worker behavior and presence cleanup. Preserve existing changes,
the 105-path scope, protected authorities and historical migrations. No commit,
push, deployment or non-loopback provider call.

**Non-goals.** Embedding executor composition, completion authority, new job
states and retrieval-attempt fences remain later reopened-P04 work.

**Implementation plan.** Continue only in
`crates/vestrace-cli/src/commands/worker.rs`,
`crates/vestrace-cli/src/main.rs` and
`crates/vestrace-cli/tests/worker_once_cli.rs`; record results here. Preserve the
existing scope amendment. Correct aggregation of failed outbox reports. Repair
the disposable SQLx fixture to reproduce deployment ownership for the specific
legacy tables it needs, without granting writes to guarded tables or running
the child as an administrator. Launch the real binary for regression evidence.

**Acceptance and verification.** Observe all three exit codes on real
PostgreSQL, including a delivery failure after successful startup, persisted
attempt/pending state, error precedence in a mixed cycle, and cleared worker
presence. Prove one-cycle behavior using durable queue state. First run the
new failure regression against the existing implementation (behavioral RED),
then apply the correction and observe GREEN. Use a bounded child-process wait
so a regression cannot strand the suite. Run the focused CLI suite, relevant
CLI contracts, format and Clippy checks, P04 scope tests, the reopened baseline
verifier, protocol lock and diff hygiene. Record exact results and distinguish
focused evidence from full-package acceptance.

**Independent review.** The read-only cdx reviewer returned `VERDICT: REVISE`:
failed deliveries in `DrainReport` were ignored, and the existing unreachable-
database test covered startup failure rather than a failed poll. Both findings
are accepted for correction. An initial live run also found that SQLx's
administrative migration owner leaves the runtime child unable to register
worker presence; fixture ownership must be established before behavior can be
tested. No acceptance is claimed at this point.

### Task 13 bounded-cycle result (2026-09-06)

**Accepted for this step only.** `vestrace worker --once` runs one sequential
poll cycle and returns 0 for work, 3 for an idle cycle, or 1 for a reported
poll/delivery failure. Failed outbox deliveries now take precedence over work
completed by another poll. Ordinary continuous worker scheduling is preserved.
The existing ports still treat unhandled topics and deferred outcome debts as
pending work; these are not reclassified by this CLI change. A processed Run
work item can itself be a valid retry/dead-letter decision, so exit 0 is not a
claim that an entire Run or embedding job succeeded.

The real-binary regression seeds one Run work item and 33 malformed
`memory.created` messages. It observes the Run item completed, exactly 32
outbox messages with one durable attempt and one untouched message, a failed
message still unprocessed and not dead-lettered with the expected error, and
one worker-presence row with none active after process exit. The child uses
the `vestrace` runtime login; nine explicitly named legacy tables receive their
deployment owner in the disposable SQLx database, with a precondition refusing
any guarded-owner table. No guarded ACL, migration or production role is
changed. Child output goes to fixture files and the parent kills/reaps a child
that exceeds the 15-second test deadline.

The builder first ran the new regression against the old aggregation. Its
reported behavioral RED exited 101: the child returned 0 while the assertion
required 1, after logging the malformed delivery failures. The persisted-state
assertions follow that assertion and were established by GREEN, not RED.
Adding `outcome.failed |= report.failed > 0` made the same focused command pass
(1 passed, 3 filtered, exit 0). The independent lead then ran the broader checks:

| Command | Observed result |
| --- | --- |
| `cargo test -p vestrace-cli --test worker_once_cli --test cli --test command_contract --test provider_runtime_wiring --test runtime_schema_gate --locked -- --test-threads=1` | exit 0; 33 passed, 0 failed (4 + 5 + 15 + 5 + 4), real local PostgreSQL |
| `cargo fmt --all -- --check` | initial exit 1 for the new test file only; after scoped rustfmt, exit 0 |
| `cargo clippy -p vestrace-cli --all-targets --all-features --locked -- -D warnings` | exit 0 |
| `node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs` | exit 0; 16 passed, 0 failed |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04r-preflight.json --scope p04-scope.mjs` | exit 0 |
| `node scripts/protocol-lock.mjs --check .` | exit 0 |
| `git -c safe.directory=E:/Soft/vestrace diff --check` | exit 0 |

The initial three-test suite had 1 pass and 2 setup failures: first the shell
lacked `DATABASE_URL`, then the correctly connected runtime child could not
register presence in an administratively migrated test database. Those are
setup failures, not behavioral RED. A test-only `chrono` reference also failed
compilation and was replaced with SQL boolean predicates; no dependency was
added. Test URLs were supplied only to the test processes from the local
PostgreSQL container configuration and the existing local runtime credential.

The final independent cdx reviewer returned **VERDICT: APPROVE**, with no
remaining meaningful findings. This continuation changed only `worker.rs`,
`worker_once_cli.rs` and this evidence document; the earlier `main.rs`, scope
test and preflight changes remain part of the existing dirty task. The 105-path
scope and protected authority bytes were preserved.

Full workspace tests, Compose smoke and reopened-P04 acceptance were not run or
claimed by this bounded step. Embedding executor composition, completion
authority, the missing terminal transitions and retrieval-attempt fences remain
open. No commit, push, deployment or external provider call was performed.

### Task 14A contract — pre-dispatch job-state edges (2026-09-06)

**Goal.** Correct the domain lifecycle before implementing durable pre-dispatch
termination. `EmbeddingJobState::may_advance_to` currently refuses cancellation
from `Requested` and claims that the spec forbids it. Frozen spec section 11.6,
line 225, explicitly allows cancellation from `Requested` and requires proved
pre-dispatch failures to reach `FailedDefinite`.

**Requirements.** Admit `Requested -> Cancelled` and
`Requested -> FailedDefinite`, retaining `Requested -> Running` and the existing
`Running` terminal edges. Keep the six-state vocabulary unchanged, terminal
states absorbing, and direct `Requested -> Succeeded` or
`Requested -> InconclusiveUnknown` refused. Document that this predicate only
describes state edges: authorization, expected version, absence of Dispatching,
lease release and reservation abandonment still belong to the guarded command
and are not proved by a state enum.

**Scope and non-goals.** Only
`crates/vestrace-domain/src/embedding/job.rs`,
`crates/vestrace-domain/tests/embedding_contract.rs` and this evidence document.
No database write path, cancellation endpoint, executor, migration, successor
policy or scope amendment is implemented by this correction. Preserve the
accepted Task 13 changes and every protected authority. The unrelated root
`PLAN.md` remains outside scope.

**Implementation and acceptance.** Replace the stale negative cancellation
assertion with specification-derived regression coverage for both lawful
pre-dispatch terminal outcomes. Cover the full six-by-six state transition
matrix against an explicit allowed-edge list, including all forbidden terminal
exits and self-transitions. Observe test assertion RED with the current domain
implementation, apply the smallest correction, then GREEN. Temporarily remove
each new edge independently and observe its regression fail; restore exact
bytes and rerun GREEN. Run the domain test suite, format, domain Clippy,
reopened-baseline verification and diff hygiene. Independently review the final
diff. Report this as a domain correction, not a working cancellation command or
completed P04 lifecycle.

### Task 14A result — domain pre-dispatch edges (2026-09-06)

The domain predicate now admits `Requested -> Cancelled` and
`Requested -> FailedDefinite`. Its other edges, six-state vocabulary, absorbing
terminal states and duplicate-charge successor policy are unchanged. The
regression suite enumerates all 36 state pairs against seven allowed edges and
has a focused assertion for each new edge. The documentation identifies the
guarded command, rather than this predicate, as the enforcement point for
pre-dispatch termination. Review narrowed that wording so it does not impose
the absence of Dispatching on unrelated completion transitions.

The builder reported the following behavioral checks. The first RED used the
new assertions against the old implementation; it was not a build failure.

| Command / implementation | Observed builder result |
| --- | --- |
| `cargo test -p vestrace-domain --test embedding_contract --locked`, old implementation | exit 101; 11 passed, 3 failed (matrix and both new assertions) |
| Same command, corrected implementation | exit 0; 14 passed |
| Same command with filter `a_requested_job_may_be_cancelled_before_dispatch`, only Cancelled edge removed | exit 101; 0 passed, 1 failed, 13 filtered |
| Same filtered command after exact restoration | exit 0; 1 passed, 13 filtered |
| Same command with filter `a_requested_job_may_fail_definitely_before_dispatch`, only FailedDefinite edge removed | exit 101; 0 passed, 1 failed, 13 filtered |
| Same filtered command after exact restoration | exit 0; 1 passed, 13 filtered |
| Unfiltered focused command after final formatting and restoration | exit 0; 14 passed |

The first mutation restored `job.rs` SHA256
`68523B2674AF2DDCDD97F21C2B163A9973814B0BFE15669C309F2893F2B7E479`.
After the documentation clarification, the second mutation restored final
`job.rs` SHA256
`77AE1B57EE35387E1C8980C273F4BBAF28ACF488F2AC8C103CC9D0D88EECBC34`.
The final test-file SHA256 is
`AE86CA582BCB57B8AE68FB88AD9838D034838FF775F8389F001F867B1A78E68E`.
The lead independently read the complete diff, verified both final hashes and
ran these checks against the restored final files:

| Command | Observed lead result |
| --- | --- |
| `cargo test -p vestrace-domain --locked` | exit 0; 277 tests plus 11 doctests passed, 0 failed, 0 ignored |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p vestrace-domain --all-targets --all-features --locked -- -D warnings` | exit 0 |
| `node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs` | exit 0; 16 passed, 0 failed |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04r-preflight.json --scope p04-scope.mjs` | exit 0 |
| `node scripts/protocol-lock.mjs --check .` | exit 0 |

After recording this result and the pending proposal below, reopened-baseline
verification and `git -c safe.directory=E:/Soft/vestrace diff --check` both
returned exit 0. The lead found no remaining blocking issue in Task 14A.
The advisory reviewer returned APPROVE for the reviewed contract, explicitly
stating that it had not re-read the final builder bytes. Its verdict is not
counted as an independent final-diff review; the lead performed that review
directly against the final files. Lead acceptance for this bounded domain
correction: **APPROVED**.

Task 13's six dirty code/test/scope/preflight files retain their entry hashes.
Only the two domain files and this evidence document changed in Task 14A.
No database termination command, executor or result finalizer was implemented
or claimed. Full workspace and PostgreSQL acceptance were not rerun for this
pure domain correction; reopened P04 remains incomplete.

### Proposed Task 14B scope amendment — pending user authorization

**Next bounded behavior.** Persist cancellation and proved definite failure
before provider dispatch through the existing governed embedding-job
repository boundary. This follows frozen spec section 11.6, line 225, and makes
the corrected domain edges usable by a durable command. The next contract must
cover authorization, expected-version/idempotency checks, ordered job/effect/
queue-lease locking, atomic terminal evidence and lease release, and abandonment
of only unprepared output reservations. A dispatch winner must cause a conflict;
a cancellation winner must prevent subsequent dispatch. ResultPrepared and
terminal jobs must not be rewritten as pre-dispatch failures.

**Proposed exact new path:**
`migrations/0192_embedding_job_pre_dispatch_termination.sql`.
This would amend the current 105-path list to 106. The migration must be forward
only; migrations through 0191 and protected authorities remain immutable.
Record an explicit scope amendment in the existing reopened preflight and
update `scripts/p04-scope.mjs` and its exact-count regression only after the user
authorizes this addition. No amendment has been applied by this proposal.

Implementation would use existing allowlisted application embedding-job and
provider-dispatch ports, PostgreSQL repositories, module exports, runtime-role
provisioning, and embedding schema/refusal/recovery tests. Before coding, write
and independently challenge the exact lock order and command contract in this
evidence document. Any additional required path must be identified explicitly;
this proposal grants no blanket permission to create files or widen manifests.

**Acceptance direction.** Use the real PostgreSQL runtime role. Prove same-key
replay, stale-version/wrong-workspace/unauthorized refusal, both orders of the
cancel-versus-dispatch race, atomic rollback, terminal history preservation,
lease release and reservation abandonment by persisted read-back. Mutating a
relevant guard or splitting the atomic operation must produce behavioral RED,
followed by exact restoration and GREEN. Cover fresh provisioning and upgrade
ownership/ACLs. Reuse the shared provider dispatch authority; do not introduce
a second scheduler, fake Run/Step ownership or a legacy adapter bypass.

The delivery/rebuild ResultPrepared/binding/finalization protocol, full worker
composition, retrieval-query completion fence and full P04 acceptance remain
later work. Their implementation will need its own reviewed contract and any
further exact scope additions. The frozen plan's global constraint at lines
84–85 is explicit: every path outside the P04 allowlist is a "stop-and-ask, not
a judgement call". That constraint is the reason this new migration remains
a proposal.

### Task 14B implementation contract — durable pre-dispatch termination (2026-09-06)

**Authorization and goal.** The user answered the preceding exact 105-to-106
scope request with "continue working in larger blocks". The added migration
0192 is now recorded in the reopened preflight and scope; the preceding proposal
is retained as history. Deliver one reviewed block comprising guarded SQL,
typed application/repository commands, production composition, dispatch and
recovery fencing, runtime-role tests and mutation evidence. This is the durable
pre-dispatch foundation from spec section 11.6, line 225, not full P04 completion.

**Required behavior.** An authorized, expected-version/idempotent cancellation
can terminalize Requested or Running before any historical Dispatching exists.
A typed definite-failure command must cite and verify a durable refusal source;
initial supported sources are an exact denied external-effect authorization and
an expired/timeout provider-admission wait. Arbitrary reason strings and a caller
assertion that dispatch did not happen are not proof. Keep other failure classes
fail-closed until their own safe durable evidence producer exists; do not claim
that this block terminalizes every reconstruction/credential failure.

Cancellation authorizes through the existing configured
`SharedPolicyDecisionEngine`, using `ExecutionWrite`, operation
`embedding.job.cancel`, workspace resource scope and Low risk. This operation
controls the workspace job and does not disclose input or call a provider.
Validate the returned decision's complete context/request tuple and Allowed
outcome, record its safe identity/evidence with the command, and fail closed
if the policy dependency is absent or mismatched. Failure reconciliation is an
internal typed repository operation consuming the owning effect's persisted
denial or database-time queue expiry; it must not require the provider
authorization that has just failed. Bind both operations to workspace/principal,
job, effect, expected version and a nonblank idempotency key.

Return a typed durable termination receipt including canonical command identity,
job/version, terminal state and reason/evidence identity. Replaying the same
key and exact semantic request returns the original receipt without another
terminal record, lease release, version increment or audit. A reused key with a
different principal/job/version/outcome/reason conflicts; a different key cannot
rewrite a terminal job. Do not rely on PgGovernedMutationRepository's existing
same-hash path to skip apply/audit: it does not. Preserve that shared repository
and implement command-specific convergence under its caller-owned shared
installation permit/transaction where appropriate.

**SQL and concurrency.** Migration 0192 owns the new immutable termination
evidence and narrow SECURITY DEFINER entrypoints, with workspace RLS, forced
RLS, guarded ownership, explicit runtime grants and raw-DML refusal. No public
grant or session-variable bypass. Reuse/extend existing shared dispatch
functions through forward definitions; do not edit historical migrations.
Derive connection/auth/space identity from the job's pinned snapshot. Lock the
permanent ConnectionExecutionGuard first, optional exact credential activation
guard second, exact space guard, then job/version, effect advisory/lifecycle
authority, and its existing admission state/wait/concurrency/credential leases
in deterministic order compatible with their existing writers. Shared routing
and admission must acquire/recheck the same job fence before their effects can
survive. Keep all Run and qualification branches semantically unchanged.

The same refusal must exist at actual `external_effect_lifecycle_transitions`
Dispatching persistence, including direct ExternalEffectRepository calls that
bypass routing/admission. The existing deferred deadline check only matches
historical admission; it does not prove a live lease or nonterminal embedding
job. Cover direct lifecycle insertion after admitted-job cancellation and
serialize it without a reverse effect-to-connection lock order.

Refuse any historical Dispatching, receipt, prepared-result evidence, unknown or
other terminal job. An admitted lease without Dispatching is cancellable: do
not equate admission with a provider call. Cancellation first leaves no possible
dispatch; dispatch first makes cancellation conflict. Close an applicable wait
and release existing concurrency/credential leases atomically with the terminal
fact and state/version. Preserve closed lease vocabularies and receipt-bearing
post-dispatch release; introduce an exact pre-dispatch termination reference
where required instead of forging a provider receipt. Recompute admission
counts consistently. Metadata remains immutable and has no provider result,
raw content, confidential digest, DEK or credential.

There is currently no embedding-specific work item or output-reservation
producer. Do not invent a Run/Step owner or claim a nonexistent output erased.
Migration 0192 must define a guarded, immutable, workspace/job/output-ordinal
material-intent membership relation with unique intent identity and exact
foreign keys. Its canonical owner kind is `embedding_job_output`; owner_id is
the job ID and output_ordinal must match the membership. An insertion authority
must reserve/register that tuple atomically under the same canonical job guard,
only while the job is nonterminal and has never dispatched. Deferred structural
validation must refuse an orphan canonical intent or mismatched membership at
commit; generic owner_id matching alone is not membership authority. No new
production output executor or vault protocol is implied by this boundary.

Termination locks the exact membership and intents in ordinal/identity order,
refuses prepared/bound/live or otherwise unreconciled material, and requires
the existing exact erasure/abandonment witness for any abandoned member. Future
embedding-output producers must use this guarded relation in their acceptance
transaction; inventing another owner kind is not a lawful embedding output.
Prove missing/mismatched membership refusal, admission/termination serialization
with enrollment, active-member refusal and witnessed-abandoned-member success.
Active output-vault reconciliation remains a later contract; this block must
not mark any key erased without its existing authority and durable witness.
Because the new enrollment boundary can create reserved members before an
output executor exists, dispatch must fail closed for enrolled-output jobs in
this block. Apply that refusal to shared dispatch and the direct lifecycle
writer. No-member jobs retain their existing behavior. A later output-execution
contract must replace this explicit refusal with the complete key-receipt and
result protocol; enrollment alone is not dispatch readiness.

Recovery must recognize Cancelled and FailedDefinite from their exact terminal
evidence and return explicit terminal outcomes, never ResumeReserved or
ResumeAdmitted. Reject inconsistent terminal evidence rather than guessing.
Keep the six domain states and existing unknown/success/result recovery intact.
Wire the configured cancellation policy in GovernedProviderRuntime; no new HTTP
surface or worker executor is required in this block.

**Exact implementation scope.** New migration 0192 only. Existing paths:
application `src/embedding/job.rs`, `src/embedding/mod.rs`, `src/lib.rs`,
`src/provider_dispatch.rs`; infrastructure `src/postgres/embedding_job_repository.rs`,
`src/postgres/provider_dispatch_repository.rs`, `src/postgres/mod.rs`;
infrastructure tests `common/mod.rs`, `embedding_dispatch_is_atomic.rs`,
`embedding_effect_recovery.rs`, `embedding_schema_contract.rs`,
`embedding_runtime_role_refusals.rs`, `p03_upgrade_provisioning.rs`,
`provider_schema_contract.rs`, `runtime_role_cannot_write_directly.rs`;
`docker/postgres/init-runtime-role.sh`; this evidence document. Root owns evidence,
scope and preflight; builder owns implementation/tests. Preserve Task 13/14A
dirty files, dependency manifests, PLAN.md and protected bytes. No commits,
network providers, deployment or unrelated cleanup.

**Acceptance and order.** Independent adversarial plan review precedes coding.
Builder adds behavioral PostgreSQL coverage and observes RED before claiming a
fix. Use real runtime login (assert current_user), real policy decisions,
persisted evidence/counters and bounded concurrency coordination; no sleep-only
race claim. Prove both job states, same-key replay, conflicting keys, stale
version, denied/mismatched authorization, cross-workspace/principal refusal,
denial and timeout evidence, no-reservation and guarded-reservation refusal,
queued and admitted lease cleanup, both dispatch/cancel orders, and terminal
recovery with no dispatch. Force rollback after tentative terminal writes and
verify every related table/counter from another connection. Independently remove
a decisive dispatch/termination fence or split the atomic write, observe the
forbidden persisted behavior make the proof RED, restore exact bytes and GREEN.
An injected error proves rollback only, not process-crash durability.

Update exact schema/function ACL inventories and both fresh/upgrade provisioning
paths; prove restricted migration ownership with the current bootstrap script.
Run focused domain/application and affected PostgreSQL suites, applicable CLI
runtime composition checks, fmt, changed-crate Clippy, scope/protocol/baseline
checks and final complete diff review. Record actual commands, exits/counts and
any environmental failures separately. Acceptance is limited to these commands
and terminal sources; full workspace/P04 completion must not be inferred.

**Pre-implementation review.** The independent cdx reviewer first returned
REVISE: the generic material owner fields did not establish an embedding-job
membership authority. The lead accepted the finding and added the exact guarded
membership/deferred-validation contract above. The reviewer then returned
APPROVE. The lead also traced the direct effect lifecycle writer and required
the actual Dispatching-persistence fence above. The baseline command
`cargo test -p vestrace-infrastructure --test embedding_effect_recovery --locked a_second_successor_identity_is_a_semantic_acceptance_refusal -- --exact`
passed against local PostgreSQL with the existing restricted runtime login
(1 passed, 10 filtered, exit 0). This baseline is not Task 14B acceptance.

**Implementation review checkpoints (work in progress, not acceptance).** The
first direct-lifecycle regression initially asserted the refusal before its
transaction committed. The lead rejected a claim of persisted RED evidence:
statement success followed by a test panic did not establish a surviving fact.
The corrected test commits an unexpectedly successful write and reads the
Dispatching count from another connection. The builder then observed the
forbidden committed count of one against the pre-fence implementation. Its
administratively seeded Cancelled job is an adversarial legacy fixture, not
proof of the new cancellation producer. Actual producer, concurrency, rollback,
restoration and GREEN acceptance remain separate requirements.

The lead's checkpoint review required corrections for overwritten PL/pgSQL
FOUND state, full material-reservation replay identity, exact output owner-kind
membership, NULL-safe authorization checks, principal binding before replay,
fresh cancellation PolicyDecision IDs on semantic replay, narrow lease-release
evidence, immutable receipt tables, shared installation/audit composition, and
exact terminal-recovery evidence. These are review requirements, not a claim
that their fixes or tests have already passed.

Implementation ownership is explicitly partitioned: the persistent builder owns
SQL, application/repository code and behavioral tests; the former investigation
thread owns only `docker/postgres/init-runtime-role.sh` and
`crates/vestrace-infrastructure/tests/p03_upgrade_provisioning.rs`. Cargo runs
remain serialized. The lead retains evidence/scope ownership and final review.

The first restricted fresh-provisioning run refused migration 0192 with SQLSTATE
42501 for the membership validator. The lead independently reproduced the
failure without any trigger: after owner transfer and removal of runtime
privileges, `SET LOCAL ROLE vestrace; REVOKE ALL ON FUNCTION ... FROM PUBLIC`
fails with the same permission error. This rejected a proposed permanent
runtime EXECUTE grant as an unproved fix. The diagnostic transaction was
connection-rolled-back, and a separate connection observed zero probe schemas.
The correction is to let the owner hand-back helper perform ACL changes and
keep superuser-only fallback ACL statements in that fallback. This observation
does not replace the required fresh/upgrade test rerun.

Further lead review traced the legacy lifecycle table's real runtime grants:
`vestrace` retains INSERT, UPDATE and DELETE there. An INSERT-only trigger cannot
establish immutable Dispatching history or reject an UPDATE into Dispatching.
The lead required embedding-specific UPDATE/DELETE refusal in migration 0192
and real runtime regressions that commit unexpected success and inspect the
surviving facts. Shared Run/qualification lifecycle behavior must remain
unchanged. This finding is pending behavioral RED and restored GREEN, not an
acceptance claim.

For the effect/lifecycle serialization step, direct repository tracing found
that `record_dispatch_started_on` writes the lifecycle row without an advisory
lock. The canonical connection/auth/space/job locks in the BEFORE trigger and
termination therefore provide the shared lifecycle fence. The lead accepts
that concrete row-lock authority instead of adding a separate advisory lock
that the direct writer does not share. Both actual lock orders still require
observed blocking and committed-state assertions.

The lead independently reran `node --test tests/p02_scope.test.mjs
tests/p03_scope.test.mjs tests/p04_scope.test.mjs` (16 passed, exit 0), the reopened
P04 dirty-baseline verifier, `node scripts/protocol-lock.mjs --check .`, and
`git -c safe.directory=E:/Soft/vestrace -c core.safecrlf=false diff --check`
(each exit 0). These static checks do not replace the pending behavioral suites.

### Task 14B independent verification checkpoints (2026-09-06)

The lead's first seven-target PostgreSQL command stopped at
`embedding_dispatch_is_atomic`: 9 passed, 13 failed, exit 1, 116.60 seconds.
Eleven failures used a migration body compiled before the builder corrected an
accidental TG_OP/OLD block in the non-trigger pre-dispatch gate. Two failures
were real credential-fixture prerequisites: Active lacked an exact Candidate
publication. The fixture was corrected to use guarded reservation, preparation,
Candidate publication and first activation. These results are recorded as
failures, not discarded or counted as acceptance.

After forcing affected test-source rebuilds, `embedding_effect_recovery` passed
all 19 tests (78.54 seconds), including persisted UPDATE/DELETE history refusal,
actual configured cancellation/replay, both terminal sources, Running cleanup
and exact terminal recovery. `embedding_runtime_role_refusals` passed all 3
(10.05 seconds). The same command then failed `embedding_schema_contract`
(10 passed, 5 failed, exit 1, 71.38 seconds): four tests exposed the new validator's
CASE expression resolving an absent record field across different trigger row
shapes; the fifth exposed a preexisting ambiguous CHECK selection. The validator
now uses explicit PL/pgSQL branches. The test chooses exact state-constraint
names and retains every closed-set assertion. A current rerun is still required.

`cargo test -p vestrace-domain -p vestrace-application --locked --quiet` passed
with exit 0: 481 tests and 20 doctests, including all 123 application unit tests
and 277 domain tests. The lead's initial changed-crate Clippy command failed on
a large PolicyDecision enum variant and a test function-pointer tuple type.
The variant was boxed and the test type named; the final Clippy rerun remains
separate. Final full acceptance and independent mutation are not yet claimed.

### Task 14B lead behavioral acceptance and independent mutation

The corrected final `embedding_effect_recovery`, `embedding_schema_contract`
and `p03_upgrade_provisioning` command returned exit 0: respectively 19 tests
(70.42 seconds), 15 tests (47.50 seconds), and 7 tests (19.62 seconds). Earlier
final focused targets also passed: `embedding_dispatch_is_atomic` 22 (78.01s),
`embedding_runtime_role_refusals` 3 (10.05s), `provider_dispatch_is_atomic` 44
(199.02s), `provider_schema_contract` 38 (128.96s), and
`runtime_role_cannot_write_directly` 45 (129.55s). These are 193 passing tests
across eight affected PostgreSQL targets, obtained in the explicitly recorded
staged runs, not a claim that a full workspace command passed. The broad
`--no-fail-fast` run retained exit 1 solely for the two schema failures later
corrected and rerun; its passing targets are not called a passing aggregate.

The lead independently faulted the actual 0192 lifecycle trigger in the test
`a_cancelled_embedding_job_refuses_lifecycle_update_to_dispatching`. Only two
fixture statements were temporarily inserted: disable the trigger after real
configured-policy cancellation committed, and re-enable it after the unexpected
runtime UPDATE committed but before the independent owner-pool observation.
No assertion or production SQL changed. Thus the failed test leaves the trigger
enabled even in its disposable database. The existing assertion observed one
persisted Dispatching row where zero was required. The exact command was:

`cargo test -p vestrace-infrastructure --test embedding_effect_recovery --locked a_cancelled_embedding_job_refuses_lifecycle_update_to_dispatching -- --exact`

- Faulted test: exit 1; 0 passed, 1 failed, 18 filtered, 3.43 seconds. The observed
  assertion was `left: 1`, `right: 0`, after the forbidden UPDATE committed.
- Exact test-byte restoration: SHA256
  `FC3CA5E3AA31B4F426FA50484419A58E88B2420C9A4FD605D1C44773E63FF79E`.
- Same command after restoration/rebuild: exit 0; 1 passed, 18 filtered,
  3.78 seconds.
- Migration 0192 was unchanged throughout: SHA256
  `6DB03A1A5CF851C2980E2D21B5772B3F00947965E72E328F63D1C1861A7E8138`.

This is independent persisted RED -> exact restore -> GREEN evidence for the
actual direct-writer fence after the real cancellation producer. The injected
audit failure separately proves transaction rollback, including an actual
unconsumed credential lease, concurrency lease, job/version, terminal receipt,
audit marks and installation watermark. Neither test is described as a process
crash/restart proof.

### Task 14B final lead verdict — APPROVED (2026-09-06)

The lead reviewed the complete Task 14B implementation and tests against the
contract above. No remaining BLOCKER or MAJOR finding was found. The final
corrections preserve the fail-closed dispatcher, exact material membership,
immutable lifecycle evidence, canonical lock chain, typed replay/conflicts,
configured cancellation policy, and atomic audit/lease/job transaction.

Final verification commands and observed outcomes:

| Command | Result |
| --- | --- |
| `cargo test -p vestrace-domain -p vestrace-application --locked --quiet` | exit 0; 481 tests and 20 doctests |
| `cargo test -p vestrace-infrastructure --test embedding_effect_recovery --test embedding_schema_contract --test p03_upgrade_provisioning --locked --no-fail-fast -- --test-threads=1` | exit 0; 19 + 15 + 7 tests |
| `cargo test -p vestrace-infrastructure --test embedding_dispatch_is_atomic --test embedding_schema_contract --test p03_upgrade_provisioning --test provider_schema_contract --test runtime_role_cannot_write_directly --test provider_dispatch_is_atomic --locked --no-fail-fast -- --test-threads=1` | staged run exit 1 solely for the two subsequently corrected schema cases; dispatch 22, provision 7, provider schema 38, direct-write 45 and shared dispatch 44 passed; schema's final 15/15 rerun is above |
| `cargo test -p vestrace-cli --test worker_once_cli --test cli --test command_contract --test provider_runtime_wiring --test runtime_schema_gate --locked -- --test-threads=1` | exit 0; 33 tests with local PostgreSQL configured |
| `cargo clippy -p vestrace-application -p vestrace-infrastructure --all-targets --all-features --locked -- -D warnings` | final exit 0, 1m12s |
| `cargo fmt --all -- --check` | final exit 0 |
| `node --test tests/p04_scope.test.mjs` | exit 0; 6 tests; earlier combined P02/P03/P04 suite passed all 16 |
| `node scripts/protocol-lock.mjs --check .` | exit 0 |
| `node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04r-preflight.json --scope p04-scope.mjs` | exit 0 |
| `git -c safe.directory=E:/Soft/vestrace -c core.safecrlf=false diff --check` | exit 0 |
| `Get-Content -Raw docker/postgres/init-runtime-role.sh \| docker exec -i vestrace-test-postgres bash -n` | exit 0; parse only |

The first CLI command omitted DATABASE_URL and returned exit 1: three SQLx
schema-gate cases stopped before behavior ran. Its unchanged rerun with the
local disposable PostgreSQL environment passed all 33. The last Clippy-only
corrections name two existing tuple types in tests; they change no query,
assertion or behavior and the final all-targets Clippy type-check passed. No
warning suppression was added. The unavailable native Bash parser was replaced
by parse-only Bash in the existing local PostgreSQL container, not by executing
the provisioner implicitly.

Scope remains exactly 106 change paths and 23 protected paths. Task 13's worker,
main and worker-once test and Task 14A's two domain files retain the previously
recorded hashes. PLAN.md retains SHA256
`621D7D6D5FCFE098D7197C4E89ED2E275007D4FD235782D4BE36C7AE8AFE5A00`.
Migrations through 0191 remain untouched; 0192 is the sole new migration. No
commit, push, deployment, production secret change or external provider call
was performed. The temporary mutation is restored byte-for-byte.

This approval is for the reviewed Task 14B block only: cancellation and the two
specified durable definite-failure sources before dispatch, including structural
output ownership and terminal recovery. Running is still an explicit legacy
fixture state where no production Running producer is composed. Active output
vault reconciliation, delivery/rebuild result completion, the embedding worker
executor and full P04 acceptance remain future work. The live gate program has
P05–P12 after P04; neither this block nor the passing focused suites complete
v1.0 or establish a release date.

### Task 14C proposal — delivery output reservation and provisional-key recovery (2026-09-06)

Status: PLAN AND EXACT SIX-PATH SCOPE AMENDMENT OPERATOR-AUTHORIZED;
IMPLEMENTATION IN PROGRESS. Task 14B remains approved. This is the next
bounded implementation contract; PLAN.md belongs to unrelated Slice 18 and
remains byte-for-byte preserved. The frozen P04 plan and spec remain unchanged.

#### Goal and investigated dependency

Create a real delivery acceptance and pre-dispatch output-key preparation path:
the accepted job durably owns the exact ordered source/output identities,
recovery creates each reserved provisional key at most once, and the ordinary
vault unwrap path cannot use any such key. Persist every creation receipt and
expose the exact waiting phase without allowing a provider dispatch yet.

Spec section 11.6 (current lines 217–225) requires output reservation at
acceptance and durable provisional receipts before leaving
`waiting_for_result_keys`. Current `AcceptEmbeddingJob` has no ordered source
or output enrollment. Migration 0192 supplies only the guarded individual
reservation function. `MaterialKeyVault` currently exposes immediate
`create_if_absent`/`unwrap`; `HostMaterialKeyVault` stores no provisional owner
or binding marker, and unwrap checks only erasure. Consequently the old vault
interface alone cannot implement this requirement. `MaterialIntentResumption`
abandons unprepared intents and is not an active output-key reconciler.

The Run provider-result repository is also not a template for transaction
boundaries: it calls the vault inside its transaction. Task 14C must keep vault
operations outside PostgreSQL transactions. Run result behavior is not changed
or accepted anew by this block.

#### Scope amendment requested after review

Add exactly these six paths to the current 106-path P04 change allowlist:

1. `migrations/0193_embedding_output_key_preparation.sql`
2. `crates/vestrace-application/src/embedding/keys.rs`
3. `crates/vestrace-application/src/material/vault.rs`
4. `crates/vestrace-infrastructure/src/postgres/embedding_key_repository.rs`
5. `crates/vestrace-infrastructure/src/crypto/material_vault.rs`
6. `crates/vestrace-infrastructure/tests/embedding_output_keys.rs`

After explicit approval, the count becomes 112; protected paths remain 23.
The scope module, P04 scope tests and P04r preflight must record precisely this
amendment without rebasing unrelated dirty files or weakening protected hashes.
No new dependency, manifest, historical migration or protected file is proposed.
Migrations through 0192, including its recorded SHA256, remain immutable.

Existing allowed implementation paths for this block are application
`embedding/job.rs`, `embedding/mod.rs`, `lib.rs`; infrastructure
`postgres/embedding_job_repository.rs`, `postgres/mod.rs`, and application
`provider_dispatch.rs` for mandatory `Arc<T>` forwarding of the new
default-unavailable vault operations. Infrastructure
`postgres/provider_dispatch_repository.rs` is allowed only if the shared
recovery result needs the new visible phase.
Allowed verification/support paths are infrastructure tests `common/mod.rs`,
`embedding_schema_contract.rs`, `embedding_effect_recovery.rs`,
`embedding_dispatch_is_atomic.rs`, `embedding_runtime_role_refusals.rs`,
`p03_upgrade_provisioning.rs`, `provider_schema_contract.rs`,
`runtime_role_cannot_write_directly.rs`; `docker/postgres/init-runtime-role.sh`;
the existing embedding fault scenario source/settings/child/report/main and
`tests/embedding_fault_scenario_e2e.rs`; the P04 evidence, scope and preflight
files. Every other path remains outside this block, even if generally allowed
by P04. No CLI worker scheduling or HTTP endpoint is added.

#### Requirements and boundaries

1. A typed delivery acceptance command fixes its exact delivery cause, job,
   snapshot, effect, MRE, ordered live source references, output ordinals,
   intent/material/key IDs and nonces in one governed acceptance transaction.
   Reuse the existing job/effect acceptance authority and generic intent
   allocator; do not introduce another external-effect lifecycle. Validate
   the delivery cause and source order against the actual MRE, not against two
   caller-supplied copies. Reject empty, duplicate, missing, cross-workspace,
   mismatched cause/source and non-delivery enrollment. An exact replay returns
   the original identities; a changed tuple conflicts and rolls back all rows,
   audit, outbox and reservations. Existing accepted jobs are not backfilled
   by guessing source references or future output IDs.
   Forward 0193 owns an immutable delivery-acceptance receipt with workspace,
   principal, idempotency key and exact structural request tuple plus the
   original accepted identities. The repository locks/checks this authority
   before applying any acceptance/audit/outbox mutation. Same key and exact
   semantic tuple returns that receipt without rerunning the mutation; changed
   tuple conflicts. New receipt, acceptance, reservations, audit and outbox
   commit atomically. Do not rely on generic governed-mutation same-hash
   replay, which may reapply its mutation. Receipt recovery cannot depend on
   caller-provided replacements for the originally allocated IDs.
   Each ordered input membership also owns an exact FK-bound existing
   `material_erasure_blockers` content/intent blocker. Its workspace, source,
   output intent and job identities must agree. Guard blocker creation and
   terminalization with the membership/job lifecycle: ordinary source erasure
   refuses while the job needs reconstruction, and lawful pre-dispatch
   termination releases the exact blockers atomically. Reuse the existing
   generic erasure check; do not add an unowned blocker or a parallel erasure
   mechanism. Cover source-erasure versus acceptance/retirement in both orders.
2. Define `EmbeddingOutputKeyPlan`, `EmbeddingOutputKeyProgress` and an
   embedding-owned repository/service in the new modules. Read reservation
   authority from PostgreSQL using the immutable job tuple. Every PostgreSQL
   mutation uses the existing shared InstallationMutationPermit and canonical
   connection -> optional credential -> exact space -> job/effect -> ordered
   intent locks. No-auth has no fabricated slot, credential reference or
   credential material. The host-vault mutation occurs only after its durable
   intent transaction commits and before its witness transaction begins; the
   P05 drain contract, not an open P04 database permit, reconciles that bounded
   interval during installation freeze. Do not introduce a second job state enum: waiting is
   a derived phase. This block need not create a production Running scheduler.
3. Extend the existing host vault with a distinct intent-bound provisional
   creation operation. Its durable record binds workspace, job, intent,
   material, key, nonce and output ordinal; exact replay returns the same
   witness, and any different tuple refuses. Ordinary `unwrap` and ordinary
   `create_if_absent` must refuse a provisional record, including after host
   process restart. Existing ordinary records retain their established
   behavior. New provisional operations must default to unavailable on other
   vault implementations, never fall back to ordinary creation/unwrap.
   No binding, encryption, runtime promotion or caller-supplied 'bound' flag is
   exposed in 14C; those require the committed result marker in the next block.
4. Provisional create, its durable publication and erasure use an immutable
   per-key filesystem protocol; they do not hold a database lock or transaction
   across a vault call. Under a stable per-key directory, every new-format key
   has exactly one immutable `claim` stage. It says `ordinary` or
   `embedding_output`; the latter binds workspace, job, intent, material, key,
   nonce and output ordinal. The process whose atomic claim publication wins
   also generates and stores the stable vault receipt in that claim. A losing
   exact concurrent creator compares only the caller-known immutable binding
   tuple, then adopts and returns the winning claim's stored receipt; it neither
   compares against its discarded locally generated receipt nor accepts a
   binding mismatch. Every later stage validates and returns only witnesses
   bound to that installed claim receipt. Fence/erased destination-exists
   replays likewise compare caller-known authority and return the installed
   witness instead of requiring a new random witness to match. Existing legacy ordinary records remain
   readable, but a legacy record and a new-format key directory for the same
   key ID is an impossible combination that fails closed.

   Publish `claim`, `envelope`, `fence` and `erased` stages with only safe Rust:
   create a unique same-directory temporary file with `create_new`, write the
   complete versioned record, call `sync_all`, then atomically install the fixed
   stage name with `std::fs::hard_link`. A destination-exists result is replay:
   read the fixed stage, compare only that stage's caller-known binding or
   authority fields, and return its installed winning witness. Never compare a
   fresh losing process's random witness with the installed one.
   Remove only the unacknowledged temporary name; never truncate, overwrite or
   rename over a published stage. A crash before the hard link leaves no fixed
   authority; a crash after it leaves a complete fixed record. Stale temporary
   names are ignored and may be reported for later cleanup. Hard links and temp
   files must be on the same supported filesystem; unsupported filesystems fail
   closed during readiness rather than falling back to the old writer.

   Ordinary create first claims `ordinary`; embedding create first claims its
   exact `embedding_output` tuple. Both therefore contend on the same fixed
   claim name. Ordinary unwrap refuses every embedding-output claim and every
   malformed or impossible stage combination, including after restart.
   Retirement atomically publishes `fence`, removes the envelope stage, then
   atomically publishes the exact `erased` receipt. Creation checks fence/erased
   both before and after publishing an envelope; if retirement won, it refuses
   the receipt and removes any late envelope. Retirement repeats envelope
   removal before replaying/returning `erased`, so a crash at any create/retire
   interleaving converges to no envelope. The database state machine ensures
   only the exact job-owned create or retirement route is eligible, but safety
   does not depend on an in-memory mutex, PID, timeout or stale lock file.

   Tests require two independent processes, both create/retire orders, death at
   every fixed-stage boundary, acknowledged exact read-back, no late envelope,
   and legacy ordinary-key compatibility. They make no power-loss claim beyond
   the file `sync_all` and atomic hard-link behavior actually verified on every
   supported filesystem. No direct or transitive locking dependency, manifest
   change, unsafe code or InstallationMutationPermit change is required.
   The existing `MaterialKeyVault for Arc<T>` implementation in
   `provider_dispatch.rs` explicitly forwards every new provisional operation;
   it may not inherit the default-unavailable method. A composition test calls
   the real host vault through `Arc<dyn MaterialKeyVault>`, observes the winning
   provisional receipt, and proves an implementation that does not opt in still
   returns unavailable.
5. Reconciliation alternates short DB transactions and host-vault calls:
   read/claim exact reservation and commit; create-if-absent outside DB; reopen
   guards and record its exact durable witness. No in-memory mutex, caller
   boolean or elapsed timeout substitutes for durable authority. Crash after
   reservation or after vault creation resumes that same intent/key/nonce.
   Never abandon an active output merely because its worker restarted.
   Forward 0193 must also fence the generic pre-prepared abandonment helper
   for owner_kind `embedding_job_output` unless its exact durable job-owned
   retirement authority already exists. Omitting the old reconciler from the
   new service is insufficient: runtime invocation of its generic helper must
   refuse too. Test the real legacy resumption attempt and its persisted rows
   against an active output, and prove the owner-specific guard by mutation.
   `waiting_for_result_keys` remains until every enrolled output has its
   validated receipt. Receipt-complete means only keys prepared: 0192's output
   dispatch fence remains in force until the future complete output protocol.
6. Cancellation/failure and active key creation share a monotone retirement
   protocol: persist retirement intent first, fence/erase outside DB, then
   record the real witness and invoke the existing 14B termination authority.
   Create racing retirement cannot resurrect a key, replace its nonce, record
   false readiness, or leave a terminal job owning usable material. Prove both
   orderings and crash recovery. All outputs must be witnessed Abandoned before
   cancellation/definite failure commits. ResultPrepared, Bound and Live are
   refused by this pre-dispatch reconciler, never generically abandoned.
7. SQL authority enforces exact membership, immutability, source order and
   complete receipt sets against runtime-role direct writes, legacy helpers,
   and forged workspace/owner tuples. Callers cannot append outputs after
   acceptance has committed. Use forward 0193 wrappers/guards, not edits to
   0192. Fresh provision and upgrade from 0192 retain narrow ownership and
   grants; no broad runtime ownership or grants repair fixtures.

#### Acceptance and verification

- Actual PostgreSQL runtime-role tests exercise the new governed acceptance,
  repository and service with the real host vault in a temporary test root.
  Observe persisted identities/versions/receipts and host records through a
  fresh observer; do not equate mocked success or compilation with acceptance.
- Cover ordered multi-output enrollment, full rollback and exact replay;
  wrong workspace/job/source/order/cause/key/nonce; no-auth and credential
  branches; partial receipt sets; runtime direct writes and generic-helper
  bypasses; ordinary unwrap refusal before/after receipt and after restart;
  create-versus-retirement in both orders; duplicate reconcilers; acknowledged
  witness stability; ordinary-vault compatibility.
- Extend the existing process fault harness for real child termination after
  reservation, after host create before DB receipt, after receipt persistence,
  and during retirement. Restart from durable state without provider calls or
  child-owned memory. Failure injection inside one surviving process is not
  restart evidence. A malformed/incomplete host record must fail closed.
- Demonstrate RED -> exact restoration -> GREEN by removing the provisional
  unwrap refusal and observing the callback execute unexpectedly; separately
  mutate a receipt-completeness/retirement guard and observe persisted unsafe
  state. Restore exact hashes and independently rerun both unchanged probes.
- Run domain/application tests; the new `embedding_output_keys` target;
  existing material-vault/material-intent tests without modifying them;
  embedding dispatch/recovery/schema/runtime-role, provider schema/direct-write
  and upgrade provisioning tests; relevant process fault tests; scoped Clippy
  with `-D warnings`, workspace fmt check; P04 scope/protocol/baseline/diff gates.
  Serialize Cargo, report actual exits/counts and environmental blocks.

#### Explicit non-goals and next boundary

14C does not create `EmbeddingJobResultPrepared`, encrypt provider output,
bind keys, promote vector materials/projections, advance a corpus, make a
generation Ready, append Succeeded, dispatch an enrolled job, implement
retrieval-query/rebuild semantics, or wire the embedding worker. The next
delivery-result block must add the exact result marker, specialized non-live
attachments, completion-only source/credential blockers, atomic receipt and
lease release, and all-bound publication under corpus/generation guards.
P04 and v1.0 remain incomplete. No implementation may begin until the six-path
amendment is explicitly accepted and the independent review findings resolved.

Lead resolution of the prior filesystem caveat: requirement 4 now names one
immutable claim/stage protocol using safe-Rust `create_new`, `sync_all` and
same-directory `hard_link`. It adds no dependency or manifest/lockfile path,
preserves `#![forbid(unsafe_code)]`, uses neither a stale lockfile/PID nor a
database session lock, and leaves the InstallationMutationPermit unchanged.
Review and tests must still disprove partial-file acknowledgement, legacy/new
namespace collision, late-envelope resurrection and every create/retire race;
the design is not accepted merely because these standard-library APIs exist.

Independent review attempt: a new `output_keys_plan_review` thread was started
with read-only instructions. It had not returned a verdict or findings after
bounded-result requests, and the lead interrupted that pass and requested its
already-collected findings. This is incomplete review, not approval. The lead's
unresolved filesystem-authority finding above independently prevents accepting
the proposal. No 0193 migration, vault change, scope amendment, dependency
installation or implementation worker was started in this continuation.

The first reviewer subsequently returned an explicitly incomplete bounded review
with `VERDICT: REVISE`: (1) undefined safe interprocess vault primitive/scope
is a BLOCKER; (2) generic resumption needs an owner-specific abandonment fence;
(3) acceptance replay needs an exact immutable receipt authority before audit
and outbox application. The lead accepts the unresolved mechanism as a blocker
without claiming that a six-path solution has been proved impossible. Findings
2 and 3 are incorporated above as contract requirements, not claimed as coded
fixes or passed tests. Requirement 4 is the lead's concrete correction for
finding 1. A new complete independent review then returned `VERDICT: REVISE`:
concurrent fresh creators cannot compare independently generated receipt values,
and the existing `MaterialKeyVault for Arc<T>` forwarding implementation would
otherwise inherit the new default-unavailable method. Both corrections are now
incorporated above. They altered no new-path count and required one final focused
re-review; at that point the status remained DRAFT FOR RE-REVIEW.

Focused review closure: the reviewer identified one remaining blanket replay
sentence that still compared random witness values. After its removal and an
exact follow-up read, the independent reviewer returned `VERDICT: APPROVE`.
The lead accepts the corrected Task 14C contract. This approves the plan only;
the frozen P04 plan's stop-and-ask rule still requires operator authorization
before adding the six paths listed above to `changeScopePaths` or implementing
against them.

Operator authorization: the user replied `да` to the exact six-path request.
The P04 allowlist and P04r preflight now record the 106 -> 112 amendment with
protected paths unchanged at 23. This authorizes Task 14C implementation only;
all stated non-goals and the no-commit/no-push/no-deploy/no-external-call rules
remain binding.

### Task 14C implementation and acceptance evidence (2026-09-06)

Source HEAD for this implementation block was
`58e7dac3cef7cc106d59f7ba3560bdbd37de76c0`. Task 14C is implemented within
the authorized 112-path P04 change scope; the protected set remains 23 paths.
The six paths added by the Task 14C amendment are:

1. `migrations/0193_embedding_output_key_preparation.sql`
2. `crates/vestrace-application/src/embedding/keys.rs`
3. `crates/vestrace-application/src/material/vault.rs`
4. `crates/vestrace-infrastructure/src/postgres/embedding_key_repository.rs`
5. `crates/vestrace-infrastructure/src/crypto/material_vault.rs`
6. `crates/vestrace-infrastructure/tests/embedding_output_keys.rs`

The implementation adds a typed delivery-output command which carries the full
existing `AcceptEmbeddingJob` authority, ordered output identities and an
immutable delivery receipt. Fresh acceptance and exact semantic replay run
under one shared installation-mutation transaction; fresh work reuses the
existing effect/job, governed audit/idempotency/outbox and generic material-key
intent authorities. Immutable SQL memberships bind each output to the live MRE
source set and its source-erasure blocker. The embedding-owned repository and
service keep host-vault calls outside PostgreSQL transactions. The vault uses a
durable per-key claim/fence/erased protocol and moves the stable `active`
namespace to `retired-active` before erasure publication, preventing a late
creator or killed child from restoring an envelope. New vault operations are
default-unavailable and retain `Arc<dyn MaterialKeyVault>` forwarding and
ordinary-vault compatibility.

Review corrections included the late-envelope race and freely minted retirement
authority: retirement is now bound to the exact expected-version pre-dispatch
terminal command and durable cancellation/denial/timeout evidence. Every output
must have the exact specialized immutable retirement receipt, not merely a
generic material-intent erasure witness, before 14B termination can commit.
Source blockers remain nonterminal while the job is active and are released only
for exact membership-bound sources in the validated terminal transaction.
Receipt and retirement writers use the canonical parent-job then ordered
`(output_ordinal, intent_id)` lock order, so aggregate progress cannot invert the
retirement lock chain. Fresh and upgrade bootstrap provisioning for 0193 retains
narrow guarded ownership, grants and self-revocation. The independent
implementation reviewer's final read-only rerun completed with
`VERDICT: APPROVE`. This does not constitute final P04 acceptance.

Lead-observed verification was:

- domain: 277 tests and 11 doctests, exit 0;
- application: 204 tests and 9 doctests, exit 0;
- `embedding_output_keys`: 23 tests; material-intent: 9 tests; material-vault:
  8 tests, all exit 0;
- embedding dispatch: 22; recovery: 19; schema: 15; runtime refusals: 3, all
  exit 0;
- provider schema: 38; runtime direct-write: 45; upgrade provisioning: 7, all
  exit 0;
- embedding fault binary build: exit 0; the ignored process scenario was run
  explicitly and passed 1/1, exit 0. The earlier command without `--ignored`
  ran 0 tests and reported 1 ignored, so it is not counted as process evidence;
- workspace formatting: exit 0;
- scoped application and infrastructure all-target, all-feature Clippy with
  `-D warnings`: exit 0;
- combined P02/P03/P04 scope suites: 16/16, exit 0;
- dirty-baseline, protocol-lock and scoped diff-check gates: each exit 0.

Mutation evidence used unchanged probes and exact restoration:

1. `material_vault.rs` had SHA256
   `ED009B435FAEC727A5328804E2FD43035BE764717E62E61849B5D5E5B229E593`
   before and after the mutation. Removing the provisional unwrap refusal made
   its unchanged exact test fail, exit 1, 0/1, with
   `must not expose provisional key`. Exact restoration returned GREEN 1/1,
   exit 0.
2. Migration 0193 had SHA256
   `7876072B3A9184DF7CB1F6FE105A8269DDEA544D1920A022D968D86443C29C6B`
   before and after the mutation. Removing only the specialized retirement
   receipt join made the unchanged
   `generic_material_abandonment_cannot_replace_output_retirement_receipt`
   probe fail, exit 1, 0/1, because terminalization unexpectedly succeeded.
   Exact restoration returned GREEN 1/1, exit 0.

Protected migration 0192 remains SHA256
`6DB03A1A5CF851C2980E2D21B5772B3F00947965E72E328F63D1C1861A7E8138`,
and `PLAN.md` remains SHA256
`621D7D6D5FCFE098D7197C4E89ED2E275007D4FD235782D4BE36C7AE8AFE5A00`.
Task 14C remains bounded to delivery-output acceptance, provisional key
creation/retirement and pre-dispatch terminal evidence. It does not add result
encryption/binding, result-prepared or live publication, corpus advancement,
successful completion, enrolled-job dispatch, retrieval/rebuild semantics or
worker wiring. P04 and v1.0 remain incomplete.

### Task 14D proposal — delivery result preparation and durable recovery (2026-09-06)

This is a proposal only. No scope amendment or implementation is authorized by
this section. It follows the Task 14C boundary and implements the first half of
the section 11.6 completion chain for `delivery` jobs only: validate the
production embeddings response, seal every output under its already receipted
provisional key, and atomically persist the specialized non-live attachments,
the definite provider receipt, and `EmbeddingJobResultPrepared`. It deliberately
stops before key binding, Live projection publication, corpus advancement and
`Succeeded`; those form Task 14E.

#### Why delivery-only and why the split is safe

The accepted delivery authority from 0193 provides an immutable ordered MRE
source set, a separate contiguous output set, the full ordered source-dependency
set for every output, and one provisional-key receipt per output. It does not
claim a one-source/one-output bijection. Rebuild jobs do not yet have an
equivalent accepted-output authority or
a direct immutable job-to-transition-recipe ordinal mapping. Deriving that
mapping from a current plan during result handling would let routing state mint
completion ownership. Task 14D therefore refuses `rebuild` and
`retrieval_query`; a separate reviewed rebuild-enrolment block must establish
their exact output mapping first.

The split occurs at the durable marker explicitly named by the specification.
Before that transaction commits there is no recoverable provider body; loss
after dispatch remains deadline-gated and then becomes the existing
`InconclusiveUnknown` path without another provider call. After it commits, the
receipt, ciphertexts, attachments and marker are one fact, so recovery returns
`ResumeResultPrepared` and never invokes the adapter. Nothing is useful to
retrieval because no material, projection or corpus member is Live. Task 14E is
the sole future owner of bind and all-output publication.

#### Exact contract

1. The production OpenAI-compatible embeddings path must consume its response
   through the existing bounded body reader and a governed parser. It must
   compare the returned provider model with the exact requested model, require
   exactly one result per input in unchanged contiguous order
   (`data[n].index == n`), reject duplicate/missing/reordered indices, reject
   non-finite components, and retain vector storage in zeroizing containers.
   The generic `serde_json::Value -> Vec<f32>` path is not acceptable for this
   result. The repository later compares every vector width with the locked
   `EmbeddingSpaceKey.dimensions`; model plus dimensions and the already locked
   workspace/registration are the exact space match.
2. Add embedding-owned application values for a result-preparation id, each
   ordered attachment identity, the exact dispatch authority, and the validated
   response. A preparation service first loads a short-lived eligibility plan
   from PostgreSQL, then performs all host-vault and sealing work outside a
   database transaction, then submits the ciphertext set to one short commit
   transaction. Plain f32 encoding scratch is bounded, canonical and
   `Zeroizing`; ciphertext may outlive the callback. No plaintext vector,
   credential or DEK enters logs, errors, audit, outbox, SQL parameters other
   than ciphertext, or retry state.
3. Extend `MaterialKeyVault` with a default-unavailable, embedding-specific
   callback that accepts the complete `EmbeddingOutputKeyBinding`. The host
   implementation must re-read the immutable on-disk provisional claim and
   require its exact workspace/job/intent/material/key/nonce/output tuple before
   lending a `ZeroizingDek` to the sealing callback. This operation neither
   binds nor promotes the key. Ordinary `unwrap` continues to return
   `VaultError::Provisional`, and the `Arc<T>` forwarding implementation must
   explicitly forward the new method.
4. Migration 0194 introduces the missing guarded storage model, not only marker
   UUIDs: one `embedding_space_corpus_states` row and one
   `embedding_index_generation_guards` row per exact registered space,
   `embedding_projection_entries`, ordered
   `embedding_projection_source_dependencies`, immutable
   `embedding_job_result_preparations`, and
   `embedding_job_result_prepared_attachments`. It also binds the result to the
   exact append-only `embedding_data_policy_decisions` delivery row. The
   migration first refuses an upgrade containing more than one delivery
   decision for the same `(causal_reference_id, delivery_attempt)` and then adds
   the unique cause key; it never selects a latest row. Fresh and upgrade
   provisioning deterministically create the two one-per-space guard rows
   before result work is executable. This task may reserve a monotonic
   projection ordinal by advancing `next_projection_ordinal`, but it does not
   advance corpus revision, change a generation or expose the projection.
5. The preparation row binds one exact delivery job to workspace, space
   registration, snapshot, MRE, external effect, acknowledged receipt, adapter,
   expected job version, response model, ordered output count and the exact
   embedding data-policy decision. The guarded repository derives that decision
   from the locked delivery outbox cause and dispatch attempt; the caller cannot
   nominate it. Its purpose/cause/attempt must match, its input count must match
   the response output count, and only an `allowed` delivery verdict may prepare
   a durable vector. Each
   `embedding_projection_entries` row is the actual non-live
   `ResultFinalizing` projection for one exact job/input-output ordinal and
   reserved space projection ordinal; it names the derived vector material,
   intent/key, response index/model/dimension, snapshot and result marker.
   It also persists the decision's channel `Sensitivity`, the complete sorted
   distinct `classification_labels`, whether any input was unclassified, and
   `retention_eligibility_state='blocked_result_finalizing'`.
   Unique composite keys cover `(job_id,input_ordinal)` and
   `(space_registration_id,projection_ordinal)`. Its dependency rows reproduce
   the complete 0193 MRE source set for that output in exact source order. Source
   count need not equal output count and no `source N -> output N` mapping is
   invented. Every dependency must remain the exact Live material already
   recorded for that output and carries a composite FK to the exact 0193 source
   membership and its erasure blocker, while every output must have its exact
   0193 provisional receipt. Because 0164 classification labels deliberately
   have no severity order, 0194 cannot invent one. Instead every output receives
   the complete request-wide label set, unclassified fact and channel
   sensitivity from the exact pre-disclosure decision. That conservative,
   lossless propagation is at least as restrictive as every individual source;
   dropping any label, the unclassified fact or sensitivity is constraint
   refused. The complete dependency set on every output is also the persisted
   eligibility basis: result-finalizing vectors remain ineligible, and every
   later retention/source-preparation command must satisfy every linked source
   predicate plus the vector's own lifecycle predicate.
6. One guarded SQL command performs the commit. Canonical order is installation
   permit, permanent connection guard, pinned credential guard/intent only for
   the credential branch, exact space registration, space-corpus state,
   index-generation guard, the exact immutable data-policy decision, ordered
   Live source materials, embedding job/effect, then all output memberships and
   material intents ordered by
   `(output_ordinal, intent_id)`, followed by prepared attachments/projections.
   It rechecks delivery kind, Running/nonterminal job, expected version, dispatch
   transition, snapshot/MRE/effect/space identity, response model/dimensions and
   indices, every output's complete ordered source-dependency set, complete key
   receipts, exact policy purpose/cause/attempt/count/allowed verdict, lossless
   classification propagation, blocked result-finalizing eligibility, and
   absence of any retirement or prior conflicting marker.
7. In that same transaction the command invokes the generic P02 result-material
   primitive for every ciphertext, inserts every specialized
   `ResultFinalizing` attachment, inserts one immutable
   `EmbeddingJobResultPrepared`, inserts the acknowledged provider receipt with
   an embedding-specific marker reference, and releases the exact provider
   concurrency lease through the existing shared dispatch authority. For a
   credential snapshot the marker carries a direct composite FK to the exact
   `CredentialRevision`, its creation intent and its completion-only erasure
   blocker. For a no-auth snapshot all three columns/rows are forbidden by a
   branch-consistent CHECK and composite FK. Partial output sets, a receipt
   without all attachments, attachments without the receipt/marker, or either
   malformed auth branch must be impossible at commit.
8. The generic provider-result completion lock remains Run/Step-only. Add a
   separate embedding completion-lock method to the same
   `ProviderDispatchRepository`; its PostgreSQL implementation revalidates the
   original embedding cause rather than weakening the existing method. The
   existing embedding recovery SQL is replaced forward in 0194 so
   `has_result_preparation` derives only from the exact embedding marker joined
   to its receipt and complete attachment set, never from Run-owned
   `provider_result_preparations`.
9. Replay is marker-first and never re-encrypts. The database winner is unique
   on `(workspace_id,job_id)` and stores one immutable semantic authority tuple:
   job/effect, expected job version, adapter, exact space/snapshot/MRE, response
   model/index/dimension metadata, exact policy decision and the complete
   ordered dependency/output set. Before any vault callback, the repository
   loads a complete existing marker by the locked job/effect authority. If its
   semantic tuple matches, it returns that winner directly and ignores all
   newly proposed receipt/marker/attachment UUIDs; those random UUIDs and the
   randomized ciphertext are not semantic authority. If the semantic tuple
   differs, it conflicts. Two fresh callers may pass the pre-vault read, but the
   guarded insert has the same convergence rule: one transaction wins; the
   unique-conflict loser discards its ciphertext, re-locks and loads the winner,
   returns `ConvergedExisting` only when the full semantic tuple matches, and
   otherwise conflicts. No branch compares ciphertext, invents a stable vector
   digest, or calls the provider/vault after a committed marker. Once
   ResultPrepared exists, pre-dispatch cancellation,
   pre-prepared retirement, generic abandonment and a second provider-result
   preparation are refused. Source and credential blockers remain nonterminal
   for Task 14E; this task has no API that terminalizes them.

#### Verification and mutation evidence required

- Application tests prove response ordering/width/model validation, bounded
  zeroizing ownership, exact vault callback authority, ordinary unwrap refusal,
  replay and no-auth/credential branch shapes.
- PostgreSQL tests prove all-output atomicity, exact reproduction of every
  output's complete ordered source-dependency set,
  immutable marker/attachment/receipt joins, runtime-role direct-write refusal,
  provider lease release in the marker transaction, direct credential-revision
  retention versus structural no-auth absence, exact data-policy cause binding,
  conservative classification propagation, all-source eligibility inheritance,
  late cancellation and retirement refusal, and that no
  projection/material/generation becomes Live.
- A real child-process scenario aborts immediately after the preparation commit.
  The parent observes the child death, reads the persisted marker/receipt/all
  ciphertexts from PostgreSQL, invokes recovery, observes
  `ResumeResultPrepared`, and uses an external listener count to prove the
  provider was called exactly once. An injected error is not this proof.
- Mutation RED -> exact restore -> GREEN must at minimum remove: the complete
  output-set check; one corpus/generation guard lock or projection dependency;
  the receipt-to-marker join used by recovery; the host-vault binding-tuple
  comparison; the classification label/unclassified/sensitivity equality or one
  inherited source-eligibility dependency; and the ResultPrepared
  terminal/retirement fence.
  Unchanged probes must detect persisted unsafe state or an external second-call
  count, not merely an injected SQL error. Record before/after SHA256 values.
- Run the focused application, provider-adapter, material-vault, result
  preparation, embedding recovery/schema/runtime-role, upgrade provisioning and
  process-fault suites; then workspace fmt, scoped Clippy with `-D warnings`,
  P02/P03/P04 scope tests, baseline, protocol and diff checks. Cargo commands are
  serialized and actual exits/counts are recorded.

#### Proposed exact scope amendment

The current P04 change scope has 112 paths and 23 protected paths. Task 14D
proposes adding exactly these eight paths, producing 120 change paths while the
protected set remains 23:

1. `crates/vestrace-application/src/embedding/result.rs`
2. `crates/vestrace-application/src/providers/ports.rs`
3. `crates/vestrace-domain/src/external_effects.rs`
4. `crates/vestrace-fault-scenario/src/scenarios/embedding_result_preparation_crash.rs`
5. `crates/vestrace-infrastructure/src/postgres/embedding_result_repository.rs`
6. `crates/vestrace-infrastructure/src/providers/openai_compatible.rs`
7. `crates/vestrace-infrastructure/tests/embedding_result_preparation.rs`
8. `migrations/0194_embedding_job_result_preparation.sql`

Already scoped paths cover the embedding/application exports, material-vault
trait and host implementation, shared provider-dispatch trait/repository,
PostgreSQL exports, fault binary/settings/report, top-level fault E2E test,
runtime-role provisioning, P04 scope/preflight/evidence and all existing
cross-cutting verification. No dependency, manifest, lockfile, P02/P03
authority, frozen P04 plan or specification path is proposed. Applied
migrations 0192 and 0193 remain unchanged.

#### Explicit non-goals and next boundary

Task 14D does not bind a provisional key, create a Live vector or ordinary
material reference, publish a projection, write `memory_embeddings`, add a
corpus member, stale/publish a generation, emit the rebuild event, append
`Succeeded`, dispatch an enrolled job, implement retrieval-query/rebuild result
semantics, or compose the worker. Task 14E must add the host-vault bind witness
and one all-bound database finalizer; rebuild first needs a separately reviewed
accepted-output/recipe mapping authority. P04 and v1.0 remain incomplete.

Independent plan review returned `VERDICT: REVISE`. The lead accepted all four
findings. The first correction makes projection, corpus-state and generation
guards real 0194 tables locked by preparation rather than treating a UUID as a
projection. The second preserves 0193's full ordered source dependency set per
output and removes the invented one-source/one-output bijection. The third
replaces randomized-ciphertext comparison with marker-first recovery that never
reseals a committed result. The fourth adds the direct credential-revision,
credential-intent and destruction-blocker relationship with structural absence
on no-auth. A focused re-review then returned `VERDICT: REVISE` with two further
findings. The replay rule now uses one SQL convergence rule over the persisted
semantic authority tuple and explicitly treats caller-generated UUIDs and
randomized ciphertext as discardable race-loser data. Projection rows now bind
the exact unique delivery data-policy decision, conservatively copy its complete
request-wide labels/unclassified fact/channel sensitivity to every output, and
persist all 0193 source memberships and blockers as the eligibility basis. These
corrections do not change the proposed eight-path amendment. The contract is
PLAN-REVIEW APPROVED: the final focused independent review returned
`VERDICT: APPROVE`.

Operator authorization: the user replied `разрешаю` to the exact eight-path
request. The P04 allowlist and P04r preflight now record the 112 -> 120
amendment with protected paths unchanged at 23. This authorizes Task 14D
implementation only; all stated non-goals and the
no-commit/no-push/no-deploy/no-external-call rules remain binding.

#### Task 14D implementation verification record (2026-09-07)

This is a builder execution record, not an acceptance decision. The final
current migration SHA256 is
`54FBD28D39AAD74F4759FFEE2F943A0CC62404D00791BDF02A0378C1BCFFDBB4`.
The live `embedding_result_preparation` suite passed 14/14 in 77.51s after
the final ownership, public-execute and loader corrections. Its checks cover
the complete two-output marker/receipt/attachment/projection/dependency tuple,
semantic marker-first replay and concurrent convergence, exact output receipt,
policy, classification, source-chain, auth-branch, no-Live/corpus/generation,
terminal-fence and recovery refusals. The live recovery suite passed 19/19;
the output-key, runtime-role refusal, schema, P03-upgrade and direct-write
suites passed 24/24, 3/3, 15/15, 7/7 and 45/45 respectively.

The focused application result tests passed 4/4. The governed parser,
malformed-later zeroization, exact vault binding and ordinary provisional
unwrap focused tests each passed. The rebuilt ignored child-abort test passed
1/1: the parent observed one loopback request, marker and acknowledged receipt
counts of one, two ciphertexts/attachments/result-finalizing projections, no
Live material or ordinary reference, and recovery phase `result_prepared`.
Scoped Clippy with `-D warnings` passed for application, infrastructure and
fault-scenario. `cargo fmt --all -- --check`, P02/P03/P04 scope tests
(3/3, 7/7 and 6/6), P04 baseline verification, protocol-lock verification and
`git diff --check` passed; the latter only emitted pre-existing CRLF checkout
warnings.

Mutation probes were run against the indicated byte-exact pre-final sources,
then restored before later corrective edits. The host-vault binding check used
`B577C567D944CD71223A6D10F1DA6552687177F725AAB0845174FD6E489E3788` ->
`AF522112EA3E2012CAB99858750DD7B13A7FCF466600B672439372B2D369DDA6` ->
the original SHA and made the unchanged test report a mismatched claim leaking
a DEK. The terminal-fence probe used original
`844E2F04CEA29569603874CAB7E2DE73A32766EC7B0EDC7A4A307EDAF58316D2`,
mutant `E5FD…18A2`, and exact restoration; its owner-bound adversarial probe
persisted one forbidden terminal authority while mutated and was refused again
after restoration. The output-receipt probe used that same original source,
mutant `A860…CA6E`, and restoration; its unchanged test observed one remaining
0193 receipt while a bad mutant persisted marker=1, receipt=1, attachments=2,
ciphertexts=2, projections=2 and dependencies=6.

After the loader correction, the source-dependency, recovery receipt-join and
classification probes used original
`30CF784FDEB0DEDEC502353BCE8393DED6BC1DC172D28C03EC8F56F9704CB638` and
exact restoration. Their mutated SHA256 captures were `D5ED…5D98`,
`E604…A3B9` and `3C68…A108`. The unchanged dependency probe converged on a
marker with five dependencies only when its predicate was mutated. The receipt
probe observed marker=1, receipt=1, attachments=2, projections=2 and
dependencies=6 with mismatched receipt evidence and unsafe recovery phase
`dispatching` only while its join was weakened. The classification probe
observed two loader rows with persisted `corrupted-label` versus policy labels
`["alpha","beta"]` only while equality was disabled. Every probe was then
restored to its recorded original SHA and rerun green. The initial fixture-only
SQLSTATE 55006 and the initial dependency-mutation alias (`42702`) were
diagnostic failures, were fixed before a valid probe, and are not counted as
mutation evidence.

#### Task 14D final lead verdict — APPROVED (2026-09-07)

The lead reviewed the final Task 14D implementation, live PostgreSQL and
child-process evidence, six restored mutation probes, provisioning/ACL
hand-back, and the final 120-change/23-protected scope boundary. An independent
read-only final review reported no `BLOCKER`, `MAJOR` or `MINOR` findings and
returned `VERDICT: APPROVE`.

The accepted block stops at the durable delivery-only `ResultPrepared`
boundary. It does not claim key binding, Live publication, corpus or generation
advancement, job success, rebuild/retrieval-query result semantics, or worker
composition. Those remain work for Task 14E and the later reviewed boundaries.

**Final status: APPROVED.**

### Task 14E proposal — bind every delivery output and publish atomically (2026-09-07)

This is a proposal only. It does not amend scope or authorize implementation.
Task 14D remains approved. Task 14E completes only the delivery half of the
section 11.6 result chain: durably bind every provisional output key to the
exact committed `EmbeddingJobResultPrepared`, then perform one all-output
database finalization that makes the encrypted vector materials and projections
Live, advances the exact space corpus, invalidates its current Ready generation,
emits the durable rebuild fact, and appends `EmbeddingJob Succeeded`.

#### Why this is a separate authority

The host vault and PostgreSQL cannot share a transaction. Binding therefore has
one recoverable external-witness boundary per output, while publication is one
database transaction after all witnesses exist. A crash after a host bind but
before its SQL receipt must adopt the same exact host witness. A crash after any
SQL receipt must bind only the remaining outputs. A crash after the publication
commit must return the committed publication before touching the vault. No
recovery branch may invoke the provider again.

Task 14E remains delivery-only. Rebuild does not yet have the accepted immutable
recipe-to-output mapping required by section 11.6, and retrieval-query uses a
different generation-fenced, transient-vector terminal transaction. Neither may
reuse this delivery completion authority.

#### Exact contract

1. Add embedding-owned application values and a service for the exact
   `(workspace, preparation, job, effect, output ordinal, intent, material,
   key, nonce)` binding tuple, its host `MaterialKeyBindingReceipt`, the
   all-bound finalization plan, and the committed publication identity. The
   service first asks PostgreSQL for the current phase. `Published` returns the
   persisted winner immediately. `NeedsBinding` performs one host operation and
   records its returned receipt before continuing. `ReadyToPublish` computes
   only key-bound commitments and submits them to one database command. The
   service never accepts a caller claim that an output is already bound.
2. Extend `MaterialKeyVault` with two default-unavailable embedding-specific
   operations. `bind_embedding_output` accepts the complete Task 14C
   `EmbeddingOutputKeyBinding` plus the exact Task 14D preparation id. The host
   implementation re-reads the immutable provisional claim, creates and fsyncs
   one fixed `bound` stage containing its claim receipt, preparation id and a
   generated `MaterialKeyBindingReceipt`, and adopts a competing stage only
   when every field matches. `with_bound_embedding_output_key` requires that
   same complete tuple and receipt before lending the DEK to a callback.
   `Arc<T>` forwards both methods explicitly.
3. Binding is one-way. Once the bound stage exists,
   `with_embedding_output_key` may no longer lend the key for resealing and
   `retire_embedding_output` must refuse it. Ordinary `unwrap` continues to
   return `VaultError::Provisional` for every embedding-output claim, including
   a bound one. Future index building must use the same exact bound-output
   authority rather than turn a specialized vector key into an ordinary key.
4. Migration 0195 adds immutable `embedding_result_key_binding_receipts`, one
   exact row per Task 14D prepared attachment. Composite keys bind preparation,
   job, output ordinal, projection, intent, material/key/nonce and the opaque
   host receipt to the existing marker and attachment. A guarded record command
   takes the completion authority and output rows in canonical order, inserts
   the exact receipt, and moves only that intent from `ResultPrepared` to
   `Bound` with the same receipt. Exact replay returns the stored row; a
   different tuple or receipt conflicts.
5. The generic P02 bind and Live finalizers must not become alternate embedding
   authorities. The forward definitions of
   `vestrace_bind_material_key_creation_intent` and
   `vestrace_finalize_bound_content_material` refuse an
   `owner_kind='embedding_job_output'` unless the former is executing against
   its exact 0195 binding row; the latter always refuses the specialized owner.
   The P02 deferred material invariant is replaced forward so a Live embedding
   output requires exactly one specialized Live projection/publication and zero
   ordinary material references. Ordinary P02 and P03 result behavior remains
   unchanged.
6. Before the final database command, the repository loads every ciphertext and
   exact binding row but no plaintext. Through
   `with_bound_embedding_output_key`, an embedding-specific commitment port
   computes a 32-byte HMAC over a fixed domain plus workspace, preparation,
   projection, output ordinal, material/key identities, ciphertext length and
   ciphertext. The commitment is verifiable only while the exact DEK exists;
   it is neither a plaintext-vector digest nor an equality oracle. No vector is
   decrypted and no vector or stable digest enters PostgreSQL, logs, Audit or
   event payloads.
7. Migration 0195 adds one immutable
   `embedding_job_result_publications` row per preparation/job and one immutable
   `embedding_index_rebuild_events` row per publication. The publication binds
   the exact output count, terminal job version, resulting corpus revision,
   live-member count, resulting generation epoch and rebuild-event identity.
   The event carries only the exact space, before/after corpus revision,
   before/after generation epoch, built-through projection ordinal and safe
   cause identities. Caller-allocated publication/event UUIDs are not replay
   authority: an already committed semantic publication wins and its stored ids
   are returned.
8. The all-output command uses the canonical order: shared installation permit,
   permanent connection guard, exact credential guard only for the credential
   branch, exact space registration, space-corpus state, index-generation guard,
   current Ready generation, Task 14D marker/policy, ordered Live source
   materials and source intents, job/effect, then binding receipt, output
   intent/material/attachment/projection rows in output order. It does not take
   or recreate a provider admission, concurrency or credential dispatch lease;
   Task 14D already released the exact provider lease with the definite receipt.
9. Under those locks the command revalidates delivery kind, Running and exact
   expected version, complete ResultPrepared marker/receipt/attachments,
   acknowledged effect, exact space/model/dimensions/policy/classification,
   every ordered source dependency still Live, no retirement/terminal fact,
   every output intent Bound to its exact 0195 host receipt, and exactly one
   supplied commitment per contiguous output. Any missing output or changed
   source rejects the whole transaction.
10. In that one transaction the command promotes every content material and
    intent to `Live`, removes every generic prepared attachment, transitions
    every specialized projection from `result_finalizing` to `live` with its
    exact erasure-bound commitment, and inserts the immutable publication. A
    forward projection guard permits only this one transition and a deferred
    all-output invariant rejects any partial promotion, ordinary reference,
    missing attachment removal, missing commitment or publication mismatch.
    The specialized Task 14D attachment mapping remains immutable history.
11. The same transaction advances `embedding_space_corpus_states.corpus_revision`
    once, adds the exact output count to a guarded monotonic
    `live_member_count`, increments the exact
    `embedding_index_generation_guards.generation_epoch`, changes every current
    Ready generation for that registration to `stale`, inserts the exact rebuild
    event, terminalizes only the source and credential completion blockers named
    by the marker, and changes the job to `succeeded` with version+1. No new
    Ready generation is invented. Retrieval therefore fails closed until the
    later index builder publishes a matching generation.
12. Publication is semantic and marker-first. Concurrent reconcilers may race
    host binding, but exact fixed stages and unique SQL receipt rows converge.
    Concurrent finalizers serialize on the connection/space guards and return
    one publication. After commit, recovery reports the stored succeeded
    publication without a vault or provider call. A changed marker, binding
    receipt, commitment set, space state or source predicate conflicts; random
    retry UUIDs are discarded.
13. All new tables are forced-RLS, PUBLIC has no privileges, runtime receives
    only the SELECT needed to resume and EXECUTE on the narrow binding/status/
    finalization functions, and no direct DML. The provisioner owns the exact
    0195 relations and SECURITY DEFINER functions through
    `vestrace_guarded_owner`; fresh, upgrade and SQLx-superuser fallback paths
    install the same final authority inventory. Trigger-only validators are not
    executable by PUBLIC or runtime.

#### Verification and mutation evidence required

- Application and host-vault tests prove exact marker-bound host receipt
  adoption, crash-safe fixed-stage replay, refusal of a changed binding or
  preparation, refusal of reseal/retire/ordinary unwrap after bind, marker-first
  published replay, and zero vault/provider calls on a committed publication.
- PostgreSQL tests prove partial binding is recoverable but never Live; only the
  complete receipt set may publish; every output becomes Live in one commit;
  no ordinary reference appears; commitments, projection/source/policy tuples,
  source and credential blocker release, job version/Succeeded, one corpus
  increment, member count, generation epoch, Ready-to-Stale change and rebuild
  event are exact. Independent-session binding/finalization and source-erasure
  races must prove the canonical order and reject deadlock/partial visibility.
- Recovery and worker-facing tests start from a production-shaped Task 14D
  marker. They cover crash after host bind before SQL receipt, after a strict
  subset of receipts, after all receipts before publication, inside every
  database publication leg, and immediately after commit. A real child process
  abort proves the parent completes the same publication and the external
  provider listener remains at exactly one request.
- Mutation RED -> exact restore -> GREEN must at minimum remove: the host
  preparation-id comparison; the complete all-bound count; the generic ordinary
  finalizer refusal; one Live source recheck; one all-output promotion leg; the
  corpus revision/generation epoch/Ready-stale coupling; the rebuild-event or
  Succeeded atomic leg; and the published-marker-first recovery branch.
  Unchanged probes must observe persisted unsafe state, a stale generation that
  remained query-eligible, a partial Live set, an ordinary reference, or a
  second provider/vault call. Record full original/restored SHA256 values and
  the mutant SHA capture available from each run.
- Run focused application/vault/finalization, Task 14D regression, embedding
  recovery/schema/runtime-role/upgrade/direct-write and real fault suites;
  then workspace fmt, scoped Clippy with `-D warnings`, P02/P03/P04 scope,
  dirty-baseline, protocol-lock and diff checks. Cargo remains serialized and
  actual exits/counts are recorded.

#### Proposed exact scope amendment

The accepted P04 scope has 120 change paths and 23 protected paths. Task 14E
proposes adding exactly these five paths, producing 125 change paths while the
protected set remains 23:

1. `crates/vestrace-application/src/embedding/finalization.rs`
2. `crates/vestrace-fault-scenario/src/scenarios/embedding_result_finalization_crash.rs`
3. `crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs`
4. `crates/vestrace-infrastructure/tests/embedding_result_finalization.rs`
5. `migrations/0195_embedding_job_result_finalization.sql`

Already scoped paths cover the application exports and Task 14D result types,
material-vault trait/host implementation, provider-dispatch recovery,
PostgreSQL exports, fault binary/settings/E2E, runtime provisioner, common
fixtures, recovery/schema/runtime-role/upgrade/direct-write tests, P04
scope/preflight/evidence and CLI worker integration tests. No dependency,
manifest, lockfile, P02/P03 protected authority, frozen P04 plan or full-product
specification path is proposed. Applied migrations through 0194 remain
unchanged.

#### Explicit non-goals and next boundary

Task 14E does not implement rebuild result preparation, transition recipe
satisfaction or activation, retrieval-query generation fences/results/retry,
the encrypted-vector index builder, Ready-generation publication, vector
queries, source/vector erasure propagation, or compose the production embedding
executor in the worker. It establishes a complete, recoverable delivery
publication and a durable rebuild fact for those later reviewed blocks. P04,
G0 and v1.0 remain incomplete after this task alone.

### Task 14E continuation review — REVISE (2026-09-08)

#### Goal and authority

The user requested continuation through `cdx` and explicitly assigned the
Claude lead role to the current coordinator. The coordinator owns planning,
technical decisions and acceptance; an independent Codex reviewer inspected
the existing 14E proposal read-only. No external Claude review completed.

Reviewed baseline: `0a0344f1bde61ee569bbb6188e3849701e77e105`, initially clean.
The root `PLAN.md` describes historical Slice 18; its classification writer
and retrieval-policy follow-up already exist in the code. The current
continuation is the 14E proposal above, after the recorded 14D acceptance.
The accepted scope remains 120 change paths and 23 protected paths. This
review does not activate the proposed five-path scope amendment, rewrite an
applied migration, or claim that 14E is implemented.

#### Independent findings, verified against the baseline

1. **BLOCKER — attachment history prevents publication.** The proposal removes
   generic prepared attachments while preserving specialized attachment
   history. `0194_embedding_job_result_preparation.sql:499` makes that history
   reference the generic attachment with `ON DELETE RESTRICT`. Merely deferring
   validation does not permit the proposed committed state.
2. **MAJOR — credential blocker ownership is not established.** The preparation
   command at `0194_embedding_job_result_preparation.sql:1170-1174` selects a
   nonterminal blocker by credential intent and UUID ordering, without a
   job/effect owner predicate. Its validator at lines 635-641 does not supply
   that missing ownership. A marker's blocker id alone cannot authorize
   terminalizing another operation's protection.
3. **MAJOR — bind and retirement need one cross-process decision.** The host
   vault publishes independent stages through hard links, whereas
   `material_vault.rs:471-488` can publish an erasure fence and remove the
   envelope. A read of "not bound" followed by a separate write is not an
   exclusive decision: retirement could pass the check before binding wins.

Independent verdict: **VERDICT: REVISE**. The lead verified all three findings.
The recorded 14D verdict remains limited to its original ResultPrepared
boundary; it is not evidence that the proposed publication path is safe.

#### Required revisions to the implementation contract

- **Historical attachment invariant:** forward migration 0195 must replace
  the incompatible FK, not delete or rewrite the specialized history. Before
  publication, an exact generic attachment must exist. After publication, the
  immutable specialized tuple must be justified by the exact publication,
  binding receipt and Live projection, with no generic attachment or ordinary
  reference. Enumerate and update the existing preparation/dependency
  validators and replay queries that assume `result_finalizing`, an extant
  attachment or a nonterminal completion blocker. Preserve prepared and
  published branches explicitly; dropping the FK alone is insufficient.
- **Owned credential completion protection:** define a durable, unique
  `(workspace, job, effect, credential intent)` completion-blocker association
  and the guarded command that creates it. Publication may terminalize only
  that owned blocker. Existing markers must not acquire ownership by adopting
  whichever credential blocker their old marker happens to name. The revised
  contract must define a fail-closed upgrade/recovery path for those markers,
  preserving unrelated blockers and immutable marker history. Forward-replace
  the preparation and validation functions where necessary; do not edit 0194.
- **Host bind/retire arbitration:** bind and provisional retirement must
  compete through the same atomic cross-process decision, keyed by the exact
  immutable claim. Specify the durable winner record, loser behavior,
  handling of pre-existing fence/erased stages, and restart recovery without
  a stale process lock. A separate `bound` check followed by publication of
  `fence` is forbidden. A binding winner keeps its usable envelope and exact
  receipt; a retirement winner cannot produce a successful binding receipt.
  Ordinary unwrap remains forbidden for specialized output claims.

#### Additional observable acceptance requirements

- Upgrade a real 0194 prepared fixture, publish all outputs, then query both
  the preserved specialized history and absence of generic attachments.
  Independently attempt dangling history and a partial Live set as the runtime
  role; both transactions must fail. Exercise prepared and published replay.
- Use two jobs sharing one credential and an unrelated nonterminal blocker.
  Publish one job and verify by persisted ids that only its owned completion
  blocker terminates. Repeat with an old marker whose selected blocker has no
  job-owned association; it must not release the unrelated blocker.
- Coordinate separate host-vault processes at bind/retire decision boundaries.
  Exercise both winners, kill a process after the durable decision, reopen the
  vault and prove the same winner. A bind winner must lend the exact key through
  the bound callback; a retire winner must refuse binding and key use. Check
  host files and callback counts, not only returned errors.
- Mutation proofs must independently remove the historical-state predicate,
  blocker-owner comparison and common arbitration. Each unchanged probe must
  observe the unsafe state or fail to reject it, then pass after exact restore.
  These augment, rather than replace, the proposal's existing mutation suite.

#### Verification status and next step

This continuation performed source/contract review only. No Rust, SQL, vault,
scope arrays or preflight captures changed; no test or runtime acceptance is
claimed. Complete the revised concrete interface/SQL/upgrade contract and its
exact file-to-proof mapping before a builder starts. Reconcile any additional
paths with the proposed five-path amendment; do not silently expand it.

### Task 14E revised implementation contract (2026-09-08)

#### Goal

Finish the delivery-only ResultPrepared chain with durable host key binding
and one atomic publication of all outputs. This section supersedes conflicting
mechanics in the 2026-09-07 proposal; the rest of that proposal's requirements
and mutation obligations remain in force. It is the implementation plan for
this continuation, kept in the already-scoped evidence path. Historical root
`PLAN.md` and the frozen P04 plan are not rewritten.

#### Requirements

- A committed preparation fixes workspace, job, effect, policy, response model,
  ordered outputs, source dependencies and identities. All new authority is
  embedding-owned. Existing Run/Step completion is not an embedding finalizer.
- Binding is durable outside SQL; publication is one SQL transaction after all
  exact host receipts exist. No recovery branch performs a provider dispatch.
- Published replay returns the persisted publication before vault access,
  current admission/credential checks or new UUID allocation affects its result.
  It still authenticates the workspace and checks the exact requested
  preparation/job/effect identity and complete immutable publication evidence.
- A partial set of bindings remains recoverable and non-Live. A partial Live
  set, dangling attachment history, an ordinary output reference or an
  unowned blocker release cannot commit.

#### Non-goals

No rebuild result preparation, retrieval-query completion, Ready-generation
builder, vector query, transition activation, source/vector erasure propagation
or production embedding executor composition. No profile, P04, G0 or release
qualification follows from this task. Delivery publication deliberately makes
current Ready generations stale until the later builder runs.

#### Constraints and exact scope

Baseline is `0a0344f1bde61ee569bbb6188e3849701e77e105`. The only dirty path
at this planning step is this evidence document. Preserve all prior changes.
The live 120-change/23-protected scope is not amended by writing this plan.
After explicit acceptance of the scope amendment, add exactly the five paths
listed under "Proposed exact scope amendment" above, yielding 125/23. All new
tables and forward function replacements below live in migration 0195; no
additional migration or Rust module is required. No manifest, dependency,
lockfile, toolchain, secret or protected-authority edit is authorized. Applied
migrations through 0194 remain byte-identical. No commit, push or deployment.

The existing reopened preflight records HEAD
`58e7dac3cef7cc106d59f7ba3560bdbd37de76c0`; its verification against this
baseline fails. Before implementation, record the approved amendment in this
document, update the module/test counts and both preflight scope arrays, then
capture a new reopened dirty snapshot at the actual HEAD with raw porcelain
bytes and SHA256 for every protected path. Preserve the original preflight's
historical snapshot fields. Do not hand-edit hashes to make an old capture pass.
The reopened verifier must pass before the builder changes production code.

#### Implementation plan

**1. Define the embedding-owned application interfaces.**

Create `crates/vestrace-application/src/embedding/finalization.rs`; export its
types from `embedding/mod.rs` and `lib.rs`. Add the following contract, using
existing domain ID types and `ApplicationError`:

```rust
struct EmbeddingResultFinalizationAuthority {
    preparation_id: EmbeddingResultPreparationId,
    job_id: EmbeddingJobId,
    effect_id: ExternalEffectId,
}
struct EmbeddingResultBoundOutput {
    binding: EmbeddingOutputKeyBinding,
    projection_id: uuid::Uuid,
    receipt: MaterialKeyBindingReceipt,
    ciphertext: Vec<u8>,
}
enum EmbeddingResultFinalizationProgress {
    Published(EmbeddingResultPublication),
    NeedsBinding(Vec<EmbeddingOutputKeyBinding>),
    ReadyToPublish(Vec<EmbeddingResultBoundOutput>),
}
```

`EmbeddingResultPublication` contains the stored publication, preparation,
job, effect, space and rebuild-event UUIDs plus output count, terminal job
version, resulting corpus revision, live-member count and generation epoch.
Counts use checked `u64`/SQL `BIGINT` conversion. Ciphertext/commitment-bearing
values must not derive payload-printing `Debug`.

`EmbeddingResultFinalizationRepository` exposes async `load_progress(context,
authority)`, `record_binding(context, authority, binding, receipt)` and
`publish(context, authority, publication_id, rebuild_event_id, commitments)`.
All arguments are borrowed except proposed UUIDs and the commitment vector.
The return values are respectively progress, the persisted exact receipt,
and publication. `EmbeddingOutputCommitment` names projection id and output
ordinal with `[u8; 32]` bytes; the repository derives every other identity from
persisted rows, never from a caller's "already bound" claim.

`EmbeddingResultFinalizationService::finalize(&RequestContext,
&EmbeddingResultFinalizationAuthority)` repeatedly loads progress, binds only
the missing outputs in ordinal order and records each receipt. It computes
commitments only for the all-bound phase, then publishes once. Any failure
returns immediately; a new invocation resumes stored progress. It has no
provider dependency and holds no SQL transaction over a vault operation.

Add default-unavailable vault methods to `material/vault.rs` and explicit
`Arc<T>` forwarding in `crates/vestrace-application/src/provider_dispatch.rs:416`:

```rust
fn bind_embedding_output(&self, binding: &EmbeddingOutputKeyBinding,
    preparation: EmbeddingResultPreparationId)
    -> Result<MaterialKeyBindingReceipt, VaultError>;
fn with_bound_embedding_output_key(&self, binding: &EmbeddingOutputKeyBinding,
    preparation: EmbeddingResultPreparationId, receipt: MaterialKeyBindingReceipt,
    use_dek: &mut dyn FnMut(&ZeroizingDek)) -> Result<(), VaultError>;
```

Before implementing the service, add unit probes named
`published_replay_never_touches_vault`, `partial_binding_resumes_missing_only`,
`changed_preparation_or_binding_is_refused`, and
`commitment_failure_never_calls_publish` in the new module. Use independent
repository state and vault call counters, including a vault that fails on
every call during published replay.

**2. Linearize host binding and provisional retirement.**

Modify `crates/vestrace-infrastructure/src/crypto/material_vault.rs` only for
specialized output claims. Publish one immutable per-key `output-disposition`
stage using the existing same-directory synced-temp/hard-link primitive.
Its tagged value is either `Bound { claim_receipt, preparation_id,
binding_receipt }` or `Retire { claim_receipt, fence_receipt }`, with version
and complete claim identity validation. The hard link to this single pathname
is the competing processes' linearization point; there is no separate
check-then-write choice between `bound` and `fence` pathnames.

Binder requires the exact claim, an installed matching envelope, and no
historical fence/erased stage. On a Bound winner, same preparation adopts the
stored receipt; another preparation conflicts. On a Retire winner it refuses
without a receipt. Retirement must win/adopt Retire before publishing fence,
moving active storage or deleting the envelope; Bound refuses all those
effects. Restart after either decision reuses its stored witness. Corrupt,
contradictory or incomplete records fail closed without deleting evidence.
Existing completed retirements remain replayable and never become Bound.

The ordinary unwrap/prepare-erasure/erase methods keep refusing specialized
claims. Provisional sealing checks disposition before key use; a call admitted
before the Bound decision may finish, but every later call refuses. Such an
in-flight seal has no authority to replace the already immutable SQL
ResultPrepared ciphertext. Bound-key use checks the exact Bound decision and
receipt. The callback borrow and zeroization rules remain unchanged.

The upgrade requires stopping every old process that can access this host
vault before enabling the new binary. Mixed old/new vault writers are
unsupported: an old retire implementation does not consult disposition.
Record process-abort/restart evidence only; synced file contents plus the
existing hard-link protocol are not new power-loss durability qualification.

Put independent-process winner/restart tests in the already-scoped
`crates/vestrace-infrastructure/tests/embedding_output_keys.rs`. Force both
orders at the common decision; inspect receipt files, envelope availability
and callback counts after reopening the vault. Kill a winner after decision
and before subsequent work. Also prove wrong preparation, reseal-after-bind,
generic unwrap/erasure refusal and old retired-record replay.

**3. Establish credential completion ownership in forward SQL.**

Migration 0195 creates immutable
`embedding_job_credential_completion_blockers(workspace_id, job_id,
external_effect_id, model_binding_snapshot_id, credential_revision_id,
credential_intent_id, blocker_id)` with one association per workspace/job,
one owner per blocker and exact workspace-scoped foreign-key tuples. The
associated generic blocker is `target_kind='credential', blocker_kind='effect',
state='nonterminal', usable_until=NULL` at creation. No-auth jobs have no row.

An internal guarded helper
`vestrace_ensure_embedding_credential_completion_blocker(UUID,UUID,UUID)`
accepts workspace/job/effect only and derives all credential identities.
It acquires permanent connection, pinned credential activation guard and
credential intent locks in that order, revalidates the immutable snapshot and
active, unerased credential, then inserts a fresh blocker and association in
one transaction or returns the existing exact owner. It is not executable by
runtime or PUBLIC directly. Do not widen the generic candidate-only
`vestrace_record_material_erasure_blocker` contract (0174:837-853).

Forward-replace the 0194 preparation command so new credential markers name
this owned blocker. Add immutable
`embedding_result_credential_blocker_adoptions(workspace_id, preparation_id,
historical_blocker_id, owned_blocker_id)` for old markers. A guarded
`vestrace_adopt_embedding_result_credential_blocker(UUID,UUID,UUID,UUID)`
takes workspace/preparation/job/effect, verifies the complete original marker
and still-valid historical protection under the same guards, and creates or
reuses a fresh exact owned association. It preserves the old marker and old
blocker's ownership/state. Only active, unerased credentials are recoverable
by this command; rotated, revoked, retired or erasure-prepared credentials
return a typed conflict with no partial adoption. No automatic ownership
backfill runs during migration. Exact adoption replay converges.

Prepared-state validation follows the owned association (or its exact legacy
adoption) and requires that owned blocker nonterminal. Until adoption, valid
legacy preparations remain identifiable as prepared but the finalization
service invokes adoption before any host bind. A missing/terminal historical
blocker cannot be used to reconstruct protection retrospectively. Published
validation requires terminal owned protection tied to the exact publication;
the historical blocker is never released by this command.

Guard changes to specialized owned blockers so no generic terminalization or
direct owner DML can release them without the exact publication in the same
transaction. This guard is implemented in 0195 on `material_erasure_blockers`;
ordinary blockers retain their existing behavior. Source completion blockers
remain the exact 0194 source-dependency-owned set, not all blockers on a source.

**4. Implement the SQL publication authority and phase invariants.**

Create the three originally proposed tables
`embedding_result_key_binding_receipts`, `embedding_job_result_publications`
and `embedding_index_rebuild_events`, in addition to the two credential tables.
Binding rows include exact preparation/job/effect/output/projection/intent/
material/key/nonce and host receipt identity. Publication and rebuild event
have mutual exact identity constraints; publication is unique by preparation
and job. Add monotonic `live_member_count BIGINT NOT NULL DEFAULT 0` to
`embedding_space_corpus_states` (0194 has no Live projections to backfill).

Public runtime commands are:

```sql
vestrace_load_embedding_result_finalization(UUID, UUID, UUID, UUID)
vestrace_record_embedding_result_key_binding(UUID, UUID, UUID, UUID, BIGINT, UUID)
vestrace_publish_embedding_job_result(UUID, UUID, UUID, UUID, UUID, UUID,
                                     UUID[], BIGINT[], BYTEA[])
```

The common first four arguments are workspace/preparation/job/effect. Record
adds ordinal/receipt, deriving the entire remaining tuple in SQL. Publish adds
proposed publication/event IDs and aligned projection/ordinal/commitment
arrays. Reject nulls, empty/unequal arrays, duplicate or noncontiguous ordinals,
wrong projection order and commitments not exactly 32 bytes. The loader
returns phase plus exact output rows or the complete stored publication;
adoption runs in the repository before returning NeedsBinding for a legacy
credential marker. Unknown phase or incomplete rows are errors, never idle.

All commands use a shared installation permit in the repository. A new SQL
completion-lock helper derives connection and credential from the marker; it
does not call the pre-dispatch readiness gate or reacquire a released dispatch
lease. Lock order is connection -> credential guard/intent/owned blocker ->
space registration -> corpus -> generation guard -> ordered Ready generations
-> marker/policy -> ordered source material/intent/blocker -> job/effect ->
ordered output binding/intent/material/attachment/projection. The publication
fast path precedes current mutable-state gating, after exact scoped identity
validation. Run same-space finalizers, 14D preparation and erasure races in
both orders to verify this order against existing authorities.

Before changing rows, revalidate every predicate in original contract item 9.
Use existing marker.expected_job_version as the exact Running version; promote
to Succeeded with one checked increment. Source dependencies must still be Live
and their exact blockers nonterminal at this point. Credential branch must
have its owned association and active pinned credential. No-auth must have
none. Record-binding changes ResultPrepared -> Bound only after its exact
specialized receipt row exists; generic bind cannot manufacture that row.

Remove only the incompatible FK from specialized attachment history to
`prepared_material_attachments`; keep its identity, intent and projection FKs.
Replace it with a deferred phase invariant, triggered by changes to generic
attachments, specialized history, projections, binding receipts, material
intents and publications. Prepared/Bound requires one exact generic attachment,
prepared bytes/material, immutable matching history and no Live projection.
Published requires no generic attachment or ordinary reference, exact preserved
history, all Live outputs, 32-byte commitments and the one complete publication.
Validate the affected preparation on attachment DELETE using OLD identity;
an inner join that hides missing rows is not a completeness check.

Forward-replace these existing functions inside 0195, preserving all unrelated
owner branches and their previous refusal behavior:

- `vestrace_bind_material_key_creation_intent` and
  `vestrace_finalize_bound_content_material` (0170:269,301): specialize binding
  through an exact receipt row and refuse generic Live finalization.
- `vestrace_validate_material_key_creation_intent` (0170:445): specialized
  Live requires zero ordinary references plus exact publication/projection.
- `vestrace_validate_embedding_result_preparation` (0194:567),
  `vestrace_validate_embedding_projection_dependency` (0194:654): explicit
  prepared versus published invariants; historical dependency identity remains
  immutable, current Live source state is checked when publishing.
- `vestrace_load_embedding_result_eligibility` (0194:746) and
  `vestrace_lock_embedding_job_recovery_authority` (0194:967): validate complete
  published evidence before their old pre-dispatch/prepared-only gates.
- `vestrace_lock_embedding_result_completion_authority` (0194:714): exact
  published preparation replay must not fail its current Running-only check
  before the preparation repository can reach the published fast path.
- `vestrace_commit_embedding_result_preparation` (0194:1125): owned credential
  protection and semantic replay for a complete already-published result.

Replace the projection's unconditional update-rejection trigger and fixed-state
CHECKs with exactly one guarded result_finalizing -> live transition; keep
immutable identity/policy/source fields unchanged. For this slice retention
remains blocked pending later erasure propagation, represented as
`blocked_pending_erasure_propagation` for Live. No automatic retention release
is implied by publication. Add a deferred all-output publication invariant
covering generic attachment deletions and job, blocker, corpus, generation and
event changes, not just the immutable preparation INSERT.

One publication transaction updates all material/intent/projection states,
removes generic attachments, stores commitments/publication/event, increments
corpus revision once and live-member count by the exact output count, increments
generation epoch once, marks every current Ready generation stale, terminates
only exact owned completion blockers and appends job Succeeded. New UUIDs lose
to a committed semantic publication. Every leg rolls back together. Later
publications in the same space may advance counters; historical publications
validate their immutable before/after event chain, not equality with today's
global counters.

**5. Implement the repository, commitments and recovery integration.**

Create `crates/vestrace-infrastructure/src/postgres/embedding_result_finalization_repository.rs`
and export it in `postgres/mod.rs`. Implement the application port using
`DrainMutationPermit`, typed SQL decoding and narrow SQLSTATE mapping: 42501
is authority refusal, 22023 malformed input, 23514/55000 lifecycle conflict;
serialization/deadlock and connection failures are not successful replay.
No plaintext vector is read by this repository.

Put the concrete commitment implementation in this same new module using the
existing `ring` dependency and the bound vault callback. HMAC-SHA256 input is
ASCII `vestrace.embedding-output.commitment.v1` followed by a NUL byte, then
fixed 16-byte workspace/preparation/job/effect/projection/intent/material/key/
nonce UUIDs, unsigned 64-bit big-endian output ordinal and ciphertext length,
then ciphertext bytes. The DEK is the HMAC key; never persist it, an unkeyed
digest or plaintext. Test every tuple field and ciphertext changes the input,
and zero callbacks occur on mismatched preparation or binding receipt.

The application commitment port is `EmbeddingResultCommitter::commitment(
&self, context: &RequestContext, authority: &EmbeddingResultFinalizationAuthority,
output: &EmbeddingResultBoundOutput, dek: &ZeroizingDek)
-> Result<[u8; 32], ApplicationError>`. The service calls it only inside the
exact bound-vault callback; an absent callback result or port error refuses
publication. The infrastructure implementation has no database or provider
dependency. Add it to the new module's exports.

Update `provider_dispatch_repository.rs` and, only where needed, its application
recovery type documentation. A succeeded delivery is returned only with a
complete exact 0195 publication, not simply state='succeeded' plus any marker.
ResultPrepared continues to return ResumeResultPrepared while partially bound.
`embedding_result_repository.rs` must accept exact published preparation replay
without resealing; validate the immutable semantic response tuple as before.
Keep production worker embedding execution outside this task.

Update `docker/postgres/init-runtime-role.sh` with distinct one-shot
`vestrace_prepare_p04_result_finalization_upgrade()` /
`vestrace_finish_p04_result_finalization_upgrade()` helpers. They cover every
new table, replaced function and new trigger relation, use
`vestrace_guarded_owner`, revoke temporary privileges on hand-back and revoke
PUBLIC/trigger-validator execution. Fresh provisioning, runtime upgrade and
SQLx-superuser fallback must install equivalent forced-RLS/owner/ACL inventories.
No runtime direct DML grant is introduced, including on owned blocker tables.

#### Acceptance criteria and verification mapping

Every row below names planned probes, not tests already run. Add them to the
named files; retain existing assertions and production-shaped role fixtures.

| Requirement | Probe and persisted observation | File |
| --- | --- | --- |
| Service replay/progress | Four step-1 probes; exact per-output calls and zero calls after Published | new application `embedding/finalization.rs` |
| Host decision | `binding_and_retirement_have_one_process_winner`; both winners, process death after decision, exact receipt/envelope read-back | infrastructure `tests/embedding_output_keys.rs` |
| History | `publication_preserves_exact_attachment_history`; old 0194 fixture upgrades, all generic rows disappear, specialized rows remain; dangling history/partial Live rejected | new infrastructure `tests/embedding_result_finalization.rs` |
| Credential ownership | `publication_releases_only_job_owned_credential_blocker`; two jobs/one credential/unrelated blocker; only winner's owned id terminates | new finalization test file |
| Legacy credential | `legacy_marker_adoption_never_transfers_old_blocker`; immutable marker unchanged, distinct owned blocker, exact replay; erasure/rotation-first refuses atomically | new finalization test file and `tests/embedding_result_preparation.rs` |
| Atomic publication | `all_outputs_publish_with_one_corpus_event`; two outputs, one publication/event/version increment, exact counts/commitments, Ready->stale and no ordinary refs | new finalization test file |
| Concurrency | `finalizers_converge_and_serialize_with_source_erasure`; independent sessions, both lock orders, one publication, no partial visibility; later same-space publication increments again | new finalization test file |
| Published replay | `published_recovery_needs_no_vault_or_provider`; unavailable vault and provider counter unchanged after commit; changed exact tuple refused | application unit tests and `tests/embedding_effect_recovery.rs` |
| Upgrade/authority | fresh/runtime-upgrade/SQLx fallback inventories, generic finalizer/terminalizer refusals, prepared/published corruption matrix | `tests/embedding_schema_contract.rs`, `tests/embedding_runtime_role_refusals.rs`, `tests/p03_upgrade_provisioning.rs`, `tests/runtime_role_cannot_write_directly.rs` |
| Real crash | `result_finalization_survives_a_real_child_abort`; host-bind-before-SQL, strict receipt subset, all-bound-before-publish, aborted SQL legs and after-commit; provider listener exactly one request | new fault `scenarios/embedding_result_finalization_crash.rs`, fault `main.rs`/`settings.rs`, root `tests/embedding_fault_scenario_e2e.rs` |

Update common fixtures only in `crates/vestrace-infrastructure/tests/common/mod.rs`
when reuse is required. The credential preparation test's old lowest-UUID
expectation is replaced with persisted job/effect ownership while retaining its
snapshot checks. The fault scenario's report must fit existing report/settings
types or use already-scoped `report.rs`; it must invoke the real finalization
service and reopen host vault/database state after a real child PID exits.

RED -> restore -> GREEN probes independently remove: exact preparation match
in Bound adoption; common bind/retire arbitration; historical attachment phase
predicate; credential blocker-owner comparison; complete all-bound count;
generic Live finalizer refusal; one Live source recheck; one output promotion;
corpus/generation/Ready coupling; rebuild-event/Succeeded atomic leg; and
published-first recovery. For each record full original/mutant/restored SHA256,
unchanged probe command and exits, and its persisted unsafe-state or external
counter observation. A constraint that still blocks a mutant means that probe
has not established mutation sensitivity; do not label compilation failure RED.

Run Cargo commands serially with the prepared PostgreSQL/runtime-role fixture
environment; never print connection secrets. Run focused probes first, then:

```text
cargo test -p vestrace-application embedding::finalization -- --nocapture
cargo test -p vestrace-application embedding::result -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_result_finalization -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_result_preparation -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_output_keys -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_effect_recovery -- --nocapture
cargo test -p vestrace-infrastructure --test embedding_schema_contract --test embedding_runtime_role_refusals --test p03_upgrade_provisioning --test runtime_role_cannot_write_directly -- --nocapture
cargo build -p vestrace-fault-scenario
cargo test --test embedding_fault_scenario_e2e -- --ignored --nocapture --test-threads=1
cargo fmt --all -- --check
cargo clippy -p vestrace-application -p vestrace-infrastructure -p vestrace-fault-scenario --all-targets -- -D warnings
node --test tests/p02_scope.test.mjs tests/p03_scope.test.mjs tests/p04_scope.test.mjs
node scripts/verify-dirty-baseline.mjs --check . docs/development-evidence/v1-g0-04r-preflight.json --scope p04-scope.mjs
node scripts/protocol-lock.mjs --check .
git -c safe.directory=E:/Soft/vestrace diff --check
```

#### Planning verification and acceptance state

Observed during this planning continuation: P04 scope tests **6 passed, 0
failed, exit 0**. Reopened dirty-baseline check **exit 1** with
`preflight HEAD differs from current HEAD`, as documented above. No Cargo,
database migration, host-vault mutation or acceptance probe was run.
An independent read-only review of this revised contract returned
**VERDICT: APPROVE**, with no BLOCKER, MAJOR or MINOR findings. The lead accepts
the revised plan, including active-only legacy recovery and the prohibition on
mixed-version host writers. This is plan acceptance, not implementation
acceptance. The proposed 125/23 scope amendment still requires explicit user
acceptance before the builder starts; the stale reopened preflight must then
be recaptured and verified. Final planning-document `git diff --check` passed.

#### Task 14E user approval and scope activation (2026-09-08)

The user replied `eутверждаю` to the explicit request to approve the revised
14E implementation plan and exactly five additional scope paths (120 -> 125,
23 protected paths unchanged). This authorizes implementation of the revised
contract and the five-path amendment listed above. The coordinator continues
as lead under the user's explicit role assignment; a separate persistent Codex
builder performs implementation. Prior planning/review text remains historical.

Both preflight scope arrays and the scope test are updated to 125/23. The
reopened snapshot is recaptured from the live worktree at the actual HEAD,
with raw porcelain and protected-file hashes; its previous capture is retained
as historical provenance inside that JSON. The original preflight's historical
snapshot fields remain unchanged. Production implementation starts only after
the updated scope tests and reopened verifier pass.

#### Task 14E implementation entry checks (2026-09-08)

The scope module and both preflight arrays now contain exactly 125 change
paths and 23 protected paths. The original preflight's old 102-path array is
retained in `scope_synchronization_history`; its historical capture fields
are unchanged. The reopened preflight's full previous 120-path capture is
retained in `previous_captures`; its current capture uses the live approved
HEAD and five dirty planning/scope files. Reopened verification returned
**exit 0**, focused P04 scope tests **6/6**, and combined P02/P03/P04 scope
tests **16/16, exit 0**. These are entry-gate results, not 14E acceptance.

The existing local `vestrace-test-postgres` container was restarted without
deleting data. PostgreSQL accepts connections on loopback port 55432; database
`vestrace_test` uses bootstrap role `test` (superuser). Runtime `vestrace` and
`vestrace_guarded_owner` are both non-superuser and lack CREATEDB. A real
runtime login was verified without changing roles or passwords. Credential
values are not recorded here. SHA256 values for all 124 existing migration
files were captured before production edits for final immutability checking.

The first Terra builder stopped on a usage-limit error before production
edits. A replacement persistent native Codex builder owns SQL, repository,
provisioning and integration; a bounded helper owns application and then host
vault work. Cargo execution is serialized through the integration builder.

#### Task 14E implementation review corrections (2026-09-08)

Live schema review found that the revised plan's name
`embedding_index_generations` is incorrect: the existing Ready-generation
relation is `embedding_corpus_generations` (including the ready/stale lifecycle
in migration 0191). Publication locking, staling and provisioning inventories
must use that existing relation. This corrects a schema anchor within the
approved files; it does not authorize a new table or a scope expansion.

Exact legacy-adoption replay must follow the established owned blocker after
validating the current pinned credential and exact association. Historical
nonterminal protection is required when first creating the adoption; once the
fresh owned protection exists, a legitimate completion by the historical
blocker's unrelated owner must not invalidate that adoption. The finalizer
still never releases or changes the historical blocker.

Runtime provisioning exposed an existing ownership round-trip ACL loss for
`vestrace_publish_embedding_corpus_generation(UUID,UUID,UUID,BIGINT)`:
the generation-fence bridge changes its owner back without restoring runtime
execution, whereas migration 0191's fallback explicitly grants that execution
(0191:252). The 0195 finish helper and SQLx fallback must restore that exact
documented grant; fixtures must continue calling the guarded function as
runtime. No broader function or table read grants are authorized by this fix.

During implementation an attempted blanket restoration of table SELECT was
rejected by automatic approval review before execution. The narrower accepted
repair restores only the eleven documented preexisting table reads and four
preexisting REFERENCES grants; generic attachments, ordinary references and
erasure blockers receive no new runtime reads. Existing migrations remain
unchanged: a fresh comparison against the captured 124 SHA256 values found
zero differences. The reopened dirty/scope verifier also returned exit 0.
These observations do not constitute full 14E acceptance.

The SQL command and deferred invariants intentionally enforce overlapping
safety predicates. For mutation qualification, an explicitly documented
composite fault-injection mutant may disable the minimal enforcement sites
of one logical invariant to expose that invariant's unsafe persisted state.
It is not evidence that deleting one statement alone defeats the remaining
guards. Every mutant must list all changed sites, retain the unchanged probe
and assertions, run only against an ephemeral database, and record full
original/mutant/restored hashes plus exact restoration and GREEN before the
next mutant. Compile failures, SQL refusals and another guard blocking the
unsafe state do not qualify as mutation-sensitive RED. Unrelated safeguards
must remain enabled; different logical invariants are tested independently.

#### Task 14E interim observed verification (2026-09-08)

The builder reported `cargo +stable test -p vestrace-application
embedding::finalization -- --nocapture`: exit 0, 7 passed, 0 failed. The
repository decoding/HMAC unit filter passed 3 tests, exit 0. The focused host
`embedding_output_keys output_disposition_` run passed 2 tests, exit 0,
including actual independent-process decision winners/death and legacy/corrupt
record handling. These are focused results, not the full host suite.

The first real end-to-end publication probe,
`cargo +stable test -p vestrace-infrastructure --test
embedding_result_finalization all_outputs_publish_with_one_corpus_event --
--nocapture`, returned exit 0: 1 passed, 0 failed, 1 filtered out (5.15 s).
It used the real service, PostgreSQL repository and host vault for two outputs;
persisted assertions covered exact keyed commitments, immutable attachment
history, corpus/event/job changes, Ready-to-stale and published replay with an
unavailable vault. The partial-binding probe had separately passed. Later
credential/concurrency additions and the crash scenario were not yet executed
at this checkpoint.

The full 14D regression initially returned 12 passed and 2 failed. Both failures
were corruption-fixture trigger-restoration errors with pending new deferred
events (SQLSTATE 55006). The fixture was updated to bypass the new write guard
alongside its existing corruption injection, retaining the independent runtime
loader refusal assertions. Its corrected rerun was still pending here.

Independent lead checks: combined P02/P03/P04 scope tests 16/16, exit 0;
`node scripts/protocol-lock.mjs --check .` exit 0. Full acceptance, mutation
qualification and fault execution remain outstanding at this checkpoint.

The later full finalization run passed 7/7, including shared-credential
ownership, genuine 0194-to-0195 legacy adoption, revocation-first refusal,
concurrent finalizers and both source-erasure lock orders. A subsequent run
including two additional history/generic-finalizer probes passed 9/9. The
corrected 14D regression passed 14/14. The accompanying schema run passed
15 tests and failed one closed-world function-inventory check; the provisioner
was updated to use its actual `allowed_targets` and
`runtime_executable_targets` arrays, with the rerun still pending here.

One planned winning-race scenario has a live authority constraint:
`vestrace_rotate_credential` is last replaced in migration 0185 (definition at
1299). Its active embedding-head check at 1484-1496 refuses rotation with
`rotation requires P04 embedding transition evidence`; no later replacement
exists. A successful rotation-first fixture must not bypass that guard or
manufacture transition authority. The existing refusal is retained; successful
rotation-before-adoption remains unexercised in this slice. Similarly, there
is no existing runtime completion command for the historical unrelated generic
credential blocker; direct owner mutation is not legitimate completion proof.


#### Task 14E observed mutation records (interim)

These records are observed builder results, independently reviewed by the lead.
They do not replace the still-outstanding full eleven-invariant matrix.

**extra published identity validation** (additional).

Command: `cargo +stable test -p vestrace-application malformed_published_identity_is_refused_before_vault -- --nocapture`.

Observation: Published wrong identity accepted; unchanged Conflict assertion failed at finalization.rs:534.

- Original SHA256: `DEDD67A6CB3E679DB68613AECFE324E520EF2390DF31E09E0A86989B6BFC3F7E`
- Mutant SHA256: `70A290AA9E643407B3DD9CB56E650E1C0BE2F719592BB29C5D9A655209473CCC`
- Restored SHA256: `DEDD67A6CB3E679DB68613AECFE324E520EF2390DF31E09E0A86989B6BFC3F7E`
- RED exit 101; restored GREEN exit 0.

**host exact preparation match in Bound adoption** (required).

Command: `cargo +stable test -p vestrace-infrastructure --test embedding_output_keys output_binding_is_exact_durable_and_refuses_generic_and_provisional_access -- --nocapture`.

Mutant: bind winner check preparation_id != requested changed to preparation_id.is_nil().

Observation: wrong preparation returned Ok(MaterialKeyBindingReceipt) rather than Err(BindingMismatch); unchanged test failed line3026.

- Original SHA256: `D1342962660DB84D0BA2ACAB162FC665CC91087C7A4B35067CA9E222A332CADD`
- Mutant SHA256: `8A185AED25A16FCF64E52F9A86CE131C306A99D1CBDA38096E384EBD62E61647`
- Restored SHA256: `D1342962660DB84D0BA2ACAB162FC665CC91087C7A4B35067CA9E222A332CADD`
- RED exit 101; restored GREEN exit 0.

**host common bind/retire arbitration** (required).

Command: `cargo +stable test -p vestrace-infrastructure --test embedding_output_keys output_disposition_process_winners_survive_death_and_refuse_the_loser -- --nocapture`.

Mutant: bypass only output_retirement_decision common publication/adoption after before-decision checkpoint; retain initial precheck and return candidate fence.

Observation: RED 101, 0 pass/1 fail: stale retirement loser succeeded with persisted erasure receipt after Bound winner at test line3149; exact restore GREEN0 1pass/0fail0.40s, RAII cleanup leaves no orphan.

- Original SHA256: `D1342962660DB84D0BA2ACAB162FC665CC91087C7A4B35067CA9E222A332CADD`
- Mutant SHA256: `EDCFD331FEF679D5ADE2DA505D7D2CE3238494DE56026A6D268561E8DE9E803E`
- Restored SHA256: `D1342962660DB84D0BA2ACAB162FC665CC91087C7A4B35067CA9E222A332CADD`
- Unchanged final probe SHA256: `533FD4DD092BBF6BDA36607EF34FC331E6A4310413CA8CD6F1059FE5AC65B12F`
- RED exit 101; restored GREEN exit 0.

#### Task 14E crash and expanded gate checkpoint

`cargo +stable build -p vestrace-fault-scenario` returned exit 0. The subsequent
ignored `result_finalization_survives_a_real_child_abort` root E2E probe passed
1/1, exit 0 (20.61 s). Its unchanged assertions verified five explicit crash
checkpoints plus all sixteen exact SQL-leg observations: actual child PIDs,
observed AFTER-row advisory barriers, OS process termination, database backend
termination, exact persisted rollback, reopened bound-key callbacks, two
published outputs, one loopback provider request and vault-free Published
replay. This is process-death evidence, not power-loss qualification.

The corrected schema suite passed 16/16, exit 0. The expanded finalization
suite passed 14/15; the remaining failure was the guarded legacy credential
erasure fixture's commit (`23514`, credential erasure preparation inconsistent).
It remains unresolved here and is not an ignored or passing test. Existing
rotation and historical-blocker authority limits described above also remain.

A new deterministic all-bound publication race regression passed unchanged
(1/1, exit 0, 5.50 s): current SQL already rechecked Published after acquiring
the connection guard. No pre-fix RED is claimed for that path. Public legacy
adoption separately received the corresponding post-connection Published check;
its additional focused regression was pending at this checkpoint.

#### Task 14E further qualification checkpoint

The focused public-adoption replay after guarded credential revocation passed
1/1, exit 0. The legacy erasure-first probe still fails, exit 101: read-only
precommit diagnostics observed intent `erasure_prepared`, occupancy `activated`,
one ciphertext row, one prepared event, zero terminal events and zero audit
tombstones. Commit rejects with SQLSTATE 23514. Source review identifies two
historical candidate-only checks: `vestrace_validate_material_erasure()` and
`vestrace_finalize_credential_material_erasure(UUID,UUID)` in migration 0174.
The guarded retired/revoked preparation in 0181 preserves activated occupancy.
No fixture occupancy rewrite, validator bypass or upstream repair was applied.
Any repair needs its own explicit contract; this remains a failed acceptance
probe, not passing evidence.

Independent read-only review of application finalization, host disposition,
repository/recovery adapters and the crash harness returned PASS for that Rust
subset with no new confirmed BLOCKER/MAJOR findings. It did not review SQL during
mutation execution and is not overall Task 14E acceptance.

The following SQL mutation runs used the unchanged finalization test file
SHA256 `55D240229B04FDE18EF0E3F6A2F5347ED60C158ABECE86F962F397E46C60B841`.
For every row the original and byte-exact restored migration SHA256 was
`3C159DEB3259E92A88A78CDC984897C778759A085AE459460C29A25C5B2E5BBC`.
Each RED exited 101 with one failing behavioral probe; each restored GREEN
exited 0 with 1/1 passing, before the next mutation began.

- Attachment history: command `cargo +stable test -p vestrace-infrastructure --test embedding_result_finalization publication_preserves_exact_attachment_history -- --nocapture`.
  Remove only prepared-phase `output.attachments<>1`. RED observed committed
  immutable history pointing to a missing generic attachment. Mutant SHA256
  `388AE3300035A503E2569331712B65856EBE16D28215F6E572BE3EE38FC7C0C2`.
- Published-first loader/service: command `cargo +stable test -p vestrace-infrastructure --test embedding_result_finalization all_outputs_publish_with_one_corpus_event -- --nocapture`.
  Replace the loader's Published return with coherent `ready_to_publish` phase.
  RED reached the forbidden NeverVault callback (`published replay touched vault`).
  Mutant SHA256 `90FAFAE46E594F5590E54FDB3C916812A03CE7B4DA2F0DA77A7EDD7FCC0C3AF7`.
  This does not independently qualify the recovery branch.
- Job/publication coupling: same `all_outputs_publish_with_one_corpus_event` command.
  Omit Succeeded/version UPDATE, its immediate NOT FOUND check and the matching
  published job-state/version predicate. RED observed committed publication,
  event, two Live outputs and counters 1/2/1 with job Running/version 2 instead
  of Succeeded/version 3. Mutant SHA256
  `B2E2C0C5ED029E3D60AF6FB942A60BC221392BA892FB918C2285DF9D04A7326C`.
  Event omission itself is not qualified by this composite mutant.
- Ready invalidation: same full-publication command. Omit Ready-to-stale UPDATE
  and the matching invalidated-generation stale predicate. RED observed persisted
  generation `ready` instead of `stale`. Mutant SHA256
  `60287023901CE7159214DBFDF702ABB67F8D5F43355954D47AD5B24C8C58333F`.
- Corpus/generation counters: same full-publication command. Omit corpus
  revision/live-member UPDATE, generation-epoch UPDATE and the matching current
  counter consistency block. RED observed committed publication/event and
  Succeeded job with counters 0/0/0 instead of 1/2/1. Mutant SHA256
  `BA4A0F91CB264C1EF5976AF95AE18FCBB34A4781120938E8A4EE861FD9F5844A`.

These are explicitly bounded logical-invariant fault injections, not claims
that removing each individual line defeats every independent guard.

Output projection promotion used the same full-publication command, original /
restored SQL hash and unchanged probe hash recorded above. The composite omitted
the projection Live/retention/commitment UPDATE and only its matching central
published predicates and material-validator `p.state='live'` check. RED exit 101
observed committed publication/event, Live materials/intents and Succeeded job
with zero Live projections/commitments instead of two. Mutant SHA256
`00F62DCBBBF36F222F5BAE19CD8CC2D98CFED84F9C978C1CBED42E611D7B1F5F`;
restored GREEN exit 0, 1/1 (5.03 s).

Broader runtime verification subsequently passed: `embedding_effect_recovery`
19/19, `embedding_runtime_role_refusals` 3/3, `p03_upgrade_provisioning` 7/7 and
`runtime_role_cannot_write_directly` 45/45, all exit 0. The upgrade test's exact
guarded-table inventory was extended from 92 to 97 for the five new 0195 tables,
conditional on observed successful migration 195 for historical-stage probes.
Root reviewed this diff; no production privilege was changed for this repair.
P02/P03/P04 scope tests again passed 16/16; protocol lock, reopened baseline and
diff checks returned 0. All 124 pre-0195 migration hashes still match the entry
capture byte-for-byte.

Two required single-gate mutation attempts did **not** establish sensitivity.
Both used original/restored SQL SHA256
`3C159DEB3259E92A88A78CDC984897C778759A085AE459460C29A25C5B2E5BBC`
and unchanged test SHA256
`27030735598864AE08CC2681E00FEC8120CE55F739DE1CBDCB716F99DD98444B`.

- `cargo +stable test -p vestrace-infrastructure --test embedding_result_finalization generic_finalizer_cannot_publish_bound_embedding_output -- --nocapture`:
  removing only the generic finalizer's explicit embedding-output refusal gave
  1/1 PASS, exit 0, because remaining validators still refused the unsafe commit.
  Persisted Live/ordinary-reference counts did not change. Mutant SHA256
  `F36D54637CDCD22A8AB8F0CE7A644483B98244CBC858AA07E8D3ABB118EBDA14`;
  exact restore also passed 1/1, exit 0 (4.80 s).
- `cargo +stable test -p vestrace-infrastructure --test embedding_result_finalization partial_binding_preserves_history_and_stays_non_live -- --nocapture`:
  removing only the `phase<>'ready_to_publish'` exception gave 1/1 PASS, exit 0.
  Strict-subset direct publication still refused and rolled back with identical
  persisted Facts. Mutant SHA256
  `CE78BF6464513DCB924D1CF6EEF5AD923FF704F7F4F3E1F71D7906DACF6E80F5`;
  exact restore passed 1/1, exit 0 (5.05 s).

Neither run is RED evidence. Root declined to remove multiple independent
validation families solely to manufacture unsafe success. The two explicit
mutation-acceptance obligations remain unqualified; no acceptance waiver is
inferred from redundant defenses.

The Live-source composite did establish behavioral sensitivity. Command:
`cargo +stable test -p vestrace-infrastructure --test embedding_result_finalization publication_defensively_refuses_a_corrupted_non_live_source -- --nocapture`.
Remove only the central source material/intent Live predicates and their
locked-source recheck; retain identities, blockers and dependency validation.
RED exit 101 observed committed publication/event, Live outputs and Succeeded
job over the explicitly corrupted non-Live source instead of unchanged prepared
Facts. Original/restored SQL SHA256
`3C159DEB3259E92A88A78CDC984897C778759A085AE459460C29A25C5B2E5BBC`;
mutant `399C04CB013E022542EF2FA330C06BFBDD6F39F084DF124FFAE622CA2909FE35`;
unchanged probe SHA256
`27030735598864AE08CC2681E00FEC8120CE55F739DE1CBDCB716F99DD98444B`.
Exact restore GREEN: exit 0, 1/1 (4.91 s). This is a defensive-corruption probe,
not evidence that legitimate source erasure won against a nonterminal blocker.

Credential blocker ownership mutation: command
`cargo +stable test -p vestrace-infrastructure --test embedding_result_finalization publication_releases_only_job_owned_credential_blocker -- --nocapture`.
The release query additionally selected same-workspace/same-credential effect
blockers with no owned association. No other job-owned blocker or independent
validator was changed. RED exit 101 observed the unrelated blocker's persisted
state `terminal` instead of `nonterminal`; exact restore GREEN exit 0, 1/1
(6.20 s). Original/restored SQL SHA256
`3C159DEB3259E92A88A78CDC984897C778759A085AE459460C29A25C5B2E5BBC`;
mutant `60005E946020DB21167870247BD062D6E4627796EE092FC4DB253255A8DA7320`;
unchanged probe SHA256
`27030735598864AE08CC2681E00FEC8120CE55F739DE1CBDCB716F99DD98444B`.

All eleven planned logical-invariant mutation attempts have now been addressed:
nine have qualifying unsafe-state/callback RED -> exact restore -> GREEN
evidence; generic-finalizer and all-bound single-gate attempts remain explicitly
NOT SENSITIVE. The Succeeded leg satisfies the contract's rebuild-event **or**
Succeeded alternative; no independent event-omission claim is needed or made.
Published-first evidence is the actual loader/service NeverVault callback test;
the additional real repository recovery regression is positive evidence only.

#### Task 14E final expanded checks (before dispatch-fixture repair rerun)

Final focused unit runs passed: application result 4/4, application finalization
7/7 and infrastructure finalization HMAC/decode 3/3, all exit 0. The complete
finalization suite passed 16/17, exit 101; the sole failure is the retained
historical credential-erasure commit. The new production-shaped repository
recovery test passed: after guarded credential revocation, two real recovery
calls return Succeeded with unchanged publication/history/Facts and dispatch,
admission, lease, effect and receipt footprints.

The first complete ignored fault suite returned 2/3, exit 101. Result preparation
and finalization passed; old dispatch setup failed before its checkpoint because
bare job acceptance supplied no exact output receipts to the 0194 admission
gate. The fixture was repaired in the two already-scoped dispatch/preparation
scenario files by sharing the existing real-host output acceptance/receipt/source
policy setup. Original checkpoints, baseline assertions and production guards
remain. Root independently reviewed the diff. Full rebuild/rerun is pending at
this checkpoint; the repaired fixture is not yet declared passing here.

`cargo +stable fmt --all -- --check` returned exit 0 after targeted formatting.
The exact mandatory Clippy command returned exit 101 before changed components
were checked, on existing domain lints:

- `src/conformance/cases.rs:4719,8855,8914`: cloned_ref_to_slice_refs.
- `src/retrieval/mod.rs:46`, `src/security/capability.rs:20`,
  `src/security/mod.rs:143`: derivable_impls.

An additional diagnostic run of the same selected packages with `--no-deps`
also returned 101, at existing application `src/runs/recovery.rs:282`
(`manual_contains`). This diagnostic is not a replacement for the mandatory
command and does not establish a clean lint result for the changed components.
No lint was suppressed and these unrelated files were not edited.

The repaired full fault suite subsequently passed **3/3, exit 0 (43.41 s)**
after a successful binary rebuild. The original dispatch scenario retained all
four checkpoints/control baselines; preparation crash and finalization's five
checkpoints plus sixteen SQL-leg observations also passed. No fault mutation
remains active. Root verified final 0195 SHA256
`3C159DEB3259E92A88A78CDC984897C778759A085AE459460C29A25C5B2E5BBC`.
Final P02/P03/P04 scope tests passed 16/16, exit 0; reopened dirty-baseline and
protocol-lock checks returned 0 with the repaired scoped fixture files present.

**Task 14E acceptance: NOT APPROVED.** The delivery path is implemented and the
authorized verification/fixture repairs above are complete, but acceptance is
not inferred from those passing subsets. Outstanding:

1. Legacy credential-erasure-first acceptance still fails at the historical
   candidate-only validator (full finalization suite 16/17, exit 101).
2. Generic-finalizer and complete all-bound single-gate mutation obligations
   remain NOT SENSITIVE; their original criteria have not been waived.
3. Mandatory Clippy exits 101 on unrelated existing domain lints. The additional
   no-deps diagnostic also fails on existing application recovery code.
4. Successful rotation-before-adoption remains unexercised because the existing
   guarded rotation command requires future P04 embedding transition evidence.
   Its actual refusal is verified; that is not a successful rotation-first race.

The following repair proposal is the next concrete, unactivated decision.
Its approval alone would not waive mutation, lint or rotation qualification.
Final test-file SHA256 after edition-2024 formatting is
`F68C2846A4A2AA8F3F052FDDD1F261430AE41F36BD64BBE0071534C037B03D22`.
The proof records retain their actual run-time hashes; subsequent assertion
wrapping is nonsemantic and does not replace those captures retrospectively.
Final post-repair formatter and exact diff checks both returned exit 0.

#### Proposed separate retired/revoked credential erasure repair — NOT AUTHORIZED

Problem: the legitimate 0181 retired/revoked entrypoint reaches ErasurePrepared
with activated occupancy, but both 0174 preparation validation and credential
finalization accept only candidate occupancy. The first transaction cannot
commit; changing only its validator would leave finalization failing after host
erasure. This is upstream of embedding finalization. The frozen P03 plan
requires exact non-current Retired/Revoked destruction (Task 3, lines 228/335).

Proposed forward-only repair, subject to explicit approval:

1. Add `migrations/0196_retired_credential_erasure.sql`, replacing only
   `vestrace_validate_material_erasure()` and
   `vestrace_finalize_credential_material_erasure(UUID,UUID)` with the original
   candidate branch retained and a narrowly evidenced activated branch. Require
   exact intent/workspace/connection/slot/revision identity, an existing
   non-current slot, and immutable exact revoked activation or outgoing
   rotation evidence. Preserve fence, receipt, bytes, events, audit, content
   and completed-finalizer replay checks. No Active/current admission.
2. Extend the existing one-shot provisioning/hand-back pattern for these two
   replacements: guarded owner, private validator, only the existing runtime
   finalizer signature executable. Fresh, runtime-upgrade and SQLx fallback
   must have equivalent inventories. No runtime grants to lifecycle tables.
3. Add full ordinary erasure lifecycle tests to existing
   `crates/vestrace-infrastructure/tests/credential_activation.rs`, reusing its
   guarded activation/rotation/revocation setup with a new real host-vault
   fixture in that file. Existing mock vaults and generated receipts/ciphertext
   are not host erasure proof. Execute the runtime-authorized 0181 prepare
   command, actual HostMaterialKeyVault fence/erase and the existing repository
   record_fence/finalize_credential methods, observing committed state between
   phases and exact final receipt replay. Refuse Active/current and mismatched
   or missing evidence. Retain Candidate-abandon and content regression coverage.
4. Re-run the existing 14E legacy-marker erasure-first refusal against an old
   0194 marker after upgrading to the repaired schema, with erasure winning
   before adoption. Do not claim that unmodified 0194 could commit this erasure.
   The marker stays immutable and the refusal must leave no owned blocker.
5. Capture regression RED before the new migration and GREEN afterward, all
   existing credential activation tests, content/Candidate regressions, exact
   upgrade/ACL checks and the complete 14E finalization suite.

Exact additional scope paths proposed: the new 0196 migration and existing
`crates/vestrace-infrastructure/tests/credential_activation.rs` (125 -> 127
change paths; protected paths remain 23). Other edits are limited to the
already-authorized provisioner, upgrade/schema/finalization tests and scope /
preflight / evidence administration. Old migrations remain byte-identical.
This section is a reviewable proposal only; no scope activation or repair has
been performed. It does not authorize transition-aware credential rotation,
embedding erasure propagation or production embedding execution.
The generic MaterialErasureService currently calls the superseded P02 Candidate
entrypoint through `src/postgres/erasure.rs`; composing ordinary retired/revoked
erasure into that service is explicitly outside this two-path repair. The new
probe proves the authorized guarded-command/repository/real-host route, not
production service composition. Independent plan review identified this limit;
the proposed test route above has been corrected accordingly.

#### Retired/revoked credential repair — user approval and entry

The user explicitly replied `подтверждаю` to the two-path proposal above.
This activates precisely `migrations/0196_retired_credential_erasure.sql` and
`crates/vestrace-infrastructure/tests/credential_activation.rs`: change scope
125 -> 127, protected scope unchanged at 23. The proposal and its corrected
real-host test route are now the implementation contract. Earlier NOT AUTHORIZED
wording records the proposal's historical state, not the current authorization.

Both preflights retain their historical baseline fields. Their new scope
amendment records the actual entry HEAD, raw untracked-aware status, hashes of
the existing dirty files and all 125 pre-0196 migrations before activation.
Old migrations including 0195 must remain byte-identical. The accepted review
correction requires a real host-vault fixture and the existing guarded SQL /
repository route; it does not compose the generic erasure service. Existing
mutation, lint and rotation evidence gaps remain separate and are not waived.

Entry gates passed: P02/P03/P04 scope 16/16, exit 0; reopened baseline exit 0.
The entry amendment captures 29 dirty paths and 125 existing migrations.

#### Credential repair observed RED before 0196

Command: `cargo +stable test -p vestrace-infrastructure --test credential_activation erasure_uses_real_host_and_commits -- --nocapture`.
Migration 0196 was absent. Both new legitimate runtime tests failed at the
preparation COMMIT: 0 passed, 2 failed, exit 101; SQLSTATE 23514,
`credential erasure preparation is inconsistent`, validator line 64. Revoked
and rotated/retired fixtures used actual host-created keys and codec ciphertext.
Neither reached host erasure. The unchanged probe file SHA256 was
`AE7AB59D9C4B808646EA353CB1423DDD4A60100560281382E1BB3BFB06ECD36E`.
This is the behavioral regression RED, not a compilation or fixture-setup
failure. The functions-only upgrade bridge was present but not invoked; the
old validators/finalizer remained unchanged.

The unchanged pre-0196 `erasure_is_one_way` suite passed 16/16, exit 0
(14.23 s), establishing the content/Candidate baseline before repair.

#### Credential repair observed GREEN and review

After adding 0196, the identical real-host command and probe SHA256 above
passed 2/2, exit 0 (4.49 s). Both routes committed preparation, actual host
fence/erase, repository finalization, exact ciphertext/event/audit observations
and receipt replay. New migration SHA256:
`C5B828DF1A6B8E646905A5605CBC044AA73AB3B936E7A7E2DF0563EB078CA230`.
An initial cached test executable had retained the old embedded migration list;
that rerun was not new-schema evidence. Mtime-only invalidation forced a real
rebuild without changing the probe bytes before the GREEN recorded here.

Independent read-only review returned APPROVE for the two-function repair.
Root additionally compared the content branch, finalizer lock/replay prefix and
SQL mutation/audit tail with 0174; each is identical after newline normalization.
The new activated branch requires exact occupancy/intent/revision/key identity,
an existing non-current slot and exact immutable revoked or rotation evidence.
The total `IS NOT TRUE` check refuses NULL eligibility. Only the two functions
change ownership during runtime upgrade; no table ownership or ACL is lent.

Full repair verification then passed, all exit 0: credential activation 16/16,
embedding result finalization 17/17, embedding schema 16/16, unchanged
`erasure_is_one_way` 16/16 and upgrade provisioning 9/9. Upgrade evidence includes
fresh provisioning, runtime 0195 -> 0196 with identical table owner/ACL/RLS
inventory, SQLx-superuser fallback and closed one-shot function bridges.
The historical finalization fixture creates its immutable marker under 0194,
upgrades through 0196, observes the real lease expire, commits guarded credential
erasure preparation, and then observes adoption refuse without any owned blocker
or marker mutation. It does not claim that old 0194 could commit erasure.
Root rechecked all 125 pre-0196 migrations: every entry hash remains unchanged.

A further defensive finalizer probe initially passed 1/1, but root review found
its other revoked credential was in a different workspace and hidden by RLS.
That run established missing-evidence refusal only. The probe is being corrected
to use a legitimate, runtime-visible other tuple in the same workspace before
claiming substitution refusal. Only the exact target revoked event is removed
inside the isolated fixture, with the sole disabled immutable trigger restored
and checked before the runtime finalizer; no production guard is weakened.

#### Credential repair final acceptance — APPROVED

The corrected defensive fixture creates another legitimate revoked credential
in the same workspace, with different connection/slot/revision/intent. Before
corruption, it observes that event under the guarded function's actual owner
and workspace-RLS context. After removing only the target event and restoring
its immutable trigger, the runtime finalizer rejects the real host-erasure
receipt with 23514 and leaves SQL state/ciphertext/events/audits unchanged.
This is explicitly a defensive-corruption test, not a legitimate deletion of
immutable evidence and not a claim that runtime has table SELECT/DDL rights.

Final frozen-source `credential_activation` run: **17/17, exit 0 (10.85 s)**.
Test SHA256: `DFCEF263833CD333C3D2F0C3E01FF86EDFE71BCF0AA9E0058A92EA145957D003`.
An earlier run overlapped the final fixture edit and is not attributed to this
source capture. The original unchanged two-test RED/GREEN capture remains as
recorded above. No production SQL changed after the reviewed 0196 hash.

Final verification: finalization 17/17, schema 16/16, content/Candidate erasure
16/16, upgrade/fallback 9/9; all exit 0. Formatter and diff checks returned 0.
Root's final P02/P03/P04 scope tests passed 16/16, exit 0; reopened baseline and
protocol lock returned 0. Scope remains 127/23. All 125 preceding migrations,
including 0195, remain byte-identical to the approved repair entry capture.

Lead verdict: **APPROVED for this two-path credential repair only**. It removes
the historical erasure blocker from Task 14E's 17-test finalization gate. It
does not constitute full Task 14E/P04 acceptance: the two NOT-SENSITIVE mutation
obligations, the existing Clippy failures and the successful rotation-before-
adoption qualification remain unresolved. Generic erasure service composition
is unchanged and outside this repair. No commit, push or deployment occurred.

#### Task 14E final qualification evidence amendment - AUTHORIZED

The earlier Clippy failure used floating `+stable` (1.97), contrary to the
repository pin. The exact approved command uses plain `cargo` under
`rust-toolchain.toml` 1.85.0. Its first real run found two scoped fixture
compatibility/lint issues: nested `use super as common` and an explicit
`unwrap_or_else(WorkspaceId::new)`. The minimal equivalent fixes are
`use crate::common` and `unwrap_or_default()`; `WorkspaceId::default()` delegates
to `new()`. The unchanged approved Clippy command then passed, exit 0 (1m02s).
`cargo fmt --all -- --check` and diff check also returned 0. The floating-stable
errors remain historical diagnostics and are not the pinned acceptance gate.

Migration 0185 intentionally refuses current-head rotation at lines 1484-1496
until later transition authority exists; transition activation is an explicit
Task 14E non-goal. The existing test proves that rotation itself refused. It
does not prove the separate contract requirement at lines 4028-4030 that
adoption refuses after a credential has already been lawfully rotated/retired.
The successful ordinary retirement-erasure lifecycle from 0196 uses a different
configuration and cannot substitute for that legacy-adoption scenario.

Two mutation requirements remain internally inconsistent with the implemented
independent defenses. Removing either named entry gate alone still reaches a
separate validator and cannot commit unsafe state. Independent source review
found no single narrow enforcement seam that permits the required unsafe state
without disabling multiple distinct safety families. Manufacturing such a
composite would weaken more authority than the named mutant and would provide
misleading evidence.

The user explicitly approved this exact amendment in the current continuation turn.
No approval timestamp is asserted here. Authorized amendment:

1. For **only** generic-finalizer refusal and complete-all-bound count, replace
   the unsafe-persisted-state RED requirement with a defense-preserving mutation
   proof. Strengthen the unchanged probes to assert the primary gate's exact
   SQLSTATE/message and an independently observed complete Facts/history
   baseline before evaluating the error.
2. Remove only the named primary gate. RED is the unchanged probe rejecting the
   different downstream refusal: generic finalization reaches its deferred
   embedding-output invariant; partial publication reaches the checked unbound
   intent/update or equivalent exact downstream invariant. A separate connection
   must observe no Live output, no ordinary reference, no publication/event and
   unchanged job/counter/history state. Record actual downstream SQLSTATE/message,
   full original/mutant/restored SHA256 and exits.
3. Restore exact bytes. GREEN must re-establish the primary error and identical
   persisted baseline. Do not weaken or remove any downstream validator.
4. Keep all nine existing unsafe-state/external-callback RED -> restore -> GREEN
   mutation records unchanged. This is not a general mutation waiver.
5. Explicitly defer the lawful successful rotation-before-adoption proof for a
   current embedding dependency to the slice that introduces transition-aware
   rotation authority. Task 14E retains the verified atomic refusal of the
   current 0185 rotation command and the 0196 ordinary retirement lifecycle,
   but neither is labelled adoption-after-rotation proof. The later slice must
   commit a legal rotation first and then prove adoption returns the typed
   conflict with no owned blocker, publication or marker mutation. It may not
   bypass the existing guard or synthesize transition evidence.
6. No production code, migration, privilege or scope change is authorized by
   this amendment. Only the already-scoped finalization probe and evidence files
   may change. Scope remains 127/23; all migrations remain byte-identical.

When the two authorized defense-preserving RED/restore/GREEN runs succeed,
Task 14E may be accepted after final scope/baseline/protocol/format/diff checks.
P04, G0 and v1.0 remain incomplete.


#### Authorized final qualification amendment: observed defense-preserving proofs

Both probes use unchanged test source SHA256 `FEE146D71CD1FBCAD171B1CCD942706D1DE371A9EF59860DE6245360B4A21081` (`embedding_result_finalization.rs`). The full ordinary baseline passed 17/17, exit 0, before mutation execution. Fault injection changes only the named primary statement in a disposable SQLx database function obtained with `pg_get_functiondef`; no migration bytes or downstream validation families change. Function definition restoration occurs before the error oracle. Original/restored definitions are byte-identical; owner, exact proacl and runtime EXECUTE are asserted unchanged before/after. Both functions retain owner `vestrace_guarded_owner`, ACL `{vestrace_guarded_owner=X/vestrace_guarded_owner,vestrace=X/vestrace_guarded_owner}`, runtime EXECUTE true.

For each attempt, the runtime transaction is committed on unexpected command success or rolled back on error. A separate owner-pool observer reads complete Facts and immutable history after that transaction ends. Before checking the error, both probes independently assert exact baseline equality: publications/events 0, bindings 1, history 2, generic attachments 2, Live materials/intents/projections 0, commitments 0, ordinary references 0, nonterminal source blockers 6, job running/version 2, corpus revision/live-member count/generation epoch 0. History bytes/identities also compare equal. Thus these are defense-preserving primary-gate sensitivity proofs, not unsafe persisted-state proofs. The nine earlier unsafe-state/external-callback records remain unchanged.

Generic finalizer:
- Unchanged command: `cargo test -p vestrace-infrastructure --test embedding_result_finalization generic_finalizer_cannot_publish_bound_embedding_output -- --nocapture`.
- Mutation selected only with `VESTRACE_TEST_FINALIZATION_PRIMARY_MUTANT=generic`; removes the exact embedding-output primary refusal in `vestrace_finalize_bound_content_material(uuid)`.
- Original/restored function SHA256: `A7767832F886249FAC8E3CA0BDFC8AA7410B52D8BB5DE9AC968C5D56ABA33F93`; mutant: `6F6AD479182132B3E3C786D754EF124BA403E8D68A9B0CBA55DC47E04A3E0D7F`.
- RED exit 101, 0/1: actual downstream SQLSTATE 23514, `Live material requires its exact Bound promotion`; unchanged exact-primary-message assertion fails after unchanged persisted Facts/history are observed.
- After exact restoration and removing the environment selector: GREEN exit 0, 1/1; primary SQLSTATE 23514, `embedding output requires specialized publication`, same baseline.

Complete-all-bound primary gate:
- Unchanged command: `cargo test -p vestrace-infrastructure --test embedding_result_finalization partial_binding_preserves_history_and_stays_non_live -- --nocapture`.
- Mutation selected only with `VESTRACE_TEST_FINALIZATION_PRIMARY_MUTANT=all_bound`; removes only the `phase <> ready_to_publish` primary refusal in `vestrace_publish_embedding_job_result(uuid,uuid,uuid,uuid,uuid,uuid,uuid[],bigint[],bytea[])`.
- Original/restored function SHA256: `FE5EBDB2AED217C1B414D0B9B843DEE555C810251C1E9EB74DF1EFF794206338`; mutant: `718EF1444C3D4DE9C30FB65ACBB7C9063DDBC78ADAB92ECAF9E26A8AAA079364`.
- RED exit 101, 0/1: downstream SQLSTATE 23514, `publication intent changed`; unchanged exact-primary-message assertion fails after the separate observer verifies the complete baseline.
- After exact restoration and removing the selector: GREEN exit 0, 1/1; primary SQLSTATE 23514, `publication requires all exact bindings`, same baseline.

Discarded harness attempt: replacing the function while SET LOCAL ROLE was `vestrace_guarded_owner` failed before mutation installation with 42501 `permission denied for schema public`. This is not a qualifying RED. The corrected harness uses the existing SQLx owner pool for isolated function replacement/restoration, without grants or ownership changes. A restored-mode probe after the discarded attempt passed 1/1.


Final-source rerun: pinned Clippy identified `format_collect` only in the new test SHA renderer; replacing it with equivalent `fold`/`write!` made the exact mandatory pinned Clippy command exit 0. `cargo fmt --all -- --check` exited 0. The final test SHA256 is `5AF4C388913B4A83C8D900FCA376F1006A0DDAEDA8F5715920A3EC0A28C0D4A9`. Both unchanged commands above were rerun on this final source: generic RED 101 / restored GREEN 0 (1/1), all-bound RED 101 / restored GREEN 0 (1/1). All original/mutant/restored function hashes, observed primary/downstream messages, authority comparisons and complete Facts/history baselines were identical to the records above. This final rerun supersedes the earlier source hash for qualification.

Root-independent final checks: combined P02/P03/P04 node scope tests exit 0, 16/16; reopened dirty-baseline verifier exit 0; protocol lock exit 0. The 125-entry pre-0196 migration digest capture was checked in full with zero mismatches. Migration 0196 SHA256 remained `C5B828DF1A6B8E646905A5605CBC044AA73AB3B936E7A7E2DF0563EB078CA230`.


Review correction before acceptance: the earlier range replacement had also removed the original partial-binding probe's positive resume tail. Independent review recovered its exact prior text: reload NeedsBinding, compare the complete missing binding to the second original binding, resume through the real repository/host-vault/HMAC service, assert two Live materials and unchanged immutable history. That exact tail has been restored after the new primary-refusal assertion. The preceding source-SHA finality claim is superseded; qualification requires the subsequent full-suite and proof reruns on the restored complete probe. No downstream SQL guard, production source or migration changed.


Final pinned regression gates (plain Cargo, repository toolchain 1.85.0):
- `cargo test -p vestrace-infrastructure --test embedding_result_finalization --test credential_activation -- --nocapture`: exit 0; finalization 17/17, credential activation 17/17. The subsequent review restoration affects only the finalization test and is separately rerun below.
- `cargo test -p vestrace-application embedding::finalization -- --nocapture`: exit 0, 7/7; `cargo test -p vestrace-application embedding::result -- --nocapture`: exit 0, 4/4.
- `cargo test -p vestrace-infrastructure --test embedding_result_preparation --test embedding_output_keys --test embedding_effect_recovery --test embedding_schema_contract --test embedding_runtime_role_refusals --test p03_upgrade_provisioning --test runtime_role_cannot_write_directly -- --nocapture`: exit 0; preparation 14/14, output keys 28/28, recovery 19/19, schema 16/16, embedding runtime refusals 3/3, upgrade provisioning 9/9, direct-write refusal 45/45.
- `cargo build -p vestrace-fault-scenario`: exit 0. `cargo test --test embedding_fault_scenario_e2e -- --ignored --nocapture --test-threads=1`: exit 0, 3/3 (42.07 seconds), including the actual finalization child-abort matrix.
- After restoring the original partial-resume assertions, `cargo fmt --all -- --check`: exit 0; exact `cargo clippy -p vestrace-application -p vestrace-infrastructure -p vestrace-fault-scenario --all-targets -- -D warnings`: exit 0.


Final complete-probe qualification (supersedes both earlier test-source hashes): SHA256 `DED3E5944022719575F5E0A1A1E46B757E0B93411CFC03DB07B733382698045A`. The original positive partial-binding resume tail is present verbatim. `cargo test -p vestrace-infrastructure --test embedding_result_finalization -- --nocapture` passed 17/17, exit 0 (23.95 seconds), before the final proof pairs. Exact pinned fmt and Clippy both exited 0 on these bytes.

Both unchanged focused commands recorded above were then run again on this complete final probe: generic mutant exit 101 / restored exit 0 (1/1); all-bound mutant exit 101 / restored exit 0 (1/1). The actual original/mutant/restored function hashes are unchanged from the two records above. Generic downstream refusal remained 23514 `Live material requires its exact Bound promotion`; all-bound downstream refusal remained 23514 `publication intent changed`. Restored runs re-established their exact original primary messages. Every run observed the separate-connection complete persisted Facts/history baseline before the primary-error assertion; both restorations were byte-identical and retained exact owner/proacl/runtime EXECUTE. The restored all-bound GREEN additionally executed the original exact missing-binding comparison and successful real-service resume to two Live materials with unchanged history.

Builder recommendation: ACCEPT Task 14E qualification under the explicitly authorized final amendment. Nine unsafe-state/external-callback mutation records and these two defense-preserving primary-gate records are retained with their distinct meanings. Successful rotation-before-legacy-adoption remains explicitly deferred until the transition-aware rotation authority exists; no bypass is introduced and ordinary retired credential erasure is not presented as that proof. This recommendation does not close the remaining P04/G0/v1.0 program packages. No commit, push or deployment was performed.

#### Task 14E final lead acceptance - APPROVED

The lead accepts Task 14E under the user-authorized final qualification
amendment. The final complete probe SHA256 is
`DED3E5944022719575F5E0A1A1E46B757E0B93411CFC03DB07B733382698045A`.
The final independent read-only review returned `VERDICT: APPROVE` after the
original partial-binding resume assertions were restored. Both amended
mutation proofs completed RED 101 -> exact restore -> GREEN 0, the final
17-test finalization suite and all recorded pinned acceptance gates passed,
scope remains 127/23, and all migration digests remain unchanged.

This acceptance closes Task 14E only. The approved rotation-before-adoption
deferral remains binding, and P04, G0, and v1.0 remain incomplete. No commit,
push, or deployment was performed.
