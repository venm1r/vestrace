# Q24 — Progressive Trust Restoration

## Goal

Restore trust only through persisted, exact, evidence-backed revalidation and
Trusted qualification closure, with fail-closed intermediate states.

## Tasks

- [x] Add RED tests for Trusted promotion, missing baseline, failed bundle,
  inconclusive result, and mismatched evidence.
- [x] Load and validate persisted trust, revalidation, and qualification
  records.
- [x] Persist `Revalidating` before evaluating the final transition.
- [x] Require exact manifest identity, qualified matching baseline, and the
  full local Trusted hard-gate decision for `Trusted`.
- [x] Preserve `Untrusted`, `Revalidating`, and `DegradedTrust` outcomes
  without promotion shortcuts.
- [x] Document the implemented boundary and explicit non-claims.
- [ ] Add capability-level restoration policy and release approval in the next
  closure block.

## Verification

- `cargo test --test progressive_trust_restoration -- --nocapture`
- `cargo test --workspace --lib -- --nocapture`
- `cargo test --workspace --no-run`
- `cargo fmt -- --check`
- `git diff --check`
