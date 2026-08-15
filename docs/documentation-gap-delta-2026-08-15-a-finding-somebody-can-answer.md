# A finding somebody can answer

**Date:** 2026-08-15
**Scope:** dispositioning a health finding, and the scope-matching limitation
found while wiring it.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Three slices ago I recorded a wrinkle and deliberately left it:

> `healthy` does not return to true when the cause is gone. Both findings report
> `observed_on_this_run: false` and remain `open` … Left as it is rather than
> changed in passing, because it is a design decision and not an oversight. If it
> were mine to decide, `healthy` would follow what is currently observed and the
> finding would stay open regardless.

That reading was half right and the conclusion was wrong. `MonitoredFinding::is_error`
already exempts a silenced finding — `matches!(severity, Error | Critical) && !self.is_silenced()`
— so the design was never "unhealthy forever". The design was: **an
error-severity finding is answered by a person, and the answer is recorded**.

`FindingDisposition` had `Suppressed` and `AcceptedRisk` variants, an actor, a
reason, a policy version, an audit reference and an expiry. Nothing could produce
one. The decision the design required had no way in, so the summary sat at
`healthy: false` with no path back — not because the semantics were wrong, but
because half the mechanism was missing.

## What was added

- `HealthFindingRepository::find_by_id` — an operator answers the finding they
  read, not an invariant and a fingerprint.
- `HealthMonitorService::disposition` — load, set, save, and return the finding
  as it now stands.
- `POST /v1/system/health/findings/{id}/disposition`, taking
  `kind` (`suppressed` or `accepted_risk`), a reason, an audit reference and an
  optional expiry. Reason and audit reference are required: a silence with no
  author and no record is the thing the type was designed to prevent.
- The policy version stored on the disposition comes from a decision that was
  really made — the handler asks the authorization boundary for
  `workspace.admin` on `health.disposition` against `finding://{id}` — rather
  than from configuration read at render time.

## Live

**Before**, with the deliberately-poisoned outbox finding from an earlier slice
still open:

```text
healthy: False
 - outbox.no_dead_letters          | error   | open | observed_on_this_run False
 - outbox.backlog_within_budget    | warning | open | observed_on_this_run True
 - memory.active_memory_is_embedded| warning | open | observed_on_this_run False
```

**Answered:**

```json
{"kind":"suppressed",
 "reason":"the three abandoned messages were re-queued and delivered; the cause
           was a deliberately misconfigured model",
 "audit_ref":"delta://2026-08-14/a-failure-nobody-could-see"}
```

```text
healthy: True
 - outbox.no_dead_letters | error | suppressed
```

The finding does not disappear and does not stop being an error. It is still
listed, still severity `error`, and now carries who decided otherwise:

```json
{"Suppressed": {"actor_id": "10000000-…-0002",
                "reason": "the three abandoned messages were re-queued …",
                "audit_ref": "delta://2026-08-14/a-failure-nobody-could-see",
                "expires_at": "2026-09-15T00:00:00Z",
                "policy_version": "local-dev-console-v1"}}
```

**And a silence nobody is still choosing does not silence.** Re-answering with an
expiry already in the past:

```text
status right after: open
healthy: False
```

`lifecycle_status_at` reads the expiry at the moment of the question, so a lapsed
disposition never has to be cleaned up by anything — it simply stops applying.
That behaviour was written and, until now, unreachable.

## The limitation this uncovered

Granting the authority took three attempts, and the reason is worth recording.

`scope_is_within(child, parent)` is `child == parent || child.starts_with("{parent}/")`.
That is correct for path scopes — `/v1/memories/x` is within `/v1` — and it does
**not** work for the URI-shaped scopes the resource checks use. A grant scoped to
`finding://` cannot cover `finding://01a000fc-…`, because the prefix test looks
for `finding:///`. The same is true of `memory://` and the purge path wired last
slice: **"may purge any memory in this workspace" is not expressible**, only "may
purge this one".

The literal workaround is `finding:/` — one slash — which does match by prefix
and reads like a typo. The real options are to teach the matcher about `://` or
to stop using URI-shaped resource scopes for grants, and both are larger than
this slice.

The consequence is not theoretical: an operator who cannot express "any finding"
will grant `workspace.admin` on `/v1` with operation `http`, which is broader
than what they wanted and is exactly what the resource-scoped check was added to
avoid.

## What this does not do

- **Nothing un-answers a finding.** There is no route to withdraw a disposition;
  the only way back is a lapsed expiry or another disposition. `Resolved` and
  `Reopened` are reachable only through `apply_verification`, which nothing
  calls.
- **The `AutoRepair`, `ManualRequired` and `NotRepairable` variants remain
  unreachable.** The surface produces the two that silence, because those are the
  two an operator needs. The other three are classifications a repair path would
  set, and there is no repair path.
- **A disposition is not an incident.** The trust module's `Incident` — with
  containment, recovery, revalidation and closure, and seventeen executed
  conformance cases — is still constructed by nothing. Suppressing a finding is
  the small answer; the large one is unbuilt.
- **The expiry is not enforced anywhere but at read time.** Nothing notices a
  disposition lapsing and tells anybody; the finding simply reappears in the next
  health response.

## Test results

Full workspace suite against a live PostgreSQL 17: **878 passed, 0 failed,
exit=0**. One new contract test asserting the disposition route takes workspace
administration. Conformance gate unchanged at **188 passed (178 executed, 10
attested), 11 skipped**.
