# Retrieval classification is now enforced

**Date:** 2026-08-25  
**Scope:** exact-revision classification at the retrieval and ContextPack
boundary.  
**Status:** working-tree delta. It follows the immutable memory-classification
slice; it does not qualify v1.0 or add a classification transition mechanism.

## What changed

Retrieval previously had a reusable
`retrieval::hydration::ClassificationPolicy`, but the production search path
did not apply it. A candidate could therefore reach fusion, reranking, and a
ContextPack without the configured retrieval disclosure decision being made
against the candidate's stored revision label.

`RetrievalService` now requires three things before search or ContextPack
construction can start: an exact-revision hydrator, a `ClassificationPolicy`,
and a non-blank retrieval policy version. Missing any of them fails closed.
The HTTP server and MCP composition roots build that policy from the explicit
`policy.data.retrieval` configuration; missing retrieval policy is not treated
as consent.

Every exact revision discovered by every successful channel is hydrated and
classified before memory-level reciprocal-rank fusion and reranking. An
inadmissible sibling therefore cannot disappear behind an admitted revision,
inflate that revision's fused score, or displace it from the result limit.
Admitted candidates receive the canonical revision content, status, temporal
metadata, and stored classification from hydration rather than trusting a
channel projection.

Withholding remains visible without disclosing the withheld content. Search,
ContextPack, the retrieval journal, HTTP, MCP, and the generated API schema
carry the policy version plus structured entries naming the memory, exact
revision, and withholding reason. Context items carry
`source_classification` from the hydrated revision.

## What was verified

- Application retrieval service: `14 passed`, including missing-policy
  fail-closed behavior, non-disclosing withholding, journaling, and a mixed
  sibling regression that proves an inadmissible revision cannot affect score
  or rank.
- Domain classification cases: `6 passed`; memory-service classification and
  idempotency cases: `9 passed`.
- PostgreSQL retrieval-policy configuration: `5 passed`; exact boundary:
  `1 passed`; revision hydration: `7 passed`; memory classification: `6 passed`.
- The full infrastructure suite passed serially against PostgreSQL 17.
- Real LM Studio ignored suites passed: embeddings `1`, embedding policy `1`,
  completion/model policy `2`.
- The isolated Docker Compose smoke suite passed `4/4` and its temporary
  project resources were removed afterwards.
- The destructive external-effect fault suite independently passed all five
  points with `FAULT_SUITE_DECISION passed=true failures=0`.
- `cargo test --workspace --no-fail-fast -- --test-threads=1`,
  `cargo fmt --all -- --check`, `git diff --check`, and
  `cargo clippy --workspace --all-targets -- -D warnings` exited zero.
- TRUSTED conformance remained `199/199`: 190 executed, 1 build-verified, and
  8 attested.

## The boundary this does not cross

Memory labels remain an unordered configured vocabulary. They are not mapped
onto `Sensitivity` or `DeclassificationDecision`, and there is no implied
"higher" or "lower" label. Creation may state a valid label; revision may
inherit it or repeat the identical trimmed label; every label change is
refused because no approved transition mechanism exists.

This slice does not add capability-grant, share/mount, or conflict governance
to retrieval. Exact and structured retriever ports still have no production
composition root, MCP does not wire the optional vector channel, and no
release approval, signed qualification bundle, or supported-target TRUSTED
evidence is produced by these tests.
