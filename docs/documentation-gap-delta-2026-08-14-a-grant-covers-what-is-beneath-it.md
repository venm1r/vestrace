# A grant covers what is beneath it

**Date:** 2026-08-14
**Scope:** hierarchical scope matching in the authorization kernel, bootstrap
grant seeding, and switching the deployment onto grant-backed authorization.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

The previous slice built the grant store and stopped at a finding: a grant had
to name a URL exactly, because `evaluate_capability_grants` compared
`resource_scope` with `!=` while the HTTP boundary scopes each request to its own
path. I deferred the fix deliberately, since changing how the kernel matches
scopes is a semantic change to what eleven conformance cases verify. This is that
change, with its case, and the switch it unblocks.

## The rule

A grant covers the resource and operation it names **and those beneath them**,
using the rule delegation already had and the evaluator did not use:

```rust
fn scope_is_within(child: &str, parent: &str) -> bool {
    if child.contains('*') || parent.contains('*') { return false; }
    child == parent || child.starts_with(&format!("{parent}/"))
}
```

`selector_is_within` is its dotted counterpart for operations. Both were private
to `delegation.rs`; they are shared now, so "what a grant covers" means the same
thing when a request is authorized and when a delegation is checked for
attenuation. Two different answers to that question in one system is how a
delegated grant comes to permit something its parent did not.

Four properties, each asserted by `exec-cap-002-scope-is-hierarchical`:

- **downward**: `/v1` covers `/v1/memories/0198`, which is what makes a grant
  usable at all;
- **never upward**: a grant for `/v1/memories/0198` does not cover
  `/v1/memories`;
- **at a separator**: `/v1` does not cover `/v1-admin/keys`, and `memory.write`
  does not cover `memory.writer` — a prefix match that ignores the boundary
  silently covers neighbours;
- **no wildcards**: `*` on either side is refused rather than interpreted,
  because nothing defines what it would mean and guessing makes a grant's reach
  depend on a convention no code enforces.

The eleven existing CAP cases pass unchanged, which is the useful signal: the
widening did not weaken any property they assert. CAP-006's mismatch check uses
`memory:project-beta` against `memory:project-alpha`, and neither is beneath the
other.

## The deployment now authorizes from grants

`docker-compose.yml` sets `policy.engine = capability-grants` for the server and
the worker, both against one store.

The bootstrap principal's grants are seeded at startup from the same
`policy.capabilities` list the static engine used, issued through
`CapabilityGrant::issue` into the same table as any other — the pattern the
access-token slice established. Three properties are deliberate:

- they are **ordinary rows**: listable through `GET /v1/capability-grants` and
  revocable one at a time;
- seeding **never resurrects a revoked grant**. The check is by capability across
  all grants including revoked ones, so an operator's withdrawal survives a
  restart;
- they are scoped `/v1` and `http`, which under the rule above is as broad as the
  static list they replace. That is stated rather than hidden: the difference is
  not narrowness, it is that each one belongs to a subject, can expire, and can
  be revoked.

## Live

Every console surface, under real grants:

```text
GET /v1/runs              200      GET /v1/artifacts          200
GET /v1/agents            200      GET /v1/system/health      200
GET /v1/models            200      GET /v1/capability-grants  200
POST /v1/retrieval/search 200
```

24 grants seeded. Then the test that matters — revoke one, change nothing else:

```text
POST /v1/capability-grants/{execution.read}/revoke   200
GET  /v1/runs                                        403  authorization denied
GET  /v1/agents                                      200
```

No restart, no cache to invalidate, and exactly one surface closed. That is
CAP-010 as a deployment property rather than a domain one.

**One honest wrinkle.** The denial reads `CapabilityMismatch`, not `Revoked`,
because the repository loads only active grants — so the revoked one never
reaches the evaluator, which then reports the first failure among the grants that
did. The refusal is correct and the reason is less informative than it could be.
Loading revoked grants so the evaluator can name them would put revocation back
on the hot path of every request; the better fix is probably for the engine to
distinguish "no grant at all" from "your grant was revoked" by asking, once,
after a denial. Recorded, not fixed.

## CAP-001 changed answer, and CAP-005 changed reason

CAP-001 — "capability must be a durable, constrained grant, not a coarse enum" —
was skipped by the previous two slices with the message "authority in this build
is a coarse enum". That is no longer true, so it passes as a `Static`
attestation, with the residual honestly stated: what remains coarse is how
*broadly* the seeded grants are issued, which is a matter of operator practice
rather than of what the model can express.

CAP-005 stays a skip. The boundary now fronts durable grants rather than a
configuration file, which was the reason it was not worth attesting before — but
"every entry point is covered" is still a survey of the code rather than a
property a case can execute, and that has not changed.

## Where the profiles stand

```text
autonomy    79 passed (71 executed,  8 attested),  24 skipped
trusted    102 passed (93 executed,  9 attested),  97 skipped, 0 N/A
```

Remaining: GOV 26, REC 18, EXT 18, QUAL 15, IDW 14, RET 2, LRN 2, CAP 2.

The eight `Static` attestations became nine, which is the first time that number
has risen. It is the correct form of evidence for CAP-001 — the claim is about
the shape of a model, and the behaviour beneath it is verified by thirteen
executable cases — but a rising attestation count is worth watching rather than
waving through.

## Test results

Full workspace suite against a live PostgreSQL 17: **855 passed, 0 failed, 132
suites, exit=0** — unchanged, since the new conformance case runs inside the
existing case-list test.

## What this does not do

The seeded grants are broad and held by one principal, which is a development
posture and is labelled as one in `docker-compose.yml`. Nothing yet issues
narrow grants per user, and there is no login flow to issue them to.

GOV's 26 requirements are now genuinely unblocked rather than nominally so:
delegation, hierarchical budgets and cross-workspace sharing all have domain code
and a place for their grants to live, and the scope rule they depend on is the
same one authorization uses.
