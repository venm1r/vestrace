# P06 (G1): Real Connections, Models, Agents, Runs, Settings and Model-Backed Execution — Design

**Status:** proposed
**Package:** P06 / gate G1, per `docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md`
**Predecessor:** P05 (accepted, evidenced)
**Exit criteria (from the master roadmap):** browser plus restart evidence for both auth branches, and real model-backed Runs.

## 1. Goal

Make the console's Connections, Models, Agents, Runs and Settings surfaces do
what they visually claim to do. Today a user can click "Create run," type a
message into "Start a separate AG-UI run," and the UI responds as if
something happened — but no model is ever called. This package closes that
gap end to end: a real Connection, bound to a real Model, set as the
workspace's default, driving a real chat completion through both LM Studio
(local) and a remote OpenAI-compatible provider, proven by a browser session
and surviving a full restart cycle.

## 2. Current state (verified by reading the code and driving it live)

This section is load-bearing: every claim below was confirmed either by
reading the exact source line or by exercising the running stack through a
browser. It is not a summary of intentions.

**Connections.** The backend already has a full governed contract:
`POST /v1/connections`, `POST /v1/connections/{id}/revisions`,
`POST /v1/connections/{id}/admission-policies`,
`POST /v1/connections/{id}/qualifications` (`crates/vestrace-http/src/api/connections.rs`).
The console's `ConnectionsPage.tsx` uses none of it: "Add Connection" opens a
modal explaining that connections are configured through environment
variables, and "Test Connection" calls `notify('info', 'Connection tests are
not implemented in this build...')` unconditionally. There is no creation
form and no qualification call.

**Models.** The backend has two routes: a legacy `POST /models`
(`CreateModelRequest { provider_id: Uuid, ... }`, explicitly commented as
retained only for compatibility) and the real governed
`POST /models/{id}/revisions` (`ModelRevisionRequest`), which already carries
`connection_id`, `connection_revision_id` and `wire_model_id` — i.e. the
backend already models "a Model revision runs against this specific
Connection revision," which is the union of Model and Connection the design
question in this package's brainstorming asked about. `ModelsPage.tsx` calls
the legacy route and lets an operator type a free-text provider name, which
the server rejects: "Select an existing provider; the legacy provider
registry is retired." The console has no UI for the governed route or for
binding a Connection.

There is also a workspace-level "default model" concept:
`ModelRevisionRepository::set_workspace_default_governed(SetWorkspaceModelDefault)`
is fully implemented against Postgres
(`crates/vestrace-infrastructure/src/postgres/model_revision_repository.rs`),
but **no HTTP route calls it**. `grep` across `crates/vestrace-http/src/api`
confirms this. Without a route, an operator has no way to set which Model a
Run should use.

**Agents.** `AgentsPage.tsx` is fully functional today — registering an
agent (name, description, system prompt) against the real backend succeeds
and persists. However, tracing the Run execution path
(`CreateRun.coordinator_snapshot_id`, `RunActorRef::AgentSnapshot(..)` in
`crates/vestrace-http/src/api/runs.rs:126` and `:396`) shows both call sites
mint a **fresh random `AgentRuntimeSnapshotId::new()`** — this id is never
looked up against a registered Agent. `crates/vestrace-application/src/run/model_step.rs`
confirms the executor's own design intent: "its route is the Run's pinned
`ModelBindingSnapshot` ... this executor has no routing authority of its own."
A registered Agent's system prompt is not part of today's execution path at
all; the `AgentRuntimeSnapshotId` on a step is only a marker meaning "this
step invokes a model," not a foreign key to anything. This is a real,
pre-existing gap, and this package deliberately does not close it (see
Non-goals).

**Runs and model execution.** "Create run" in `RunsPage.tsx` calls the real
`POST /v1/runs`, which persists a run. `AddRunSteps`
(`crates/vestrace-http/src/api/runs.rs:318`) is "what makes a run executable"
per its own doc comment — it is real and already wired to
`state.run_orchestrator()`. The worker (`crates/vestrace-cli/src/commands/worker.rs:157`)
already installs a real `GovernedProviderStepExecutor` via
`.with_model_executor(...)`. When a Run is created,
`crates/vestrace-infrastructure/src/postgres/run_command_committer.rs:165`
already calls `ModelBindingResolver::resolve_for_run_in`, which (for the
legacy Run path) pins a `ModelBindingSnapshot` to the workspace's default
Model for `purpose = "chat"`
(`LEGACY_RUN_MODEL_DEFAULT_PURPOSE`, `crates/vestrace-application/src/models/bindings.rs`).
**All of this machinery is real and already composed** — the missing piece
is that nothing lets an operator set that default (see Models, above), and
nothing calls any of it from a chat-style entry point.

