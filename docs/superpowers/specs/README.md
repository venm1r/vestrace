# Historical Design Archive

The documents in this directory predate the v0.2 normative architecture baseline.

They are retained because they contain valuable rationale, earlier ADR/design exploration, State Engine reconciliation work, cross-workspace sharing design, encryption/export design and implementation context.

They are **not** the authoritative v0.2 target architecture when they conflict with newer documents.

Use this precedence:

```text
docs/specs/vestrace-architecture-contract-v0.2.md
→ newer Accepted docs/adr/*
→ specialized docs/specs/vestrace-*-v0.2.md
→ docs/specs/vestrace-normative-invariants-v0.2.md
→ current implementation source/tests for implementation reality
→ documents in this historical archive
```

In particular, older plans or designs that describe a separate State Engine service/runtime, broader role-based authority, old version scope, or earlier sharing/recovery semantics must not be executed as current architecture without a new gap-analysis/planning phase.

The v0.2 normative index is [`../../specs/README.md`](../../specs/README.md).
