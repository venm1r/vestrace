# Documentation Gap Delta — Q12 Startup Recovery Orchestration

**Date:** 2026-08-12
**Scope:** application startup recovery orchestration after Q11 durable recovery evidence
**Authority:** Health / Repair / Incident Contract, Qualification / Conformance Specification, and v0.2 → v1.0 roadmap.

## Implemented bounded contract

- Added `StartupRecoveryCandidate` and a typed `StartupRecoveryReport`/record boundary.
- Added `StartupRecoveryService` over the existing `RunRecoveryOperations` port.
- Candidate IDs are validated as unique before any recovery operation runs.
- Domain recovery classification is authoritative: only `SAFE_TO_RESUME` and `SAFE_TO_RETRY` rebuild the run projection.
- `MUST_RECONCILE`, `MUST_ABORT`, and `HUMAN_REQUIRED` become explicit non-mutating outcomes.
- A projection rebuild error returns immediately and does not produce a partial report.
- Added focused tests for safe resume/retry, barrier reporting, duplicate rejection, and fail-closed operation errors.

## Explicit boundary

This slice provides the application orchestration decision boundary but does not discover unfinished candidates from durable storage, wire the service into server/worker readiness, execute external-effect reconciliation, perform isolated crash/fault tests, or approve a v1.0 release.