**AG-UI run execution.** `POST /ag-ui/run` (`run_agent` in
`crates/vestrace-http/src/api/ag_ui.rs:124`) is a hard-coded stub: both its
`State` and `Json<RunAgentRequest>` parameters are unused, and it
unconditionally returns `governed_run_input_required`. Its own module doc
says why: "confidential agent input must first be accepted by the governed
Run authority, which is not composed here." That authority is exactly the
`RunOrchestrator` + `ConfidentialRunInput` + `AddRunSteps` path already used
by `runs.rs` — it exists, it is just never called from this handler. The
console's `CompactChat.tsx` already sends `{ message }` only (deliberately —
"the backend creates a separate AG-UI run and ignores a selected console run
id") and expects `{ run_id, status, message }` back, then opens
`GET /ag-ui/events/stream?run_id=...`, which is also already real (polls
`ag_ui_repository().events_since(...)`).

**Settings.** `SettingsPage.tsx` already has a working kernel-limits form
(`max_concurrent_runs`, `run_budget_cap_micros`) with real optimistic
concurrency (`version`), and a read-only Environment tab reporting the
database role and migration compatibility. It has no model-related section
yet; this package adds one (the workspace default model, once route B below
exists).

**Both auth branches.** `ModelBindingSnapshot`'s `branch` column already
distinguishes a `"credential"`-backed binding from a
`no_auth_binding_revision_id`-backed one
(`crates/vestrace-infrastructure/src/postgres/model_binding_repository.rs`).
The roadmap's "both auth branches" phrase refers to this: proving one
model-backed Run through a Connection that carries a real credential
(remote OpenAI-compatible provider, `auth_mode` set), and one through a
no-auth Connection (LM Studio at `http://localhost:12345/v1`, which needs no
key).

## 3. Non-goals

- **Agent personality / system-prompt injection into model calls.** Wiring a
  registered Agent's system prompt into the request sent to a model is a
  real gap (see above) but belongs to the interaction/context-kernel work
  scoped for P07 ("interaction/thread/message/state/context kernel"), not
  here. For this package, an AG-UI run's only input is the operator's raw
  message; "Agents" stays organizational metadata plus the existing
  registration CRUD.
- **Run approval workflows.** `RunExecutionMode::Supervised` is stored today
  but nothing gates dispatch on it; this package does not add an approval
  step. AG-UI runs are created the same way `runs.rs::create_run` already
  creates them.
- **Rewriting the AG-UI event protocol.** The event stream, its polling
  interval, and its payload shape are unchanged.
- **A general "which agent should this chat use" selector.** See §4.3 —
  resolved by relying on the workspace default model, not by extending the
  AG-UI request shape.
- **Changing the legacy `POST /models` / `POST /connections` `CreateModel`
  request contracts.** They stay as compatibility routes; the console simply
  stops calling them from these two pages.

## 4. Design

### 4.1 Backend: expose the workspace default model

**Approach chosen:** add `POST /v1/models/{id}/default`, following the exact
shape `create_model_revision` already uses (a governed mutation with a
required idempotency key, an `expected_version` for optimistic concurrency
against the default pointer, and a `GovernedMutationResponse` reply). The
handler builds a `SetWorkspaceModelDefault` and calls the already-implemented
`state.model_revision_repository()?.set_workspace_default_governed(...)`.

*Alternative considered:* fold "set default" into the Model creation call
itself (a `make_default: bool` flag on `ModelRevisionRequest`). Rejected —
`SetWorkspaceModelDefault` is independently versioned from the Model revision
head specifically so that re-pointing the default doesn't require publishing
a new revision; collapsing them into one call would either lose that
independence or force every model creation to decide the workspace's default,
which isn't its concern.

Request body: `{ default_id, purpose, model_id, required_capabilities,
expected_version }`. For P06, the console always sends
`purpose: "chat"` (the same constant the resolver already reads); the field
stays in the wire contract because the backend command already carries it
generally.

### 4.2 Console: Connections page — real creation and testing

Replace the "configured via env vars" modal with a form that calls
`vestraceClient.createConnection` then `createConnectionRevision` (both
already exist in `sdk/client.ts` with `GovernedMutationOptions`). Fields
mirror `ConnectionRevisionRequest`: kind (LM Studio / remote OpenAI-compatible
as a `logical_base_url` + `runtime_base_url` pair), `auth_mode` (none vs
bearer-token), and, when `auth_mode` is not none, a credential slot created
via the existing `createConnectionCredential` /
`activateConnectionCredential` SDK methods.

