# 11. Implementation handoff and start decisions

## Delivery status

The original proposal reviewed selected sources at `6f610253`; it did not run Vestrace,
PostgreSQL, Rust suites, browser E2E, upgrades, or import/export. Original verification reports
cover their own documentation editions. This English edition uses supplied archive `3e05dfbd`
and records documentation-only checks separately under [maintenance](../../maintenance/README.md).

Integration as a proposed package is not implementation or v1.0 acceptance. Begin with MW-00;
resolve occupied scope and source drift before code. A documentation commit cannot remove
that prerequisite.

## Decisions to accept

Explicitly accept or amend MW-D01–MW-D12: additive API, one writer, source/manual revision
separation, bounded whole-document import, exact preview, atomic items/partial batch, existing
outbox, Missing rather than Delete, ordinary materials, portability without authority, and
feature-scoped qualification. Separately decide release placement: named independent milestone
or an authorized amendment. MW cannot silently join or weaken P01–P12.

## Implementation sequence

1. Pin fresh HEAD and review the delta from the original baseline, including latest P04/14E
   evidence and the occupied 0196 candidate.
2. Complete MW-00 with exact paths, source references, supported test environment, and collision
   checks for modules/tables/routes.
3. Resolve ordinary-material owner/read, content policy, idempotency compatibility, shared lock
   order, and qualified-tokenizer dependencies. Do not substitute mocks for missing authority.
4. After scope approval, implement MW-01/02 with valid behavioral RED; classify setup failure separately.
5. Record observations/limitations after every task. Independent review checks the actual final
   diff and requirements; advisory plan review is not final code review.
6. Do not claim feature-complete until the whole selected gate passes, including rendered output,
   synchronization, and preserved editorial changes.

## Prohibited shortcuts

No second vault/event store/scheduler; disabled generation/label/role checks for demos; fake
success receipts; silent loss of edits; imported trust as local authority; direct client SQL;
byte-only output sold as hard-token qualification; or inherited gates across unvalidated commits.

## Per-task evidence template

```text
Task / requirements / exact input baseline
Changed files and scope authorization
Environment and role identities (without secrets)
RED command / actual assertion / exit
Implementation revision
GREEN command / counts / exit
Mutation / restore digest / GREEN
Independent review status and reviewed revision
Remaining blockers; what was not run
```

Fill this with observations, not invented PASS. Fixture UUIDs are synthetic. Proposed symbols
and test targets are not assumed existing. The proposed OpenAPI subset does not replace
vestrace schema http.

## Definition of documentation delivery

Links, schema, examples, traceability, scope, and dependencies must agree. Implementation readiness
is a separate MW-07 gate on a real installation. Documentation integration establishes only the
first boundary.
