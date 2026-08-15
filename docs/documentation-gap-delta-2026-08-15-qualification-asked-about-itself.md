# Qualification asked about itself

**Date:** 2026-08-15
**Scope:** conformance cases for the qualification family (QUAL), including the
first ones that had to be written in the application layer.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The qualification family is the machinery that decides whether a release may
claim a profile. Fifteen of its eighteen requirements read `SKIP — No conformance
case registered yet`, which meant the mechanism that judges everything else had
almost never been judged.

**Seventeen of eighteen now pass**, sixteen by execution.

```text
before:  160 passed (151 executed, 9 attested), 39 skipped
after:   174 passed (164 executed, 10 attested), 25 skipped
```

## What the cases ask

- **QUAL-002** — a bundle cannot be built from a report missing any requirement
  its profile closes over. Omit one of CORE's twenty-eight and the bundle is
  refused.
- **QUAL-004** — each profile in the chain closes over everything the previous
  one requires and adds to it, and a CORE report is refused as a MEMORY
  qualification. The closure is enforced, not described.
- **QUAL-005** — the limitations a bundle is given survive into the bundle.
- **QUAL-006** — one skipped requirement out of twenty-eight fails the profile,
  and the bundle over it reports `Failed`, not `Passed`.
- **QUAL-007** — of the 183 pieces of evidence the TRUSTED gate requires, any one
  failing, skipped, inconclusive, not-applicable **or absent** fails the gate. A
  security requirement cannot be outvoted, and deleting evidence is not a way
  through.
- **QUAL-013** — a bundle keeps its report, its suite version, its build,
  configuration and environment identity and its target digest; blank identity is
  refused rather than stored as an empty string; and two pieces of evidence for
  one requirement are refused, because that is an unresolved disagreement rather
  than more evidence.
- **QUAL-014** — the baseline binds to a digest of *what was qualified*.
  Changing the build, the configuration, the environment or the suite version
  each changes the digest and each produces `BaselineMismatch`. The old evidence
  stops applying to the new thing.
- **QUAL-015** — a baseline can be marked stale or invalidated, must say why, and
  afterwards qualifies nothing: the same bundle that passed an hour earlier is
  refused with `BaselineNotQualified`.
- **QUAL-017** — evidence a remote asserted about *itself* fails the gate with
  `RemoteSelfAssertion`, while the identical claim attested by a third party
  passes. Verified failable by removing the check: the case failed with *"a
  remote's claim about its own trust passed the gate, so participating in a
  federation would mean believing whatever a peer says about itself"*.
- **QUAL-018** — a trust claim is a complete TRUSTED bundle against its own
  baseline, publishing its limitations; a FEDERATION bundle, however well it did,
  is refused with `WrongProfile`.

## Four that had to be written a layer up

`vestrace_domain::conformance::cases` can only reach domain types. Fault
injection is an application concern, so these live in
`vestrace_application::conformance_cases`, which the CLI already merges:

- **QUAL-008** — fault injection refuses a blank target digest, keeps the digest
  it was bound to, defaults to the least privileged driver, and is off unless
  configured on. A crash scenario cannot be credited to a build it never ran
  against.
- **QUAL-016** — every environment a destructive scenario can name is an isolated
  one: `Ephemeral`, `DesignatedNonProduction`, `ProviderSandbox`. The case
  matches exhaustively, so adding a variant that names production stops it
  compiling rather than quietly passing.
- **QUAL-009** — the TRUSTED profile closes over eighteen recovery requirements
  including REC-011 and REC-012, and seventeen of them are answered by an
  **executed** case. A trust claim resting on somebody's reading of the code is
  refused by the case itself.
- **QUAL-012** — a manifest that supports only CORE refuses to qualify TRUSTED.
  What is unsupported is said, not discovered.

## Another field nobody could read

`QualificationBaseline` requires a reason to mark itself stale or invalidated —
and stored it in a private field with no accessor. A withdrawn qualification
recorded why and could not be asked. That is the third instance of this exact
shape in three slices (`DataExportPlan::object_revisions`, the recovery types'
evidence, and now this), and all three were found the same way: by trying to
write a case about the thing the field records.

`invalidation_reason`, `profile` and `published_at` are now readable, and QUAL-015
asserts the reason survives.

## The one that still skips

**QUAL-010** — cognitive qualification should check properties rather than exact
model wording. There are no cognitive qualification scenarios at all: nothing
runs a model against a rubric as part of qualification, so there is no place
where wording could be compared and none where properties are compared instead.
A SHOULD over an unbuilt subsystem, and the skip now says so.

**QUAL-001** became an attestation rather than a case, which is correct for its
`Static` class: tests, conformance and qualification are three types with three
lifetimes, and neither of the last two can be produced from the first — a green
suite is not a report, and a passing report is not a bundle.

## What this does not do

- **The cases qualify the qualifier, not the deployment.** They show the gate
  refuses what it should refuse. They say nothing about whether the evidence fed
  to it is true, which is what the other 181 requirements are for.
- **QUAL-007's "183 required pieces of evidence" is a number about a fixture.**
  The gate requires evidence for every MUST in the TRUSTED closure; the case
  builds a passing piece for each and then spoils one. A real qualification has
  to produce those 183 from somewhere, and this deployment does not.
- **QUAL-009 tolerates one unexecuted recovery requirement**, because REC-016 is
  an unimplemented SHOULD. If that changes and a second one lapses, the case will
  fail — which is the intended sensitivity, but it is a threshold rather than a
  property.
- **QUAL-016 proves an enum has no production variant.** That is a real
  guarantee and a narrow one: it says a destructive scenario cannot *name* a
  production environment, not that the environment it names is genuinely
  isolated.

## Test results

Full workspace suite against a live PostgreSQL 17: **875 passed, 0 failed,
exit=0**. Fourteen new conformance cases — ten in the domain, four in the
application layer — one verified failable by mutation.

Remaining skips: EXT 8, GOV 6, IDW 3, CAP 2, LRN 2, RET 2, QUAL 1, REC 1.
