# H1–H5 Understand Health and Repair Implementation Plan

> **Execution note:** follow the test-driven development skill for each bounded block: add a focused failing test, implement the smallest contract that satisfies it, then run the focused and workspace checks.

**Goal:** implement the v0.5 Understand chain for invariant health, immutable repair, bounded recurrence handling, the operator contract, and the conformance gate without introducing hidden mutation or claiming runtime persistence that is not implemented.

**Authority:** `docs/specs/vestrace-normative-invariants-v0.2.md`, `docs/specs/vestrace-domain-model-v0.2.md`, `docs/specs/vestrace-qualification-conformance-spec-v0.2.md`, and `docs/plans/v0.2-to-v1.0-36-pr-execution-matrix.md`.

## Bounded blocks

1. **H1 — invariant and finding evidence.** Add stable invariant definitions, logical findings, separate occurrences, explicit scope/severity/repairability, and aggregate health as a dependency-aware projection where `unknown` is never healthy.
2. **H2 — repair lifecycle.** Add immutable repair plans with precondition fingerprints, capability/policy authorization input, stale-plan rejection before execution, and verification-controlled finding closure.
3. **H3 — bounded recovery behavior.** Add recurrence/flapping assessment, attempt budgets/cooldown, and explicit suppression/accepted-risk dispositions with actor, reason, expiry/policy, and audit references.
4. **H4 — operator contract.** Add typed inspect/plan/repair phases and CLI/API argument contracts. Repair accepts an exact immutable plan and authorization; it must not perform an ad-hoc rebuild or hidden mutation.
5. **H5 — Understand gate.** Add a milestone gate for the HLT requirements and Understand scenarios. Keep Understand as a milestone label rather than adding it to `QualificationProfile`, per ADR-0010.

## Verification gates

- focused H1–H5 integration tests and fixture parsing;
- domain/application/CLI unit tests;
- `cargo check --workspace`;
- `cargo test --workspace --lib -- --nocapture`;
- `cargo test --workspace --no-run`;
- `git diff --check` and scoped formatting checks;
- independent review before the completion claim.

## State boundary

This slice is additive and contract-first. It does not add database migrations or silently rewrite the legacy `rebuild` command. Existing dirty work and generated evidence remain untouched.
