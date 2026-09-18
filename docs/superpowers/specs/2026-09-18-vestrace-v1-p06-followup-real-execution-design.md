# P06 Follow-Up: Closing the Connector/Provider and Qualification Gaps — Design

**Status:** proposed
**Predecessor:** P06 (G1), closed BLOCKED per
`docs/development-evidence/v1-g0-06-real-execution.md`. This package is not
itself a fixed step in `docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md`'s
roadmap table — it is a follow-up the P06 evidence run's own findings called
for, scoped and named on its own.
**Goal:** close three of the four backend gaps P06's Task 8 found live —
Connector/Provider creation and the qualification-job executor — so a
Connection and its Models can reach a real `qualified` state end to end on a
fresh workspace, surviving a restart. **The fourth gap (composing
`GovernedRunStepInputAuthority` so AG-UI can execute) is scoped out during
this plan's own writing** (§3.4) and deferred to a further package — see
the note there for why. This package does not, by itself, make a real chat
message produce a model completion; it removes three of the four things
that were stopping it.

## 1. Context (what P06's evidence proved, verbatim from its own findings)

Four independent, live-verified gaps stop any real model completion today,
none of them a defect in P06's own six implementation tasks:

1. `connections.connector_id` is a real foreign key to `connectors(id)`;
   nothing creates a connector row; every fresh workspace's Connection
   creation fails `HTTP 500`.
2. Identical shape of gap for `models.provider_id` → `providers(id)`;
   `POST /v1/providers` is permanently and deliberately refused
   (`legacy_provider_registry_retired`) with no replacement.
3. No production binary ever executes a qualification job —
   `QualificationJobService::run_next_probe`'s only callers are integration
   tests — so every Connection/Model is permanently `blocked`.
4. `POST /ag-ui/run` (and any agent-assigned confidential-input Run step at
   all) is refused with `503`, because the real
   `GovernedRunStepInputAuthority` implementation
   (`GovernedRunStepInputReservation`) is never composed into any binary;
   `crates/vestrace-cli/src/commands/server.rs` wires the plain
   `RunCoordinator`, which explicitly refuses that combination.

Full detail, exact file:line citations, and live repro commands are in
`docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt`.

## 2. Approaches considered

**Chosen: minimal composition of what already exists.** For gaps 1 and 2,
auto-materialize the missing compatibility row transactionally, inside the
existing governed-creation SQL, using the id the caller already supplies —
no new HTTP route, no new request field, no migration. For gap 3, a new
worker poll loop, built to the same shape as the existing embedding-work
poller, driving the already-complete `QualificationJobService`/
`PgQualificationProbeRunner` machinery (which needs nothing beyond
collaborators the worker and server already construct). Gap 4 is deferred
to a further package — see §3.4.

**Rejected: full Connector/Provider CRUD** (a real `POST /v1/connectors`
and a governed replacement for `POST /v1/providers`, with console
selectors). This is exactly what `create_provider`'s own doc comment
already warns against: "Provider registry rows do not carry an immutable,
qualified connection revision. Leaving this compatibility route executable
would therefore create an object which the governed dispatcher must never
use." A general-purpose creation route re-introduces the same hazard the
prior architecture deliberately closed. Auto-materializing the row as a
pure shadow of an already-governed Connection/Model revision cannot be
misused the same way, because it never exists independently of one.

**Rejected: drop the foreign keys.** `connector_id`/`provider_id` could be
made nullable or removed. Rejected — both tables (migrations 0014, 0054)
are old, carry their own RLS policies, and a schema change to them is a
larger, riskier surface than the actual goal requires. Auto-materialization
achieves the same practical outcome (nothing blocks Connection/Model
creation) without touching the schema at all.

## 3. Design

### 3.1 Connector and Provider: auto-materialized compatibility rows

`crates/vestrace-infrastructure/src/postgres/connection_revision_repository.rs`'s
`insert_stable_connection` gains, immediately before its existing
`INSERT INTO connections`, in the same transaction:

```sql
INSERT INTO connectors (id, workspace_id, name, provider_type)
VALUES ($1, $2, $3, $4)
ON CONFLICT (id) DO NOTHING
```

bound to `command.connection.connector_id`, `command.connection.workspace_id`,
`command.connection.name` (reusing the Connection's own name — there is no
separate connector identity in this console flow), and a `provider_type`
derived from `ConnectionKind`: `LMStudioLocal → "local"`,
`OpenAiChatCompletionsV1 → "remote"`.

`crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`'s
`insert_or_verify_stable_model` gains the equivalent insert into `providers`
(`id, workspace_id, name, locality`) using `command.model.provider_id`,
`command.model.model_name` as the reused name, and a fixed `locality`
value (proposed: `"governed"`) — this field is vestigial in the current
architecture (real transport/locality lives on the Connection's `kind` and
`transport_policy`; nothing in the governed dispatch path reads
`providers.locality`, to be confirmed by a grep before implementation, since
that confirmation gates whether a fixed value is truly safe or whether one
more real caller needs checking).

