# An acknowledgement is not an outcome

**Date:** 2026-08-22
**Scope:** whether a provider saying "received" ends the question.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. **Fault point 4 turns green** — the first point to turn since
point 5, and the first turned by repairing a named contract violation rather
than by adding evidence the system was not recording.

```text
fault suite (real ephemeral deployment): FAILED — 3 failures, down from 4; points 1, 4 and 5 agree
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
```

## The violation

§34 of the external-effects contract lists the forbidden shortcuts. The first
line is:

```text
HTTP success == business outcome
```

ADR-0005 rejects the same thing as alternative 4. §11 states it positively —
`COMMITTED` in the transport sense means request dispatch, not a confirmed
business outcome — and puts `CONFIRMED / RECONCILING / UNKNOWN` after
`ACKNOWLEDGED / FAILED / UNKNOWN`.

The system committed it. `HttpWebhookEffectAdapter` maps 2xx to
`AdapterOutcome::Acknowledged`, `from_dispatch` maps that to
`EffectLifecycleStatus::Acknowledged`, and the candidate query filtered
`r.outcome_status = 'unknown'`. So an acknowledged effect was never asked about
again, and nothing ever established whether the thing the user asked for actually
happened.

What made it a defect rather than a design choice is that the domain already said
otherwise. `ExternalEffectReceipt::is_business_confirmation` returns `false`
unconditionally, and three places assert it. The rule was stated and guarded one
layer up, and contradicted one layer down by a SQL predicate.

## What it took to get here

Four slices, none of which was this one:

- **Read-back routed to the adapter that dispatched.** Without it, confirming an
  effect could have asked the wrong provider and recorded its answer.
- **A bounded sweep.** Confirmation multiplies the candidate population; an
  unbounded query in a 500 ms loop would have become catastrophic rather than
  merely wrong.
- **Backoff for effects nobody can ask about.** Bounding without it starved the
  queue, and confirmation makes unaskable effects common.
- **Read-back carrying the provider's own identifiers.** An acknowledged receipt
  is precisely the case where the provider handed us its identifier, and
  confirming by asking with ours would have been asking a question it is under no
  obligation to answer precisely.

Each was found by trying to do this one and discovering the ground was missing.

## What changed

An acknowledged receipt with no settled reconciliation is now a candidate, on the
same terms as an unknown one. `reconcile` accepts it; `Failed` stays refused,
because a dispatch the provider rejected was settled by the provider.

Confirmation runs **only for adapters that declare `supports_read_back`**. That
field has been on `ExternalEffectAdapterDescriptor` since the beginning and had
never been read in production — the fourth declared-and-unused affordance this
subsystem has turned up, after `synthetic_unknown`, the deadline-less transition
count, and the two components `worker.rs` admits were "both written and neither
constructed". This is what it was for.

The capability is checked in the service, not in SQL, and migration 0157 says why
in its header: the descriptor belongs to the configured adapter, not to
historical evidence, and storing it beside the receipt would create a second
source of truth that could silently starve eligible candidates before the
failed-attempt backoff could work.

The partial index from 0128 covered `outcome_status = 'unknown'` only. It is
replaced by one covering both unsettled transport states, keyed workspace-first to
match the workspace-scoped, oldest-first access path the bounded sweep uses.

## Verification, and three tests that had to change

The implementing agent stalled before finishing — the fourth this session — so
everything below was produced by running it here against a PostgreSQL 17 with
pgvector. Its incomplete run had left three failures it never saw.

**Point 4 turns green, exactly as predicted.** It crashes after `insert_receipt`
with an acknowledged receipt; recovery now finds it, reads back against the stub,
settles `Confirmed`, writes a `Reconciling` transition, and delivery defers
because the scenario's run has no event stream:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=3
```

Three tests failed and were dealt with differently, which is the part worth
recording.

**One was a defect in a test, not in the code.** The planner test's helper
required a single plan node to carry both `Relation Name: external_effect_receipts`
and the index name. PostgreSQL chose a bitmap shape, which splits them — the
`Bitmap Heap Scan` names the relation, its child `Bitmap Index Scan` names the
index — so the conjunction reported "no index used" about a plan whose own text
names the index. The query was right; the detector was wrong, and now looks for
the index beneath the relation, where the planner actually puts it.

**Two were assertions that this slice deliberately inverted.** Both encoded "an
acknowledged effect is not a candidate", which was true and is now false by
design. They were **inverted rather than deleted**, each keeping its real
subject: one still pins that an acknowledged receipt is reachable by effect id,
the other that a late real receipt retires the lost-dispatch guess — now
expressed as the candidate carrying a receipt rather than arriving from the
receipt-less path. Deleting an assertion that has become inconvenient removes
protection; inverting it with the reason written down keeps the property pinned
from the side it now falls on.

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from the pre-existing `authorized_shared_reader_uses_exact_revision_under_forced_rls`.

## What this changes for a running deployment

**Every acknowledged effect on a read-back-capable adapter becomes a candidate,
including every one already in the database.** That is the point — none of them
has ever been confirmed — but it means a deployment upgrading to this will read
back its entire acknowledged history. The bounded sweep paces it at eight per
tick per workspace and the failed-attempt backoff keeps unreachable providers
from monopolising that budget, but the traffic is real and an operator should not
have to discover it.

## What this does not do

**Fault points 2 and 3 stay red.** Authorization persists nothing, so `Authorized`
is not a state any reader can see. A lost dispatch becomes `UNKNOWN` only once
its deadline passes, and the scenario sweeps seconds after the crash — closing
that needs worker liveness, so that a dispatch owned by a provably dead process
is lost at once.

**There is still no minimum evidence strength for settling.** A positive answer
confirms regardless of how weak the evidence was. §20.4 strengthens risk policy
for `IRREVERSIBLE`/`UNKNOWN` reversibility without stating a floor, and inferring
a normative requirement from a suggestion is how two earlier claims in these
deltas turned out wrong.

**Two workers can still both reconcile one effect.** Read-back is an observation
and the suite pins that it never moves the adapter stub's dispatch count, so the
cost is duplicated provider reads rather than duplicated effects — but the
population this slice creates makes it more frequent.

The `NOT VALID` exemption debt is open, §18's rank 1 is still not offered, and
the release gate still reports four evidence families with no producer.
