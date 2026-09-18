# P06 (G1) — Real Execution Evidence: Closed as Blocked

**Package:** P06 (G1), per `docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md`
**Plan:** `docs/superpowers/plans/2026-09-18-vestrace-v1-p06-real-execution.md`
**Spec:** `docs/superpowers/specs/2026-09-18-vestrace-v1-p06-real-execution-design.md`
**Exit criteria (master roadmap):** browser plus restart evidence for both auth branches and real model-backed Runs.

**Verdict: BLOCKED, not PASS.** Recorded honestly, following the same
convention this program has already used elsewhere (e.g. P05-H closing
g0-10/g0-14/g0-15 as blocked rather than claiming a pass they could not
prove). Tasks 1-7 of the implementation plan are complete, individually
reviewed, and pushed — each does exactly what its task-scoped review
verified. Task 8's job was to prove the whole system produces a real model
completion end to end, through a real browser, for both a no-auth and a
credentialed Connection, surviving a restart. That could not be proven,
for reasons independent of Tasks 1-7's own correctness.

## What is proven (real, live, independently verified)

**Restart durability — PASS.** Every piece of state this package's own work
created (a Connection, a Connection revision, two Model revisions, the
workspace's chat-default pointer) survived a genuine `docker compose down`
(volume preserved, no `-v`) + `up` byte-identically. (The connector and
provider rows underneath this state were seeded directly via SQL — the same
mechanism this repository's own test suites use — and the Connection and
both Model revisions were themselves created via direct API calls rather
than through the console's own forms, per Blockers One and Two; see
`p06-no-auth-run-test.txt`, Blockers One and Two, for the full account.)
The full migration chain replayed cleanly with no manual intervention, and
no restart-fragility symptom of any kind appeared. See
`docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt`, section 5.

**Docker networking and LM Studio reachability — PASS.** `host.docker.internal`
resolves correctly from inside both `vestrace-server` and `vestrace-worker`
(the compose file already declares the needed `extra_hosts` entry); a real
chat completion was independently obtained by curling LM Studio directly from
inside the server container, proving the model host itself is genuinely
reachable and working. Same file, section 0.

**Every task's own reviewed behavior — confirmed live, not just at the unit
level.** The Connections and Models pages render and submit exactly as
designed (including a browser-cache artifact the controller found and
corrected during this evidence pass — see the correction inline in
`p06-no-auth-run-test.txt`'s Blocker Two section: Task 6's real form was
initially mis-captured against a stale cached bundle; re-verified fresh, it
is correct). `POST /v1/models/{id}/default` and `GET /v1/models/default`
work and persist through a restart. `POST /ag-ui/run` genuinely attempts to
reach the orchestrator rather than refusing unconditionally, exactly as
Task 2 built it.

## What is not proven, and why (four independent root causes)

None of these are defects in Tasks 1-7's own code — each is a pre-existing
gap in what the *rest* of the system composes, discovered because this task
is the first one in this package to actually run the whole stack end to end
rather than testing one layer at a time.

1. **No Connection can be created on a fresh workspace.**
   `connections.connector_id` is a real foreign key to `connectors(id)`
   (`migrations/*`, confirmed live via `\d connectors`). No HTTP route of any
   method creates a connector (`grep -n connector crates/vestrace-http/src/route_inventory.rs`
   returns nothing), and the console mints a random UUID for it
   (`ConnectionsPage.tsx`). Every existing test suite seeds this row directly
   via SQL — there is no operator-facing path. Observed live:
   `POST /v1/connections` → HTTP 500, `connections_connector_id_fkey`
   violation. See `p06-no-auth-run-test.txt`, Blocker One.

2. **No Model can be registered on a fresh workspace**, for the identical
   reason one table over: `models.provider_id` FKs to `providers(id)`
   (`migrations/0014_provider_and_model_registry.sql:20`); `POST /v1/providers`
   is permanently and deliberately refused (`legacy_provider_registry_retired`)
   with no replacement route; `ModelsPage.tsx` mints a random UUID for it.
   Independently re-verified live by the controller against Task 6's actual,
   correctly-rendering form (not the stale-bundle capture): submitting the
   real form still fails `HTTP 500` on the same FK-violation class of error.
   See `p06-no-auth-run-test.txt`, Blocker Two (including the controller's
   correction).

3. **No production binary ever executes a qualification job.**
   `QualificationJobService::run_next_probe`
   (`crates/vestrace-application/src/provider_qualification.rs:147`) is the
   only code path that runs a q1 probe; every caller of it is an integration
   test. The worker's only qualification-related code is a startup one-shot
   conformance-manifest check
   (`crates/vestrace-cli/src/commands/worker.rs:75-82`), not a poller over
   `qualification_jobs`, and it is not enabled in `docker-compose.yml`.
   A qualification job requested live sat in `requested` for 60 seconds of
   polling with zero probe attempts recorded. Consequence: every Connection
   and Model in this build is permanently `blocked`/`qualification_required`,
   so the governed dispatcher can never admit a call regardless of anything
   else being fixed. See `p06-no-auth-run-test.txt`, Blocker Three.

4. **`POST /ag-ui/run` cannot execute a step even when everything else is
   granted.** `crates/vestrace-cli/src/commands/server.rs:280` wires the
   plain `RunCoordinator` as the only production `RunOrchestrator`, and its
   `add_steps` explicitly refuses any `RunActorRef::AgentSnapshot` step
   carrying `NewRunStepInput::Confidential`
   (`crates/vestrace-application/src/run/coordinator.rs:249-256`) —
   returning exactly the `governed_run_input_required`-successor message
   observed live: `HTTP 503 governed Run-step input authority is not
   configured`. The real authority that would accept it,
   `GovernedRunStepInputReservation` (the sole impl of
   `GovernedRunStepInputAuthority`), is never constructed in any binary —
   `server.rs:65` even discards the governed dispatch handle it builds
   (`let _governed_dispatch = governed.dispatch();`). Task 2's own unit tests
   pass because they install a test double in place of the real coordinator,
   so this refusal is never exercised by them. This blocker alone is
   sufficient to block a real completion through AG-UI regardless of
   Connections, Models, or qualification. See `p06-no-auth-run-test.txt`,
   Blocker Four.

5. **Credentialed branch only: no real credential material exists, and
   nothing creates it.** Beyond inheriting all four blockers above unchanged,
   the only credential "material" producible in this build is a test-fixture
   placeholder byte string (`b"credential-fixture-ciphertext"`), not a usable
   bearer token, and no HTTP route accepts real secret bytes at all. See
   `docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt`.

## What would actually prove this, if the four gaps were closed

A Playwright transcript showing a Connection reaching `qualified`, a Model
reaching a non-blocked state, the workspace default set, and an
`/ag-ui/events/stream` capture containing the model's real completion text.
None of that is asserted here — no such transcript exists, because none of
the four prerequisites for it exist yet.

## Evidence files

| File | SHA-256 | Bytes |
|---|---|---|
| `docs/development-evidence/v1-g0-05-gate/p06-no-auth-run-test.txt` | `8396c06d9c69b381c3bd535dc5b48cc263921d799c0e2f374c005362c950bdb1` | 33110 |
| `docs/development-evidence/v1-g0-05-gate/p06-credentialed-run-test.txt` | `8eac3e728bc44d436b885f1ab0993431339603c5a37a570639d397720f7452a0` | 5665 |
| `docs/development-evidence/v1-g0-05-gate/p06-screenshots/p06-connections-create-failure.png` | `551a03b7d1677af3c1e851a941a366b3389128b6b7a35692d7e5ed85f7c38231` | 125242 |
| `docs/development-evidence/v1-g0-05-gate/p06-screenshots/p06-models-register-blocked.png` | `170ee24a37ab7664f3687bf4b574b842510ac861ca87c4a9c883b0b815acf20e` | 101716 |
| `docs/development-evidence/v1-g0-05-gate/p06-screenshots/p06-credentialed-create-failure.png` | `c30a5f9b36b7d241d1d64f46e9a9a72ad7846695d727593a1097b68de9488543` | 138892 |
| `docs/development-evidence/v1-g0-05-gate/p06-screenshots/p06-settings-default-model-pre-restart.png` | `26af883fa7424641794b397dbcc1d99066d954cc29967ee014fdc6d7a2fd2787` | 87163 |
| `docs/development-evidence/v1-g0-05-gate/p06-screenshots/p06-settings-default-model-post-restart.png` | `26af883fa7424641794b397dbcc1d99066d954cc29967ee014fdc6d7a2fd2787` | 87163 |

(The last two are byte-identical — the workspace default's browser rendering
was pixel-identical before and after restart, corroborating the curl-based
proof of byte-identical API state above, not a duplication error.)

## Disposition

Per the operator's decision: close P06 with this evidence rather than expand
its scope now. The four backend gaps above (connector creation, provider
creation or an equivalent FK relaxation, a real qualification-job executor,
and composing `GovernedRunStepInputAuthority` into the server) are
substantial enough — each plausibly its own design — to warrant their own
brainstorming and plan as a follow-up package, rather than being folded into
this one as "one more task."
