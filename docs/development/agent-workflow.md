# Agent-assisted development

## Ownership

**Sol: orchestrator, architect, reviewer → Terra: persistent builder → Sol: final quality gate.**

These are agreed workflow roles, not requirements for particular currently available models or usage limits.

The architect owns requirements, dependencies, and acceptance boundaries. The builder implements agreed scope and reports observations. A reviewer in a separate context checks the contract, fixture assumptions, and complete production path. Builder self-report is not independent review.

## Task input

Provide the commit, read/write/protected paths, requirement IDs, dependencies, and stop conditions. Link the canonical specification and one plan rather than competing copies with different numbers. If a required authority is missing, record a gap and proposed amendment rather than inventing a bypass.

## Review questions

Which production producer creates the state? Which fixtures fabricate it? Can an old endpoint bypass a guard? Do retry and concurrent edits converge? Would the test pass if the implementation were removed? Which requirement lacks an acceptance case?

## Process quality

Measure detected and escaped defects, rework, execution cost, and reproducibility—not the number of reviews. Reopening P04 demonstrates an ability to correct a verdict, not that the harness can never miss a gap.

This discipline need not become another tooling product. Keep it portable, compact, and useful to Vestrace first.

**Sources:** [gate program](../superpowers/plans/2026-08-26-vestrace-v1-gate-program.md), [recorded evidence](../development-evidence/v1-g0-04-embedding-transition-foundation.md).