"Test Connection" changes from a hardcoded notice to a real call to
`requestConnectionQualification` (already in the SDK), then polls the
returned qualification job the same way `ModelsPage.tsx`'s "Test Connection"
button will (§4.4 job polling is shared, not duplicated — see §5).

### 4.3 Console: Models page — governed creation bound to a Connection

Replace the legacy `createModel` call with: pick an existing qualified
Connection (dropdown sourced from `listConnections`), then call
`createModelRevision` with that Connection's id and current revision id, plus
`wire_model_id` (the model name to send in requests — e.g.
`ternary-bonsai-27b` for LM Studio). This directly implements the "Model
references Connection directly" decision already made during brainstorming
— which this investigation confirms the backend has supported all along.

Add a "Set as default for chat" action per qualified Model row, calling the
new route from §4.1. Show the current default (if any) at the top of the
page, sourced from a new `getWorkspaceDefaultModel`-style read — reusing
`list_safe_models`'s existing projection is not enough since it doesn't
mark which one is default, so the read comes from the same settings/health
surface described in §4.5.

**This is also how the AG-UI "which agent/model" question resolves:**
because `ModelBindingResolver::resolve_for_run_in` already pins the
workspace's `purpose = "chat"` default to every Run created through the
legacy path — and `run_agent` will create Runs through that exact path (§4.6)
— there is no need to extend `RunAgentRequest` with a model or agent
selector. Setting the default in Models is the entire "which model does chat
use" UI. This was the natural resolution surfaced by tracing the existing
code rather than inventing a new selection mechanism.

### 4.4 Shared: qualification job polling

Both Connections' "Test Connection" and Models' "Test Connection" need to
show a qualification job's outcome (queued → running → qualified/blocked).
Factor a small `useQualificationJob(jobId)` hook in
`apps/console/src/sdk/` that polls `GET` on the job resource at a fixed
interval and stops on a terminal state, so neither page reimplements
polling.

### 4.5 Console: Settings — surface the workspace default model

Add a fourth Settings tab, "Models," showing the current `purpose: "chat"`
default (model name, Connection, qualification state) as a read-only summary
with a link to the Models page to change it. Settings is the natural home
for this because it is already the page that reports workspace-scoped,
single-value configuration (`max_concurrent_runs`, `run_budget_cap_micros`);
the default model is exactly that shape of fact.

### 4.6 Backend: implement `run_agent`

Replace the stub in `crates/vestrace-http/src/api/ag_ui.rs` with:

1. Validate `request.message` is non-blank — reuse
   `ConfidentialRunInput::parse`, which already enforces "not blank" and the
   32 KiB ceiling, so no separate check is needed.
2. If `request.run_id` is absent: call
   `state.run_orchestrator()?.create_run(&context, CreateRun { objective:
   "AG-UI run".to_owned(), coordinator_snapshot_id:
   AgentRuntimeSnapshotId::new(), execution_mode: RunExecutionMode::Supervised,
   parent: None, correlation_id: None, idempotency_key: <fresh uuid> })`
   — identical shape to `runs.rs::create_run`, so this run behaves exactly
   like one created through the console's own "Create run" button. If
   `request.run_id` is present, look up its current version via
   `state.run_use_cases().get_run(...)` instead of creating a new run.
3. Call `state.run_orchestrator()?.add_steps(&context, AddRunSteps { run_id,
   expected_version, steps: vec![NewRunStepDto { id: RunStepId::new(),
   plan_step_reference: None, assigned_actor:
   RunActorRef::AgentSnapshot(AgentRuntimeSnapshotId::new()),
   input_references: vec![], input: NewRunStepInput::Confidential(
   ConfidentialRunInput::parse(request.message)?) }], actor:
   RunActorRef::Principal(context.principal_id), idempotency_key: <fresh
   uuid> })`.
4. Return `RunAgentResponse { run_id, status: "accepted", message: "run
   accepted".to_owned() }`. `thread_id` stays accepted-and-ignored exactly as
   today — the existing unit test
   (`thread_and_run_identifiers_are_accepted_without_being_echoed`) keeps
   passing unmodified.

This makes `run_agent` a thin translation from "one chat message" to the same
`AddRunSteps` call the console's Run detail page already issues by hand. The
worker picks the step up through the existing queue; nothing new is needed
on the worker side. This is the direct answer to the brainstorming question
about how the worker learns about a new AG-UI-originated run — it learns
about it the same way it learns about every other run's steps: through the
existing work-item queue that `add_steps` already writes to.

