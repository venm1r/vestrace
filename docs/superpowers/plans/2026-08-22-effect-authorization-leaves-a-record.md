# Goal

Make the authorization of an external effect leave a record.

`ExternalEffectService::authorize` produces a `PolicyDecision` — a fully modelled
domain record carrying the policy and its version, the subject, the capability,
the operation, the resource scope, the result, the reason, the input state, the
matched grant and when it was decided — and then drops it. Nothing persists it.
There is no table for it: migration 0024 is named
`policy_decisions_tickets_and_approval_grants` and creates only
`authorization_tickets`, so even the schema's own name promises something that
was never built. No line of Rust mentions `policy_decisions`.

`ExternalEffectIntent::policy_decision_ref` is an `Option<String>` with nothing
to point at.

For a system whose premise is that a claim without evidence is not a claim, an
authority decision that leaves no trace is the sharpest kind of gap: the effect
happened *because* something was authorized, and afterwards nobody can say what
was authorized, by which policy version, against which grant, or why.

It is also fault point 2. `Authorized` is not a state any reader can distinguish
from `Prepared`, because nothing anywhere records that authorization happened.

# Requirements

1. A durable record of the authorization that permitted — or refused — an
   external effect, carrying what `PolicyDecision` carries. Named for what it is:
   authorizations **of external effects**, not a general policy-decision log. This
   boundary is used by run work too, and persisting every decision everywhere is
   a different change with a different cost.
2. **A denial is recorded too.** A log of authorizations that only contains the
   permitted ones is a record of successes, and the question an auditor asks most
   often is what was refused.
3. The record is written by `ExternalEffectService::authorize`, which
   `PerformExternalEffectService::perform` already delegates to. One place, both
   paths — the lesson the `Dispatching` transition cost a whole round to learn.
4. The lifecycle gains an `Authorized` transition with cause
   `authorization_recorded` and `cause_ref` naming the authorization record.
   Migration 0153's `cause_qualified` CHECK has no arm for `authorized` and will
   refuse it, so this needs a migration — and the CHECK is what makes the status
   inseparable from its evidence.
5. Recording happens in the same transaction as nothing else, and **before** the
   dispatch path continues. An authorization that permitted an effect the system
   then performed, with no record because a later step failed, is the gap this
   closes reopened at a different point.
6. `policy_decision_ref` on the intent stays as it is. The intent is written
   before authorization, so it cannot carry the decision's id; the reference runs
   the other way, from the authorization to the effect. Leaving a field that
   cannot be populated is worth stating rather than quietly repurposing.

# Non-goals

- **A general policy-decision store.** Every authorization in the system,
  including run work on every tick, is a volume and a design question this does
  not answer.
- **Consuming the record.** Nothing queries these rows yet beyond the lifecycle
  transition that names one. An audit surface is its own work; this makes it
  possible rather than pretending to deliver it.
- **Worker liveness**, and with it fault point 3, which stays red.
- §18's rank 1, a minimum evidence strength for settling, a uniqueness claim on
  reconciliation, the `NOT VALID` exemption debt, release-gate producers.

# Constraints

- Preserve the uncommitted changes under `apps/console/`.
- Do not modify the fault suite evaluator, the expected observations, the adapter
  stub, the fixtures, or the scenario program's call sequence and abort sites.
- The new table carries a workspace, is under `ENABLE`/`FORCE ROW LEVEL SECURITY`
  with the isolation policy the rest of this subsystem has, and has a composite
  foreign key onto `(id, workspace_id)` of the intent.
- Reads and writes predicate on workspace explicitly, never on RLS alone.
- Both `ExternalEffectRepository` implementations get real behaviour.
- Nothing about which effects are reconciliation candidates changes.

# Acceptance criteria

1. A permitted authorization writes one record carrying the decision's policy
   version, subject, capability, operation, resource scope, result, reason and
   matched grant, and one `Authorized` transition naming it.
2. A refused authorization writes the record with the refusing result and reason,
   and **no** `Authorized` transition — the effect was not authorized, and a
   transition saying it was would be the lie this exists to prevent.
3. Raw SQL inserting an `authorized` transition with any other cause is refused
   by the CHECK.
4. An authorization record cannot be filed against an effect in another
   workspace, and cannot be read from one.
5. Both the composed path (`perform`) and the granular path (`authorize` called
   directly) produce the record, asserted separately, because that distinction
   has already cost one round.
6. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
   `cargo test --workspace --no-fail-fast` green against a real PostgreSQL 17
   with pgvector, excepting the known pre-existing
   `authorized_shared_reader_uses_exact_revision_under_forced_rls`.
7. Conformance gate unchanged: 199 passed (190 executed, 8 attested,
   1 build-verified), 0 failed, 0 skipped.
8. The fault suite is run against a real ephemeral deployment and reported
   verbatim.

   **Prediction, to be verified.** Fault point 2 crashes after
   `granular.authorize` and before dispatch. With the transition written inside
   `authorize`, `observe` should read `Authorized` — which is what point 2
   expects — and the suite should go from three failures to **two**, leaving only
   point 3's two.

   Point 3 would then be the sole remaining red, and it needs worker liveness:
   its dispatch is seconds old and the deadline has not passed.

# Implementation plan

1. Migration `0158`: the authorizations table with its foreign key, RLS, policy
   and index; and the `authorized`/`authorization_recorded` arm added to the
   0153 CHECK. Header comment in the established style — what was wrong, and
   that a denial is recorded too and why.
2. Port: record an authorization; read one back for tests.
3. `ExternalEffectService::authorize` writes the record and, when permitted, the
   transition — before returning the authorized effect.
4. `MemoryEffectRepository` mirrors it.
5. Run the fault suite against an ephemeral PostgreSQL 17 and record the output.

# Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace --no-fail-fast` with `DATABASE_URL` pointing at a
  pgvector-enabled PostgreSQL 17
- `vestrace conformance check trusted`, counts compared to 199/190/8/1/0/0
- `cargo test -p vestrace-integration-tests --test effect_fault_scenario_e2e --
  --ignored --nocapture`, output captured verbatim
