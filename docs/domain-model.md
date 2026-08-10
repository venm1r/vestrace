# Vestrace Domain Model

## Documentation status

This file is the domain-model entry point for the `docs/architecture-v0.2` documentation branch.

The previous `docs/domain-model.md` on `main` is an implementation-oriented reference describing currently present Rust domain modules and whether they are wired/type-only. It remains valid as a snapshot of `main@729d456f70f4de93c97d05cce795c09025c62f24`, but it is **not** the normative target domain model.

## Normative target model

Use:

- [`specs/vestrace-domain-model-v0.2.md`](specs/vestrace-domain-model-v0.2.md) — canonical target entities, authority tiers, aggregate ownership and forbidden conflations;
- [`specs/vestrace-data-temporal-model-v0.2.md`](specs/vestrace-data-temporal-model-v0.2.md) — temporal/revision semantics;
- [`specs/vestrace-trust-authority-model-v0.2.md`](specs/vestrace-trust-authority-model-v0.2.md) — identity, capabilities, workspace/federation and trust;
- [`specs/vestrace-normative-invariants-v0.2.md`](specs/vestrace-normative-invariants-v0.2.md) — stable domain requirements.

## Core target distinctions

The v0.2 target model explicitly keeps these concepts separate:

```text
Memory != Evidence
Memory != Claim
Claim != Truth
Role != Capability
Identity != Authority
Scope != Permission
Hash != Permission
Receipt != Confirmed Outcome
Repair Success != Finding Resolution
Health != Trust
Recovery != Revalidation
Encryption != Governance
Mount != Local Memory
Compensation != Rollback
Projection != Source of Truth
Milestone Label != Qualified Profile
```

## Target authority tiers

```text
Tier A — source evidence
  Event / ArtifactRevision / external receipts / human feedback / imported refs

Tier B — canonical cognition
  Memory / MemoryRevision / Claim / assessments / provenance / conflicts

Tier C — canonical operational and governance state
  identity / capability / policy / execution / effects / health / incident / governance

Tier D — derived state
  embeddings / search docs / context caches / aggregate snapshots / projections
```

Tier D must be rebuildable and must not silently overwrite a higher-authority tier.

## Current implementation reference

See [`current-implementation.md`](current-implementation.md) for a concise snapshot of what is actually wired today.

For the detailed pre-v0.2 implementation-level inventory of Rust types and wired/type-only modules, consult `docs/domain-model.md` on the `main` branch at commit `729d456f70f4de93c97d05cce795c09025c62f24`.

## Planning reference

The source-based gap analysis and future domain transition contracts are already documented in:

- [`gap-analysis-v0.2.md`](gap-analysis-v0.2.md)
- [`plans/README.md`](plans/README.md)
- [`plans/v0.2-to-v1.0-pr-specification-index.md`](plans/v0.2-to-v1.0-pr-specification-index.md)

## Documentation rule

A target entity in the v0.2 Domain Model does not count as implemented until current implementation evidence identifies its runtime path, persistence contract, migration state where needed, and conformance evidence. A planning document alone is not implementation evidence.
