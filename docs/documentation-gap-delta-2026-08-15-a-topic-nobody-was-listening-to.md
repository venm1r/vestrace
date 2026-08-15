# A topic nobody was listening to

**Date:** 2026-08-15
**Scope:** the two outbox topics with no consumer, the stub that would have been
reached for to give one of them a consumer, and the guard that stops the next
one.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Since the outbox drain was built, the deployment's health has carried a warning
that nothing could clear:

```text
outbox.backlog_within_budget | warning | 24 messages waited > 60s (event.recorded=24)
```

Every delta since has repeated the same line — "`event.recorded` and
`memory.relation.linked` have no consumer" — and moved on. The drain reports them
as `unhandled` and leaves them pending, which is correct behaviour and not an
answer.

## The decision, made

An outbox message exists because something is waiting to act on it. These two had
nothing waiting, and nothing to say that the tables do not: `events` is the
record of an event, `knowledge_relations` is the record of a link. The outbox is
not a second event log.

So the producers are gone. The write path no longer promises a delivery it never
intended to make.

**The alternative was considered and is worse.** The obvious way to give
`event.recorded` a consumer is `MemoryExtractor` — and the only implementation
was `DeterministicExtractor`, whose entire body returns the literal string
`"Extracted fact from event {id}"` for any event whose type starts with `fact.`.
Its own comment said "for test suite" while it sat in the library beside the
trait. Wiring it would have filled a workspace with placeholder memories that
look exactly like real ones — confidence 0.9, importance 0.8, indexed,
embedded, retrievable.

It is deleted, along with the test that asserted the stub does what the stub
does. `MemoryExtractor` and `ExtractedCandidate` remain: an extension point with
no implementation is honest, a stub named as though it were one is not.

## The guard

A contract test now reads the topics out of the memory service's source and
requires each to appear in the worker's handler registration. Verified failable
by re-adding the `memory.relation.linked` producer:

```text
the write path produces outbox topic `memory.relation.linked` and the worker
registers no handler for it, so every message on it would sit undelivered forever
```

That is the point where the decision is cheap. Once the producer ships, the
choice is between an unconsumed backlog and a consumer written to justify it.

## Live

```text
pending before:  event.recorded=24
record an event, write two memories
pending after:   event.recorded=24
```

The two memories were delivered and embedded; nothing new was written to a topic
nobody consumes.

**The historical rows stay**, and the finding they produce was answered rather
than deleted — using the disposition surface from two slices ago, against its
first real case:

```json
{"kind":"accepted_risk",
 "reason":"24 event.recorded rows predate the removal of that producer; the topic
           had no consumer and never will, and the rows are kept as evidence of a
           promise the system used to make",
 "expires_at":"2026-11-15T00:00:00Z"}
```

```text
healthy: True
 - outbox.no_dead_letters           | error   | suppressed
 - outbox.backlog_within_budget     | warning | accepted_risk
 - memory.active_memory_is_embedded | warning | accepted_risk
```

Three findings, three decisions, each with an author, a reason and an expiry.
The summary is green because somebody said so, not because the system decided
nothing was wrong.

## What this does not do

- **It deletes a capability, not just a defect.** Anything that wanted to react
  to an event or a link now has nothing to subscribe to. That is the right
  trade while nothing does — the hook cost a permanent false alarm and bought
  nothing — but re-adding it is a decision somebody may have to reverse.
- **The 24 rows are still there.** They are undeliverable by construction now,
  and nothing marks them so. Dead-lettering them would be a lie in the other
  direction: they never failed delivery, they were never attempted.
- **Extraction remains unbuilt.** Deleting the stub does not bring the real
  thing closer; it stops the fake one being mistaken for progress. Turning
  events into candidate memories is a genuine gap with a genuine design behind
  it, and it is still empty.
- **The guard reads source text.** A topic built by string concatenation, or
  produced anywhere but the memory service, slips past it.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**. One new contract test, verified failable; one test deleted, because it
tested a stub against itself.
