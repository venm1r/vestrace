# Historical documentation validation report — English reading edition

**Original report date:** 2026-09-07. **Original baseline:**
`07e2977a20b05c5b16953a206a6d68bdbff3a052`.
This translates the [unchanged original](validation-report.md). It does not report this
English refactor's checks; those are in [the current report](english-validation-report.md).

## Recorded result

The original result was PASS for the listed static documentation checks, not product qualification,
a Vestrace execution, or independent architecture review.

| Historical check | Recorded observation |
| --- | --- |
| Patch source documents | Ten Git blob SHA-1 values matched the baseline. |
| Scope | Ten modified and 62 added files; only README/docs. |
| MW preservation | All 61 files preserved; tree 1f69acad8676991b2f435501ef3fd8aec742c3b4. |
| Markdown | 88 files parsed; internal links and anchors checked. |
| Roadmap | 34 initiatives, five groups, 92 criteria, unique IDs, acyclic dependencies. |
| Corpus | Eight synthetic sources, 12 scenarios, all NOT_RUN. |
| Bash examples | 35 blocks parsed with bash -n only, not executed. |
| MW schema/examples | 41 definitions, 20 proposed operations, 13 positive and ten negative examples. |
| MW traceability | 45 requirements, 23 tasks, 54 planned cases, eight packages; links checked. |
| JSON/Python | Duplicate-key rejection and execution of documentation validators. |
| Patch | git apply --check and application in an isolated checkout with exact source bytes; output compared. |

Criteria and scenarios in a register are not executed tests. These results concern documents,
format examples, and patch applicability. Proposed-directory name checks used the named GitHub
tree, not an assumed empty checkout.

## Original limitations

The original work did not clone the complete repository locally; it reconstructed ten changed
documents and the preserved MW subtree. Newer commits/dirty files require separate compatibility
checks. The patch did not remove old documents, alter source/CI/migrations, or grant new scope.

Rust builds/tests/doctests, PostgreSQL migrations/roles/races, server/worker/HTTP/MCP execution,
browser/official-client interoperability, actual import/export/backup/restore, full OpenAPI-standard
validation, live availability of every external URL, and independent design review were not run.

Normative/historical originals remained in the source repository through pinned links. That
older ZIP was a guide/roadmap overlay plus unchanged MW, **not a full repository backup**.

## Historical repeat command and records

```bash
python docs/maintenance/validate_documentation.py .
```

The original tool required Python 3.10+, markdown-it-py, jsonschema, and Bash; it read local files
without network or output-file writes. The same entrypoint now validates the new English edition;
original tool source is preserved under history/editorial-2026-09-07.

[validation-result.json](validation-result.json), [scope-result.json](scope-result.json), and
[payload-manifest.json](payload-manifest.json) are original records. The old manifest excluded
itself and its result JSON to avoid circular hashes. Later editions need fresh evidence rather
than changing an old result to PASS without execution.
