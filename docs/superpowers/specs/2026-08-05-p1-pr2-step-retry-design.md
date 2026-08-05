# Vestrace P1 / PR2 Step Retry Design

**Status:** Approved for implementation planning  
**Date:** 2026-08-05  
**Base:** `agent/p1-state-engine-vertical`  
**Branch:** `agent/p1-step-retry`

## 1. Objective

Add crash-safe, deterministic retry semantics for one active run step without introducing a scheduler, worker loop, HTTP command surface, or run-wide retry model.

The canonical lifecycle is:

```text
Running
→ StepAttemptFailed
→ StepRetryScheduled
→ WaitingForRetry
→ StepRetryResumed
→ StepAttemptStarted with the same step_id and attempt + 1
→ Running
```

The State Engine records retry intent, attempt identity, retry limit, failure details, and the earliest permitted resume time. It does not calculate backoff, sleep, poll timers, or execute the retried side effect.

## 2. Current foundation

P1 / PR1 already provides:

- typed run commands and events;
- pure `decide`, `apply`, and `replay` functions;
- optimistic stream concurrency through `run_streams`;
- atomic event plus projection commits;
- checkpoints, bounded replay, and projection rebuild;
- waiting for input and approval;
- a generic `Resume` command for input and approval waits.

The current step model is insufficient for retries:

- `RunStepState` has no attempt number or retry limit;
- `FailStep` removes the active step;
- `StepFailed` does not preserve enough retry context;
- the run cannot distinguish a final step failure from a scheduled retry;
- no typed state records `resume_at`;
- `Resume` could not safely reconstruct the failed step.

## 3. Considered approaches

### A. Retry outside the State Engine

A worker could catch a failure, sleep, and invoke the same tool again without new domain events.

Rejected because a process crash would lose retry intent, replay would not explain repeated side effects, and attempt limits would depend on transient worker memory.

### B. Mutable retry counters in `agent_runs`

The projection could store `attempt`, `max_attempts`, and `resume_at`, while the event stream retained only generic step failures.

Rejected because projection state is derived and rebuildable. Retry truth must survive projection deletion and be reproduced exactly from canonical events.

### C. Event-sourced step attempts with timed waiting

Each executable attempt has a durable identity. A retryable failure and its schedule are appended atomically. Resuming a due retry appends a resume event and the next attempt-start event atomically.

Selected because it preserves deterministic replay, exposes an idempotency identity for future executors, survives crashes, and does not require a scheduler in this PR.

## 4. Scope

### Included

- attempt-aware step commands, events, and state;
- a typed retry wait reason;
- caller-supplied retry timing;
- deterministic due-time validation;
- retry-limit exhaustion;
- checkpoint format migration;
- projection status support for `waiting_for_retry`;
- domain, application, PostgreSQL, recovery, and end-to-end tests.

### Excluded

- retry of an entire run;
- automatic backoff calculation and jitter;
- scheduler polling and due-work queries;
- production workers;
- automatic execution of side effects;
- HTTP endpoints;
- persisted command idempotency;
- generic step deadlines or timeout cancellation;
- delegated subagents;
- compensation;
- capability and risk policy;
- Artifact CAS.

## 5. Attempt identity

A logical step attempt is identified by:

```text
(workspace_id, run_id, step_id, attempt)
```

Rules:

1. The first attempt is `1`.
2. `max_attempts` includes the first attempt.
3. `max_attempts` must be at least `1`.
4. A retry uses the same `step_id`.
5. Every resumed attempt increments `attempt` by exactly one.
6. `attempt` must never exceed `max_attempts`.
7. Attempt increment uses checked arithmetic.

Future executors should derive their side-effect idempotency key from this identity. PR2 exposes the identity but does not execute or deduplicate the external side effect.

## 6. Domain types

### 6.1 Failure disposition

Replace the ambiguous command-level `retryable: bool` decision with:

```rust
pub enum StepFailureDisposition {
    Final,
    Retry { resume_at: Timestamp },
}
```

`Final` means no retry is scheduled for this failed attempt. It is final for the step, not automatically terminal for the run. The orchestrator may subsequently fail, cancel, stall, compensate, or start another step according to later policies.

`Retry` requests another attempt. The State Engine either schedules it or stalls the run when the configured attempt limit is exhausted.

