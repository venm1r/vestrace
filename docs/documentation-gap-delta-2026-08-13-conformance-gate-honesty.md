# Documentation Gap Delta — The Conformance Gate Told the Truth About Itself

**Date:** 2026-08-13
**Scope:** profile closures, and what a "passing" conformance case actually meant
**Decision:** operator approved the ordered plan; items 1 and 2 executed here
**Repository state:** dirty, implementation changes uncommitted

## 1. Thirty-five requirements belonged to no gate

`run_for_profile` marks any case outside the requested profile's closure
`NotApplicable`. A family absent from *every* profile is therefore unreachable:
it can never pass and can never fail, because it is never asked.

RET (15 requirements) and HLT (20) were in exactly that position. TRUSTED —
the v1.0 gate — reported them `N/A`, so a passing v1.0 qualification would have
certified a release while 35 mandatory requirements had never been put to any
gate. The roadmap meanwhile gates **H5 → v0.5 Understand** on HLT and produces
RET evidence in C5–C7. The gate and the roadmap disagreed silently.

Placement now follows the roadmap: RET closes with MEMORY (retrieval reads what
memory stores; C5–C7 precede the v0.2 gate), HLT with TRUSTED. HLT's real home
would be a profile matching the v0.5 gate, and the chain has no such level;
TRUSTED is the latest correct answer rather than a guessed intermediate one.

Two tests now hold this shut:

- `every_registered_requirement_is_reachable_from_the_trusted_profile` — adding
  a family to the registry without placing it in a profile fails here instead
  of silently widening the hole.
- `the_profile_chain_only_ever_grows` — a successor profile cannot drop a
  requirement its predecessor already required.

`TRUSTED` now reports **0 N/A**. The skip count rose from 124 to 154 because 35
orphans became visible, which is the point.

### The closure was also defined twice

`vestrace_domain::conformance::runner::profile_requirements` and the CLI's own
`profile_requirement_ids` each carried a full copy of the `match`, and
`build_report` used the CLI's. The hard gate and `conformance check` could
therefore disagree about what a profile requires, and nothing would have said
so. The CLI copy is gone.

## 2. No conformance case executed anything

This is the finding that changed what "closing CORE" means.

`build_report` walked the registry and called `evaluate_requirement`, a large
`match` returning a hardcoded `(CaseStatus::Pass, "a sentence", Some("a/path"))`
for each requirement it knew about. Nothing was constructed, nothing was
exercised, nothing could fail. **Forty passing requirements were forty
hand-written claims that some code existed.**

The domain already had the right abstraction — `ConformanceCase` with a `run()`
method, and `ConformanceRunner` to execute them. **Nothing implemented the
trait.** The machinery was written and never connected.

And `hard_gate_evidence` stamped every result `EvidenceOrigin::LocalExecutable`.
That enum exists precisely to separate executed evidence from self-asserted
evidence, and the code handed the strongest value to claims that executed
nothing.

Adding thirteen rows to that `match` would have "closed CORE" in an afternoon
and produced exactly the kind of false record the rest of this system refuses
to produce. It was not done.

### What replaced it

`CaseOrigin { Executed, Attested }` now travels on every result, defaulting to
`Attested` on deserialization — an older report carries no origin, and the
weaker reading is the only safe one. The summary reports the split, the human
output tags each pass, and `hard_gate_evidence` maps `Attested` to
`RemoteSelfAsserted` rather than claiming local execution.

`ConformanceReport::attested_but_should_execute()` names the shortfall:
requirements whose `VerificationClass` is anything but `Static` that pass on an
attestation alone. `Static` is exempt on purpose — "the architecture must use a
single execution/policy/audit boundary" is a claim about shape that no runtime
assertion observes, and inventing a fake assertion for it would be worse than
admitting what it is.

`conformance check core` now ends with:

