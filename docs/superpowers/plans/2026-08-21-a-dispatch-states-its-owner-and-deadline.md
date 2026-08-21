# Goal

Let a dispatch say who owns it and by when it should be done, so that "this
dispatch is lost" becomes a fact somebody stated rather than a guess about the
clock.

The previous plan proposed adopting a lost dispatch and was refused on four
counts, the first of which is the root of the rest: **an external effect's
`Dispatching` transition carries no owner, no generation, no deadline and no
lease.** Run recovery has exactly these — `run_leases` has held `worker_id`,
`generation`, `heartbeat_at` and `lease_until` since migration 0021 — and
excludes live leases for that reason. Effects have nothing, so
`DISPATCH_CONSIDERED_LOST_AFTER` is five minutes of guessing, and both
`vestrace server` and `vestrace worker` call `run_startup_recovery`, so a
starting replica would be guessing about another live replica's in-flight call.

With a stated deadline the question stops being "how long is too long" and
becomes "did the process that took this on say it would be finished by now".
That is answerable, it is uniform across replicas, and it removes the need for a
startup special case entirely: an expired dispatch is expired whoever notices.

# Requirements

1. `external_effect_lifecycle_transitions` gains `dispatch_owner TEXT` and
   `dispatch_expires_at TIMESTAMPTZ`, both nullable.
2. Only a `dispatching` transition may carry them, and a new one must carry
   both. Expressed as a CHECK added **`NOT VALID`**, so rows written by 0153
   before this concept existed are exempt rather than retroactively declared
   malformed. This is deliberate and is the point of requirement 6.
3. `dispatch_owner` is the `WorkerId` of the process performing the dispatch —
   the same identity `run_leases.worker_id` records, not a second vocabulary for
   the same idea. It is supplied by the caller; the repository does not invent
   one, because a repository that names the owner is a repository asserting
   something it cannot know.
4. `dispatch_expires_at` is stated by the dispatcher from what it knows about
   how long the call may take. A caller with nothing better to go on uses a
   documented default, and that default's value is a statement about adapters in
   general rather than a threshold recovery picked.
5. The candidate query's lost-dispatch branch selects on
   `dispatch_expires_at < $cutoff` instead of `recorded_at < $cutoff`. The
   argument stops being `at - DISPATCH_CONSIDERED_LOST_AFTER` and becomes `at`:
   the sweep asks for dispatches whose own deadline has passed.
6. A `dispatching` transition with no deadline — every one written before this
   migration — **can never be declared lost by machine**. It is not swept, not
   adopted, and not quietly given a default, because nobody stated one. Such
   effects must be *countable*: something must be able to report how many exist,
   or the exemption becomes a silent leak.

# Non-goals

- **Adoption itself.** Nothing declares a lost dispatch `UNKNOWN` yet. That is
  the next slice, and it additionally needs an atomic claim (two workers must not
  both adopt), a rule for a real receipt arriving after adoption, and a candidate
  set that still contains crash-derived `UNKNOWN` so a failed read-back cannot
  strand the effect a second time. All four were BLOCKERs on the previous plan.
- **Startup recovery for effects.** With deadlines it is no longer a separate
  mechanism, and without adoption there is nothing for it to do.
- **Heartbeats or lease renewal.** A dispatch is one call, not a long-running
  claim. If a call legitimately outlives its deadline that is a fact worth
  seeing, not one worth extending silently.
- Fault points 2, 3 and 4 stay red. Release-gate producers stay absent.
- `ExternalEffectReceipt::synthetic_unknown` stays without a caller. Which
  mechanism adoption should use is genuinely open — see the correction below.

# A correction carried into this plan

The previous plan rejected `synthetic_unknown` on the grounds that fault point 3
expects `receipt_persisted: false` and therefore "the suite says so". That is
false. `evaluate_fault_suite` checks status, `retry_attempted`, and
`reconciliation_started` for point 3 only. **It never checks
`receipt_persisted`.** `FaultObservation::expected` declares the field and the
evaluator ignores it.

This is the second time these slices have read `FaultObservation::expected` as
if it were a specification. It is a structure the evaluator draws some of its
comparisons from, and the rest is documentation. The choice between a synthetic
receipt and a lifecycle transition has to be argued from the contract and from
what strands fewer effects — and there is a real argument for the synthetic
receipt, since it lands the effect in the existing unknown-receipt branch that
already works, rather than in a state nothing queries. That argument belongs to
the adoption slice, and this plan does not prejudge it.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- Transitions stay workspace-predicated explicitly, never on RLS alone.
- The owner is recorded, never derived. No `unwrap_or_else(WorkerId::new)`
  anywhere near this path: a freshly minted id would make every dispatch look
  owned by a process that has never existed.
- Both `ExternalEffectRepository` implementations get real behaviour.

# Acceptance criteria

1. A `Dispatching` transition written through the dispatch path carries the
   caller's `WorkerId` and a deadline strictly after the moment it was recorded.
2. Raw SQL inserting a `dispatching` row with either column NULL is refused;
   inserting a non-`dispatching` row with either column set is refused.
3. Rows written by migration 0153 survive the migration unchanged and unvalidated
   — asserted against a database holding such a row, not assumed from the
   `NOT VALID` keyword.
4. A dispatch whose deadline has not passed is **not** returned by
   `find_reconciliation_candidates`; one whose deadline has passed is. Asserted
   with two transitions of the same age and different deadlines, so the test
   cannot pass by accident of timing.
5. A `dispatching` transition with a NULL deadline is never returned as a lost
   candidate, at any cutoff.
6. Something reports the count of deadline-less dispatching transitions per
   workspace, and a test pins that it counts them.
7. Two dispatches from two different `WorkerId`s are distinguishable in storage.
8. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace` green against a real PostgreSQL 17 with pgvector,
   excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
9. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
10. The fault suite is run against a real ephemeral deployment and reported
    verbatim.

    **Prediction, to be verified rather than assumed:** no change. Nothing adopts
    a lost dispatch yet, so point 3 should still read `Dispatching`, and all five
    lines should match the previous run. A change would mean this slice did
    something it was not asked to do.

# Implementation plan

1. Migration `0154`: the two columns, the `NOT VALID` CHECK, and an index change
   so the lost-dispatch partial index covers `dispatch_expires_at`. Header
   comment in the style of 0144/0151/0153: what was wrong — a lost dispatch was
   a guess about the clock — and why `NOT VALID` rather than a backfill, which
   would mean inventing an owner and a deadline for calls nobody recorded.
2. Domain/application: the dispatch path carries `WorkerId` and a deadline into
   the transition. Inspect how `WorkerId` is threaded for runs and follow it
   rather than adding a parallel path.
3. Port: `record_dispatch_started` takes owner and deadline;
   `find_reconciliation_candidates` selects on the deadline; a way to count
   deadline-less dispatching transitions.
4. `MemoryEffectRepository` mirrors it.
5. `DISPATCH_CONSIDERED_LOST_AFTER` becomes the documented default deadline a
   dispatcher applies, not a cutoff recovery subtracts. If it survives at all,
   its doc comment must say which of the two it is.
6. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace` with `DATABASE_URL` pointing at a pgvector-enabled
  PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the migration applied against a database holding a 0153-era `dispatching` row,
  with that row asserted unchanged
