# Documentation

Use the reading path that matches your task. Guides explain the repository; [normative contracts and Accepted ADRs](specs/README.md) define its requirements.

## Reading paths

**Users and integrators:** [Product overview](product/overview.md) → [status](status.md) → [setup](getting-started.md) → [Memory API walkthrough](guides/memory-api-exercise.md) → [HTTP](reference/http.md) or [MCP](reference/mcp.md).

**Developers:** [Architecture](architecture.md) → [domain model](domain-model.md) → [transactions](design/transactions.md) → [development](development/README.md) → [testing](development/testing.md) → [implementation programs](plans/README.md).

**Operators:** [Deployment](operations/deployment.md) → [runbook](operations/runbook.md) → [backup and recovery](operations/backup-restore.md) → [troubleshooting](operations/troubleshooting.md).

**Memory Workspace implementers:** [Product scope](product/memory-workspace.md) → [implementation package](implementation/memory-workspace/README.md) → [MW-00](implementation/memory-workspace/plans/00-preflight.md). Read the shared contracts before implementing an isolated task.

## Map

| Section | Pages |
| --- | --- |
| Product | [Overview](product/overview.md), [scenarios](product/scenarios.md), [Memory Workspace](product/memory-workspace.md) |
| Status | [Capabilities](status.md), [open gaps](status/open-gaps.md) |
| Tutorials | [Getting started](getting-started.md), [Memory API](guides/memory-api-exercise.md), [Console](guides/console.md) |
| Architecture | [Overview](architecture.md), [domain model](domain-model.md), [memory/time](design/memory-time.md), [retrieval](design/retrieval-context.md), [transactions](design/transactions.md), [execution](design/execution.md), [materials](design/materials.md), [learning/health](design/learning-health.md) |
| Reference | [HTTP](reference/http.md), [route catalog](reference/route-catalog.md), [memory](reference/memory.md), [retrieval](reference/retrieval.md), [MCP](reference/mcp.md), [CLI](reference/cli.md), [configuration](reference/configuration.md), [glossary](reference/glossary.md), [database](database-schema.md), [security](security-and-rls.md) |
| Operations | [Deployment](operations/deployment.md), [runbook](operations/runbook.md), [restore](operations/backup-restore.md), [troubleshooting](operations/troubleshooting.md) |
| Development | [Guide](development/README.md), [testing](development/testing.md), [agent workflow](development/agent-workflow.md), [releases](development/release.md) |
| Requirements | [Normative index](specs/README.md), [English contracts](specs/en/README.md), [Accepted ADRs](adr/README.md) |
| Implementation and planning | [Programs](plans/README.md), [packages](implementation/README.md), [roadmap](roadmap/README.md), [milestones](roadmap/milestones.md), [next actions](roadmap/next-actions.md) |
| Evaluation | [Method and synthetic corpus](evaluation/README.md) |
| Maintenance | [Rules](maintenance/README.md), [sources](maintenance/sources.md), [change map](maintenance/migration-map.md), [validation](maintenance/english-validation-report.md) |
| History | [Preserved baselines and evidence](history/README.md) |

## Proposed designs

[Temporal conflicts](design/temporal-conflicts.md), [consolidation](design/consolidation.md), [context inspection](design/context-observability.md), and [external-agent integration](design/integration-boundaries.md) describe proposed extensions. Their dependencies and acceptance gates still apply.

## Read status labels literally

A **source observation** is not a successful runtime test. **Recorded evidence** reports a result on its identified snapshot; it is not a new independent execution. **Normative** means required, not implemented. **Proposed** means the design still requires acceptance. **Not audited** does not mean absent.

[Current status](status.md) is the implementation entry point. Historical records keep their own baselines and verdicts. [Maintenance](maintenance/README.md) explains translation coverage, generated views, and the preserved originals.