```text
Summary: 199 total, 20 passed (5 executed, 15 attested), 0 failed, 8 skipped, 171 N/A

13 behavioural requirement(s) pass on an attestation alone, with no case that
could ever fail: ARC-007, TMP-001, TMP-006, TMP-007, TMP-010, MUT-001, MUT-002,
MUT-003, MUT-004, MUT-005, MUT-006, MUT-007, MUT-008
```

### CORE closes: nine requirements are now genuinely verified

`crates/vestrace-domain/src/conformance/cases.rs` holds executable cases that
build domain values, exercise the operation, and return `Fail` when the property
does not hold:

| ID | Class | What runs |
| --- | --- | --- |
| TMP-002 | Domain | `Event::new` accepts an absent `occurred_at`, preserves `recorded_at`, and does not invent a source time |
| TMP-003 | Domain | A revision whose validity window closed before it was recorded is accepted, and the window is not stretched to the recording time |
| TMP-004 | Stateful | `Superseded` and `Expired` both refuse `activate()`; `expire()` clears `active_revision_id` so no reader can follow it back |
| TMP-005 | Stateful | Every prefix of the log replays to the status of *that* version, while the current projection stands elsewhere |
| TMP-008 | Domain | A log whose `occurred_at` runs backwards replays cleanly; a log with two events swapped out of sequence is refused |
| TMP-009 | Domain | `authorize_resolution` refuses a mismatched purpose, a mismatched workspace, and a blank authorization reference — the reference alone grants nothing |
| ARC-002 | Stateful | `replay` reconstructs run state from the event log alone and repeats identically |
| ARC-003 | Stateful | Replay leaves the log unchanged, and a drifted projection is refused rather than reconciled against history |
| ARC-006 | Evidence | Supersession advances `state_revision`, retains the superseded revision id, and keeps the memory identity stable |

The replay cases share one fixture whose last two events carry **backwards**
timestamps. That is deliberate: it is simultaneously an ordinary log under a
sequence contract and an inconsistent one under a wall-clock contract, so
TMP-008 has something real to distinguish.

The four remaining CORE requirements — ARC-001, ARC-004, ARC-008, ARC-009 — are
`Static`, and now carry written attestations, which is the correct evidence
form for a claim about architectural shape. ARC-008's attestation says plainly
which half of it is executable (the lease, via TMP-009) and which half is the
attested part.

Thirteen further cases then replaced the behavioural attestations that CORE had
been leaning on:

| ID | Class | What runs |
| --- | --- | --- |
| ARC-007 | Stateful | `RunStepStatus::Unknown` is neither terminal nor active and round-trips through its wire form; the three run-level waiting states are waiting, not terminal |
| TMP-001 | Domain | An event carries a source time six hours before its recording time, with neither field disturbing the other |
| TMP-006 | Domain | `CognitiveMutation` carries `expected_state_revision` and validates it before applying |
| TMP-007 | Stateful | A stale mutation yields `RevisionConflict` naming **both** revisions, so the caller can re-read rather than guess |
| TMP-010 | Stateful | Versions increase by exactly one; a gap and a repeat are both refused |
| MUT-001 | Evidence | Actor, target, expected revision, reason, provenance and resulting revision all survive on the record |
| MUT-002 | Stateful | A correction produces a separately addressable revision and leaves the earlier one byte-identical |
| MUT-003 | Domain | A deterministic reconciliation with no basis evidence is refused at construction |
| MUT-004 | Domain | `accept_ambiguity` is available only to a semantic reconciliation; a deterministic one can neither accept ambiguity nor defer to a human |
| MUT-005 | Stateful | Inputs, basis and outcome are all present on the record |
| MUT-006 | Evidence | The record names its resolver and time and reconstructs identically from its stored bytes |
| MUT-007 | Domain | A correction is a second record; the first is untouched and still separately addressable |
| MUT-008 | Security | A semantic record stays semantic across every operation it permits and gains no basis evidence; a human-required record refuses auto-deferral |

