# A failure nobody could see

**Date:** 2026-08-14
**Scope:** outbox delivery failure, dead-lettering, forced row-level security on
`outbox`, and the subscriber wiring that was discarding most of this system's
log output.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice gave the outbox a drain and said plainly what it did not do:
no retry limit, no dead-letter path, a poison message retried forever. This slice
closes that, and in the course of proving it live found something larger — the
retries were producing no log output at all, and neither was anything else
outside the binary crate.

## Retrying forever is not resilience

`claim_pending` took the oldest pending messages. A handler that fails on every
pass leaves its messages pending, so a batch of thirty-two permanently failing
messages would be re-claimed, re-attempted and re-failed on every poll, forever,
while every message written after them waited behind them. The drain would report
itself as running the whole time.

Migration `0149` gives a message a delivery history: `attempts`,
`next_attempt_at`, `last_attempt_at`, `last_error`, `dead_lettered_at`. The claim
query now asks for messages that are pending, not dead-lettered, **and due**;
`record_failure` increments the attempt, stores the reason, and pushes the next
attempt out by 30s, 1m, 2m, 4m, capped at 8m. After five refusals the message is
set aside.

Three decisions worth stating:

- **The reason is stored on the row.** A dead letter whose cause existed only as
  a log line is one nobody can act on: the log has rotated by the time anyone
  reads the table. Bounded to 2000 characters, because a provider that returns a
  large body in its error would otherwise write it into every row it fails.
- **A dead letter is never marked processed and never deleted.** Those are
  contradictory claims about the same delivery, and a `CHECK` refuses them
  together. The row is the evidence that something the system undertook to do was
  not done.
- **Dead-lettering is an error, not a warning**, and has its own invariant.
  `outbox.backlog_within_budget` says delivery is late; `outbox.no_dead_letters`
  says delivery was abandoned. The backlog query now excludes dead letters, or
  the same message would be reported twice and the backlog could never reach
  zero.

## The policy that was decorative

`outbox` had row-level security `ENABLE`d and never `FORCE`d. The runtime role
owns the table, and ownership bypasses an unforced policy, so
`outbox_workspace_isolation` had been inert since the day it was written.

Forcing it required scoping `save`, which ran on a bare pool with no
`vestrace.workspace_id` set — forcing alone would have made writing a memory
fail. Both halves are in this change, which is the same lesson as
[`forcing-and-scoping-are-one-change`](documentation-gap-delta-2026-08-14-forcing-and-scoping-are-one-change.md).

**Twenty-eight tables still have `ENABLE` without `FORCE`**, including
`idempotency_keys`, `sessions`, `claims`, `tool_invocations` and
`cross_workspace_memory_grants`. Their policies are equally inert. Not addressed
here, and named so the number is on the record rather than implied.

## The larger thing: the log was throwing most of itself away

Verifying dead-lettering live meant pointing the worker at a model that does not
exist. The database showed exactly what it should — attempts climbing, the reason
stored, the message set aside at five. The log showed **nothing**. Not the
retries, not the dead letter.

The drain's own report line, emitted from the binary crate three statements
later, printed normally in the same iteration.

What was happening, and it is worth writing down because it is invisible by
construction:

- The subscriber attached its filter to the formatting layer with
  `.with_filter(...)`, which is a **per-layer** filter.
- A per-layer filter records its decision in a thread-local bitmap, written only
  when `Subscriber::enabled` runs.
- `enabled` does not always run. When a callsite's cached `Interest` is `always`,
  `tracing` skips it and calls `on_event` directly. `Filtered::on_event` then
  reads a bit nothing set for that callsite on that thread, finds it false, and
  drops the event.

The effect was not "no logs", which is why it survived: startup logged normally,
and so did the binary crate. What vanished was every `warn!` and `error!` in
`vestrace-application`, `vestrace-infrastructure` and `vestrace-http` emitted from
inside the async runtime — including `run worker iteration failed`,
`access token could not be resolved`, and every `internal application request
failed`. A previous slice found that the worker installed no subscriber at all
and fixed it; this is the same silence one layer down, and the fix looked like it
worked because the binary's own lines started appearing.

