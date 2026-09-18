# Vestrace English documentation refactor — validation report

**Edition:** September 8, 2026.  
**Source snapshot:** `3e05dfbdce063aa44a3a9e5a7a84c274597e8188`.  
**Input:** user-uploaded `vestrace-main.zip`; the ZIP comment identifies this source commit.  
**Input SHA-256:** `f79bf527191e05e2cf0a49b6ac30645a187d9de8d091f0cfe6774a3b6120310b`.

## Result

**PASS for the declared documentation checks.** The English edition is delivered as a cleaned
repository archive and an applicable Git patch. It changes documentation only. It is not a
product build, deployment, release qualification, or independent human architecture verdict.

The patch modifies **92 existing files** and adds **37 files**
(**129 documentation files total**), with no removals. The current
English authored/translated reading surface contains **102 Markdown pages**.
The cleaned repository contains **1057 regular files**;
node_modules, target, and Git metadata are excluded from the output. Existing script permissions
are retained from the uploaded ZIP when packaging original files.

## What changed

The root README and documentation index now provide task-oriented paths for users, integrators,
developers, operators, and Memory Workspace implementers. Product explanations, API reference,
setup, Console guidance, operating procedures, architecture explanations, and development/release
rules use consistent English terminology and explicit source/evidence boundaries.

The roadmap keeps all **34 initiatives and 92 acceptance criteria**, their IDs, priority groups,
dependencies, package mappings, and release relationships. Cards are generated from the JSON
register rather than maintained independently.

Memory Workspace keeps **45 requirements, 23 tasks, 54 acceptance cases, eight packages, and
96 original file-map entries**. Repeated execution rules are centralized. Generated plans retain
the original task file lists and test commands, with the already documented test-runner and
manual-override scope clarifications made explicit. The acceptance catalog is generated from
traceability, not a competing requirements list.

There are **11 complete English normative reading editions** under `docs/specs/en/`. Frozen
originals remain unchanged and authoritative if translation wording differs. English companions
also make the three preserved historical validation reports readable without rewriting their
old observations. Exact Unicode fixture input bytes remain unchanged deliberately.

Previously missing database-schema and security guide entry points were restored. This does
not claim that every target schema or security mechanism is implemented.

## Corrections backed by the uploaded snapshot

The current P04 description now recognizes **recorded final lead approval of Task 14E**.
The same record retains the rotation-before-adoption deferral and explicitly leaves **P04,
G0, and v1.0 incomplete**. This editorial task read that record; it did not rerun its product tests.

The archive contains `migrations/0196_retired_credential_erasure.sql`. MW-M01's old candidate
`0196_memory_workspace_receipts.sql` therefore collides. The English plans flag coordinated
reallocation during MW-00; they do not reserve another number or change applied migration SQL.

The roadmap's keep_manual wording is clarified against the existing detailed sync contract:
it may preserve the same memory revision while recording an audit/binding resolution receipt.
A useless empty text revision is not required. No new sync behavior is introduced by that clarification.

Original source IDs, commits, blob hashes, and inspected ranges are retained. A separate source
delta identifies matching and changed files. In particular, the reviewed S03–S17 implementation
blobs match the supplied archive, while P04 evidence has advanced. A hash match is not runtime evidence.

## Executed checks