### 6.2 Failure record

```rust
pub struct StepFailure {
    pub code: String,
    pub message: String,
    pub failed_at: Timestamp,
}
```

Failure code and message use the existing required-text normalization rules.

### 6.3 Active step state

```rust
pub struct RunStepState {
    pub id: RunStepId,
    pub kind: String,
    pub label: Option<String>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub status: RunStepStatus,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}
```

A running active step always satisfies:

```text
1 <= attempt <= max_attempts
```

### 6.4 Retry wait state

Add:

```rust
RunWait::Retry {
    step_id: RunStepId,
    kind: String,
    label: Option<String>,
    failed_attempt: u32,
    next_attempt: u32,
    max_attempts: u32,
    resume_at: Timestamp,
    last_failure: StepFailure,
}
```

This is the complete durable context needed to start the next attempt without consulting a projection or transient worker state.

### 6.5 Run status

Add:

```rust
RunStatus::WaitingForRetry
```

`RunStatus::is_waiting()` includes input, approval, and retry waiting.

The PostgreSQL status mapping adds:

```text
waiting_for_retry
```

`agent_runs.status` is text and requires no schema expansion for this value.

### 6.6 Retry-limit stall reason

Add:

```rust
StallReason::RetryLimitExceeded {
    step_id: RunStepId,
    attempts: u32,
}
```

The run becomes terminal `Stalled` when retry is requested after the final allowed attempt has failed.

## 7. Commands

### 7.1 Start the first attempt

Change the internal command contract to:

```rust
RunCommand::StartStep {
    step_id: RunStepId,
    kind: String,
    label: Option<String>,
    max_attempts: u32,
}
```

It is valid only when the run is `Running`, no active step or wait exists, and `max_attempts >= 1`.

It emits `StepAttemptStarted` with `attempt = 1`.

### 7.2 Complete an attempt

The existing command remains:

```rust
RunCommand::CompleteStep {
    step_id: RunStepId,
    output_references: Vec<String>,
}
```

It always emits `StepAttemptCompleted` using the attempt stored in `RunStepState`. This also applies when the active state originated from a legacy `StepStarted` event, because legacy replay normalizes that state to `attempt = 1` and `max_attempts = 1`.

### 7.3 Fail an attempt

Change the internal command contract to:

```rust
RunCommand::FailStep {
    step_id: RunStepId,
    code: String,
    message: String,
    disposition: StepFailureDisposition,
}
```

It always emits `StepAttemptFailed` using the active attempt identity, including for an active state reconstructed from a legacy `StepStarted` event.

For `Final`, the active step is cleared and the run remains `Running` with no retry wait.

For `Retry { resume_at }`:

- `resume_at >= command.issued_at` is required;
- when `attempt < max_attempts`, `StepRetryScheduled` is emitted in the same command transaction;
- when `attempt == max_attempts`, `Stalled(RetryLimitExceeded)` is emitted in the same transaction;
- no schedule is created after the limit is exhausted.

### 7.4 Resume a due retry

Add:

```rust
RunCommand::ResumeStepRetry {
    step_id: RunStepId,
}
```

It is valid only when:

- status is `WaitingForRetry`;
- `RunWait::Retry` exists;
- `step_id` matches the scheduled step;
- `command.issued_at >= resume_at`.

It emits two events atomically:

```text
StepRetryResumed
StepAttemptStarted
```

The second event starts `next_attempt` with the stored step kind, label, and retry limit.

The generic `Resume` command remains valid only for input and approval waits. It cannot bypass retry timing.

## 8. Events and compatibility

Existing persisted variants remain unchanged:

- `StepStarted`;
- `StepCompleted`;
- `StepFailed`.

Changing their payload shape would make existing JSON events unreadable. PR2 therefore adds new variants:

```rust
RunEvent::StepAttemptStarted {
    step_id: RunStepId,
    kind: String,
    label: Option<String>,
    attempt: u32,
    max_attempts: u32,
}

RunEvent::StepAttemptCompleted {
    step_id: RunStepId,
    attempt: u32,
    output_references: Vec<String>,
}

RunEvent::StepAttemptFailed {
    step_id: RunStepId,
    attempt: u32,
    max_attempts: u32,
    code: String,
    message: String,
    retryable: bool,
}

RunEvent::StepRetryScheduled {
    step_id: RunStepId,
    kind: String,
    label: Option<String>,
    failed_attempt: u32,
    next_attempt: u32,
    max_attempts: u32,
    resume_at: Timestamp,
    code: String,
    message: String,
}

RunEvent::StepRetryResumed {
    step_id: RunStepId,
    attempt: u32,
}
```

