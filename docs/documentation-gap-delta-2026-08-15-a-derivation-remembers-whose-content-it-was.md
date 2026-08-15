# A derivation remembers whose content it was

**Date:** 2026-08-15
**Scope:** IDW-011 — deriving something local from mounted content without
losing where it came from.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
before:  189 passed (181 executed, 8 attested), 10 skipped
after:   190 passed (182 executed, 8 attested),  9 skipped
```

## What was actually missing

The skip said "nothing derives from mounted content", which was true and not the
interesting part. The interesting part is what would have happened if something
had:

`Derivation` records `input_refs: Vec<EvidenceRef>`, and the only variant that
could describe a memory was `MemoryRevisionRef { memory_id, revision_id }`. A
memory and a revision — and nothing about **whose** they are. A derivation from
another workspace's content, recorded that way, is indistinguishable a week later
from a derivation from something local.

So the fix is not a function; it is a vocabulary. `EvidenceRef` gains
`SharedMemoryRevisionRef`, carrying the source workspace, the exact memory and
revision, the grant revision that made it reachable, and the source generation.
The grant revision matters as much as the workspace: it says *under what* the
content was reachable, which is the difference between a provenance record and a
pair of identifiers.

## Why the mount produces the derivation

`MemoryMount::derive_local` is the only way to make one, and it builds the input
reference from what the mount already knows. There is no constructor that takes a
bare local identifier, so a derivation from borrowed content cannot be recorded
as anything else — the provenance is not "preserved" by discipline, it is the
only thing available.

It is also a **disclosure**. Deriving from somebody's content is a use of it, so
`derive_local` goes through the same `record_disclosure` that governs reading:
the mount must permit `DeriveLocal`, the source grant and the target policy must
both allow it, and the use is recorded where the source can see it.

## The case

- A mount permitting only discovery and reading is **refused** a derivation.
- A mount granted and accepted for `DeriveLocal` produces one, and it names the
  source workspace (which is never the deriving workspace), the exact revision,
  the grant revision, and who made it.
- The derivation appears in the mount's disclosures as a `DeriveLocal` use.
- After the grant is revoked, deriving is refused like reading is.

Verified failable by changing the input reference to a plain `MemoryRevisionRef`:
the case failed with *"the derivation cites 1 input reference(s), and a
derivation from mounted content must cite the shared one"* — the count is right
and the shape is wrong, which is exactly the silent failure the variant exists to
prevent.

## What this does not do

- **Still nothing calls it.** Sharing is constructed by no adapter, so no
  derivation from mounted content has ever been made. IDW-011 is now verifiable;
  it is not exercised.
- **The `derivations` table is untouched.** It has existed since migration 0006
  with no adapter, and this adds no persistence — the derivation is returned to a
  caller that does not exist yet.
- **Only memories.** A mount is over a memory, so the shared reference describes
  one. Deriving from a shared artifact or evaluation would need its own variant,
  and the same argument would apply.
- **Nothing propagates the source further.** A derivation of a derivation records
  its immediate input; the chain back to the original workspace is only as long
  as somebody follows it, and nothing walks it.

## Test results

Full workspace suite against a live PostgreSQL 17: **884 passed, 0 failed,
exit=0**. Live gate read from an image verified to contain the new case.

Remaining skips: **nine** — CAP-005, CAP-012, IDW-010, IDW-014, LRN-005, LRN-008,
QUAL-010, REC-016, RET-004.
