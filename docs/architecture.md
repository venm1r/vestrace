# Vestrace Architecture

## Documentation status

This file is the architecture entry point for the `docs/architecture-v0.2` branch.

Vestrace has deliberately separate documentation layers:

1. **Target architecture (normative)** — what Vestrace is designed to become and the invariants future implementation must satisfy.
2. **Current implementation snapshot** — what is actually wired in `main@729d456f70f4de93c97d05cce795c09025c62f24`.
3. **Transition planning** — the evidence/migration/PR contracts for moving from the inspected baseline toward the target.

Do not infer current runtime availability from target architecture or planning documents.

## Canonical product definition

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

> **Memory Engine is the substrate. Persistent Cognition is the capability.**

## Normative architecture

Start with:

- [`specs/vestrace-architecture-contract-v0.2.md`](specs/vestrace-architecture-contract-v0.2.md) — top-level Architecture Contract;
- [`specs/README.md`](specs/README.md) — normative documentation hierarchy;
- [`adr/README.md`](adr/README.md) — accepted architecture decisions.

The target baseline covers twelve completed architecture blocks:

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

## Specialized normative documents

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

See [`current-implementation.md`](current-implementation.md) for the current wired snapshot.

The source snapshot remains a Rust Edition 2024 modular workspace with domain/application/infrastructure/HTTP/CLI/MCP layers and a PostgreSQL-backed run/memory/retrieval foundation. The presence of future-facing domain types or placeholder endpoints does not imply target feature completion.

## Transition planning

See [`plans/README.md`](plans/README.md) and [`plans/v0.2-to-v1.0-pr-specification-index.md`](plans/v0.2-to-v1.0-pr-specification-index.md).

The transition package includes all 36 planned future implementation PRs, dependency ordering, migration/backfill contracts, conformance cases, release evidence gates and review rules.

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
11. milestone labels do not imply named profile qualification without evidence closure.

## Documentation branch state

The documentation consistency pass, source-based gap analysis and 36-PR transition planning package are complete for the inspected `main` baseline.

This branch remains documentation-only:

- no Rust implementation changes;
- no migrations added or edited;
- no runtime/API behavior changes;
- architecture changes after freeze require deliberate spec/ADR amendment;
- material movement of `main` requires a gap delta before plans are treated as current.
