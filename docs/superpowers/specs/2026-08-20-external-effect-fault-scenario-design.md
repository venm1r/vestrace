# External Effect Fault Scenario Design

**Date:** 2026-08-20
**Status:** approved design, not yet implemented.
**Scope:** the program that actually breaks the external-effect lifecycle, so
the fault suite has something truthful to observe.
**Non-claim:** this qualifies no profile and makes no release claim. Its likely
first result is a **failing** fault suite, and that is the point.

## 1. Problem

The v1.0 release gate reports `fault_suite_missing`. The reason is not that the
suite is unbuilt — almost all of it exists:

```text
FaultInjectionSettings / FaultInjectionDriver / FaultInjectionEnvironment
ConfiguredEffectFaultScenarioExecutor
ProcessFaultInjectionRuntime  /  DockerFaultInjectionRuntime
ExternalEffectFaultSuiteService
evaluate_fault_suite  →  FaultSuiteDecision  →  the release gate
```

What is missing is the thing at the bottom of that stack. `EffectFaultScenarioExecutor::execute(point)` must return a `FaultObservation` describing what the
system actually did when it died at that point, and every test of this machinery
supplies it from a mock:

```rust
async fn execute(&self, ..., point: EffectFaultPoint) -> Result<FaultObservation, _> {
    Ok(FaultObservation::expected(returned_point))   // MockRuntime
}
```

`ProcessFaultInjectionRuntime` shells out to a program that prints the
observation as JSON. **No such program exists in this repository.** A search for
the five fault-point names finds them only in the parser that reads them.

### 1.1 The trap this design exists to avoid

A twenty-line program that prints `FaultObservation::expected(point)` for each
point would remove `fault_suite_missing` from the release gate and prove
nothing. It would be an attestation wearing the costume of an execution — the
same substitution this project refuses everywhere else, from
`EvidenceOrigin::RemoteSelfAsserted` to the executed/attested split in the
conformance gate.

The whole value of this work is that the observation is *found*, not *stated*.
Every decision below is subordinate to that.

## 2. Goal

A scenario program that, for each of the five fault points, kills a real
process mid-lifecycle against a real database, lets the system recover, reads
what survived, and reports it.

Success is **not** "the fault suite passes". Success is that its verdict is
earned. Given that effect recovery runs in the worker and points 4 and 5 cross
services that have never been exercised this way, a failing suite is the
expected first outcome and is more valuable than a passing one.

## 3. Where the program lives, and why not in the binary

Three ways to kill the lifecycle at an exact point were considered.

**Hooks compiled into the shipped binary**, guarded by
`FaultInjectionSettings::enabled` and the environment. Reaches every point, and
the guard already exists in the design — but it puts a switch that aborts the
process into the image that runs in production. Rejected: a capability that
cannot be reached is stronger than a capability that is merely guarded.

**A killer outside the process**, where the webhook target kills the container
at dispatch time. Zero production hooks and ideal for point 3, where the effect
must reach the world before the process dies. It cannot reach points 1, 2, 4
or 5.

**A separate scenario binary** — chosen. `ProcessFaultInjectionRuntime` already
invokes its program **once per point**, so the program can own the whole
sequence: spawn a child, have the child drive the real lifecycle and abort at
the requested point, then recover, inspect and report.

It lives in a new workspace crate, `crates/vestrace-fault-scenario`, with a
single binary. The image builds `--package vestrace-cli --bin vestrace` and
copies that one path, so a new crate is excluded by construction rather than by
discipline. A guard test asserts the Dockerfile never names it.

## 4. The invocation contract, and what it forces

`execute_fault_command` already fixes the contract, and it constrains this
design more than it might appear:

```rust
command.args(args).env_clear();
for name in preserve_env { /* allowlist only */ }
command
    .env("VESTRACE_FAULT_TARGET_DIGEST", target_digest)
    .env("VESTRACE_FAULT_POINT", fault_point_name(point))
    .env("VESTRACE_FAULT_ISOLATION", isolation_name(settings.environment()));
```

The environment is **cleared**. The allowlist is empty off Windows. So the
scenario program cannot inherit `VESTRACE_DATABASE__URL`, and that is a
property worth keeping: a process whose job is to kill things mid-transaction
must not silently inherit the credentials of whatever ran it.

**Decision:** the database is named by `--database-url-file <path>` in the
configured `args`, and the program reads the URL from that file. A path in
`argv` is not a credential; a URL in `argv` would be, since argv is world-
readable on a normal host. The file is provisioned by whoever set up the
ephemeral environment.

**Decision:** the program refuses to run unless `VESTRACE_FAULT_ISOLATION` is
`ephemeral`. It kills processes mid-transaction against whatever database it is
given; the isolation claim is the only thing standing between that and someone's
data, so it is checked rather than assumed.

## 5. The scenario

For one point, in one invocation:

