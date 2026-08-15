# The first thing that can be destroyed

**Date:** 2026-08-15
**Scope:** wiring the hard purge — the only irreversible operation in the system
— to a surface, with the authorization it needs.
**Status:** bounded, uncommitted delta. It qualifies no profile and makes no
release claim.

Every conformance slice for the last week ended with the same sentence in
different words: the domain is thoroughly verified and the deployment does
nothing with it. `HardPurgeMemoryService` and `PgPurgeRepository` were both
written, both had conformance cases, and neither was ever constructed. **There
was no way to delete a memory from this system.**

There is now, and it took more than a route.

## What was missing besides the route

- **The authorizer was a fixture.** `DeterministicPurgeAuthorizer` returns a
  fixed reference and an hour's validity to anybody who asks. Wiring it would
  have meant the one irreversible operation authorizing itself, with an audit row
  naming `deterministic-test-approval` as the thing that permitted the deletion.
  `GrantedPurgeAuthorizer` asks the same policy engine everything else consults,
  for `memory.purge` against `memory://{id}` at **critical** risk, and carries
  the decision's identifier forward as the authorization reference. The audit
  names a decision that was really made, against a grant that can be listed and
  revoked.
- **The route's risk was wrong.** `http_risk` made approving a run critical and a
  memory deletion medium. Erasing the record a run might have been approved on
  the basis of is not less consequential than the approval.
- **The capability was wrong.** `DELETE /v1/memories/{id}` would have mapped to
  `memory.write` — so every client that can record a memory could destroy one,
  and the local development configuration, which grants `memory.write` and
  pointedly omits `memory.purge`, would have been granting it all along. A
  contract test now asserts the three verbs map to three capabilities.

## Live

**Denied by default.** With the development configuration exactly as it ships:

```text
DELETE /v1/memories/{id}
  → 403 {"code":"forbidden","message":"authorization denied: CapabilityMismatch"}
```

**Two grants, each naming one memory.** The surface grant says this principal may
call this route; the resource grant says *for this memory*:

```text
memory.purge | http         | /v1/memories/01a0034e-…
memory.purge | memory.purge | memory://01a0034e-…
```

**Then it destroys, and says exactly what it destroyed:**

```json
{"memory_id":"01a0034e-f198-7a83-bd43-c736563c005a",
 "removed":{"memories":1,"memory_revisions":1,"memory_sources":1,
            "search_documents":1,"memory_embeddings":1,"knowledge_relations":0}}
```

```text
rows before: memories=1 revisions=1 sources=1 embeddings=1 documents=1
rows after:  memories=0 revisions=0 sources=0 embeddings=0 documents=0
```

**The audit says what it was for and what it removed:**

```text
target_type | reason                    | approval_id           | removed_counts
memory      | subject exercised erasure | ticket-2026-08-15-001 | {"memories": 1, … }
```

**And a second attempt is a 404, not a cheerful 200:**

```text
{"code":"not_found","message":"… is not in this workspace, so nothing was purged"}
```

That last one is the behaviour added two slices ago when the adapter was found
running unscoped: a purge that removes nothing must not be able to report
success. It is the first time that guard has been exercised through a surface.

## A 403 worth explaining

While verifying, `GET /v1/memories/{id}` answered 403 `OperationMismatch` — for
memories that exist. It is not a defect, and the explanation is worth writing
down because it looks exactly like one.

The grant table shows no broad `memory.read` grant. What it shows is a revoked
narrow one from an earlier slice's live revocation demonstration. The bootstrap
seeder inserts only what is missing and **never resurrects something withdrawn**
— documented behaviour, working. Re-issuing the grant returned the surface to 200
immediately, with no restart.

So a deployment can lock itself out of reading its own memories by revoking a
seeded grant, and that is the design: an operator's withdrawal outranks the
seeder. Worth knowing before somebody reports it as a bug.

## What this does not do

- **Two grants are needed to delete one memory**, and an operator in a hurry will
  grant `http` on `/v1` instead, which is broader than either. The layering is
  deliberate — the surface check and the resource check answer different
  questions, and only the resource grant can name a single memory — but the
  ergonomics push the wrong way, and nothing here mitigates that.
- **The approval reference is not verified.** `approval_id` is whatever the
  caller sends, recorded verbatim. There is no approval store for purges, so it
  is a string an auditor can follow rather than a fact the system checked.
- **There is no confirmation step, dry run or undo window**, though the adapter
  declares reversibility elsewhere in the model. `DELETE` with a body is the
  whole ceremony.
- **Nothing purges anything else.** Events, runs, artifacts and audit entries are
  untouched by this path, and a subject's erasure request covers more than their
  memories. `DeletionRequest`, `DeletionPlan` and `verify_deletion` — the domain
  model for exactly that, with GOV cases passing — remain unwired.

## Test results

Full workspace suite against a live PostgreSQL 17: **878 passed, 0 failed,
exit=0**. One new contract test asserting purge, write and read map to three
different capabilities. Conformance gate unchanged at **188 passed (178 executed,
10 attested), 11 skipped** — this closes no requirement, it makes one reachable.
