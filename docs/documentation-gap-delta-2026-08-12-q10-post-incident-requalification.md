# Documentation Gap Delta — Q10 Post-Incident Requalification

**Date:** 2026-08-12
**Scope:** post-incident qualification evidence and deterministic recovery action gate after Q9
**Authority:** Health / Repair / Incident Contract, Qualification / Conformance Specification, and v0.2 → v1.0 roadmap.

## Implemented bounded contract

- `PostIncidentQualificationEvidence` binds an incident id, revalidation run id, revalidation result, and non-empty evidence references.
- `QualificationBundle` accepts that evidence only for `POST_INCIDENT` lifecycle.
- A post-incident bundle without evidence is `INCOMPLETE`; failed or inconclusive revalidation cannot produce a passed bundle.
- `conformance bundle --lifecycle post-incident` requires `--post-incident-evidence-file` and persists the failed artifact before returning non-zero when revalidation is unsuccessful.
- The deterministic recovery gate requires exactly one evidence-bearing observation for each recovery target and checks the classified action (`RESUME`, `RETRY`, `RECONCILE`, `ABORT`, or `HUMAN_REVIEW`).

Focused coverage is in `tests/q10_post_incident_requalification.rs` and `crates/vestrace-cli/tests/q10_post_incident_cli.rs`.

## Explicit boundary

The evidence file is a typed local qualification input; it is not a durable Incident/RevalidationRun repository, does not execute recovery orchestration, and does not prove production crash injection. Durable incident persistence, runtime recovery execution, isolated fault-suite orchestration, and release approval remain open.
