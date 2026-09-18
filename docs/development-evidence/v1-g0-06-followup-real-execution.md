# P06 Follow-Up — Real Execution Evidence: Closed with Two Gaps Fully Closed and the Third Blocked Out of Scope

**Package:** P06 follow-up, per `docs/superpowers/plans/2026-08-26-vestrace-v1-gate-program.md`
**Plan:** `docs/superpowers/plans/2026-09-18-vestrace-v1-p06-followup-real-execution.md`
**Spec:** `docs/superpowers/specs/2026-09-18-vestrace-v1-p06-followup-real-execution-design.md`
**Predecessor:** `docs/development-evidence/v1-g0-06-real-execution.md` (P06, closed BLOCKED)
**Plan goal:** "make a Connection and its Models reach a real `qualified` state
end to end on a fresh workspace, surviving a restart."

**Verdict: the plan's headline end-to-end claim is NOT MET. No Connection or
Model reached `qualified` in any run of this package, and nothing in this
document should be read as claiming one did.**

What this package did deliver, stated at the size it actually is: of the three
gaps it set out to close, **two are fully closed with live browser evidence,
and the third's infrastructure is real, deployed and proven for the first time
in this codebase's history** — but a qualification job still cannot complete,
blocked by a pre-existing defect in the worker's identity and authorization
model that is *not* part of what this plan built, was found by this package
rather than introduced by it, and has been explicitly scoped out and deferred
to a separate future package. A fourth gap (AG-UI) was deferred during this
plan's own design phase and remains exactly where P06 left it.

That is a partial result, recorded as one. It is not rounded up to a pass, and
the two gaps that genuinely closed are not undersold to make the summary
tidier.

## How this evidence was produced

Three passes, all against the same real Compose deployment and the same
Postgres volume, all recorded in
`docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt`:

1. **Task 7, first attempt — BLOCKED outright.** The whole stack could not
   boot: migration 0217, which Task 1 introduced, had no route into any
   deployed database.
2. **Task 8 — the corrective task.** Gave 0217 a real deployment route (a
   SECURITY DEFINER installer plus a new Compose migration stage), verified end
   to end. Sections 0-9 of the transcript are Task 7's re-run on top of it.
3. **Task 9 plus this verification pass.** Task 7's re-run found a third
   console-layer defect (Blocker C, below). Task 9 fixed it; this pass rebuilt
   the stack and verified the fix live. Section 10 of the transcript.

## What is proven (real, live, independently verified)

**Migration 0217 is genuinely deployed — PASS.** The Compose stack booted
against a volume holding all 147 migrations up to version 217, all `success`,
and replayed the full chain — including Task 8's new 0217 stage — to exit 0
with no manual intervention **three separate times**: on first boot, across the
restart cycle, and again during this verification pass's rebuild. The blocker
that ended the first evidence attempt is resolved, and its fix is durable
rather than a one-off.

**Gap 1 — connector auto-materialization — CLOSED.** On a workspace verified to
hold zero `connectors` rows, a Connection was created through the console's own
form, in a real browser, and succeeded. The `connectors` row
(`77acd3f1-b5a8-4f7a-beb5-d972be2216f1`, `provider_type=local`) was created
transactionally alongside it by Task 3's `insert_stable_connection`. This is the
first time in this program that a Connection has been created on a fresh
workspace through the operator-facing UI. Transcript section 2.

**Gap 2 — provider auto-materialization — CLOSED, including the console path.**
Task 7's re-run proved the SQL: two `providers` rows were auto-materialized by
Task 3's `insert_or_verify_stable_model` on a workspace that had zero, killing
the FK violation P06 recorded as Blocker Two. But at that point the Models had
to be published by direct API call, because the console form could not complete
(Blocker C). After Task 9's fix, **two further Models were registered entirely
through the console form** — one chat-kind, one embedding-kind — and each
auto-materialized its own `providers` row on the way
(`c9bfb46e-baca-415d-ab1a-53515606a0d0` and
`48094c9b-a348-4617-a819-a0cb6e9cbf66`; the table went from 2 rows to 4). The
gap is now closed on the operator-facing path, not only in the repository
layer. Transcript sections 3.4 and 10.5.3.

**Blocker C — console Model registration — FIXED and verified live.** Task 7's
re-run found that `migrations/0178_model_revisions_and_binding_snapshots.sql`
requires a model revision's `execution_guard_id` to be *exactly* its connection
revision's guard, while `ModelsPage.tsx` minted a fresh `crypto.randomUUID()`
for it and no read route exposed the real value — so every console registration
died with HTTP 500 / `23514`,
`model revision requires its exact referenced connection guard`. Task 9 added
`execution_guard_id` to the `GET /v1/connections` projection and made the
console send it.

