# Configuration and environment

## Source of truth

Read [Cargo.toml](../../Cargo.toml), [rust-toolchain.toml](../../rust-toolchain.toml), AppConfig, [Compose](../../docker-compose.yml), the server/worker entry points, and frontend configuration in the selected checkout. This page does not duplicate every default.

Configuration separates non-secret settings from environment/mounted secret stores. The documented precedence is defaults → optional TOML → `VESTRACE_` environment with `__` nesting → typed CLI overrides. Unknown TOML fields are rejected. A secret database URL must not migrate into public configuration.

## Operational groups

| Group | What matters |
| --- | --- |
| Database | Secret URL, pooling/scope, and restricted runtime identity. |
| HTTP | Bind and proxy boundary; loopback development is not public deployment. |
| Workspaces | Which scope a worker serves; an empty scope is not useful completion. |
| Provider execution | Persistent material vault and separate read-only bootstrap root; key identity, not plaintext. |
| Policy | Capabilities and explicit disclosure/destination constraints. |
| Observability | Structured logs without content or credentials. |
| Qualification/recovery | Exact prerequisites, not an administrative switch to trusted. |

Participating processes must use compatible root volumes and allowed destinations. A shared governed provider graph reduces drift but does not qualify deployment configuration by itself.

## Environment evidence

The supplied checkout pins Rust 1.85.0, uses edition 2024, and declares Node 22 and PostgreSQL 17/pgvector in its build/deployment files. This refactor does not newly qualify Windows, Linux, a filesystem, or a deployment topology.

For release, record OS, kernel/filesystem, architecture, image digests, toolchain, roles, and model/provider settings. Changes to vault filesystem semantics, Node/TypeScript major versions, or PostgreSQL/extensions require separate verification. Lockfile upgrades are outside this documentation change.

**Sources:** repository configuration files and [source register](../maintenance/sources.md).
