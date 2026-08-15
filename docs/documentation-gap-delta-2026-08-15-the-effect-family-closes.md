# The effect family closes

**Date:** 2026-08-15
**Scope:** conformance cases for the remaining eight external-effect
requirements, and the receipt fields they needed.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Ten of the eighteen EXT requirements already executed — how a timeout becomes an
unknown, what an adapter must declare, why dispatch is not delivery. The other
eight read `SKIP — No conformance case registered yet`, and they are the ones
about the *record*: the intent before dispatch, the receipt after it,
compensation, and what a receipt is allowed to contain.

**All eighteen now execute.**

```text
before:  174 passed (164 executed, 10 attested), 25 skipped
after:   182 passed (172 executed, 10 attested), 17 skipped
```

## The receipt kept things nobody could read — again

Writing the case for EXT-016, which requires a dispatched effect to have a
receipt, ran into the same wall as the previous two slices:
`ExternalEffectReceipt` stored `adapter`, `dispatched_at`, `acknowledgement_at`,
`response_class`, `external_resource_id`, `external_version` and
`response_digest`, and exposed none of them. The receipt recorded what the
provider called the thing it created and no reader could ask.

`ExternalReconciliation` had the same problem with `receipt_id` — the link
between "we looked" and "at what" — and `ExternalEffectIntent` with
`expected_effect`, the claim the whole reconciliation is measured against.

**This is the fourth slice in a row that found this.** `DataExportPlan`'s
`object_revisions`, the recovery types' evidence, `QualificationBaseline`'s
invalidation reason, and now these. Every one was found the same way: by trying
to write a case about the thing the field records. That is a pattern in the
codebase, not four accidents — an EVIDENCE-class requirement is satisfied by a
field existing, and nothing forces the field to be readable, so the requirement
looks satisfied to the author and is unverifiable to everyone else.

Fifteen accessors added.

## What the cases ask

- **EXT-001** — an intent is unchanged by being authorized and by being
  dispatched, the receipt names it, and an authorization differing in operation,
  target or actor is refused rather than reconciled.
- **EXT-003** — an adapter's declared delivery semantics, idempotency profile and
  reversibility must each match the intent's, or dispatch returns
  `AdapterDoesNotMatchIntent`. Otherwise an intent's guarantees would be whatever
  the adapter happened to do.
- **EXT-012** — a compensation is a new effect with its own identity performing a
  different operation, requiring its own authority, leaving the original
  untouched; an irreversible effect cannot produce one, which is the word
  *rollback* wearing another name.
- **EXT-013** — a compensation carries the identifier of what it compensates, and
  an ordinary effect carries none.
- **EXT-014** — dispatch compares the preconditions as they are *now* against the
  digest recorded when the intent was written, refuses a stale intent, and
  proceeds on a current one.
- **EXT-016** — acknowledged, failed and unknown dispatches each leave a receipt
  naming the effect, the adapter, the response class and the evidence; a
  synthetic unknown receipt cannot be written with no evidence at all.
- **EXT-017** — a receipt is never a business confirmation, however positively the
  far side answered. Confirmation comes from reconciliation against observed
  state, and an inconclusive observation stays inconclusive. Verified failable by
  making an acknowledgement count as confirmation: the case failed with *"a 2xx
  acknowledgement counted as the business outcome, so 'the provider received our
  request' and 'the customer was charged' would be one fact"*.
- **EXT-018** — a receipt serializes twelve fields and **none of them can hold a
  response body**. It records a response *class* and a response *digest*; the
  case asserts no `response_body`, `payload`, `body`, `credential`, `token` or
  `secret` field exists, so there is nowhere for a provider's answer to be
  written into the trace verbatim.

## What this does not do

- **EXT-018 proves a shape, not a discipline.** There is no field a secret
  belongs in; there is nothing stopping an adapter putting one in
  `response_class`, which is a free string. The guarantee is that the receipt
  offers no *invitation*, and that is weaker than the requirement's wording.
- **EXT-001 verifies immutability, not persistence.** The requirement says the
  intent must be *stored* before dispatch. The domain can show the intent is
  fixed and that dispatch derives from it; whether a row is written first is an
  adapter property, and `PgExternalEffectRepository` is still constructed
  nowhere.
- **The whole family is verified and unreachable.** Eighteen requirements now
  execute against a domain that no surface exposes and no adapter drives.
  Nothing in this build has performed an external effect, which has been true for
  every slice that touched EXT and is worth repeating rather than letting a full
  green column imply otherwise.
- **The accessor pattern is fixed four times and not prevented once.** Nothing
  stops the fifth instance. A lint, or a convention that every field validated on
  construction gets a reader, would.

## Test results

Full workspace suite against a live PostgreSQL 17: **875 passed, 0 failed,
exit=0**. Eight new conformance cases, one verified failable by mutation.

Remaining skips: GOV 6, IDW 3, CAP 2, LRN 2, RET 2, QUAL 1, REC 1.
