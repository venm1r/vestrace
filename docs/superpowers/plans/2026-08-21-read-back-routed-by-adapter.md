# Goal

Ask about an effect the adapter that dispatched it.

`build_effect_reconciliation` takes the **first** configured adapter and builds
one read-back from its URL:

```rust
let Some(adapter) = config.effects.configured().into_iter().next() else { ... };
let read_back = HttpExternalEffectReadBackAdapter::new(&adapter.read_back_url)?;
```

`crates/vestrace-cli/src/commands/worker.rs:351`. And
`EffectsConfig::configured` returns a list — the environment webhook chained
with every adapter from the configuration file
(`crates/vestrace-infrastructure/src/config.rs:442`).

So in any deployment with more than one adapter, an effect dispatched through
adapter B and gone `UNKNOWN` is asked about at adapter A's read-back endpoint.
Whatever A says about an id it has never seen becomes the recorded outcome of
B's effect. `reconcile_effect` maps `effect_applied: Some(true)` to
`ReconciliationOutcome::Confirmed`, so a false **confirmation** is reachable:
the system records that something happened, in a durable settled reconciliation,
delivers that fact to the run that asked, and removes the effect from the sweep
forever.

This is worse than the gap it was found next to. An effect nobody can find is a
known unknown; a confirmed outcome nobody performed is a lie the system will
defend. It is also, exactly, the shape ADR-0005 exists to prevent — the
transport answer of one system taken as evidence about another's business
outcome.

The intent has carried the answer the whole time: `ExternalEffectIntent::adapter`
is persisted on every intent and indexed on the row.

# Requirements

1. Recovery resolves read-back by `candidate.intent().adapter()`. Never by
   position, never by a default, never by "the only one configured" — a
   deployment with one adapter must take the same path as one with three, or the
   single-adapter case silently reintroduces the bug the day a second is added.
2. An effect naming an adapter that is not registered **cannot be asked**. It is
   reported through the existing `UnreconciledEffect` channel, which exists
   precisely so that "we could not ask" is never folded into "we asked and
   learned nothing", and it stays a candidate because nothing about it has been
   settled. No fallback to another adapter under any circumstance.
3. Registration refuses two adapters with the same name. `configured()` chains
   the environment webhook with the file list and nothing today prevents the
   same name appearing twice; a silent last-wins would route an effect to
   whichever the iteration order favoured, which is the same class of defect as
   the one being fixed.
4. The worker registers **every** entry from `config.effects.configured()`, not
   the first. A deployment that configures no adapter keeps today's behaviour:
   reconciliation is not constructed at all, with the log line that says why.

# Non-goals

- **§18's evidence ladder.** `HttpExternalEffectReadBackAdapter` queries
  `{endpoint}/{effect_id}` and ignores the receipt's `external_resource_id`,
  `external_version` and `response_digest`, which §18 ranks above a lookup by
  our own identifier. Fixing that changes the read-back wire protocol, and the
  adapter stub that would have to answer the new shape is a protected file this
  slice may not touch. It needs its own design and is recorded here, not carried.
- **Confirming acknowledged effects.** The previous plan's subject. It depends on
  this one — routing has to be right before more traffic is sent through it —
  and it additionally needs bounded batches, an index, backoff, a claim against
  double-asking, and a durable escalation path for what cannot be confirmed.
- **A lost `Dispatching` becoming `UNKNOWN`**, and the startup-recovery work it
  needs. Fault point 3 stays red.
- **`Authorized`.** Fault point 2 stays red.
- Release-gate producers.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the
  scenario program's call sequence or abort sites, the adapter stub, or the
  fixtures. `crates/vestrace-fault-scenario/src/main.rs:250` constructs its own
  recovery service and will need whatever the new constructor takes; that is a
  signature update, not a change to what the scenario drives.
- `ExternalEffectRecoveryService` must not be constructible in a way that has no
  route for some adapter and does not say so. Prefer a shape where an
  unresolvable adapter is an explicit outcome rather than an `unwrap_or` waiting
  to be written.
- Both `ExternalEffectRepository` implementations and both
  `ExternalEffectReadBackAdapter` implementations stay working:
  `HttpExternalEffectReadBackAdapter` and `MemoryReadBack` in
  `tests/effect_recovery.rs`.

# Acceptance criteria

1. With two adapters registered under distinct names and distinct endpoints, an
   effect whose intent names the second is read back against the **second**
   endpoint. Asserted by observing which endpoint received the request, not by
   inspecting the wiring.
2. The first adapter's endpoint receives **nothing** for that effect. This is the
   regression test for the defect: it must fail against the current code.
3. An effect naming an unregistered adapter produces no reconciliation, no
   lifecycle transition, and appears in the sweep report's `unreachable` set with
   a reason naming the adapter. It is still returned as a candidate by the next
   sweep.
4. Constructing the registry with two adapters sharing a name is an error naming
   the collision.
5. A deployment with a single configured adapter still reconciles that adapter's
   effects, and an effect naming a different adapter is refused rather than
   routed to the only one available.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace` green against a real PostgreSQL 17 with pgvector.
   `authorized_shared_reader_uses_exact_revision_under_forced_rls` is a known
   pre-existing failure and is not this slice's.
7. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and its output
   reported verbatim.

   **Prediction, to be verified rather than assumed:** the scenario configures
   exactly one adapter and every effect names it, so routing by name reaches the
   same endpoint routing by position did. The expected outcome is therefore
   **no change at all** — points 1 and 5 agree, points 2, 3 and 4 fail exactly as
   they do today, `failures=4`. A change here would mean this slice did something
   it was not asked to do, and would need explaining before anything else.

# Implementation plan

1. Application: a registry mapping adapter name to
   `Arc<dyn ExternalEffectReadBackAdapter>`, refusing duplicate names at
   construction. `ExternalEffectRecoveryService` takes it in place of the single
   adapter.
2. `reconcile_candidate` resolves by `candidate.intent().adapter()` and returns
   a distinguishable "no route" error rather than a generic one, so `run` can
   classify it into `unreachable` instead of treating it as a provider failure.
3. `build_effect_reconciliation` registers every configured adapter and reports
   the count and names it registered, in the manner of the log line it already
   writes.
4. `crates/vestrace-fault-scenario/src/main.rs`: the constructor call only.
5. `tests/effect_recovery.rs`: `MemoryReadBack` gains the ability to record which
   instance was asked, so criteria 1 and 2 can be asserted from the outside.
6. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace` with `DATABASE_URL` pointing at a pgvector-enabled
  PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
- the regression test from criterion 2 confirmed to **fail** against `HEAD`
  before the fix, so that what it protects is known rather than assumed