Verified in this pass with a before-and-after on the same command and the same
connection. Against the still-running old binary, `GET /v1/connections`
returned no `execution_guard_id` key at all. Against the rebuilt binary it
returned `"execution_guard_id":"72ba298d-04f1-427d-930d-394816684b5d"` —
byte-identical to the durable `connection_revisions` row. Both console
registrations then returned **HTTP 201 Created**, carrying that real guard id
rather than a random UUID, and the server logged **zero** `ERROR` lines and no
occurrence of `23514` or the guard message across the whole container lifetime.
The served bundle hash changed (`index-E50tngQU.js` → `index-DZnxtKDo.js`) and
was checked against a cache-bypassing fetch, so this is not a stale-bundle
artifact. Transcript section 10.

**Task 4 — PASS.** `GET /v1/connections` returns a real
`no_auth_binding_revision_id` (`01a0b549-34af-7050-bcc0-b6f2ef555173`), verified
identical to the durable `no_auth_binding_revisions` row. Section 4.1.

**Task 5 — PASS.** The console's "Test Connection" button requested a
qualification job whose pinned `qualification_target_bindings` row carries
exactly that real binding id and both real Model revision ids — not a
`crypto.randomUUID()`. Section 4.2.

**Task 6 — PASS.** Task 7's re-run could only record this as partial: the Kind
selector existed in the served bundle and serialized `"kind":"embedding"`
correctly, but the request it produced was refused by Blocker C. With Blocker C
fixed, an embedding-kind Model registered through that selector returns HTTP
201 and persists with `kind=embedding`. Sections 3.3 and 10.5.2.

**Gap 3 — the qualification executor genuinely runs, for the first time in this
codebase's history.** P06's Blocker Three was "a qualification job requested
live sat in `requested` for 60 seconds of polling with zero probe attempts
recorded." That is no longer true. 86 milliseconds after the browser click,
Task 1's `qualification_job_work_claims` table held a live 60-second lease owned
by the worker's own `WorkerId`, the job had moved to `running`, and Task 2's
poll loop had driven q1 ordinal `00` to `pass`. Section 5.1.

**Restart durability — PASS, twice.** Every piece of state the evidence run
created — the Connection and its auto-materialized connector, the Model
revisions and their auto-materialized providers, the workspace chat-default
pointer, the qualification job, its probe result and its work-claim row —
survived a genuine `docker compose down` (volume preserved, no `-v`) + `up`.
`GET /v1/connections` and `GET /v1/models/default?purpose=chat` returned
byte-identical responses, and the Settings page's Models tab rendered
pixel-identically (screenshots 05 and 08 share a sha256 for that reason, as
P06's own pair did). The single `qualification_job_work_claims` row, created at
16:14:10.544044, was re-claimed by a *new* `WorkerId` after that restart and by
a *third* distinct `WorkerId` after this pass's rebuild — one durable claim row
re-owned across three separate worker process lifetimes, never stranded and
never duplicated. That is Task 1's lease behaving exactly as designed across
process death. Sections 7 and 10.6.

**Docker networking and LM Studio — PASS.** LM Studio answered on
`http://localhost:12345/v1/models` from the host and on
`http://host.docker.internal:12345/v1/models` from inside the worker container.
No failure recorded anywhere in this package is attributable to the model host;
every one of them occurs strictly before any network call to it is attempted.

## What is not proven, and why

### A qualification job cannot reach `qualified` — blocked by a pre-existing gap outside this plan's scope

The job stopped at ordinal `10`, the first *network* probe (`00` and `15` are
static). Two blockers were found underneath, in the order they appeared:

- **Blocker A — the worker's principal has no `principals` row.**
  `crates/vestrace-cli/src/commands/worker.rs:212-221` builds every
  `RequestContext` as `PrincipalId::from_uuid(*workspace)` — the principal id
  *is* the workspace id. The only seeded principal is dev-seed's `…0002`. Every
  network q1 probe writes an audit event under the worker's context, so every
  one violated `audit_events_principal_id_fkey`: 1592 failures in five minutes.
- **Blocker B — the worker's principal holds no capability grant, and the one
  a probe needs is not in its configured set.** With a `principals` row seeded
  by hand as an explicitly-labelled diagnostic, the FK error was replaced by a
  durable policy denial: `result=deny`, `reason=default_deny`,
  `capability=export.read`, `operation=qualification_probe`,
  `subject_id=10000000-…-0001`, `matched_grant_id=null`. All 28 capability
  grants belong to the server's principal `…0002`; `docker-compose.yml` states
  in the worker's own comment that "the worker seeds nothing itself, so that
  grant has to exist," and nothing creates it. Even if it did, the worker's
  configured `VESTRACE_POLICY__CAPABILITIES` is
  `execution.read,execution.write`, which does not contain `export.read`.

A third diagnostic — inserting the missing grant — was refused by the evidence
environment's permission policy and was therefore **not performed**. **This
package therefore cannot say whether the policy denial is the last blocker, and
does not guess.** Sections 5.2-5.4.

**Three things about A and B matter for how this package is judged, and all
three are load-bearing:**

1. **They are not defects in anything Tasks 1-9 built.** Both live in the
   worker's identity construction and the capability-grant seeding, which this
   plan never touched.
2. **They are not specific to qualification, and they predate this plan.**
   `PrincipalId::from_uuid(workspace_id)` is how *every* background worker in
   this deployment constructs its identity — embedding, run, message and
   qualification alike. No real deployment has ever seeded a matching
   `principals` row or capability grants for that identity. The qualification
   worker is simply the first worker in this program to attempt a governed
   network effect and therefore the first to hit it.
3. **They are explicitly out of this plan's scope and deferred.** The
   controller and the operator scoped the worker identity and authorization
   model into a separate future package. This package documents them; it does
   not claim to fix them, and no work toward fixing them was attempted.

Consequence, stated plainly: `qualification_jobs.state` stayed `running` and
`completed_at` stayed NULL for every run in this package; exactly one of twelve
q1 ordinals ever recorded a result. `finalize_success` — the step the plan
itself flagged as needing implementation-time resolution — was never reached,
so **its correctness remains unproven either way by this package.** Not proven
correct, and not proven incorrect. Section 5.6.

**So gap 3 is half-closed, and this is what each half means.** The half this
plan owned — a real claim/lease mechanism in a real migration, and a production
worker that polls, claims, leases, executes and releases qualification work — is
built, deployed and demonstrated on live data. The half it did not own — an
identity the rest of the system will authorize — is missing, and was missing
before this plan started.

### Operational concern found along the way

`poll_qualification_work` releases a failed probe as `RetryableFailure`, making
the job immediately reclaimable with no backoff. Against a deterministic
failure the worker reclaims and refails roughly twenty times per second,
forever, writing a durable `external_effect_authorizations` row each time —
8731 rows from one stuck job in about eight minutes, and the loop resumed at the
same rate after both the restart and this pass's rebuild (19595 rows
cumulative in the volume by the end). A deployment that hit this would
accumulate rows at roughly 1.5M/day from a single stuck job. The worker was
stopped to halt it. This is recorded as a finding; fixing it is not in this
package's scope. Sections 5.5 and 10.6.

