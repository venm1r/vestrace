# Recovery kept evidence nobody could read

**Date:** 2026-08-15
**Scope:** conformance cases for the recovery family (REC), and the accessors
they needed.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Eighteen requirements govern what happens after something goes wrong: how an
incident differs from a finding, what a restart may and may not assume, when
trust comes back. All eighteen read `SKIP — No conformance case registered yet`,
and the domain behind them was substantial: an incident lifecycle with
containment, recovery, revalidation and closure; a trust record with four states;
recovery points with integrity status; revalidation runs with checks and
evidence; a repair budget with a window and a cooldown.

**Seventeen now execute.**

```text
before:  143 passed (134 executed, 9 attested), 56 skipped
after:   160 passed (151 executed, 9 attested), 39 skipped
```

## The reason three of them could not have been written

Writing the cases ran into a wall immediately: the recovery types keep evidence
in private fields with no way to read it.

- `Incident` stored `triggering_findings`, `affected_resources`, `disposition`,
  `closed_by` and `closed_at`. Every one validated on the way in, none readable.
- `RevalidationRun` stored `checks` and `baseline_state_ref`. REC-012 says a run
  must preserve *scope, level, checks, evidence and result*; four of the five had
  accessors and `checks` — the part that says what was actually verified — did
  not.
- `RecoveryPoint` stored `state_ref`, `sequence_position`, `consistency_scope`
  and `provenance`. REC-007 requires a recovery point to reference a **provably
  consistent position**, and the only observable thing about one was whether it
  claimed to be valid.

This is the same defect as `DataExportPlan`'s unreadable `object_revisions` two
weeks of slices ago: a field that exists, is checked on construction, and can
never be looked at. It is a specific and quiet kind of dishonesty, because the
type looks like it records something. Requirements in the EVIDENCE class are the
ones it hurts most — the whole requirement is that the record survives, and
nothing could ask whether it did.

Eleven accessors added. Three of them are the reason REC-007, REC-012 and REC-018
can be asked at all.

## What the cases ask

- **REC-001** — an incident names the findings it came from without becoming
  one, and cannot be closed the way a finding is dispositioned.
- **REC-002** — recovery refuses to begin before containment completes,
  revalidation before recovery, resolution before revalidation passes. Four gates
  in order, each tested by trying to skip it.
- **REC-003** — an untrusted scope cannot be restored by a revalidation of a
  different scope, nor by a passing run it never registered. That is what a
  restart amounts to: evidence from somewhere else.
- **REC-004** — only two of the eight kinds of unfinished work are safe to
  resume; a sweep that resumes an in-flight external dispatch, or that omits a
  whole class, fails qualification.
- **REC-005** — an interrupted dispatch and an unobserved outcome both classify
  as `MustReconcile`, and the action that follows is reconciliation, not retry.
- **REC-006** — nothing ambiguous is safe to retry, and a repair interrupted
  while being verified aborts rather than running again.
- **REC-007** — a recovery point carries its state reference, sequence position,
  consistency scope and provenance; `Unknown` integrity does not permit a
  restore.
- **REC-008** — of the three integrity answers exactly one permits a restore.
  Unknown is not treated as valid.
- **REC-009** — a recovery point is a *position*, and the derived state there is
  rebuilt from the canonical log: replaying the prefix is deterministic, differs
  from replaying the whole log, and leaves the log untouched.
- **REC-010** — recovered, revalidated and trusted are three answers. An incident
  whose revalidation was inconclusive does not resolve, and the scope is not
  trusted.
- **REC-011** — a failed revalidation leaves the scope untrusted, a passing one
  restores it, and the record keeps the identifier of the run that decided.
- **REC-012** — a run keeps scope, level, baseline, checks and evidence, and
  cannot complete with no checks (an assertion wearing the shape of evidence) or
  no evidence.
- **REC-013** — inconclusive is not success and not failure: it leaves the scope
  `Revalidating` where a failure leaves it `Untrusted`.
- **REC-014** — divergent history is the one target requiring a human, and a
  sweep that resolves it as resume, retry *or* reconcile is refused.
- **REC-015** — two attempts an hour with a five-minute cooldown; the window is
  what makes it a rate rather than a lifetime cap.
- **REC-017** — passing with degradation restores *degraded* trust, not full
  trust, and a later clean run completes the restoration. Progressive
  restoration, executed rather than asserted.
- **REC-018** — a closed incident keeps when it was opened, what it was opened
  from, the containment it required, and who closed it with what evidence.
  Closure without evidence or without a disposition is refused.

Verified failable by making `RevalidationRun::is_successful` treat anything but
an outright failure as success: REC-013 failed with *"an inconclusive
revalidation reports success, so 'we could not tell' would count as 'we checked
and it is fine'"*.

## The one that still skips

**REC-016** — preserve forensic evidence before a destructive recovery. Nothing
does, because nothing performs a destructive recovery. `CaptureProfile::Forensic`
exists in the state-engine types and is constructed by no code; there is no
snapshot-before-overwrite step for evidence to hang off. It is a SHOULD, and it
is unimplemented rather than unverified — which is now what the skip says instead
of "not registered yet".

## What this does not do

- **None of this is wired either.** `Incident`, `TrustStateRecord`,
  `RevalidationRun` and `RecoveryPoint` are domain types that no adapter
  persists and no surface exposes. What the deployment actually runs is
  `run_startup_recovery`, which sweeps unfinished runs and classifies them — the
  live log line `startup recovery left run for explicit handling
  target=UnknownOutcome outcome=ReconciliationRequired` is the same
  classification these cases verify, reached by a different path. The rest of the
  incident lifecycle has never opened an incident.
- **REC-004 and REC-006 verify a table, not a sweep.** `classify_recovery` is a
  total function over eight targets, and the cases check that the table says the
  safe thing and that a qualification refuses a sweep that disagrees. Whether the
  code that *performs* recovery consults that table for every case is a survey of
  call sites, not a property.
- **The accessors widen the API.** Eleven fields became readable. That is the
  price of the requirement being checkable, and the alternative was eleven fields
  that only the type itself can see.
- **REC-015 tests `RepairBudget`, which lives in `health`, not `trust`.** The
  requirement is about recovery automation; the budget is the one bounded-retry
  mechanism the domain has, and it is the repair path's. Whether recovery
  automation shares it is not something a domain case can see.

## Test results

Full workspace suite against a live PostgreSQL 17: **875 passed, 0 failed,
exit=0**. Seventeen new conformance cases, one verified failable by mutation.