Both inserts are idempotent (`ON CONFLICT (id) DO NOTHING`) and require no
migration, since both tables and their columns already exist. No console
change is needed for either: `ConnectionsPage.tsx`/`ModelsPage.tsx` already
mint a UUID for these fields; the backend now makes that UUID real instead
of dangling.

### 3.2 Console: expose the real no-auth binding id, and embedding publication

**No-auth binding projection.** `GovernedConnectionProjection` (and its HTTP
`ConnectionResponse`) gains `no_auth_binding_revision_id: Option<Uuid>`,
read from `no_auth_binding_revisions` (already populated automatically when
a Connection with `auth_mode: none` is created — confirmed live by P06's
evidence run reading it directly: `SELECT id FROM no_auth_binding_revisions
WHERE connection_id = ...` returned a real row). The console's
`GovernedConnectionItem` gains the same field, and `ConnectionsPage.tsx`'s
qualification request uses it instead of `crypto.randomUUID()` for
`target.binding_revision_id`.

**Embedding model publication.** `ModelsPage.tsx` gains a `Kind` selector
(`Chat` / `Embedding`, default `Chat`) in the model-registration form,
setting the already-supported `kind` field on `ModelRevisionRequest`. No
backend change: `create_model_revision` already accepts
`kind: ModelKind::Embedding`. Without this, qualification is permanently
unreachable through the console, since `QualificationRequest` requires both
a `chat_model_revision_id` and an `embedding_model_revision_id`.

`target_binding_id` (the top-level `QualificationRequest` field, distinct
from `target.binding_revision_id`) is a caller-invented tracking id, not a
reference to an existing row — P06's evidence did not flag it, and it needs
no change.

### 3.3 Qualification job executor (worker)

A new `poll_qualification_work` function, added to `worker.rs`'s existing
sequential poll chain (alongside `poll_embedding_work`), sharing its rhythm
(one pass per cycle, `sleep(500ms)` between cycles when nothing was found).

