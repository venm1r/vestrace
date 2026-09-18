# Product purpose and boundaries

**Type:** Explanation of the accepted product boundary and target scenarios.

## The problem

A system with a long history must do more than find similar text. It must distinguish sources, interpretations, accepted decisions, corrections, and stale representations. Vestrace separates long-lived knowledge from a particular model and a single agent run.

For example, one document may state that a package is complete, followed by a correction reopening it. Useful context should preserve both sources, avoid treating the old conclusion as current, and identify what remains unverified. This is a target workflow with its own acceptance criteria, not an automatic consequence of storing both texts.

## Memory-first is a priority, not a prohibition on execution

**Memory Engine is the substrate. Persistent Cognition is the capability.** Runs, governance, retrieval, models, and recovery support safe use and modification of accumulated knowledge. A new orchestration mechanism needs a concrete benefit to that workflow; a competitor's feature list alone is not a reason to build it.

Models remain computational resources. Replacing a provider must not replace knowledge identity or automatically increase trust in an answer. Model weights and the current prompt are not Vestrace's database.

## What Vestrace does not promise

Provenance does not prove arbitrary text true. Vestrace cannot promise exactly-once behavior for an arbitrary external service without the necessary operation-specific guarantees. Exported JSON does not transfer authority. A successful build does not qualify security.

Do not present Vestrace as a finished replacement for every agent framework, a standalone language model, a universal no-code editor, or merely a vector database. Search and vectors are mechanisms, not the product definition.

## Who benefits

Developers obtain shared context across tasks and clients. Knowledge editors obtain history and controlled corrections. Operators obtain observable state and recovery-validation procedures. Actual availability is determined by [status](../status.md), not by this statement of purpose.

The first selected extension is [Memory Workspace](memory-workspace.md): read, inspect, correct, import, and transfer knowledge using the existing model rather than another state owner.

**Sources:** [ADR-0001](../adr/0001-memory-first-persistent-cognition.md), [Architecture Contract](../specs/en/vestrace-architecture-contract-v0.2.md).
