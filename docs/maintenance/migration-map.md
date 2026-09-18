# Documentation refactor map

**Scope:** README and documentation only, against uploaded archive `3e05dfbd`.
The accompanying Git patch applies to that exact source snapshot; inspect any newer or dirty
checkout before applying it. Do not replace a live repository blindly with an archive.

| Area | Change |
| --- | --- |
| Root README and docs index | English task-oriented entry points and explicit source/evidence boundaries. |
| Product, guides, reference, operations, development | English prose, unified terminology, reduced repetition, source-backed limitations. |
| Status | Recognize recorded Task 14E approval, preserve narrower acceptance, flag occupied MW migration candidate. |
| Roadmap | Translate 34 initiatives/92 criteria; generate cards from the unchanged dependency graph. |
| Memory Workspace | Translate/refactor chapters and all plans; preserve 45 requirements, 23 tasks, 54 cases, 96 file-map entries, and proposed API/schema semantics. |
| Normative specifications | Add 11 complete English reading editions; frozen originals stay untouched. |
| Historical verification | Preserve original records and provide English reading companions. |
| Database/security entry points | Restore missing guide files; distinguish implementation from target guarantees. |
| Maintenance | Deterministic generators, current manifests, link/structure/parity checks, and regression tests. |

The [scope inventory](english-scope.json) lists every added/changed file and the supplied-source
hashes. [Preservation inventory](preservation-manifest.json) identifies original files which
must remain byte-exact. [Validation report](english-validation-report.md) states actual checks,
limitations, and pre-existing historical reference problems.

No source/CI/migration/lockfile/PLAN change, remote commit, push, or pull request is implied by
this local delivery. The patch is the reviewable integration artifact.
