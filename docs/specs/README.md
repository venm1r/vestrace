# Normative specifications and explanatory guides

The authority hierarchy remains **Architecture Contract → explicitly clarifying newer
Accepted ADR → specialized specifications/invariants**. Guides and roadmaps explain and
plan; they do not override this hierarchy. MUST in a Proposed MW design describes the
proposed contract after acceptance, not a current baseline amendment.

## English reading editions

The [English specification index](en/README.md) provides complete reading editions of the
11 normative documents. All original sections, requirement identifiers, normative strength,
technical states, and examples are retained. Frozen source files remain unchanged.

A translation discrepancy must be resolved against the original and applicable Accepted
ADRs. Editorial translation is neither a new architectural decision nor runtime qualification.

## Decisions, implementation, and history

[Accepted ADRs](../adr/README.md) include the Brain–Face–Organ system decomposition and
clarifications of trust, finding disposition, and qualification scope. The
[architecture guide](../architecture.md) explains their boundaries without claiming every
runtime component is implemented.

[Memory Workspace](../implementation/memory-workspace/README.md) is a separate Proposed
extension. The [roadmap](../roadmap/README.md) prioritizes decisions but cannot replace frozen
release gates. Read [current status](../status.md) for observed implementation/evidence and
[history](../history/README.md) for older records. A code/spec mismatch is a gap, not automatic
permission to weaken the norm.

[Documentation](../README.md) · [Source register](../maintenance/sources.md)