Event type strings are:

```text
step.attempt_started
step.attempt_completed
step.attempt_failed
step.retry_scheduled
step.retry_resumed
```

Each starts at event version `1`.

Legacy `StepStarted` replay creates an active step with `attempt = 1` and `max_attempts = 1`. Legacy `StepCompleted` and `StepFailed` continue to replay unchanged. New commands operating on a legacy active state emit the new attempt-aware completion or failure event.

## 9. Decision semantics

### First attempt

```text
StartStep(max_attempts = N)
→ StepAttemptStarted(attempt = 1, max_attempts = N)
```

### Final failure

```text
FailStep(Final)
→ StepAttemptFailed(retryable = false)
```

Result:

```text
status = Running
active_step = None
wait = None
```

### Scheduled retry

```text
FailStep(Retry { resume_at })
→ StepAttemptFailed(retryable = true)
→ StepRetryScheduled(next_attempt = attempt + 1)
```

Result:

```text
status = WaitingForRetry
active_step = None
wait = Retry { ... }
```

### Exhausted retry limit

```text
FailStep(Retry { resume_at }) at attempt == max_attempts
→ StepAttemptFailed(retryable = true)
→ Stalled(RetryLimitExceeded)
```

Result:

```text
status = Stalled
active_step = None
wait = None
completion = Stalled
```

The supplied `resume_at` is not persisted when no next attempt is permitted.

### Due resume

```text
ResumeStepRetry(step_id) at issued_at >= resume_at
→ StepRetryResumed(next_attempt)
→ StepAttemptStarted(next_attempt)
```

Both events share the same causation and correlation IDs and are committed atomically.

## 10. Reducer semantics

`StepAttemptStarted` requires status `Running`, no active step, no wait, positive attempt values, and `attempt <= max_attempts`.

`StepAttemptCompleted` and `StepAttemptFailed` require matching `step_id` and `attempt`.

`StepAttemptFailed` clears the active step but does not choose the next run status by itself. The immediately following event in the same command determines whether the run remains running, waits for retry, or stalls.

`StepRetryScheduled` requires:

- status `Running`;
- no active step;
- `next_attempt == failed_attempt + 1`;
- `next_attempt <= max_attempts`.

It sets `WaitingForRetry` and stores the complete retry context.

`StepRetryResumed` requires a matching retry wait. It clears the wait and returns the run to `Running`. The following `StepAttemptStarted` recreates the active step in the same atomic event batch.

Every event prefix remains reducible, while the command committer guarantees that readers never observe a partially committed multi-event batch.

## 11. Time authority

The reducer never reads a clock.

The decision function compares:

```text
command.issued_at >= retry.resume_at
```

The command boundary supplies a trusted server timestamp. Future HTTP or worker adapters must not accept arbitrary client time as authoritative `issued_at`.

An immediate retry is allowed when `resume_at == issued_at`.

## 12. Failure and concurrency semantics

- resume before `resume_at`: typed `RetryNotReady` error;
- wrong retry `step_id`: typed invalid-command error;
- zero `max_attempts`: typed invalid-command error;
- attempt overflow: typed decision/reduction error;
- retry after limit exhaustion: atomic terminal stall;
- duplicate concurrent resume commands: one commit and one optimistic conflict;
- crash after scheduling: replay restores the exact retry wait;
- crash after resume commit: replay restores the exact next active attempt;
- SQL failure during a multi-event transition: stream, events, and projection roll back together;
- cancellation, explicit run failure, and manual stall remain allowed while waiting for retry;
- `Complete`, `StartStep`, generic `Resume`, and a second retry schedule are invalid while waiting for retry.

## 13. Checkpoint compatibility

Adding attempt fields and a retry wait variant changes canonical `RunState` serialization and its SHA-256 checkpoint hash.

PR2 introduces checkpoint format version `2`.

Migration `0113_step_retry_checkpoint_format.sql` must:

