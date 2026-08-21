# Asking the wrong system what happened

**Date:** 2026-08-21
**Scope:** which external system recovery asks about an effect.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim. Unlike the two slices before it, its subject is not a gap in what
the system knows but a way for the system to record something false.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, deliberately
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
regression tests:                        3 fail when the old routing is reinstated, verified twice independently
```

## The defect

This was found by an adversarial review of a *different* plan, which is worth
recording because nothing was looking for it.

```rust
let Some(adapter) = config.effects.configured().into_iter().next() else { ... };
let read_back = HttpExternalEffectReadBackAdapter::new(&adapter.read_back_url)?;
```

`crates/vestrace-cli/src/commands/worker.rs:351`, before this change. And
`EffectsConfig::configured` returns a **list** — the environment webhook chained
with every adapter from the configuration file
(`crates/vestrace-infrastructure/src/config.rs:442`).

So a deployment with more than one adapter built one read-back from whichever
adapter came first and used it for every effect in every workspace. An effect
dispatched through adapter B and gone `UNKNOWN` was asked about at adapter A's
read-back endpoint, about an identifier A had never issued.

The consequence is not a missing answer. `reconcile_effect` maps
`effect_applied: Some(true)` to `ReconciliationOutcome::Confirmed`, and
`Confirmed` is settled: the reconciliation is persisted, the outcome is
delivered to the run that asked for the effect, and the effect leaves the sweep
permanently. A confirmation of something that never happened is durable,
attributed, and never revisited.

That is the inversion the previous two slices were not about. An effect nobody
can find is a known unknown — the system knows it does not know. A confirmed
outcome nobody performed is a claim the system will defend. It is also precisely
the shape ADR-0005 exists to refuse: one system's transport answer taken as
evidence about another system's business outcome.

The routing key was persisted the whole time. `ExternalEffectIntent::adapter` is
written on every intent and indexed on the row. Nothing read it.

## The repair

`ExternalEffectReadBackRegistry` keys read-back endpoints by the adapter name
the intent carries. `ExternalEffectRecoveryService::new` takes the registry in
place of a single adapter, so there is no longer a shape in which the service
can be constructed with something to fall back to.

Three properties carry the change:

- **Resolution is by name, always.** A deployment with one adapter takes the
  same path as one with three. Special-casing the single-adapter case would have
  reintroduced the defect silently on the day a second adapter was configured.
- **An unresolvable adapter is an outcome, not a default.**
  `ExternalEffectRecoveryError::MissingReadBackRoute` is distinct from a provider
  or storage failure, and `run` classifies it into the report's `unreachable`
  set — which exists precisely so "we could not ask" is never folded into "we
  asked and learned nothing". The effect stays a candidate, because nothing about
  it has been settled.
- **Two adapters cannot share a name.** `configured()` chains two sources and
  nothing prevented a collision; a silent last-wins would route by iteration
  order, which is the same defect wearing different clothes. Registration
  refuses it and names the collision.

`crates/vestrace-fault-scenario/src/main.rs` needed the new constructor
argument and nothing else.

## Verification

The claim that matters here is not that the new tests pass — it is that they
would have failed before. `resolve` was patched to return the first registered
adapter regardless of the name asked for, which is exactly the old behaviour,
and three tests failed:

```text
recovery_asks_only_the_adapter_named_by_the_effect
  assertion failed: !requests.contains(&"https://first.effects.test/read-back")
single_registered_adapter_reconciles_only_its_own_effects
unregistered_adapter_is_unreachable_without_settling_the_effect
```

Restoring name-based resolution returned 13/13. This was done twice, once by the
implementation and once independently afterwards, because a regression test that
is green both with and without the bug protects nothing and reads identically in
a report.

**The fault suite is byte-identical to the previous run**, and that was the
prediction. The scenario configures one adapter and every effect names it, so
routing by name reaches the endpoint routing by position reached. A change here
would have meant the slice did something it was not asked to do:

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

`cargo fmt` and `clippy` clean; `cargo test --workspace` green against a real
PostgreSQL 17 with pgvector apart from
`authorized_shared_reader_uses_exact_revision_under_forced_rls`, which fails
identically on `cec0628` and is recorded in the previous delta as pre-existing.

## What this does not do

**§18's evidence ladder is still not climbed.** `HttpExternalEffectReadBackAdapter`
asks `{endpoint}/{effect_id}` and ignores the receipt's `external_resource_id`,
`external_version` and `response_digest` — identifiers §18 ranks *above* a lookup
by our own id. The previous delta noted approvingly that read-back "cost almost
nothing" because it used only `receipt.effect_id()`; read against §18 that
cheapness is the gap. Closing it changes the read-back wire protocol, and the
adapter stub that would have to answer the new shape is a file these slices may
not touch, so it needs its own design.

**An acknowledged dispatch is still not a confirmed outcome, and still nothing
confirms it.** That was the plan this defect was found while reviewing. It
depends on this slice — routing has to be right before more traffic is sent
through it — and additionally needs bounded batches, a supporting index, backoff,
a claim against two workers asking the same provider twice, and a durable
escalation path for what cannot be confirmed. Fault point 4 stays red.

**A lost `Dispatching` still does not become `UNKNOWN`**, so fault point 3 stays
red on both its counts, and **`Authorized` is still unrecorded**, so point 2 does
too. The release gate still reports four evidence families with no producer.

**Nothing about rate, cost or cadence was decided.** Every configured adapter now
gets its own read-back client. In a deployment with many adapters that is many
clients, and no limit governs how often any of them is asked beyond the existing
unsettled-retry cutoff.
