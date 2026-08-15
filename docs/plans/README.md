# Vestrace Planning Documents — Archive Edition

This archive contains the current v0.2 → v1.0 transition summary and execution matrix needed to understand the planned implementation sequence.

The v0.2 architecture baseline is integrated into `main`. Planning files do not themselves authorize implementation; future code work should use dedicated implementation branches from a reviewed current `main`.

## Included here

- [`v0.2-to-v1.0-pr-specification-index.md`](v0.2-to-v1.0-pr-specification-index.md) — all 36 planned PRs and phase boundaries.
- [`v0.2-to-v1.0-36-pr-execution-matrix.md`](v0.2-to-v1.0-36-pr-execution-matrix.md) — compact dependency/evidence/schema-impact matrix.
- [`future-pr-review-checklist.md`](future-pr-review-checklist.md) — universal review/merge checklist.
- [`../implementation-plan-v0.2.md`](../implementation-plan-v0.2.md) — consolidated implementation roadmap.

## Repository note

The GitHub repository also contains detailed per-phase contracts, migration/backfill matrices, conformance matrices, risk, rollout, release-evidence and parallelization documents. This ZIP intentionally omits repetitive detailed planning files while preserving the complete normative architecture, gap analysis, requirement coverage and the full 36-PR sequence.

Historical date-prefixed plans and `docs/superpowers/plans/**` are excluded from this clean archive.

## Precedence by question

For **target architecture**:

```text
Architecture Contract
→ newer Accepted ADR
→ specialized normative specs / invariant catalog
→ current transition planning
→ historical plans
```

For **what the repository actually does now**:

```text
source / migrations / tests
→ current implementation documentation
→ planning assumptions
```

A planning document never overrides source reality and never authorizes code changes by itself.
