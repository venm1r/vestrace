# The deadline comes from whoever enforces it

**Date:** 2026-08-22
**Scope:** where a dispatch's deadline comes from.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. It removes a 20× disagreement between two hardcoded durations.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
webhook dispatch deadline:               5 minutes → 20 seconds
```

## The disagreement

`DEFAULT_DISPATCH_ALLOWANCE` was five minutes, chosen in the application layer by
something that never makes the call. `HttpWebhookEffectAdapter` holds
`DEFAULT_TIMEOUT = 15s`, abandons the request at that point, and maps the timeout
to `AdapterDispatchResult::unknown("timeout", …)` — so a call that merely times
out already writes a receipt, and the ordinary unknown-receipt sweep already sees
it.

Which means the five-minute allowance covered exactly one thing: the window in
which the **process** can die between the adapter returning and the receipt
committing. That window is bounded by the adapter's own timeout plus the time to
write a row. Promising five minutes for it left a crashed dispatch undiscovered
twenty times longer than the adapter's own contract implied it could still be in
flight.

Two constants described one fact from opposite sides and disagreed by 20×.
Neither was wrong in isolation; the arrangement was, because the duration that is
**enforced** was not the duration that was **promised**.

## What changed

`ExternalEffectAdapterDescriptor` now carries a dispatch timeout, alongside
delivery semantics and idempotency profile, because it is the same kind of fact:
something the adapter knows and callers must not guess. Zero or negative is
refused at construction — an adapter claiming an unbounded call is claiming that
no crash of its dispatcher is ever detectable.

The dispatcher derives `dispatch_expires_at` from that declared timeout plus a
**separately named** five-second margin for committing the receipt. The margin is
not folded into the timeout because the two answer different questions: how long
the far side may take, and how long we may take to write down what it said. For
the webhook adapter the deadline is now twenty seconds rather than five minutes.

`DEFAULT_DISPATCH_ALLOWANCE` survives as the fallback for an adapter that
declares nothing, and its doc comment says that is all it is.

Eleven descriptor construction sites were updated, and the values differ because
the adapters do: five webhook descriptors take fifteen seconds, matching the
shipped HTTP client; six local synchronous adapters take one second, because they
perform no external I/O. One new test passes `None` deliberately, to exercise the
fallback. A field given one copied value everywhere would have been declared and
meaningless — the shape this subsystem has already been caught in three times,
with `synthetic_unknown`, `supports_read_back`, and the count of pre-ownership
transitions.

The declaration and the enforcement are pinned to each other by a loopback timing
test that observes what the HTTP client actually does, rather than reading both
numbers from one constant and proving nothing.

## What the scenario's stub did not have to say

`crates/vestrace-fault-scenario/src/adapter_stub.rs` constructs no descriptor —
it is an HTTP receiver, not an adapter — so it needed no timeout at all. The
scenario dispatches through the real `HttpWebhookEffectAdapter` and therefore
receives its fifteen-second declaration honestly, rather than a number chosen to
move a fault point. This was the one place in the slice where a convenient value
could have shifted a red point to green, and there turned out to be nothing to
choose.

## Verification

Run here against a PostgreSQL 17 with pgvector; the implementing agent had
neither a database nor Docker and said so rather than reporting compiled tests as
executed.

The fault suite is byte-identical to the previous run, which is what the plan
predicted: the scenario sweeps within seconds of the crash and twenty seconds has
not passed, so adoption still does not fire and point 3 still reads
`Dispatching`.

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from `authorized_shared_reader_uses_exact_revision_under_forced_rls`,
pre-existing and verified against `cec0628` in an earlier delta. Conformance gate
unchanged.

## What this does not do

**It does not make a crashed dispatch detectable immediately, and it cannot.**
Twenty seconds is the honest floor for a deadline, because until the adapter's
timeout elapses the call may genuinely still be in flight. The thing that would
make a crash knowable at once is knowing the **owner is dead** — which needs a
liveness record for effects, the equivalent of `run_leases.heartbeat_at`, which
exists for runs and not for effects. That is also what would make an effects
sweep at startup safe across replicas, and what would finally let the fault
scenario reach adoption at all. Until then fault point 3 stays red and the
adoption built in the previous slice is proven by tests rather than by the
harness.

**Fault points 2 and 4 stay red** — authorization persists nothing, and an
acknowledged dispatch is still treated as settled, which §34 forbids by name.
**§18's evidence ladder is still not climbed.** The `NOT VALID` exemption debt
recorded in the previous delta is still open. The release gate still reports four
evidence families with no producer.

**The five-second commit margin is an assumption**, stated as its own constant so
that it can be argued with. Nothing measures how long committing a receipt
actually takes under load, and if that ever exceeds five seconds a live dispatch
would be adopted as lost.
