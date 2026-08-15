# Asking whether it actually happened

**Date:** 2026-08-15
**Scope:** reconciliation of unknown external effect outcomes — wiring it,
making the sweep survive a failure, and watching it settle both answers.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice ended with one `unknown` receipt in the table and this
sentence:

> No reconciliation is wired. The adapter can read back — that is why it is
> allowed to exist — and nothing calls it. The one `unknown` receipt in the table
> is a genuine open question the deployment cannot yet answer.

`ExternalEffectRecoveryService` and `HttpExternalEffectReadBackAdapter` were both
written and neither was constructed. The worker now runs the sweep beside the
outbox drain, once per configured workspace, and the open question is answered.

## Both answers, live

The case reconciliation exists for is the one where **the effect happened and
nobody upstream knows**. A receiver that applies the effect and then stalls past
the adapter's timeout produces exactly that:

```text
POST /v1/effects  → unknown | timeout | reconcile: true
receiver log      → APPLIED 01a003f6-… (then stalling)
worker            → an unknown external effect outcome was settled
                    outcome=Confirmed strength=ExternalResourceReadBack
```

And the other direction, from the effect that timed out against a receiver which
never recorded it:

```text
outcome=NotApplied strength=ExternalResourceReadBack
```

```text
outcome     | strength                    | observed
confirmed   | external_resource_read_back | receiver://effects/01a003f6-…
not_applied | external_resource_read_back | receiver://effects/01a003b9-…
```

Two effects that were indistinguishable an hour ago — both `unknown`, both
un-retryable — are now one confirmed and one not applied, each with the evidence
that decided it.

## One failure no longer ends the sweep

`run()` propagated the first error, so a single endpoint that said nothing left
**every other unknown effect in the workspace unreconciled**, including ones
whose endpoints were answering. The same shape as a drain that stops at its first
failed message.

A candidate that could not be asked is now reported by identity and reason and
stays a candidate, because nothing about it has been settled. The worker logs it:

```text
an unknown external effect could not be asked about and stays unknown
  effect=… reason=…
```

**I changed a test to do this, and that deserves saying.**
`read_back_without_observations_fails_closed_before_persistence` asserted that
the whole sweep returned `Err`. The property it protects — that no reconciliation
is persisted from zero observations, because "we asked and learned nothing" must
not be written down as a conclusion — still holds and is still asserted. What
changed is the blast radius, and the test now also asserts the failure is
reported rather than swallowed. The old name described the mechanism; the new one
describes the property.

## What this does not do

- **`Inconclusive` is reachable and untested live.** A read-back that answers
  without saying whether the effect applied produces it. The receiver here always
  knows, so only `Confirmed` and `NotApplied` were demonstrated.
- **Nothing acts on a reconciliation.** `Confirmed` and `NotApplied` are
  recorded, and no component retries the not-applied one, compensates the
  confirmed one, or tells anybody. The next honest step for a `NotApplied` effect
  is a new intent, and nothing makes one.
- **`HumanRequired` has no route.** `ReconciliationOutcome` has four variants and
  the read-back path can produce three.
- **The read-back is only as good as the far side's memory.** The toy receiver
  keeps its record in a process that can be restarted, and after a restart it
  answers `effect_applied: false` for effects it really did apply. That is not a
  defect in this system — it is precisely what `EvidenceStrength` exists to
  qualify — but it means the `not_applied` above rests on a receiver that had
  been restarted, and I would not treat it as proof of anything but the
  mechanism.
- **No effect-level health invariant.** An unknown outcome that stays unknown
  because its endpoint is down produces a warning per sweep and no finding, so
  it is invisible to `vestrace doctor`.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0** — one test rewritten, none added, because the behaviour that changed was
already covered and the new behaviour is a widening of what that test asserts.