Per cycle: claim up to a batch of `qualification_jobs` rows in `requested`
or `running` state (`SELECT ... FOR UPDATE SKIP LOCKED`, or a lease column —
exact shape decided at plan time, following whichever of the two existing
patterns, `RunWorker`'s two-tier lease or `EmbeddingWorkerRuntime`'s
claim/finish, reads cleaner against this table's actual constraints). For
each claimed job: read which ordinals already have a row in
`qualification_probe_results`, take the first ordinal from the fixed,
database-enforced sequence
(`'00','10','15','20','30','35','40','50','60','70','80','90'` — this exact
array is hard-coded in both the table's `CHECK` constraint and
`vestrace_record_qualification_probe_result`'s own ordering guard, so the
worker does not need to invent or duplicate this sequence, only read it
once as a shared constant) that has no result yet, and call
`QualificationJobService::run_next_probe(context, job_id, ordinal, &runner)`
where `runner = PgQualificationProbeRunner::new(store.clone(),
governed.dispatch(), worker_id)` — the same dispatch graph
`GovernedProviderRuntime` already builds; no new HTTP client, no new
credential-decryption path. One ordinal per job per cycle, so one slow job
never starves the others. On a terminal `Succeeded` result, additionally
call `finalize_success`.

This is the one section of the design with a real implementation choice
deferred to plan time: the exact claim/lease SQL. Everything else here is a
direct, already-proven composition.

### 3.4 RunCoordinator: accepting governed confidential input — DEFERRED

**This section was scoped out while writing this plan's implementation
tasks, after deeper reading than the design phase's own research reached.**
The original plan (below, kept for the record) assumed constructing
`PrepareGovernedRunStepInput` — the command `GovernedRunStepInputAuthority`
accepts — was a matter of composing existing collaborators, the same way
§3.1-§3.3 are. It is not. `PrepareGovernedRunStepInput` has 15 fields, and
tracing every one against the current codebase (not just the one existing
test that builds it) found:

- **`connection_revision_id`/`connection_qualification_revision_id`/
  `model_revision_id`/`model_qualification_revision_id`** need to come from
  the `ModelBindingSnapshot` already pinned for the run at creation time —
  but no repository method reads an already-pinned snapshot back by
  `run_id`. `ModelBindingResolver::resolve_for_run_in` only *creates* a new
  pin (called once, at Run creation, from `run_command_committer.rs:165`);
  calling it again would mint a second one. The one existing test
  (`run_acceptance_binding_race.rs`) gets these values via raw SQL against
  `run_model_binding_snapshots`/`model_binding_snapshots`, not through any
  application port. A new read method is real but mechanical work.
- **`sampling: EffectiveSampling`** (temperature/top_p) has *zero*
  production call sites anywhere in this codebase — only test code
  constructs one, with an arbitrary test value (`0.2, 0.9`). There is no
  established default to reuse; choosing one is a policy decision, not
  plumbing.
- **`intent: ExternalEffectIntent`** — its constructor and validation are
  used in production (`POST /v1/effects`), but there, the caller's own HTTP
  request body supplies `adapter`/`operation`/`target`/
  `normalized_arguments_digest`/`expected_effect`/`preconditions` directly.
  `RunCoordinator::add_steps` has no analogous source for any of these six
  values for an AG-UI-originated chat message. The one existing test
  invents literal strings (a fake provider adapter name, a fake target
  URL, a fake digest) that exist only to satisfy the constructor's
  validation, not to mean anything real.

In short: `GovernedRunStepInputAuthority`/`GovernedRunStepInputReservation`
is real, tested library code, but it was built as a *separate* execution
model from the one that actually runs today's real (non-confidential) Run
steps (`crates/vestrace-application/src/run/model_step.rs`'s
`GovernedProviderStepExecutor`, already wired into the worker). Composing
it requires deciding, for the first time, what an AG-UI chat message's
sampling parameters and effect-intent identity should be — a design
question, not a wiring task. This belongs in its own brainstorming session
and its own package, once this one's evidence (§5) shows the rest of the
system is ready to receive it.

The original (superseded) design is kept below for that future package's
reference:

> `RunCoordinator<S, C, Q, L>` would gain a field
> `input_authority: Option<Arc<dyn GovernedRunStepInputAuthority>>` and a
> builder method `.with_input_authority(...)`, following the same idiom
> `AppState`'s existing `.with_*` builders already use — additive, not a
> breaking change to `RunCoordinator`'s public shape. In `add_steps`, a
> single-step batch with `AgentSnapshot` + `Confidential` input and a
> configured authority would delegate to `authority.accept(...)` and return
> its `outcome.run` directly, bypassing `RunCoordinator`'s own
> `RunStorePort`-based step creation for that one step. In `server.rs`,
> `GovernedRunStepInputReservation` would be constructed from `store`,
> `governed.dispatch()`, the material vault, and the handful of
> `Pg*Repository` types already visible in that function (the sole existing
> test that constructs it, `run_acceptance_binding_race.rs`, is a worked
> example) and passed in via `.with_input_authority(...)`.

## 4. Non-goals

- **Composing `GovernedRunStepInputAuthority` into the server (gap 4).**
  Deferred per §3.4 — this needs real design work (sampling defaults,
  effect-intent derivation for a chat message, a new pinned-snapshot read
  port), discovered while writing this plan's implementation tasks, not a
  composition this package can respond to.
- **Real credential material for the credentialed branch.** Operator
  decision: stays out of scope. The credentialed branch remains documented
  as blocked (on top of this package's four fixes) until a separate,
  security-material-focused package addresses it.
- **Full Connector/Provider CRUD or console selectors for them.** Rejected
  in §2; auto-materialization removes the need.
- **Removing or relaxing the `connector_id`/`provider_id` foreign keys.**
  Rejected in §2.
- **Mixed-batch governed Run steps** (a confidential step alongside
  ordinary ones in one `AddRunSteps` call). Nothing in this system produces
  one; not handled.
- **Agent system-prompt injection into model calls.** Already out of scope
  since P06; unchanged here.
- **Changing the fixed q1 ordinal sequence or its SQL-side ordering
  guarantees.** The worker reads the existing sequence; it does not modify
  the qualification protocol itself.

## 5. Testing and evidence plan

**Unit/integration (Rust):**
- §3.1: Connection/Model-revision creation succeeds on a fresh workspace
  with zero pre-existing `connectors`/`providers` rows; a second creation
  reusing the same connector/provider id is a no-op, not a conflict.
- §3.2: `GovernedConnectionProjection` correctly carries
  `no_auth_binding_revision_id` for a Connection created with `auth_mode:
  none`.
- §3.3: the worker claims a `requested` job, advances it across several
  poll cycles (one ordinal per cycle) using a fake `QualificationQ1Adapter`,
  and reaches a terminal state; two concurrent worker instances do not
  double-process the same job (lease exclusivity).

**Live evidence (mirrors P06's Task 8, this time reaching further, but
still honestly short of a full pass):** a full Playwright walkthrough —
create a Connection through the console form on a fresh workspace, publish
both a chat and an embedding Model revision through the console form,
request qualification with the real `no_auth_binding_revision_id`, and
observe the job reach `qualified` through the new worker (this involves
real wall-clock time for real network probes — the evidence run should
record how long). Set the workspace default. Then attempt an AG-UI message
and record the *current, expected* outcome: it still refuses (gap 4 is
deferred), and the evidence doc should say so plainly rather than silently
stopping short of that check. A full restart cycle (`docker compose down`,
volume preserved, `up`) confirms the qualified state, the workspace
default, and any in-flight or completed qualification job data all survive.
The credentialed branch is not attempted; it remains blocked on credential
material (§4) on top of everything else.
