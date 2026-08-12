# Q28 Exact-Environment v1.0 Release Evidence

## Goal

Close the application-side release-evidence boundary without confusing typed
decision composition with live deployment qualification.

## Authority

- `vestrace-version-roadmap-v0.2-to-v1.0.md`, v1.0 exit criteria and release evidence
- ADR-0008, v1.0 as a target-specific qualification contract
- Q25, Q26, and Q27 implementation deltas

## Design

1. Represent exact release/build/configuration/environment identity and schema versions as a target.
2. Require matching observed identity and claimed profile.
3. Compose the existing typed release, runtime, crypto, recovery/fault, and capability-restoration decisions.
4. Fail closed for unavailable decisions, target drift, duplicate/blank references, and unpublished limitations.
5. Expose a probe seam for real deployment evidence while preserving the pure application boundary.
6. Keep live Docker/runtime/provider execution as a separate final gate.

## Verification gate

- RED test before implementation.
- Focused unavailable/pass/drift tests.
- Formatting, workspace tests, no-run compilation, and scoped diff check.
- Exact staged diff review before commit.
