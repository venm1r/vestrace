# Findings come from a registry

**Date:** 2026-08-14
**Scope:** HLT-001 — wiring the invariant registry, and removing the ad-hoc
check path it replaces.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice verified nineteen HLT requirements against a health domain
that nothing called, and said so: "nineteen requirements are now verified
against code that nothing calls." This closes that gap for the twentieth
requirement, which is the one that was about the running system rather than the
domain.

## What was there

`PgDiagnosticsRepository` held eight SQL methods. Each ran a query **and**
decided what the result meant — a code it invented, a severity, a message and a
remediation string, all written into the adapter:

```rust
DiagnosticFinding::warning(
    "OUTBOX_LAG",
    None,
    &format!("{pending_count} outbox messages pending for more than 60 seconds"),
    "process the outbox queue or check worker health",
)
```

There was therefore no list of what the system claims to check, no version on
any check, and nothing to stop a ninth check appearing with its own vocabulary.
`InvariantRegistry` — the domain type with versions, fingerprints, lifecycles
and dispositions — had no caller anywhere. That is exactly what HLT-001
forbids.

## What replaces it

Three pieces, with the boundary in the middle:

- **`standard_invariants()`** is the catalogue: the only place a check's
  identity, version, severity, repairability and remediation are decided. Eight
  entries, one per former hand-written check.
- **`InvariantObserver`** measures. `PgInvariantObserver` runs the same SQL and
  returns `InvariantObservation` — an invariant id, a scope, a fingerprint, an
  observed state, evidence, and the sentence describing *this* violation. It
  decides nothing about what any of that means.
- **`HealthInspectionService`** joins them, and **refuses an observation naming
  an unregistered invariant**. Dropping it would hide a check whose author
  believed it was running; passing it through would reintroduce the ad-hoc
  finding the requirement is about. So it is an error.

`InvariantDefinition` gained a `remediation` field, validated non-blank. It
belongs to the invariant rather than to each observation: the action follows
from what was violated, not from which run noticed. A violation an operator
cannot act on is a notification, not a finding, and requiring the text at
definition time means that gap shows up when the invariant is written rather
than when somebody is paged.

## One producer, not two

The old path is gone rather than left beside the new one:

```text
deleted   crates/vestrace-infrastructure/src/postgres/diagnostics_repository.rs
deleted   DiagnosticsRepository, DoctorService   (application)
deleted   crates/vestrace-domain/src/diagnostics.rs
          — DiagnosticFinding, DiagnosticReport, DiagnosticSeverity, KnowledgeRef
```

Both consumers moved: `vestrace doctor`, `vestrace rebuild`'s post-rebuild
verification, and `GET /v1/system/health`. There is now exactly one finding type
in the system, `HealthFinding`, and one place that decides what a measurement
means.

This is the part that would have been easiest to skip. Leaving
`PgDiagnosticsRepository` in place and adding the registry beside it would have
produced two health mechanisms, which is the shape this repository has been
punished by repeatedly — two run write paths, two ways to establish a workspace
scope, an event store with a parallel projection writer.

## The HTTP contract changed

`GET /v1/system/health` keeps its shape and gains two fields:

```json
{
  "code": "outbox.backlog_within_budget",
  "invariant_version": "v1",
  "severity": "warning",
  "message": "10 outbox messages have waited more than 60 seconds",
  "remediation": "process the outbox queue or check worker health; note that no component currently drains the outbox",
  "fingerprint": "outbox.backlog_within_budget:10000000-0000-0000-0000-000000000001"
}
```

`code` values changed from shouted constants (`OUTBOX_LAG`) to invariant ids.
`invariant_version` and `fingerprint` are new and are the point: a reader can
tell a check whose definition moved from one that did not, and can recognise the
same violation across runs. The console's `SystemHealthFinding` type was updated
to match.

## Live verification

Rebuilt and redeployed; both consumers read through the registry.

```text
$ vestrace doctor
Checking 8 registered invariants
Running diagnostics for workspace 1000…0001 ... done
[WARN ] outbox.backlog_within_budget@v1: 10 outbox messages have waited more than 60 seconds
         remediation: process the outbox queue or check worker health; note that
                      no component currently drains the outbox
Doctor: 1 warning(s) found, no errors
```

and `GET /v1/system/health` returns the object above, with
`database_role: vestrace`, `is_superuser: false`, `bypasses_rls: false`.

The finding it reports is the outbox backlog the doctor first surfaced two
slices ago. Its remediation now says plainly that nothing drains the outbox,
rather than telling an operator to "check worker health" for a queue that has no
consumer.

## Where the profiles stand

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      61 passed (54 executed,  7 attested),   2 skipped
cognition   67 passed (60 executed,  7 attested),   4 skipped
trusted     90 passed (82 executed,  8 attested), 109 skipped, 0 N/A
```

**HLT is closed: 20 of 20, all executed.** Remaining skips: GOV 26, REC 18,
EXT 18, QUAL 15, IDW 14, CAP 14, RET 2, LRN 2.

## What this does not do

The health *subsystem* is still not wired — only its inspection half is.
Findings are produced, rendered and discarded: nothing persists them, so
`occurrence_count` is always zero in practice, no finding has a lifecycle
across two runs, recurrence and flapping detection have no history to assess,
and no disposition can be recorded because there is nothing to record it
against. `RepairPlan`, `RepairExecution` and `VerificationRun` remain
unreachable, and `HealthOperatorService` is still constructed nowhere.

So HLT-001 is met and the requirements that depend on durable findings are met
by the domain rather than by the deployment. Persistence is the next step, and
it is a schema change rather than a wiring one.

One naming problem left behind deliberately: `HealthRepository` in the
application layer is the *readiness probe* — a single `check()` answering "is
the database reachable" — and has nothing to do with health in the sense this
module now means. It is documented as such rather than renamed, because renaming
it touches the probe path and this slice already touched enough.

## Test results

Full workspace suite against a live PostgreSQL 17: **846 passed, 0 failed, 130
suites, exit=0**. One net test fewer than the previous slice: the `DoctorService`
unit tests went with the code they covered, and five new ones arrived with the
inspection service and the HLT-001 case.
