# Documentation Gap Delta — Q26 Capability-Level Trust Restoration

**Date:** 2026-08-12
**Scope:** progressive capability restoration after trust changes
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `CapabilityRestorationPolicy` requires an explicit stage for every capability
  it can restore and rejects duplicate or empty policy declarations.
- Restoration stages are ordered: diagnostics/read-only, internal deterministic
  writes, semantic mutation, reversible external effects, and irreversible/high-
  risk external effects.
- `Untrusted` and `Revalidating` states expose only diagnostics; they cannot be
  promoted by a capability or administrator role.
- `DegradedTrust` can restore only evidence-backed internal deterministic writes.
  Semantic mutation and external effects remain held behind the trust barrier.
- `Trusted` still requires qualification evidence for non-read-only stages,
  revalidation evidence for semantic/effect stages, and non-empty evidence refs.
- Undeclared capabilities and evidence without references fail closed.

## Evidence

- Focused capability-restoration tests: 6/6 passed.
- Coverage includes Untrusted, Revalidating/Degraded barriers, Trusted staged
  promotion, missing references, and duplicate/undeclared policy rejection.

## Explicit non-claims

This is an application decision boundary; it does not itself issue grants,
replace universal authorization, persist capability restoration state, or prove
provider/KMS/HSM/Vault qualification. Durable runtime wiring and exact release
qualification remain separate gates.
