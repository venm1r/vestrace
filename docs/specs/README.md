# Vestrace v0.2 Normative Documentation Index

This directory contains the target architecture documentation for the `docs/architecture-v0.2` branch.

> These specifications describe the target architecture. They do not by themselves assert current implementation availability.

## Normative hierarchy

1. [Architecture Contract v0.2](vestrace-architecture-contract-v0.2.md) — top-level product and architecture contract.
2. [Domain Model v0.2](vestrace-domain-model-v0.2.md) — entities, authority tiers and aggregate boundaries.
3. [Normative Invariants Catalog](vestrace-normative-invariants-v0.2.md) — stable MUST/SHOULD requirement IDs.
4. [Trust & Authority Model](vestrace-trust-authority-model-v0.2.md) — capabilities, delegation, risk, approvals, workspaces and trust.
5. [Data & Temporal Model](vestrace-data-temporal-model-v0.2.md) — revisions, validity, occurrence/recording time and historical queries.
6. [Execution & External Effects Contract](vestrace-execution-external-effects-contract-v0.2.md) — durable execution, side effects, idempotency and reconciliation.
7. [Health / Repair / Incident Contract](vestrace-health-repair-incident-contract-v0.2.md) — findings, repair, recovery and revalidation. Read together with [ADR-0009](../adr/0009-finding-disposition-is-not-integrity-state.md), which clarifies that `SUPPRESSED` / `ACCEPTED_RISK` are disposition overlays, not finding integrity states.
8. [Crypto & Data Governance Contract](vestrace-crypto-data-governance-contract-v0.2.md) — classification, secrets, crypto, retention, deletion and export.
9. [Qualification / Conformance Specification](vestrace-qualification-conformance-spec-v0.2.md) — profiles, hard gates, fault scenarios and qualification bundles.
10. [Version Roadmap v0.2 → v1.0](vestrace-version-roadmap-v0.2-to-v1.0.md) — release capability/qualification sequence.
11. [`docs/adr/`](../adr/) — accepted architecture decisions.

## Legacy files in this directory

The pre-v0.2 `r1-*` specifications are historical implementation/design artifacts, not part of the normative v0.2 set. See [LEGACY.md](LEGACY.md).

## Conflict resolution

If an older specification conflicts with this normative set:

1. the Architecture Contract has highest priority;
2. an Accepted newer ADR may explicitly supersede or clarify an older decision;
3. specialized v0.2 specs refine but may not silently weaken the Architecture Contract or a newer Accepted ADR;
4. legacy/current implementation documentation must be labeled as implementation status rather than target architecture.

Older specs are retained as historical design artifacts unless explicitly updated or marked superseded during the consistency pass.

## Documentation phase rule

No implementation code or migrations are changed as part of this documentation phase. After the documentation consistency pass is complete, a separate gap analysis will compare current `main` against this target contract.