## Gap 4 (AG-UI real execution): unchanged, and deliberately not attempted

Spec §3.4 scoped composing `GovernedRunStepInputAuthority` out of this plan
during the plan's own writing, after tracing all 15 fields of
`PrepareGovernedRunStepInput` and finding that sampling defaults, effect-intent
derivation and a pinned-binding read port are undecided production design work,
not composition. This package did not attempt to prove AG-UI works, and its
refusal is **not** a failure of this package.

For the record, it was exercised once and is byte-identical to what P06
recorded: `POST /ag-ui/run` → HTTP 503,
`{"code":"unavailable","message":"governed Run-step input authority is not configured"}`.
Nothing regressed and nothing improved, which is the expected and correct
outcome. No `/ag-ui/events/stream` payload and no model completion text appear
anywhere in this package's evidence, because none was produced. Section 6.

**This package does not make AG-UI work, and this document does not round its
real, narrower scope up to a claim that it does.**

## What would still be needed for a real `qualified` state

Stated as open work, not as anything this package proved:

1. A `principals` row for whatever identity the worker runs as, or a change to
   `worker.rs:212-221` so the worker runs as a real seeded principal rather than
   as the workspace id. *(Blocker A — deferred to the worker-identity package.)*
2. A capability grant of `export.read` / `qualification_probe` for that identity,
   and `export.read` added to the worker's configured capability set.
   *(Blocker B — same package.)*
3. Whatever, if anything, lies beyond (2) — **unknown**, not reached in any run
   of this package.
4. Separately: backoff on `RetryableFailure`, so a deterministically-failing job
   does not hot-loop.
5. Separately: the gap-4 design work spec §3.4 deferred to its own package.

Item 1 of the previous revision of this list — an operator-facing way to obtain
a Connection revision's `execution_guard_id` — **is done**, by Task 9, and is
verified live in transcript section 10.

Only after (1)-(3) could the twelve-ordinal q1 sequence run against LM Studio
and `finalize_success` be exercised at all.

## Evidence files

