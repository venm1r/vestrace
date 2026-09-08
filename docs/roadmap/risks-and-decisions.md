# Risks, trade-offs, and decisions

**Status:** Proposed scope and risk policy. Editorial changes are distinguished from product decisions.

| Risk | Early signal | Control |
| --- | --- | --- |
| Unbounded scope | Capabilities added without a user task. | One milestone, explicit owner decision, non-goals. |
| Specifications without a complete product path | Fixtures create unreachable production states. | Real binary/role composition tests. |
| Partial atomicity | Memory commits without receipt/outbox. | Shared UoW and fault/replay tests. |
| New disclosure bypass | GET is protected, debug/export reveals the same content. | Shared disclosure gate and negative matrix. |
| Import destroys editorial work | mtime-wins or unconditional overwrite. | B/I/M conflicts and preconditions. |
| Sources become presumed truth | Extraction auto-assigns Active/Trusted. | Provenance and separate mutation authority. |
| Consolidation hides corrections | Summaries remain current after source changes. | Exact input revisions, freshness, invalidation. |
| Development becomes too expensive | Disk exhaustion and long local cycles. | Measure profiles/targets without removing risk classes. |
| Onboarding loses users | No useful result without author intervention. | One client, reference scenario, observed external setup. |
| Documentation becomes competing truth | Endpoint copies disagree. | Canonical registers, generated views, source/status labels. |

## Editorial decisions in this edition

Use the supplied archive as the exact editing baseline; retain original source-observation
pins. Translate and refactor active guides. Provide complete English reading editions of
frozen specifications without changing their original bytes. Preserve historical evidence
and record fresh documentation results separately. Do not change code, migration SQL, CI,
lockfiles, or root PLAN.md.

Correct the current P04 description to recognize recorded Task 14E approval while preserving
its deferral and the incomplete P04/G0/v1.0 boundaries. Flag the occupied 0196 candidate
without assigning a new migration number.

## Proposed product decisions

Memory-first remains central, with API/Console/import/portability as P1. Keep MW an independent
milestone until release placement is accepted. The first importer is bounded and needs no
LLM; source and editorial revisions remain separate. Qualify one external client before
expanding SDKs. Summaries are derived state, not automatic canonical truth. P3 obligations
remain required for full v1.0; hosting, teams, and a standalone harness are P4 proposals.

These choices do not become Accepted ADRs automatically. Any change to an accepted law
needs an explicit amendment, consequences, tests, and migration analysis.

Do not implicitly add a custom model/vector database, IDE/coding agent, no-code builder,
a large unsupported adapter catalog, or generic importer runtime.

[Roadmap](README.md) · [Program mapping](program-mapping.md)
