# Maintaining the English documentation

**Edition:** 2026-09-08. **Input:** user-supplied archive at
`3e05dfbdce063aa44a3a9e5a7a84c274597e8188`. This is a documentation-only refactor.

## Document ownership

Active guides explain tasks and current source boundaries. Frozen originals, Accepted ADRs,
protected plans, and recorded evidence keep their existing authority and bytes. Complete English
[reading editions](../specs/en/README.md) accompany the 11 normative documents; they do not
replace the originals. Historical verification reports have `.en.md` reading companions.

Preserve Proposed/Accepted distinctions, technical identifiers, original source pins, and exact
requirements. A route declaration is not a runtime result. Document tests do not close P04,
G0, MW feature qualification, P12, or TRUSTED.

## Editable sources and generated views

| Editable source | Derived view |
| --- | --- |
| docs/roadmap/feature-register.json | Five P0–P4 roadmap pages. |
| docs/reference/route-catalog.json | Selected route reference. |
| MW traceability.json | Acceptance catalog, requirement/task/case mapping. |
| MW traceability.json + task-details.json + file-plan.json | Eight package plans. |

Edit these JSON sources, then regenerate from the repository root:

```bash
python docs/maintenance/render_catalogs.py
python docs/maintenance/render_catalogs.py --check
python docs/maintenance/update_manifests.py .
```

The generated plans share execution rules in MW plans/README.md. Do not maintain independent
copies of requirements or repeated task procedures.

## Validation

Use Python 3.10+, markdown-it-py, jsonschema, and Bash for shell-snippet syntax checks.
No product credentials, network, database, or model are needed.

```bash
python docs/maintenance/validate_documentation.py .
python -m unittest discover -s docs/maintenance/tests -p 'test_*.py'
python docs/implementation/memory-workspace/verification/validate_bundle.py
```

The main validator checks active links/anchors, closed fences, JSON, generated views, normative
translation parity, traceability, fixture validation, and current manifest hashes. It also
checks original preserved code/contract/evidence bytes against the supplied-archive inventory.
Historical pre-existing broken references are reported separately, not silently counted as
successful links. See the exact executed scope in [the current report](english-validation-report.md).

Bash snippets are parsed with bash -n, not executed. Unicode fixture payloads stay byte-for-byte
unchanged; in particular, the multi-byte 40,000-character negative case must not be translated
into ASCII and accidentally become valid. Frozen Russian originals and historical reports
are intentional language-preservation exceptions, with English reading copies linked nearby.

## Current versus historical verification

Original payload-manifest.json, validation-result.json, scope-result.json, and MW verification
records describe the older documentation deliveries. They are preserved as historical records,
not rewritten to claim a fresh PASS. This edition uses english-payload-manifest.json and
english-validation-result.json, plus MW english-file-manifest.json. Current validator entrypoints
select the current manifest explicitly; original validator source is retained under
[history](../history/editorial-2026-09-07/README.md).

Before publishing edits, regenerate views and manifests, run the validators and negative
regression tests, inspect the exact diff, and record checks not run. External URL availability
and semantic translation review are distinct from local structural validation.

[Source register](sources.md) · [Migration map](migration-map.md) · [Input reconciliation](input-reconciliation.md)
