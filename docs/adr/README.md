# Vestrace Architecture Decision Records

This directory contains accepted architecture decisions for the frozen v0.2 documentation baseline and explicitly accepted post-v0.2 architecture extensions.

| ADR | Decision | Status |
|---|---|---|
| [ADR-0001](0001-memory-first-persistent-cognition.md) | Memory-first Persistent Cognition Product Boundary | Accepted |
| [ADR-0002](0002-single-execution-state-engine-boundary.md) | Single Execution / State Engine Boundary | Accepted |
| [ADR-0003](0003-capabilities-are-runtime-authority.md) | Capabilities Are Runtime Authority | Accepted |
| [ADR-0004](0004-repair-follows-authority-hierarchy.md) | Repair Follows the Authority Hierarchy | Accepted |
| [ADR-0005](0005-unknown-external-outcomes-require-reconciliation.md) | UNKNOWN External Outcomes Require Reconciliation | Accepted |
| [ADR-0006](0006-cross-workspace-sharing-is-grant-plus-mount.md) | Cross-workspace Sharing Uses Grant + Mount | Accepted |
| [ADR-0007](0007-trust-restoration-requires-revalidation.md) | Trust Restoration Requires Revalidation | Accepted |
| [ADR-0008](0008-v1-trust-is-a-qualification-contract.md) | v1.0 Trust Is a Qualification Contract | Accepted |
| [ADR-0009](0009-finding-disposition-is-not-integrity-state.md) | Finding Disposition Is Not Integrity State | Accepted |
| [ADR-0010](0010-qualification-profile-scope-follows-evidence-closure.md) | Qualification Profile Scope Follows Evidence Closure | Accepted |
| [ADR-0011](0011-brain-face-organ-system-decomposition.md) | Brain–Face–Organ System Decomposition | Accepted, post-v0.2 extension |

## Status semantics

- **Proposed** — documented but not authoritative.
- **Accepted** — normative decision for the current architecture baseline or an explicitly identified post-baseline extension.
- **Superseded** — replaced by a later ADR; retained for history.
- **Deprecated** — should no longer guide new design but may describe legacy implementation.

## Baseline rule

ADR-0001 through ADR-0010 belong to the frozen v0.2 architecture package.

ADR-0011 and later decisions are post-v0.2 extensions unless a later document explicitly assigns them to another frozen baseline. A post-v0.2 ADR MUST NOT be interpreted as retroactively changing v0.2 implementation availability, qualification claims, or the existing 36-PR v0.2→v1.0 transition package.

## Change rule

An Accepted ADR is not edited to silently reverse its core decision. A material reversal requires a new ADR that explicitly supersedes the old one.
