# Telling the run what happened

**Date:** 2026-08-15
**Scope:** delivering a settled external-effect outcome into the history of the
run that asked for it.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate:   192 passed (184 executed, 8 attested), 0 failed, 7 skipped — unchanged
suite:  906 → 909 passed, 0 failed
```

## The gap, and why it took four slices

An effect that times out gets an `unknown` receipt and the run carries on
without knowing. The reconciliation sweep later asks the provider and finds out
— and wrote the answer to a table nobody joined against. `NotApplied` means the
system dispatched something that did not happen, and no run, no surface and no
operator was told.

Three consecutive deltas ended by naming this and deferring it. Each deferral
was honest, and each removed one obstacle: the sweep had to stop forgetting
unsettled effects, and an effect had to be able to say which run asked for it
before anything could be told. This is the delivery.

## The decision worth defending: a debt, not a write

Appending to the run's event stream and inserting the reconciliation touch two
different aggregates and cannot share a transaction. Either order loses
something:

- **insert then append** — a failed append leaves an effect that is *settled*,
  and therefore out of the sweep, attached to a run that never learned. A silent
  lost update.
- **append then insert** — a crash between them duplicates the run event.

So they are separated by a marker instead. `external_reconciliations` gains
`notified_at`, and a settled outcome stays **owed** to its run until something
records that it was delivered. A failed append leaves the debt outstanding and
the next pass retries it. A crash after appending but before marking re-appends
— at-least-once, the same guarantee the outbox gives, and the effect id in the
event lets a reader collapse duplicates.

This is why the delivery pass is separate from the sweep rather than a step
inside it. A sweep that also delivered would have to choose one of the two lossy
orders.

## What the event does, and does not, do

`LegacyRunEvent::ExternalEffectSettled` carries the effect, the receipt, the
outcome and the evidence strength. The reducer arm is deliberately **unguarded**,
unlike every other arm: a reconciliation settles long after the dispatch, often
after the run has finished, and that late answer is the entire reason the event
exists. Guarding it on `Running` would make the one case it is for the one case
it rejects, and replay of a real stream would fail with `InvalidTransition` on a
fact that is simply true.

It changes no state. Whether a `NotApplied` effect should reopen a succeeded run
is a policy question nobody has answered, and answering it here by silently
mutating status would be the wrong way to ask. The run's history now says what
was learned; deciding what to do about it is left to a reader.

`occurred_at` is the observation's time, not the append's. The two differ by
however long the run was unreachable.

Only **settled** outcomes are delivered. "We asked and could not tell" is the
absence of a fact, not a fact, and writing it into a run's history on every
sweep would be noise.

## Which event model, and why

The persisted stream is `LegacyRunEvent`; `RunEventPayload` is a separate
canonical model the Postgres store does not write. I asked rather than guessed,
because adding to the wrong one produces an event nobody stores or nobody reads.
`LegacyRunEvent` was chosen: the fact lands where replay and readers already
look, and can be carried across if the canonical model takes over.

## Evidence

Three tests against real PostgreSQL — the property is about two tables and an
optimistic append, none of which a fake would model honestly:

- a settled outcome appears in the run's history naming the effect and the
  outcome, and a second pass does not append it again;
- an outcome belonging to no run closes its debt as `unattributable` rather than
  being counted as delivered or retried forever;
- an inconclusive outcome is not delivered at all.

Mutation-proved by making `mark_outcome_delivered` never mark: two tests failed,
with *"the same outcome was appended to the run twice"* and *"the debt was
retried forever"* — the two hazards the marker exists to prevent. Restored with a
forced rebuild.

Live, on the deployed stack: migration `152 | a settled outcome is owed to its
run | t`, and the worker's first pass reported

```text
settled external effect outcomes were recorded in their runs
  workspace=10000000-…  delivered=0  unattributable=2  deferred=0
```

Both pre-existing reconciliations carry the free-string execution references this
system accepted until yesterday, so no run resolves from them. They were closed
as unattributable — not counted as delivered — once, and not retried. That is
the honest outcome for effects that genuinely cannot be attributed, and it needed
no backfill to reach.

The migrations guard from the previous slice passes on 0152, which adds a
nullable column and performs no `UPDATE`, so it does not need to lift the forced
policy.

## What this does not do

- **Nothing reads the event.** The fact is in the run's history and no surface
  shows it, no projection counts it, and nothing alerts on `NotApplied`. An
  operator would have to read the stream. This is delivery, not presentation.
- **No run changes state.** By design, argued above — but it means a run that
  succeeded on the strength of an effect that never happened still reads as
  succeeded, now with a contradicting fact beside it. Reconciling *that* is a
  policy decision nobody has made.
- **The three runs stuck at `ReconciliationRequired` are still stuck.** They are
  a different mechanism — startup recovery classifying runs whose own outcome is
  unknown — and this touches none of it.
- **Delivery is per-workspace and unbounded in age.** A debt owed for a month is
  delivered whenever it is noticed, with no expiry and no dead-lettering. The
  outbox has both; this has neither.
- **`deferred` is only a counter.** A run whose append keeps conflicting is
  retried forever and nothing escalates. In practice the conflict window is
  small, but "small" is not "bounded".
- **The batch is 32 per workspace per pass** and there is no ordering guarantee
  across workspaces.

## Test results

Full workspace suite against a live PostgreSQL 17: **909 passed, 0 failed** (906
before). Conformance gate: **199 total, 192 passed (184 executed, 8 attested), 0
failed, 7 skipped** — unchanged.

Remaining skips are unchanged: CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010,
REC-016, RET-004.
