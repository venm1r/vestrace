# Where an effect stands

**Date:** 2026-08-21
**Scope:** persisted lifecycle evidence for an external effect, and what the
fault suite says once the system can be asked where an effect is.
**Status:** committed delta on `main`. It qualifies no profile, makes no release
claim, and its headline result is still a **failing** suite — with a different
composition, and with a finding about the suite itself.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures across 5 points, points 1 and 5 agree
  point 5 agrees for the first time; points 3 and 4 fail against a contract the system breaks
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
cargo fmt / clippy:                      clean
release gate:                            still cannot pass — four evidence families with no producer
```

## What the last slice left

`docs/documentation-gap-delta-2026-08-20-a-fault-nobody-had-injected.md` built
the fault suite a subject and ran it. Four of five points disagreed. Its findings
4 and 5 were the same hole seen from two sides: **the system persists no
lifecycle status for an effect at all.** `external_effect_intents` carries `id`,
`workspace_id`, `adapter`, `payload`, `created_at` and nothing that says where in
its lifecycle the effect is. So `observe` did not read the status, it *inferred*
it — from a receipt's presence, and when there was none, from an adapter stub's
dispatch count.

Finding 2 was the consequence and the larger defect: a crash between `dispatch`
returning and `insert_receipt` committing left **nothing in the database to
find**, and `find_reconciliation_candidates` joined receipts, so the effect was
invisible to recovery forever — precisely the crash the external-effects design
exists to survive.

## What was built

An append-only `external_effect_lifecycle_transitions` table, under the same
forced row level security every other table in 0144 carries, with a composite
foreign key onto `(id, workspace_id)` of the intent.

Append-only rather than a status column on the intent, because the stored intent
is immutable evidence — `PerformExternalEffectService`'s own doc comment says so
— and a mutable projection does not belong on that row.

Every transition names what caused it, and the database refuses one that does
not:

```sql
CONSTRAINT external_effect_lifecycle_cause_qualified CHECK (
       (status = 'prepared'    AND cause = 'intent_recorded')
    OR (status = 'dispatching' AND cause = 'dispatch_started')
    OR (status IN ('acknowledged', 'failed', 'unknown') AND cause = 'receipt_recorded')
    OR (status = 'reconciling' AND cause = 'outcome_settled')
    OR (status IN ('confirmed', 'failed')  AND cause = 'outcome_delivered')
)
```

`cause_ref` is the id of the record that is the evidence. A caller cannot assert
`confirmed` and have it stand: not through the port, which exposes no general
status setter, and not through raw SQL either, because the constraint demands a
cause the status is entitled to.

**There is deliberately no backfill.** Effects that already exist have no
transitions and `find_lifecycle_status` returns `None`, which is the true answer:
nobody recorded where they were. That also means no `UPDATE` across tenant rows,
so unlike 0151 this migration never has to lift `FORCE ROW LEVEL SECURITY` — the
trap 0151 documents after falling into it.

## Where the transitions are written, and why not in `perform`

`PerformExternalEffectService::perform` is the wrong place, and the first draft
of this work put them there anyway. The fault child decomposes `perform` by hand
— `insert_intent`, then `ExternalEffectService::authorize`, then
`ExternalEffectService::dispatch` — because it has to die *between* the steps
`perform` runs internally. A fact written only inside `perform` is absent from
that path, and the first run proved it: point 3 read `Prepared`, and the entire
new lost-dispatch branch of the candidate query was never reached.

Making the child call the missing step would have been the wrong repair. The
harness would then reproduce production logic in order to observe it, which is
the substitution this project refuses.

So transitions ride the repository operations both paths already call, in the
same transaction as the row that is their evidence — `insert_intent`,
`insert_receipt`, `insert_reconciliation`, `mark_outcome_delivered` — each
conditioned on its own row actually being written, so a replayed receipt cannot
regress a newer state. The one transition with no row of its own,
`Dispatching`, moved *into* `ExternalEffectService::dispatch`, which became async
and repository-aware. `perform` delegates to it rather than repeating it. The
child needed one constructor argument and one `.await`; its call sequence and
its abort site are unchanged, and it records nothing itself.

The order inside that method is load-bearing: validate the adapter descriptor,
the adapter-to-intent match and the precondition digest **first**, then commit
`Dispatching`, then call the adapter. Writing before validation would persist a
lie about a dispatch that never happened. An adapter that returns an error leaves
the effect in `Dispatching`, which is correct and is the point: we tried, and we
do not know.

## A reconciliation about a dispatch, not about a receipt

`ExternalReconciliation::receipt_id` became `Option`, and
`external_reconciliations.receipt_id` dropped `NOT NULL`. When something was
dispatched and nothing came back, there is a dispatch to reconcile and no receipt
to name. The row keeps a parent: 0144 already installed the
`(effect_id, workspace_id)` foreign key onto the intent, so a null receipt does
not leave the row unanchored.

Read-back followed, and cost almost nothing:
`HttpExternalEffectReadBackAdapter` used exactly one thing from the receipt —
`receipt.effect_id()` — and the intent carries the same id.

`find_reconciliation_candidates` became a union: the old set (a receipt whose
`outcome_status` is `unknown`), plus effects with no receipt whose **latest**
transition is `Dispatching` older than a lost-dispatch cutoff. "Latest" is
enforced by `NOT EXISTS (... newer transition ...)` rather than assumed.

## The two runs

Against a PostgreSQL 17 provisioned for the purpose. First run, before the
`Dispatching` transition reached the granular path:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

Second run, after:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
FAULT_SUITE_FAILURE fault point AfterAuthorizationBeforeDispatch has incorrect lifecycle status
FAULT_SUITE_FAILURE after dispatch before receipt must become UNKNOWN
FAULT_SUITE_FAILURE UNKNOWN fault point must start reconciliation
FAULT_SUITE_FAILURE fault point AfterReceiptBeforeOutcomeConfirmation has incorrect lifecycle status
```

