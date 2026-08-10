# ADR-0010 — Qualification Profile Scope Follows Evidence Closure

**Status:** Accepted  
**Date:** 2026-08-10

## Context

The release roadmap uses product milestone labels (`Correct`, `Learn`, `Govern`, `Understand`, `Connect`, `Trust`) while the qualification specification defines named profiles (`CORE`, `MEMORY`, `COGNITION`, `AUTONOMY`, `FEDERATION`, `TRUSTED`). These are related but not interchangeable.

Two ambiguities must be removed:

1. `AUTONOMY` includes crash/fault safety, while incident-level trust recovery/revalidation is completed later under `TRUSTED`.
2. A milestone may implement federation-facing behavior without automatically earning the formal `FEDERATION` profile unless the complete applicable evidence closure exists.

## Decision

### Milestone labels are not qualification profiles

A release milestone name MUST NOT be interpreted as a profile claim unless the release evidence explicitly lists that profile and its full applicable dependency closure passes.

### AUTONOMY crash scope

For the `AUTONOMY` profile, crash/fault safety means deterministic safety of governed execution, repair and external-effect boundaries, including preservation/reconstruction of ambiguous execution/effect state across process failure.

`AUTONOMY` does **not** by itself imply installation-level incident containment, `TrustState` restoration, broad recovery-point governance or post-incident revalidation. Those are `TRUSTED` obligations covered by `REC-*` and later trust qualification.

Therefore v0.6 may claim `AUTONOMY` only if its execution/effect crash-safety evidence and all AUTONOMY dependencies pass. It may not claim `TRUSTED` recovery semantics.

### FEDERATION claim scope

v0.4 may ship governed cross-workspace/federation boundaries. The formal `FEDERATION` profile may be claimed only for configurations where all applicable federation requirements, remote evidence checks and profile dependencies have executable passing evidence. Otherwise the release claim is limited to the implemented Govern/federation boundary behavior.

### Understand milestone

v0.5 `Understand` is a roadmap capability milestone, not a new named qualification profile. Its health/repair evidence becomes part of later `AUTONOMY`/`TRUSTED` dependency closure.

## Consequences

- Release labels and profile claims are recorded separately.
- Partial feature completion cannot be worded as a stronger profile claim.
- `AUTONOMY` crash tests remain required without duplicating the later incident/trust-revalidation model.
- `FEDERATION` remains conditional on evidence closure, not feature presence.
- `TRUSTED` remains the only profile that asserts the full incident/revalidation/data-governance trust contract.
