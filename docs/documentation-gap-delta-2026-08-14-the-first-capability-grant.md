# The first capability grant

**Date:** 2026-08-14
**Scope:** a durable capability grant store, the engine that reads it, and the
surface that issues and revokes grants.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice verified the capability kernel and ended by naming what was
missing: "all three CAP remainders and all 26 GOV requirements rest on the same
missing thing, a capability grant store." This builds it.

## What existed and what did not

`CapabilityGrant` has been in the domain since the G1 slice — subject, operation,
resource scope, validity window, budget, risk ceiling, conditions, revocation —
and eleven conformance cases verify it. Nothing could issue one. There was no
table, no repository and no route, so the deployed engine was
`ConfiguredCapabilityPolicyEngine`: a static list of capability names, granted to
every principal alike, with no expiry and no revocation, printing a warning at
startup that says exactly that.

Now:

- migration `0147` adds `capability_grants`, forced under row level security from
  the first migration rather than retrofitted;
- `CapabilityGrantRepository` / `PgCapabilityGrantRepository` store and load them;
- `StoredGrantPolicyEngine` loads the active grants **for the authenticated
  principal** and hands them to `evaluate_capability_grants`, the function the
  CAP cases exercise;
- `POST /v1/capability-grants`, `GET /v1/capability-grants` and
  `POST /v1/capability-grants/{id}/revoke` issue, list and revoke, all governed
  by `workspace.admin`;
- `[policy] engine = "capability-grants"` selects it, and the **server and worker
  share one store**, so a grant issued over HTTP governs run execution too.

Two details worth naming. The **issuer is the authenticated principal**, never a
body field: it is the one value in an authority record that must not be
assertable. And the **status column carries a check constraint** pairing it with
`revoked_at`, because a revoked grant that can only be recognised by decoding its
payload would put revocation off the query path — and CAP-010 requires revocation
to take effect on the next decision.

## Live: issue, use, revoke

A grant was issued through the API and stored — the first this system has ever
held:

```text
POST /v1/capability-grants                          201
  capability memory.read, operation http.get,
  resource_scope /v1/memories/0000…0000, risk critical
```

A second server was then started on `policy.engine = capability-grants` and
probed with the same credential:

```text
GET /v1/memories/0000…0000   (exactly the granted scope)   404
GET /v1/memories/1111…1111   (a different path)            403
GET /v1/runs                 (no grant at all)             403
```

The `404` is the point: authorization passed and the handler answered "no such
memory". A request reached a governed surface on the strength of a stored grant,
evaluated by the domain kernel, for the first time.

Revocation, through the API:

```text
POST /v1/capability-grants/{id}/revoke   200   status revoked, revoked_at set
POST … again                             403   "capability grant is already revoked"
```

and in the database, one active grant and one revoked grant with its timestamp.

## The finding: exact-scope grants are not usable over HTTP

The probe above needed a grant naming `/v1/memories/0000…0000` **exactly**,
because `evaluate_capability_grants` compares `resource_scope` with `!=`. The
HTTP boundary scopes each request to its own path. So authorizing a real
deployment this way would need one grant per URL, which is not a capability
model — it is an access control list with extra ceremony.

This is a design gap rather than a bug: exact equality is a defensible choice for
a scope like `memory:project-alpha`, and the delegation code already carries the
hierarchical rule (`scope_is_within`, which treats `/` as the separator and
rejects wildcards) that the evaluator does not use. Adopting it in the evaluator
would make `/v1` cover `/v1/memories/…` naturally.

I did not do that here. Changing how the authorization kernel matches scopes is a
semantic change to the thing eleven conformance cases verify, and it deserves its
own slice with its own cases rather than being smuggled in behind a store. What
this slice establishes is that everything *else* on the path works: storage,
subject scoping, expiry, revocation, and the engine's use of the kernel.

**So the deployment still runs `configured-capabilities`.** The grant engine is
selectable and proven at the port and HTTP level; it is not yet the default,
because with exact-scope matching a console using it would need a grant per path.
That is the next slice.

## Test results

Full workspace suite against a live PostgreSQL 17: **855 passed, 0 failed, 132
suites, exit=0** (850 across 131 before).

Five new database-backed tests, each about the half the domain cases could not
reach:

- a stored grant authorizes and names itself in the decision; with none, the
  denial is `DefaultDeny` rather than a near-miss;
- revocation reaches the **next decision** — no restart, no new grant cycle, no
  cache — and a second revocation is refused so the recorded time cannot move;
- a grant issued to another principal does not authorize this one, and the
  loaded set is subject-scoped in the query rather than filtered afterwards;
- grants do not cross a workspace boundary, and a grant claiming another
  workspace is refused by name;
- an expired grant stops authorizing **without being revoked**, and stays
  `active` in the store — expiry is not revocation, and the row records what was
  issued rather than a status somebody has to sweep.

One source-contract test was updated: it asserted the exact call
`build_policy_engine(&config.policy)`, which now takes the grant store. It also
asserts that the grant engine is reachable from configuration, so the store
cannot end up built and unconsulted.

## What this does not close

CAP-001 remains skipped, and its message needs updating in a later pass: grants
can now be issued, but the deployed engine is still the configured list, so
authority *in this deployment* is still a coarse enum. The requirement is about
what runs, not about what is available.

The 26 GOV requirements are unblocked rather than met — delegation, budgets and
cross-workspace sharing all have domain code and now have somewhere for their
grants to live.
