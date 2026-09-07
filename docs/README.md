# Vestrace Documentation

This index separates product requirements, implementation observations and work that is only proposed. File names containing `current`, a checked box, or an old successful test record do not establish the status of a later commit.

## Choose a starting point

| Question | Start here | What the document establishes |
| --- | --- | --- |
| What is Vestrace designed to be? | [Architecture](architecture.md) and [normative index](specs/README.md) | Memory-first product boundary and required invariants |
| What did the earlier inspected implementation contain? | [Implementation snapshot](current-implementation.md) | Its explicitly pinned source snapshot, not a fresh audit of HEAD |
| How is v1.0 delivery organized? | [Planning index](plans/README.md) | Distinct 36-PR and P01–P12 programs; their own approval rules |
| How will memory become editable and importable? | [Memory Workspace](implementation/memory-workspace/README.md) | Proposed API/Console/import/sync/export extension on `6f610253` |
| Where does implementation start? | [MW-00 preflight](implementation/memory-workspace/plans/00-preflight.md) | Fresh source/scope/dependency checks before code changes |
| How are the new documents checked? | [Documentation verification](implementation/memory-workspace/verification/report.md) | Schema/examples/links, not product qualification |

## Memory Workspace integration — 2026-09-07

The implementation package has one canonical documentation location: `docs/implementation/memory-workspace/`. The single-file reading edition and any ZIP are generated distributions, not additional authorities.

The package covers a single cycle: import a bounded source → read and find memories → inspect provenance → make an editorial correction → obtain context → synchronize without losing the correction → export permitted knowledge.

**Status boundaries:**

- Documentation placement and cross-references are integrated separately from product implementation.
- MW-D01–MW-D12 remain proposed design decisions; no Accepted ADR or frozen spec is modified by their registration.
- MW-00–MW-07 are a feature program, not additions to P01–P12 or evidence that v1.0 is qualified.
- The source-pinned [baseline](implementation/memory-workspace/00-baseline.md) distinguishes implemented P04 work through 14D from the next proposed boundaries. Recheck it before execution.
- Proposed API routes, DTOs, migrations and test targets do not become available when these documentation files are merged.

Read the [integration contract](implementation/memory-workspace/12-integration.md) for exact precedence, overlap and feature-specific gates.

## Preserved references

[Architecture guide](architecture.md), [domain reference](domain-model.md), [database schema](database-schema.md), [security/RLS](security-and-rls.md), and [getting started](getting-started.md) retain their documented scope and baseline. The root README contains historical foundation setup assumptions; this documentation-only integration does not qualify those commands on a fresh installation.

The [v0.2 documentation status](documentation-status-v0.2.md), historical gap deltas, accepted evidence, frozen release plans and protocol locks are preserved. New source observations belong in an explicitly pinned delta; they must not rewrite a historical observation as though it originally described the new code.
