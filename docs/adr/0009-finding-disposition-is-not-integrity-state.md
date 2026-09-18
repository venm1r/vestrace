# ADR-0009: Finding Disposition Is Not Integrity State

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Earlier health-lifecycle wording listed `SUPPRESSED` and `ACCEPTED_RISK` alongside `OPEN`, `VERIFYING` and `RESOLVED`. That conflates two different questions:

1. Is the invariant violation still present?
2. How should operators/policy treat that violation operationally?

A suppressed or accepted-risk violation may still be objectively present, so representing it as an alternative integrity lifecycle state can make raw health appear better than the evidence supports.

## Decision

`HealthFinding` integrity lifecycle and operational disposition are separate.

Canonical integrity lifecycle:

```text
OPEN
ACKNOWLEDGED
REPAIR_PLANNED
REPAIRING
VERIFYING
RESOLVED
REOPENED
```

Operational disposition is a separate governed relation/object, for example:

```text
NONE
SUPPRESSED
ACCEPTED_RISK
MAINTENANCE_WINDOW
```

A finding with `ACCEPTED_RISK` remains open unless verification proves the invariant is restored.

Suppression may affect alerting/operational routing, but MUST NOT change raw integrity truth.

## Consequences

- `raw_integrity` and `operational_health` can differ honestly;
- accepted risk cannot masquerade as repair;
- health aggregation can expose both open defects and their disposition;
- expiry of suppression/risk acceptance does not need to mutate finding history.

## Supersedes / clarifies

This ADR clarifies any earlier v0.2 health specification text that lists `SUPPRESSED` or `ACCEPTED_RISK` as mutually exclusive `HealthFinding` lifecycle states. Those terms are disposition overlays, not integrity resolution states.

## Normative references

- Architecture Contract Block 8.9;
- Health / Repair / Incident Contract §§15, 19 (with this ADR taking precedence where wording conflicts);
- `HLT-018`, `HLT-019`.