Still four failures, and the count is the least informative thing about it.

**Point 5 agrees, for the first time.** `EffectLifecycleStatus::Reconciling` had
no writer anywhere in the workspace — the previous delta's finding 1 — and now
it has one, bound to the reconciliation that caused it. All four fields agree.

**Point 3's reading changed from accidentally right to honestly wrong.** It used
to report `Unknown`, inferred from the stub's dispatch count. It now reports
`Dispatching`, read from persisted evidence, which is what the system actually
leaves behind. The old reading happened to be closer to the expectation; the new
one is closer to the truth, and the difference between those two is the whole
subject of this machinery.

**Point 2 stays red by design.** `Authorized` is not recorded, because
authorization touches no repository and there is no authorizations table in any
migration. A transition with no evidence behind it is the thing every other part
of this slice exists to prevent. Giving the authorization decision a durable
record is its own work.

## What this does not do, and one finding about the judge

**Finding 2 is not demonstrably closed.** The lost-dispatch branch of the
candidate query is built and unit-tested, and the scenario does not reach it.
`DISPATCH_CONSIDERED_LOST_AFTER` is five minutes and the scenario runs recovery
seconds after the crash, so the sweep correctly refuses to treat the dispatch as
lost — it could still be in flight. The machinery is there; the property is not
proven end to end, and this document does not claim it is.

The cutoff is also the wrong shape for the case it exists for, and that is worth
writing down rather than discovering later. **A dispatch in flight cannot survive
the process that started it.** At startup, every `Dispatching` transition belongs
to a process that is gone and is lost by definition, with no timeout involved.
The codebase already has this idea for runs — `run_startup_recovery` at
`crates/vestrace-cli/src/commands/worker.rs:62` — and has no equivalent for
effects. Age is a proxy for the question; process generation is the question.

**The judge was doubted, and the contract says the judge is right.** This
section first argued that `FaultObservation::expected` encoded *"the system
should end up not knowing"* where it should encode *"the system should end up
knowing, having recovered"* — reasoning from what a good deployment ought to look
like rather than from what this system has already committed to. Checking it
against the normative contract reverses the conclusion on both points, and the
correction is left visible rather than quietly rewritten, because the reasoning
that produced it is the kind worth being able to recognise again.

`docs/specs/vestrace-execution-external-effects-contract-v0.2.md` §34 lists the
forbidden shortcuts, and the first line is:

```text
HTTP success == business outcome
```

`HttpWebhookEffectAdapter` maps 2xx to `AdapterOutcome::Acknowledged`,
`ExternalEffectReceipt::from_dispatch` maps that to
`EffectLifecycleStatus::Acknowledged`, and the recovery sweep excludes
acknowledged receipts. The system therefore takes a provider's transport
acknowledgement as the end of the story, which is the shortcut named above and
also ADR-0005's rejected alternative 4, *"treat provider ACK as guaranteed
desired business outcome"*. §11 says the same thing positively — `COMMITTED` in
the transport sense means request dispatch, not a confirmed business outcome —
and puts `CONFIRMED / RECONCILING / UNKNOWN` *after* `ACKNOWLEDGED`. **Point 4
demanding `Reconciling` of an acknowledged effect is that rule expressed as a
test, and the system is what fails it.**

Point 3 goes the same way. §17 lists "process crash между provider response и
persistence" as a cause of `UNKNOWN`, and §26 states the boundary verbatim:

```text
after dispatch / before receipt
→ UNKNOWN
→ reconcile
```

So an effect whose dispatch was lost must **become** `UNKNOWN`. Leaving it in
`Dispatching` — a transient in-flight phase — is a defect, not the honest
reading this document credited it as being. The reading is honest about what is
stored; what is stored is wrong.

That also dissolves the paper argument above. Reconciliation at point 3 has no
receipt, so §18's evidence ladder cannot reach step 2, "exact external resource
ID" — the strongest evidence is unavailable by construction, and an outcome that
settles nothing leaves the effect `UNKNOWN`, which is exactly what point 3
expects. Whether the adapter stub should answer definitively without that
evidence is a separate question about the fixture.

None of this was acted on here and the expectations were not touched. The next
slice has its subject: an acknowledged dispatch is not a confirmed outcome, and
a lost dispatch must become `UNKNOWN` rather than staying `DISPATCHING`.

**Also unchanged:** only process death is exercised; no container is stopped and
no network partitions. Postgres is assumed to keep its own promises. The suite
does not run in CI. Nothing invokes the suite in production, so the release gate
still reports `fault_suite_missing` rather than the `fault_suite_failed` it has
earned. Release approval, recovery qualification and capability restoration
remain without producers.

**A pre-existing failure was found and is not this slice's.**
`authorized_shared_reader_uses_exact_revision_under_forced_rls` in
`tests/idw_014_shared_read_postgres.rs` fails against a freshly provisioned
database with `active memory must have at least one source`, raised by the
`vestrace_check_active_memory_source` trigger. It fails identically on `cec0628`
with none of this slice's changes applied, verified in a separate worktree. It is
recorded here so it is not lost, and it is not diagnosed here.

**One compatibility note.** `LegacyRunEvent::ExternalEffectSettled::receipt_id`
became `Option`. Events already written carry the field and deserialize
unchanged; events written from now on may carry null, which older builds cannot
read. Backward compatible, not forward compatible.
