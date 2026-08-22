# Evidence we were holding back

**Date:** 2026-08-22
**Scope:** what the read-back request tells the provider about its own effect.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim, and it is invisible to every harness in this repository — which
is the expected result, not a shortcoming.

```text
fault suite (real ephemeral deployment): FAILED — 4 failures, byte-identical to the previous run, as predicted
conformance gate:                        199 passed (190 executed, 8 attested, 1 build-verified), 0 failed, 0 skipped — unchanged
```

## The defect

§18 of the external-effects contract opens with "Reconciliation использует
**strongest available evidence**" and ranks it: provider idempotency lookup,
exact external resource ID, operation status endpoint, ETag/version/revision,
content hash, and so on down to human verification.

`HttpExternalEffectReadBackAdapter::observe` took the receipt and ignored it —
the parameter was literally `_receipt` — and asked `{endpoint}/{intent.id()}`, a
lookup by *our* identifier, which appears nowhere on that ladder. The receipt it
discarded carries `external_resource_id` (rank 2), `external_version` (rank 4)
and `response_digest` (rank 5), all of them given to us by the very provider
being asked.

So this was not a case of using weak evidence. We held the strong evidence and
did not pass it on, which meant the far side could not perform the stronger
lookup even where it would have.

A previous delta noted approvingly that making the receipt optional "cost almost
nothing" because read-back used only `receipt.effect_id()`. Read against §18 that
cheapness was the defect, and this closes it.

## What changed

The receipt's identifiers now travel with the request, named by the rung of §18
they occupy rather than by Vestrace's storage fields — `exact_external_resource_id`,
`etag_version_revision`, `content_hash` — so a provider implementing read-back
can tell what it has been given without matching on our internal schema.

The change is **additive**. The path is unchanged, the identifiers ride as query
parameters, and a provider that ignores them behaves exactly as before. That is
what keeps every existing read-back endpoint working, and it is also what lets
the fault scenario's protected adapter stub keep answering untouched — by
construction rather than by hope.

Absent values are omitted entirely rather than sent empty. The code says why:
*sending a default would claim evidence the provider never gave us.*

And the honesty requirement is stated where the next reader would doubt it:

> The outbound identifiers are lookup hints, not proof of which lookup the
> provider performed. Only the provider's answer can state the strength of the
> observation we record.

A test enforces it: a response declaring weak evidence strength is recorded as
weak even when the request carried a strong identifier. Sending a good question
is not evidence of having received a good answer, and recording strength because
of what we sent would be the substitution this subsystem is built to refuse.

## Verification

Run here against a PostgreSQL 17 with pgvector; the implementing agent stalled
before delivering a report — the third to do so this session — so everything
below was produced by running it rather than by reading a summary. Its trace
showed the red check had been done: the new evidence cases fail against the
unfixed adapter.

The fault suite is byte-identical, as predicted. This slice was expected to be
invisible to every harness in the repository, and it is; its value is that a real
provider stops being asked a weaker question than it could answer.

```text
FAULT_SUITE_OBSERVED point=after_intent_persistence status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_authorization_before_dispatch status=Prepared receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Authorized receipt_persisted=false reconciliation_started=false retry_attempted=false
FAULT_SUITE_OBSERVED point=after_dispatch_before_receipt status=Dispatching receipt_persisted=false reconciliation_started=false retry_attempted=false | expected status=Unknown receipt_persisted=false reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_receipt_before_outcome_confirmation status=Acknowledged receipt_persisted=true reconciliation_started=false retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_OBSERVED point=after_outcome_before_run_commit status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false | expected status=Reconciling receipt_persisted=true reconciliation_started=true retry_attempted=false
FAULT_SUITE_DECISION passed=false failures=4
```

`cargo fmt` and `clippy` clean; conformance gate unchanged;
`cargo test --workspace --no-fail-fast` green apart from the pre-existing
`authorized_shared_reader_uses_exact_revision_under_forced_rls`.

**The `effect_fault_runtime` flake recurred**, giving 12 of 14 in the workspace
run and 14 of 14 standalone, as it did once before. Two occurrences make it a
property of the suite rather than an accident: the container-driver tests share a
Docker daemon with the database container provisioned for verification, and a
suite whose result depends on who else is holding Docker deserves a look of its
own.

## What this does not do

**It does not raise the bar for settling.** `reconcile_effect` still maps
`effect_applied: Some(true)` to `Confirmed` regardless of how weak the evidence
was. §20.4 says risk policy is strengthened for `IRREVERSIBLE`/`UNKNOWN`
reversibility, which *suggests* a floor without stating one — and inferring a
normative requirement from a suggestion is how two earlier claims in these deltas
turned out to be wrong. It is recorded as an open question rather than answered.

**Rank 1 of the ladder — provider idempotency lookup — is still not offered**,
because nothing in the intent is designated as the provider's idempotency key in
a form a read-back could carry.

**Fault points 2, 3 and 4 stay red.** §34's forbidden shortcut still stands: an
acknowledged dispatch is treated as a settled outcome. This slice was its
prerequisite — confirming an acknowledged effect while asking with our own
identifier would have been asking a question the provider is under no obligation
to answer precisely. The `NOT VALID` exemption debt is open, and the release gate
still reports four evidence families with no producer.
