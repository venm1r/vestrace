# Vestrace Current Implementation Snapshot

**Snapshot source:** `main@729d456f70f4de93c97d05cce795c09025c62f24`  
**Snapshot date:** 2026-08-07  
**Documentation baseline date:** 2026-08-10

> This document describes what is currently implemented/wired in the source snapshot. It is not the target Architecture Contract v0.2.

## 1. Target architecture

The normative target is defined in:

- [`docs/specs/vestrace-architecture-contract-v0.2.md`](specs/vestrace-architecture-contract-v0.2.md)
- [`docs/specs/README.md`](specs/README.md)

Canonical product definition:

> **Vestrace is a memory-first platform for persistent cognition shared across agents and executions.**

## 2. Repository/runtime shape

Current source is a Rust Edition 2024 workspace with:

```text
crates/vestrace-domain
crates/vestrace-application
crates/vestrace-infrastructure
crates/vestrace-http
crates/vestrace-cli
crates/vestrace-mcp
crates/vestrace-rig-spike
```

plus root integration tests, SQLx migrations, Docker/Compose and documentation.

The intended dependency direction is inward:

```text
CLI / HTTP / Infrastructure
        ↓
Application
        ↓
Domain
```

`vestrace-rig-spike` is experimental and not part of the supported runtime contract.

## 3. Currently wired execution foundation

Current runtime includes:

- event-sourced Run command execution;
- deterministic replay;
- optimistic concurrency;
- run checkpoints;
- checkpoint validation/restoration;
- run projection rebuild;
- append-only RunEvent history;
- PostgreSQL-backed projection/event repositories.

The current implementation is a foundation for the target single execution/state boundary, but it does not yet implement the complete v0.2+ target contracts.

## 4. Currently wired memory foundation

Implemented/wired paths include:

- immutable-ish Memory revision lifecycle through application services;
- memory creation with first revision;
- provenance source persistence;
- memory revision with optimistic concurrency;
- relation linking;
- event recording;
- idempotency/outbox integration;
- memory/event read paths;
- active-source database invariant;
- authorized hard purge path using a deterministic/test authorization adapter.

Target v0.2 expands this foundation with explicit Claim, richer Evidence/Derivation, temporal/reconciliation and governance semantics.

## 5. Retrieval currently wired

Current runtime includes:

- retrieval request normalization;
- PostgreSQL full-text search channel;
- RRF fusion;
- deterministic reranking;
- token-bounded ContextPack builder;
- representation ladder `Full → Summary → Atomic → Reference`;
- retrieval journaling;
- degraded behavior when a retrieval channel fails.

Ports exist for vector/exact/structured channels, but they are not all wired in the current runtime.

## 6. Security/governance currently wired

Current code contains:

- capability domain types;
- sensitivity levels;
- approval domain types;
- `PolicyEngine` abstraction and several implementations;
- workspace-scoped policy helper;
- redaction service;
- audit repository;
- HTTP auth middleware/local trusted mode.

Important current limitation: complete target capability governance is not yet enforced uniformly across all runtime entry points, and production approval authorization is not fully wired.

## 7. Current infrastructure

PostgreSQL infrastructure includes repositories/adapters for run events/projections, recovery, memory, provenance, relations, jobs, outbox, idempotency, retrieval journal, audit and provider access.

Current job worker can lease/complete jobs with `FOR UPDATE SKIP LOCKED`, but typed production handlers are not fully registered.

## 8. Current API/CLI surfaces

Currently exposed HTTP foundation includes health, runs, events, memories and retrieval paths. Several future/product surfaces intentionally return `501 Not Implemented` rather than pretending to work.

Currently supported CLI process modes include server, migration, worker and MCP startup.

Historical/current docs have described `doctor`/`rebuild` as unavailable in the wired snapshot; the target v0.2 documentation defines their future semantic boundary but not current availability.

## 9. MCP

Current MCP server exposes memory-oriented read/search tools. The target architecture may expand governed agent-facing surfaces only through the common application/capability boundary.

## 10. Providers

OpenAI-compatible text generation/embedding adapters exist, but current docs note that they are not consumed by every target runtime path.

## 11. Target features not implied by current types

A domain type or placeholder endpoint is not considered implemented just because it exists in the repository.

The current snapshot does **not** establish complete target support for:

- Claim-based persistent cognition model;
- full temporal/as-of semantics;
- complete mutation/reconciliation engine;
- ContextPack 2.0 across all channels;
- execution feedback/learning lifecycle;
- full capability attenuation/delegation;
- cross-workspace grant+mount federation;
- findings-first Health/Repair engine;
- governed ExternalEffectIntent/receipt/reconciliation;
- Incident/Recovery/Revalidation trust model;
- target Crypto/Data Governance contract;
- Qualification/Conformance profiles.

## 12. Documentation rule

When current implementation and target v0.2 architecture differ:

1. this file and implementation-status/acceptance docs describe current wiring;
2. `docs/specs/vestrace-*.md` describes normative target architecture;
3. accepted `docs/adr/*` resolves target architecture decisions;
4. no target feature may be claimed implemented without a wired runtime path and appropriate acceptance/conformance evidence.
