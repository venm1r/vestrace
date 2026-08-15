# Two gates and a deletion

**Date:** 2026-08-15
**Scope:** conformance cases for the remaining governance requirements.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Six governance requirements still read `SKIP — No conformance case registered
yet`. They are about the two things governance is for: that authority and data
policy are separate gates neither of which can open the other, and that a
deletion says only what it actually did.

**Five now execute; the sixth says why it cannot.**

```text
before:  182 passed (172 executed, 10 attested), 17 skipped
after:   187 passed (177 executed, 10 attested), 12 skipped
```

## Two gates

- **GOV-001** — a permissive data policy allows a destination, and a retired
  signing key is still unusable; a usable key does not widen the policy by one
  destination, and the refusal names the destination rather than the key. Two
  gates, each refusing on its own grounds, neither answering for the other.
- **GOV-011** — holding the capability does not make a forbidden destination
  allowed or lift the sensitivity ceiling. The refusal for an over-classified
  request names *the ceiling*, with the capability granted. A grant is a
  condition of the policy, never a way around it.
- **GOV-012** — and the converse: a policy that allows both the classification
  and the destination still refuses when the required capability is absent, and
  names the missing capability. Both decisions come from the same policy version,
  so the difference is the capability and not a different policy.

The pair matters more than either half. GOV-011 and GOV-012 are the two ways a
system with both checks quietly collapses into one, and each is now a case that
fails if the other check is dropped.

## A deletion that claims only what it did

- **GOV-015** — logical delete, physical delete and crypto erasure travel from
  the request into the verification unchanged, and each maps to a distinct
  disposal method. Three words that stay three.
- **GOV-016** — the interesting one. A verification with a surviving backup is
  `Incomplete` and **names the copy**; a verification that did not cover every
  planned dependency is refused outright rather than recorded as a weaker claim;
  an active legal hold produces `BlockedByHold` rather than completion; and a
  fully covered deletion with nothing left does complete, so the refusals are
  about the state of the world and not about the fixture being unable to
  succeed.

Verified failable by loosening `DeletionVerification::is_complete` to ignore
remaining copies: the case failed with *"a deletion with a remaining copy
reported itself complete"*.

## The one that cannot be asked yet

**GOV-017** — deleting evidence should trigger revalidation of the claims that
rested on it. Every part exists and none of them are joined:

- `DeletionPlan` carries `dependency_refs`, and `verify_deletion` refuses a
  verification that did not cover every one — so the *scope* of a deletion
  already reaches derived state.
- `Claim` has a lifecycle with `contest`, `supersede` and `expire`.
- `RevalidationRun` exists in the trust module.

But nothing takes a completed deletion and reopens the claims whose evidence it
removed, and `ClaimEvidenceLink` has no reverse traversal to find them by. Missing
implementation, not a missing case, and the skip now says that instead of "not
registered yet".

## What this does not do

- **GOV-001 tests two gates in the same process, not two gates in the
  deployment.** The domain has an independent crypto gate and an independent data
  gate. Whether every path that sends data consults both is a survey of call
  sites; the HTTP surface has no model boundary wired at all, so today the
  question is moot in the least reassuring way.
- **GOV-016 verifies the shape of the claim, not the search.** The verification
  refuses to say "complete" while a copy is listed. Who lists the copies, and
  whether anything actually looks in the backups, is outside the domain —
  `remaining_copies` is an argument.
- **The whole governance model is unreached.** `DataPolicy`, `DeletionRequest`,
  `verify_deletion` and the classification types are constructed by no adapter
  and exposed by no surface, exactly as with EXT and IDW. Five more green rows
  describe a design.

## Test results

Full workspace suite against a live PostgreSQL 17: **877 passed, 0 failed,
exit=0**. Five new conformance cases, one verified failable by mutation.

Remaining skips: GOV 1, IDW 3, CAP 2, LRN 2, RET 2, QUAL 1, REC 1 — twelve, down
from sixty-seven when this run of slices started.
