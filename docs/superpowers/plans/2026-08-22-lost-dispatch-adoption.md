# Goal

Declare a lost dispatch `UNKNOWN`, so recovery can ask about it.

This closes finding 2 of `docs/documentation-gap-delta-2026-08-20-a-fault-nobody-had-injected.md`:
a crash between `dispatch` returning and `insert_receipt` committing leaves an
effect no sweep can see. §26 of the external-effects contract states the
required behaviour verbatim — `after dispatch / before receipt → UNKNOWN →
reconcile` — and §17 names a crash between provider response and persistence as
a cause of `UNKNOWN`.

Three slices have built the machinery for this without doing it: the lifecycle
transitions that can carry the state, read-back routed to the adapter that
actually dispatched, and a dispatch that states its owner and deadline so
"lost" is a checkable promise rather than a guess. This is the slice those were
for.

# The two hazards this must not create

An adversarial review refused the earlier attempt at this partly because
adoption introduces failure modes that did not exist before. Both are addressed
by the same observation: **a lifecycle's order is the order things happened to
the record, and the record does not currently know that order.**

**A late receipt would be filed and ignored.** `insert_receipt` writes its
transition with `receipt.recorded_at()`, which is stamped *before* the adapter
is called, while adoption's timestamp is necessarily later. "Latest transition"
orders by `recorded_at DESC`. So a real provider answer arriving after adoption
inserts successfully, sorts *earlier* than the adoption that superseded it, and
never becomes the effect's state. A genuine answer from the outside world would
sit in the database, unread, behind a guess.

**Two workers would both adopt.** Candidate discovery commits before adoption,
so two sweeps can hold the same `Dispatching` candidate, both append `Unknown`,
both query the provider, and both persist separate reconciliations.

# Requirements

1. `external_effect_lifecycle_transitions` gains `ordinal BIGSERIAL`. Every read
   of "the latest transition" orders by `ordinal DESC` rather than by
   `recorded_at`. `recorded_at` keeps its meaning — when the thing described
   happened, as stated by whoever knew — and stops being asked to carry an
   ordering it cannot.
2. The port exposes the identity of the `Dispatching` transition a candidate was
   selected on, and adoption's `cause_ref` is that transition's `id`.

   This corrects a false premise in the first draft of this plan, which claimed
   `cause_ref` "already names the specific `Dispatching` transition". It does
   not. `record_dispatch_started` writes `id::text` — the **effect** id — so
   every dispatch of one effect carries the same `cause_ref`, and an index keyed
   on it could not tell one attempt from another. The transitions table has
   given every row a `gen_random_uuid()` primary key since 0153; nothing has
   ever returned it.

3. A partial unique index admits at most one adoption per dispatch attempt:
   unique on `(workspace_id, effect_id, cause_ref)` where `cause = 'dispatch_lost'`.
   With requirement 2 this genuinely means one adoption per attempt, so a future
   re-dispatch can be adopted on its own merits while a second adoption of the
   same one is refused.

   **This is the atomicity primitive, and it is the only one available.**
   `begin_scoped` sets no isolation level, so transactions run at PostgreSQL's
   default `READ COMMITTED`. A conditional insert — "adopt only if `Dispatching`
   is still the latest transition" — is therefore *not* safe: two adopters can
   both observe `Dispatching` as latest and both insert. The loser must learn it
   lost from a unique violation, not from a check it performed earlier and no
   longer holds.
4. Recovery adopts a lost dispatch: a candidate with no receipt whose latest
   transition is an expired `Dispatching` gets an `Unknown` transition with
   cause `dispatch_lost` and `cause_ref` naming that `Dispatching`.
5. Adoption happens **before** read-back and does not depend on it. §26 does not
   make the state contingent on the provider answering. An effect whose adapter
   has no registered route, or whose provider is unreachable, is still `UNKNOWN`
   — it is reported through the existing `unreachable` channel and stays a
   candidate.
6. Requirement 5 is only true if a crash-derived `UNKNOWN` remains discoverable.
   The candidate query's receipt-less branch currently matches only a latest
   `Dispatching`; once adoption makes `Unknown` latest, that branch stops
   matching. It must also match a latest `Unknown` with cause `dispatch_lost`
   and no receipt, or adoption strands the effect in exactly the invisibility
   this slice exists to end.
7. A receipt arriving after adoption is **kept, not refused**. It is a real
   answer from the outside world and discarding it would be worse than the
   confusion it causes.

8. **A receipt-derived transition outranks a `dispatch_lost` one by kind, not by
   position.** Requirement 1 alone is not enough for requirement 7, and the
   reason is worth stating because it is the kind of thing that looks settled and
   is not. A `BIGSERIAL` is allocated at `INSERT` and becomes visible at
   `COMMIT`, so a transaction that started earlier can commit later carrying a
   *lower* ordinal. `insert_receipt` runs the moment the adapter returns, and
   adoption fires once the deadline has passed — which is precisely when a slow
   call is still outstanding. The two can interleave, and ordering by ordinal
   alone would let a guess outrank a provider's actual answer.

   So when both exist for one effect, the receipt wins regardless of ordinal. The
   ordinal orders everything else, where no such asymmetry exists. This is a
   statement about evidence, not about clocks or sequences: a record of what came
   back is worth more than a record of nobody having heard.

