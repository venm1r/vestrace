# Historical MW bundle validation — English reading edition

This translates the [unchanged original report](report.md), not a fresh runtime or documentation
run. Original baseline: `6f6102536e9a535b7086db14573bf45fe750ad71`.
The original main pointer was rechecked during integration; see [the integration report](integration-report.en.md).

## Recorded checks

| Historical check | Result |
| --- | ---: |
| Markdown files in the original bundle | 27 |
| JSON files without duplicate keys | 33 |
| Draft 2020-12 schema definitions | 41 |
| Proposed HTTP operations | 20 |
| Resolved internal OpenAPI references | 81 |
| Positive schema/selected-semantic examples | 13 |
| Negative examples refused by the expected layer | 10 |
| Structural / semantic negatives | 6 / 4 |
| Requirements linked to task/spec/acceptance | 45 |
| Tasks / packages | 23 / 8 |
| Planned, unexecuted acceptance cases | 54 |
| Existing/proposed file-map entries | 96 |

The original checks covered local links/anchors, closed fences, Python syntax, and exclusion of
private token-bearing URLs. They did not establish live availability of external pages.
OpenAPI checks covered operation structure, required path parameters, unique operationId, and
internal references; **a full dedicated OpenAPI standards validator was not run**.

Schema checks validate structure. The small semantic probes also distinguish traversal,
duplicate identities, UTF-8 byte limits, and dangling provenance. They test neither authorization,
transactions, cryptography, nor an actual server.

## Original editorial self-review fixes

Epoch/receipt groundwork moved into MW-01 to avoid a read/writer dependency cycle. Initial source
binding belongs to first import rather than being invented by sync later. Conflicts keep immutable
M0 while allowing resolution against current M with preconditions. Provenance completeness is
separate from current/history selection. Complete scans bound the union of input and Missing
items. The pure Node test script is created before test:memory; the browser runner is separate.

This was editorial self-review, not independent review by another engineer/agent.

## Repeat procedure and limits

From the MW directory, the original command was:

```bash
python verification/validate_bundle.py
```

It required Python 3.10+, jsonschema, and markdown-it-py, performed no network/GitHub/product work,
and checked the bundle's manifest. The original manifest excluded itself and two result JSON
files to avoid circularity. The current entrypoint selects english-file-manifest.json for the
translated edition; original tool source and records remain historical.

Rust/Cargo/doctests, PostgreSQL migrations/roles/races/failures, HTTP/MCP, browser E2E, and actual
import/sync/export were not executed. New tables/routes/scripts/targets were proposed, not
implemented by the documentation. MW-07 must qualify the real feature separately.

## Original integration edition 0.1.1

Linked indexes and the integration contract were added without changing runtime schemas or
frozen authorities. The large UTF-8 negative example became a bounded declarative fixture,
expanded into the same payload before schema/semantic validation. Its intended rejection
remained the semantic byte limit, not invalid fixture-descriptor structure.

Original result: [validation-result.json](validation-result.json). Current editorial checks:
[English validation report](../../../maintenance/english-validation-report.md).
