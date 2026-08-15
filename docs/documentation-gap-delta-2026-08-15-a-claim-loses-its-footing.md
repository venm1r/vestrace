# A claim loses its footing

**Date:** 2026-08-15
**Scope:** GOV-017 — connecting evidence deletion to the claims that rested on
the evidence.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Third item on the v1.0 plan: close the skips that are code rather than
architecture. GOV-017 was the clearest of them, and its own skip reason was the
specification:

> Every part exists and none of them are joined: `DeletionPlan` carries
> `dependency_refs` … `Claim` has a lifecycle with `contest`, `supersede` and
> `expire` … `RevalidationRun` exists. But nothing takes a completed deletion and
> reopens the claims whose evidence it removed, and `ClaimEvidenceLink` has no
> reverse traversal to find them by.

```text
before:  188 passed (180 executed, 8 attested), 11 skipped
after:   189 passed (181 executed, 8 attested), 10 skipped
```

## The missing joint was a shared vocabulary

A deletion records what it removed as **strings**: it works over storage, and
knows nothing about claims. A `ClaimEvidenceLink` holds a typed `EvidenceRef` —
an event, an artifact revision, a memory revision, a tool invocation. The two
could not be compared, so "the deletion removed the evidence this claim cites"
was not a question anything could ask.

`EvidenceRef::reference()` gives each variant one canonical textual form
(`event://…`, `memory://<memory>/<revision>`, `tool-invocation://…`), and that is
the whole joint. Everything else follows from it.

## Two decisions worth defending

**Only a complete verification triggers anything.** An `Incomplete` verification
found copies still standing and a `BlockedByHold` one deleted nothing; neither
establishes that the evidence is gone. Acting on them would contest a claim
because somebody *started* a deletion. Verified failable by removing that guard:
the case failed with *"a deletion that left a copy standing reopened claims
anyway"*.

**Contested, not rejected.** Losing the evidence for a claim does not make the
claim false — it makes it unsupported, which is what `Contested` means: the claim
stands, its basis does not, somebody has to look. Rejecting would be a judgement
nobody made; leaving it `Supported` would be a record asserting evidence that no
longer exists.

**A claim already settled is reported, not overwritten.** A superseded or
rejected claim cannot be contested, and re-opening it would undo somebody's
decision. `RevalidationImpact::already_settled` names them with the status they
were in, because "we deleted the evidence for a claim nobody re-examined" is
exactly what an auditor asks about.

## The case

One deletion, three claims: one resting on the destroyed evidence, one resting on
evidence the deletion did not touch, one already superseded.

```text
PASS GOV-017 [executed] — one claim contested, one left alone, one already
settled and reported; an incomplete deletion reopened 0 claims
```

## The process failure in this slice

I reported GOV-017 as still skipped from a container whose image was **from a
build that had failed** — a transient DNS error fetching the base image — because
my wait loop watched for text in a log rather than the command's exit status, and
I read the gate without first checking the binary.

That is the third stale-image incident in three slices, and the first where the
build had not merely cached but *failed*. The rule that catches all three is the
same and I will treat it as one: before reporting any live result, grep the
deployed binary for a string only the new code contains. The numbers above come
from an image where that check returns 1.

## What this does not do

- **Nothing calls it.** `revalidate_after_deletion` is a domain function, and
  the deletion path that would invoke it — `DeletionRequest`, `DeletionPlan`,
  `verify_deletion` — is still constructed by no adapter. The requirement is now
  verifiable and the behaviour is still unreachable, which is the same shape as
  the rest of GOV.
- **Derived state other than claims is untouched.** GOV-017 says "claims *and*
  derived state". Search documents, embeddings and context packs are all derived
  from evidence and none of them are consulted here. The purge path removes a
  memory's index entries directly, which is a different mechanism with no link
  to this one.
- **No `RevalidationRun` is produced.** The trust module's revalidation machinery
  — the thing with scope, level, checks and evidence — is not involved; a
  contested claim is the signal, and nothing schedules the re-examination it
  asks for.
- **The reference form is now load-bearing.** Anything that writes a deletion
  reference by hand must produce the same string `EvidenceRef::reference()`
  does, and nothing enforces the agreement.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**. One new conformance case, verified failable by mutation; one skip
reason deleted because it is no longer true.

Remaining skips: **ten** — CAP-005, CAP-012, IDW-010, IDW-011, IDW-014, LRN-005,
LRN-008, QUAL-010, REC-016, RET-004.
