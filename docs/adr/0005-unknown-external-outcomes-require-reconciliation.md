# ADR-0005: UNKNOWN External Outcomes Require Reconciliation

**Status:** Accepted  
**Date:** 2026-08-10

## Context

For external side effects, a timeout or process crash can occur after the remote system already applied an operation. Treating the local transport failure as proof that nothing happened can cause duplicate emails, payments, deployments or other irreversible effects.

## Decision

External transport result and actual external outcome are separate.

`UNKNOWN` is a first-class outcome state.

If Vestrace cannot prove whether a dispatched effect occurred, it MUST NOT automatically translate the result to `FAILED` or retry the operation unless the adapter contract proves retry safety.

```text
dispatch
→ ambiguous completion
→ UNKNOWN
→ reconciliation
→ CONFIRMED / FAILED / remains UNKNOWN
```

Adapters declare delivery/idempotency/reconciliation semantics. Vestrace does not claim universal exactly-once delivery.

## Consequences

- ambiguous irreversible operations do not duplicate automatically;
- adapters must provide explicit semantics and limitations;
- recovery after crash can safely classify unfinished external work;
- some operations may require human resolution when read-back is impossible.

## Rejected alternatives

1. Timeout means failure.
2. Retry every failed network request.
3. Universal exactly-once abstraction over providers.
4. Treat provider ACK as guaranteed desired business outcome.

## Normative references

- Execution & External Effects Contract;
- Incident / Recovery Contract;
- `EXT-003..010`, `REC-004..006`.
