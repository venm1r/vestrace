# IDW-014 Shared Read Adapter Design

**Date:** 2026-08-16  
**Status:** approved design  
**Scope:** implement the smallest executable cross-workspace shared-read path and restricted-role PostgreSQL isolation proof without claiming offline conformance closure

## 1. Problem

The TRUSTED conformance profile currently reports 199 requirements: 196 passed, 0 failed, 3 skipped, and 0 not-applicable. `IDW-014` is one of the three skips. Its normative statement is:

> Cross-workspace access must not require removing RLS or normal workspace isolation.

The domain already models the two-sided sharing decision through immutable `MemoryShareGrantRevision`, `MemoryShareGrant`, target-owned `MemoryMount`, `TargetSharePolicy`, `SharedMemoryRef`, and `evaluate_share_access`. Those types are exercised by domain conformance cases, but no application or infrastructure adapter has ever used them to read a source workspace revision.

The ordinary memory repository cannot fill that role. Its `RequestContext` is the authenticated workspace and principal, and its PostgreSQL implementation scopes every transaction to that context. A target context must therefore see no source rows. Replacing its workspace with the source workspace would manufacture an unauthenticated context; reading through a cross-workspace join, `SECURITY DEFINER`, disabled RLS, a superuser, or a `BYPASSRLS` role would defeat the requirement being implemented.

The existing migration `0087` table is not a sufficient authority source. It records only owner workspace, target workspace, memory, and grant time. It does not represent an immutable grant revision, exact memory revision, operation set, target acceptance, mount lifecycle, source generation, or disclosure.

## 2. Goal

Add a bounded application and PostgreSQL read path in which:

1. the authenticated target identity is checked against the accepted mount;
2. the current source grant, mount, and target policy all authorize `ShareOperation::ReadContent`;
3. successful evaluation creates an opaque, single-use application permit;
4. a dedicated PostgreSQL adapter derives source scope only from that permit;
5. the adapter reads exactly one namespaced source revision while normal forced RLS remains in effect; and
6. content is returned only after row identity is revalidated and the disclosure is recorded.

The runtime proof must use a restricted `NOSUPERUSER NOBYPASSRLS` role and must demonstrate both halves of the boundary: an ordinary target-scoped read cannot see the source row, while the authorized shared reader can return only the exact permitted source revision.

## 3. Truthful evidence boundary

This slice produces executable application tests and a real PostgreSQL integration gate. It does not claim that a passing SQLx test automatically becomes a durable conformance result.

The default CLI conformance report assembles synchronous domain and application cases. It has no database-backed evidence input, and `BuildVerified` is reserved for compiler-enforced invariants. Therefore the slice must not replace `IDW-014` with a mock-only `Executed` case, a static attestation, or a `BuildVerified` result.

The offline fallback is updated to name the narrower remaining blocker: the adapter and restricted-role test exist, but the offline report cannot consume their database-backed evidence. The expected TRUSTED result remains:

```text
199 total
196 passed
0 failed
3 skipped
0 not applicable
```

The remaining skips remain exactly `IDW-014`, `QUAL-010`, and `REC-016`. A later, separately designed DB-backed qualification evidence path must execute or ingest the runtime proof before `IDW-014` can truthfully become an `Executed` pass.

## 4. Security invariants

### 4.1 Authenticated target authority

The service accepts the genuine target `RequestContext`. Before any storage call, both values must match the mount exactly:

- `context.workspace_id == mount.target_workspace_id()`;
- `context.principal_id == mount.target_principal_id()`.

The requested operation is fixed by the use case to `ShareOperation::ReadContent`. Callers cannot substitute a broader sharing operation.

The public read operation accepts no authorization timestamp. The service owns an injected trusted clock, samples it exactly once per read, and reuses that value for the initial access decision, mount reference check, and disclosure recording. A caller therefore cannot choose a past timestamp to revive expired authority. Inactive, suspended, stale, revoked, expired, mismatched, or insufficient authority fails before the reader is invoked.

### 4.2 Raw references are not authority

`SharedMemoryRef` preserves source namespace and exact identity, but it is not sufficient authorization. `MemoryMount::shared_ref` checks exact grant identity and grant/mount activity; operation and current target-policy checks belong to `evaluate_share_access`.

Consequently, the PostgreSQL port never accepts a raw `SharedMemoryRef`, bare `WorkspaceId`, bare `MemoryId`, bare `MemoryRevisionId`, or caller-selected source `RequestContext`.

### 4.3 Opaque single-use permit

The application layer introduces the public nominal type `SharedMemoryReadPermit`. It contains the exact source workspace, memory, revision, grant revision, source generation, and verified target identity needed by the adapter contract. Its fields and constructor remain private to application issuance. Because `vestrace-infrastructure` is a separate crate, the type exposes only non-consuming, read-only identity accessors needed by a port implementation. Those accessors reveal values from an already-issued permit; they provide no constructor or reconstruction path.

The permit has:

- private fields;
- no public constructor;
- no `Clone`;
- no `Serialize` or `Deserialize`;
- no generic conversion from identifiers or `SharedMemoryRef`;
- no public service method that returns a permit to its caller.

