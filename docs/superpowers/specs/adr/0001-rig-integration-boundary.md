# ADR-0001: Rig Integration Boundary and Agent Runtime Spike

**Status:** Accepted  
**Date:** 2026-07-31  
**Decision owners:** Vestrace maintainers  
**Related design:** `docs/superpowers/specs/2026-07-31-vestrace-harness-v0.2-design.md`

## Context

Vestrace needs production-grade model-provider integration, streaming, structured output, tool-call parsing and, potentially, a bounded model↔tool loop. The Rust `rig` project can accelerate those capabilities, but Vestrace must retain ownership of durable runs, policies, budgets, approvals, memory, artifacts, tools, checkpoints and audit.

Directly adopting Rig domain types or treating `rig::AgentRun` as the authoritative Vestrace run would couple persistence, public contracts and execution semantics to a fast-moving external API. Avoiding Rig entirely would require Vestrace to duplicate mature provider and completion abstractions.

## Decision

Vestrace uses Rig only through an anti-corruption layer as a replaceable infrastructure dependency.

```text
vestrace-domain
vestrace-application
        │
        ▼
vestrace-model-runtime
        │ Vestrace-owned ports and DTOs
        ▼
├── vestrace-provider-openai-compatible
└── vestrace-rig-adapter
```

No Rig type may appear in:

- `vestrace-domain`;
- application service signatures;
- PostgreSQL schemas;
- public HTTP, event, MCP or extension contracts;
- authoritative `AgentRun`, `RunStep` or checkpoint records;
- Agent Profile, Skill or Workflow definitions.

## Native OpenAI-compatible adapter

Vestrace maintains a production-grade but deliberately narrow native adapter. It is not a test stub and must support the baseline vertical slice without Rig:

- text completion;
- streaming;
- structured output;
- tool-call parsing;
- embeddings;
- usage accounting;
- timeout and cancellation when supported;
- normalized provider errors;
- custom endpoint, headers and credential references.

Provider-specific audio, image, transcription and nonstandard reasoning controls are outside this adapter unless later approved by a separate decision.

## Rig provider adapter

The Rig adapter may provide broader provider coverage and provider-specific features, but it implements the same Vestrace-owned `ModelProviderPort` and returns Vestrace-owned results.

Model routing selects a `ModelDefinition` and provider binding. It never selects a Rust library directly. Policy eligibility, data classification, budget reservation, model independence and fallback remain Vestrace responsibilities.

## Tool boundary

Rig never receives real credentials and never performs external side effects directly. Any Rig-visible tool is a proxy into the Vestrace Tool Runtime:

```text
Rig tool call
→ VestraceToolProxy
→ Policy Engine
→ approval validation
→ credential lease
→ budget reservation
→ execution adapter
→ verification and audit
```

A hook or request patch may reduce the visible tool set, but authoritative authorization is repeated immediately before execution.

## Agent-loop decision gate

Use of `rig-agent` as the bounded inner-loop engine is not approved automatically. A mandatory technical spike, `H0-RIG`, must run after the Vestrace model-loop, policy and journal ports exist and before production implementation of the model↔tool loop is selected.

The spike must prove:

1. Vestrace can stop execution before a real tool call.
2. Every invocation passes through Vestrace authorization and budget enforcement.
3. Rig receives no credentials and cannot bypass the Tool Runtime.
4. The loop can pause between model output and tool execution.
5. Restart recovery does not duplicate an external side effect.
6. Recovery is possible from a versioned checkpoint or the canonical Vestrace journal.
7. An `Unknown` external outcome is preserved and reconciled rather than blindly retried.
8. Streaming and non-streaming paths emit equivalent canonical execution events.
9. Usage, errors and tool calls normalize without Rig types escaping the adapter crate.
10. Updating Rig requires changes only inside the adapter and its conformance tests.

Possible decisions:

- `Accepted`: use `RigModelLoopAdapter` behind `ModelLoopPort`;
- `AcceptedWithRestrictions`: use selected Rig parsing, hooks or state-machine components only;
- `Rejected`: implement `NativeVestraceModelLoop`.

The outcome is recorded in a follow-up ADR. Until then, production plans must not assume `rig-agent` is the selected loop engine.

## Checkpoint compatibility

Serialized Rig state is never the sole source of truth. Any optional engine checkpoint stores:

```text
engine_name
engine_version
state_schema_version
serialized_state
canonical_journal_cursor
```

Vestrace must be able to recover or fail safely when an engine checkpoint is incompatible after an upgrade.

## Conformance requirements

The native and Rig provider adapters share a conformance suite covering:

- completion;
- streaming;
- structured output;
- tool-call parsing;
- usage normalization;
- timeout and cancellation;
- rate-limit mapping;
- context overflow;
- malformed responses;
- secret redaction;
- provider unavailability.

CI must include a Rig-free build and test path:

```bash
cargo test --workspace --no-default-features --features provider-openai-compatible
```

Rig integration is tested separately:

```bash
cargo test --workspace --features provider-rig
```

## Consequences

### Positive

- Vestrace gains mature provider abstractions without surrendering architecture ownership.
- A production path remains available when Rig is disabled or incompatible.
- Rig upgrades are localized and reviewable.
- The model loop remains replaceable.

### Costs

- Vestrace must maintain translation code and conformance tests.
- Some provider-specific functionality may not be available through the native adapter.
- The spike delays the final model-loop choice, but prevents premature runtime coupling.

## Non-goals

This decision does not approve Rig conversation memory, vector stores, workflow execution, durable scheduling or direct tool execution as Vestrace infrastructure.
