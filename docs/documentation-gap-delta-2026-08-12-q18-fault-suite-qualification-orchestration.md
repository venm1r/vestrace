# Documentation Gap Delta — Q18 Fault-Suite Qualification Orchestration

**Date:** 2026-08-12
**Scope:** application composition of deterministic fault execution and durable evidence
**Status:** implemented as a bounded additive slice; not a v1.0 qualification claim

## Implemented

- `ExternalEffectFaultQualificationService` executes the existing required
  fault-point suite through an injected executor.
- The service snapshots the exact target digest and persists the resulting
  `ExternalEffectFaultSuiteEvidence` through `FaultSuiteEvidenceRepository`.
- A failed or unsafe decision is persisted and returned as failed evidence;
  it is not converted into an application error or a passing qualification.
- Executor failure occurs before evidence construction/persistence and remains
  fail-closed.

## Evidence

- Focused orchestration tests: 3/3 passed.
- Q17 snapshot tests: 3/3 passed.
- Docker PostgreSQL repository tests: 7/7 passed.

## Explicit non-claims

This is an application composition boundary. It does not select or inject a
production provider fault mechanism, run destructive scenarios automatically,
attach evidence to a complete `QualificationBundle`, restore trust, or approve
a release profile.