`SharedMemoryRevisionReader::read_exact` consumes the permit by value. A successful domain decision may create one permit for one immediate storage operation; a prior `SharedMemoryRef`, disclosure, or permit cannot be replayed as current authority. The service method takes `&mut MemoryMount`, because successful completion records the disclosure on that mount.

Focused compile-time or compile-fail checks verify that the permit is not `Clone`, cannot be serialized or deserialized, cannot be constructed or converted from a `SharedMemoryRef` or bare identifiers, and has no public constructor. These checks cover the unforgeability contract rather than relying on documentation alone.

### 4.4 Source-scoped storage

The dedicated PostgreSQL adapter opens a fresh adapter-private transaction. It must not synthesize `RequestContext::new(source_workspace, target_principal)`, because that pair was never authenticated and would violate the existing repository-context contract.

Inside that same transaction and through the same runtime pool used for the content query, the adapter requires `row_security_active('memory_revisions'::regclass) = true` before reading. A false value is a fail-closed configuration error. This makes active RLS a property of the actual reader connection rather than an unrelated catalog observation.

The adapter sets the source workspace scope only from the consumed permit and queries the exact triple:

```sql
WHERE workspace_id = $source_workspace_id
  AND memory_id = $source_memory_id
  AND id = $memory_revision_id
```

It revalidates the parsed row against all three identifiers before returning it. It does not:

- reuse the target transaction;
- call the ordinary `MemoryRepository` with a substituted context;
- perform a cross-workspace SQL join;
- use `SECURITY DEFINER`;
- alter or disable RLS;
- use a superuser or `BYPASSRLS` role;
- fall back to a latest revision, local lookup, retrieval result, cache, or index.

Missing or mismatched rows fail closed.

### 4.5 Namespaced result and disclosure

The adapter result cannot be mistaken for a local memory. The application returns a namespaced shared projection that retains its `SharedMemoryRef` and exposes content without converting the source reference into a local `MemoryId`.

After an exact row has been read and revalidated, the service rechecks and records one `ReadContent` disclosure before returning content. An absent row, storage error, identity mismatch, or failed disclosure records no successful disclosure and returns no content.

Disclosure remains in-memory in this slice. No durable audit claim is made.

## 5. Components and data flow

### 5.1 Application layer

The application layer adds:

- `SharedMemoryReadService<R, C>` with an injected trusted clock;
- `SharedMemoryRevisionReader` port;
- a narrow shared-read clock port and system implementation;
- opaque `SharedMemoryReadPermit`;
- namespaced shared-revision result.

Its public operation has the following authority-bearing inputs and no caller-selected timestamp:

```text
read_shared(
    context: &RequestContext,
    grant: &MemoryShareGrant,
    mount: &mut MemoryMount,
    target_policy: &TargetSharePolicy,
)
```

The service flow is:

1. compare the authenticated context with mount target workspace and principal;
2. sample the injected trusted clock once;
3. call `evaluate_share_access` for `ReadContent` at that trusted time;
4. obtain the exact `SharedMemoryRef` from the still-active mount and grant;
5. create a private permit;
6. consume the permit through `SharedMemoryRevisionReader`;
7. reject absence or any returned identity mismatch;
8. record the `ReadContent` disclosure using the same trusted time;
9. return the namespaced projection and disclosure.

The ordinary `MemoryRepository` and its `RequestContext` contract remain unchanged.

### 5.2 Infrastructure layer

`PgSharedMemoryRevisionReader` implements the new port. Its transaction setup is deliberately separate from `PgStore::begin_scoped(&RequestContext)`: both set a PostgreSQL workspace setting, but their authorities are different and must not share an API that makes them interchangeable.

The shared reader uses the same restricted runtime-equivalent database identity as normal application access. It obtains scope only through the permit's read-only accessors and consumes that permit through the port call. RLS on `memory_revisions` remains enabled and forced before, during, and after the operation, and the adapter verifies `row_security_active` inside the content transaction.

### 5.3 Error semantics

Application policy failures use `ApplicationError::Policy`. Domain validation failures remain `ApplicationError::Domain`. Database failures use `ApplicationError::Storage`. A missing exact row is not replaced with another row and returns no shared content. A row that violates the permit identity is treated as a fail-closed storage/integrity failure rather than as valid content.

No error path returns the permit or source-scoped transaction to the caller.

## 6. Test strategy

### 6.1 Application RED/GREEN

Focused service tests must first fail because the service and port do not exist. The minimal implementation must then prove:

- the exact target workspace and principal with permitted `ReadContent` invoke the reader once with the exact source namespace;
- a wrong target workspace or principal denies before invoking the reader;
- a source grant lacking `ReadContent` denies with zero reader calls and zero disclosures;
- an active mount lacking `ReadContent` denies with zero reader calls and zero disclosures;
- wrong grant revision, revoked or expired grant, stale or inactive mount, and target-policy denial never invoke the reader or record a disclosure;
- the service uses one injected trusted-clock sample for evaluation and disclosure and exposes no caller-selected authorization time;
- absence or storage failure returns no content and records no disclosure;
- a returned row with the wrong source workspace, memory, or revision is rejected and records no disclosure;
- one successful exact read records exactly one `ReadContent` disclosure and returns a namespaced result.

