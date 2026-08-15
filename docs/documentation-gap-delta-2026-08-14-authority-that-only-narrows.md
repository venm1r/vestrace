# Authority that only narrows

**Date:** 2026-08-14
**Scope:** executable conformance cases for CAP-002 through CAP-014, and honest
skips for the three CAP requirements that are claims about the deployment.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Fourteen hundred lines of `security/capability.rs` and `security/delegation.rs`
that no case had ever executed — the same position HLT was in before the previous
two slices. Eleven of the fourteen CAP requirements are properties of those
types. They are cases now.

## The eleven

Each builds domain values, exercises the operation the requirement names, and
returns `Fail` if the property does not hold. The ones worth reading for what
they assert rather than what they cover:

- **CAP-006** checks that *each* of subject, capability, operation, resource and
  validity is independently sufficient to deny — and that the denial names which
  one failed. A near-miss grant that gets through on four out of five is the
  whole shape of an authorization bypass, and a denial that will not say why
  sends an operator to the wrong fix.
- **CAP-007** asserts that context risk *raises* effective risk, so a low-risk
  action in a critical context fails a ceiling it would otherwise pass. Checking
  only the requested risk would let a caller set the number it is judged by.
- **CAP-008** separates planning from spending: a request inside the grant's
  declared budget is allowed, and the reservation still refuses a charge that
  would exceed it. A budget checked only at planning time is a budget that
  authorizes the first call and every call after it.
- **CAP-009** asserts that a parent's own remaining spend *drops* when a child
  reserves, and that a second root cannot be registered. Two hierarchies each
  believing they own the limit is how a hard limit stops being one.
- **CAP-003 / CAP-011** cover delegation from both ends: a child cannot widen the
  capability, operation, scope, risk, budget or validity, cannot drop a parent's
  condition, cannot be issued to the delegator themselves, cannot be created
  from an ordinary grant standing in for a `capability.delegate` permission, and
  cannot outlive its parent or exceed its contract's depth.

## What CAP-014 surfaced

The case failed on its first run, on this:

```rust
if !grant.is_active_at(at) { /* before the revocation */ }
```

`is_active_at(at)` evaluates the validity window against `at` and evaluates
revocation against the *current* status — so a revoked grant reports itself
inactive at every instant, including ones before it was revoked. My case had
assumed the temporal reading.

The behaviour is right and the asymmetry is deliberate: answering "was it active
then" faithfully would let a caller presenting an older instant be authorized by
a grant that has since been revoked, which is exactly the stale-authority hole
CAP-014 is about. What was missing was any statement that this is a choice. A
method that takes an instant and honours it for one half of its logic reads as a
bug until something says otherwise — the same shape as `lifecycle_status`
ignoring disposition expiry two slices ago, which *was* a bug.

So the fix is documentation, and the case now asserts the property that was
actually intended: a revoked grant is unusable at every instant, the decision
made before the revocation is preserved as a value, and `revoked_at` is recorded
for a reader who wants the history. No behaviour changed.

That is a different outcome from the HLT round, where two cases found two real
defects, and worth naming as such: here the domain was right and the code was
silent about why.

## The three that are not about the domain

CAP-001, CAP-005 and CAP-012 are claims about the running system. This build
meets none of them, so they are skipped with reasons rather than attested —
attesting them would be false.

**CAP-001** — "capability must be a durable, constrained grant, not a coarse
enum." `CapabilityGrant` is exactly that, and CAP-002..014 verify it. Nothing
issues one: there is no grant table, no repository and no issuance route. The
deployed engine is `ConfiguredCapabilityPolicyEngine`, which allows a static list
of capability enum values with no subject scoping, expiry or revocation, and
reports `ConfiguredAllowance` rather than `GrantMatched` precisely so it cannot
be mistaken for a grant. **Authority in this build is a coarse enum.**

**CAP-005** — every entry point through the boundary. The HTTP router, the MCP
surface and the worker do pass through it; what they pass through it *to* is the
configured static engine. Verifying "every" is a survey rather than an executable
property, and it is not worth attesting while the thing behind the boundary
grants by configuration.

**CAP-012** — effective authority as the intersection of Brain, Face-local and
host/organ rights. There is no Face and there are no organs. One of three terms
exists.

## Where the profiles stand

```text
core        28 passed (22 executed,  6 attested),   0 skipped   exit=0
memory      61 passed (54 executed,  7 attested),   2 skipped
cognition   67 passed (60 executed,  7 attested),   4 skipped
autonomy    78 passed (71 executed,  7 attested),  25 skipped
trusted    101 passed (93 executed,  8 attested),  98 skipped, 0 N/A
```

TRUSTED was `90 passed (82 executed, 8 attested), 109 skipped`. **Past half** of
the v1.0 gate, and every attestation is still `Static`.

Remaining: GOV 26, REC 18, EXT 18, QUAL 15, IDW 14, CAP 3, RET 2, LRN 2.

## The shape of what is left in CAP

All three remaining CAP skips, and all 26 GOV skips, rest on the same missing
thing: **a capability grant store.** The domain kernel is verified now, which
means the store can be built against a model that has been shown to attenuate
rather than amplify — but until grants can be issued, scoped to a subject,
expired and revoked, authorization in the deployment is a configuration file with
a warning printed at startup.

That is the largest single block left, and it is a subsystem rather than a slice:
a table, a repository, an issuance surface, and the removal of the stopgap engine
once something real can replace it.

## Test results

Full workspace suite against a live PostgreSQL 17: **850 passed, 0 failed, 131
suites, exit=0** — unchanged, because the new cases run inside the existing
case-list test rather than adding tests of their own.
