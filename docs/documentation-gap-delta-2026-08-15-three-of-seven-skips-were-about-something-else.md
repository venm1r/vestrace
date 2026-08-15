# Three of seven skips were about something else

**Date:** 2026-08-15
**Scope:** the seven remaining conformance skips, read against the authoritative
catalogue rather than the registry.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

```text
before: 192 passed (184 executed, 8 attested), 7 skipped
after:  195 passed (187 executed, 8 attested), 4 skipped
suite:  911 passed, 0 failed
```

## What reading them properly showed

The previous slice established that three sources can disagree — the catalogue,
the registry statement, and the skip reason — and that I had been reading two.
Reading all seven skips against `vestrace-normative-invariants-v0.2.md`:

| id | catalogue says | the skip answered |
|---|---|---|
| **CAP-005** | child/subagent effective authority cannot be wider than the parent's | whether every entry point reaches the authorization boundary |
| **CAP-012** | a material change to an approved operation must invalidate the approval | there is no Face and there are no organs |
| **RET-004** | `ContextPack.used_budget` must not exceed the hard budget | row level security is forced on every table |
| IDW-010 | `SharedMemoryRef` must not be substitutable for a local `MemoryId` | *matches* |
| IDW-014 | cross-workspace access must not require removing RLS | *matches* |
| QUAL-010 | cognitive qualification checks properties, not exact LLM wording | *matches* |
| REC-016 | forensic evidence should be preserved before destructive recovery | *matches* |

Three skips had parked verifiable requirements behind explanations about
something else — and two of those three requirements were already implemented and
had simply never been asked.

## RET-004 — implemented, never asked

`ContextPack::new` has always refused `used_tokens > token_budget`. The case
asks it as the property: over budget is **refused at construction** rather than
trimmed (a pack that silently drops items to fit is a pack whose contents nobody
chose), exactly at budget is allowed (a ceiling is not a limit to stay under),
and what a constructed pack reports having spent equals what its items actually
account for — a total kept independently of the contents could satisfy the first
check while describing a different pack.

## CAP-005 — a property about behaviour, not about fields

CAP-003's case already checks that a delegated *spec* cannot widen its parent's.
CAP-005 asks something stronger: that **effective authority** attenuates. The
case takes a parent, delegates a narrower child, and probes both across
capability, operation, scope, risk and budget, asserting that no request exists
which the child is allowed and the parent is not.

It also asserts the child is denied *something* the parent may do — a guard
against the case passing against a delegation that attenuated nothing.

## CAP-012 — a field nothing compared

`ApprovalRecord.operation_hash` has been recorded at approval time since the type
existed, and **nothing ever compared it**. `is_valid` checks status and expiry
and says nothing about *what* was approved, so an approval obtained for one
operation authorised any other — a caller could get "yes" for a small transfer
and present it for a large one, which is the whole of what an approval prevents.

`ApprovalRecord::covers(operation_hash, at)` closes it. The decision worth
defending is that **an approval which recorded no operation covers nothing**:
there is no operation it was about, so there is none it can be checked against,
and treating that as "covers everything" is precisely the blank cheque the
requirement refuses. An approver who wants an approval to be usable has to say
what it is for.

This is the only one of the three that was a missing implementation rather than a
missing question.

## Two methodological failures, both caught by mutation testing

**I wrote a vacuous case.** CAP-005 passed immediately, and three separate
mutations failed to break it. The reason: `child_spec` issues the child to
`PrincipalId::new()` — delegating to yourself is self-escalation and the domain
refuses it — while `decide` evaluated against the *parent's* subject. Every child
probe was denied `SubjectMismatch`, so `child_allows` was always false and the
assertion could never fire. Fixed by evaluating each grant against its own
subject.

**I mutated dead code.** The three mutations that "failed to break" CAP-005 were
edits to `attenuate`, which `from_root` does not call — it has its own inline
logic, and `attenuate` serves only second-level delegation. Mutating `from_root`
to issue the child wider than its parent failed the case immediately, with *"a
delegated child is allowed a resource outside the scope and its parent is not,
so delegation produced authority the delegator never held"*.

Both are the same family as the stale-artifact traps recorded earlier: **the
check ran against something other than what I thought I was checking.** A
mutation that does not fail the test is information, and the first thing it tells
you is to doubt the setup rather than the test.

All three new cases are mutation-proved against code on the path under test —
CAP-012 twice, once for each half of its rule.

## What remains, and why

Four skips, and all four match their catalogue statement:

- **IDW-010** — that `SharedMemoryRef` has no conversion to a local `MemoryId` is
  the absence of a thing, which no runtime case can observe. A source-scanning
  guard could assert it; the conformance runner cannot, because it runs from a
  deployed binary with no source tree.
- **IDW-014** — needs a sharing adapter to exist before "does it relax isolation"
  can be asked.
- **QUAL-010** — a SHOULD over a subsystem nobody has built.
- **REC-016** — a SHOULD that is unimplemented rather than unverified.

## What this does not do

- **It does not re-audit the 192 passing cases against the catalogue.** I read
  the seven skips. Any passing case may cite a requirement whose registry
  statement drifted from the catalogue, and the previous slice's guard checks
  level and class only. That audit is still owed.
- **CAP-012's approval check is unwired.** `covers` exists and nothing calls it:
  no approval path in the application or HTTP layer consults it before acting.
  The requirement is now verifiable and the enforcement is not built, which is
  the same shape as most of the EXT and IDW families.
- **CAP-005 tests one delegation level.** The fixture issues children to fresh
  principals and the second-level path needs the child's own subject as issuer,
  which the fixture does not model. Depth-bounding is CAP-007's requirement and
  is separately covered.
- **The three replaced skip texts were not wrong about their subjects** — every
  one described something real and true. They were attached to the wrong
  requirement ids, and that content is now lost from the gate; where it belongs,
  if anywhere, I have not determined.

## Test results

Full workspace suite against a live PostgreSQL 17: **911 passed, 0 failed**.
Conformance gate: **199 total, 195 passed (187 executed, 8 attested), 0 failed,
4 skipped**.

Remaining skips: IDW-010, IDW-014, QUAL-010, REC-016.