Compile-time or compile-fail tests additionally prove the permit cannot be cloned, serialized, deserialized, publicly constructed, or converted from raw references or identifiers.

The focused tests use a recording fake only to verify orchestration and fail-closed application behavior. They do not stand in for PostgreSQL/RLS evidence.

### 6.2 PostgreSQL RED/GREEN

The infrastructure integration test must run the reader itself through a restricted runtime-equivalent pool. Through that same identity it records `current_user`, asserts `rolsuper = false` and `rolbypassrls = false`, and proves the role has no membership path to any superuser or `BYPASSRLS` role. It seeds isolated source and target workspaces and at least two source revisions, then proves:

- an ordinary target-scoped `PgMemoryRepository` cannot read the source revision;
- the authorized shared reader returns the exact pinned source revision;
- it cannot substitute another revision in the same source workspace;
- a permit whose declared source workspace does not own the pinned revision returns no content;
- `memory_revisions.relrowsecurity` and `relforcerowsecurity` are true before and after the shared read;
- `row_security_active('memory_revisions'::regclass)` is true inside the adapter's actual source-scoped content transaction;
- no RLS policy, table ownership, role attribute, or migration changes are required.

All fixtures are isolated and cleaned by the test harness. The test must not target the default Compose project or persistent user data.

### 6.3 Mutation proof

At least one temporary mutation must demonstrate that the tests observe the security boundary. The preferred mutation derives source scope from the target context instead of the permit, causing the authorized source read to fail while the direct target-isolation assertion remains true. A mutation that merely removes `memory_id` is not admissible evidence because the globally unique revision primary key could still select the same row.

The mutation is applied and removed with `apply_patch`, produces no commit, and the same focused gate must be RED under mutation and GREEN after restoration.

### 6.4 Conformance truth test

A focused CLI test asserts that IDW-014 remains `Skip`, its message names the implemented adapter/runtime proof and the missing DB-backed conformance evidence path, and the TRUSTED JSON counts remain 199/196/0/3/0 with exactly the same three skip IDs.

## 7. Non-goals and limitations

This slice does not:

- add or change database migrations;
- treat `cross_workspace_memory_grants` from migration `0087` as current authority;
- persist grant revisions, mounts, target policies, permits, or disclosures;
- wire HTTP, MCP, worker, retrieval, context hydration, cache, index, provider, export, or public CLI sharing commands;
- expose the adapter as a production sharing authorization surface;
- validate `source_generation` against PostgreSQL, because `memory_revisions` has no matching field;
- provide durable revocation, cross-process TOCTOU protection, cache invalidation, active-run revalidation, or deletion propagation;
- implement the DB-backed qualification evidence ingestion needed to close the offline conformance skip;
- close `QUAL-010`, `REC-016`, production crypto custody, production release evidence, or exact-environment TRUSTED qualification;
- claim that TRUSTED or Vestrace v1.0 is complete.

Grant provenance is an explicit limitation. `MemoryShareGrantRevision::issue` validates the shape of caller-provided source identifiers; it does not prove that an authenticated source repository issued them. Keeping the service unwired from public surfaces is therefore a security boundary, not merely a packaging choice.

## 8. Acceptance gates

The slice must pass focused application, infrastructure, CLI truth, and mutation gates, followed by:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --no-run
cargo test --test v1_release_evidence -- --nocapture
cargo test -p vestrace-domain --test the_registry_matches_the_specification -- --nocapture
```

The restricted-role PostgreSQL gate must be run in an isolated environment and record the exact role flags and RLS catalog state. The default Compose project must remain unchanged and all isolated containers, networks, and volumes must be cleaned after the gate.

The built CLI TRUSTED parse must remain:

```text
199 total / 196 passed / 0 failed / 3 skipped / 0 not-applicable
IDW-014, QUAL-010, REC-016
```

The exact task range must pass `git diff --check`. The cached diff must remain only the pre-existing nginx rename. Preserved console `dist`, `node_modules`, generated/cache status, and the established generated/cache hash must remain unchanged.

## 9. Commit and review boundaries

The design document is committed independently before implementation planning. The implementation plan must split the work into independently reviewable TDD tasks for:

1. application permit, port, service, result, and focused fail-closed tests;
2. PostgreSQL adapter and restricted-role isolation integration test;
3. truthful IDW-014 fallback and CLI count/message test;
4. final isolated runtime gates and durable report.

Each implementation task receives an independent spec-compliance review and code-quality review. A final broad review evaluates the complete slice, especially the permit boundary, absence of synthesized `RequestContext`, exact SQL predicates, truthful conformance status, runtime role, RLS state, and preservation of unrelated dirty/generated files.

## 10. Truthful completion statement

After every accepted gate passes, the slice may state:

> IDW-014 shared-read adapter and restricted-role PostgreSQL isolation evidence complete; offline TRUSTED remains open because database-backed conformance evidence is not yet ingestible.

It must not state that `IDW-014` is a conformance PASS, that production sharing exists, or that TRUSTED or Vestrace v1.0 is complete.
