# Legacy / Historical Specifications

The following files in `docs/specs/` predate the v0.2 normative architecture baseline and are retained for implementation/history context only:

- `r1-event-sourced-state-engine.md`
- `r1-3-projection-integration.md`
- `r1-4-command-http-integration.md`

They are **not normative v0.2 architecture specifications**.

When they conflict with the current architecture baseline, use this precedence:

```text
Architecture Contract v0.2
→ Accepted newer ADR
→ specialized v0.2 normative specification
→ Normative Invariants Catalog
→ current implementation source/tests for what is actually wired
→ legacy specification
```

The legacy documents may still accurately describe specific implementation decisions in the current source history. Their presence does not grant future architectural authority and they must not be used as implementation plans for v0.2 without a fresh gap-analysis/planning phase.

See [`README.md`](README.md) for the normative v0.2 specification index.
