# Vestrace

**Governed long-term memory and context shared across agents and executions.**

Vestrace keeps knowledge independent of a particular model or agent session. It preserves sources, revision history, and the authority to use or change that knowledge. Retrieval and context are derived views; execution, governance, and recovery support the memory lifecycle rather than replace it.

> **Development status:** This repository contains Rust services, PostgreSQL storage, HTTP and MCP interfaces, and a React/TypeScript Console. Source code and documentation are not a production-readiness or v1.0 qualification claim. Check [implementation status](docs/status.md) before choosing an integration.

## Start here

| Goal | Read |
| --- | --- |
| Understand the product | [Overview](docs/product/overview.md) and [scenarios](docs/product/scenarios.md) |
| Check what exists and what is proposed | [Implementation status](docs/status.md) |
| Prepare a local environment | [Getting started](docs/getting-started.md) |
| Exercise the current API | [Memory walkthrough](docs/guides/memory-api-exercise.md) |
| Work on the codebase | [Architecture](docs/architecture.md) and [development guide](docs/development/README.md) |
| Operate an installation | [Deployment](docs/operations/deployment.md) and [runbook](docs/operations/runbook.md) |
| Find specifications and plans | [Documentation index](docs/README.md) |

## Before running locally

Docker Compose does **not** provision the bootstrap credential. Prepare the read-only secret store, persistent vault volumes, and appropriate database roles first. An empty external volume is not a configured secret store. This checkout does not provide a verified one-command clean-machine setup.

HTTP clients authenticate with a Bearer token. The server resolves principal and workspace from that token; client-supplied identity headers do not select another identity. See [Console authentication](docs/guides/console.md) for the local UI flow.

## Current work and proposed extensions

The [roadmap](docs/roadmap/README.md) groups 34 initiatives into product priorities P0–P4 and maps them to the existing P01–P12 and MW-00–MW-07 programs. [Milestones](docs/roadmap/milestones.md) describe observable outcomes, not delivery dates.

[Memory Workspace](docs/product/memory-workspace.md) proposes useful memory/context reads, Console editing, import, synchronization, and portability. Its [implementation package](docs/implementation/memory-workspace/README.md) is a design, not evidence that those features are available in the binary.

The supplied snapshot includes embedding-result publication code and **recorded lead acceptance of Task 14E**. That acceptance closes Task 14E only: P04, G0, and full v1.0 remain incomplete in the recorded verdict. This editorial work did not rerun those product tests. See [status and evidence boundaries](docs/status.md).

## Documentation

The English edition separates tutorials, reference, explanations, implementation plans, and historical evidence. [English reading editions of the normative contracts](docs/specs/en/README.md) preserve the original requirement identifiers and technical vocabulary; frozen original specifications remain unchanged and authoritative if a translation differs.

**Input snapshot:** `3e05dfbdce063aa44a3a9e5a7a84c274597e8188`, identified by the supplied ZIP's archive comment. Older observations retain their source pins. See the [source register](docs/maintenance/sources.md) and [refactor report](docs/maintenance/english-validation-report.md).

This refactor changes documentation only, not runtime code, migrations, API schemas served by the product, or release gates.

## Community

Contributions and issue reports are welcome within the project's current implementation and support boundaries.

- [Contributing guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security policy](SECURITY.md)
- [Support and issue guidance](.github/SUPPORT.md)
