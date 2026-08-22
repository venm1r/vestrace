# What Trust still needs

**Date:** 2026-08-22
**Scope:** a survey of the v1.0 Trust phase against the code, not against the
plan.
**Status:** committed delta on `main`. It implements nothing and claims nothing.
It answers one question — how far v1.0 actually is — by reading the repository
rather than the roadmap.

## Why the two gates disagree

The TRUSTED conformance gate reports **199 passed, 0 failed, 0 skipped** — 190
executed, 8 attested, 1 build-verified. It has reported that consistently
through every slice of the last two days.

The v1.0 release gate cannot pass.

They are not measuring the same thing. Conformance asks *are the requirements
satisfied*. The release gate asks *is there evidence they are satisfied in this
exact environment*. A requirement can be demonstrated by a conformance case
operating on domain types while nothing in a running deployment does the thing at
all.

## The three levels

Every concept here can exist at three levels, and this session has found the gap
between them eight separate times — `synthetic_unknown`, `supports_read_back`,
the deadline-less transition count, the table migration 0024's filename promised,
the whole fault-suite evidence chain, and others:

1. **modelled** — a domain type exists, with its invariants and its tests;
2. **stored** — a table and a repository exist;
3. **used** — something outside the domain crate calls it.

An affordance stopped at level 1 or 2 does not have no defects. It has
undiscovered ones. The fault-suite evidence store was fully tested, sat at level
2 for months, and would have rejected the first genuine evidence any deployment
produced — a timestamp comparison assuming truncation where PostgreSQL rounds.
That was found the day it acquired a caller.

## The survey

| Concept | Modelled | Stored | Used outside the domain |
|---|---|---|---|
| `Incident` | yes | yes | yes |
| `TrustStateRecord` | yes | yes | yes |
| `RevalidationRun` | yes | yes | yes |
| `SecretRef` / `KeyProvider` | yes | partly | yes |
| `RecoveryClassification` | yes | no | application only |
| `ContainmentAction` | yes | no | **nothing** |
| `DataPolicy` | yes | no | **nothing** |
| `RetentionPolicy` | yes | no | **nothing** |
| `DataHold` | yes | no | **nothing** |
| `DeletionVerification` | yes | no | **nothing** |
| `DataExportPlan` | yes | no | **nothing** |
| `AuthorizedDataExport` | yes | no | **nothing** |

"Nothing" is literal: zero references in `vestrace-infrastructure`,
`vestrace-cli`, `vestrace-http` or `vestrace-application`. Their only users are
conformance cases inside the domain crate — `DataPolicy` appears sixteen times in
`cases.rs`, `DeletionVerification` ten, `RetentionPolicy` six.

So the requirement that deletion is confirmed by a dependency-aware verification
is satisfied in the sense that the type can do it and a case demonstrates it. No
line of a deployed system does it.

## Against the roadmap

`docs/plans/v0.2-to-v1.0-pr-specification-index.md` puts v1.0 at eight PRs:

```text
T1  Incident + Containment + TrustState
T2  Recovery Classification + RevalidationRun
T3  SecretRef / KeyProvider / Key Lifecycle
T4  DataPolicy / Classification Lineage / Model Boundary
T5  Retention / Hold / Dependency-aware Deletion Verification
T6  Governed Export / Audit Integrity
T7  QualificationBundle / Baseline Lifecycle
T8  TRUSTED Qualification Gate
```

Read against the survey:

- **T1 and T2 are substantially real.** Incidents, trust state and revalidation
  runs are stored and used. `ContainmentAction` and `RecoveryClassification` are
  the parts that are not.
- **T3 is partly real.** Secrets and key providers have production callers, and
  the release gate already consumes crypto qualification.
- **T4, T5 and T6 are modelled and unwired.** Three PR families whose concepts
  have no storage and no caller anywhere outside the domain.
- **T7 was started in this session** — the baseline gained a store and a publish
  command — and its lifecycle is stored but not driven.
- **T8 is the gate**, and it is the thing that cannot pass.

## What the release gate still needs

- **runtime qualification** — producer exists (`--runtime-evidence`);
- **crypto qualification** — producer exists (`--crypto-evidence`);
- **fault suite** — producer built this session; answers `failures=2`;
- **release approval** — producer built this session;
- **recovery qualification** — no producer. It needs observations for eight
  targets: running execution, dispatching external effect, verifying repair,
  stale lease, unfinished workflow, orphan temporary state, unknown outcome,
  divergent history. Each must be *obtained* by driving the system into that
  state and watching what recovery did. For scale, `vestrace-fault-scenario`
  does this for five points of **one** target and was substantial work in its own
  right.
- **capability restoration** — no producer, and no source for the restoration
  policy it needs.

And a producer is not a pass. The gate needs affirmative answers, and the fault
suite's is currently negative on a point whose expectation a previous delta
established may be unsatisfiable against a stub that always answers.

## The honest estimate

There isn't one, and this document declines to invent it.

What can be said: the remaining work is not the tail of a project. Three PR
families — data governance, retention and deletion, governed export and audit
integrity — exist only as domain types. Two release-gate producers are absent,
one needing a harness comparable to the one built for external effects. That is a
body of work of the same order as what has been done, not a finishing pass.

What matters more: **the conformance gate's 199/199 should not be read as v1.0
readiness, and nothing in this repository claims it should.** The release gate
exists precisely to distinguish the two, and it is doing its job — including, as
of this session, by reporting `fault_suite_failed` where it used to report that
nobody had looked.