*Alternative considered:* give `run_agent` its own bypass path that calls
the provider directly, skipping the Run/Step/worker machinery, since AG-UI
runs are meant to be simple. Rejected — that would create a second execution
path with its own policy/audit/data-sensitivity handling to maintain
alongside the one `execute_step.rs` already has, duplicating exactly the
kind of routing authority `model_step.rs`'s own doc comment says this
executor must not have.

### 4.7 Error handling

- `run_agent` surfaces `ApplicationError` from `create_run`/`add_steps`
  through the existing `ApiError::from_application` mapping (same as every
  other `runs.rs` handler) — no new error shapes.
- If no workspace default model is set for `purpose: "chat"`,
  `ModelBindingResolver::resolve_for_run_in`'s underlying
  `vestrace_create_run_model_binding_snapshot` SQL function already fails
  closed (confirmed by its existing test suite,
  `crates/vestrace-infrastructure/tests/model_binding_snapshot.rs`); that
  failure surfaces through `create_run`'s existing error path, so
  `run_agent` needs no bespoke "no default configured" check — the
  underlying commit already refuses, and the console's chat widget will show
  whatever message that produces. If that message is not
  operator-legible when this is implemented, add a friendlier wrap at the
  `run_agent` call site — the plan should record this as a task
  contingent on what the raw error looks like once observed.
- Connections/Models UI: qualification failures render through the existing
  `describeError`/`NoticeBanner` pattern already used elsewhere in the
  console (`SettingsPage.tsx` is the reference implementation).

### 4.8 Testing and evidence plan

Per the gate program's exit criteria ("browser plus restart evidence for
both auth branches and real model-backed Runs"):

1. **Unit/integration (Rust):** new tests for the `POST /models/{id}/default`
   route (success, version conflict, unknown model); new tests for
   `run_agent` (creates a run when `run_id` absent, adds a step to an
   existing run when present, rejects blank message, surfaces the
   no-default-configured failure) replacing the current
   `ag_ui_run_reports_unavailable_when_no_orchestrator_is_configured` test's
   assumption (that test's premise — "no orchestrator is configured" — no
   longer holds once one is wired in; it must be rewritten to match the new
   behavior, not deleted silently).
2. **Browser evidence, no-auth branch:** through Playwright, create an LM
   Studio Connection (no credential), qualify it, create a Model revision
   against it (`ternary-bonsai-27b`), set it as the chat default, then send
   a message through the console's AG-UI chat widget and observe a real
   completion arrive over the event stream.
3. **Browser evidence, credentialed branch:** same sequence against a real
   remote OpenAI-compatible provider with a bearer credential through the
   credential-slot flow.
4. **Restart evidence:** after both branches are proven, run a full
   `docker compose down` (volumes preserved) + `up` cycle and repeat one
   AG-UI message against the already-configured default model, proving the
   pinned `ModelBindingSnapshot`/default survive a restart — this is the
   same restart-fragility class of bug this session already found and fixed
   three instances of in P05's migration chain, so it is deliberately
   exercised again here rather than assumed.
5. Every new file this package creates follows the standing
   `scripts/p05-scope.mjs`-and-preflight rule already in force for this
   repo's evidence discipline.

## 5. Rough task shape (for the implementation plan, not final)

This is not a task breakdown — `writing-plans` will produce that — but the
natural seams observed while researching this design are:

1. Backend: `POST /models/{id}/default` route + tests.
2. Backend: real `run_agent` implementation + rewritten AG-UI router
   contract test + new unit tests.
3. Console: shared qualification-job-polling hook.
4. Console: Connections page real creation + qualification wiring.
5. Console: Models page rewired to governed revision creation + "set
   default" action.
6. Console: Settings "Models" tab.
7. Browser + restart evidence for both auth branches (closing evidence, run
   last, against the finished stack).

## 6. Open questions for plan time

- Exact wire shape of `getWorkspaceDefaultModel` (a new read endpoint, or a
  field added to an existing settings/health response) — left for the plan
  to settle against whichever is less invasive once the console's current
  settings-fetch shape is read in full.
- Whether the remote OpenAI-compatible provider used for the credentialed
  branch's evidence is a real third-party endpoint or a local stand-in
  (e.g., a second LM Studio-compatible server configured to require a
  bearer token) — operationally decided at evidence-gathering time, not a
  design fork.