| File | SHA-256 | Bytes |
|---|---|---|
| `docs/development-evidence/v1-g0-05-gate/p06-followup-no-auth-run-test.txt` | `eafa6aa56e5ec665dc37716d93d3f7a07772d39dd1a9ff05de54a42ee9c07fd2` | 73107 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/01-connection-created.png` | `3b02521308044ba5394fe01f9de81f097f9fcaf7479d6bf38a2e8d2af93324af` | 92641 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/02-model-register-console-form-blocked.png` | `d7bf3a7d7dccc8fcd5461cde6be873cc318a51fa4c6c695f83677ffb45a13b30` | 110860 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/03-both-models-registered.png` | `bd60166c2dd778525ac61c4cbbb447a27a9c2cc9b6af7409fd9ef7e40711836c` | 94299 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/04-chat-default-set.png` | `2b1992d89c230eb0b9329f4c2769e5dbd6293f50693d40dfd2a2fdfd7b5028b2` | 103486 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/05-settings-models-default-pre-restart.png` | `f708024411ced0c30f1fff9c122da03704f91a01a26fa04e6527bae23104998b` | 86653 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/06-ag-ui-refusal-expected.png` | `dcc5165d7d88d51811a6826cf0ceb9ac5ec2cb21360a170dd578bf88665018f0` | 134428 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/07-connection-qualification-stuck-pre-restart.png` | `336cfe402bd96c3ab57c89835ff9ebf7ce4fb93caba4fc9fb0e23005bdd4a876` | 87703 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/08-settings-models-default-post-restart.png` | `f708024411ced0c30f1fff9c122da03704f91a01a26fa04e6527bae23104998b` | 86653 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/09-console-chat-model-registered.png` | `eb4943c58820564f6dcc32aa55d90ead84e83b13ed4ba18c4d033dec87679dc7` | 112503 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/10-console-embedding-model-registered.png` | `a82322a0dfbd026406365738e32cedbe4a531a2ca3a67ff77129110b46640280` | 125958 |
| `docs/development-evidence/v1-g0-05-gate/p06-followup-screenshots/11-models-list-after-reload.png` | `7cf47d03ba0c63edcd520e973f8339129f83be77bd16a0268824478db3aae7dc` | 119167 |

(05 and 08 are byte-identical: the workspace default's browser rendering was
pixel-identical before and after the restart, corroborating the curl-based proof
of byte-identical API state above. This is the same phenomenon P06's own closing
document recorded for its pair, not a duplication error.)

Screenshots 09-11 are from the Task 9 verification pass; 01-08 are from Task 7's
re-run. The transcript's sections 0-9 are preserved unedited from that re-run,
with section 10 appended by the verification pass and a pointer added to the
up-front scorecard so the original summary cannot be read as final.

## Database mutations made by this package's evidence runs that are not product paths

Recorded so no future reader mistakes them for normal deployment state:

1. Two Model revisions (`1dad39a6-…` chat, `f0603965-…` embedding) were created
   by direct `POST /v1/models/{id}/revisions` calls carrying the connection's
   real `execution_guard_id`, because at that time the console form could not.
   This was Blocker C, and it is now fixed: the two later Models
   (`a687eb45-…` chat, `356c940e-…` embedding) went through the console form,
   and would be the normal path today.
2. A `principals` row with id `10000000-0000-0000-0000-000000000001`, identifier
   `worker-context-principal-diagnostic`, was inserted by hand as the labelled
   section-5.3 diagnostic. dev-seed does not create it; it is still present in
   the volume.
3. As a consequence of (2), the worker's probe failures change character
   mid-transcript (FK violation → policy denial). Both are recorded in the order
   they actually occurred.
4. ~19,600 `external_effect_authorizations` deny rows accumulated from the hot
   retry loop across all runs. They are genuine governance evidence records, not
   fixtures.
5. The Postgres volume was deliberately never destroyed (`docker compose down -v`
   was never run), because a virgin database hits a separate, known,
   out-of-scope P05-era defect: `migrate_through_version`'s compatibility
   pre-check queries `_sqlx_migrations` before `Migrator::run()` creates it.
   That defect is unrelated to this package and is not addressed by it.

## Disposition

This package closed two of the three gaps it targeted, outright and with live
browser evidence, and fixed a third console-layer defect it discovered along the
way. It put a real qualification executor into a production binary and
demonstrated it claiming, leasing, running and releasing work on live data — the
first time that has happened in this codebase — and showed the lease surviving
two real process deaths. It did not deliver its headline outcome: no Connection
or Model reached `qualified`, and `finalize_success` was never exercised.

The reason it did not is a pre-existing gap in how every background worker in
this system obtains an authorized identity. That gap predates this plan, is not
confined to qualification, and has been deliberately scoped into its own future
package rather than folded into this one. Recommending it as that package's
subject is this package's most useful output after the code it shipped.

Gap 4 remains exactly as `docs/development-evidence/v1-g0-06-real-execution.md`
left it, deferred by spec §3.4 to its own future package.
