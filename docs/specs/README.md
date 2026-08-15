# Vestrace Normative Documentation Index

This directory contains the frozen v0.2 target architecture plus explicitly accepted post-v0.2 architecture extensions.

> These specifications describe target architecture. They do not by themselves assert current implementation availability.

## Frozen v0.2 normative hierarchy

1. [Architecture Contract v0.2](vestrace-architecture-contract-v0.2.md) — top-level product and architecture contract.
2. [Domain Model v0.2](vestrace-domain-model-v0.2.md) — entities, authority tiers and aggregate boundaries.
3. [Normative Invariants Catalog](vestrace-normative-invariants-v0.2.md) — stable MUST/SHOULD requirement IDs.
4. [Trust & Authority Model](vestrace-trust-authority-model-v0.2.md) — capabilities, delegation, risk, approvals, workspaces and trust.
5. [Data & Temporal Model](vestrace-data-temporal-model-v0.2.md) — revisions, validity, occurrence/recording time and historical queries.
6. [Execution & External Effects Contract](vestrace-execution-external-effects-contract-v0.2.md) — durable execution, side effects, idempotency and reconciliation.
7. [Health / Repair / Incident Contract](vestrace-health-repair-incident-contract-v0.2.md) — findings, repair, recovery and revalidation. Read together with [ADR-0009](../adr/0009-finding-disposition-is-not-integrity-state.md), which clarifies that `SUPPRESSED` / `ACCEPTED_RISK` are disposition overlays, not finding integrity states.
8. [Crypto & Data Governance Contract](vestrace-crypto-data-governance-contract-v0.2.md) — classification, secrets, crypto, retention, deletion and export.
9. [Qualification / Conformance Specification](vestrace-qualification-conformance-spec-v0.2.md) — profiles, hard gates, fault scenarios and qualification bundles. Read together with [ADR-0010](../adr/0010-qualification-profile-scope-follows-evidence-closure.md), which clarifies milestone/profile evidence scope.
10. [Version Roadmap v0.2 → v1.0](vestrace-version-roadmap-v0.2-to-v1.0.md) — release capability/qualification sequence.
11. [`docs/adr/`](../adr/) — accepted architecture decisions.

## Accepted post-v0.2 extensions

- [Brain–Face–Organ System Model](vestrace-brain-face-organ-system-model.md) — system-level decomposition for the future autonomous-agent product: `Vestrace + Prime-like Runtime` as Brain, Desktop/CLI + Host Broker as Face, and replaceable execution endpoints as Organs.
- [ADR-0011](../adr/0011-brain-face-organ-system-decomposition.md) — accepts that decomposition while preserving the v0.2 single-authority, capability, external-effect, and trust laws.

Post-v0.2 extensions MUST NOT be interpreted as retroactively changing v0.2 implementation availability, qualification claims, or the frozen 36-PR v0.2→v1.0 transition package unless a later transition document explicitly amends that package.

## Legacy files in this directory

The pre-v0.2 `r1-*` specifications are historical implementation/design artifacts, not part of the normative v0.2 set. See [LEGACY.md](LEGACY.md).

## Conflict resolution

For the frozen v0.2 baseline:

1. the Architecture Contract v0.2 has highest priority;
2. an Accepted newer ADR may explicitly supersede or clarify an older decision;
3. specialized v0.2 specs refine but may not silently weaken the Architecture Contract or a newer Accepted ADR;
4. legacy/current implementation documentation must be labeled as implementation status rather than target architecture.

For post-v0.2 extensions:

1. they MUST preserve frozen v0.2 laws unless a new ADR explicitly supersedes a specific decision;
2. they MUST identify themselves as post-baseline extensions;
3. they MUST NOT silently rewrite implementation or qualification status.

Older specs are retained as historical design artifacts unless explicitly updated or marked superseded.

## Documentation state

The v0.2 normative architecture consistency pass is complete. A source-based gap analysis against implementation commit `729d456f70f4de93c97d05cce795c09025c62f24` and a complete 36-PR transition package are integrated into `main` alongside the frozen v0.2 set.

The Brain–Face–Organ model is a later accepted documentation extension. It defines system topology and authority boundaries only; it does not claim the Brain runtime, Face/Host Broker, or Organ layer is implemented.

See:

- [`../documentation-status-v0.2.md`](../documentation-status-v0.2.md)
- [`../documentation-post-merge-audit-v0.2.md`](../documentation-post-merge-audit-v0.2.md)
- [`../gap-analysis-v0.2.md`](../gap-analysis-v0.2.md)
- [`../plans/README.md`](../plans/README.md)
- [`../plans/v0.2-to-v1.0-pr-specification-index.md`](../plans/v0.2-to-v1.0-pr-specification-index.md)

The normative documents define target requirements only. Runtime code, migrations, and qualification claims require separate implementation and executable evidence.
