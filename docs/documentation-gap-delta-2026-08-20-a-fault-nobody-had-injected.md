# A fault nobody had injected

**Date:** 2026-08-20
**Scope:** the external-effect fault suite, and what happens the first time
anything actually crashes the lifecycle it claims to describe.
**Status:** committed delta on `feat/effect-fault-scenario`. It qualifies no
profile, makes no release claim, and its headline result is a **failing** suite.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures across 5 points, 1 point agrees
vestrace-fault-scenario suite:            27 passed, 0 failed
tests/effect_fault_scenario_e2e.rs:       1 test, #[ignore] by default, fails when run
release gate:                             still cannot pass — 3 evidence sources with no producer
conformance gate:                         199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — untouched
```

## What existed, and what was at the bottom of it

Almost all of the fault suite was already built: `FaultInjectionSettings`,
`ProcessFaultInjectionRuntime` and `DockerFaultInjectionRuntime`,
`ConfiguredEffectFaultScenarioExecutor`, `ExternalEffectFaultSuiteService`,
`evaluate_fault_suite`, and a `FaultSuiteDecision` the v1.0 release gate
consumes. What did not exist was the thing at the bottom of that stack.
`EffectFaultScenarioExecutor::execute(point)` must return a `FaultObservation`
describing what the system did when it died at that point, and **every** test of
this machinery supplied it from a mock:

```rust
async fn execute(&self, ..., point: EffectFaultPoint) -> Result<FaultObservation, _> {
    Ok(FaultObservation::expected(returned_point))   // MockRuntime
}
```

`ProcessFaultInjectionRuntime` shells out to a program that prints the
observation as JSON, and no such program was in this repository. Searching for
the five fault-point names found them only in the parser that reads them back.
The suite had a verdict-shaped hole where its subject should be.

## The trap: the twenty-line liar

A twenty-line program that prints `FaultObservation::expected(point)` for each
point would remove `fault_suite_missing` from the release gate today and prove
nothing. It would be an attestation wearing the costume of an execution — the
same substitution this project refuses everywhere else, from
`EvidenceOrigin::RemoteSelfAsserted` to the executed/attested split in the
conformance gate. Everything below is subordinate to the observation being
*found* rather than *stated*, and the crate carries a guard test,
`the_crate_never_constructs_the_expected_observation`, asserting that
`FaultObservation::expected` is named nowhere in it and its body is not
reproduced. A second guard, `the_shipped_image_does_not_build_this_binary`,
asserts the `Dockerfile` never names the crate: the image builds
`--package vestrace-cli --bin vestrace` and copies that one path, so a program
whose entire purpose is `std::process::abort()` is excluded by construction
rather than by discipline.

## Where the child dies

The process is its own child. `VESTRACE_FAULT_CHILD` decides which half runs, so
the code that dies and the code that reads back what survived are the same build
of the same binary. The child **aborts** rather than exiting: `abort()` leaves
in-flight transactions unresolved and buffers unflushed, which is what a crash
is; an orderly exit would test a shutdown path nobody is asking about.

| Point | Abort site | What the suite requires |
|---|---|---|
| `AfterIntentPersistence` | after `insert_intent`, before authorization | `Prepared`, no retry, no reconciliation, no receipt |
| `AfterAuthorizationBeforeDispatch` | after `authorize`, before `dispatch` | `Authorized`, no retry |
| `AfterDispatchBeforeReceipt` | after `dispatch` returns, before `insert_receipt` | **`Unknown`**, reconciliation started, no retry |
| `AfterReceiptBeforeOutcomeConfirmation` | after `insert_receipt`, before the reconciliation settles | receipt persists, `Reconciling` |
| `AfterOutcomeBeforeRunCommit` | after the outcome settles, before `mark_outcome_delivered` | outcome survives and is still undelivered, `Reconciling` |

Points 4 and 5 sit **outside** `PerformExternalEffectService::perform` — point 5
inside `ExternalEffectOutcomeDeliveryService`, between
`find_undelivered_outcomes` and `mark_outcome_delivered`. Recovery is run by the
parent using the construction the **worker** builds at
`crates/vestrace-cli/src/commands/worker.rs:369`, not a rehearsal of it: a
scenario that restarted only the server would record point 3 as failing because
half the deployment was never started, and that false negative would read as a
real defect.

The database is named by `--database-url-file <path>`, never by environment and
never by `argv`. `execute_fault_command` calls `env_clear()` and preserves an
allowlist that is empty off Windows, so the program cannot inherit a connection
string — a property worth keeping, since a process whose job is to kill things
mid-transaction must not silently acquire the credentials of whatever ran it. A
path in `argv` is not a credential; a URL would be. The program also refuses to
run unless `VESTRACE_FAULT_ISOLATION` is `ephemeral`, and `ScenarioSettings`
prints `[REDACTED]` for the URL it holds, because a `derive(Debug)` on a struct
carrying a password renders that password into any panic in a crate that panics
by design.

## Where each field comes from, and why

The suite reads four fields. Each is derived from persisted state or from a
party outside the process that died; none is derived from what the parent knows,
and the parent knows a great deal by that point.

- **`status`** — the status the *persisted receipt* carries. With no receipt
  there are two states that are identical in the database, and the stub's
  dispatch count separates them: a counted dispatch with no surviving record of
  what came back is `Unknown`; no counted dispatch means the effect never left,
  which is `Prepared`.
- **`receipt_persisted`** — whether a receipt could actually be *read*, by
  whichever route reaches one from an effect id. Reported as "not read" rather
  than guessed at, which is not the same as absent (see finding 3).
- **`reconciliation_started`** — a reconciliation row found through the outcomes
  the workspace still owes its runs, then re-read by its own id through
  `find_reconciliation`; never a flag set because the parent called recovery.
- **`retry_attempted`** — counted by the adapter stub, which records every
  dispatch that reaches it. A second dispatch for one intent is a retry, and the
  count comes from outside the process being tested. The stub was taught the
  production read-back vocabulary (`effect_applied`, `state_ref`,
  `evidence_refs`, `evidence_strength`) so the deployment's own
  `HttpExternalEffectReadBackAdapter` can query it; a dedicated test asserts
  reading back never moves the count, because read-back observes and does not
  act.

`point` is the only field carried through rather than read: it is the question
that was asked, and an answer cannot establish its own question.

Mutation testing was run at each stage and each mutation killed exactly one
case: the isolation refusal, the abort-site stage map, and all four observation
derivations in both directions with none left unpinned. One blind spot that
mutation did not reach was disclosed and pinned rather than closed —
`find_undelivered_outcomes` additionally requires `notified_at IS NULL` and a
settled outcome, currently harmless at point 5 and therefore exactly the kind of
assumption that stops being harmless silently. Widening the query is a change to
the repository port and was refused as out of scope.

## The verdict

Run against a PostgreSQL 17 provisioned for the purpose (compose publishes no
host port, so this cannot be the compose database), one ephemeral database, five
invocations, five aborted children:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Unknown receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Unknown receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Acknowledged receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
FAULT_SUITE_FAILURE fault point AfterAuthorizationBeforeDispatch has incorrect lifecycle status
FAULT_SUITE_FAILURE UNKNOWN fault point must start reconciliation
FAULT_SUITE_FAILURE fault point AfterReceiptBeforeOutcomeConfirmation has incorrect lifecycle status
FAULT_SUITE_FAILURE fault point AfterOutcomeBeforeRunCommit has incorrect lifecycle status
```

