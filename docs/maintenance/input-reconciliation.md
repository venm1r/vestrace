# Input reconciliation

## This edition

The actual input is the uploaded vestrace-main.zip, whose ZIP comment identifies
`3e05dfbdce063aa44a3a9e5a7a84c274597e8188`. The [input manifest](english-input.json) records
its digest and extraction scope. Excluded dependencies/build directories are not documentation
changes. No separate earlier archive was treated as the current implementation baseline.

The original project source was retained for exact comparison. Active documents were translated
and refactored; source code, CI, schemas served by the runtime, lockfiles, migration SQL, PLAN.md,
Accepted ADRs, frozen normative originals, and development evidence were not changed.

## Earlier provenance retained in the repository

| Earlier material | Preserved meaning | Treatment now |
| --- | --- | --- |
| Handbook at 58e7dac3 | Memory-first explanation and API/operations topics. | Earlier observations are not promoted to current state. |
| MW design at 6f610253 | Requirements, eight plans, schemas, fixtures, and boundaries. | Use the integrated package as source; translate and preserve its requirement graph. |
| Documentation integration at 07e2977a | One package location and authority hierarchy. | New navigation still points to that location; old reports remain historical. |
| Monolithic Markdown/ZIP exports | Reading conveniences derived from files. | They do not become a second normative authority. |
| Promotion plan | Demo, onboarding, pilots, and working adoption targets. | Targets are goals, not forecasts; current platform rules require separate verification. |

The old [input-documents.json](input-documents.json) belongs to that earlier reconciliation.
Its old checksums do not describe the current translated files, and the old statement that MW
was byte-for-byte unchanged applied only to that integration. This edition changes MW prose
while preserving requirement/task/case identities, schema semantics, and exact Unicode fixtures.

Source manifests keep original reviewed commits/hashes; a separate [source delta](source-delta.json)
records comparisons against the uploaded snapshot. Recognize recorded 14E acceptance and the
occupied 0196 candidate without editing historical verdicts or applied SQL.
