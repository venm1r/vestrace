# A finding with a history

**Date:** 2026-08-14
**Scope:** durable health findings and occurrences — the step the previous slice
named as next.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice wired the invariant registry and said what was still missing:
"findings are produced, rendered and discarded — nothing persists them, so
`occurrence_count` is always zero in practice, no finding has a lifecycle across
two runs, recurrence and flapping detection have no history to assess." That is
now closed.

## What a stateless check cannot do

Every run built a fresh finding. The consequences were not cosmetic:

- `occurrence_count` was always zero, so HLT-008 (recurrence) held in the domain
  and was inert in the deployment.
- `assess_recurrence` had nothing to read, so HLT-009 (flapping) was in the same
  position.
- A disposition had nothing to attach to. An operator's only options were fix the
  problem or keep reading about it — suppressing or accepting a finding was
  impossible, which makes HLT-015 and HLT-020 unreachable in practice however
  well the domain models them.
- And a problem stopped existing the moment a run failed to observe it, which is
  resolving findings by forgetting them.

## Identity is what makes a history

Migration `0146` adds `health_findings` and `health_occurrences`. The finding key
is `(workspace_id, invariant_id, fingerprint)` — the fingerprint being what the
observer computes for "this violation of this invariant" — so the same problem
seen on two runs is one finding with two occurrences.

`invariant_version` is deliberately **not** part of that key. A check whose
definition changes keeps its finding and records the new version on it;
including the version would silently split the history of a problem at the moment
somebody edited the check that watches it. `HealthFinding::observed_by_version`
exists for exactly that, and says so.

The upsert preserves the stored row id rather than taking `EXCLUDED.id`. A new id
per run would orphan every occurrence written before it — and because occurrences
reference the finding by a composite key including the workspace, that would
have surfaced as a foreign key violation rather than as silent drift.

Both tables are `FORCE ROW LEVEL SECURITY` with a policy from the first
migration. Five batches of that work were spent retrofitting tables created
without it; a new tenant table should not join that list.

## What the monitor does

`HealthMonitorService` observes, folds the results into what is already stored,
and returns what an operator should see:

- an observation matching a stored finding **records an occurrence** on it;
- an observation matching nothing **opens** a finding, with its first occurrence
  in the same transaction, so a finding never exists with a count of zero and no
  history;
- the returned list is every *known* finding, not only the ones observed on this
  run, with `observed_on_this_run` saying which is which;
- a stored finding whose invariant has since left the registry is logged and
  skipped rather than rendered, because rendering it would mean inventing the
  severity and remediation the registry exists to own.

Silenced findings are returned and printed as `[SILNT]` rather than filtered.
Hiding them would make an operator's decision to look away indistinguishable
from the problem having gone — and the lapse logic from the previous slice means
a silence that has expired stops applying without anything having to sweep it.

## Live verification

Three consecutive `vestrace doctor` runs against the deployed stack:

```text
[WARN ] outbox.backlog_within_budget@v1 (seen 1x, since 2026-08-14 07:39:58 UTC): 10 outbox messages …
[WARN ] outbox.backlog_within_budget@v1 (seen 2x, since 2026-08-14 07:39:58 UTC): 10 outbox messages …
[WARN ] outbox.backlog_within_budget@v1 (seen 3x, since 2026-08-14 07:39:58 UTC): 10 outbox messages …
```

The count rises and `since` does not. `GET /v1/system/health` reports the same
finding with `occurrence_count: 4`, `first_seen_at` from the first run,
`last_seen_at` from the latest, `observed_on_this_run: true` and
`status: "open"`. In the database:

```text
health_findings      outbox.backlog_within_budget  v1  open  4
health_occurrences   4 rows, sequence 1..4
schema-wide          58 forced, 31 exempt, 9 without a policy
```

## A defect found by reading the output

The outbox invariant's remediation contained a literal `\n` — a wrapped source
string where the escape was written rather than a line continuation — so the
doctor printed a remediation across two lines with the source file's
indentation in the middle of it, and the console would have rendered the same
text into one cell.

Fixed, and `every_standard_invariant_declares_what_to_do_about_it` now also
asserts that no description or remediation contains a line break. The text is
shown on one line by two consumers; a check that only asserts "not empty" cannot
see that.

## Test results

Full workspace suite against a live PostgreSQL 17: **850 passed, 0 failed, 131
suites, exit=0** (846 across 130 before this slice).

Four new database-backed tests, each covering a property that could not exist
before persistence:

- the same problem seen twice is one finding with two occurrences, keeping its id
  and its sequence numbers;
- a finding not observed again stays open, and does **not** accumulate an
  occurrence for the run that did not see it;
- findings do not cross a workspace boundary;
- an oscillating history is detected as flapping by `assess_recurrence` reading
  stored occurrences — the deployment-side half of HLT-009.

## What is still not wired

`RepairPlan`, `RepairExecution` and `VerificationRun` remain unreachable:
findings can now be recorded and counted, and nothing can yet be repaired
through the plan/authorize/execute/verify path the domain defines and the
conformance cases verify. `HealthOperatorService` is still constructed nowhere,
and no surface exists for setting a disposition — so suppression and accepted
risk are storable now but not settable by an operator.

Those are the next steps, and they need an authorization decision on the repair
path, which is the first place the health subsystem touches the missing
capability-grant store.
