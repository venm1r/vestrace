# A lookup that would never have matched

**Date:** 2026-08-22
**Scope:** the release gate's second evidence producer, and a defect inside it
that no test could have caught.
**Status:** committed delta on `main`. It qualifies no profile and makes no
release claim.

```text
release gate, with --release-approval and a published matching baseline: approval passes
release gate, without the flag:                                          release_approval_missing
fault suite (real ephemeral deployment):                                 FAILED — 2 failures, unchanged
conformance gate:                                                        199 passed, 0 failed, 0 skipped — unchanged
```

## What was built

`conformance release --release-approval` collects the decision
`ReleaseApprovalService` makes. Everything it needs is now obtainable: the
manifest and bundle are loaded, the baseline is looked up from the store the
previous slice created, the trust state is read from the database, the signer
trust policy is built from the mounted key store, and the signature evidence
comes from verifying both artifacts.

That last point was the one place this slice could have been made worthless.
`ReleaseSignatureEvidence` is two `bool`s; writing `true, true` compiles and
reads naturally and turns approval into a formality. Both booleans are produced
by real verification against the mounted store, and three tests hold that in
place: a manifest signature verified through the store, a bundle signature
verified through the store, and a **valid signature from an untrusted identity
rejected**.

An absent baseline or trust state is reported as `ReleaseApprovalFailed` with a
reason naming what was looked for — not as a missing evidence family. "Nobody
looked" and "we looked and there is nothing" are different answers, and the gate
already had two words for them.

## The defect no test could have caught

`QualificationBaseline::from_bundle` records the **bundle's target digest**. The
producer looked the baseline up by the **manifest digest**. They are different
values, so no release would ever have found a baseline that `publish-baseline`
had actually published. The whole producer would have reported "no published
baseline" forever, for every release, in every deployment.

It survived implementation and an independent review. What makes it worth
recording is why it could also have survived its tests: the case that exercises a
missing baseline **passes whether the lookup key is right or wrong**. An
assertion about absence is satisfied by asking the wrong question. Only a test
that finds something can prove the question was the right one, and until the
happy-path case ran there was no such test.

It was found by running the happy-path test that the implementing agent — which
stalled, the sixth this session — never executed.

## Nineteen checks, one word

The gate reported `release_approval_failed` and nothing else.
`ReleaseApprovalService` has nineteen distinct failures: an unsigned manifest, an
invalid signature, a bundle bound to another manifest, a bundle that did not
pass, a baseline that is not qualified, a baseline that does not match, a trust
scope mismatch, a trust state that is not trusted, a blank known limitation, and
more. An operator reading one word learns that approval failed and nothing they
can act on.

The report now names which of the service's own checks failed. That change is
what made the defect above findable at all: the first run said only "failed", and
the second said `no published baseline for target sha256:e590…`, which is the
whole diagnosis.

This is the third message this session that said something had gone wrong without
saying what — after two "indexed metadata does not match payload" errors that now
name their column. A check that cannot say what it rejected costs whoever reads
it an afternoon.

## What the TRUSTED gate actually demands

The happy-path fixture had to become a bundle that genuinely satisfies the
profile it names, and getting there mapped out what that means. Four separate
things were missing, each failing differently:

- **A complete passing conformance report.** The original fixture carried one
  result, for ARC-001, against a profile of 199 requirements.
- **Binding to the manifest**, which the same construction fixed.
- **Hard-gate evidence.** `TrustedQualificationGate` evaluates the
  governance/federation gate over the bundle's evidence *after* its own four
  checks pass, so an empty list fails it however complete the conformance report
  is. These are two different questions and the bundle must answer both.
- **Policy versions.** Evidence with the right requirement, the right origin and
  a `Pass` status is still rejected where the requirement demands a policy
  version and none is given.

That last one is the gate justifying its existence: it checks not only what was
attested but what the attestation rests on.

## Verification

Run here against a PostgreSQL 17 with pgvector. Eighteen of eighteen release-gate
tests pass, including the happy path — approval succeeds with signed artifacts, a
published matching baseline and a trusted state — and every case that pins the
producer's honesty.

`cargo fmt` and `clippy` clean; `cargo test --workspace --no-fail-fast` green
apart from the pre-existing `authorized_shared_reader_uses_exact_revision_under_forced_rls`;
conformance unchanged at 199/199; fault suite unchanged at `failures=2`.

## What this does not do

**The gate still cannot pass.** Two evidence families remain without producers —
recovery qualification, which needs observations from eight recovery targets and
therefore a harness of its own, and capability restoration, which needs a
restoration policy that has no source. The fault suite reports two failures.

**Nothing publishes a baseline as part of a release.** An operator publishes one
deliberately, and the gate finds it or reports that it did not.

**The trust scope is supplied by the operator** (`--trust-workspace-id`) rather
than derived from the release. Deriving it would have meant inventing a rule
about which record a release reads, and inventing one quietly is how every
release ends up reading the same row.
