# A silence nobody was still choosing

**Date:** 2026-08-14
**Scope:** executable conformance cases for HLT-002 through HLT-020, and the two
defects they found.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Health was the largest untested block left: a thousand lines of domain —
invariants, findings, occurrences, repair plans, executions, verification runs,
budgets, recurrence, an operator contract — and twenty requirements, none of
which had ever been executed against it. Nineteen are properties of those types.
They are cases now, and two of them failed the first time they ran.

## The first defect: an expiry that nothing read

`FindingDisposition::Suppressed` and `AcceptedRisk` both carry an `expires_at`.
It was written in four places and **read in none**.

Compare `RepairPlan`, twenty lines away, where the same field is validated at
construction (`expiry must not precede creation`) and consulted before every
execution (`is_expired`). The health domain knew how to do this and did not do
it here.

The consequence: a suppression or an accepted risk silenced its finding for
ever, whatever bound its author had put on it. "Suppress this until Friday" and
"suppress this permanently" were the same stored value, and only one of them was
what anybody agreed to. After Friday the finding stayed out of view, silenced by
a decision nobody was still making — which is the exact thing HLT-015 ("must not
be silenced without an explicit disposition") exists to prevent, arrived at by a
route the requirement's wording does not obviously cover.

The fix adds `FindingDisposition::expires_at`, `has_lapsed` and `silences`, and
`HealthFinding::lifecycle_status_at` / `disposition_in_force_at`. Three points
of design worth stating:

- **Only silencing dispositions can lapse.** `AutoRepair`, `ManualRequired` and
  `NotRepairable` are classifications, not decisions to look away, so they have
  nothing to expire.
- **`disposition()` still reports a lapsed disposition** while
  `disposition_in_force_at` does not. The decision was made and belongs in the
  record even once it has stopped taking effect; a lapse is not an erasure.
- **`record_occurrence` lapses an expired silence itself.** Observing the
  problem again is the moment the finding comes back into view. Leaving that to
  `lifecycle_status_at` alone would mean the lapse only ever showed up in a read
  that remembered to ask for it — which is how the original bug worked.

## The second defect: a verdict that could be rewritten

`RepairExecution::complete` overwrote whatever was there:

```rust
execution.complete(Failed, t1).complete(Succeeded, t2)   // → Succeeded
```

An execution record exists to be evidence about a change to production state. A
record whose verdict can be replaced after the fact is evidence of nothing, and
the replacement leaves no trace because only the final value is stored — a
failed repair could be recorded as a success with no sign that anything had been
overwritten.

`complete` returns `Result` now and refuses a contradicting second verdict. It
accepts a repeat of the *same* verdict, so a retried write is not an error; only
a contradiction is. One integration test moved to the new signature and gained
the assertion that the rewrite is refused.

## The nineteen cases

Each builds domain values, exercises the operation the requirement names, and
returns `Fail` if the property does not hold. A few worth calling out for what
they assert rather than what they cover:

- **HLT-005** checks that an authorization for a *different* capability is
  refused, not merely that a denial is. Accepting any allow-decision would let
  one grant unlock every plan.
- **HLT-009** asserts that a persistently unhealthy finding is **not** flapping,
  as well as that an oscillating one is. A flapping detector that fires on
  repetition sends an operator looking for an oscillation that is not there.
- **HLT-011** checks that the budget *recovers* once its window has passed, as
  well as that it refuses inside the cooldown. A budget that never recovers
  disables repair permanently after one bad hour.
- **HLT-014** asserts that `apply_verification` returns a new finding and leaves
  the original open, so the state before the repair survives the repair.
- **HLT-019** treats `Inconclusive` exactly like `Failed`: a repair whose
  outcome is unknown must not resolve the finding it was supposed to fix.

## Where the profiles stand

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      61 passed (54 executed,  7 attested),   2 skipped
cognition   67 passed (60 executed,  7 attested),   4 skipped
trusted     89 passed (81 executed,  8 attested), 110 skipped, 0 N/A
```

TRUSTED was `70 passed (62 executed, 8 attested), 129 skipped`. Nineteen
requirements moved from skipped to executed, and the eight attestations are
unchanged and still all `Static`.

Remaining skips by family: GOV 26, REC 18, EXT 18, QUAL 15, IDW 14, CAP 14,
RET 2, LRN 2, HLT 1.

## The one HLT requirement that stays open, and why it is the important one

HLT-001 — "health findings must be produced by an invariant registry, not ad-hoc
checks" — is the only one of the twenty that is not a property of the domain.
It is a claim about the running system, and **this build does not meet it**.

`InvariantRegistry` has no caller. Nothing anywhere constructs one, registers a
definition, or produces a `HealthFinding`. The findings an operator actually
sees come from `PgDiagnosticsRepository`'s nine hand-written checks, which emit
`DiagnosticFinding` — a different, simpler type with no invariant id, no
version, no fingerprint, no lifecycle and no disposition.

So this slice has verified that the health domain is correct, and changed
nothing about the fact that it is unreachable. Its skip message says so now,
instead of "No conformance case registered yet", which read as clerical backlog
for what is really missing implementation.

That is the honest summary of the whole slice: nineteen requirements are now
verified against code that nothing calls. It is worth having — the wiring will
be built on a domain that has been shown to hold, and two defects were fixed
before anything depended on them — but it is not the same as health working.

## Test results

Full workspace suite against a live PostgreSQL 17: **847 passed, 0 failed, 130
suites, exit=0** — unchanged, because the new cases run inside the existing
case-list test.
