# ADR-0001: Memory-first Persistent Cognition Product Boundary

**Status:** Accepted  
**Date:** 2026-08-10

## Context

Vestrace historically combined memory, knowledge, execution, routing and agent-oriented surfaces. Without a clear product boundary it can drift into a generic agent framework, workflow engine or vector database.

## Decision

Vestrace is defined as:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

And:

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

Execution, retrieval, governance, federation, health and external effects exist to preserve, use and safely evolve persistent cognition; they do not replace the product center.

## Consequences

### Positive

- memory/provenance/temporal correctness remain architectural priorities;
- multi-agent continuity has a stable shared substrate;
- derived retrieval/vector features cannot become source of truth;
- roadmap can reject features that add orchestration breadth without cognition value.

### Negative / Cost

- generic agent-runtime features may be intentionally deferred;
- some execution features must integrate with cognition/provenance rather than exist as standalone abstractions.

## Rejected alternatives

1. **Vector database product** — too narrow; similarity search is derived mechanism.
2. **Generic agent framework** — loses durable cognition as primary capability.
3. **Workflow engine first** — creates orchestration-centric architecture and duplicates external runtimes.
4. **Event store first** — event history alone does not define cognition semantics.

## Normative references

- Architecture Contract §1, §3;
- Domain Model authority tiers;
- `ARC-001..010`, `MEM-*`.