| Check | Observed result |
| --- | --- |
| Main documentation validator | PASS, exit 0. |
| English authored/translated Markdown | 102 pages checked for unintended Cyrillic prose and credential-like additions. |
| Markdown scan | 165 files and 838 local-link occurrences inspected; **zero new issues**. |
| JSON documents | 61 parsed with duplicate-key/nonfinite-value rejection. |
| JSON code fences | 8 inspected; one inherited invalid example is reported separately below. |
| Bash snippets | 39 parsed with bash -n; none executed. |
| Generated documentation | 15 views match their editable JSON sources. |
| Normative translation parity | All 11 editions preserve normative-keyword, requirement-ID, and inline-identifier multisets; technical non-prose fenced blocks also match. |
| Contract graph parity | Eight structural groups match the original snapshot, including requirements/tasks/cases, file map, source pins, corpus relations, and roadmap dependencies. |
| Roadmap | 34 features, 92 criteria, groups 8/9/7/6/4, acyclic graph; all verification remains NOT_RUN_HERE. |
| Synthetic evaluation | Eight sources and 12 cases; IDs/scopes/times/relationships preserved; all results remain NOT_RUN. |
| MW schemas | 41 definitions and 20 proposed operations; 81 internal references resolved. |
| MW fixtures | 13 valid examples pass; 10 invalid examples reject at the intended layer (6 structural, 4 semantic). |
| Original file preservation | 927 protected original files match SHA-256, including product code/configuration, frozen contracts, evidence, runtime schemas, and byte-sensitive fixtures. |
| Current payload manifest | 232 file digests checked; self/result exclusions are explicit to prevent circular hashes. |
| Documentation-tool regression suite | **31 tests passed**, exit 0. These test documentation tooling, not Vestrace. |
| Whole-validator negative probes | Five deliberate regressions rejected; exact original bytes restored; final validator PASS. |
| Existing foundation-doc-truth.sh | Passed, exit 0; documentation string checks only. |
| Git diff whitespace | git diff --check passed using the repository Markdown whitespace attributes. |
| Patch applicability | git apply --check and actual git apply passed in an isolated original-snapshot copy. |
| Patch round-trip | All 1057 delivered files matched the edited tree byte-for-byte after application. |

The five negative probes changed a current link, assigned an unearned feature PASS, weakened a
normative keyword, modified runtime source, and changed the multi-byte fixture unit to ASCII.
Each failed for its intended substantive check, not only a stale payload hash. Every mutated
file was restored exactly. The Unicode fixture still expands to 40,000 characters / 80,000 UTF-8
bytes, preserving its semantic byte-limit failure.

## Inherited historical defects — not silently marked fixed

The supplied archive already contained **110 distinct missing local targets** and **one invalid
JSON code fence**. All 111 diagnostics remain unchanged in preserved historical/frozen documents:
104 in documentation-status-v0.2, five in requirement-coverage-v0.2, and one each in the historical
foundation plan and frozen protocol-lock plan. Many targets are older gap-delta documents absent
from the upload. This work does not invent their content or rewrite historical evidence.

The validator explicitly separates those inherited diagnostics from new defects. **PASS means
no new documentation defect within the declared scope; it does not mean every historical link
in the entire repository resolves.** Exact paths are recorded in historical-link-baseline.json
and english-validation-result.json. Updated English pages have no unresolved local links detected.

## Verification environment and limitations

Observed environment: Python 3.13.5, markdown-it-py
4.2.0, jsonschema 4.26.0,
Linux with Bash and Git. The tools use explicit UTF-8 and prune build/dependency directories.
Windows execution was not tested here; Bash is needed for shell-snippet parsing.

Not run: Rust build/tests/doctests, PostgreSQL migrations/grants/races, HTTP/MCP/worker execution,
browser or official-client interoperability, real provider/model calls, actual import/sync/export,
or backup/restore. A dedicated full OpenAPI standards validator and live availability checks
for external URLs were not run. Structural parity checks cannot prove every semantic nuance of
a translation; independent human translation/architecture review was not performed.

The English synthetic corpus is a new set of text bytes. Its digest changes; any future
comparison must pin the actual corpus rather than reuse results from the Russian text.

## Files and reproducibility

The archive contains the full cleaned repository, not a replacement of docs alone. Start at
README.md and docs/README.md. The patch targets the supplied snapshot; check a newer or dirty
checkout before applying it. It creates no remote commit, push, or pull request.

Run from the repository root:

```bash
python docs/maintenance/render_catalogs.py --check
python docs/maintenance/validate_documentation.py .
python -m unittest discover -s docs/maintenance/tests -p 'test_*.py'
python docs/implementation/memory-workspace/verification/validate_bundle.py
bash scripts/foundation-doc-truth.sh
```

After an authorized documentation edit, regenerate views and current manifests, then rerun
checks. Never rewrite old verification JSON merely to make it say PASS. Historical manifests
remain historical; this edition uses english-payload-manifest.json and MW english-file-manifest.json.

The machine-readable result, input/source/scope inventories, and preservation manifests are
under docs/maintenance. No product qualification is inferred from this editorial report.