# Why a transition and not `synthetic_unknown`

The domain offers `ExternalEffectReceipt::synthetic_unknown`, with no production
caller, and it would land the effect in the existing unknown-receipt branch,
making requirement 6 unnecessary. The previous plan rejected it for a reason
that was **false** — it claimed the fault suite requires `receipt_persisted:
false`, and `evaluate_fault_suite` never reads that field.

It is still rejected, for a reason that survives inspection. A synthetic receipt
and a late real receipt would coexist as two rows for one effect. The synthetic
one carries `outcome_status = 'unknown'` forever, so the effect would remain in
the unknown-receipt candidate set permanently — asked about again and again
despite an answer already sitting beside it. The transition is superseded by a
later transition; a receipt is not superseded by anything.

# Non-goals

- **Confirming acknowledged effects.** §34's forbidden shortcut stands and fault
  point 4 stays red.
- **`Authorized`.** Fault point 2 stays red.
- **§18's evidence ladder.** Read-back still asks by our own identifier.
- Backoff, escalation to human verification, or a durable queue for effects that
  cannot be confirmed. An effect that stays unsettled stays a candidate under the
  existing retry cutoff, which is today's behaviour and not made worse here.
- Release-gate producers.
- **Repairing `dispatch_started`'s own `cause_ref`.** It holds the effect id,
  which the row already carries in `effect_id`, so for that one cause the field
  carries nothing. The 0153 plan said it would hold the intent's idempotency key
  and the implementation wrote the effect id; the review that accepted it — mine
  — did not catch the drift, and the delta then described `cause_ref` as "the id
  of the record that is the evidence", which for this cause is not true. There is
  arguably no such record: a dispatch beginning is evidenced by the transition
  itself. Recorded here so the claim is not repeated, and left alone because
  changing it means rewriting rows that describe calls already made.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- Adoption must not dispatch anything. A retry counted at any fault point is a
  suite failure at every point.
- The `ordinal` change touches every "latest transition" read. Find them all;
  a reader left ordering by `recorded_at` would disagree with the rest about what
  state an effect is in, which is worse than the ordering bug being fixed.
- Both `ExternalEffectRepository` implementations get real behaviour.

# Acceptance criteria

1. An effect whose latest transition is an expired `Dispatching` with no receipt
   is adopted: exactly one `Unknown` transition, cause `dispatch_lost`,
   `cause_ref` naming the `Dispatching`. A dispatch whose deadline has not passed
   is not adopted.
2. Two concurrent adoptions of the same dispatch produce exactly one transition
   and exactly one reconciliation. Asserted with two real sweepers against
   PostgreSQL, not by reasoning about the index.
3. Adoption survives read-back failure and a missing adapter route: the effect is
   `UNKNOWN`, appears in `unreachable`, and is returned again by the next sweep.
4. A receipt inserted after adoption becomes the effect's state, and the effect
   leaves the receipt-less candidate set. This test must fail if ordering reverts
   to `recorded_at`.
5. The same holds when the receipt's transition carries a **lower** ordinal than
   the adoption — the interleaving requirement 8 exists for. Asserted by writing
   the two transitions in that order deliberately, not by hoping a race produces
   it.
6. Existing transitions written by 0153 and 0154 receive ordinals in insertion
   order and no row is otherwise modified.
7. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
8. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
9. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, partial and to be verified.** Point 3's dispatch is seconds old
   and `DEFAULT_DISPATCH_ALLOWANCE` is five minutes, so its deadline has **not**
   passed when the scenario sweeps. Adoption should therefore not fire and point
   3 should still read `Dispatching`, unchanged. If that is what happens, this
   slice closes the production defect while the suite cannot show it, and the
   remaining gap is that nothing runs an effects sweep at startup — where a
   deadline is no longer needed, because a process that is gone cannot still be
   dispatching. That is the next slice and it is now safe to write, which it was
   not before ownership existed.

   No prediction is offered for what point 3 would read if adoption did fire.
   Two of the four predictions made across these slices were wrong, both times
   by reasoning about what the system ought to do instead of reading what it
   does.

# Implementation plan

1. Migration `0155`: `ordinal BIGSERIAL`, the partial unique index for
   `dispatch_lost`, and a header comment in the style of 0144/0151/0153/0154
   explaining that caller-supplied timestamps cannot order a lifecycle and that
   the unique index is where two racing adopters are resolved.
2. Repository: every latest-transition read orders by `ordinal`.
3. Port and service: adoption, before read-back, with the loser of a race
   returning something the sweep can classify rather than an error it reports as
   a provider failure.
4. Candidate query: the receipt-less branch also matches a latest
   `dispatch_lost` `Unknown`.
5. `MemoryEffectRepository` mirrors it, including the uniqueness.
6. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the two-sweeper concurrency test from criterion 2, run against PostgreSQL
