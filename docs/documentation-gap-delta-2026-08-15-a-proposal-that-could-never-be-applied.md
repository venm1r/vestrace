# A proposal that could never be applied

**Date:** 2026-08-15
**Scope:** LRN-005 and LRN-008 — applying what learning proposes, and what
survives when a projection is thrown away.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
before:  190 passed (182 executed, 8 attested), 9 skipped
after:   192 passed (184 executed, 8 attested), 7 skipped
```

## LRN-005: the lifecycle had no end

`LearningProposal` could be drafted, submitted, rejected or withdrawn. There was
no way to **apply** one — so nothing ever changed a cognitive asset through the
learning path, and "a change made by learning must be versioned and provenanced"
had nothing to be true or false about.

`AppliedLearningChange` is the record of what the system did, as distinct from
what somebody proposed: which revision it moved the target from and to, under
which policy, by whose approval, carrying forward the projections and raw
evaluation facts the proposal rested on. A later reader of an agent's
instructions can find not only that they changed but which measurements changed
them.

Two refusals, both deliberate:

- **Only a submitted proposal applies.** A draft was never put forward; a
  rejected or withdrawn one was decided against. Applying either would be a
  change to a cognitive asset that nobody reviewed.
- **The target must be on the revision the proposal expected.** A proposal is
  written against a state of the world; applying it to a target that has moved
  would silently overwrite whatever happened in between. This is the same
  optimistic check `CognitiveMutation` makes, using the same
  `RevisionConflict { expected, current }` so the caller can re-read rather than
  guess.

And `to_revision` is always `from_revision + 1`: a learning change is one step,
and a gap would be a step nobody recorded.

Verified failable by dropping the projection ids from the applied record — the
case failed with *"the applied change carries no projections or raw facts, so a
later reader could not find what measurements it came from"*.

## LRN-008: the property was already true and never asked

"Deleting a learned projection must not destroy raw execution history." The skip
said projection storage exposes no destructive delete path and raw facts are
independent, but that this was not runtime-verified.

It is now, as the property it actually is: a projection is a *function* of raw
facts. The case builds one, discards it, checks the facts are unchanged, then
rebuilds from the same facts **offered in a different order and under a different
projection identity** — and gets identical content and the same cited facts.

That is stronger than "deleting does not cascade". It says a discarded projection
is recoverable from the history that remains, which is the reason the requirement
exists.

## What this does not do

- **Nothing applies a proposal.** `AppliedLearningChange` is returned to a caller
  that does not exist: no adapter persists it, no surface exposes it, and the
  target's revision is a number passed in rather than read from an agent,
  workflow or routing policy. The requirement is verifiable; the path is not
  built.
- **The target is not actually changed.** `apply` produces the record and moves
  the proposal's status. Whose job it is to write the new agent revision — and to
  do both in one transaction — is unanswered, and doing one without the other is
  precisely the failure this record would be evidence of.
- **No approval mechanism.** `approved_by` is a principal the caller supplies.
  Nothing checks it differs from the proposer, holds a capability, or exists.
  Whether learning changes need separation of duties is a governance question I
  did not invent an answer to.
- **LRN-008 tests the domain function, not storage.** Nothing deletes a
  projection from a database, because nothing stores one.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**. Live gate read from an image verified to contain the new cases.

Remaining skips: **seven** — CAP-005, CAP-012, IDW-010, IDW-014, QUAL-010,
REC-016, RET-004. Every one of them now needs either architecture that does not
exist (no Face, no organs, no sharing adapter), a property no runtime assertion
can observe, or a third evidence source — none is closable by writing a case.