**Four of five points disagree with what the suite expects. Point 1 agrees.**
These are the same five observations an earlier hand-run produced against a
fresh database per point; running all five against one provisioned database
reproduces them exactly, because every reading filters by effect id.

Nothing was tuned toward this. The way to turn it green is to change the system
being observed, and the harness says so in its own doc comment: it may not be
turned green by changing the scenario program, the stub, or the fixtures.

## What the failures actually say

**1. `EffectLifecycleStatus::Reconciling` has no writer anywhere in the
workspace.** Outside the fault-scenario crate the identifier occurs exactly
three times: the enum declaration, `FaultObservation::expected` itself, and the
parser in `fault_runtime.rs` that reads observations back. No adapter outcome
maps to it — `from_dispatch` produces `Acknowledged`, `Failed` or `Unknown`
only, so even a 5xx stub would give `Unknown`. A persisted receipt status is
**write-once**: there is no `UPDATE external_effect_receipts` statement in the
codebase at all, only one backfill inside migration `0144`. And the HTTP API
collapses every status that is not `Acknowledged` or `Failed` to the string
`"unknown"`. The suite demands a state the system cannot reach, and the only way
to produce it is to state it — which is the one thing this work is forbidden to
do. Points 4 and 5 fail on exactly this.

**2. The recovery sweep is a no-op at all five fault points.** This is arguably
the larger finding, and it was not something the slice was sent to look for.
`find_reconciliation_candidates` filters `r.outcome_status = 'unknown'`, and
**no fault point leaves an unknown receipt**. Points 1–3 leave no receipt row at
all — at the dispatch point the crash lands before `insert_receipt`, so there is
nothing for the query to select. Points 4 and 5 leave an *acknowledged* one,
because the stub answers 200 and `HttpWebhookEffectAdapter` maps 2xx to
acknowledged. The sweep is therefore structurally unable to see a
dispatched-but-unreceipted effect, which is **precisely the crash the
external-effects design exists to survive**. Point 3's
`reconciliation_started: false` is the same fact from the other side: with no
receipt row nothing enrols the effect, and it is invisible to recovery forever.
Recorded, not fixed — the repair is a change to the recovery query and its port,
and this slice's job is to find.

