# ADR-0011 — Brain–Face–Organ System Decomposition

**Status:** Accepted  
**Date:** 2026-08-11

## Context

The frozen v0.2 architecture defines persistent cognition, one durable execution authority, capability governance, external effects, recovery, trust, and qualification. It intentionally does not prescribe a complete end-user autonomous-agent product topology.

A future Vestrace-based agent needs three distinct responsibilities:

1. persistent cognition and autonomous reasoning;
2. user interaction plus controlled access to the user's host machine;
3. replaceable execution environments for browser, coding, desktop, compute, and other specialized work.

Combining these responsibilities into one process or one container would create ambiguous state ownership, excessive host authority, fragile recovery, and pressure to make UI/runtime-local state canonical.

## Decision

Vestrace adopts the **Brain–Face–Organ** system decomposition as the target architecture for the future autonomous-agent product.

### Brain

The Brain is `Vestrace + a Prime-like autonomous cognition runtime`.

Vestrace remains authoritative for durable identity, cognition, `AgentRun`, memory, evidence, capabilities, policy, effects, recovery, trust, and qualification state. The Prime-like runtime owns active reasoning, planning, working context, RLM-style computation, model routing, goals, and subagent coordination without becoming a competing durable state engine.

### Face

The Face is a replaceable user and host boundary. A Face may package:

- a thin Desktop/CLI/mobile/web client; and
- a local Host Broker exposing explicitly permitted host files, processes, applications, shell, devices, browser integrations, and settings.

The Face may further restrict Brain-granted authority but MUST NOT widen it.

Host authority is the intersection of Brain capability/policy, Face-local policy, and actual OS principal rights.

### Organ

An Organ is a replaceable execution capability. An Organ may be a Docker container, sandbox, VM, remote worker, browser service, GPU worker, desktop environment, or another bounded execution endpoint.

An Organ is a logical capability and is not required to map one-to-one to a container.

The Brain may attach multiple Organs simultaneously. Organs may be disposable and MUST NOT own canonical cognition.

Organ authority is the intersection of Brain capability/policy, Organ-local policy, and sandbox/OS rights.

### System Interconnect

Brain, Faces, and Organs communicate through an authenticated System Interconnect carrying commands, events, observations, artifacts, capabilities, approvals, leases, receipts, heartbeats, reconciliation messages, and telemetry.

The Interconnect MUST NOT become a second state engine, policy authority, scheduler of record, or canonical event store.

### Models

LLMs and other models are compute resources used by the Brain, not the persistent identity of the agent.

Changing a model provider MUST NOT by itself create a new agent identity or discard durable cognition.

## Consequences

- Agent identity survives Face, Organ, worker, and model replacement.
- Closing the user interface does not inherently terminate durable Brain state or running work.
- Host-machine access can be mediated by a local Host Broker rather than broad container mounts or unrestricted remote shell.
- Multiple hosts may attach to one Brain with host-scoped capabilities.
- Multiple specialized Organs may attach to one Brain with independent leases and capability scopes.
- Subagent authority remains attenuated and may be bound to dedicated Organs.
- Organ/Host output is treated as observation/evidence before it may become durable cognition.
- External effects retain Vestrace intent/authorization/receipt/reconciliation semantics.
- Face and Organ local state remains non-canonical unless explicitly projected into Brain-owned durable state.
- The existing frozen 36-PR v0.2→v1.0 roadmap is not silently expanded; implementation sequencing for this system layer requires a separate future transition plan.

## Compatibility

This decision preserves and extends the existing architecture rather than superseding its core laws. In particular it remains compatible with:

- ADR-0002 — one execution/state engine boundary;
- ADR-0003 — capabilities are runtime authority;
- ADR-0005 — unknown external outcomes require reconciliation;
- ADR-0007 — trust restoration requires revalidation.

The detailed normative contract is defined in [`../specs/vestrace-brain-face-organ-system-model.md`](../specs/vestrace-brain-face-organ-system-model.md).
