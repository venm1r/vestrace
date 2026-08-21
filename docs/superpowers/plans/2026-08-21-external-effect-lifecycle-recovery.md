# Goal

Make an external effect that left the process findable again.

Today a crash between `dispatch` returning and `insert_receipt` committing makes
the effect **invisible to recovery forever**. That is finding 2 of
`docs/documentation-gap-delta-2026-08-20-a-fault-nobody-had-injected.md`, and it
is structural: `find_reconciliation_candidates` joins
`external_effect_receipts`, so an effect with no receipt row is dropped before
any status filter runs. No value of `outcome_status` and no cutoff can bring it
back. This is precisely the crash the external-effects design exists to survive.

It cannot be fixed on its own. There is nothing in the database to find, because
nothing is written between the intent and the receipt — which is finding 5: the
system persists no lifecycle status for an effect at all. So this slice writes
the missing evidence and then teaches recovery to use it.

The claim underneath the whole change: **a reconciliation is about a dispatch,
not about a receipt.** When something was dispatched and nothing came back,
there is a dispatch to reconcile and no receipt to name.

# Requirements

## 1. Lifecycle transitions, append-only, each naming its cause

New table `external_effect_lifecycle_transitions`:
`(id, effect_id, workspace_id, status, cause, cause_ref, recorded_at, created_at)`,
composite foreign key `(effect_id, workspace_id)` onto
`external_effect_intents (id, workspace_id)`, `ENABLE`/`FORCE ROW LEVEL SECURITY`
plus the workspace-isolation policy every other table in 0144 carries.

Append-only, not a mutable column on the intent. The stored intent is immutable
evidence — `PerformExternalEffectService`'s own doc comment and
`crates/vestrace-infrastructure/tests/external_effect_repository.rs` both say so
— and a mutable projection does not belong on the same row.

Every transition names what caused it, so that no status is an unqualified
assertion. `cause_ref` is the id of the record that is the evidence:

| status | cause | cause_ref |
|---|---|---|
| `Prepared` | `intent_recorded` | the intent id |
| `Dispatching` | `dispatch_started` | the intent's idempotency key |
| receipt's `outcome_status` | `receipt_recorded` | the receipt id |
| `Reconciling` | `outcome_settled` | the reconciliation id |
| `Confirmed` / `Failed` | `outcome_delivered` | the reconciliation id |

**No backfill.** Effects that already exist have no transitions and
`find_lifecycle_status` returns `None` for them, which is the true answer:
nobody recorded where they were. This is also why this migration does not have
to lift `FORCE ROW LEVEL SECURITY` the way 0151 did — there is no `UPDATE` that
would silently see zero rows under a forced policy it has no workspace for.

## 2. Transitions are written where both execution paths already go

`PerformExternalEffectService::perform` is not the place. The fault child
decomposes it by hand — `insert_intent`, then `ExternalEffectService::authorize`,
then `ExternalEffectService::dispatch` — because it has to die *between* the
steps `perform` runs internally. A fact written only inside `perform` is absent
from that path, and making the child reproduce it would turn the harness into a
copy of production logic rather than a driver of it.

So transitions ride the repository operations both paths already call, in the
same transaction as the row that is their evidence:

- `insert_intent` → `Prepared`, only when the intent row was actually inserted.
  `ON CONFLICT (id) DO NOTHING` already makes replay a no-op and must not reset a
  status that has moved on.
- `insert_receipt` → the receipt's `outcome_status`, **only when
  `rows_affected == 1`**. Replay of an older receipt must not regress a newer
  state, and the post-commit conflict verification must not mutate anything.
- `insert_reconciliation` → `Reconciling` when the reconciliation settles the
  outcome; nothing when it settles nothing (see requirement 6).
- `mark_outcome_delivered` → `Confirmed` or `Failed` per the settled outcome.

The one transition with no row of its own is `Dispatching`, which gets a single
narrow port method — `record_dispatch_started` — not a generic status setter. An
unrestricted `record_lifecycle_status(effect_id, status, at)` would let any
caller assert `Confirmed` with no receipt, no reconciliation and no evidence
reference, which is the substitution this subsystem refuses everywhere else.

## 3. `Dispatching` is committed after validation and before the adapter call

`AuthorizedExternalEffect::dispatch` validates the adapter descriptor, the
adapter-to-intent match and the precondition digest before it calls the adapter.
Any of those can fail without the world being touched, so writing `Dispatching`
before them persists a lie.

Split the domain method: `validate_dispatch(adapter, current_precondition_digest)
-> Result<(), DispatchError>`, called first by `dispatch` so the existing
behaviour is unchanged. The composer then validates, commits `Dispatching`, and
only then dispatches. The write must be its own committed transaction — folded
into anything later it does not survive the crash it exists to describe.

An adapter that returns an error leaves the effect in `Dispatching`. That is
correct and is the point: we tried, and we do not know.

## 4. A reconciliation may name no receipt

- Domain: `ExternalReconciliation::receipt_id` becomes
  `Option<ExternalEffectReceiptId>`; `reconcile_effect` takes
  `Option<&ExternalEffectReceipt>`.