The filters are now **global** layers on the registry. They take no per-layer
state, they short-circuit the whole subscriber, and that is what they wanted
anyway: the target allowlist exists to clamp dependency output for everyone, not
for one layer.

**Honest about the test.** A unit test cannot reproduce this: installing a scoped
subscriber with `with_default` disables `Interest` caching, so `enabled` runs
every time and the failure cannot occur in-process. Every test written that way
passed while deployments logged nothing — including two I wrote while chasing
this. The behavioural test that remains asserts a library-target event is written
through the global dispatcher; the actual guard is a contract test asserting the
subscriber does not attach a per-layer filter, and the proof the fix works is
live.

## Live

**The same worker build, before and after the wiring change**, with a probe
warning emitted from `vestrace-application` inside the drain loop:

```text
before:  0 library warnings in the container log
after:  17
```

**Dead-lettering, end to end.** Worker pointed at `a-model-nobody-serves`, one
memory written over HTTP, then the backoff advanced by hand rather than waiting
eight minutes:

```text
attempts=1 dead=false  last_error="conflict: embedding space nomic-768 holds …"
attempts=2 dead=false
attempts=3 dead=false
attempts=4 dead=false
attempts=5 dead=true
```

Made due again afterwards, the dead letter stayed at `attempts=5`, unprocessed
and unclaimed.

**The log, for the first time:**

```text
WARN vestrace_application::outbox: outbox delivery failed and will be retried
  topic=memory.created message_id=01a00100-6512-… attempts=1
  error=conflict: embedding space nomic-768 holds text-embedding-nomic-embe…
```

**The doctor, with three invariants each saying a different thing:**

```text
outbox.no_dead_letters          error    3 messages abandoned … memory.created=3 (conflict: …)
memory.active_memory_is_embedded warning 3 active memories have no embedding
outbox.backlog_within_budget    warning  14 messages waited > 60s (event.recorded=14)
```

The second is the same three memories seen through independent evidence — the
embeddings table rather than the outbox — and the third now counts only the topic
that has no consumer.

**The documented remediation, executed.** The invariant says to read `last_error`,
fix what refused them, and clear `dead_lettered_at` to return them to the queue:

```text
UPDATE outbox SET dead_lettered_at = NULL, attempts = 0 …   →  3 rows
6 seconds later: memory.created pending=0, embeddings 9 → 12
```

Both findings then reported `not seen on this run`.

**Writing still works under the forced policy**: a memory created over HTTP was
recorded, delivered and embedded, with `attempts=0`.

## What this does not do

- **`healthy` does not return to true when the cause is gone.** Both findings
  above report `observed_on_this_run: false` and remain `open`, and
  `MonitoredFinding::is_error` looks only at severity and silencing — so one
  transient error-severity finding pins the summary to unhealthy until somebody
  dispositions it. There is a real argument for that (a finding is a durable
  record, and `FindingDisposition` exists precisely to close one deliberately)
  and a real argument against (a signal that can never recover stops being read).
  Left as it is rather than changed in passing, because it is a design decision
  and not an oversight. If it were mine to decide, `healthy` would follow what is
  currently observed and the finding would stay open regardless.
- **`event.recorded` and `memory.relation.linked` still have no consumer.**
  Fourteen pending messages, correctly reported.
- **Twenty-eight tables still have unforced row-level security.**
- **The root cause of the `Interest`/per-layer-filter interaction is
  characterised, not fully explained.** I can state which wiring drops events and
  which does not, and demonstrate both live; I cannot say why the sqlx callsites
  were re-evaluated per event when the filter should have cached them as
  `never`. The fix does not depend on that answer, and the contract test guards
  the wiring rather than the theory.
- **No vector index still.**

## Test results

Nine unit tests now cover the dispatcher against fakes, five of them new: a
failure records its reason and its backoff, a failed message is not immediately
reclaimed, a message is dead-lettered once its attempts are spent, a dead letter
is not claimed again, and backoff grows to a cap. The unhandled-topic test was
verified to fail by making the dispatcher acknowledge unhandled messages.

Full workspace suite against a live PostgreSQL 17: **868 passed, 0 failed,
exit=0**.