**One of these caught a false attestation.** ARC-007's written claim said
"RunStatus has explicit Unknown/WaitingForInput/WaitingForApproval states".
`RunStatus` has no `Unknown` variant — `Unknown` belongs to `RunStepStatus`. The
sentence had been passing as evidence for a requirement it described
incorrectly. An executed case cannot make that mistake, because it names the
type in order to compile.

**`conformance check core` exits 0.** CORE passes its own gate: 28 passed
(**22 executed**, 6 attested), 0 failed, 0 skipped. All six attestations are
`Static` — ARC-001, ARC-004, ARC-005, ARC-008, ARC-009, ARC-010 — so the
shortfall line is now absent from CORE: no behavioural requirement in the
profile rests on a claim.

One test had to be inverted rather than kept.
`bundle_command_binds_a_capability_manifest_file` asserted that bundling
**fails**, with the message "current evaluator remains incomplete" — it pinned
the fact that CORE could not qualify. It now asserts success, which is what
would catch CORE regressing back.

## The number that got worse, correctly

TRUSTED went from `40 passed, 124 skipped, 35 N/A` to
`57 passed (41 executed, 16 attested), 142 skipped, 0 N/A`.

Seventeen more requirements pass than before, and forty-one of the passes are
now checks that could have failed, against nought before. The skip count is
higher because the 35 orphans are counted at last. The headline is worse; it is
also true for the first time.

## MEMORY: nineteen more executed, and a MUST that was not enforced

The same treatment was then applied to MEM. Nineteen executable cases replaced
the family's attestations and closed its four unregistered requirements, so
MEMORY now reads `48 passed (41 executed, 7 attested), 15 skipped` — the seven
attestations are all `Static`, and the shortfall line is absent from MEMORY too.

Writing MEM-003 turned up a real defect rather than a documentation gap.
`Memory::activate` took a bare `MemoryRevisionId`:

```rust
pub fn activate(mut self, revision_id: MemoryRevisionId, at: Timestamp) -> ...
```

With only an id it could not check anything, so a memory could be pointed at a
revision belonging to **another memory, or another workspace**, and the domain
would accept it. Reading that memory afterwards would return content never
written to it. "Active revision must belong to the same Memory" is a MUST, and
nothing enforced it.

The signature now takes the revision and verifies ownership — a foreign memory
is an `InvalidArgument`, a foreign workspace a `PolicyViolation`, because the
second is a tenancy breach and should not read as a validation detail. Two
application call sites and five test call sites moved across; all of them
already had the revision in hand.

MEM-009 is worth noting for what it checks: `Confidence` and `Importance` reject
NaN and both infinities as well as out-of-range values. A range test written as
`v < 0.0 || v > 1.0` would let NaN through, since NaN fails every comparison.
The case covers it explicitly.

## Still attested where it should execute

CORE and MEMORY are both clean: every remaining attestation in them is `Static`.
LRN still carries behavioural requirements passing on a sentence, and
`attested_but_should_execute()` names them on every run.

MEMORY's fifteen remaining skips are the whole of RET. Some are reachable from
the existing domain — `ContextPack::new` already refuses `used_tokens >
token_budget` (RET-007) and refuses construction when `authorization_checked` is
false (RET-003), and `RepresentationLevel` has the four variants RET-006 asks
for. The rest — exact-revision hydration, channel fusion, invalidation
propagation, the retrieval journal — need a retrieval implementation to
exercise, not just a case to write.

## Test results

Full workspace suite against a live PostgreSQL 17: **796 passed, 0 failed, 127
suites, exit=0** (790 before this work).

## Not addressed here

Items 3–5 of the approved plan: authentication with attribution, production key
custody, and the G6 → H5 → E4 → T8 gate sequence. Item 3 reverses the operator's
standing decision that development runs on one shared administrator account, so
it needs their word before it starts.
