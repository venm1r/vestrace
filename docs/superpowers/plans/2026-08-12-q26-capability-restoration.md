# Q26 Capability-Level Trust Restoration

## Goal

Make post-incident restoration progressive and capability-specific so that
recovery never turns into an all-capabilities trust switch.

## Authority

- `docs/specs/vestrace-health-repair-incident-contract-v0.2.md`, §36
- `docs/specs/vestrace-trust-authority-model-v0.2.md`, §18.4
- `docs/specs/vestrace-normative-invariants-v0.2.md`, `REC-*`, `GOV-*`, `QUAL-*`
- `docs/adr/0007-trust-restoration-requires-revalidation.md`

## Design

1. Require explicit capability-to-stage policy declarations.
2. Keep diagnostics/read-only available while Untrusted or Revalidating.
3. Allow DegradedTrust to restore only evidence-backed deterministic writes.
4. Require qualification evidence for every non-read-only stage.
5. Require revalidation evidence for semantic and external-effect stages.
6. Reject unknown capabilities, duplicate policy entries, and unreferenced
   evidence without a default allow path.

## Verification gate

- RED test before implementation.
- Focused integration tests for all trust/stage barriers.
- Formatting, workspace tests, no-run compilation, scoped diff check.
- Independent review before commit.
