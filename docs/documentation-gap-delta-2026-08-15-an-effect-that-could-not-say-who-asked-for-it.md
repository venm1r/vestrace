# An effect that could not say who asked for it

**Date:** 2026-08-15
**Scope:** `ExternalEffectIntent::execution_ref` — the field that was supposed to
make an effect traceable.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
gate:   192 passed (184 executed, 8 attested), 0 failed, 7 skipped — unchanged
suite:  903 → 906 passed, 0 failed
```

## The defect

The previous two slices both ended on the same admission: a settled
reconciliation outcome never reaches the run that asked for the effect.
`NotApplied` means the system dispatched something that did not happen, and
nobody is told.

Following that through, the reason turned out to be one field. `POST /v1/effects`
documents its `execution_ref` as:

> Which execution this belongs to — a run, a workflow step, an operator action.
> **An effect with no execution behind it is untraceable.**

It was a `String`, checked only for being non-blank along with six other fields.
`run-step-1` satisfied it. So did `asdf`. The doc comment asserted a property the
type did not have, and it was false for **every effect this route had ever
created**: when a reconciliation finally settled what happened, there was no way
to find what had asked for it.

This is the same shape as the resource-scope defect two slices ago — a free
string standing in for a reference, checked for non-blankness, that nothing can
route on — which is why the fix reuses that vocabulary rather than inventing a
second one.

## What changed

An execution reference must now name something: `run://<id>`,
`execution://<id>`, or `workspace://` for an operator acting directly.

`ExternalEffectIntent::execution_run_id()` resolves the first form to an
`AgentRunId`. It deliberately returns `None` for `workspace://` and for
`execution://` — an operator action has no run behind it, and a workflow
execution is a different identity. Saying so is more honest than inventing a
synthetic run or quietly returning the wrong one.

The recovery report carries the run alongside each reconciliation, because only
the sweep holds the intent — a reconciliation names an effect and a receipt, not
what asked for them. The worker's settled line now names the run.

## The mutation check I got wrong, again

I wrote two unit tests for the vocabulary and mutated `ExternalEffectIntent::new`
to delete the validation call. **All five tests still passed.**

The tests called `validate_execution_ref` directly, so they pinned the function's
behaviour and said nothing about whether anything used it. A check that cannot
tell whether the thing it checks is wired up is not a check — it is a
restatement.

Replaced with `an_intent_cannot_be_built_with_an_execution_reference_that_names_
nothing`, which goes through the constructor. Re-mutated: it fails with *"an
intent was built saying it belongs to `run-step-1`, which names nothing"*, and
passes again once restored.

That is the second false mutation proof in three slices — the first was a `mv`
that preserved mtime so cargo never rebuilt. Different mechanism, same failure:
**the check ran against something other than what I thought I was checking.**

## Evidence

Live, against the deployed stack:

```text
POST /v1/effects  execution_ref: "run-step-1"
  → 400 external effect execution reference is not a reference: … `run-step-1`
        is in no vocabulary this system checks …

POST /v1/effects  execution_ref: "run://019ffbca-…"
  → 403
```

The 403 is the effect's own authorization, not this validation: the reference was
accepted and the intent was built, then the authorization boundary refused
because no grant covers that external target. That is the "two grants for one
action" gap recorded two slices ago showing up again, and it is not a failure of
this change.

Image built (exit 0) and verified to contain the new refusal by grepping the
deployed binary.

## What this does not do

- **It still does not tell the run.** This makes the outcome *attributable*; it
  does not deliver it. A `NotApplied` effect now logs which run asked for it, and
  that run's step is still not failed, not resumed, and not notified. That is the
  next slice, not this one, and I would rather say so than describe a log line as
  a fix.
- **Nothing validates that the run exists.** `run://<uuid>` is checked for shape,
  not for referring to a real run in this workspace. A caller can name a run that
  never existed and the effect will be accepted and attributed to it.
- **The three runs stuck at `ReconciliationRequired` are still stuck.** Startup
  recovery classifies them and leaves them, on every restart, and this changes
  nothing about that.
- **`execution://` is accepted and unroutable.** It is a legitimate form the
  surface documents, and nothing resolves it. Whether workflow executions should
  be reachable the same way is unanswered.
- **Existing rows are not migrated.** Effects already stored with a label keep
  it; validation is at construction. Nothing scans for or reports them.

## Test results

Full workspace suite against a live PostgreSQL 17: **906 passed, 0 failed** (903
before). Conformance gate: **199 total, 192 passed (184 executed, 8 attested), 0
failed, 7 skipped** — unchanged.

Remaining skips are unchanged: CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010,
REC-016, RET-004.
