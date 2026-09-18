# CLI reference

**Scope:** Command groups found in source, not commands executed in this refactor. [Clap definitions](../../crates/vestrace-cli/src/main.rs) and the selected binary's `--help` define exact syntax.

| Command | Purpose and boundary |
| --- | --- |
| `server` | HTTP process; database/schema/vault/policy prerequisites apply. |
| `worker` | Persistent processing of available work. |
| `worker --once` | One cycle: 0 for work, 3 for idle, 1 for an error; not a full Run result. |
| `mcp` | MCP process mode. |
| `migrate` | Apply migrations after the required role provisioning. |
| `doctor` | Diagnose the current implementation. |
| `plan --finding-id UUID` | Plan for named findings, not a general roadmap editor. |
| `repair --plan-id UUID --current-state-ref REF` | Apply a particular plan against expected state. |
| `rebuild TARGET` | Target is defined by the source enum/help, not invented in prose. |
| `schema FORMAT` | Emit a supported schema format. |
| `conformance …` | Qualification artifacts/checks and evidence-bound release operations. |

Global `--config` and `--http-bind` do not replace provisioning. Keep database credentials in the process's secret environment, not public TOML or the command line.

Conformance includes list/check, bundle/manifest, verify/sign/verify-signature, publish-baseline/release, and explicitly destructive qualification modes. A `conformance check` is not automatically full production acceptance. Read its target digests, profiles, artifact identities, and trust-source requirements.

Do not use invented commands such as `vestrace qualify` or `vestrace doctor --plan` from old conceptual examples. Proposed MW folder-import/export commands become usable only when implemented.
