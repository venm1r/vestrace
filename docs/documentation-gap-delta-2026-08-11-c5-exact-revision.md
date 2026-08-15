# Documentation Gap Delta — C5 Exact-revision Retrieval

**Date:** 2026-08-11  
**Checkout:** `E:\Soft\vestrace`  
**State:** dirty and uncommitted; unrelated changes and deleted documentation were preserved.

## Implemented in this bounded slice

- `RetrievalService::search` rejects a request whose workspace differs from the trusted `RequestContext` before invoking a retrieval channel.
- `RetrievalService::build_context` rejects a result from another workspace and passes the trusted principal into `ContextPack`.
- `PgTextRetriever` explicitly scopes the query to the request workspace and joins `memories.active_revision_id` to `memory_revisions` in the same workspace.
- `Current` normalization now forces `MemoryStatus::Active`, so an internal caller cannot widen current retrieval by supplying superseded or expired statuses.
- PostgreSQL candidates now carry the stored `memory_revisions.id` and its content; the adapter no longer generates a placeholder revision UUID.
- `ContextPackBuilder` renders hydrated revision content while retaining the exact `revision_id`; the HTTP candidate projection exposes that revision identity.
- Focused application tests cover both workspace mismatch cases and exact revision/content preservation.

## Evidence

- `cargo test -p vestrace-application retrieval`: passed, 17 retrieval tests.
- `cargo test -p vestrace-domain retrieval`: passed, 3 retrieval tests.
- RED was observed before implementation: the workspace test reached the retriever, and the context test returned the diagnostic explanation instead of hydrated content.

## Remaining gaps and limits

- PostgreSQL runtime qualification was not available in this environment: `DATABASE_URL` is unset, and `docker compose ps` is blocked by permission errors reading `C:\Users\venmi\.docker\config.json` and connecting to the Docker Engine pipe.
- `RET-001` remains partial: classification, capability and cross-workspace share/mount policy checks are not implemented here.
- `RET-002` is partially hardened: `Current` is forced to active status, while temporal/as-of semantics and full lifecycle filtering remain C7 work.
- `RET-003` remains pending: unresolved conflict hydration is not added to this C5 slice.
- `RET-005` is improved by exact revision identity, but typed provenance references for every item remain incomplete.
- `CAP-001` is only bounded by trusted workspace identity here; effective capability/policy enforcement remains the later G2 universal authorization boundary.

This delta records implementation evidence; it does not qualify a conformance profile or replace the frozen v0.2 baseline.
