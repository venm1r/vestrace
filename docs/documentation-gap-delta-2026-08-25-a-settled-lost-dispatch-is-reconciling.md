# A settled lost dispatch is reconciling

**Date:** 2026-08-25  
**Scope:** the canonical lifecycle observation for fault point 3 after the real
recovery pass.  
**Status:** implementation delta. It changes no provider operation, migration,
or release qualification by itself.

## The contradiction the runtime exposed

Fault point `AfterDispatchBeforeReceipt` used to require two facts at once:

- lifecycle status `Unknown`;
- reconciliation started.

That pair describes the instant after a lost dispatch is adopted, but not the
state after the fault scenario completes the recovery pass it is designed to
observe. `adopt_lost_dispatch` first appends durable `Unknown` evidence. The
registered read-back adapter then obtains a definitive provider observation,
`insert_reconciliation` persists the settled reconciliation, and the same
transaction appends `Reconciling`. The observer correctly returns the latest
persisted transition.

The repository contract already pins this behavior for a settled receipt-less
reconciliation. Keeping `Unknown` as the suite's final expectation would require
either suppressing valid lifecycle evidence or changing the observer to report
an earlier transition. Both would tune the observed world toward the judge
instead of correcting a judge whose assumption the runtime disproved.

## Decision

The canonical point-3 observation after recovery is now:

```text
status=Reconciling receipt_persisted=false reconciliation_started=true retry_attempted=false
```

`Unknown` remains required as the durable adoption boundary before provider
read-back. It is not the final lifecycle status when that read-back settles.
The evaluator now applies the same canonical-status comparison to all five
points rather than carrying a point-3-only `Unknown` branch.

This correction does not relax the safety assertions: point 3 still requires
reconciliation, still refuses any retry, and still requires exactly one
observation. The scenario observer, adapter stub, fixtures, child call sequence,
abort sites, and production repository semantics are unchanged.

## Behavioral evidence

The unchanged real end-to-end harness was first run against an isolated
PostgreSQL 17 before the contract correction. After worker presence lapsed, it
reported one remaining disagreement:

```text
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Reconciling receipt_persisted=false reconciliation_started=true retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=1
```

A domain regression test independently reproduced that rejection and failed
with `after dispatch before receipt must become UNKNOWN`. After the minimal
canonical correction, all 222 domain unit tests passed and the real harness
reported:

```text
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Reconciling receipt_persisted=false reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=true failures=0
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

This is local ephemeral-deployment evidence. It does not claim a supported
release target, production isolation, container/node failure, or v1.0 release
qualification.

The checkout-level gates were then run against the same isolated PostgreSQL 17:

```text
cargo fmt --all -- --check                         exit 0
cargo clippy --workspace --all-targets -- -D warnings
                                                    exit 0
cargo test --workspace --no-fail-fast              exit 0
vestrace conformance check trusted --json          199 passed, 0 failed, 0 skipped
                                                    190 executed, 1 build-verified
```

The full workspace command leaves its deliberately ignored LM Studio, Compose,
and destructive fault-harness tests ignored. The destructive fault harness is
covered by the separate explicit run above; the other ignored environments are
not evidence produced by this delta.
