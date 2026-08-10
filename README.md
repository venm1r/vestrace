# Vestrace

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

This branch (`docs/architecture-v0.2`) is a **documentation-only architecture branch**. It defines the target architecture while keeping the current implementation status separate. No Rust code or migrations are changed as part of this documentation phase.

## Documentation layers

### Target architecture — normative

Start here:

- [Architecture Contract v0.2](docs/specs/vestrace-architecture-contract-v0.2.md)
- [Normative Documentation Index](docs/specs/README.md)
- [Accepted ADRs](docs/adr/README.md)
- [Version Roadmap v0.2 → v1.0](docs/specs/vestrace-version-roadmap-v0.2-to-v1.0.md)

### Current implementation

- [Current Implementation Snapshot](docs/current-implementation.md)
- [Database Schema & Migrations](docs/database-schema.md)
- [Security & RLS](docs/security-and-rls.md)
- [Getting Started](docs/getting-started.md)

### Transition package

- [36-PR Planning Index](docs/plans/v0.2-to-v1.0-pr-specification-index.md)
- [Planning Directory Status](docs/plans/README.md)
- [Documentation Status](docs/documentation-status-v0.2.md)

The target docs describe what Vestrace **must become**. The implementation snapshot describes what is actually wired in `main@729d456f70f4de93c97d05cce795c09025c62f24`.

## Target architecture

The v0.2 baseline completes twelve architecture blocks:

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

Core architectural laws include:

- canonical state is distinct from derived projections;
- history is corrected, not silently rewritten;
- Vestrace has one durable execution/state boundary;
- runtime authority comes from capabilities + policy, not role names;
- subagent authority can only be attenuated;
- `UNKNOWN` is a first-class state for ambiguous outcomes;
- deterministic repair only reconstructs downward from a more authoritative layer;
- recovery does not restore trust without revalidation;
- secrets are not ordinary memory;
- v1.0 is defined by the `TRUSTED` qualification profile, not feature count;
- milestone labels and qualification-profile claims are distinct and evidence-scoped (ADR-0010).

## Current implementation snapshot

The current source foundation includes:

- Rust Edition 2024 workspace;
- PostgreSQL + SQLx migrations and workspace-scoped RLS context;
- event-sourced Run command execution with optimistic concurrency;
- deterministic Run replay, checkpoints and projection rebuild;
- memory creation/revision/provenance/relations with idempotency and outbox;
- append-only event enforcement and active-source invariant;
- PostgreSQL FTS retrieval, RRF fusion, deterministic reranking and token-bounded ContextPack building;
- retrieval journaling and degraded channel behavior;
- capability/sensitivity/approval domain types, policy abstractions, redaction and audit infrastructure;
- worker/job leasing foundation;
- MCP memory search/read tools;
- OpenAI-compatible provider adapters.

A type, placeholder endpoint or adapter existing in the repository does **not** mean the corresponding target architecture capability is fully implemented.

See [Current Implementation Snapshot](docs/current-implementation.md) for the boundary and known gaps.

## Repository shape

```text
crates/
  vestrace-domain/
  vestrace-application/
  vestrace-infrastructure/
  vestrace-http/
  vestrace-cli/
  vestrace-mcp/
  vestrace-rig-spike/       # experimental

migrations/
tests/
docs/
apps/
```

The intended dependency direction is inward:

```text
interfaces / infrastructure
          ↓
      application
          ↓
        domain
```

## Local development

Prerequisites and current commands are documented in [Getting Started](docs/getting-started.md).

Typical Docker flow:

```bash
docker compose -p vestrace up --build -d
./scripts/foundation-smoke.sh
bash ./scripts/foundation-run-smoke.sh
./scripts/foundation-runtime-rls.sh
docker compose -p vestrace down --remove-orphans
```

## Documentation branch state

The architecture baseline, implementation gap analysis, 36-PR transition package, migration/conformance matrices and consistency audit are complete for the inspected baseline.

Branch rules remain:

- no implementation work belongs in this branch;
- no migrations are added or edited;
- no runtime/API behavior is changed;
- no target feature is claimed implemented merely because it is specified;
- if `main` moves materially, re-run a gap delta before using the plans unchanged;
- architecture changes after freeze require deliberate spec/ADR amendment.
