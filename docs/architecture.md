# Vestrace Architecture

## Documentation status

This file is the architecture entry point for the frozen v0.2 documentation baseline integrated into `main`, plus explicitly accepted post-v0.2 architecture extensions.

Vestrace has deliberately separate documentation layers:

1. **Target architecture (normative)** — what Vestrace is designed to become and the invariants future implementation must satisfy.
2. **Current implementation snapshot** — what was actually wired in the inspected implementation baseline `729d456f70f4de93c97d05cce795c09025c62f24`.
3. **Transition planning** — the evidence/migration/PR contracts for moving from that inspected baseline toward the target.
4. **Post-v0.2 architecture extensions** — accepted system-level decisions that preserve the frozen v0.2 laws but are not silently inserted into the existing implementation roadmap.

Do not infer current runtime availability from target architecture or planning documents.

## Canonical product definition

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

## Normative architecture

Start with:

- [`specs/vestrace-architecture-contract-v0.2.md`](specs/vestrace-architecture-contract-v0.2.md) — frozen top-level Architecture Contract;
- [`specs/README.md`](specs/README.md) — normative documentation hierarchy and accepted extensions;
- [`adr/README.md`](adr/README.md) — accepted architecture decisions.

The frozen v0.2 baseline covers twelve completed architecture blocks:

```text
[✓] 1. Persistent Cognition Core
[✓] 2. Temporal & Concurrency
[✓] 3. Mutation & Reconciliation
[✓] 4. Retrieval / ContextPack 2.0
[✓] 5. Execution Feedback & Learning
[✓] 6. Capability Governance
[✓] 7. Identity / Workspace / Federation
[✓] 8. Health / Integrity / Repair
[✓] 9. External Effects
[✓] 10. Incident / Recovery / Revalidation
[✓] 11. Crypto / Data Governance
[✓] 12. Qualification / Conformance
```

## Post-v0.2 system architecture: Brain–Face–Organ

ADR-0011 accepts a system-level decomposition for the future persistent autonomous-agent product.

```text
                         USER
                          │
                          ▼
                       FACE
              Desktop / CLI / Host Broker
                          │
                  System Interconnect
                          │
                          ▼
                       BRAIN
             Vestrace + Prime-like Runtime
                          │
                  System Interconnect
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
       Coding          Browser         Compute
       Organ            Organ           Organ
```

The formal meanings are:

- **Brain** — persistent agent identity and cognition. Vestrace owns durable cognition, authority, `AgentRun`, effects, trust, recovery and qualification state; the Prime-like runtime owns active reasoning, planning, working context, model routing, goals and subagent coordination without becoming a second durable state engine.
- **Face** — replaceable user and host boundary. A thin Desktop/CLI/mobile/web client may be paired with a local Host Broker for explicitly permitted host files, processes, applications, shell, browser integrations, devices and settings.
- **Organ** — replaceable execution capability such as a Docker container, VM, sandbox, browser worker, GPU worker or remote execution endpoint. An Organ is logical and need not map one-to-one to a container.
- **System Interconnect** — authenticated transport for commands, events, observations, capabilities, approvals, leases, artifacts, receipts, heartbeats and telemetry. It owns no canonical cognition or authority.
- **Models** — replaceable cognitive compute resources, not the persistent identity of the agent.

Key additional laws:

1. Face and Organ endpoints may further restrict Brain authority but never amplify it.
2. Effective host authority is the intersection of Brain capability/policy, Face-local policy and host OS rights.
3. Effective Organ authority is the intersection of Brain capability/policy, Organ-local policy and sandbox/OS rights.
4. Organ or Host output is observation/evidence before it may become durable cognition.
5. Face, Organ, model and worker replacement must not replace agent identity.
6. Reconnect/restart/recreate must not silently imply effect success or restored trust.

See:

- [`specs/vestrace-brain-face-organ-system-model.md`](specs/vestrace-brain-face-organ-system-model.md)
- [`adr/0011-brain-face-organ-system-decomposition.md`](adr/0011-brain-face-organ-system-decomposition.md)

