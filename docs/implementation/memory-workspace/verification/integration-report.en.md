# Historical integration validation — English reading edition

This translates the [unchanged original](integration-report.md). Original baseline:
`6f6102536e9a535b7086db14573bf45fe750ad71`. It is not runtime qualification or this edition's result.

## Original change

Four existing files changed: root README, architecture, normative index, and planning index.
The general documentation index, implementation index, full MW package, and integration contract
were added. The package ZIP README did not overwrite the project README blindly.

Frozen specifications, Accepted ADRs, P01–P12, P04 preflight/scope, historical evidence, PLAN.md,
product code, runtime schemas, migrations, and CI were outside scope. Four original Git blob
SHA-1 values were verified before editing reconstructed GitHub source documents.

## Original preservation

The work preserved 45 requirements, 23 tasks, 54 future cases, 41 schema definitions, and
20 proposed operations. It did not redesign domain chapters/plans, only navigation/handoff
status and the old/new documentation mapping.

The UTF-8 negative example became a bounded declarative fixture rather than 40,000 repeated
characters. The validator expanded it to identical payload before schema/semantic checking;
payload equivalence was checked. No fixture scripts were executed.

## Checks and limits

The bundle validator checked JSON/schema/references/examples, Markdown links/anchors, Python
syntax, traceability, and manifest hashes. An additional local check matched new navigation
to fetched GitHub/local targets and compared original requirement maps with the delivery.

[validation-result.json](validation-result.json), [integration-result.json](integration-result.json),
and [integration-manifest.json](integration-manifest.json) contain historical details.
Hashes are not signatures or statements that product code is trusted.

This was editorial self-review, not independent architecture review. Old historical links
were not all reaudited. Public external pages were not checked for freshness; Rust/PostgreSQL/
browser/product tests were not executed.

## Original repeat command

```bash
python docs/implementation/memory-workspace/verification/validate_bundle.py
```

Check commit/tree and patch applicability against the actual checkout. Proposed implementation
commands are not executed tests of the documentation delivery. Current checks are reported
[separately](../../../maintenance/english-validation-report.md).
