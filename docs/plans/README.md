# Vestrace Planning Documents

This directory contains both the current v0.2 → v1.0 transition package and older implementation plans retained for history.

## Current planning baseline

Start here:

- [`v0.2-to-v1.0-pr-specification-index.md`](v0.2-to-v1.0-pr-specification-index.md)
- [`v0.2-to-v1.0-36-pr-execution-matrix.md`](v0.2-to-v1.0-36-pr-execution-matrix.md)
- [`v0.2-to-v1.0-requirement-pr-traceability.md`](v0.2-to-v1.0-requirement-pr-traceability.md)
- [`v0.2-to-v1.0-schema-impact-summary.md`](v0.2-to-v1.0-schema-impact-summary.md)
- [`v0.2-to-v1.0-conformance-case-index.md`](v0.2-to-v1.0-conformance-case-index.md)
- [`v0.2-to-v1.0-migration-rollout-compatibility-contract.md`](v0.2-to-v1.0-migration-rollout-compatibility-contract.md)
- [`v0.2-to-v1.0-release-evidence-gates.md`](v0.2-to-v1.0-release-evidence-gates.md)
- [`v0.2-to-v1.0-risk-register.md`](v0.2-to-v1.0-risk-register.md)
- [`v0.2-to-v1.0-parallelization-map.md`](v0.2-to-v1.0-parallelization-map.md)
- [`future-pr-review-checklist.md`](future-pr-review-checklist.md)

Per-phase current plans use these prefixes:

```text
v0.2-*
v0.3-*
v0.4-*
v0.5-*
v0.6-*
v1.0-*
```

They are transition/implementation specifications subordinate to the normative `docs/specs/**` and accepted `docs/adr/**` architecture.

## Historical plans

Date-prefixed plans in this directory, including older `2026-*.md` files, predate the frozen v0.2 architecture baseline unless explicitly referenced by a current plan.

They are retained for implementation history/rationale and MUST NOT override or be executed in preference to the current v0.2 → v1.0 package.

Likewise, `docs/superpowers/plans/**` is historical unless a current document explicitly cites a specific artifact as implementation evidence or rationale.

## Precedence

```text
normative specs / accepted ADRs
→ current v0.2 → v1.0 planning package
→ current implementation source/migrations/tests for implementation reality
→ historical date-prefixed plans
```

Planning documents never authorize code changes by themselves.