- Migration: `external_reconciliations.receipt_id` drops `NOT NULL`. Its
  composite foreign key onto the receipt survives — a row with a NULL
  `receipt_id` satisfies `MATCH SIMPLE` — but that leaves the row with **no**
  foreign key at all, so add the one that should always have been there:
  `(effect_id, workspace_id)` onto `external_effect_intents (id, workspace_id)`.
  The effect is the real parent; the receipt never was.
- `ExternalEffectReconciliationService::reconcile` currently refuses anything
  but an `Unknown` receipt. The rule becomes: an `Unknown` receipt, **or** no
  receipt at all. A receipt in any other state is still refused.

## 5. Read-back may name no receipt

`ExternalEffectReadBackAdapter::observe(intent, Option<&receipt>)`. This is
nearly free: `HttpExternalEffectReadBackAdapter` uses exactly one thing from the
receipt — `receipt.effect_id()` at
`crates/vestrace-infrastructure/src/http_read_back.rs:70` — and the intent
carries the same id. `MemoryReadBack` in `tests/effect_recovery.rs` updates with
it.

## 6. Recovery sees a dispatch that never came back

`ExternalEffectRecoveryCandidate::receipt` becomes `Option<...>`.

`find_reconciliation_candidates` becomes a `LEFT JOIN` and selects an effect in
the caller's workspace when either:

- it has a receipt whose `outcome_status = 'unknown'` (today's set, unchanged), or
- it has **no** receipt and its latest lifecycle transition is `Dispatching`
  recorded before `dispatch_considered_lost_before`,

and whose most recent reconciliation settled nothing and is older than
`retry_unsettled_before`.

The lost-dispatch cutoff is a second, separate argument. It is not
`RECONCILIATION_RETRY_AFTER`: one minute is how long to wait before re-asking a
provider that already answered nothing, and it is far too short to declare an
in-flight dispatch lost. A new constant `DISPATCH_CONSIDERED_LOST_AFTER` states
the other question — how long a dispatch may be in flight before an effect still
sitting in `Dispatching` is treated as a crash rather than as slow — and
`sweep` supplies both cutoffs. Sweeping a genuinely in-flight dispatch would
dispatch a second time, which is the one thing the suite counts as a failure at
every point.

Index for that access path: partial, on
`(workspace_id, recorded_at)` where `status = 'dispatching'` — the exact shape
the query needs, not a speculative wide one.

## 7. An effect id reaches its receipt

Finding 3: `find_receipt` takes a receipt id that nothing outside the dead
process ever held, and no by-effect route exists, so a genuinely persisted
receipt reads as absent. Requirement 1 already fixes this — the
`receipt_recorded` transition carries the receipt id in `cause_ref` — but the
property must be stated and tested: given an effect id, a persisted receipt is
reachable.

## 8. The fault scenario reads rather than infers

`crates/vestrace-fault-scenario/src/observe.rs` reads the persisted status.
`status_of` and its doc comment go; the adapter stub's dispatch count keeps its
other job, `retry_attempted`, and stops being a status oracle. `receipt_of` uses
the route from requirement 7.

# Non-goals

- **`Authorized` is not recorded.** Authorization touches no repository and there
  is no authorizations table in any migration (finding 4). Writing an
  `Authorized` transition would be a status with no evidence behind it — exactly
  what requirement 2 exists to prevent. Giving the authorization decision a
  durable record is its own slice, and until it exists **fault point 2 stays
  red** and reads `Prepared`.
- No release-gate producer. The gate still reports four evidence families as
  missing.
- The HTTP surface that collapses every non-`Acknowledged`/`Failed` status to
  `"unknown"`. It reports a receipt; nothing here changes receipts.
- `DockerFaultInjectionRuntime`, provider sandboxes, container or network faults.
- Running the fault suite in CI.

# Constraints

- Preserve the working tree's uncommitted changes under `apps/console/`. They are
  unrelated and are not yours.
- The fault-scenario crate keeps its guards: it may not name
  `FaultObservation::expected`, and the `Dockerfile` may not name the crate.
- **The suite may not be tuned.** Do not adjust `evaluate_fault_suite`,
  `FaultObservation::expected`, the scenario program, the adapter stub or the
  fixtures to make anything agree. If the suite still disagrees after this
  change, that disagreement is the finding — report it, do not close it.
- A SQL `CHECK` cannot derive its values from a Rust enum. The migration restates
  the literals and a schema-contract test enforces parity with
  `EffectLifecycleStatus`'s serialization, so a new variant cannot silently fall
  outside the constraint. This is parity verification, not derivation.
- Both `ExternalEffectRepository` implementations get real behaviour, not
  no-op methods added to compile: `PgExternalEffectRepository` and
  `MemoryEffectRepository` in `tests/effect_recovery.rs:96`.
- Reads and writes of transitions predicate on **both** `effect_id` and
  `workspace_id` explicitly, never on RLS alone — the sqlx repository tests run
  as superuser and bypass policies. A write that affects zero rows is a generic
  error that does not distinguish "foreign" from "missing", which would disclose
  foreign identifiers.

# A contradiction to investigate, not to resolve by choosing

`evaluate_fault_suite` demands `Unknown` at point 3 *and* `reconciliation_started`
at point 3, while points 4 and 5 demand `Reconciling`. Those are only consistent
under a reading where an existing-but-unsettled reconciliation leaves the status
`Unknown`, and a settled-but-undelivered one makes it `Reconciling`. The adapter
stub answers read-back definitively, so a point-3 reconciliation may well settle
— in which case point 3 would read `Reconciling` and fail.

Implement the semantics as requirement 2's table states them, then **run the
suite and record what it says**. If point 3 still disagrees, the finding may be
that `FaultObservation::expected` is itself wrong about what a lost dispatch
should look like. That is a legitimate outcome and must be written up. It is not
licence to edit the expectations.

# Acceptance criteria

1. The migration applies to a database holding pre-existing intents, receipts
   and reconciliations. No existing row is modified. `find_lifecycle_status`
   returns `None` for every pre-existing effect.
2. After `insert_intent`, the latest transition is `Prepared` with cause
   `intent_recorded`. Re-inserting the same intent after the status has moved
   appends nothing and leaves the moved status in place.
3. After `insert_receipt`, the latest transition is the receipt's
   `outcome_status` with `cause_ref` equal to the receipt id. A conflicting
   receipt id appends no transition. Replaying an older receipt after a later
   transition appends no transition.
4. `record_dispatch_started` against an effect in another workspace, or against
   no effect, returns an error, appends nothing, and its message does not
   distinguish the two cases. A cross-workspace *read* of a transition returns
   `None` under a restricted runtime role.
5. A test driving the granular path — `insert_intent`, `authorize`,
   `validate_dispatch`, `record_dispatch_started`, adapter — observes that
   `Dispatching` was committed strictly before the adapter was called, and that a
   failed validation commits nothing.
6. An effect whose dispatch returned and whose receipt was never written is
   returned by `find_reconciliation_candidates` once its `Dispatching`
   transition is older than the lost-dispatch cutoff, and is **not** returned
   while it is younger. The candidate carries `receipt: None`.
7. That candidate reconciles end to end: read-back is called with `None` for the
   receipt, a reconciliation is inserted with `receipt_id` NULL, and the run is
   told. `reconcile` still refuses a receipt that is present and is not
   `Unknown`.
8. Given an effect id, a persisted receipt is reachable (requirement 7).
9. Concurrency: two writers appending transitions for one effect both succeed
   and the latest-transition read is one of them, never a torn or lost row. A
   duplicate receipt replayed after a later transition does not regress the
   status.
10. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` and
    `cargo test --workspace` are green, and the conformance gate's counts are
    unchanged (199 passed, 190 executed, 8 attested, 1 build-verified, 0 failed,
    0 skipped).
11. The fault suite is run against a real ephemeral deployment and its five
    `FAULT_SUITE_OBSERVED` lines plus `FAULT_SUITE_DECISION` are reported
    **verbatim**. Point 2 is expected to stay red for the reason in Non-goals.
    Every remaining disagreement is analysed and reported. A suite that passes is
    as suspect as one that fails and must be explained, not celebrated.

# Implementation plan

1. Domain: `receipt_id` to `Option` on `ExternalReconciliation`;
   `reconcile_effect` takes `Option<&receipt>`; extract
   `AuthorizedExternalEffect::validate_dispatch`; add the lifecycle status name
   parity accessor.
2. Migration `0153`: the transitions table with its FK, RLS, policy and partial
   index; `external_reconciliations.receipt_id` drops `NOT NULL`; the
   `(effect_id, workspace_id)` FK is added. Header comment in the style of 0144
   and 0151 — what was wrong, why append-only, why no backfill and therefore no
   `NO FORCE` dance.
3. Port: `record_dispatch_started`, `find_lifecycle_status`, the by-effect
   receipt route, `receipt: Option` on the candidate, `Option<&receipt>` on
   read-back, both cutoffs on `find_reconciliation_candidates`.
4. `PgExternalEffectRepository`: transitions inside `insert_intent`,
   `insert_receipt`, `insert_reconciliation`, `mark_outcome_delivered`, each
   conditioned on its own row actually being written; the rewritten candidate
   query.
5. `MemoryEffectRepository`: the same behaviour, statefully.
6. Application: `DISPATCH_CONSIDERED_LOST_AFTER`, `sweep` passing both cutoffs,
   `reconcile` accepting no receipt, `perform` validating then recording
   `Dispatching` then dispatching.
7. `HttpExternalEffectReadBackAdapter` and `MemoryReadBack` for the optional
   receipt.
8. Fault scenario: `observe` reads the status; `status_of` deleted.
9. Run the suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`
- the migration applied against a throwaway PostgreSQL 17 holding pre-existing
  intents, receipts and reconciliations, with "no existing row modified"
  asserted
- `cargo test --test effect_fault_scenario_e2e -- --ignored` against an ephemeral
  deployment, output captured verbatim
- the conformance gate rerun and its counts compared to 199/0/0
