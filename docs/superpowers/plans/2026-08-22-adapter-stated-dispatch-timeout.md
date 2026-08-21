# Goal

Let the deadline come from the adapter that enforces it.

`DEFAULT_DISPATCH_ALLOWANCE` is five minutes, chosen in the application layer by
something that never makes the call. `HttpWebhookEffectAdapter` holds
`DEFAULT_TIMEOUT = 15s` (`crates/vestrace-infrastructure/src/providers/webhook.rs:10`)
and abandons the request at that point, mapping the timeout to
`AdapterDispatchResult::unknown("timeout", …)` — so a call that merely times out
already produces a receipt, and the ordinary unknown-receipt sweep already sees
it.

The five-minute allowance therefore covers exactly one thing: the window in
which the **process** can die between the adapter returning and the receipt
being committed. That window is bounded by the adapter's own timeout plus the
time to write a row. Promising five minutes for it means a crashed dispatch sits
undiscovered for twenty times longer than the adapter's own contract implies it
could still be in flight.

Two hardcoded durations describe the same fact from opposite sides and disagree
by 20×. The one that is enforced should be the one that is promised.

# Requirements

1. `ExternalEffectAdapterDescriptor` gains a stated dispatch timeout: how long a
   call through this adapter may be in flight before the adapter itself gives up.
   It is part of the adapter's declared contract, alongside delivery semantics
   and idempotency profile, because it is the same kind of fact — something the
   adapter knows and callers must not guess.
2. `HttpWebhookEffectAdapter` declares its own `DEFAULT_TIMEOUT`, and enforces
   the declared value rather than a separate constant. A descriptor that
   advertises one duration while the client enforces another is the disagreement
   this slice exists to end, restated one layer down.
3. The dispatcher derives `dispatch_expires_at` from the adapter's declared
   timeout plus a stated margin for committing the receipt, rather than from a
   global constant. The margin is named and explained; it is not folded into the
   timeout, because the two answer different questions.
4. `DEFAULT_DISPATCH_ALLOWANCE` survives only as the fallback for an adapter that
   declares no timeout, and its doc comment says that is what it is. If every
   adapter declares one, the constant should have no callers left in production —
   say so if that turns out to be the case rather than leaving it looking used.
5. The descriptor's timeout is validated like its other fields: zero or negative
   is refused at construction. An adapter claiming an unbounded call is claiming
   that no crash of its dispatcher is ever detectable.

# Non-goals

- **Worker liveness.** A dispatch owned by a provably dead worker is lost
  immediately regardless of any deadline, and that is the mechanism that would
  make startup adoption safe across replicas and let the fault scenario reach
  adoption at all. It needs a liveness record for effects — the equivalent of
  `run_leases.heartbeat_at`, which exists for runs and not for effects — and it
  is its own slice. Fault point 3 stays red.
- **Per-adapter read-back cadence, backoff, or rate limits.** Unrelated to how
  long one call may take.
- **Confirming acknowledged effects** (§34), **`Authorized`** (fault point 2),
  **§18's evidence ladder**, release-gate producers.
- **Repairing the `NOT VALID` exemption** recorded as debt in the 2026-08-22
  delta. Still open, still not urgent, still not to be closed by fabricating
  history.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
  The stub is a test double implementing the adapter trait; if the trait's
  descriptor gains a field, the stub must supply one, and the value it supplies
  must be the honest one for a local in-process stub rather than a value chosen
  to move a fault point.
- Every existing `ExternalEffectAdapterDescriptor::new` call site must supply a
  timeout. There are many, including conformance cases — inspect them and give
  each a value that describes that adapter, not a copied placeholder.
- Both `ExternalEffectRepository` implementations keep working; this slice
  changes what value is written, not the shape of the write.

# Acceptance criteria

1. A descriptor with a zero or negative dispatch timeout is refused at
   construction, with a message naming the field.
2. `HttpWebhookEffectAdapter`'s descriptor and its HTTP client agree: a test
   reads the declared timeout from the descriptor and asserts the client was
   built with it, so the two cannot drift.
3. A dispatch through an adapter declaring timeout T records
   `dispatch_expires_at` equal to the transition's `recorded_at` plus T plus the
   stated margin — asserted against the persisted row, not against the call.
4. An adapter that declares no timeout — if the type still permits one — falls
   back to `DEFAULT_DISPATCH_ALLOWANCE`, and a test pins that. If the type does
   not permit one, this criterion is satisfied by the type and the plan should
   say so.
5. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
6. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
7. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** The scenario dispatches through
   `HttpWebhookEffectAdapter` at the stub, so its allowance becomes roughly
   fifteen seconds instead of five minutes. The scenario's parent sweeps within
   seconds of the crash, so the deadline still will not have passed and adoption
   still will not fire: point 3 should read `Dispatching` and all five lines
   should be unchanged.

   If the output *does* change, the most likely cause is that the margin or the
   declared timeout came out small enough for the sweep to catch it — which
   would mean this slice accidentally became the demonstration slice. That would
   need reporting and checking rather than accepting, because a fault point that
   goes green for an incidental reason is worth less than one that stays red for
   a stated one.

# Implementation plan

1. Domain: the descriptor field, its validation, and its accessor. Update every
   construction site; the conformance cases are the largest group.
2. Infrastructure: the webhook adapter declares and enforces one value.
3. Application: the dispatcher derives the deadline from the descriptor, with the
   commit margin named as its own constant and explained.
4. `DEFAULT_DISPATCH_ALLOWANCE`'s doc comment updated to say it is a fallback,
   or the constant removed if nothing falls back to it.
5. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