```text
parent  read --database-url-file, check isolation, prepare a clean effect fixture
        spawn child: same binary, VESTRACE_FAULT_CHILD=1, same point
  child  drive the real lifecycle through the real services against the real
         database, and at exactly the requested point call std::process::abort()
parent  observe the child died; run the recovery path the deployment runs
         (the worker's ExternalEffectRecoveryService, plus outcome delivery)
        read the persisted state back through PgExternalEffectRepository
        print the observation as JSON on stdout
```

The child aborts rather than exiting: `abort()` leaves in-flight transactions
unresolved and buffers unflushed, which is what a crash is. An orderly exit
would test a shutdown path nobody is asking about.

### 5.1 Where the points sit

Points 1-3 are inside `PerformExternalEffectService::perform`:

| Point | Abort site | What the suite requires |
|---|---|---|
| `AfterIntentPersistence` | after `insert_intent`, before authorization | status `Prepared`, no retry, no reconciliation, no receipt |
| `AfterAuthorizationBeforeDispatch` | after `authorize`, before `dispatch` | status `Authorized`, no retry |
| `AfterDispatchBeforeReceipt` | after `dispatch` returns, before `insert_receipt` | status **`Unknown`**, reconciliation started, no retry |

Point 3 is the one that matters most: the effect has happened in the world and
nothing has recorded it. Nothing may retry, and something must start
reconciling.

Points 4 and 5 are **outside** `perform`, which is a fact this design has to
state rather than discover during implementation:

| Point | Abort site | What the suite requires |
|---|---|---|
| `AfterReceiptBeforeOutcomeConfirmation` | after `insert_receipt`, before the reconciliation settles the outcome | the receipt persists; the outcome is not asserted |
| `AfterOutcomeBeforeRunCommit` | after the outcome settles, before `mark_outcome_delivered` | the outcome survives and is still undelivered, so delivery can resume |

Point 5 lives in `ExternalEffectOutcomeDeliveryService`
(`find_undelivered_outcomes` / `mark_outcome_delivered`).

### 5.2 Recovery must include the worker

`ExternalEffectRecoveryService` is constructed in
`crates/vestrace-cli/src/commands/worker.rs`, not in the server. A scenario that
restarts only the server would record point 3 as failing because half the
deployment was never started — a false negative that would read as a real
defect. The parent runs the same recovery construction the worker runs.

## 6. How the observation is kept honest

The fields the suite reads are `status`, `retry_attempted`,
`reconciliation_started`, `receipt_persisted`. Each must be derived from
persisted state, never from what the scenario believes it did:

- `status` — read back from the effect record.
- `receipt_persisted` — `find_receipt` returns a row, or it does not.
- `reconciliation_started` — `find_reconciliation` / the candidate query, not a
  flag the parent sets because it called recovery.
- `retry_attempted` — counted by the **adapter stub**, which records every
  dispatch it receives. A second dispatch for one intent is a retry, and the
  count comes from outside the process being tested.

The adapter stub is a small HTTP server the parent runs: it records dispatches,
answers read-backs, and is the only party that can truthfully say whether the
world was touched twice.

**No branch of the program may construct `FaultObservation::expected(...)`.** A
guard test asserts that string appears nowhere in the crate, because the single
cheapest way for this work to become worthless is for a tired implementer to
fill an awkward field with the answer the suite wants.

## 7. Test strategy

The scenario program is itself tested, and the tests must not require the full
environment:

- **Point mapping:** for each point, a unit test that the child aborts at the
  right site, driven by a fake service set whose calls are recorded — proving
  the abort is where the table says.
- **Observation derivation:** given a database in a known state, the parent
  reports the fields that state implies. Mutation-proved: change each derivation
  to a constant and confirm exactly one case fails.
- **Isolation refusal:** the program exits non-zero when
  `VESTRACE_FAULT_ISOLATION` is not `ephemeral`, and when the URL file is
  missing or empty.
- **No-expected guard:** a test asserting `FaultObservation::expected` is not
  referenced in the crate.
- **Image guard:** a test asserting the Dockerfile does not build or copy this
  binary.
- **End to end**, requiring Docker and a database: the full five-point suite
  against an ephemeral Postgres, recorded as evidence with its verdict whatever
  that verdict is.

## 8. Non-goals and limitations

- This does not make the fault suite pass, and is not permitted to be adjusted
  until it does. If the suite fails, the failure is the finding.
- It does not exercise container or node failure — only process death. The
  `DockerFaultInjectionRuntime` path stays untouched.
- It does not test partial disk writes, fsync loss, or database-side crash
  recovery. Postgres is assumed to keep its own promises.
- It does not run in CI by default: it needs Docker and it kills processes.
- The other three evidence sources in the release gate — release approval,
  recovery qualification and capability restoration — remain without producers,
  so the gate still cannot pass after this work even if this suite passes, which
  it is not expected to.
