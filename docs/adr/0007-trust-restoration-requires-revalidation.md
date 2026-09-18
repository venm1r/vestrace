# ADR-0007: Trust Restoration Requires Revalidation

**Status:** Accepted  
**Date:** 2026-08-10

## Context

After crash, restore, corruption repair or crypto anomaly, a service may be available before its state is trustworthy. Treating process startup or successful restore as trust restoration hides unresolved divergence, stale projections and unknown external effects.

## Decision

Vestrace separates availability, health, recovery and trust.

```text
AVAILABLE ≠ HEALTHY
HEALTHY ≠ TRUSTED
RECOVERED ≠ REVALIDATED
```

A scope in `UNTRUSTED` or `REVALIDATING` MUST NOT transition to `TRUSTED` solely by operator command or process restart.

Required transition:

```text
recovery/containment complete
→ RevalidationRun produces required evidence
→ policy evaluates result
→ trust transition
→ capabilities restored progressively
```

`INCONCLUSIVE` does not equal pass. `AcceptedRisk` does not itself turn untrusted state into trusted state.

## Consequences

- trust becomes evidence-backed;
- partial workspace/domain recovery is possible;
- dangerous capabilities can remain suspended until proof exists;
- post-incident qualification can build on a clear revalidation barrier.

## Rejected alternatives

1. Restart => trusted.
2. Restore completed => trusted.
3. Operator `--force-trusted` without evidence.
4. Accepted risk as equivalent to successful revalidation.

## Normative references

- Incident / Recovery / Revalidation Contract;
- Trust & Authority Model §15–16;
- `REC-003`, `REC-010..013`, `REC-017`.