**3. An acknowledged receipt is unreachable from an effect id.** `find_receipt`
takes an `ExternalEffectReceiptId`, which nothing outside the dead child ever
held, and the port offers no by-effect-id query. Point 4 genuinely persists a
receipt and the observation still reports `receipt_persisted: false`, because
neither surviving route reaches it: the candidate query filters on a status the
row does not have, and a reconciliation naming the receipt is exactly what point
4 crashed before writing. "Unreachable" and "absent" are different facts and
only one of them is reported, deliberately as the weaker claim.

**4. Points 1 and 2 are identical in storage.** Authorization writes nothing
anywhere — `authorize` touches no repository and there is no authorizations
table in any migration — so `Authorized` is not a state any reader can
distinguish from `Prepared`. Both points read back `prepared`, and point 2's
failure is that difference and nothing else.

**5. Handing this decision to the release gate would change one word.**
`ExactEnvironmentReleaseFailure::FaultSuiteMissing` becomes `FaultSuiteFailed` —
the same distinction the previous slice drew for runtime evidence between
"nobody looked" and "we looked and it did not pass". Both are failures; only one
of them is knowledge.

## What this does **not** do

It does not make the fault suite pass, and it is not permitted to be adjusted
until it does.

**Only process death is exercised.** No container is stopped, no node is lost,
no network partitions. `DockerFaultInjectionRuntime` is untouched and unexercised
by this work, and a `FaultInjectionEnvironment::ProviderSandbox` claim is still
refused outright because no provider sandbox driver exists.

**Postgres is assumed to keep its own promises.** Nothing here tests partial
disk writes, `fsync` loss, torn pages, or database-side crash recovery. The
child aborts; the database is trusted to have done what it said it had done.

**The suite does not run in CI.** It needs Docker and it aborts five processes,
so `tests/effect_fault_scenario_e2e.rs` is `#[ignore]`d and no pipeline runs it.
`docker-compose.yml` publishes no host port for Postgres, so there is no
checked-in database it could find either — the run recorded above used a
throwaway container that was removed afterwards. This document is therefore
evidence produced by hand, which is weaker than a test and is not claimed to be
one; what is committed is the harness that reproduces it.

**Nothing invokes the suite in production.** `ExternalEffectFaultSuiteService`
and `evaluate_fault_suite` still have no call site in `vestrace-cli` or
`vestrace-http`. This slice built the suite a subject; it did not build the
suite a producer, so the release gate still reports `fault_suite_missing` rather
than the `fault_suite_failed` it has now earned.

**The target digest is not verified.** The harness passes
`sha256:local-debug-build-not-digest-verified` because nothing in it checks that
the scenario binary was built from the tree it runs against. A digest naming a
real build would be a claim this harness cannot support.

**The release gate still cannot pass**, for the same structural reason as
before: release approval, recovery qualification and capability restoration
remain without producers, so a passing fault suite would not be sufficient even
if there were one.

**Deferred minors, recorded rather than silently carried.** `crates/vestrace-cli`
has a shared `redact_url` helper that the scenario's redacted `Debug` does not
reuse. `AdapterStub::shutdown` aborts the accept task but not in-flight
connection handlers, so "no task leaks" is broader than the code guarantees, and
`read_request_line` returns a partial first line when a peer closes mid-header
rather than treating it as malformed. The child duplicates the delivery batch
size `32` instead of referencing the private `DELIVERY_BATCH` constant, and its
intent uses `RiskCategory::Medium` copied from the fixture where the real HTTP
call site uses `High`. `deliver_outcomes` discards the delivery report, so a
storage partial failure that did not surface as an `ApplicationError` would be
invisible. The stub's `effect_applied` is per-stub rather than per-effect, valid
only because one stub drives one effect per invocation.

**Point 5's reading is order-dependent and known fragile.** `observe` can see
the reconciliation only because outcome delivery *deferred* — the child's
`execution_ref` names a run with no event stream, so the debt stays owed and
`notified_at` stays NULL. A future scenario child that created a real run would
have delivery pay the debt before `observe` looked, turning point 5's
`reconciliation_started` to `false`. The recover-deliver-observe order is what
the design prescribes and the truthful reading is the one that order produces,
but it is a reading with a hinge in it.
