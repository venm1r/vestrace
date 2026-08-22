# A decision nobody kept

**Date:** 2026-08-22
**Scope:** what remains after authority permits an external effect.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. **Fault point 2 turns green**; only point 3 is still red.

```text
fault suite (real ephemeral deployment): FAILED — 2 failures, down from 3; points 1, 2, 4 and 5 agree
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
```

## The gap

`ExternalEffectService::authorize` obtained a fully modelled `PolicyDecision` —
policy and version, subject, capability, operation, resource scope, result,
reason, input state, matched grant, decided at — and dropped it.

There was nowhere to put it. Migration 0024 is named
`policy_decisions_tickets_and_approval_grants` and creates only
`authorization_tickets`; no line of Rust mentions `policy_decisions`. The schema's
own filename promised a store that was never built.
`ExternalEffectIntent::policy_decision_ref` is an `Option<String>` with nothing to
point at.

For a system whose premise is that a claim without evidence is not a claim, an
authority decision that leaves no trace is the sharpest kind of gap. The effect
happened *because* something was authorized, and afterwards nobody could say what
was authorized, under which policy version, against which grant, or why.

It was also fault point 2: `Authorized` was indistinguishable from `Prepared`,
because nothing recorded that authorization had happened at all.

## What changed

`external_effect_authorizations` records the decision, keyed to the effect by a
composite foreign key so the effect-to-workspace relationship is a database
invariant rather than an adapter's promise. The payload is authoritative and the
columns beside it are projections checked on read, following the existing
external-effect and capability-grant stores.

The table is deliberately narrower than the general store 0024's name implied.
This authorization boundary serves run work too, and persisting every decision
everywhere has volume and query-design costs this does not answer — so the name
says *of external effects*, which is what it is.

**A refusal is recorded too.** Storing only `allow` would preserve the answer
operators already see and discard the one auditors ask about most. Both results
are stored; only a permitted decision appends an `Authorized` transition, because
a lifecycle transition claiming a refused effect was authorized would be a
contradiction rather than evidence. The 0153 CHECK gains the
`authorized` / `authorization_recorded` arm, so the status stays inseparable from
its cause.

The write lives in `ExternalEffectService::authorize`, which
`PerformExternalEffectService::perform` already delegates to — one place, both
paths. That the composed and granular paths must be fed from one point was the
lesson an earlier slice cost a whole review round to learn.

## A defect worth naming

The read-back check compares each projection column against the payload it
describes. The implementation knew PostgreSQL stores microseconds while chrono
carries nanoseconds, and said so in a comment — then compared
`timestamp_micros()` on both sides.

`timestamp_micros()` **truncates**. PostgreSQL **rounds**. A decision made at
`.123456789` is stored as `.123457` and truncates to `.123456`, so the check
rejected a faithfully persisted decision — but only when the nanosecond
remainder exceeded half a microsecond. With a fixture ending `.123` it would have
passed.

That is a defect which fires on roughly half of all data and is invisible to any
test whose timestamps happen to be round. In production it would have surfaced as
read-back intermittently declaring correctly stored authorizations corrupt.

The repair is not a tolerance chosen to make a test pass: a microsecond is
exactly the column's granularity, so a difference within it *is* the storage
boundary, and anything larger is a projection describing a different decision —
which is what the check exists to catch.

The error message was also changed to name the disagreeing column. "Indexed
metadata does not match payload" costs whoever reads it an afternoon; it cost
three runs here to find that the column was `decided_at`.

## Verification

The implementing agent stalled mid-run — the fifth this session — with the work
in the tree and two failing tests it never saw. Everything below was produced by
running it here against a PostgreSQL 17 with pgvector.

**Point 2 turns green, as predicted.** It crashes after `granular.authorize` and
before dispatch, and now finds an `Authorized` transition where there was nothing:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=2
```

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from the pre-existing `authorized_shared_reader_uses_exact_revision_under_forced_rls`;
conformance gate unchanged.

## What this does not do

**Nothing reads these rows yet** beyond the transition that names one. An audit
surface — who authorized what, under which policy, and what was refused — is its
own work. This makes the question answerable rather than answering it.

**`policy_decision_ref` on the intent stays unpopulated.** The intent is written
before authorization, so it cannot carry the decision's id; the reference runs the
other way. A field that cannot be filled is worth stating rather than quietly
repurposing.

**Only external effects.** Every other authorization this boundary makes — run
work on every worker tick, above all — is still decided and forgotten. That is a
larger question about volume and retention, and naming this table narrowly is how
it stays visible instead of looking solved.

**Fault point 3 is the last one red**, and both its failures are the same fact: a
lost dispatch becomes `UNKNOWN` only once its deadline passes, and the scenario
sweeps seconds after the crash. Closing it needs worker liveness, so that a
dispatch owned by a provably dead process is lost at once rather than after
twenty seconds. The `NOT VALID` exemption debt is open, §18's rank 1 is still not
offered, no minimum evidence strength governs settling, and the release gate
still reports four evidence families with no producer.
