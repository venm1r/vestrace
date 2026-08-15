# A boundary that can be asked

**Date:** 2026-08-15
**Scope:** conformance cases for the isolation family (IDW), and the closing of
the row-level-security hole the previous slice emptied.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Fourteen requirements said what a workspace boundary must do. All fourteen read
`SKIP — No conformance case registered yet`, which is the same sentence a
requirement gets when nobody has looked at it and when its behaviour is fully
implemented and simply never asked. The domain had the whole two-sided sharing
model — grants with revisions, target policies, mounts, disclosures, federation
trust — and none of it was ever put to a question that could fail.

Eleven of the fourteen now execute.

```text
before:  132 passed (123 executed, 9 attested), 67 skipped
after:   143 passed (134 executed, 9 attested), 56 skipped
```

## What the cases actually ask

Each takes a working two-sided share and removes exactly one thing.

- **IDW-001** — substituting the accepting workspace, or the accepting
  principal, is refused. A mount belongs to the pair that accepted it.
- **IDW-002** — a target policy answers yes only for its own workspace *and* its
  own principal. A permission held locally never widens into a global one.
- **IDW-003** — the right identity with the authority removed is
  `TargetPolicyDenied`; the authority under an identity nobody accepted for is
  denied too. Neither answer substitutes for the other.
- **IDW-004** — a mount cannot name a grant that does not exist, cannot accept a
  revision other than the one it names, and cannot accept an operation the
  source never granted. Acceptance narrows; it never adds.
- **IDW-005** — a wildcard target is refused, and so is a workspace sharing with
  itself.
- **IDW-006** — re-sharing is refused four times over: at the grant, in the
  target policy, at acceptance, and in the decision, which returns
  `TransitiveSharingProhibited` rather than falling through to another reason.
- **IDW-007** — narrowing the source alone gives `SourceOperationDenied` and
  narrowing the target alone gives `TargetPolicyDenied`. Two policies, evaluated
  independently, each able to say no by itself.
- **IDW-008** — revoked, expired and stale mounts each stop disclosing **and say
  which of the three they are**. The stale case is the interesting one: the
  source issuing a different revision does not revoke anything, and the mount
  must still stop.
- **IDW-009** — a shared reference carries its source workspace, the exact
  memory and revision, the grant revision that permitted it, and the source
  generation; its source workspace is never the borrowing one.
- **IDW-012** — a fully trusted, active federation relationship with no local
  grant returns `LocalShareRequired`, and a grant belonging to a different pair
  of workspaces returns `DataBindingMismatch`. Recognizing a remote is not
  disclosing to it.
- **IDW-013** — a disclosure that happened survives the revocation of the grant
  that permitted it, while further disclosure is refused. Withdrawing permission
  does not edit the record of what was already sent.

Verified failable by removing the re-share guard from `evaluate_share_access`:
IDW-006 failed with `evaluating a re-share gave SourceOperationDenied rather than
refusing it outright` — the case notices not just that the answer was no, but
that it was no *for the wrong reason*, which is the difference between a rule and
a coincidence.

## The three that still skip, and why the sentence changed

"No conformance case registered yet" is gone from this family. Each remaining
skip now says what would have to exist:

- **IDW-010** — `SharedMemoryRef` has no conversion to a local `MemoryId`: no
  `From`, no `Into`, no `Deref`, no accessor that yields one. That is a property
  of the type, and the absence of a conversion is exactly what a runtime case
  cannot observe; asserting it would only re-state that the case did not call
  something. IDW-009 executes the observable half.
- **IDW-011** — nothing derives from mounted content. `ShareOperation::DeriveLocal`
  can be granted and nothing consumes it, and the `derivations` table has no
  adapter. Missing implementation, not a missing case.
- **IDW-014** — cross-workspace sharing is constructed by no adapter and
  reachable from no surface, so **this build has never performed a
  cross-workspace read**. There is no code path that could relax isolation to
  perform one. The negative half is checked live rather than asserted (below).

That distinction matters more than the eleven passes. A skip that names its
blocker is a piece of work; a skip that says "not registered" is a question
nobody has asked.

## The row-level-security list is empty

The live test carried a `KNOWN_UNFORCED` list of thirty-one tables with a comment
promising it "stops being a comment the day the list is empty". Migrations 0149
and 0150 emptied it, so the list is now `&[]` and the assertion has stopped being
one-directional: a table appearing there is a table whose policy does not apply
to the role that owns it.

The RET-004 skip text still said *31 tables* — written with a note that it would
go stale if not updated with the test. It had. It now states zero, and the same
test asserts the emptiness, so the sentence cannot rot again without something
failing.

## Live

```text
PASS IDW-001 [executed] — a mount belongs to workspace … and principal …; substituting either is refused
PASS IDW-006 [executed] — re-sharing is refused at the grant, the policy, the acceptance and the decision
PASS IDW-008 [executed] — revoked, expired and stale mounts each stop disclosing and say which they are
PASS IDW-012 [executed] — federation trust is necessary and not sufficient; the local share still decides
PASS IDW-013 [executed] — one disclosure at 2026-08-14 19:35:03 UTC survives revocation, and no further
                          disclosure is permitted
Summary: 199 total, 143 passed (134 executed, 9 attested), 0 failed, 56 skipped, 0 N/A
```

Remaining skips: REC 18, QUAL 15, EXT 8, GOV 6, IDW 3, CAP 2, LRN 2, RET 2.

## What this does not do

- **None of this is reachable.** Every case here exercises domain types that no
  adapter constructs and no surface exposes. The isolation family is now
  well-verified *as a design* and entirely unimplemented as a deployment — which
  is the same shape as EXT, and worth saying twice rather than once.
- **`TargetSharePolicy::allows` was made public** so a case could ask it
  directly rather than infer it from a decision that folds four checks into one
  answer. That is a slightly wider API surface in exchange for a question that
  can be asked precisely; the alternative was a case that passes for the wrong
  reason.
- **IDW-002 is the weakest of the eleven.** The requirement is about a
  workspace-local `global` scope not meaning federation-global, and what the
  case can execute is that a locally-held permission answers no for any other
  workspace or principal. That is the spirit; the letter would need a scope
  language that spans workspaces, which does not exist.
- The three skips above remain, and none of them is closable by writing a case.

## Test results

Full workspace suite against a live PostgreSQL 17: **875 passed, 0 failed,
exit=0**. Eleven new conformance cases, one verified failable by mutation; the
suite's own `every_executable_case_passes_against_the_current_domain` covers the
rest.