1. truncate `run_checkpoints` because checkpoints are derived acceleration data;
2. replace the `format_version = 1` constraint with `format_version = 2`;
3. preserve streams and events unchanged.

The application constant becomes:

```rust
RUN_CHECKPOINT_FORMAT_VERSION: u16 = 2;
```

No event history is discarded. Checkpoints are recreated lazily from authoritative events.

## 14. Projection behavior

The compact `AgentRun` projection continues to expose only run identity, status, version, and timestamps.

Required changes:

- map `RunStatus::WaitingForRetry` to `waiting_for_retry`;
- parse `waiting_for_retry` from PostgreSQL;
- preserve exact projection rebuild equality after retry events;
- do not add retry queue columns or a scheduler table in PR2.

The full retry context remains recoverable from canonical events and checkpoints. A later scheduler PR may add a derived due-retry projection without changing domain semantics.

## 15. Testing strategy

### Domain decision tests

- first start emits attempt `1` and configured limit;
- zero retry limit is rejected;
- final failure emits no retry schedule;
- retryable failure emits failure plus schedule atomically;
- retry time before command time is rejected;
- exhausted limit emits failure plus stall;
- resume before due time is rejected;
- due resume emits resume plus next attempt start;
- wrong step ID is rejected;
- generic `Resume` cannot resume a retry wait;
- terminal runs reject retry commands;
- expected-version checks remain unchanged.

### Reducer and replay tests

- legacy step events replay as attempt `1/1`;
- new completion and failure commands work on legacy active state;
- attempt-aware start, complete, and fail validate identity;
- retry schedule stores full context;
- retry resume recreates the same step with incremented attempt;
- sequence gaps and reordered multi-event batches fail;
- replay across several retries is deterministic;
- retry-limit stall is terminal;
- checkpoint hash is stable for the same retry state.

### Application and PostgreSQL tests

- command service commits failure/schedule batches atomically;
- command service commits resume/start batches atomically;
- stream version advances by two for each multi-event transition;
- projection status becomes `waiting_for_retry`;
- rollback leaves no partial retry state;
- concurrent resume commands produce one success and one conflict;
- projection deletion followed by recovery preserves retry context;
- checkpoint v2 round-trips retry state;
- migration removes v1 checkpoints but preserves events and stream heads;
- workspace RLS isolates retry streams and rebuilt projections.

### End-to-end acceptance

Mandatory flow:

```text
Create
→ MarkReady
→ Start
→ StartStep(max_attempts = 3)
→ FailStep(Retry at T1)
→ restore after simulated restart
→ reject ResumeStepRetry before T1
→ ResumeStepRetry at T1
→ fail attempt 2 with another retry
→ ResumeStepRetry at T2
→ complete attempt 3
→ delete projection
→ restore and rebuild
```

Assertions:

- all attempts use the same `step_id`;
- attempts are exactly `1, 2, 3`;
- no fourth attempt is possible;
- every state is reproduced by replay;
- rebuilt projection matches the pre-deletion projection;
- no event is duplicated;
- no partial multi-event transition is visible.

A second acceptance flow fails the final allowed attempt with `Retry` and verifies terminal `Stalled::RetryLimitExceeded`.

## 16. Exit conditions

P1 / PR2 is complete only when:

1. Step attempts have durable numbered identities.
2. Retry limits include the first attempt and are enforced deterministically.
3. Retryable failure and scheduling are one atomic command transition.
4. Due resume and next-attempt start are one atomic command transition.
5. Resume before the stored due time fails explicitly.
6. A crash after scheduling restores the exact retry wait.
7. A crash after resume restores the exact active attempt.
8. Retry-limit exhaustion stalls the run without creating another schedule.
9. Legacy step events remain replayable.
10. New completion and failure commands can operate on legacy active state.
11. Checkpoint format v2 is introduced without changing event history.
12. Projection rebuild remains exact after retry transitions.
13. Concurrency remains enforced by `run_streams`.
14. Workspace isolation and transaction rollback remain intact.
15. No scheduler, worker, HTTP, idempotency, subagent, compensation, capability, CAS, memory, or UI scope is introduced.
16. Formatting, Clippy, full Rust/PostgreSQL tests, CLI truthfulness, console, and clean Compose acceptance are green.
