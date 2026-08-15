# The last behavioural attestations

**Date:** 2026-08-14
**Scope:** executable conformance cases for LRN-001, LRN-002, LRN-003, LRN-004,
LRN-006, LRN-007, QUAL-003 and QUAL-004.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Every run of `conformance check trusted` ended with the same line:

```text
8 behavioural requirement(s) pass on an attestation alone, with no case that
could ever fail: LRN-001, LRN-002, LRN-003, LRN-004, LRN-006, LRN-007,
QUAL-003, QUAL-004
```

That line was added by the slice that made attestations visible, precisely so it
would be uncomfortable to leave. It is gone now, and nothing replaced it with a
weaker claim.

## The six learning requirements

Each case builds domain values, exercises the operation the requirement is
about, and returns `Fail` if the property does not hold.

**LRN-001 — a signal names what it judged.** A `ModelJudge` evaluator with no
model id and no revision is refused, and so is a fact carrying no evidence
reference at all. The first is the one worth having: an unattributable opinion
that later outranks a measurement is how a learning pipeline launders a guess
into a fact.

**LRN-002 — a projection is not a fact.** A projection is `Advisory` whether it
is constructed or rebuilt, references the facts it derives from rather than
containing them, and cannot be rebuilt from another workspace's measurements.
The last of those was worth asserting separately: without it, the separation of
raw from derived would be bookkeeping rather than scope.

**LRN-003 — learning cannot ask for privilege.** Four shapes of privilege
request are refused — a top-level `capabilities` list, a `grants` key nested two
levels down, a renamed `required_capabilities`, and a `permission` inside an
array element — while an ordinary routing change is accepted. The last check
matters as much as the first four: a guard that rejects everything would pass a
test that only tries to break it.

**LRN-004 — learning proposes and does not apply.** A proposal is born `Draft`
whatever its author asked for, reaches `Submitted` once, refuses a second
submission, and carries the target revision it was written against. Submitting
twice is the shape of an apply loop that skips whatever sits between draft and
applied.

**LRN-006 — a measurement outranks an opinion.** Deterministic and
human-authorized signals share a priority above advisory, and the case asserts
they are *equal* as well as higher: ordering them against each other is a
decision the domain has no basis to make, and a case that only checked "higher
than advisory" would let that ordering appear unnoticed.

**LRN-007 — a conclusion keeps its measurements.** A projection with no fact
references, one with no evidence, and one citing an evaluation it did not
declare as a source are all refused.

### Confirming the cases can fail

I removed the governance-key guard from `LearningChange::validate` and re-ran:

```text
exec-lrn-003-no-self-elevation failed: a learning proposal carrying a top-level
capability list was accepted, so the pipeline can ask for privilege it was not
granted
```

Then restored it. A case that has never been seen to fail is an attestation with
extra steps.

## The two qualification requirements

QUAL-003 ("a case maps to one or more requirement IDs") and QUAL-004 ("the
result is machine-readable with pass/fail/skip and evidence") are about the
report rather than the system it describes, which is why they sat on
attestations: the obvious case — run every registered case and inspect the
results — would include itself, and a check that grades its own output is not a
check.

Both build a **separate** runner holding two sample cases, one that passes and
one that fails, and inspect what comes back. The failing sample is what makes it
meaningful: a report that could not represent a failure would satisfy any
assertion made only about passes.

QUAL-003 also checks that `run_for_profile` marks out-of-closure cases
`NotApplicable` rather than dropping them. Dropping them would make the report's
totals depend on the profile in a way a reader cannot see, and "not asked" would
become indistinguishable from "not run" — which is the exact hole the RET/HLT
finding closed at the profile level.

## Where the profiles stand

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      61 passed (54 executed,  7 attested),   2 skipped
cognition   67 passed (60 executed,  7 attested),   4 skipped
trusted     70 passed (62 executed,  8 attested), 129 skipped, 0 N/A
```

TRUSTED was `54 executed, 16 attested` before this slice. The eight that moved
are exactly the eight the report named.

**Every remaining attestation is `VerificationClass::Static`** — ARC-001,
ARC-004, ARC-005, ARC-008, ARC-009, ARC-010, MEM-020 and QUAL-011. Those
describe an architectural shape no runtime assertion observes: "authoritative
and derived state are separated by table and by type", "one execution boundary",
"durable entities carry newtype ids". Writing a fake assertion to make them look
executed would be worse than admitting what they are.

So: **no behavioural requirement in any profile now passes on a sentence.**

## What did not move, and why

The pass counts are unchanged — these requirements were already passing. What
changed is what "passing" means for them, which does not show up in a total and
is the only part worth reporting.

Nothing broke, which deserves a note rather than a celebration: unlike the MEM
and RET rounds, writing these cases exposed no defect. The learning domain was
built with its guards in place — `validate_no_governance_keys` recursing through
nested objects and arrays, `LearnedProjection` requiring both fact references
and evidence provenance, `LearningProposal` having no apply path at all — and
the cases found them working. That is the outcome the earlier rounds did not
have, and it is worth recording precisely because those rounds set the
expectation that a new case finds something.

LRN-005 and LRN-008 remain skipped, and their messages say why: asset
publication and application evidence is outside the fixture for the first, and
deletion/recovery survival is not runtime-verified for the second. Both are
missing mechanism rather than missing cases.

## One stale number corrected

RET-004's skip message told every reader that "66 tables still enable row level
security without forcing it". It is 31, after four batches of that work, and
every table holding a `workspace_id` now carries a policy. The message says so,
and says that the count is checked by the test it names — a number in prose that
nothing verifies goes stale exactly this way.

## Test results

Full workspace suite against a live PostgreSQL 17: **847 passed, 0 failed, 130
suites, exit=0** — unchanged, because the new cases run inside the existing
`every_executable_case_passes_against_the_current_domain` test rather than
adding tests of their own.
