# ADR-0002: Single Execution / State Engine Boundary

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Repair, recovery, external effects, governance and incidents each need durable execution semantics. Implementing a separate runtime for each would create competing state machines, retry engines and event histories.

Earlier State Engine exploration also considered a generic `Run → Task → Step → Action → Attempt` hierarchy, which overlaps existing Run/Step and typed invocation owners.

## Decision

Vestrace has one durable execution/state boundary.

`Vestrace State Engine` is an internal architectural term, not a separate service, database, public API or second event store.

Canonical execution hierarchy remains Run-first:

```text
AgentRun
├─ ExecutionPlanRevision
├─ RunStep[]
├─ RunEvent[]
├─ RunCheckpoint[]
└─ typed operation references
```

Typed operation owners keep their own attempt semantics.

Repair, recovery, external effects, governance workflows, incident actions and verification use the same execution, capability, policy, audit and persistence infrastructure.

## Consequences

- no parallel retry/event/lease systems;
- cross-cutting recovery semantics can be shared;
- domain-specific attempt state remains with its owning operation;
- new generic Task/Action aggregates require a future ADR proving missing semantics.

## Rejected alternatives

1. Separate Repair Runtime.
2. Separate Incident Runtime.
3. Parallel generic Task/Action/Attempt execution model.
4. State Engine as independent service/API.

## Normative references

- Architecture Contract §3.2, Block 3, Block 9;
- Domain Model §13;
- `ARC-004`, `ARC-005`, `MUT-007`.
