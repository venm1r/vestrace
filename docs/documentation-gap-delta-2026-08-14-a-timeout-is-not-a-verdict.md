# A timeout is not a verdict

**Date:** 2026-08-14
**Scope:** executable conformance cases for ten EXT requirements — what a
dispatch proves, what it does not, and what an adapter has to declare first.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

External effects are the family where being wrong costs the most: the system is
reasoning about actions it has already taken somewhere it does not control. Six
of the eighteen EXT requirements are properties of `external_effects.rs`, which
no case had executed.

## The six

- **EXT-004** — unknown is a status of its own. A receipt whose outcome is
  unknown reports `Unknown` rather than success or failure, asks to be
  reconciled, and denies retry **for its own stated reason**
  (`DeniedUnknown`) so a caller cannot confuse it with an ordinary non-retryable
  failure. It also cannot be claimed without evidence: asserting "we do not
  know" is still an assertion.
- **EXT-006** — a dispatch receipt is never a business confirmation. The provider
  accepting the call says the call was accepted; whether the thing was done is a
  separate question that reconciliation answers against observed state.
- **EXT-007** — reconciliation ties a receipt to its own intent (a receipt from
  another effect is refused), requires at least one observation, and lets the
  **strongest evidence** decide when two observations disagree — not the first or
  the most recent. An observation that cannot tell either way stays
  `Inconclusive` rather than becoming a verdict.
- **EXT-008** — a compensation is a new effect with its own identity that names
  what it compensates, and the original is untouched. An effect declared
  `Irreversible` **or** `Unknown` cannot produce one: treating an undeclared
  effect as undoable is the same mistake as treating a timeout as a failure.
- **EXT-009** — reversibility travels on the intent, `Unknown` exists as an
  explicit variant so "has not said" is distinguishable from "reversible", and a
  compensation inherits the declaration rather than inventing its own.
- **EXT-010** — a timeout yields an unknown outcome rather than a failure or an
  acknowledgement, denies retry, and demands reconciliation. Recording it as a
  failure would assert the effect did not happen when nobody knows; allowing
  retry is how one external action becomes two.

All six pass. The theme running through them is one property stated six ways:
**the system never converts absence of knowledge into knowledge.**

## The second batch: the adapter contract

- **EXT-002** — an adapter promising *effectively-once* delivery without a
  provider-side idempotency mechanism is refused, because that is a promise only
  the provider can keep. An adapter that does not support reconciliation is
  refused too: an unknown outcome it produced could never be settled. And
  dispatch refuses an adapter whose declaration does not match the intent, or the
  intent's recorded semantics would describe something other than what ran.
- **EXT-005** — the retry rule and the idempotency rule are two halves of one
  protection, and the case says why: retrying an unknown effect through an
  idempotent adapter is safe, retrying a known failure through a
  non-idempotent one is safe, and retrying an unknown effect through a
  non-idempotent adapter is how one action becomes two.
- **EXT-011** — authorization must match the intent in **every** field it names —
  workspace, actor, capability, operation, target and the recorded policy
  decision — so a decision about one action cannot permit another; a denial never
  authorizes; and dispatch additionally refuses an intent whose preconditions
  have moved, because authority is not the only thing that must still hold.
- **EXT-015** — an adapter declares delivery semantics, idempotency,
  reversibility, dry-run support, read-back support and required capability,
  **including when the answer is that it cannot**. The negative declarations are
  the point: a caller planning a rehearsal or a reconciliation needs to know
  before it depends on one.

## Where the profiles stand

```text
trusted    132 passed (123 executed, 9 attested), 67 skipped, 0 N/A
```

Remaining: REC 18, QUAL 15, IDW 14, EXT 8, GOV 6, RET 2, LRN 2, CAP 2.

## What is left in EXT

The eight remaining split the same three ways as GOV did:

**Executable against existing domain code** — queue ordering (EXT-016) is the
only one left, and it has no domain type: order within a run lives in
`run_work_items`, so it is an infrastructure test rather than a case.

**Claims about the deployment** — the effect lifecycle being auditable (EXT-012)
and effects not bypassing the execution boundary (EXT-017) are true of the domain
and unverifiable in this build for the reason recorded two slices ago:
`PgExternalEffectRepository` is constructed nowhere, so no effect has ever been
dispatched by this system.

**Requirements needing a running adapter** — fault suite evidence (EXT-018),
reconnect and restart semantics (EXT-014), and observation-before-cognition
(EXT-013) all need an adapter that actually talks to something.

That last group is worth naming plainly: EXT is the family where the domain is
most thoroughly verified and the deployment has done the least. Nothing in this
build has ever performed an external effect.

## Test results

Full workspace suite against a live PostgreSQL 17: **855 passed, 0 failed, 132
suites, exit=0** — unchanged, since the new cases run inside the existing
case-list test.
