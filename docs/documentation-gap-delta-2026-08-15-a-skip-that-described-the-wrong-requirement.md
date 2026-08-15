# A skip that described the wrong requirement

**Date:** 2026-08-15
**Scope:** RET-011, and what its skip reason was actually about.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Every remaining skip in the conformance gate names a blocker now — none of them
says "no conformance case registered yet". That made it worth reading the twelve
reasons rather than the twelve identifiers, and one of them does not describe its
own requirement.

**RET-011 in the invariants catalogue:**

> Несопоставимые channel scores следует объединять rank-based fusion, а не
> прямым суммированием.
>
> *Incomparable channel scores should be combined by rank-based fusion rather
> than by direct summation.*

**RET-011's skip reason, as it stood:**

> No invalidation mechanism exists. Superseding or correcting a memory does not
> propagate into retrieval results or into previously issued context packs, so
> there is no behaviour to verify — this is missing implementation, not a missing
> case.

Those are two different requirements. The reason is well-written, specific, and
about something else entirely.

## Why this is worse than "not registered yet"

"No case registered yet" is honest and useless: it says nobody has looked. A
detailed reason that describes a different requirement is worse, because it looks
like somebody *did* look. It survives review, it reads as considered, and it
parks a requirement indefinitely under an explanation that was never true of it.

I wrote several of these reasons in the last few slices. This one predates them,
and I did not check it when I was counting the skips by family.

## The requirement was already satisfied

`reciprocal_rank_fusion` combines channels by `1 / (k + rank)`. Each channel's
own score never enters the sum — the score field on a fused candidate is replaced
by the accumulated rank contribution. That is exactly what RET-011 asks for, and
it has been true since fusion was written.

The case exercises it as an invariance:

- **The fused order does not move** when one channel's scores are multiplied by
  a thousand, shifted up by five hundred, or shrunk to near zero, as long as its
  ranking is unchanged.
- **A loud channel cannot carry its favourite.** The candidate ranked first by a
  channel scoring in the ten-thousands and last by a channel scoring in the
  hundredths does not come out on top.
- **No channel's magnitude survives into the result** — every fused score is a
  rank contribution below 1.

The pairing in the fixture is the real one: BM25-style lexical scores in the tens
against cosine similarities in the hundredths. Summing those directly would mean
the text channel always wins, and nobody would notice, because the results would
still look plausible.

Verified failable by changing one line of `reciprocal_rank_fusion` from
`1.0 / (k + rank)` to `candidate.score / (k + rank)`:

```text
FAIL RET-011 — with the lexical channel shrunk to near zero the fused order
changed, so the channels are being combined by magnitude and whichever one
happens to produce larger numbers decides the ranking
```

Note which of the three rescalings caught it. Multiplying by a thousand did not:
under score-weighted fusion the loud channel already dominated, so making it
louder changed nothing. It took *shrinking* the channel to reveal that the
ranking had been the channel's all along. A case that only tried the obvious
direction would have passed against broken code.

## What I checked about the other half

The skip's actual subject — whether superseding a memory propagates into
retrieval — is worth answering even though it belongs to no requirement number I
can find. Live, on the deployed stack:

```text
create   "the aardvark sleeps during the daytime"      → indexed
revise   "the pangolin is covered in keratin scales"
search_documents.content  →  "the pangolin is covered in keratin scales"
memory_revisions          →  revision 1 inactive, revision 2 active
```

The text index does follow a revision, and the vector index does too, since the
outbox drain handles `memory.revised`. So the first clause of that skip reason is
also **no longer true of the system**, whatever requirement it belonged to. The
second clause — previously issued context packs — remains true: a `ContextPack`
records `candidate_ids` and sections and nothing marks one stale when a memory it
quoted is superseded.

## What this does not do

- **It does not audit the other eleven skip reasons.** I read them; they match
  their requirements as far as I can tell. "As far as I can tell" is what the
  RET-011 reason would also have earned before I compared it against the
  catalogue line by line.
- **Context pack invalidation is still missing**, and now has no requirement
  number pointing at it. It is recorded here so it is written down somewhere
  other than a skip message that no longer exists.
- **RET-011 is a SHOULD**, so this closes a recommendation rather than an
  obligation. It moves the gate by one.

## Test results

Full workspace suite against a live PostgreSQL 17: **877 passed, 0 failed,
exit=0**. One new conformance case, verified failable by mutation, and one stale
skip reason deleted.

Remaining skips: eleven — GOV 1, IDW 3, CAP 2, LRN 2, RET 1, QUAL 1, REC 1.
