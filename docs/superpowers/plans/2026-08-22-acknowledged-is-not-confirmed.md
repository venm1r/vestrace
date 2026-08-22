# Goal

Stop treating a provider's acknowledgement as a confirmed business outcome.

§34 of the external-effects contract lists the forbidden shortcuts and the first
line is:

```text
HTTP success == business outcome
```

ADR-0005 rejects the same thing as alternative 4, *"treat provider ACK as
guaranteed desired business outcome"*. §11 states it positively — `COMMITTED` in
the transport sense means request dispatch, not a confirmed business outcome —
and puts `CONFIRMED / RECONCILING / UNKNOWN` **after**
`ACKNOWLEDGED / FAILED / UNKNOWN`.

The domain already says the right thing: `ExternalEffectReceipt::is_business_confirmation`
returns `false` unconditionally, and two conformance cases plus
`tests/e1_e4_connect.rs` assert it. The rule is stated and guarded one layer up,
and contradicted one layer down by `r.outcome_status = 'unknown'` in the
candidate query: an acknowledged effect is never asked about again, so nothing
ever establishes whether the thing the user wanted actually happened.

Four slices were spent making this safe to do. Read-back is routed to the adapter
that dispatched; the sweep is bounded; an effect nobody can ask about backs off
instead of starving the queue; and read-back now carries the provider's own
identifiers so the question it asks is one the provider can answer precisely.
Without the last of those, confirming an acknowledgement would have meant asking
with our identifier about the provider's resource.

# Requirements

1. An acknowledged receipt with no settled reconciliation is a reconciliation
   candidate, on the same terms as an unknown one: the existing unsettled-retry
   cutoff governs re-asking, and a settled reconciliation removes it from the set.
2. **Only for adapters that declare `supports_read_back`.** That field has been
   in `ExternalEffectAdapterDescriptor` since the beginning and has never been
   read in production — the fourth declared-and-unused affordance this subsystem
   has turned up. This is what it was for. An adapter that does not declare it
   cannot have its outcomes confirmed by machine, which is §18's step 8 and not
   something to pretend otherwise about.
3. `ExternalEffectReconciliationService::reconcile` accepts an `Acknowledged`
   receipt. It still accepts `Unknown` and no receipt, and still refuses
   `Failed` — a dispatch the provider rejected was settled by the provider.
4. A confirming read-back settles to `Confirmed`; one that answers "not applied"
   settles to `NotApplied`; one that cannot tell stays `Inconclusive` and the
   effect remains a candidate under the existing cutoff.
5. The partial index `idx_external_effect_receipts_unknown` covers
   `outcome_status = 'unknown'` only. The widened predicate needs an index that
   serves it, or the query falls back to a workspace-wide scan on the largest
   table in this subsystem.

# Non-goals

- **A minimum evidence strength for settling.** Still open, still not stated
  normatively, still recorded rather than inferred.
- **Scoping confirmation by risk or reversibility.** §20.4 says risk policy is
  strengthened for `IRREVERSIBLE`/`UNKNOWN`, which reads like an argument for
  confirming those harder rather than confirming others less. Turning that into a
  rule is a policy decision, not a reading.
- **A uniqueness claim on reconciliation.** Two workers can already both reconcile
  one unknown effect and insert two reconciliations; this makes it more frequent,
  not newly possible. Delivery is already at-least-once by design (0152), and
  read-back is an observation that the fault suite pins as not moving the adapter
  stub's dispatch count. Worth a slice, not this one.
- **Escalation to human verification** for what cannot be confirmed.
- Worker liveness, **`Authorized`** (fault point 2), the `NOT VALID` exemption
  debt, release-gate producers. Fault points 2 and 3 stay red.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- Both `ExternalEffectRepository` implementations honour the widened predicate.
- The workspace predicate, the deadline comparison, the crash-derived `Unknown`
  branch, the failed-attempt exclusion and the batch limit all keep their current
  meaning.
- **This changes what a running deployment does to its providers.** Every
  acknowledged effect on a read-back-capable adapter becomes a candidate,
  including every one already in the database. The bounded sweep and the
  failed-attempt backoff pace that, and the delta must state the consequence
  plainly rather than let an operator discover it.

# Acceptance criteria

1. An acknowledged receipt with no reconciliation is returned as a candidate; one
   whose reconciliation settled is not; one whose reconciliation settled nothing
   returns only after the unsettled cutoff. The same three cases already asserted
   for unknown receipts.
2. An acknowledged effect whose adapter does not declare `supports_read_back` is
   **not** returned as a candidate, and a test would fail if the declaration were
   ignored.
3. `reconcile` accepts `Acknowledged`, still accepts `Unknown` and no receipt,
   and still refuses `Failed`.
4. A sweep over an acknowledged effect produces a reconciliation whose outcome
   follows the provider's answer — `Confirmed` when applied, `NotApplied` when
   not, `Inconclusive` when the provider cannot tell — carrying the evidence
   strength the observation reported.
5. Read-back over an acknowledged effect performs no dispatch: the adapter stub's
   dispatch count is unchanged across the sweep.
6. The widened query uses an index rather than scanning, demonstrated against
   PostgreSQL with a population large enough for the planner to care.
7. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
8. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
9. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** Fault point 4 crashes after `insert_receipt`
   with an acknowledged receipt, and the scenario then runs recovery. That effect
   should now be a candidate, be read back against the stub, settle `Confirmed`,
   and gain a `Reconciling` transition — while outcome delivery defers, because
   the scenario's run has no event stream. `observe` should therefore report
   `status=Reconciling receipt_persisted=true reconciliation_started=true`, which
   is what point 4 expects on every field.

   **If that holds, point 4 turns green and the suite goes to three failures** —
   the first point to turn since point 5, and the first turned by fixing a named
   contract violation rather than by adding evidence.

   Two of six predictions across these slices were wrong, both by reasoning about
   what the system ought to do rather than reading what it does. This one is
   derived from the code path and is still to be checked, not believed.

# Implementation plan

1. Domain: the receipt predicate that distinguishes "settled by the provider"
   from "acknowledged but unconfirmed". Extend `requires_reconciliation`'s
   neighbourhood rather than duplicating it — inspect what it does before
   changing it.
2. Application: `reconcile`'s receipt rule; the candidate set widened; the
   `supports_read_back` filter applied where the descriptor is available, which
   is the service and not SQL — the descriptor is not in the database and putting
   it there would be a second source of truth.
3. Migration `0157`: an index serving the widened predicate. Header comment
   explaining what the old partial index covered and why it no longer suffices.
4. `MemoryEffectRepository` mirrors the widened predicate.
5. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
