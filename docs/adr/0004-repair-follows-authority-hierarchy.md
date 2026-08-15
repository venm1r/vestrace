# ADR-0004: Repair Follows the Authority Hierarchy

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Automatic repair is useful for derived corruption and index drift, but dangerous if repair logic guesses semantic truth or rewrites canonical state based on a less authoritative projection.

## Decision

Vestrace may automatically repair only what is deterministically reconstructible from a more authoritative layer.

```text
authoritative source
        ↓
canonical state
        ↓
derived state
        ↓
indexes / caches / projections
```

Automatic repair may move **down** this hierarchy (for example canonical Memory → rebuild retrieval projection), but MUST NOT move upward based only on derived evidence.

Semantic contradiction, ambiguous truth selection and destructive canonical mutation are reconciliation/governance problems, not deterministic repair.

Repair always uses:

```text
HealthFinding
→ immutable RepairPlan
→ authorization
→ RepairExecution
→ Verification
```

## Consequences

- derived corruption can be auto-healed safely;
- repair cannot make health green by corrupting source truth;
- semantic conflicts remain explicit;
- repair verification becomes mandatory for closure.

## Rejected alternatives

1. Checker directly mutates whatever appears inconsistent.
2. Projection/index used to reconstruct canonical state automatically.
3. LLM chooses truth during health repair without reconciliation policy.

## Normative references

- Architecture Contract Block 8;
- Health / Repair / Incident Contract;
- `HLT-006..013`, `ARC-003`.
