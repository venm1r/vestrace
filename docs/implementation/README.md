# Implementation Extensions

Feature designs are registered here without changing the [normative hierarchy](../specs/README.md) or the [release planning programs](../plans/README.md).

| Package | Baseline | Scope | Documentation / implementation status |
| --- | --- | --- | --- |
| [Memory Workspace](memory-workspace/README.md) | `6f6102536e9a535b7086db14573bf45fe750ad71` | Memory/Context API, Console editing, bounded import/sync/export | Integrated proposed design; no runtime implementation or release qualification claimed |

A package owns its specifications, proposed schemas/examples, traceability and implementation plans. It references shared contracts instead of copying them. Its validator checks documentation only.

Before implementation, read its preflight and establish a fresh source baseline, exact writable paths, protected authorities, dependency acceptance and a release-program decision where needed. The presence of the directory is not scope authorization.

For Memory Workspace, start with [integration and precedence](memory-workspace/12-integration.md) and [MW-00](memory-workspace/plans/00-preflight.md). The package does not widen P04, reserve migration numbers or weaken named qualification profiles.

[All documentation](../README.md).