This extension does **not** claim that the Brain runtime, Face/Host Broker, or Organ layer is implemented, and it does not silently expand the frozen 36-PR v0.2→v1.0 roadmap.

## Specialized frozen v0.2 normative documents

- [`specs/vestrace-domain-model-v0.2.md`](specs/vestrace-domain-model-v0.2.md)
- [`specs/vestrace-normative-invariants-v0.2.md`](specs/vestrace-normative-invariants-v0.2.md)
- [`specs/vestrace-trust-authority-model-v0.2.md`](specs/vestrace-trust-authority-model-v0.2.md)
- [`specs/vestrace-data-temporal-model-v0.2.md`](specs/vestrace-data-temporal-model-v0.2.md)
- [`specs/vestrace-execution-external-effects-contract-v0.2.md`](specs/vestrace-execution-external-effects-contract-v0.2.md)
- [`specs/vestrace-health-repair-incident-contract-v0.2.md`](specs/vestrace-health-repair-incident-contract-v0.2.md)
- [`specs/vestrace-crypto-data-governance-contract-v0.2.md`](specs/vestrace-crypto-data-governance-contract-v0.2.md)
- [`specs/vestrace-qualification-conformance-spec-v0.2.md`](specs/vestrace-qualification-conformance-spec-v0.2.md)
- [`specs/vestrace-version-roadmap-v0.2-to-v1.0.md`](specs/vestrace-version-roadmap-v0.2-to-v1.0.md)

ADR-0010 clarifies the boundary between roadmap milestone labels and formal qualification-profile claims.

## Current implementation

See [`current-implementation.md`](current-implementation.md) for the inspected wired snapshot.

The inspected source foundation is a Rust Edition 2024 modular workspace with domain/application/infrastructure/HTTP/CLI/MCP layers and a PostgreSQL-backed run/memory/retrieval foundation. The presence of future-facing domain types or placeholder endpoints does not imply target feature completion.

## Transition planning

See [`plans/README.md`](plans/README.md) and [`plans/v0.2-to-v1.0-pr-specification-index.md`](plans/v0.2-to-v1.0-pr-specification-index.md).

The frozen transition package includes all 36 planned future implementation PRs, dependency ordering, migration/backfill contracts, conformance cases, release evidence gates and review rules.

The Brain–Face–Organ extension requires a separate future transition plan before implementation. It is not silently inserted into those 36 PRs.

## Architectural laws

The highest-level invariants are:

1. authoritative/canonical state is distinct from derived projections;
2. derived state cannot automatically rewrite higher-authority state;
3. Vestrace has one execution/state boundary, not competing runtimes;
4. history is corrected by new facts/revisions/compensation, not silently rewritten;
5. capabilities plus policy determine runtime authority; roles are templates;
6. ambiguity is first-class (`UNKNOWN` is not silently failure/success/trust);
7. repair only auto-modifies state deterministically reconstructible from a more authoritative layer;
8. recovery does not restore trust without revalidation evidence;
9. secrets are not ordinary memory;
10. v1.0 is defined by the `TRUSTED` qualification contract, not feature count;
11. milestone labels do not imply named profile qualification without evidence closure;
12. persistent agent identity and cognition live in the Brain, not in a Face, Organ, model, or transient worker;
13. connected endpoints may only preserve or reduce delegated authority, never amplify it;
14. observations from execution are not automatically durable beliefs.

## Baseline state

The v0.2 documentation consistency pass, source-based gap analysis and 36-PR transition planning package are complete and integrated into `main`.

The Brain–Face–Organ model is an accepted post-v0.2 target architecture extension pending its own implementation transition plan.

Going forward:

- target documentation does not itself change runtime behavior;
- implementation work belongs on dedicated implementation branches;
- architecture changes after a frozen baseline require deliberate spec/ADR amendment;
- post-baseline extensions must not silently alter frozen qualification or roadmap claims;
- material changes to implementation code require a gap delta before the transition plans are treated as current without review.
